/**
 * The `jocasta` herdr plugin's service side (0553, 0554, 0558): the `today` pane kept open beside the boss, boss 65 /
 * `today` 35, opened again when it is gone (0518, 0549); the Agents view (blocked and done first, then working, then
 * idle; the boss's workspace left out); his questions shown one at a time in the plugin's `ask` popup, with a
 * notification and herdr's request sound unless his own key opened them (0581); and his thread keys (Finish, Stop,
 * Review, New thread, Quit).
 */
import { UserError } from "../../shared/errors.ts";
import { log } from "../../shared/log.ts";
import { type PluginAction, POPUP_ENV, type PopupView } from "../protocol.ts";
import { BOSS_WORKSPACE, bossHome, isTodayPane } from "./boss.ts";
import type { Herdr, HerdrSnapshot } from "./herdr.ts";
import { finishQuestion, MERGE, newThreadQuestion, type Popup, PR, START } from "./popups.ts";
import type { JocastaService } from "./service.ts";

/** The plugin's id in `config/herdr-plugin/herdr-plugin.toml`. */
export const PLUGIN_ID = "jocasta";
/** The boss's share of its tab beside `today` (0553): boss 65 / `today` 35. */
const BOSS_SHARE = 0.65;
/** The first wait before `today` opens again after it went soon after opening; doubled each time, up to 5 minutes. */
const TODAY_AGAIN_MS = 5_000;
const TODAY_AGAIN_MAX_MS = 5 * 60_000;
/** A `today` that lived this long went for a reason of its own: it opens again at once. */
const TODAY_LIVED_MS = 30_000;
/** One warning per subject this often, so a herdr without the plugin doesn't fill the log. */
const WARN_EVERY_MS = 10 * 60_000;

/**
 * The Agents view (0553, 0554): blocked and done threads first, then working, then idle, as herdr's status sort puts
 * them; the boss's workspace (the boss and `today`) left out.
 */
export function agentView(bossWorkspaceId: string): { source: string; label: string; filter: object; sort: object[] } {
	return {
		source: `plugin:${PLUGIN_ID}`,
		label: "threads",
		filter: { op: "not", filter: { op: "eq", field: "workspace_id", value: bossWorkspaceId } },
		sort: [
			{ field: "attention", order: "desc" },
			{ field: "status", order: "desc" },
			{ field: "state_change_seq", order: "desc" },
		],
	};
}

/**
 * Applies the Agents view for the boss's workspace as herdr has it now (the plugin's startup hook, 0553); false, and
 * nothing set, before the boss's workspace exists: the service sets it once it made it.
 */
export async function applyAgentView(herdr: Herdr): Promise<boolean> {
	const boss = (await herdr.workspaces()).find((w) => w.label === BOSS_WORKSPACE);
	if (!boss) return false;
	await herdr.setAgentView(agentView(boss.id));
	return true;
}

/** Logs each subject's warning at most once per `WARN_EVERY_MS`. */
function warner(now: () => number): (subject: string, error: unknown) => void {
	const said = new Map<string, number>();
	return (subject, error) => {
		const last = said.get(subject);
		if (last !== undefined && now() - last < WARN_EVERY_MS) return;
		said.set(subject, now());
		log("warn", "plugin", `${subject}: ${error instanceof Error ? error.message : String(error)}`);
	};
}

/** `today` beside the boss and the Agents view, healed on every refresh (0518, 0549, 0553). */
export class PluginPanes {
	private todayId: string | undefined;
	private openedAt = 0;
	private nextTry = 0;
	private again = TODAY_AGAIN_MS;
	private viewFor: string | undefined;
	private readonly warn: (subject: string, error: unknown) => void;

	constructor(
		private readonly herdr: Herdr,
		private readonly now: () => number = Date.now,
	) {
		this.warn = warner(now);
	}

	/** The `today` pane's id while it is open, else undefined. */
	get today(): string | undefined {
		return this.todayId;
	}

	async heal(snapshot: HerdrSnapshot, bossPaneId: string | undefined): Promise<void> {
		const boss = bossPaneId ? snapshot.panes.find((p) => p.paneId === bossPaneId) : undefined;
		if (!boss) return;
		if (boss.workspaceId !== this.viewFor) {
			try {
				await this.herdr.setAgentView(agentView(boss.workspaceId));
				this.viewFor = boss.workspaceId;
			} catch (error) {
				this.warn("the Agents view wasn't set", error);
			}
		}
		const found =
			snapshot.panes.find((p) => p.paneId === this.todayId) ??
			snapshot.panes.find((p) => p.workspaceId === boss.workspaceId && isTodayPane(p));
		const now = this.now();
		if (found) {
			this.todayId = found.paneId;
			return;
		}
		if (this.todayId !== undefined) {
			// Gone: at once after a long life; soon after opening, after a wait that doubles each time.
			const short = now - this.openedAt < TODAY_LIVED_MS;
			this.again = short ? Math.min(this.again * 2, TODAY_AGAIN_MAX_MS) : TODAY_AGAIN_MS;
			this.nextTry = short ? now + this.again : now;
			this.todayId = undefined;
			log("info", "plugin", "today is gone; it opens again beside the boss");
		}
		if (now < this.nextTry) return;
		try {
			this.todayId = await this.herdr.openPluginPane({
				pluginId: PLUGIN_ID,
				entrypoint: "today",
				placement: "split",
				direction: "right",
				targetPaneId: boss.paneId,
				cwd: bossHome(),
				focus: false,
			});
			this.openedAt = now;
		} catch (error) {
			this.nextTry = now + this.again;
			this.again = Math.min(this.again * 2, TODAY_AGAIN_MAX_MS);
			this.warn("today didn't open", error);
			return;
		}
		try {
			await this.herdr.setSplitRatio(boss.tabId, BOSS_SHARE);
		} catch (error) {
			this.warn("today's width wasn't set", error);
		}
	}
}

