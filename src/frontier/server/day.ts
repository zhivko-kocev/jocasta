import type { Database } from "bun:sqlite";
import { existsSync, statfsSync } from "node:fs";
import { calendarSecret } from "../../shared/credentials.ts";
import { dayLabel, dayStart, hhmm, localDay, midnightOf } from "../../shared/day.ts";
import { Inbox, type InboxItem } from "../../shared/events/inbox.ts";
import {
	EventQueue,
	type EventRow,
	type EventSource,
	type NewEvent,
	type PollRound,
} from "../../shared/events/queue.ts";
import { isQuiet, type QuietHours, readQuietHours } from "../../shared/events/quiet.ts";
import { log } from "../../shared/log.ts";
import { dataDir } from "../../shared/paths.ts";
import { AZURE_KINDS, pollAzure } from "../../shared/pollers/azure.ts";
import { type Meeting, pollCalendar } from "../../shared/pollers/calendar.ts";
import { type RunCli, runCli } from "../../shared/pollers/cli.ts";
import { GITHUB_KINDS, pollGitHub } from "../../shared/pollers/github.ts";
import { type CpuTimes, cpuTimes, sampleMachine } from "../../shared/pollers/machine.ts";
import { pollNews } from "../../shared/pollers/news.ts";
import { type PollerSettings, readPoller } from "../../shared/pollers/settings.ts";
import { findPlace, type Place, pollWeather, readWeatherPlace } from "../../shared/pollers/weather.ts";
import type { Project } from "../../shared/projects.ts";
import { openMeDb } from "../../shared/schema.ts";
import { goneSettings, readSettings, SettingsError } from "../../shared/settings.ts";
import {
	type Dashboard,
	type Leftover,
	type Machine,
	type NewsList,
	type QuotaWindow,
	quotaHigh,
	type RailItem,
	type ServerMessage,
	type ServiceHealth,
	type Today,
	type Weather,
	waitLabel,
} from "../protocol.ts";
import { ompUsage, quotaWarning, readQuota } from "./quota.ts";
import { graphSpentOn } from "./spend.ts";
import {
	commitsAsync,
	GitPlaces,
	projectDay,
	projectDayLine,
	type WorklogSession,
	worklogPath,
	writeWorklog,
} from "./worklog.ts";

const MINUTE = 60_000;
/** The one reminder comes 30 minutes after an event was raised (0303). */
const REMINDER_AFTER_MS = 30 * MINUTE;
/** A meeting is urgent this close to its start (0298). */
const MEETING_SOON_MS = 5 * MINUTE;
/** Disk under this fraction free is urgent (0303). */
const DISK_LOW = 0.1;
/** Without quiet hours, the worklog is written at this hour (0302: "at day's end"). */
const WORKLOG_HOUR = 23;
/** The weather refreshes every 30 minutes (0429); Open-Meteo's data moves every 15. */
const WEATHER_MINUTES = 30;
/** CPU, RAM, disk and the services are sampled this often (0429; every 10 s, 0443). */
const MACHINE_MS = 10_000;
/** Today's worklog so far is recomputed this often: it reads every project's sessions and git log. */
const WORKLOG_SO_FAR_MS = 10 * MINUTE;
/** The activity lists' length (0429). */
const ACTIVITY_ROWS = 10;
/** The subscription windows are read this often (0456). */
const QUOTA_MINUTES = 5;
/** Leftover worktrees are looked for this often (0456): they change only when a thread is closed. */
const LEFTOVER_MINUTES = 10;
/** The weekly review (0470) runs from this weekday (0 = Sunday) and hour on, once per project and week. */
const WEEKLY_REVIEW_DAY = 1;
const WEEKLY_REVIEW_HOUR = 10;

/** Test seams; the service's server runs with the defaults. */
export interface DayOptions {
	/** Whether the pollers run (0301); tests turn them off and call `poll()` themselves. Default true. */
	pollers?: boolean;
	run?: RunCli;
	fetchUrl?: typeof fetch;
	now?: () => Date;
	/** The calendar's secret iCal URL (0430): Credential Manager, else the env var it was taken from. */
	calendarSecret?: () => string | undefined;
	/** The free fraction of the data dir's disk. */
	diskFree?: () => number;
	/** `omp usage --json`'s output (0456). */
	usage?: () => Promise<string>;
}

/** How a delivered event reaches the boss pane (0299, 0405): urgent at once, normal when the boss is idle. */
export interface AlertDetails {
	priority: "urgent" | "normal";
	url?: string;
	reminder?: boolean;
	/**
	 * The parts of the text that came from outside (SEC-2): PR titles and repos, check and pipeline names, meeting
	 * titles. The service marks them in the boss's line as information, never his instruction.
	 */
	outside?: string[];
}

/**
 * The event kinds whose line after " — " is someone else's words (SEC-2): a blocked thread's question is its omp's
 * (SE-2), but never Jocasta's own tails.
 */
const OUTSIDE_KINDS: readonly string[] = ["review", "pipeline", "meeting-soon", "blocked", "weekly-review"];
/** A blocked row's tails Jocasta wrote itself: never quoted. */
const OWN_TAILS: readonly string[] = ["awaiting you", "plan ready"];

/** A thread pane as `today` shows it (0409). */
export interface DayThread extends RailItem {
	state: "working" | "blocked" | "done" | "idle";
	/** When it last changed state, epoch ms. */
	changedAt: number;
	/** When its review was ready (0456), epoch ms: set while it is reviewed and not finished. */
	reviewedAt?: number;
}

