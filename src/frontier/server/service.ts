/**
 * The Jocasta service (0392, 0552): herdr runs the panes; this service keeps the boss pane (0404), starts, watches and
 * finishes threads (0388, 0412, 0426), serves the `jocasta` MCP tools (`/api/mcp/<tool>`), turns threads' changes and
 * his day's events into `[event]` lines and Windows notifications through herdr (0405, 0407, 0413, 0554), reports the
 * sidebar's tokens and the HUD (0402, 0403), opens hunk beside a thread that finished a task and has it test and
 * review its work (0394, 0406, 0425), then sends `/handoff` (0400), and feeds omp's compaction summaries to Graphiti
 * (0389). No Jocasta code runs inside omp (0386).
 *
 * This file wires the parts (SVC-15) and holds the start, the stop and the API server.ts, plugin.ts and run.ts use:
 * `thread.ts` (a thread and the registry), `places.ts` (git places, projects, trunks), `worktrees.ts` (worktrees on
 * disk), `overlay.ts` (the omp overlay), `events.ts` (`[event]` lines, toasts, notifications), `allow.ts` (his
 * Allow), `pane-launch.ts` (starting omp in a pane), `omp-processes.ts` (the panes' omps), `thread-lifecycle.ts`
 * (state changes), `review-flow.ts` (the review step), `finish.ts` (`thread_finish`), `thread-start.ts`,
 * `weekly-review.ts`, `status.ts` (tokens, HUD, `today` rows, `thread_status`), `tools.ts` (the tools and CLI
 * commands), `keys.ts` (his keys) and `refresh.ts` (the tick).
 */
import type { Database } from "bun:sqlite";
import { basename, join } from "node:path";
import { dayStart, midnightOf } from "../../shared/day.ts";
import { UserError } from "../../shared/errors.ts";
import { hostsOn, layerOn } from "../../shared/layer.ts";
import { openInBrowser, webLink } from "../../shared/link.ts";
import { log } from "../../shared/log.ts";
import type { GraphFact } from "../../shared/memory/graph/index.ts";
import { dataDir } from "../../shared/paths.ts";
import type { Project } from "../../shared/projects.ts";
import { settingsProblems } from "../../shared/settings.ts";
import type { ClientMessage, Hud, Leftover, ServerMessage, ServiceHealth, Today } from "../protocol.ts";
import { AllowGate, type Confirm } from "./allow.ts";
import { freshSession, LaunchedPanes } from "./boss.ts";
import { CheckRunner, checksDir } from "./checks.ts";
import type { CodeIndex } from "./code-index.ts";
import type { ServiceContext } from "./context.ts";
import { Day, type DayOptions } from "./day.ts";
import { clean, Events } from "./events.ts";
import { Finisher } from "./finish.ts";
import { type Herdr, type HerdrSnapshot, paneText } from "./herdr.ts";
import { Keys } from "./keys.ts";
import { OmpTracker } from "./omp-processes.ts";
import { PaneLauncher } from "./pane-launch.ts";
import { Places } from "./places.ts";
import { STT_TOGGLE_KEY } from "./ptt-hold.ts";
import { Refresher } from "./refresh.ts";
import { ReviewFlow } from "./review-flow.ts";
import { SessionIndex } from "./sessions.ts";
import { StatusViews } from "./status.ts";
import { ThreadRegistry } from "./thread.ts";
import { ThreadLifecycle } from "./thread-lifecycle.ts";
import { ThreadStarts } from "./thread-start.ts";
import { type Caller, type ToolReply, Tools } from "./tools.ts";
import { type Episode, startWatcher, type Watcher } from "./watcher.ts";
import { WeeklyReviews } from "./weekly-review.ts";
import type { WorklogSession } from "./worklog.ts";
import { ReviewedCommits, ThreadNumbers } from "./worktrees.ts";

/** How often panes, sidebar tokens and the HUD are refreshed. */
const REFRESH_MS = 5_000;
/** How often the thread panes' omp hosts are asked for their state (0547, 0561); a pipe answers in 0–5 ms. */
const HOST_POLL_MS = 1_000;
/** How long `stop` waits for tool calls in flight before closing the catalog under them (SVC-13). */
const CALLS_WAIT_MS = 25_000;