interface Shown {
	popup: Popup;
	held: boolean;
	heldNow: PromiseWithResolvers<void>;
	/** Ends the popup's held request: the popup closes. */
	release?: () => void;
}

/** Text cut to herdr's notification body (240 characters). */
const NOTE_BODY = 240;

/**
 * His questions in the plugin's `ask` popup (0553, 0558), one at a time: Allow questions and the close question first,
 * then those his keys opened (0578), each oldest first. The popup gets the question's one-time code through `--env` and
 * holds `GET /api/popup/<code>/hold` while it shows it. Until it reads the question, herdr is asked to open it again
 * (Settings or another popup in front, no client attached); once read, never again, since a second read ends it
 * (0578), and a read never followed by its hold within the wait counts as closed. A held request that ends before the
 * answer is the popup closed some other way: the question ends as its fallback. Each question but those his own keys
 * opened also goes out as a notification with herdr's request sound, for when the terminal isn't in front (0552, 0581).
 */
export class AskPopups {
	private readonly queue: Popup[] = [];
	private current: Shown | undefined;
	private readonly warn: (subject: string, error: unknown) => void;

	constructor(
		private readonly herdr: Herdr,
		private readonly options: { holdWaitMs?: number; retryMs?: number; now?: () => number } = {},
	) {
		this.warn = warner(options.now ?? Date.now);
	}

	show(popup: Popup): void {
		this.queue.push(popup);
		void popup.answer.then(() => this.ended(popup));
		this.pump();
	}

	/** A question asked again (0578): first among its kind in the queue; the one shown now stays as it is. */
	front(popup: Popup): void {
		const index = this.queue.indexOf(popup);
		if (index <= 0 || this.current?.popup === popup) return;
		this.queue.splice(index, 1);
		this.queue.unshift(popup);
		this.pump();
	}

	/** The popup's held request for `code` (`GET /api/popup/<code>/hold`): 403 unless it is the question shown now. */
	hold(code: string, signal: AbortSignal): Response {
		const shown = this.current;
		if (!shown || shown.popup.code !== code || !shown.popup.open || shown.held) {
			return new Response("Forbidden", { status: 403 });
		}
		const gone = () => {
			if (!shown.popup.open) return;
			log("info", "question", `the popup closed unanswered: ${shown.popup.view.title}`);
			shown.popup.close("closed");
		};
		let controller: ReadableStreamDefaultController<string> | undefined;
		const stream = new ReadableStream<string>({
			start(c) {
				controller = c;
				c.enqueue(": shown\n\n");
			},
			cancel: gone,
		});
		signal.addEventListener("abort", gone, { once: true });
		shown.held = true;
		shown.release = () => {
			try {
				controller?.enqueue("event: end\ndata: over\n\n");
				controller?.close();
			} catch {
				// Already closed by the popup.
			}
		};
		shown.heldNow.resolve();
		return new Response(stream, { headers: { "content-type": "text/event-stream", "cache-control": "no-store" } });
	}

	private ended(popup: Popup): void {
		const index = this.queue.indexOf(popup);
		if (index >= 0) this.queue.splice(index, 1);
		if (this.current?.popup !== popup) return;
		this.current.release?.();
		this.current = undefined;
		this.pump();
	}

	private pump(): void {
		if (this.current) return;
		const next = this.queue.find((p) => p.open && !p.fromKey) ?? this.queue.find((p) => p.open);
		if (!next) return;
		const shown: Shown = { popup: next, held: false, heldNow: Promise.withResolvers<void>() };
		this.current = shown;
		void this.present(shown);
	}

