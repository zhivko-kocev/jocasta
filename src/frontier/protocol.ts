/**
 * The service's state feed and question API (4.1, 0409, 0552): typed JSON messages from the Jocasta service to a
 * client (the herdr plugin's panes, 0553) and the questions it asks him. The service owns this file.
 *
 * Transport: a client opens `ws://127.0.0.1:<port>/ws` with the service's token (the cookie `/?t=<token>` sets, or
 * `Authorization: Bearer <token>`, 0254). Every message is one JSON object with a `type`. Questions (the close
 * question, his Allow) are read and answered over HTTP with a one-time code, never broadcast (0519).
 */

/** The WebSocket path. */
export const WS_PATH = "/ws";
/** The query parameter a browser page opens with: `http://127.0.0.1:<port>/?t=<token>` (0254). */
export const TOKEN_QUERY = "t";
/** The cookie the token becomes (0254). */
export const TOKEN_COOKIE = "jocasta_frontier";
/**
 * Where a question's view reads its texts (`GET /api/popup/<code>`) and posts its answer (`POST /api/popup/<code>`, body
 * `PopupAnswer`), with the service's token and the question's one-time code (0519). The code is never listed or
 * broadcast: the service hands it only to what shows the question (the herdr popup, 0553). The popup holds
 * `GET /api/popup/<code>/hold` open while it shows the question: the question ends unanswered when that request ends
 * before an answer (closing the popup any other way denies, 0558), and the service ends it once the question is over.
 */
export const POPUP_PATH = "/api/popup/";
/** The suffix of a popup's held request (`POPUP_PATH` + code + this). */
export const POPUP_HOLD = "/hold";
/** The variable that hands the plugin's `ask` popup its question's one-time code (0553), through herdr's `--env`. */
export const POPUP_ENV = "JOCASTA_POPUP";
/**
 * His thread keys (0544, 0554): `POST /api/plugin/action` with the service's token, body `{action, paneId?}`; answered
 * at once (202), the outcome follows as an `action` message and, when refused, a notification.
 */
export const PLUGIN_ACTION_PATH = "/api/plugin/action";
/** The herdr plugin's actions (0553, 0554): Finish, Stop, Review and New thread on the focused thread; Quit closes Jocasta. */
export const PLUGIN_ACTIONS = ["finish", "stop", "review", "new-thread", "quit"] as const;
export type PluginAction = (typeof PLUGIN_ACTIONS)[number];

/**
 * One question for him (0434, 0484, 0519, 0558): its choices; `fallback` is Escape's and the answer when the question
 * goes unanswered (Jocasta stops, its wait runs out, the popup closes any other way).
 */
export interface PopupView {
	title: string;
	message: string;
	detail?: string;
	choices: readonly string[];
	fallback: string;
	/**
	 * Allow, Merge, PR and Stop everything (0558, 0568): three random letters he types to take any choice but the
	 * fallback; Enter and the other keys never take those. Absent for New thread, where Enter takes the selected choice.
	 */
	code?: string;
	/** New thread (0554): the projects to start one in, the one picked first, and a brief he types. */
	form?: { projects: string[]; project?: string };
}

/** A popup's answer (`POST /api/popup/<code>`): the choice, the letters he typed (a coded choice), the form's fields. */
export interface PopupAnswer {
	choice: string;
	typed?: string;
	project?: string;
	brief?: string;
}

/** The boss pane's agent state from herdr (0392): the Presence's orbit runs while it works. */
export type BossState = "working" | "blocked" | "idle" | "unknown";

/** One thread row on the rail (0409): a herdr thread pane. */
export interface RailItem {
	paneId: string;
	title: string;
	project: string | null;
	/** One plain line: what it is doing, its question, or "review ready". */
	line: string;
	/** The titles of the project's other open threads that change the same files (0456); absent when none. */
	overlaps?: string[];
	/** An open thread's state, when it changed (epoch ms), its session's cost (USD) and context (tokens), for `today` (0555). */
	state?: "working" | "blocked" | "done" | "idle";
	since?: number;
	cost?: number;
	context?: number;
}

/** How long a thread has waited on him (0456), short: `25 min`, `3 h`, `2 d`; `1 min` at least. */
export function waitLabel(ms: number): string {
	const minutes = Math.max(1, Math.floor(ms / 60_000));
	if (minutes < 60) return `${minutes} min`;
	if (minutes < 60 * 24) return `${Math.floor(minutes / 60)} h`;
	return `${Math.floor(minutes / (60 * 24))} d`;
}