/** The knowledge graph as the service uses it (test seam). */
export interface Graph {
	search(projectId: string, query: string, limit: number): Promise<GraphFact[]>;
	post(projectId: string, episode: Episode): Promise<void>;
}

export interface ServiceOptions {
	herdr: Herdr;
	broadcast(message: ServerMessage): void;
	/** Whether Graphiti and whiteboard answer (0429), for the dashboard; herdr's the service checks itself. */
	services?(): Promise<ServiceHealth[]>;
	/**
	 * The herdr plugin's panes (0553): healed on every refresh with herdr's panes as read and the boss pane's id, so
	 * `today` is open beside the boss and the Agents view is in place. Absent in tests that don't need it.
	 */
	plugin?: { heal(snapshot: HerdrSnapshot, bossPaneId: string | undefined): Promise<void> };
	/**
	 * A whiteboard review by id (its `session_create` sessionId) on whiteboard's server, `GET /reviews-api/<id>?full=true`:
	 * its title, or null when there is none; throws when the server doesn't answer. Absent when whiteboard is off.
	 */
	findReview?(id: string): Promise<{ title: string } | null>;
	/** Opens a whiteboard review in his browser (the boss's `show`, 0552). Absent when whiteboard is off. */
	openReview?(id: string): void;
	/** Phase 8's pollers and clock (test seam). */
	day?: DayOptions;
	/** How omp starts: `["omp"]` by default (test and smoke seam). */
	omp?: string[];
	/** How hunk starts: the shipped or npm binary (`findHunk`) by default (test seam). */
	hunk?: string[];
	/** The loopback port of Jocasta's own hunk session daemon; `HUNK_PORT` by default (test seam). */
	hunkPort?: number;
	/** The boss pane's argv; `omp --continue --config <config>/boss.yml --config <config>/guard.yml` by default (0424, 0450). */
	bossCommand?: string[];
	/**
	 * A prompt file every project and thread pane appends with `--append-system-prompt`: the shipped
	 * `<config>/ponytail.md` when it exists (0421, 0427); `null` appends nothing (test seam).
	 */
	panePrompt?: string | null;
	/** How gh starts for `thread_finish(…, "pr")`: `["gh"]` by default (test seam). */
	gh?: string[];
	/** How long one gh call may take before it is ended; `GH_TIMEOUT_MS`, a minute (test seam). */
	ghTimeoutMs?: number;
	/** omp's sessions dir; `~/.omp/agent/sessions` by default. */
	sessionsDir?: string;
	/** The knowledge graph; none in tests. */
	graph?: Graph;
	/** How long `thread_start` waits for omp to come up in the new pane before sending the brief; `AGENT_WAIT_MS`, 20 s. */
	agentWaitMs?: number;
	/**
	 * How long `thread_start` waits for the pane to start working on the brief before sending one Enter;
	 * `SUBMIT_WAIT_MS`, 5 s.
	 */
	submitWaitMs?: number;
	/** How often panes, sidebar tokens and the HUD are refreshed; 5 s. */
	refreshMs?: number;
	/** How long the end of a thread pane's omp is waited for before its leftovers end (0490); 10 s (test seam). */
	leftoverWaitMs?: number;
	/** When leftovers are looked for again, for workers an exiting omp started (0490); 3 s later (test seam). */
	leftoverAgainMs?: number;
	/** The session watcher's scan interval; 30 s. */
	watcherIntervalMs?: number;
	/** mise's argv (test seam); `null` runs none. Default: the `mise` on PATH (0473, 0477). */
	mise?: string[] | null;
	/** How long the weekly big-picture review may run (0470); 30 minutes. */
	weeklyReviewMs?: number;
	/** `false` (test seam): the minute tick never starts a weekly review (0470), so no test spawns omp; the boss's tool still can. */
	weeklyReviews?: false;
	/**
	 * Asks him Allow or Deny (0484): a question through the service's question API (0519, 0552), with how it ended
	 * (0578: his answer, three wrong codes, the popup closed, another reader). `signal` aborts once the answer isn't
	 * wanted any more (the wait ran out, the service is stopping) and the question goes. Without it every ask counts as
	 * Deny.
	 */
	confirm?: Confirm;
	/** How long an ask waits for his answer; `CONFIRM_WAIT_MS` (test seam). */
	confirmWaitMs?: number;
	/** The boss pane's key (0484); a new random one per run by default (test seam). */
	bossKey?: string;
	/** The port of Jocasta's collab relay the panes' overlays name (0561); `RELAY_PORT` by default. */
	relayPort?: number;
	/**
	 * `collab.relayUrl` as the omp of `command` resolves it in `cwd` with `env` (0561); omp's own `omp config get` by
	 * default (test seam).
	 */
	relayUrlOf?(command: string[], cwd: string, env: Record<string, string>): Promise<string>;
	/** omp's host registry folder (`hosts.ts`); `~/.omp/run/collab-hosts` by default (test seam). */
	hostsDir?: string;
	/** How often the thread panes' omp hosts are asked for their state (0547, 0561); 1 s. */
	hostPollMs?: number;
	/**
	 * `false` (test seam): a task end without a `report_result` goes straight to its review, with no steer asking for
	 * the report first (0539).
	 */
	askReport?: false;
	/** The check runner (0542, 0559): `<data>/checks` by default (test seam). */
	checks?: CheckRunner;
	/** The code index behind `graph_search` (0562); none answers with facts only. */
	codeIndex?: CodeIndex;
	/** How long a plan-mode turn's end waits for the session to show its proposal; `PLAN_SETTLE_MS` (test seam). */
	planSettleMs?: number;
}

