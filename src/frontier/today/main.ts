// `jocasta plugin today`: the `today` pane (0551, 0555), an OpenTUI program the `jocasta` herdr plugin opens beside the
// boss. It draws the service's feed (`snapshot`, then `today`, `hud`, `dashboard`), redraws each minute for the clock,
// and takes his keys: up/down pick a thread, Enter goes to its pane, f and s Finish and Stop it (0544, 0554).
import {
	bg,
	bold,
	CliRenderEvents,
	createCliRenderer,
	fg,
	StyledText,
	type TextChunk,
	TextRenderable,
} from "@opentui/core";
import { TOKYO_NIGHT } from "../canvas/palette.ts";
import type { ServerMessage } from "../protocol.ts";
import { postAction, stateFeed } from "./api.ts";
import { type Line, render, type TodayState, threadPanes } from "./view.ts";

/** The view's lines as OpenTUI styled text. */
export function styled(lines: Line[]): StyledText {
	const chunks: TextChunk[] = [];
	lines.forEach((line, index) => {
		if (index > 0) chunks.push(fg(TOKYO_NIGHT.fg)("\n"));
		for (const segment of line) {
			let chunk = fg(segment.fg ?? TOKYO_NIGHT.fg)(segment.text);
			if (segment.bold) chunk = bold(chunk);
			if (segment.bg) chunk = bg(segment.bg)(chunk);
			chunks.push(chunk);
		}
	});
	return new StyledText(chunks);
}

/** Runs the pane until herdr closes it; resolves its exit code. */
export async function runToday(): Promise<number> {
	const renderer = await createCliRenderer({ exitOnCtrlC: false, useMouse: false, backgroundColor: TOKYO_NIGHT.bg0 });
	const text = new TextRenderable(renderer, { content: "" });
	renderer.root.add(text);
	const state: TodayState = { up: false };
	const draw = () => {
		const panes = threadPanes(state.today);
		if (state.selected !== undefined && !panes.includes(state.selected)) state.selected = undefined;
		state.selected ??= panes[0];
		text.content = styled(render(state, new Date(), renderer.terminalWidth, renderer.terminalHeight));
		renderer.requestRender();
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
					state.said = { text: message.text, isError: message.isError };
					break;
				default:
					return;
			}
			draw();
		},
		(up) => {
			state.up = up;
			draw();
		},
	);
	const act = async (action: "finish" | "stop") => {
		if (!state.selected) {
			state.said = { text: "No thread to act on.", isError: true };
		} else {
			const refused = await postAction(action, state.selected);
			if (refused) state.said = { text: refused, isError: true };
		}
		draw();
	};
	renderer.keyInput.on("keypress", (key) => {
		const panes = threadPanes(state.today);
		const at = state.selected === undefined ? -1 : panes.indexOf(state.selected);
		if (key.name === "up" || key.name === "k") state.selected = panes[Math.max(0, at - 1)] ?? state.selected;
		else if (key.name === "down" || key.name === "j") state.selected = panes[Math.min(panes.length - 1, at + 1)];
		else if ((key.name === "return" || key.name === "enter") && state.selected) {
			feed.send({ type: "pane.focus", paneId: state.selected });
		} else if (key.name === "f" && !key.ctrl) void act("finish");
		else if (key.name === "s" && !key.ctrl) void act("stop");
		else return;
		draw();
	});
	renderer.on(CliRenderEvents.RESIZE, draw);
	const clock = setInterval(draw, 30_000);
	draw();
	const { promise, resolve } = Promise.withResolvers<number>();
	const end = () => {
		clearInterval(clock);
		feed.close();
		renderer.destroy();
		resolve(0);
	};
	process.on("SIGTERM", end);
	process.on("SIGHUP", end);
	return promise;
}