/** What the day service needs from the Jocasta service. */
interface DayHooks {
	send(message: ServerMessage): void;
	/** An `[event]` line for the boss pane (0299, 0405). */
	alert(text: string, details: AlertDetails): void;
	notify(text: string, level: "warning" | "error"): void;
	projects(): Project[];
	threads(): DayThread[];
	/**
	 * Every project's omp sessions (in its main checkout and its worktrees) that may hold messages of `day`, by project
	 * id: the sessions folder listed once (CO-12).
	 */
	sessionsByProject(day: string): ReadonlyMap<string, WorklogSession[]>;
	/**
	 * Reads the sessions that may hold `day`'s messages in steps, letting the event loop run, so `sessionsByProject`
	 * for that day then only parses what was appended.
	 */
	prepareSessions(day: string): Promise<void>;
	/** Whether Graphiti, whiteboard and herdr answer (0429), for the dashboard's services row. */
	services(): Promise<ServiceHealth[]>;
	/** The watched project a folder (a main checkout or a thread's worktree) belongs to, by name; null outside. */
	projectName(cwd: string): string | null;
	/** Thread worktrees closed in herdr and still on disk (0456). */
	leftovers(): Promise<Leftover[]>;
	/**
	 * The weekly big-picture review of one project (0470): what merged in the past 7 days against its rules and
	 * lessons, read-only. Resolves to the result's one line, or null when nothing merged that week.
	 */
	weeklyReview(project: Project): Promise<string | null>;
}

type PollerName = "github" | "azureDevOps" | "calendar" | "news" | "weather";

/** Whether `error` is a settings problem the service already said at start (0584, C7). */
const said = (error: unknown) => error instanceof SettingsError && error.said;

function diskFreeOfData(): number {
	const stats = statfsSync(dataDir());
	return stats.blocks > 0 ? stats.bavail / stats.blocks : 1;
}

/**
 * When a reminder is due (0302), from the boss: `HH:MM` (today, or tomorrow once that time passed), `YYYY-MM-DD HH:MM`
 * or an ISO time. Impossible times and moments already past are refused (F-11) with a one-line reason.
 */
function parseWhen(at: string, now: Date): Date {
	const value = at.trim();
	const impossible = `"${at}" is not a time: hours run 00–23 and minutes 00–59`;
	const time = /^(\d{1,2}):(\d{2})$/.exec(value);
	if (time) {
		const [h, mi] = [Number(time[1]), Number(time[2])];
		if (h > 23 || mi > 59) throw new Error(impossible);
		const due = new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, mi);
		if (due.getTime() <= now.getTime()) due.setDate(due.getDate() + 1);
		return due;
	}
	const local = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{1,2}):(\d{2})$/.exec(value);
	let due: Date | undefined;
	if (local) {
		const [y, mo, d, h, mi] = local.slice(1).map(Number) as [number, number, number, number, number];
		if (h > 23 || mi > 59) throw new Error(impossible);
		due = new Date(y, mo - 1, d, h, mi);
		// `new Date` rolls 2026-02-30 over to March; a day that doesn't exist is refused instead.
		if (due.getFullYear() !== y || due.getMonth() !== mo - 1 || due.getDate() !== d) {
			throw new Error(`"${at}" is not a day on the calendar`);
		}
	} else if (/^\d{4}-\d{2}-\d{2}T/.test(value) && !Number.isNaN(new Date(value).getTime())) {
		due = new Date(value);
	}
	if (!due) throw new Error(`"${at}" is not a time: use HH:MM or YYYY-MM-DD HH:MM`);
	if (due.getTime() <= now.getTime()) throw new Error(`"${at}" has already passed: give a time still to come`);
	return due;
}

/** A reminder's `[event]` line (UX-05): `Reminder (16:07) — send the invoice`. */
function reminderLine(text: string, due: Date): string {
	return `Reminder (${hhmm(due)}) — ${text}`;
}

/**
 * His day (phase 8): the pollers (8.1, 0301), the event queue with quiet hours (8.2, 0298), delivery (8.3, 0299),
 * interjections and their reminder (8.7, 0303), the `today` view (8.4, 0300), the inbox and worklog (8.5, 0302), and
 * the dashboard's third column (0429): news, weather, machine and services, today's activity.
 */
export class Day {
	readonly queue: EventQueue;
	readonly inbox: Inbox;
	private readonly me: Database;
	private readonly now: () => Date;
	/** `gh`/`az` and the pollers' fetches, ended when the day stops. */
	private readonly run: RunCli;
	private readonly fetchUrl: typeof fetch;
	private readonly halt = new AbortController();
	private meetings: Meeting[] = [];
	private readonly timers: Timer[] = [];
	/** The poller rounds under way, awaited by `stop()` before the databases close (F7). */
	private readonly rounds = new Map<PollerName, Promise<void>>();
	private readonly lastError = new Map<string, string>();
	private todaySent = "";
	private worklogDay = "";
	/** The local day of the last minute tick: a new one checks yesterday's worklog was written (SRV-B-5). */
	private tickDay = "";
	/** The worklog being written (yesterday's at start, today's at day's end): its sessions are read in steps first. */
	worklogWriting: Promise<void> = Promise.resolve();
	private stopped = false;
	// The dashboard's third column (0429): kept in memory, never ingested, spoken or sent to the boss or the graph.
	private news: NewsList[] = [];
	private newsError: string | null = null;
	private weather: Weather | null = null;
	private weatherError: string | null = null;
	/** The resolved `dashboard.weather`, so the place is looked up once per setting. */
	private place: { query: string; place: Place } | undefined;
	private machine: Machine | null = null;
	private cpuBefore: CpuTimes | undefined;
	private sampling = false;
	private services: ServiceHealth[] = [];
	private activity: Dashboard["activity"] = { events: [], worklog: [], summaries: [] };
	/** The service's thread lines delivered today, for today's activity. */
	private serviceEvents: Dashboard["activity"]["events"] = [];
	private worklogSoFarAt = 0;
	/** Whether today's sessions were read in steps, so the worklog so far may read them (CO-11). */
	private worklogReady = false;
	/** The worklog so far's pass running now (it runs git off the event loop, CO-12). */
	private worklogSoFar: Promise<void> | undefined;
	/** Each project's git email and origin for the worklogs, kept for one local day (CO-12). */
	private readonly gitPlaces = new GitPlaces();
	/** What Graphiti's model calls cost today (0582), for today's activity; 0 hides its line. */
	private graphToday = 0;
	private dashboardSent = "";
	/** The subscription windows (0456), as `omp usage` last reported them. */
	private quotaWindows: QuotaWindow[] = [];
	/** The windows past `QUOTA_WARN` whose toast went out; a window drops out once it is back under. */
	private readonly quotaWarned = new Set<QuotaWindow["window"]>();
	private quotaReading = false;
	private leftoverList: Leftover[] = [];
	private leftoversReading: Promise<void> | undefined;
	/** The weekly reviews (0470) run or checked this week, by `<week>|<project id>`; running ones included. */
	private readonly weeklyDone = new Set<string>();