export class JocastaService {
	readonly day: Day;
	/** What the parts share (`context.ts`). */
	private readonly c: ServiceContext;
	private readonly refresher: Refresher;
	private readonly tools: Tools;
	private readonly keys: Keys;
	private watcher: Watcher | undefined;
	private unwatch: (() => void) | undefined;
	private timer: Timer | undefined;
	/** The thread panes' omp hosts are being asked for their state (`OmpTracker.poll`), every `hostPollMs`. */
	private hostTimer: Timer | undefined;
	/** The start under way or done (`stop` waits for it). */
	private starting: Promise<void> | undefined;
	private readonly found = Promise.withResolvers<void>();
	/**
	 * Resolves once the threads herdr already runs were found (the first `discover()`), or when the start failed or
	 * the stop began: a close waits for it before counting the threads at work (LC-9).
	 */
	readonly discovered = this.found.promise;

	constructor(
		readonly catalog: Database,
		private readonly options: ServiceOptions,
	) {
		const c = {
			catalog,
			options,
			herdr: options.herdr,
			sessions: new SessionIndex(options.sessionsDir),
			checks: options.checks ?? new CheckRunner({ dir: checksDir(dataDir()) }),
			launched: new LaunchedPanes(),
			threads: new ThreadRegistry(),
			layer: layerOn(),
			hosts: hostsOn(),
			calls: new Set<Promise<unknown>>(),
			finishing: new Map<string, "removing" | "removed">(),
			stopping: false,
			warmed: false,
			boss: undefined,
			warn: (error: unknown) => {
				if (c.stopping) return;
				log("warn", "service", error instanceof Error ? error.message : String(error));
			},
		} as unknown as ServiceContext;
		this.c = c;
		c.events = new Events(c, options.broadcast);
		c.allow = new AllowGate(options.confirm, options.confirmWaitMs);
		c.places = new Places(catalog, {
			problem: (text) => c.events.warning(text),
			gone: (id) => void options.codeIndex?.forget(id).catch((e) => c.warn(e)),
			synced: () => this.refresher.rediscover(),
		});
		c.numbers = new ThreadNumbers();
		c.reviewed = new ReviewedCommits();
		c.panes = new PaneLauncher(c);
		c.omps = new OmpTracker(c);
		c.status = new StatusViews(c);
		c.review = new ReviewFlow(c);
		c.finisher = new Finisher(c);
		c.lifecycle = new ThreadLifecycle(c);
		c.starts = new ThreadStarts(c);
		c.weekly = new WeeklyReviews(c);
		this.refresher = new Refresher(c);
		this.tools = new Tools(c);
		this.keys = new Keys(c, (run) => this.track(run));
		this.day = new Day(
			catalog,
			{
				send: (message) => c.events.send(message),
				alert: (text, details) => c.events.alert(text, details),
				notify: (text, level) => c.events.send({ type: "toast", reason: "event", level, text }),
				projects: () => c.places.listed(),
				threads: () => c.threads.all().map((t) => c.status.dayThread(t)),
				// A session last written before the day began holds none of its messages; the folder is listed once.
				sessionsByProject: (day) => {
					const byProject = new Map<string, WorklogSession[]>();
					for (const s of c.sessions.list(dayStart(day))) {
						const project = c.places.projectAt(s.cwd);
						if (!project) continue;
						const sessions = byProject.get(project.id) ?? [];
						sessions.push({ label: s.title ?? basename(s.cwd), times: c.sessions.timesOn(s.file, day) });
						byProject.set(project.id, sessions);
					}
					return byProject;
				},
				prepareSessions: (day) => c.sessions.prepare(day, () => c.stopping),
				projectName: (cwd) => c.places.projectAt(cwd)?.name ?? null,
				services: async () => {
					let herdr: ServiceHealth;
					try {
						await c.herdr.ping();
						herdr = { name: "herdr", state: "up" };
					} catch {
						herdr = { name: "herdr", state: "down", note: "not answering" };
					}
					return [...((await options.services?.()) ?? []), herdr];
				},
				leftovers: async () => this.leftovers(),
				weeklyReview: async (project) => (options.weeklyReviews === false ? null : this.weeklyReview(project)),
			},
			options.day,
		);
		c.day = this.day;
	}