/** A subscription window this used or more (0456) gets its one toast and shows high on the rail: the builder's default. */
export const QUOTA_WARN = 0.8;

/** A window's use as the rail and the HUD say it, a whole percent: 0.796 is 80. */
export function quotaPercent(q: Pick<QuotaWindow, "used">): number {
	return Math.round(q.used * 100);
}

/**
 * Whether a window is past `QUOTA_WARN` (0456), judged on the percent he reads, so "80%" is always high: the rail's
 * colour and the toast follow this one rule.
 */
export function quotaHigh(q: Pick<QuotaWindow, "used">): boolean {
	return quotaPercent(q) >= Math.round(QUOTA_WARN * 100);
}

/** The `today` view (0300) as the rail shows it: Awaiting you, Landed, Underway, Next (0409). */
export interface Today {
	/** The local day, `YYYY-MM-DD`. */
	date: string;
	/**
	 * Today's meetings from the calendar (8.1), by start; all-day ones first with `time` "all day". `url` is the
	 * meeting's link (0429): the entry's URL, a link in its location, or a Teams/Meet/Zoom link in its description.
	 */
	calendar: { time: string; title: string; url?: string }[];
	/**
	 * What awaits him (0432): reviews requested, red pipelines, threads awaiting him, threads reviewed and not finished
	 * (0456: oldest first, with `since`), inbox reminders due today (0302) and a nearly full disk. Every row counts in
	 * `Hud.awaiting`.
	 */
	needsYou: {
		kind: "review" | "pipeline" | "blocked" | "reviewed" | "reminder" | "disk";
		/** One line, `<subject> — <what>`; a thread's is `<title> — <what>` with its project apart. */
		text: string;
		url?: string;
		paneId?: string;
		/** A thread row's project. */
		project?: string;
		/** A reminder's time, `HH:MM`. */
		time?: string;
		/** A reviewed thread's wait (0456): when its review was ready, ISO time. */
		since?: string;
		/** A thread row's overlapping open threads (0456), as `RailItem.overlaps`. */
		overlaps?: string[];
	}[];
	/**
	 * What landed since yesterday (0263, 0300): threads that finished a task and aren't reviewed yet (with their pane),
	 * then threads merged into trunk or sent as a PR, oldest first, whose pane is gone (no `paneId`).
	 */
	landed: (Omit<RailItem, "paneId"> & { paneId?: string })[];
	/** Open threads working, or idle with nothing for him (0555): with their state, time, cost and context. */
	underway: RailItem[];
	/** Inbox notes filed into projects' next work (0302), oldest first. */
	next: { id: number; text: string; project: string | null }[];
}

/** One headline from brutalist.report (0429): title, source, when it was published and its link only. */
export interface Headline {
	title: string;
	source: string;
	url: string;
	/** Published, ISO time. */
	at: string;
}

/** Source names said in words (0442); any other source shows as brutalist.report names it. */
const NEWS_NAMES: Record<string, string> = { hn: "Hacker News", lwn: "LWN" };

/** A headline's source as the page says it: `hn` → "Hacker News", `lwn` → "LWN", any other as it is. */
export function newsSourceName(source: string): string {
	return NEWS_NAMES[source] ?? source;
}

/** A feed's heading (0442): "Hacker News", "LWN", otherwise its source or topic capitalized ("Gaming"). */
export function newsHeading(name: string): string {
	return NEWS_NAMES[name] ?? name.charAt(0).toUpperCase() + name.slice(1);
}

/** One feed of `pollers.news.feeds` (0442): its heading and headlines, newest first. */
export interface NewsList {
	heading: string;
	items: Headline[];
}

/** The weather for `dashboard.weather` from Open-Meteo (0429). */
export interface Weather {
	/** The place as Open-Meteo's geocoding named it, e.g. "Example City, Example Country". */
	place: string;
	tempC: number;
	/** The WMO weather code. */
	code: number;
	/** The code in words, sentence case: "Partly cloudy". */
	summary: string;
	highC: number;
	lowC: number;
	/** Observed, ISO time. */
	at: string;
}

/** CPU, RAM and the data dir's disk (0429); fractions and bytes, rounded so they change on whole percents. */
export interface Machine {
	/** Busy fraction of all cores since the last sample, 0..1. */
	cpu: number;
	ram: { used: number; total: number };
	disk: { free: number; total: number };
}

/**
 * A service Jocasta relies on (0429, 0444): `up` answers; `starting` while Jocasta starts it; `idle` while it isn't running
 * and starts when needed (Graphiti, 0073); `down` is broken, red on the page.
 */