	constructor(
		private readonly catalog: Database,
		private readonly hooks: DayHooks,
		private readonly options: DayOptions = {},
	) {
		this.now = options.now ?? (() => new Date());
		const run = options.run ?? runCli;
		this.run = (command, args, okCodes) => run(command, args, okCodes, this.halt.signal);
		const fetchUrl = options.fetchUrl ?? fetch;
		this.fetchUrl = Object.assign(
			(input: string | URL | Request, init?: RequestInit) =>
				fetchUrl(input, {
					...init,
					signal: init?.signal ? AbortSignal.any([init.signal, this.halt.signal]) : this.halt.signal,
				}),
			{ preconnect: fetchUrl.preconnect },
		);
		this.queue = new EventQueue(catalog, this.now);
		this.me = openMeDb();
		this.inbox = new Inbox(this.me, this.now);
		this.tickDay = localDay(this.now().getTime());
	}

	/**
	 * Starts the pollers, the machine sample and the minute tick; writes yesterday's worklog when it is missing. Today's
	 * activity leaves the worklog so far for later (`refreshActivity({ worklog: true })` once today's sessions were read
	 * in steps, CO-11).
	 */
	start(): void {
		this.catchUpWorklog();
		// Nothing thrown in a tick may escape the timer: that would end the service (CO-1).
		this.timers.push(setInterval(() => this.guard("tick", () => this.tick()), MINUTE));
		this.refreshActivity({ worklog: false });
		// `frontier.autostart` and `frontier.dashboard.display` went with the app (0557): said once at each start.
		try {
			for (const gone of goneSettings(readSettings())) this.hooks.notify(gone, "warning");
		} catch {
			// A malformed jocasta.yaml is said where its settings are read.
		}
		if (this.options.pollers === false) return;
		// The first CPU figure needs two readings; the first sample comes a moment after the baseline.
		this.cpuBefore = cpuTimes();
		this.timers.push(setTimeout(() => void this.sample(), 2_000));
		this.timers.push(setInterval(() => void this.sample(), MACHINE_MS));
		void this.readQuota();
		this.timers.push(setInterval(() => void this.readQuota(), QUOTA_MINUTES * MINUTE));
		void this.refreshLeftovers();
		this.timers.push(setInterval(() => void this.refreshLeftovers(), LEFTOVER_MINUTES * MINUTE));
		let settings: Record<string, unknown>;
		try {
			settings = readSettings();
		} catch {
			// A jocasta.yaml that isn't YAML turns off all its settings, the pollers and weather with them; the service
			// said it at start (0584).
			return;
		}
		// Each poller reads only its own keys: a bad value turns that poller alone off (0584).
		const poller = <K extends keyof PollerSettings>(name: K): PollerSettings[K] | undefined => {
			try {
				return readPoller(settings, name);
			} catch (error) {
				this.reportOnce(name, (error as Error).message, said(error));
				return undefined;
			}
		};
		const every = (name: PollerName, minutes: number) => {
			void this.poll(name);
			this.timers.push(setInterval(() => void this.poll(name), minutes * MINUTE));
		};
		const github = poller("github");
		if (github) every("github", github.everyMinutes);
		const azure = poller("azureDevOps");
		if (azure && azure.orgs.length > 0) every("azureDevOps", azure.everyMinutes);
		const calendar = poller("calendar");
		if (calendar && this.secret()) every("calendar", calendar.everyMinutes);
		const news = poller("news");
		if (news && news.feeds.length > 0) every("news", news.everyMinutes);
		// Reads `dashboard.weather` each round: hidden until set, picked up within 30 minutes once he sets it.
		every("weather", WEATHER_MINUTES);
	}

	/**
	 * Stops every timer and ends the poller rounds under way, awaiting them before its database closes (the service
	 * closes the catalog after this, F7); resolves once a worklog-so-far pass and a worklog being written let go of
	 * their project folders (git) and wrote nothing more.
	 */
	async stop(): Promise<void> {
		this.stopped = true;
		for (const timer of this.timers.splice(0)) clearInterval(timer);
		this.halt.abort();
		// With none under way, the database closes before `stop()` first yields.
		if (this.rounds.size > 0) await Promise.allSettled(this.rounds.values());
		this.me.close();
		await this.worklogSoFar;
		await this.worklogWriting;
	}

	private secret(): string | undefined {
		return (this.options.calendarSecret ?? calendarSecret)();
	}

	/** Quiet hours now (0298); a malformed setting is reported once and counts as none. */
	quiet(): boolean {
		return isQuiet(this.quietHours(), this.now());
	}

	/** `service.quietHours`, read live; a malformed value is reported once and counts as none. */
	private quietHours(): QuietHours | undefined {
		try {
			const hours = readQuietHours(readSettings());
			this.lastError.delete("quietHours");
			return hours;
		} catch (error) {
			this.reportOnce("quietHours", (error as Error).message, said(error));
			return undefined;
		}
	}

	/**
	 * A failure is shown as a toast and logged once until it changes or clears, not every round. A settings problem
	 * the service said at start (`said`) is only remembered, so it is said once (0584).
	 */
	private reportOnce(key: string, message: string, said = false): void {
		if (this.lastError.get(key) === message) return;
		this.lastError.set(key, message);
		if (said) return;
		log("warn", "day", message);
		this.hooks.notify(message, "warning");
	}