	/**
	 * Starts: syncs projects from the watched roots, ensures the boss pane (0404), finds the threads herdr already
	 * runs, watches every Jocasta pane, starts his day and the session watcher, reads today's sessions without holding
	 * the event loop (the HUD's spend waits for it), then the refresh tick.
	 */
	start(): Promise<void> {
		// A start that fails or ends early still lets a waiting close count the threads (`discovered`).
		this.starting ??= this.startUp().finally(() => this.found.resolve());
		return this.starting;
	}

	private async startUp(): Promise<void> {
		const { c } = this;
		// The server answers /health before the catalog sync holds the event loop (PERF-7): bare `jocasta` polls it.
		const yielded = Promise.withResolvers<void>();
		setImmediate(yielded.resolve);
		await yielded.promise;
		await c.places.sync();
		// Each problem in jocasta.yaml is said once, at the start (0584, C7).
		for (const problem of settingsProblems()) {
			log("warn", "settings", problem);
			c.events.send({ type: "toast", reason: "event", level: "warning", text: `settings: ${problem}` });
		}
		// Only a session whose server this run started and that holds nothing of Jocasta's yet is new (0495).
		this.refresher.herdrDefault = c.herdr.startedServer && freshSession(await c.herdr.workspaces());
		await c.panes.ensureBoss({ adopt: true });
		if (c.stopping) return;
		await this.refresher.discover(await c.herdr.snapshot());
		this.found.resolve();
		this.unwatch = c.herdr.watch(
			() => [...(c.boss ? [c.boss.paneId] : []), ...c.threads.paneIds()],
			(event) => c.lifecycle.herdrState(event.paneId, event.state),
		);
		this.day.start();
		void import("../../setup/setup.ts")
			.then(({ setupToast }) => setupToast())
			.then((text) => {
				c.status.setupNotice = text;
			})
			.catch((error) => c.warn(error));
		const graph = this.options.graph;
		this.watcher = startWatcher({
			sessionsDir: c.sessions.dir,
			stateFile: join(dataDir(), "watcher.json"),
			db: this.catalog,
			projectFor: (cwd) => {
				const project = c.places.projectAt(cwd);
				return project ? { projectId: project.id } : undefined;
			},
			post: async (projectId, episode) => {
				if (!graph) throw new Error("no knowledge graph in this run");
				await graph.post(projectId, episode);
			},
			...(this.options.watcherIntervalMs ? { intervalMs: this.options.watcherIntervalMs } : {}),
			onError: (e) => c.warn(e),
		});
		await this.refresh();
		this.timer = setInterval(() => void this.refresh(), this.options.refreshMs ?? REFRESH_MS);
		// omp's own state for each thread pane (0547, 0561), between herdr's events and the refresh tick; never with
		// the host registry off (0576's `herdr` condition).
		if (c.hosts) {
			this.hostTimer = setInterval(() => void c.omps.poll(), this.options.hostPollMs ?? HOST_POLL_MS);
		}
		// The first read of a long session takes seconds: done in steps, so the page and herdr's events go on meanwhile.
		await c.sessions.warm(
			[
				...c.sessions.list(midnightOf(Date.now())).map((s) => s.file),
				...[c.boss?.sessionPath, ...c.threads.all().map((t) => t.sessionPath)].filter(
					(file): file is string => file !== undefined,
				),
			],
			() => c.stopping,
		);
		c.warmed = true;
		if (c.stopping) return;
		this.day.refreshActivity({ worklog: true });
		await this.refresh();
	}

