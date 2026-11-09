/**
 * What the service says (0405, 0407, 0554): `[event]` lines into the boss pane, held while the boss works and in quiet
 * hours (0298), and messages to the clients, a toast also raised as a Windows notification through herdr.
 */
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { webLink } from "../../shared/link.ts";
import { log } from "../../shared/log.ts";
import { dataDir } from "../../shared/paths.ts";
import type { ServerMessage } from "../protocol.ts";
import type { ServiceContext } from "./context.ts";
import type { AlertDetails } from "./day.ts";
import { type AgentState, paneText } from "./herdr.ts";
import { isIdle, subject, type Thread } from "./thread.ts";
import { cut, quoted } from "./wording.ts";

/** herdr's notification limits (0554): a title of 80 characters, a body of 240. */
const NOTIFICATION_TITLE = 80;
const NOTIFICATION_BODY = 240;

/** The boss pane as the service keeps it (0404). */
export interface BossPane {
	paneId: string;
	workspaceId: string;
	state: AgentState;
	sessionPath?: string;
	/** When herdr last pushed its state. */
	eventAt?: number;
	/** When the service last put a line into it. */
	promptedAt?: number;
}

/**
 * `value` with control characters (ESC, BEL, the other C0 but tab and newline, DEL, C1) removed from every string in
 * it (N-3): what the page, toasts and speech get can't carry terminal escapes from a title or a question.
 */
export function clean<T>(value: T): T {
	if (typeof value === "string") return paneText(value) as T;
	if (Array.isArray(value)) return value.map(clean) as T;
	if (value && typeof value === "object") {
		return Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, clean(inner)])) as T;
	}
	return value;
}

export class Events {
	/** `[event]` lines waiting for the boss to be idle (0405), each with what must still hold when it goes (P8). */
	private readonly bossQueue: { line: string; holds?: () => boolean }[] = [];
	/** Notifications and `[event]` lines sent without waiting, until herdr answered and the log line is written. */
	private readonly inFlight = new Set<Promise<void>>();
	/** Notifications herdr couldn't show for want of a client, by text, to send again once (`resendUnshown`). */
	private readonly unshown = new Map<string, Extract<ServerMessage, { type: "toast" }>>();
	private resending = false;

	constructor(
		private readonly c: ServiceContext,
		private readonly broadcast: (message: ServerMessage) => void,
	) {}

	/** The boss pane's state as `today` would say it: herdr's `done` is idle. */
	bossState(): "working" | "blocked" | "idle" | "unknown" {
		const state = this.c.boss?.state ?? "unknown";
		return state === "done" ? "idle" : state;
	}

	/** The boss pane's state changed: an idle boss gets what waits for it. */
	async bossChanged(state: AgentState): Promise<void> {
		const boss = this.c.boss;
		if (!boss || boss.state === state) return;
		boss.state = state;
		if (isIdle(state)) await this.flush();
	}

	/**
	 * A thread's change as an `[event]` line for the boss (0405), in the one format (UX-21): `[event] <project>/<title> —
	 * <what>`. In quiet hours only urgent ones go (0298); a normal one is held and goes when they end (0470's way). A
	 * line that waits for the boss and has `holds` goes only if it still holds then (beta day P8: a plan approved
	 * meanwhile isn't said as waiting).
	 */
	async thread(thread: Thread, what: string, priority: "urgent" | "normal", holds?: () => boolean): Promise<void> {
		// Thread lines that aren't queue rows go into today's activity (N-4); urgent ones (awaiting him) are rows.
		if (priority === "normal") this.c.day.recordEvent(`${subject(thread)} — ${what}`);
		if (priority === "normal" && this.c.day.quiet()) {
			this.c.day.hold(`${subject(thread)} — ${what}`);
			return;
		}
		await this.tell(`[event] ${subject(thread)} — ${what}`, priority, holds);
	}

	/**
	 * One of his day's events (0299) as an `[event]` line (UX-21): its text with the parts from outside quoted
	 * (SEC-2), then ` · <url>` when the url is a plain http(s) link (SE-4): a newline or a script link from a calendar
	 * entry never reaches the boss pane.
	 */
	alert(text: string, details: AlertDetails): void {
		let said = text.replace(/\s+/g, " ");
		for (const part of details.outside ?? []) {
			const flat = part.replace(/\s+/g, " ");
			const at = said.indexOf(flat);
			if (at >= 0) said = `${said.slice(0, at)}${quoted(flat)}${said.slice(at + flat.length)}`;
		}
		const url = webLink(details.url);
		const line = `[event] ${said}${url ? ` · ${url}` : ""}`;
		this.track(this.tell(line, details.priority).catch((e) => this.c.warn(e)));
	}