	private guard(key: string, work: () => unknown): void {
		try {
			work();
			this.lastError.delete(key);
		} catch (error) {
			this.reportOnce(key, `${key}: ${(error as Error).message}`);
		}
	}

	/**
	 * One round of a poller; an overlapping round is skipped, and none starts once the day stopped. News and weather
	 * only feed the dashboard (0429): a failure is reported once and shown in its section, and the last good list stays.
	 */
	poll(name: PollerName): Promise<void> {
		if (this.stopped || this.rounds.has(name)) return Promise.resolve();
		const round = this.round(name).finally(() => this.rounds.delete(name));
		this.rounds.set(name, round);
		return round;
	}

	private async round(name: PollerName): Promise<void> {
		try {
			if (name === "github" || name === "azureDevOps") {
				const round =
					name === "github"
						? await pollGitHub(this.run)
						: await pollAzure(this.run, readPoller(readSettings(), "azureDevOps").orgs);
				this.ingest(name === "github" ? "github" : "azure", name === "github" ? GITHUB_KINDS : AZURE_KINDS, round);
				// A part of the round that failed (one org or project) is said once; the rest was taken (SHR-8).
				if (round.failures.length > 0) {
					this.reportOnce(name, `Poller ${name} failed: ${round.failures.join("; ")}`);
					if (!this.stopped) this.pushToday();
					return;
				}
			} else if (name === "calendar") {
				const url = this.secret();
				if (!url) throw new Error("calendar: no secret iCal URL; `jocasta setup` saves it");
				this.meetings = await pollCalendar(url, this.now(), this.fetchUrl);
				this.meetingsSoon();
			} else if (name === "news") {
				this.news = await pollNews(readPoller(readSettings(), "news").feeds, this.fetchUrl);
				this.newsError = null;
			} else await this.pollWeather();
			this.lastError.delete(name);
			if (!this.stopped) this.pushToday();
		} catch (error) {
			// A round the stop ended says nothing.
			if (this.stopped) return;
			const message = (error as Error).message;
			if (name === "news") this.newsError = message;
			if (name === "weather") this.weatherError = message;
			this.reportOnce(
				name,
				name === "news" || name === "weather" ? message : `Poller ${name} failed: ${message}`,
				said(error),
			);
		} finally {
			if (!this.stopped) this.pushDashboard();
		}
	}

	/** The weather at `dashboard.weather` (0429); none while it is unset. */
	private async pollWeather(): Promise<void> {
		const query = readWeatherPlace(readSettings());
		if (!query) {
			this.place = undefined;
			this.weather = null;
			this.weatherError = null;
			return;
		}
		if (this.place?.query !== query) this.place = { query, place: await findPlace(query, this.fetchUrl) };
		this.weather = await pollWeather(this.place.place, this.fetchUrl);
		this.weatherError = null;
	}

	/** CPU, RAM and disk, and whether the services answer (0429). */
	async sample(): Promise<void> {
		if (this.sampling) return;
		this.sampling = true;
		try {
			const now = cpuTimes();
			this.guard("machine", () => {
				this.machine = sampleMachine(this.cpuBefore ?? now, now, dataDir());
			});
			this.cpuBefore = now;
			try {
				this.services = await this.hooks.services();
				this.lastError.delete("services");
			} catch (error) {
				this.reportOnce("services", `services: ${(error as Error).message}`);
			}
			this.pushDashboard();
		} finally {
			this.sampling = false;
		}
	}

	/**
	 * The subscription windows from `omp usage --json` (0456), for the HUD and the rail; one toast when a window passes
	 * `QUOTA_WARN`, again only after it went back under. A failure (omp missing, no login) is logged once, never a toast:
	 * the windows then stay empty.
	 */
	async readQuota(): Promise<void> {
		if (this.quotaReading) return;
		this.quotaReading = true;
		try {
			this.quotaWindows = readQuota(await (this.options.usage ?? ompUsage)());
			this.lastError.delete("quota");
		} catch (error) {
			const message = `omp usage: ${(error as Error).message}`;
			if (this.lastError.get("quota") !== message) log("warn", "day", message);
			this.lastError.set("quota", message);
			return;
		} finally {
			this.quotaReading = false;
		}
		for (const q of this.quotaWindows) {
			if (!quotaHigh(q)) this.quotaWarned.delete(q.window);
			else if (!this.quotaWarned.has(q.window)) {
				this.quotaWarned.add(q.window);
				this.hooks.send({
					type: "toast",
					reason: "event",
					level: "warning",
					priority: "urgent",
					text: quotaWarning(q),
				});
			}
		}
	}

	/** The subscription windows (0456): 5h then 7d, those omp reported. */
	quota(): QuotaWindow[] {
		return this.quotaWindows;
	}

	/** Looks for thread worktrees left on disk (0456), for the dashboard; one look at a time. */
	refreshLeftovers(): Promise<void> {
		this.leftoversReading ??= (async () => {
			try {
				this.leftoverList = await this.hooks.leftovers();
				this.lastError.delete("leftovers");
				this.pushDashboard();
			} catch (error) {
				this.reportOnce("leftovers", `Leftover worktrees: ${(error as Error).message}`);
			} finally {
				this.leftoversReading = undefined;
			}
		})();
		return this.leftoversReading;
	}

	/**
	 * The weekly big-picture review (0470): from Monday 10:00 on, outside quiet hours, once per project and week, each
	 * project's review runs; one with merges in the past 7 days ends in a `weekly-review` event. The event's row in the
	 * queue marks the week done across restarts; a result that came during quiet hours is delivered after them.
	 */
	private weeklyReviews(quiet: boolean): void {
		if (quiet) return;
		for (const row of this.queue.open()) {
			if (row.kind !== "weekly-review" || row.raised_at !== null) continue;
			this.deliver(row);
			this.queue.resolve(row.source_id);
		}
		const now = this.now();
		const back = (now.getDay() - WEEKLY_REVIEW_DAY + 7) % 7;
		if (back === 0 && now.getHours() < WEEKLY_REVIEW_HOUR) return;
		const week = localDay(new Date(now.getFullYear(), now.getMonth(), now.getDate() - back).getTime());
		for (const project of this.hooks.projects()) {
			const key = `${week}|${project.id}`;
			const sourceId = `weekly-review:${project.id}:${week}`;
			if (this.weeklyDone.has(key) || this.queue.bySource(sourceId)) continue;
			this.weeklyDone.add(key);
			void this.weeklyReview(project, sourceId);
		}
	}