export interface ServiceHealth {
	name: "Graphiti" | "whiteboard" | "herdr";
	state: "up" | "starting" | "idle" | "down";
	/** One plain line after the state, e.g. "starts when needed", "Docker isn't running". */
	note?: string;
}

/**
 * A thread's worktree left on disk after its thread was closed in herdr (0456): its branch, commits not on the trunk
 * and changed files not committed. Removed only on his word.
 */
export interface Leftover {
	project: string;
	/** The worktree's folder, absolute. */
	folder: string;
	branch: string | null;
	ahead: number;
	dirty: number;
}

/** The dashboard's third column (0428, 0429): news, weather, machine and services, today's activity. */
export interface Dashboard {
	/** One list per feed in `pollers.news.feeds` (0442); the last good lists stay on failure. */
	news: NewsList[];
	/** The news poller's last failure, one plain line; null once a round works. */
	newsError: string | null;
	/** Null until `dashboard.weather` is set (the section is hidden) or the first round answered. */
	weather: Weather | null;
	weatherError: string | null;
	/** Null until the first sample. */
	machine: Machine | null;
	services: ServiceHealth[];
	activity: {
		/** The latest delivered `[event]` lines, latest first, at most 10. */
		events: { at: string; text: string }[];
		/** Today's worklog so far, one line per project: `acme-api · 1h 20m · 3 commits`. */
		worklog: string[];
		/** Compaction and handoff summaries saved today, latest first: their first line. */
		summaries: { at: string; project: string | null; text: string }[];
	};
	/** Thread worktrees left on disk (0456), by project and folder. */
	leftovers: Leftover[];
}

/** One subscription window from `omp usage --json` (0456): the most used of that window across his accounts. */
export interface QuotaWindow {
	window: "5h" | "7d";
	/** Fraction used, 0..1 (above 1 past the limit). */
	used: number;
	/** When the window resets, ISO time, when omp knows it. */
	resetsAt?: string;
}

/** The HUD figures (0403): the same ones `<data>/hud.txt` carries. */
export interface Hud {
	/** Thread panes working. */
	running: number;
	/** Thread panes waiting on him. */
	blocked: number;
	/** Every row of `today.needsYou` (0432): the same count the greeting says. */
	awaiting: number;
	/**
	 * Today's spend in USD, summed from today's assistant messages in omp's session files (local day); null while the
	 * service is still reading today's sessions after its start.
	 */
	spendToday: number | null;
	/** The subscription windows (0456), 5h then 7d; empty until `omp usage` answered with them. */
	quota: QuotaWindow[];
}

/** Service → client. */
export type ServerMessage =
	/** Sent on connect: everything `today` shows (0409). */
	| {
			type: "snapshot";
			hud: Hud;
			today: Today;
			boss: BossState;
			/** Whether quiet hours are on now (0298). */
			quietHours: boolean;
			/** The dashboard's third column (0428, 0429). */
			dashboard: Dashboard;
	  }
	/** The dashboard's third column changed (0429): pushed only when it differs from the last one sent. */
	| { type: "dashboard"; dashboard: Dashboard }
	| { type: "hud"; hud: Hud }
	/** The boss pane's state changed. */
	| { type: "boss"; state: BossState }
	/**
	 * A notification (0407): raised in Windows through herdr (0554), urgent and blocked ones with herdr's request
	 * sound; `paneId` names the pane it is about.
	 */
	| {
			type: "toast";
			reason: "blocked" | "done" | "failed" | "event";
			level: "info" | "warning" | "error";
			text: string;
			paneId?: string;
			/** For `event` (0299): urgent goes through quiet hours, and sounds like a blocked thread. */
			priority?: "urgent" | "normal";
			url?: string;
	  }
	/** The `today` view (0300): in reply to `today.open`, and pushed while it changes. */
	| { type: "today"; today: Today }
	/** Quiet hours began or ended (0298): only urgent notifications go out while quiet. */
	| { type: "quiet"; quiet: boolean }
	/** What one of his thread keys did, or why it didn't (0544, 0554): `today` shows the last one. */
	| { type: "action"; text: string; isError: boolean }
	| { type: "error"; message: string }
	/** The service is stopping (Jocasta closing as one; 0434). */
	| { type: "bye" };

/** Client → service. */
export type ClientMessage =
	/** Text for the boss pane (0392): typed into it through herdr. */
	| { type: "boss.send"; text: string }
	/** A row was chosen: focus that pane in herdr (0407). */
	| { type: "pane.focus"; paneId: string }
	/** Refresh `today` (0300). */
	| { type: "today.open" };