	private async present(shown: Shown): Promise<void> {
		const { popup } = shown;
		let first = true;
		while (this.current === shown && popup.open && !shown.held) {
			if (popup.read) {
				// Read and not held within the wait: that popup is gone, and opening another would read it again.
				await Promise.race([shown.heldNow.promise, popup.answer, Bun.sleep(this.options.holdWaitMs ?? 10_000)]);
				if (this.current === shown && popup.open && !shown.held) {
					log("info", "question", `the popup read the question and closed: ${popup.view.title}`);
					popup.close("closed");
				}
				return;
			}
			let wait = this.options.holdWaitMs ?? 10_000;
			try {
				await this.herdr.openPluginPane({
					pluginId: PLUGIN_ID,
					entrypoint: "ask",
					placement: "popup",
					env: { [POPUP_ENV]: popup.code },
					focus: true,
				});
			} catch (error) {
				wait = this.options.retryMs ?? 3_000;
				this.warn("the question's popup didn't open", error);
			}
			// A question his own key opened shows where he looks: no notification (0581, UX-6).
			if (first && !popup.fromKey) {
				first = false;
				const body =
					popup.view.message.length > NOTE_BODY ? `${popup.view.message.slice(0, NOTE_BODY - 1)}…` : popup.view.message;
				this.herdr
					.notify({ title: "Jocasta asks", body, sound: "request" })
					.catch((error: unknown) => this.warn("the question's notification failed", error));
			}
			await Promise.race([shown.heldNow.promise, popup.answer, Bun.sleep(wait)]);
		}
	}
}

/** What one of his keys did (0581, UX-5): a half-done or refused outcome is an error (red, and a notification). */
export interface ActionOutcome {
	text: string;
	isError: boolean;
}

/** What his thread keys need beside the service: a question, Jocasta's close and where a start line goes (0563). */
export interface ActionDeps {
	service: Pick<JocastaService, "keyStop" | "keyReview" | "keyFinishable" | "keyFinish" | "newThreadForm" | "keyStart">;
	/** Asks him `view`, opened by his key (no notification, after Allow questions); it ends unanswered on `signal`. */
	ask(view: PopupView, signal: AbortSignal): Popup;
	/** Brings an open question to the front of the queue (asked again). */
	front(popup: Popup): void;
	quit(): Promise<"stopped" | "kept">;
	/** A start line for a slow action, said at once (0581, UX-5). */
	progress(text: string): void;
	/** The open questions of his keys by (action, thread): one at a time each (0578, SEC-NET-8). */
	asking: Map<string, Popup>;
	/** How long Finish and New thread wait for his answer, as Allow does: `CONFIRM_WAIT_MS`. */
	waitMs: number;
}

/** Asks `view` as (action, thread) `key`, so a repeat finds it; its answer and how it ended. */
async function askOnce(deps: ActionDeps, key: string, view: PopupView): Promise<Popup> {
	const popup = deps.ask(view, AbortSignal.timeout(deps.waitMs));
	deps.asking.set(key, popup);
	try {
		await popup.answer;
	} finally {
		if (deps.asking.get(key) === popup) deps.asking.delete(key);
	}
	return popup;
}

/**
 * One of his thread keys (0544, 0554) on the thread of `paneId` (the focused pane, or the thread picked in `today`):
 * what it did, in one line; a refusal throws `UserError` with why. A Finish or New thread whose question is open
 * already brings that one to the front instead (0578); a New thread then says nothing more.
 */
export async function runAction(
	action: PluginAction,
	paneId: string | undefined,
	deps: ActionDeps,
): Promise<ActionOutcome | undefined> {
	const said = (text: string): ActionOutcome => ({ text, isError: false });
	switch (action) {
		case "stop":
			return said(await deps.service.keyStop(paneId));
		case "review":
			return said(await deps.service.keyReview(paneId));
		case "finish": {
			const finish = await deps.service.keyFinishable(paneId);
			const key = `finish:${finish.paneId}`;
			const open = deps.asking.get(key);
			if (open?.open) {
				deps.front(open);
				return said(`Finish: already asking about ${finish.name}.`);
			}
			const popup = await askOnce(deps, key, finishQuestion(finish.name, finish.detail));
			const choice = await popup.answer;
			if (choice !== MERGE && choice !== PR) return said(`Finish: ${finish.name} stays as it was.`);
			if (choice === MERGE) deps.progress(`Merging ${finish.name} into ${finish.trunk}…`);
			return deps.service.keyFinish(finish.paneId, choice === MERGE ? "merge" : "pr");
		}
		case "new-thread": {
			const open = deps.asking.get("new-thread");
			if (open?.open) {
				deps.front(open);
				return undefined;
			}
			const form = await deps.service.newThreadForm(paneId);
			const popup = await askOnce(deps, "new-thread", newThreadQuestion(form.projects, form.project));
			const { project, brief } = popup.fields;
			if ((await popup.answer) !== START || !project || !brief) return said("New thread: none started.");
			deps.progress(`Starting a thread in ${project}: worktree, omp, brief…`);
			return deps.service.keyStart(project, brief);
		}
		case "quit":
			return said((await deps.quit()) === "kept" ? "Jocasta keeps running." : "Jocasta stopped.");
		default:
			throw new UserError(`no action ${String(action)}.`);
	}
}