	/** One project's weekly review; a failure is said once and the week isn't tried again until a restart. */
	private async weeklyReview(project: Project, sourceId: string): Promise<void> {
		try {
			const line = (await this.hooks.weeklyReview(project))?.trim();
			if (!line || this.stopped) return;
			this.raise({
				sourceId,
				source: "thread",
				kind: "weekly-review",
				priority: "normal",
				text: `Weekly review: ${project.name} — ${line}`,
				projectId: project.id,
			});
			// Delivered once, never reminded of; in quiet hours it stays open until they end.
			if (this.queue.bySource(sourceId)?.raised_at) this.queue.resolve(sourceId);
		} catch (error) {
			this.reportOnce(`weekly review ${project.id}`, `Weekly review of ${project.name}: ${(error as Error).message}`);
		}
	}

	/**
	 * A thread's `[event]` line held in quiet hours (0298), kept as a queue row so a restart keeps it too; sent to the
	 * boss pane once they end (`deliverHeld`).
	 */
	hold(text: string): void {
		const at = this.now().toISOString();
		this.queue.add({ sourceId: `thread:held:${at}|${text}`, source: "thread", kind: "held", priority: "normal", text });
	}

	/**
	 * Outside quiet hours: the thread lines held during them go to the boss pane as `[event]` lines, oldest first (0405;
	 * as 0470's weekly review waits for their end). Only the boss line: their toast showed when they happened, and
	 * today's activity listed them then.
	 */
	private deliverHeld(): void {
		for (const row of this.queue.open()) {
			if (row.kind !== "held") continue;
			this.hooks.alert(row.text, { priority: "normal" });
			this.queue.resolve(row.source_id);
		}
	}

	/**
	 * A poller's round: what went away from the part it saw is resolved; what is new is delivered. Nothing once the day
	 * stopped.
	 */
	private ingest(source: EventSource, kinds: readonly string[], round: PollRound): void {
		if (this.stopped) return;
		for (const row of this.queue.sync(source, kinds, round.events, round.observed)) this.deliver(row);
	}

	/** Adds an event and delivers it when new. */
	raise(event: NewEvent): EventRow | undefined {
		const row = this.queue.add(event);
		if (row) this.deliver(row);
		return row;
	}

	/**
	 * Delivery (0299, 0405, 0407): each delivered event is an `[event]` line for the boss pane and a notification
	 * (through herdr, 0554); low goes only to `today`. In quiet hours only urgent goes through; the rest waits for the
	 * morning `today` (0298). The one reminder of an event still open (0303) reads `Still open: <line>` (UX-05).
	 */
	private deliver(row: EventRow, reminder = false): void {
		if (row.priority === "low") return;
		if (row.priority === "normal" && this.quiet()) return;
		const priority = row.priority;
		const text = reminder ? `Still open: ${row.text}` : row.text;
		const dash = row.text.indexOf(" — ");
		// Poller and calendar lines are `<kind> — <what they said>`, a blocked thread's `<project>/<title> — <its
		// question>`; reminders are his, disk is Jocasta's.
		const tail = dash >= 0 ? row.text.slice(dash + 3) : "";
		const outside =
			OUTSIDE_KINDS.includes(row.kind) && dash >= 0 && !(row.kind === "blocked" && OWN_TAILS.includes(tail.trim()))
				? [tail]
				: [];
		this.hooks.alert(text, {
			priority,
			...(row.url ? { url: row.url } : {}),
			...(reminder ? { reminder: true } : {}),
			...(outside.length > 0 ? { outside } : {}),
		});
		this.hooks.send({
			type: "toast",
			reason: "event",
			level: priority === "urgent" ? "warning" : "info",
			text,
			priority,
			...(row.thread_id ? { paneId: row.thread_id } : {}),
			...(row.url ? { url: row.url } : {}),
		});
		if (reminder) this.queue.markReminded(row.id);
		else this.queue.markRaised(row.id);
		this.refreshActivity();
		this.pushDashboard();
	}

	/**
	 * A thread pane waiting on him (0298: urgent). The service already delivered it; the queue keeps it for the one
	 * reminder (0303) until the pane moves on.
	 */
	threadBlocked(paneId: string, text: string, projectId: string | null): void {
		const row = this.queue.add({
			sourceId: `thread:blocked:${paneId}`,
			source: "thread",
			kind: "blocked",
			priority: "urgent",
			text,
			threadId: paneId,
			...(projectId ? { projectId } : {}),
		});
		if (row) this.queue.markRaised(row.id);
	}

	threadUnblocked(paneId: string): void {
		this.queue.resolve(`thread:blocked:${paneId}`);
	}

	/**
	 * Every minute: meetings starting in 5 minutes, due reminders, disk space, the one reminder of unresolved events
	 * (0303), the thread lines held in quiet hours once they end, the day's-end worklog, today's activity, and the open
	 * `today` view.
	 */
	tick(): void {
		// A tick on a new day (the machine may have slept through the day's end): yesterday's worklog, if missing.
		const day = localDay(this.now().getTime());
		if (day !== this.tickDay) {
			this.tickDay = day;
			this.catchUpWorklog();
		}
		const quiet = this.quiet();
		this.meetingsSoon();
		for (const item of this.inbox.due()) {
			this.raise({
				sourceId: `inbox:${item.id}`,
				source: "inbox",
				kind: "reminder",
				priority: "normal",
				text: reminderLine(item.text, new Date(item.due_at as string)),
			});
		}
		this.guard("disk", () => this.checkDisk());
		for (const row of this.queue.dueReminders(REMINDER_AFTER_MS)) {
			if (row.priority === "normal" && quiet) continue;
			this.deliver(row, true);
		}
		if (!quiet) this.guard("held lines", () => this.deliverHeld());
		this.endOfDay(quiet);
		this.guard("weekly review", () => this.weeklyReviews(quiet));
		this.refreshActivity();
		this.pushToday();
		this.pushDashboard();
	}