	/**
	 * Stops the service; herdr's panes keep running (0392). The HUD says Jocasta is off and the sidebar's tokens are
	 * cleared (UX-13); tokens a herdr that doesn't answer keeps expire with their TTL.
	 */
	async stop(): Promise<void> {
		const { c } = this;
		c.stopping = true;
		this.found.resolve();
		c.allow.withdrawAll();
		try {
			// A start still running stops at its next step; what it set up is torn down below.
			await this.starting?.catch(() => {});
			// Tool calls in flight finish first (a squash mid-commit removes its thread), for a while (SVC-13).
			await Promise.race([Promise.allSettled([...c.calls]), Bun.sleep(CALLS_WAIT_MS)]);
			clearInterval(this.timer);
			clearInterval(this.hostTimer);
			await c.omps.polling();
			this.unwatch?.();
			this.watcher?.stop();
			await this.day.stop();
			await this.refresher.running();
			await c.lifecycle.settled();
			// A check still running ends with the service; nothing is stored for it.
			await c.checks.stop();
			await c.review.settled();
			await c.status.clear();
			// Notifications still waiting on herdr, and their lines in notifications.jsonl, before herdr closes.
			await c.events.settled();
		} finally {
			c.herdr.close();
			this.catalog.close();
		}
	}

	/**
	 * One refresh tick (`refresh.ts`); a call while one runs gets that one. Notifications herdr couldn't show for want
	 * of a client are sent again (0584).
	 */
	refresh(): Promise<void> {
		this.c.events.resendUnshown();
		return this.refresher.refresh();
	}

	/** Before Jocasta stops everything (0549): the threads at work are kept for `--restore` (`OmpTracker.pause`). */
	pauseWorking(): Promise<void> {
		return this.c.omps.pause();
	}

	/** Stop everything (0549): the omps kept for `--restore` end mid-turn (`OmpTracker.cutPaused`). */
	cutPausedTurns(): Promise<void> {
		return this.c.omps.cutPaused();
	}

	/** Stop everything's last step for omp (0547): the host entries of the omps Jocasta ended. */
	removeEndedHosts(): Promise<void> {
		return this.c.omps.removeEndedHosts();
	}

	/** Once herdr's server is gone (0486, 0490): what the thread panes' omps left ends. */
	endThreadLeftovers(): Promise<void> {
		return this.c.omps.endLeftovers();
	}

	/** Ends the hunk session daemon this service started (0434, 0486). */
	endHunkDaemon(): Promise<void> {
		return this.c.panes.endHunkDaemon();
	}

	/** One big-picture review of a project (0470), one at a time per project (SVC-12). */
	weeklyReview(project: Project): Promise<string | null> {
		return this.c.weekly.review(project);
	}

	/** Leftover worktrees (0456): thread checkouts no open thread holds. */
	leftovers(): Promise<Leftover[]> {
		return this.c.status.leftovers();
	}

	hud(today?: Today): Hud {
		return this.c.status.hud(today);
	}

	/** The threads closing Jocasta would cut off (0434, 0438): awaiting him, and working among the rest. */
	busyThreads(): { working: number; awaiting: number } {
		return this.c.status.busyThreads();
	}

	/** The boss pane's state, herdr's `done` as idle. */
	bossState(): "working" | "blocked" | "idle" | "unknown" {
		return this.c.events.bossState();
	}

	/** A toast as a Windows notification through herdr (0407, 0554), logged in `notifications.jsonl` (H2). */
	notify(toast: Extract<ServerMessage, { type: "toast" }>, options: { show?: boolean } = {}): Promise<void> {
		return this.c.events.notify(toast, options);
	}

	/**
	 * Push-to-talk (0556): omp's dictation toggle (`app.stt.toggle`, bound by setup) sent to the boss pane through herdr,
	 * whether or not it has the focus.
	 */
	async toggleDictation(): Promise<void> {
		if (!this.c.boss) throw new Error("the boss pane isn't open yet");
		await this.c.herdr.sendKeys(this.c.boss.paneId, [STT_TOGGLE_KEY]);
	}