	/**
	 * Urgent lines go into the boss pane at once; normal ones wait until it is idle (0405). A line put in makes the boss
	 * busy at once, so the next one waits for the turn it starts (CR-08); herdr's next idle sends what waits as one
	 * prompt, without the lines that no longer hold (P8).
	 */
	async tell(line: string, priority: "urgent" | "normal", holds?: () => boolean): Promise<void> {
		const boss = this.c.boss;
		if (priority === "urgent" && boss) {
			if (await this.promptBoss(boss, line)) return;
			// Not ready, or blocked on his answer: it waits with the rest.
		}
		this.bossQueue.push({ line, ...(holds ? { holds } : {}) });
		await this.flush();
	}

	private async flush(): Promise<void> {
		const boss = this.c.boss;
		if (!boss || !isIdle(boss.state) || this.bossQueue.length === 0) return;
		const waiting = this.bossQueue.splice(0).filter((queued) => queued.holds?.() ?? true);
		if (waiting.length === 0) return;
		if (!(await this.promptBoss(boss, waiting.map((queued) => queued.line).join("\n")))) {
			this.bossQueue.unshift(...waiting);
		}
	}

	/**
	 * Puts `text` into the boss pane, marking it busy until herdr says otherwise (CO-7): herdr's own working event then
	 * changes nothing, and the next line waits for the turn this one starts. False when herdr refused it.
	 */
	private async promptBoss(boss: BossPane, text: string): Promise<boolean> {
		const was = boss.state;
		boss.state = "working";
		boss.promptedAt = Date.now();
		try {
			await this.c.herdr.prompt(boss.paneId, text);
			return true;
		} catch (error) {
			if (boss.state === "working") boss.state = was;
			this.c.warn(error);
			return false;
		}
	}

	/** A message to every client, cleaned (N-3); a toast is also a notification. */
	send(message: ServerMessage): void {
		// Control characters from outside (titles, questions, reminders) stop here for every surface (N-3).
		message = clean(message);
		this.broadcast(message);
		if (message.type === "toast") this.track(this.notify(message));
	}

	/** Keeps `work` until it settled, so the service's stop can wait for it (`settled`). */
	private track(work: Promise<void>): void {
		this.inFlight.add(work);
		void work.finally(() => this.inFlight.delete(work));
	}

	/** Resolves once every notification and line sent without waiting settled (the service's stop, before herdr closes). */
	async settled(): Promise<void> {
		while (this.inFlight.size > 0) await Promise.allSettled([...this.inFlight]);
	}

	/**
	 * A toast as a Windows notification through herdr (0407, 0554), and a refused key's (0552): the thread it names as
	 * the title (the text before " — "), else "Jocasta", and the rest as the body, cut to herdr's 80 and 240
	 * characters; urgent and blocked ones with herdr's request sound. Each call and herdr's answer is one line of
	 * `<data>/notifications.jsonl`, for the benchmark's time to the notification (H2, 0543). One herdr couldn't show
	 * for want of a client (`no_foreground_client`: the service started before his terminal attached) is kept and sent
	 * again once by `resendUnshown` (0584: the start's settings problems are said once).
	 */
	async notify(toast: Extract<ServerMessage, { type: "toast" }>, options: { show?: boolean } = {}): Promise<void> {
		const split = toast.text.indexOf(" — ");
		const title = cut(split > 0 ? toast.text.slice(0, split) : "Jocasta", NOTIFICATION_TITLE);
		const body = cut(split > 0 ? toast.text.slice(split + 3) : toast.text, NOTIFICATION_BODY);
		const urgent = toast.reason === "blocked" || toast.priority === "urgent";
		let result: string;
		try {
			// A repeat (0578, SEC-NET-8) is logged, not shown again.
			result =
				options.show === false
					? "repeat"
					: await this.c.herdr.notify({ title, body, sound: urgent ? "request" : "none" });
		} catch (error) {
			result = "error";
			this.c.warn(error);
		}
		if (result === "no_foreground_client") this.unshown.set(toast.text, toast);
		else if (result !== "repeat") this.unshown.delete(toast.text);
		try {
			appendFileSync(
				join(dataDir(), "notifications.jsonl"),
				`${JSON.stringify({ at: Date.now(), paneId: toast.paneId ?? null, reason: toast.reason, title, body, result })}\n`,
			);
		} catch (error) {
			this.c.warn(error);
		}
	}

	/**
	 * Sends the notifications herdr couldn't show for want of a client again, in order (each refresh tick): the first
	 * one probes; while herdr still has no client they all wait for the next tick. Once herdr answered anything else, a
	 * notification isn't sent again.
	 */
	resendUnshown(): void {
		if (this.unshown.size === 0 || this.resending) return;
		this.resending = true;
		this.track(
			(async () => {
				try {
					for (const toast of [...this.unshown.values()]) {
						if (this.c.stopping) return;
						await this.notify(toast);
						if (this.unshown.has(toast.text)) return;
					}
				} finally {
					this.resending = false;
				}
			})(),
		);
	}

	/** A warning toast and its log line: a problem said to him once (settings, `projects.yaml`, a missing tool). */
	warning(text: string, source = "service"): void {
		log("warn", source, text);
		this.send({ type: "toast", reason: "event", level: "warning", text });
	}
}