	private meetingsSoon(): void {
		const now = this.now().getTime();
		for (const row of this.queue.open()) {
			if (row.kind !== "meeting-soon") continue;
			const start = Number(row.source_id.split("|").at(-1));
			if (!(start > now)) this.queue.resolve(row.source_id);
		}
		for (const m of this.meetings) {
			const until = m.start.getTime() - now;
			if (m.allDay || until <= 0 || until > MEETING_SOON_MS) continue;
			this.raise({
				sourceId: `calendar:soon:${m.uid}|${m.start.getTime()}`,
				source: "calendar",
				kind: "meeting-soon",
				priority: "urgent",
				text: `Meeting in ${Math.ceil(until / MINUTE)} min — ${m.title}`,
				...(m.url ? { url: m.url } : {}),
			});
		}
	}

	private checkDisk(): void {
		const free = (this.options.diskFree ?? diskFreeOfData)();
		const sourceId = "machine:disk";
		if (free >= DISK_LOW) {
			this.queue.resolve(sourceId);
			return;
		}
		this.raise({
			sourceId,
			source: "machine",
			kind: "disk",
			priority: "urgent",
			text: `Disk almost full — ${Math.round(free * 100)}% free`,
		});
	}

	/** The worklog at day's end (0302): when quiet hours start, or at 23:00 without them; once per day. */
	private endOfDay(quiet: boolean): void {
		const now = this.now();
		const day = localDay(now.getTime());
		if (this.worklogDay === day) return;
		const hours = this.quietHours();
		// Quiet hours that start after midnight belong to the next morning, so the day ends at 23:00 instead.
		const ended = hours && hours.from >= 12 * 60 ? quiet && now.getHours() >= 12 : now.getHours() >= WORKLOG_HOUR;
		if (!ended) return;
		this.worklogDay = day;
		this.queueWorklog(day);
	}

	/**
	 * Yesterday's worklog when it is missing (0302): at start, and at the first tick of a new day, which catches a day
	 * whose end the machine slept through (SRV-B-5). The calendar's day before, not 24 hours back: right after a
	 * spring-forward day those differ (CO-17).
	 */
	private catchUpWorklog(): void {
		const yesterday = localDay(dayStart(localDay(this.now().getTime()), -1));
		if (this.worklogDay === yesterday || existsSync(worklogPath(yesterday))) return;
		this.worklogDay = yesterday;
		this.queueWorklog(yesterday);
	}

	/** Writes `day`'s worklog after any one being written. */
	private queueWorklog(day: string): void {
		this.worklogWriting = this.worklogWriting.then(() => this.writeWorklogFor(day));
	}

	/** Writes a day's worklog (0302) once its sessions were read in steps; a failure is said once. */
	private async writeWorklogFor(day: string): Promise<void> {
		try {
			await this.hooks.prepareSessions(day);
		} catch {
			// The worklog then reads what it needs itself.
		}
		if (this.stopped) return;
		try {
			const byProject = this.hooks.sessionsByProject(day);
			await writeWorklog(
				this.hooks.projects(),
				day,
				(p) => byProject.get(p.id) ?? [],
				this.gitPlaces,
				() => this.stopped,
			);
			this.lastError.delete("worklog");
		} catch (error) {
			this.reportOnce("worklog", `worklog: ${(error as Error).message}`);
		}
	}

	// ---- the dashboard's third column (0429) ----