	/**
	 * Runs one tool, CLI or key call (SVC-13): refused once Jocasta is closing, and counted while it runs, so the stop
	 * waits for it before closing the catalog under it.
	 */
	private track<T>(run: () => Promise<T>): Promise<T> {
		if (this.c.stopping) return Promise.reject(new UserError("Jocasta is closing; nothing was changed."));
		const call = run();
		this.c.calls.add(call);
		void call.catch(() => {}).finally(() => this.c.calls.delete(call));
		return call;
	}

	/** One `jocasta mcp` tool call (`POST /api/mcp/<tool>`). A refusal is `isError` with a one-line reason. */
	async callTool(name: string, args: Record<string, unknown>, caller: Caller): Promise<ToolReply> {
		try {
			return { text: paneText(await this.track(() => this.tools.tool(name, args, caller))) };
		} catch (error) {
			return { text: (error as Error).message, isError: true };
		}
	}

	/**
	 * One `jocasta thread|worktree` command (`POST /api/cli`, 0544): the same functions and rules as the boss's tools;
	 * `thread send`, `thread stop`, `thread finish` and `worktree remove` wait for his Allow. A refusal is `isError`.
	 */
	async cliCall(command: string, args: Record<string, unknown>): Promise<ToolReply> {
		try {
			return { text: paneText(await this.track(() => this.tools.cli(command, args))) };
		} catch (error) {
			return { text: (error as Error).message, isError: true };
		}
	}

	// ---- his keys (0544, 0554), `keys.ts` ----

	keyStop(paneId: string | undefined): Promise<string> {
		return this.keys.stop(paneId);
	}

	keyFinishable(paneId: string | undefined): Promise<{ paneId: string; name: string; detail: string; trunk: string }> {
		return this.keys.finishable(paneId);
	}

	keyFinish(paneId: string, how: "merge" | "pr"): Promise<{ text: string; isError: boolean }> {
		return this.keys.finish(paneId, how);
	}

	keyReview(paneId: string | undefined): Promise<string> {
		return this.keys.review(paneId);
	}

	newThreadForm(paneId?: string): Promise<{ projects: string[]; project: string }> {
		return this.keys.newThreadForm(paneId);
	}

	keyStart(project: string, brief: string): Promise<{ text: string; isError: boolean }> {
		return this.keys.start(project, brief);
	}

	// ---- the state feed ----

	/** What a client gets on connect (0409): the HUD, `today` and the dashboard. */
	snapshot(): Extract<ServerMessage, { type: "snapshot" }> {
		return clean({
			type: "snapshot",
			hud: this.hud(),
			today: this.day.today(),
			dashboard: this.day.dashboard(),
		});
	}

	/** The http(s) links `today` and the dashboard show now (0581, UX-7): the only ones `link.open` opens. */
	private shownLinks(): Set<string> {
		const links = new Set<string>();
		const walk = (value: unknown): void => {
			if (Array.isArray(value)) for (const item of value) walk(item);
			else if (value && typeof value === "object") {
				for (const [key, inner] of Object.entries(value)) {
					if (key === "url" && typeof inner === "string") {
						const url = webLink(inner);
						if (url) links.add(url);
					} else walk(inner);
				}
			}
		};
		walk(this.day.today());
		walk(this.day.dashboard());
		return links;
	}

	/**
	 * One message from a client (SEC-NET-5: `pane.focus` and `link.open` only; any other type is ignored); `reply`
	 * answers that client only. Refusals come back as `error`.
	 */
	async handle(message: ClientMessage, reply: (message: ServerMessage) => void): Promise<void> {
		try {
			switch (message.type) {
				case "pane.focus":
					await this.c.herdr.focus(message.paneId);
					break;
				case "link.open": {
					// Only a link the service itself shows now, and only http(s): a client can't open anything else (C9).
					const url = webLink(message.url);
					if (!url || !this.shownLinks().has(url)) throw new UserError("That link isn't one today shows.");
					openInBrowser(url);
					break;
				}
			}
		} catch (error) {
			reply({ type: "error", message: (error as Error).message });
		}
	}
}
