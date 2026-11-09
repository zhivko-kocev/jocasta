// `jocasta plugin today`: the `today` pane (0551, 0555, 0581), an OpenTUI program the `jocasta` herdr plugin opens
// beside the boss. It draws the service's feed (`snapshot`, then `today`, `hud`, `dashboard`), redraws every 30 s for
// the clock, and takes his keys: up/down pick a row, Enter goes to a thread's pane or opens a link row, f, s, r and n
// Finish, Stop, Review and start a New thread (0544, 0554, 0581).

import { postAction, stateFeed } from "../client.ts";
import type { PluginAction, ServerMessage } from "../protocol.ts";
import { runPane } from "../tui.ts";
import { type Frame, type Pick, render, SAID_MS, type TodayState } from "./view.ts";

/** Runs the pane until herdr closes it; resolves its exit code. */
export async function runToday(): Promise<number> {
	return runPane((pane) => {
		const state: TodayState = { up: false };
		let picks: Pick[] = [];
		let sayTimer: Timer | undefined;
		const say = (text: string, isError: boolean) => {
			state.said = { text, isError, at: Date.now() };
			// Drawn away once it is a minute old.
			clearTimeout(sayTimer);
			sayTimer = setTimeout(() => pane.draw(), SAID_MS);
		};
		const frame = (width: number, height: number): Frame => {
			let drawn = render(state, new Date(), width, height);
			if (!drawn.picks.some((p) => p.key === state.selected)) {
				// The picked row went (or none was picked yet): the first thread, else the first row Enter acts on.
				const first = drawn.picks.find((p) => p.paneId) ?? drawn.picks[0];
				if (first?.key !== state.selected) {
					state.selected = first?.key;
					drawn = render(state, new Date(), width, height);
				}
			}
			picks = drawn.picks;
			return drawn;
		};
		const feed = stateFeed(
			(message: ServerMessage) => {
				switch (message.type) {
					case "snapshot":
						state.hud = message.hud;
						state.today = message.today;
						state.dashboard = message.dashboard;
						break;
					case "today":
						state.today = message.today;
						break;
					case "hud":
						state.hud = message.hud;
						break;
					case "dashboard":
						state.dashboard = message.dashboard;
						break;
					case "action":
						say(message.text, message.isError);
						break;
					case "error":
						say(message.message, true);
						break;
					// Not today's to draw: notifications (herdr raises them) and the service's goodbye (the feed going down
					// says it).
					case "toast":
					case "bye":
						return;
					default:
						message satisfies never;
						return;
				}
				pane.draw();
			},
			(up) => {
				if (up) {
					state.seen = true;
				} else if (state.up) {
					// It went down now (before the first connect, the pane says it is connecting instead).
					state.downSince = Date.now();
				}
				state.up = up;
				pane.draw();
			},
		);
		const picked = () => picks.find((p) => p.key === state.selected);
		const act = async (action: PluginAction) => {
			const paneId = picked()?.paneId;
			// New thread needs no thread: the picked one's project goes first in the form when there is one (C10).
			if (!paneId && action !== "new-thread") say("No thread to act on.", true);
			else {
				const refused = await postAction(action, paneId);
				if (refused) say(refused, true);
			}
			pane.draw();
		};
		const clock = setInterval(() => pane.draw(), 30_000);
		return {
			lines: (width, height) => frame(width, height).lines,
			onKey(key) {
				const at = picks.findIndex((p) => p.key === state.selected);
				const keys: Record<string, PluginAction> = { f: "finish", s: "stop", r: "review", n: "new-thread" };
				if (key.name === "up" || key.name === "k") state.selected = picks[Math.max(0, at - 1)]?.key ?? state.selected;
				else if (key.name === "down" || key.name === "j") {
					state.selected = picks[Math.min(picks.length - 1, at + 1)]?.key ?? state.selected;
				} else if (key.name === "return" || key.name === "enter") {
					const pick = picked();
					if (!pick) return;
					const sent = feed.send(
						pick.paneId ? { type: "pane.focus", paneId: pick.paneId } : { type: "link.open", url: pick.url ?? "" },
					);
					if (!sent) say("Not connected; try again.", true);
				} else if (!key.ctrl && !key.meta && keys[key.name]) void act(keys[key.name] as PluginAction);
				else return;
				pane.draw();
			},
			cleanup() {
				clearInterval(clock);
				clearTimeout(sayTimer);
				feed.close();
			},
		};
	});
}