	/**
	 * Today's activity (0429): the latest delivered `[event]` lines, the worklog so far, what Graphiti's model calls
	 * cost today (0582) and the compaction and handoff summaries saved today, each with its project. The worklog so far
	 * is recomputed every 10 minutes off the event loop (it reads every project's sessions and git log, CO-12), and only
	 * once today's sessions were read in steps: `worklog: true` allows it from now on, `false` holds it (CO-11).
	 */
	refreshActivity({ worklog }: { worklog?: boolean } = {}): void {
		if (worklog !== undefined) this.worklogReady = worklog;
		this.guard("activity", () => {
			const now = this.now();
			const midnight = new Date(midnightOf(now.getTime()));
			this.graphToday = graphSpentOn(localDay(now.getTime()));
			const stored = this.catalog
				.query<{ text: string; raised_at: string }, [string, number]>(
					"SELECT text, raised_at FROM events WHERE raised_at >= ? ORDER BY raised_at DESC, id DESC LIMIT ?",
				)
				.all(midnight.toISOString(), ACTIVITY_ROWS)
				.map((row) => ({ at: row.raised_at, text: row.text }));
			// The service's own thread lines (done and reviewed, closed, PRs) aren't queue rows (N-4).
			this.serviceEvents = this.serviceEvents.filter((e) => e.at >= midnight.toISOString());
			const events = [...stored, ...this.serviceEvents]
				.sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0))
				.slice(0, ACTIVITY_ROWS);
			const summaries = this.catalog
				.query<{ cwd: string; at: number; summary: string }, [number, number]>(
					"SELECT cwd, at, summary FROM summaries WHERE at >= ? ORDER BY at DESC LIMIT ?",
				)
				.all(midnight.getTime(), ACTIVITY_ROWS)
				.map((row) => {
					// A thread's summary is in its worktree: the service knows its project (N-4).
					const project = this.hooks.projectName(row.cwd);
					// The first line of content: a heading such as `## Goal` names a section, not what happened.
					const first = row.summary
						.split("\n")
						.filter((line) => !/^\s*#/.test(line))
						.map((line) => line.replace(/^[>*\-\s]+/, "").trim())
						.find(Boolean);
					return { at: new Date(row.at).toISOString(), project, text: (first ?? "").slice(0, 200) };
				});
			this.activity = { ...this.activity, events, summaries };
			const due = now.getTime() - this.worklogSoFarAt >= WORKLOG_SO_FAR_MS || now.getTime() < this.worklogSoFarAt;
			if (this.worklogReady && due && !this.worklogSoFar) {
				this.worklogSoFarAt = now.getTime();
				this.worklogSoFar = this.passWorklog(localDay(now.getTime()));
			}
		});
	}

	/**
	 * The worklog so far: today's sessions listed once and grouped by project, each project's git email and origin
	 * kept for the day, and `git log` run off the event loop, one project after another (CO-12).
	 */
	private async passWorklog(day: string): Promise<void> {
		try {
			const projects = this.hooks.projects();
			const byProject = this.hooks.sessionsByProject(day);
			const lines: string[] = [];
			for (const project of projects) {
				if (this.stopped) return;
				const mine = await commitsAsync(project, day, await this.gitPlaces.of(project, day));
				const found = projectDay(day, byProject.get(project.id) ?? [], mine);
				if (found) lines.push(projectDayLine(project.name, found));
			}
			if (this.stopped) return;
			this.activity = { ...this.activity, worklog: lines };
			this.lastError.delete("worklog so far");
			this.pushDashboard();
		} catch (error) {
			this.reportOnce("worklog so far", `worklog so far: ${(error as Error).message}`);
		} finally {
			this.worklogSoFar = undefined;
		}
	}

	dashboard(): Dashboard {
		// The graph's share of "$ today" (0582) is named under the worklog so far, once it spent anything.
		const graph = this.graphToday > 0 ? [`graph memory · $${this.graphToday.toFixed(2)}`] : [];
		return {
			news: this.news,
			newsError: this.newsError,
			weather: this.weather,
			weatherError: this.weatherError,
			machine: this.machine,
			services: this.services,
			activity: { ...this.activity, worklog: [...this.activity.worklog, ...graph] },
			leftovers: this.leftoverList,
		};
	}

	/** One of the service's thread lines (done and reviewed, closed, a PR), delivered now: into today's activity. */
	recordEvent(text: string): void {
		this.serviceEvents = [{ at: this.now().toISOString(), text }, ...this.serviceEvents].slice(0, ACTIVITY_ROWS);
		this.refreshActivity();
		this.pushDashboard();
	}

	/** Pushes the dashboard to the page when it changed. */
	pushDashboard(): void {
		const dashboard = this.dashboard();
		const json = JSON.stringify(dashboard);
		if (json === this.dashboardSent) return;
		this.dashboardSent = json;
		this.hooks.send({ type: "dashboard", dashboard });
	}

	// ---- today (0300) ----

	today(): Today {
		const now = this.now();
		const today = localDay(now.getTime());
		const start = new Date(dayStart(today));
		const end = new Date(dayStart(today, 1));
		const yesterday = dayStart(today, -1);
		const meetings = this.meetings.filter((m) => m.start < end && m.end > start);
		const entry = (time: string, m: Meeting, ends?: string) => ({
			time,
			title: m.title,
			...(m.url ? { url: m.url } : {}),
			...(ends ? { end: ends } : {}),
		});
		// A timed meeting carries its end (C9, 0581): `today` keeps a started one as "now" until then.
		const calendar = [
			...meetings.filter((m) => m.allDay).map((m) => entry("all day", m)),
			...meetings.filter((m) => !m.allDay && m.start >= start).map((m) => entry(hhmm(m.start), m, hhmm(m.end))),
		];
		const needsYou: Today["needsYou"] = [];
		for (const row of this.queue.open()) {
			if (row.kind !== "review" && row.kind !== "pipeline" && row.kind !== "disk") continue;
			needsYou.push({ kind: row.kind, text: row.text, ...(row.url ? { url: row.url } : {}) });
		}
		const threads = this.hooks.threads();
		const rail = ({ paneId, title, project, line, overlaps }: DayThread): RailItem => ({
			paneId,
			title,
			project,
			line,
			...(overlaps?.length ? { overlaps } : {}),
		});
		const thread = (t: DayThread) => ({
			paneId: t.paneId,
			...(t.project ? { project: t.project } : {}),
			...(t.overlaps?.length ? { overlaps: t.overlaps } : {}),
		});
		// A blocked thread says since when it waits (UX-4, 0581): its last state change.
		for (const t of threads.filter((t) => t.state === "blocked")) {
			needsYou.push({
				kind: "blocked",
				text: `${t.title} — ${t.line || "awaiting you"}`,
				since: new Date(t.changedAt).toISOString(),
				...thread(t),
			});
		}
		// Reviewed and not finished (0456): they wait on him, oldest first, for as long as it takes.
		const reviewed = threads
			.filter((t) => t.state === "done" && t.reviewedAt !== undefined)
			.sort((a, b) => (a.reviewedAt as number) - (b.reviewedAt as number));
		for (const t of reviewed) {
			needsYou.push({
				kind: "reviewed",
				text: `${t.title} — ${t.line || "reviewed"}`,
				since: new Date(t.reviewedAt as number).toISOString(),
				...thread(t),
			});
		}
		// A reminder awaits him once it is due (N-5), not from the morning on; one filed for a project shows its name.
		const projects = this.hooks.projects();
		const projectName = new Map(projects.map((p) => [p.id, p.name]));
		for (const item of this.inbox.reminders(start, end)) {
			const due = new Date(item.due_at as string);
			if (due > now) continue;
			const project = item.project_id ? projectName.get(item.project_id) : undefined;
			needsYou.push({ kind: "reminder", text: item.text, time: hhmm(due), ...(project ? { project } : {}) });
		}
		const next = projects
			.flatMap((p) => this.inbox.notes(p.id).map((n) => ({ id: n.id, text: n.text, project: p.name })))
			.sort((a, b) => a.id - b.id);
		return {
			date: localDay(now.getTime()),
			calendar,
			needsYou,
			landed: [
				...threads
					.filter((t) => t.state === "done" && t.reviewedAt === undefined && t.changedAt >= yesterday)
					.map(rail),
				...this.finishedSince(yesterday, projectName),
			],
			// Working and idle threads with what `today` shows of them (0555): state, since when, cost and context.
			underway: threads
				.filter((t) => t.state === "working" || t.state === "idle")
				.map((t) => ({
					...rail(t),
					state: t.state,
					since: t.changedAt,
					...(t.cost !== undefined ? { cost: t.cost } : {}),
					...(t.context !== undefined ? { context: t.context } : {}),
				})),
			next,
		};
	}

	/**
	 * A thread that landed (0263, 0300): squash-merged into trunk or sent as a PR, its pane gone. Kept as a low event
	 * (0299: only `today`), so today's Landed lists it since yesterday, across restarts; `line` says how it landed.
	 */
	landed(paneId: string, title: string, projectId: string, line: string): void {
		const sourceId = `thread:landed:${paneId}`;
		this.queue.add({
			sourceId,
			source: "thread",
			kind: "landed",
			priority: "low",
			text: `${title} — ${line}`,
			projectId,
		});
		this.queue.resolve(sourceId);
		this.pushToday();
	}

	/** The threads `landed` recorded since `since` (epoch ms), oldest first, as Landed rows without a pane. */
	private finishedSince(since: number, projectName: ReadonlyMap<string, string>): Today["landed"] {
		return this.catalog
			.query<Pick<EventRow, "text" | "project_id">, [string]>(
				"SELECT text, project_id FROM events WHERE kind = 'landed' AND created_at >= ? ORDER BY created_at, id",
			)
			.all(new Date(since).toISOString())
			.map((row) => {
				// The line is Jocasta's own words, without " — "; a title may hold one.
				const cut = row.text.lastIndexOf(" — ");
				return {
					title: cut < 0 ? row.text : row.text.slice(0, cut),
					project: row.project_id ? (projectName.get(row.project_id) ?? null) : null,
					line: cut < 0 ? "" : row.text.slice(cut + 3),
				};
			});
	}

	/** Pushes `today` to the page when it changed; `today` is one the caller built already. */
	pushToday(today: Today = this.today()): void {
		const json = JSON.stringify(today);
		if (json === this.todaySent) return;
		this.todaySent = json;
		this.hooks.send({ type: "today", today });
	}

	/** `today` as text for the boss (0300's layout). */
	todayText(): string {
		const t = this.today();
		const label = dayLabel(t.date);
		const row = (name: string, items: string[]) =>
			`${name.padEnd(12)} ${items.length > 0 ? items.join(" · ") : "nothing"}`;
		const now = this.now();
		const thread = (r: Omit<RailItem, "paneId">) =>
			`${r.project ? `${r.project}/` : ""}${r.title}${r.overlaps ? ` (overlaps ${r.overlaps.join(", ")})` : ""}`;
		return [
			`TODAY · ${label}`,
			row(
				"Calendar",
				t.calendar.map((c) => `${c.time} ${c.title}`),
			),
			row(
				"Awaiting you",
				t.needsYou.map((n) => {
					const said = n.kind === "reminder" ? `${n.time} ${n.text}` : n.project ? `${n.project}/${n.text}` : n.text;
					const waited = n.since ? ` (waiting ${waitLabel(now.getTime() - Date.parse(n.since))})` : "";
					const overlaps = n.overlaps ? ` (overlaps ${n.overlaps.join(", ")})` : "";
					return `${said}${waited}${overlaps}`;
				}),
			),
			// A merged or PR'd thread says how it landed; one with a pane is done and awaits its review.
			row(
				"Landed",
				t.landed.map((r) => (r.paneId === undefined ? `${thread(r)} (${r.line})` : thread(r))),
			),
			row("Underway", t.underway.map(thread)),
			row(
				"Next",
				t.next.map((n) => `${n.project ? `${n.project}: ` : ""}${n.text} (#${n.id})`),
			),
		].join("\n");
	}

	// ---- inbox (0302) ----

	/** Files a note into a project's next work. */
	note(text: string, project: Project): InboxItem {
		const item = this.inbox.add(text.trim(), { projectId: project.id });
		this.pushToday();
		return item;
	}

	/** Files a reminder into `today` at a time; it is delivered as a normal event when due. */
	remind(text: string, at: string, project?: Project): InboxItem {
		const body = text.trim();
		if (!body) throw new Error("a reminder needs its text");
		const due = parseWhen(at, this.now());
		const item = this.inbox.add(body, { dueAt: due, ...(project ? { projectId: project.id } : {}) });
		this.pushToday();
		return item;
	}

	done(id: number): boolean {
		const done = this.inbox.done(id);
		if (done) this.queue.resolve(`inbox:${id}`);
		this.pushToday();
		return done;
	}

	/**
	 * The worklog on request (0302): today's unless a day is given; every project's unless some are given. The day's
	 * sessions are read in steps first (CO-11); a day not on the calendar (2026-02-30) is refused (CO-22).
	 */
	async worklog(
		day?: string,
		projects: readonly Project[] = this.hooks.projects(),
	): Promise<{ path: string; projects: string[] }> {
		const wanted = day?.trim() || localDay(this.now().getTime());
		const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(wanted);
		if (!parts) throw new Error(`"${day}" is not a day: use YYYY-MM-DD`);
		const [y, m, d] = parts.slice(1).map(Number) as [number, number, number];
		const date = new Date(y, m - 1, d);
		if (date.getFullYear() !== y || date.getMonth() !== m - 1 || date.getDate() !== d) {
			throw new Error(`no such day: ${wanted}`);
		}
		await this.hooks.prepareSessions(wanted);
		const byProject = this.hooks.sessionsByProject(wanted);
		return writeWorklog(projects, wanted, (p) => byProject.get(p.id) ?? [], this.gitPlaces);
	}
}
