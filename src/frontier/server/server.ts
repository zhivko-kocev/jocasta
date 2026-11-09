import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { join } from "node:path";
import type { Server, ServerWebSocket } from "bun";
import { herdrSession } from "../../shared/cli.ts";
import { dockerEngineRunning, isConnectionError } from "../../shared/containers/index.ts";
import {
	graphitiPort,
	jocastaContainers,
	stopJocastaContainers,
	stopJocastaContainersDetached,
} from "../../shared/containers/resources.ts";
import { takeCalendarEnv } from "../../shared/credentials.ts";
import { sameToken } from "../../shared/fsx.ts";
import { selfCommand } from "../../shared/install.ts";
import { openInBrowser } from "../../shared/link.ts";
import { log } from "../../shared/log.ts";
import { containerCallbackUrl, setLlmCallback } from "../../shared/memory/graph/callback.ts";
import { addEpisodes, defaultGraphDeps, searchGraph } from "../../shared/memory/graph/index.ts";
import { configDir, dataDir } from "../../shared/paths.ts";
import { openCatalog } from "../../shared/schema.ts";
import { readSettings } from "../../shared/settings.ts";
import { CLI_PATH } from "../../shared/thread-cli.ts";
import {
	type ClientMessage,
	PLUGIN_ACTION_PATH,
	PLUGIN_ACTIONS,
	type PluginAction,
	POPUP_HOLD,
	POPUP_PATH,
	type PopupView,
	type ServerMessage,
	type ServiceHealth,
	TOKEN_COOKIE,
	TOKEN_QUERY,
	WS_PATH,
} from "../protocol.ts";
import { uiAsset } from "./assets.ts";
import { CodeIndex } from "./code-index.ts";
import { removeFrontierFile, writeFrontierFile } from "./frontier-file.ts";
import { connectHerdr, type Herdr, herdrRevivals } from "./herdr.ts";
import { SHUTDOWN_PATH } from "./launch.ts";
import { createLlmEndpoint, LLM_PATH } from "./llm-endpoint.ts";
import { AskPopups, PluginPanes, runAction } from "./plugin.ts";
import { ALLOW, allowQuestion, type Popup, Popups, STOP, stopQuestion } from "./popups.ts";
import { RELAY_PORT, startRelay } from "./relay.ts";
import { type Graph, JocastaService, type ServiceOptions } from "./service.ts";
import {
	CANVAS_PATH,
	CONNECTION_PATH,
	canvasAsset,
	canvasDir,
	canvasFiles,
	monacoWorker,
	startWhiteboard,
	WHITEBOARD_VERSION,
	type Whiteboard,
	type WhiteboardOptions,
	WORKER_PATH,
} from "./whiteboard.ts";

/** The pub/sub topic every client of the state feed subscribes to. */
const CLIENTS = "clients";
/** Where `jocasta mcp` calls its tools (`src/mcp/server.ts`). */
const MCP_PATH = "/api/mcp/";

/** How the close went: everything stopped, or he chose Cancel and everything runs on. */
export type CloseOutcome = "stopped" | "kept";

export interface FrontierServer {
	port: number;
	token: string;
	/** The collab relay's port on 127.0.0.1 (0561). */
	relayPort: number;
	/** The browser page's URL: `http://127.0.0.1:<port>/?t=<token>` (0254). */
	url: string;
	service: JocastaService;
	/** The service closes and `frontier.json` goes away; herdr and every pane keep running (a failed start). */
	stop(): Promise<void>;
	/**
	 * Closes Jocasta as one (0434, 0549): the quit action's close. With `ask` and any thread working or awaiting him,
	 * the close question is asked first (Stop everything or Cancel); Cancel leaves everything as it was. Otherwise
	 * everything Jocasta started stops (`stopEverything`) and `stopped` resolves.
	 */
	shutdown(options: { ask: boolean }): Promise<CloseOutcome>;
	/**
	 * His herdr client closed (he quit it, or closed its terminal; 0549): with nothing working or awaiting him,
	 * everything stops at once; otherwise a new terminal attaches to the session again and the close question is asked
	 * there.
	 */
	clientClosed(): Promise<CloseOutcome>;
	/**
	 * Registers something else Jocasta started that stops with it (0549): run on Stop everything after the containers,
	 * bounded by `ms` (default 10 s). Returns the function that unregisters it.
	 */
	stopsWith(name: string, stop: () => unknown, ms?: number): () => void;
	/** Resolves once the server has stopped, by `stop()` or a close; every step is bounded, so it always does. */
	stopped: Promise<void>;
	/**
	 * Resolves once the Jocasta service has started (the boss pane ensured, threads found, sessions read); rejects when
	 * it couldn't. The server listens before that.
	 */
	ready: Promise<void>;
	/** whiteboard's server, the service's child (0393); undefined when the server was started without it. */
	whiteboard: Whiteboard | undefined;
	/**
	 * Asks him a question (0519, 0552): its one-time code goes to `onQuestion` (what shows it) and nowhere else; its
	 * answer comes through `POST /api/popup/<code>`. Every open one ends unanswered when the server stops.
	 */
	question(view: PopupView, signal?: AbortSignal): Popup;
}

export interface ServerOptions
	extends Pick<
		ServiceOptions,
		| "omp"
		| "hunk"
		| "hunkPort"
		| "bossCommand"
		| "panePrompt"
		| "gh"
		| "ghTimeoutMs"
		| "sessionsDir"
		| "agentWaitMs"
		| "refreshMs"
		| "findReview"
		| "openReview"
		| "submitWaitMs"
		| "mise"
		| "weeklyReviewMs"
		| "weeklyReviews"
		| "confirm"
		| "confirmWaitMs"
		| "bossKey"
		| "leftoverWaitMs"
		| "leftoverAgainMs"
		| "relayUrlOf"
		| "hostsDir"
		| "hostPollMs"
		| "askReport"
	> {
	/** The collab relay's port on 127.0.0.1 (test seam): `RELAY_PORT`; 0 a free one. Taken: the start fails (0561). */
	relayPort?: number;
	/** herdr (test seam): session `jocasta` by default, started with `~/.jocasta/agent/herdr.toml` when absent. */
	herdr?: Herdr;
	/** The herdr session to connect to when `herdr` isn't given (smoke seam); `jocasta` by default. */
	herdrSession?: string;
	/** The knowledge graph (test seam): Graphiti's by default; null for none, so tests never start Docker. */
	graph?: Graph | null;
	/** The code index behind `graph_search` (0562; test seam): codebase-memory-mcp under `<data>/code-index` by default; null for none. */
	codeIndex?: CodeIndex | null;
	/** Phase 8's pollers and clock (test seam); tests pass `{ pollers: false }` so nothing calls the real gh or az. */
	day?: ServiceOptions["day"];
	/** The port to try first (test seam): `frontier.port`, else 47400 (0358). */
	port?: number;
	/** whiteboard's server (0393): started from the install or the built clone by default; null for none (tests). */
	whiteboard?: WhiteboardOptions | null;
	/**
	 * Shows a question to him (the herdr popup, 0553): it gets the question's one-time code. Until something shows it, a
	 * question is answered only through `POST /api/popup/<code>`.
	 */
	onQuestion?: (popup: Popup) => void;
	/**
	 * The herdr plugin (0553): `today` beside the boss, the Agents view, his questions in its `ask` popup (unless
	 * `onQuestion` shows them) and his thread keys. On with the real herdr, off with a test's `herdr` unless set.
	 */
	plugin?: boolean;
	/**
	 * Opens a new terminal attached to herdr's session, as bare `jocasta` does (0549): his client closed while threads
	 * are at work, so the close question has somewhere to show. A new Windows Terminal window running `jocasta` by
	 * default (test seam).
	 */
	reattach?: () => void;
	/** Stops Graphiti and FalkorDB, their data kept (test seam); the shared container manager's by default. */
	stopContainers?: () => Promise<void>;
	/** Each stop step's bound in ms (test seam): service 30 s, herdr 15 s, containers 20 s, whiteboard 5 s. */
	stopBoundsMs?: Partial<Record<"service" | "herdr" | "containers" | "whiteboard", number>>;
}

/** A stop step that outlived its bound. */
class StepTimeout extends Error {}

/** `promise`, or a `StepTimeout` after `ms` (the timer never outlives the promise). */
async function within<T>(promise: Promise<T>, ms: number): Promise<T> {
	let timer: Timer | undefined;
	try {
		return await Promise.race([
			promise,
			new Promise<never>((_, reject) => {
				timer = setTimeout(() => reject(new StepTimeout(`no answer in ${ms / 1000} s`)), ms);
			}),
		]);
	} finally {
		clearTimeout(timer);
	}
}

/** Graphiti behind the shared container manager (0334), with this process's `/llm/v1` callback (0390). */
function graphiti(): Graph {
	const deps = defaultGraphDeps();
	return {
		search: (projectId, query, limit) => searchGraph(projectId, query, limit, deps),
		post: (projectId, { name, body, source, at }) =>
			addEpisodes(projectId, [{ name, body, source, reference_time: at }], deps),
	};
}

/** What Graphiti's line on the services row is chosen from (0444); tests inject them. */
export interface GraphitiChecks {
	/** `/health` now: "ok"; "refused" when nothing listens; "bad" when it listens but isn't OK or times out. */
	health(): Promise<"ok" | "refused" | "bad">;
	/** Whether this process is starting Graphiti's containers. */
	starting(): boolean;
	/** Whether Docker's engine runs. */
	dockerRunning(): boolean;
}

const GRAPHITI_CHECKS: GraphitiChecks = {
	health: async () => {
		try {
			const response = await fetch(`http://127.0.0.1:${graphitiPort()}/health`, {
				signal: AbortSignal.timeout(2_000),
			});
			return response.ok ? "ok" : "bad";
		} catch (error) {
			return isConnectionError(error) ? "refused" : "bad";
		}
	},
	starting: () => jocastaContainers().isStarting("graphiti"),
	dockerRunning: () => dockerEngineRunning(),
};

/**
 * Graphiti on the services row (0444): "up" while it answers; "starting" while Jocasta starts it; "idle · starts when
 * needed" while nothing listens and Docker runs (it starts on first use and stops after 15 min idle, 0073); "down ·
 * Docker isn't running"; "down · not answering" when it listens but `/health` isn't OK.
 */
export async function graphitiHealth(checks: GraphitiChecks = GRAPHITI_CHECKS): Promise<Omit<ServiceHealth, "name">> {
	const health = await checks.health();
	if (health === "ok") return { state: "up" };
	if (checks.starting()) return { state: "starting" };
	if (health === "bad") return { state: "down", note: "not answering" };
	return checks.dockerRunning()
		? { state: "idle", note: "starts when needed" }
		: { state: "down", note: "Docker isn't running" };
}

/** `frontier.port`'s default (0358). */
export const DEFAULT_PORT = 47_400;

/** `frontier.port` in `jocasta.yaml` (0358, 0387): a port number, else 47400. */
function configuredPort(): number {
	const value = (readSettings().frontier as { port?: unknown } | undefined)?.port;
	return typeof value === "number" && Number.isInteger(value) && value > 0 && value < 65_536 ? value : DEFAULT_PORT;
}

/**
 * The canvas page's Content-Security-Policy (0368, 0552): scripts, styles, workers and fetches come only from this
 * server, plus whiteboard's server on loopback (0393), which the canvas fetches from and shows images of.
 * `wasm-unsafe-eval` lets the canvas's router (libavoid) compile its wasm, and `unsafe-eval` lets its embind glue build
 * functions from strings, so whiteboard's software maps route (0493); inline scripts and other origins stay blocked.
 * Inline styles stay allowed: the canvas and Monaco set `style` attributes, which take no nonce, and a style can't run
 * code. Monaco's worker is this server's (0411).
 */
export function contentPolicy(port: number, whiteboard?: string): string {
	const other = whiteboard ? ` ${whiteboard}` : "";
	return [
		"default-src 'self'",
		"script-src 'self' 'wasm-unsafe-eval' 'unsafe-eval'",
		"style-src 'self' 'unsafe-inline'",
		`img-src 'self' data: blob:${other}`,
		"font-src 'self' data:",
		"media-src 'self' data: blob:",
		`connect-src 'self' ws://127.0.0.1:${port}${other}`,
		"worker-src 'self'",
		"object-src 'none'",
		"base-uri 'none'",
		"form-action 'none'",
		"frame-ancestors 'none'",
	].join("; ");
}

/**
 * Every response carries the content policy, and the page runs cross-origin isolated (0354): COOP/COEP, and its own
 * files say they may be embedded only by this origin.
 */
function guarded(response: Response, port: number, whiteboard?: string): Response {
	response.headers.set("content-security-policy", contentPolicy(port, whiteboard));
	response.headers.set("cross-origin-opener-policy", "same-origin");
	response.headers.set("cross-origin-embedder-policy", "require-corp");
	response.headers.set("cross-origin-resource-policy", "same-origin");
	return response;
}

function cookieToken(request: Request): string | undefined {
	const header = request.headers.get("cookie") ?? "";
	for (const part of header.split(";")) {
		const [name, ...value] = part.trim().split("=");
		if (name === TOKEN_COOKIE) return value.join("=");
	}
	return undefined;
}

function bearerToken(request: Request): string {
	const auth = request.headers.get("authorization") ?? "";
	return auth.startsWith("Bearer ") ? auth.slice(7) : "";
}

/**
 * A new Windows Terminal window running `jocasta`, which attaches to herdr's session as it always does (0549); a new
 * console window where Windows Terminal isn't installed. Off Windows there is none: the question waits for a client.
 */
export function openJocastaTerminal(): void {
	if (process.platform !== "win32") {
		log("warn", "shutdown", "no terminal opens here; the close question waits for `jocasta`");
		return;
	}
	const command = selfCommand();
	const wt = Bun.which("wt");
	const [program, ...args] = wt
		? [wt, "-w", "new", "--title", "Jocasta", ...command]
		: [process.env.ComSpec ?? "cmd.exe", "/d", "/c", "start", "Jocasta", ...command];
	if (!program) return;
	spawn(program, args, { detached: true, stdio: "ignore" }).unref();
}

/**
 * The Jocasta service's server (4.1, 0392, 0552): 127.0.0.1 on `frontier.port` (0358), or a free port for this run
 * when that one is busy; `<data>/frontier.json` with `port`, `token` and `pid`. Every request carries the token, as a
 * bearer (`jocasta mcp`, bare `jocasta`, a state client) or as the cookie a browser's `/?t=<token>` sets (0254). It
 * serves the MCP tools, the state feed, his questions, whiteboard's canvas page (0393) and Graphiti's `/llm/v1`
 * (0390), and runs the herdr-backed service.
 */
export async function startServer(options: ServerOptions = {}): Promise<FrontierServer> {
	// The calendar's secret leaves this process's environment before anything is spawned (SEC-5, 0430).
	takeCalendarEnv();
	const token = randomBytes(32).toString("hex");
	// The collab relay every omp pane hosts its session in (0536, 0561), first: a port another program holds stops the
	// start before herdr or any pane starts. It stops with Jocasta, and with a failed start.
	const relay = startRelay(options.relayPort ?? RELAY_PORT);
	let server: Server<undefined> | undefined;
	const broadcast = (message: ServerMessage) => server?.publish(CLIENTS, JSON.stringify(message));
	const popups = new Popups();
	/** What shows his questions: the test's, else the plugin's popup once herdr is connected (0553). */
	let showQuestion = options.onQuestion;
	const question = (view: PopupView, signal?: AbortSignal): Popup => {
		const popup = popups.open(view);
		if (signal?.aborted) popup.close();
		signal?.addEventListener("abort", () => popup.close(), { once: true });
		if (popup.open) {
			log("info", "question", `asking him: ${view.title}`);
			try {
				showQuestion?.(popup);
			} catch (error) {
				log("warn", "question", `couldn't show it: ${(error as Error).message}`);
			}
		}
		return popup;
	};
	const herdrConfig = join(configDir(), "herdr.toml");
	const session = options.herdrSession ?? herdrSession();
	const herdr =
		options.herdr ??
		(await connectHerdr({
			session,
			...(existsSync(herdrConfig) ? { configPath: herdrConfig } : {}),
		}).catch((error: unknown) => {
			relay.stop();
			throw error;
		}));
	const plugin = (options.plugin ?? options.herdr === undefined) ? new PluginPanes(herdr) : undefined;
	const askPopups = plugin ? new AskPopups(herdr) : undefined;
	showQuestion ??= askPopups ? (popup) => askPopups.show(popup) : undefined;
	const graph = options.graph === undefined ? graphiti() : (options.graph ?? undefined);
	// The code index (0562): its stdio servers start on the first relationship question and stop with Jocasta.
	const codeIndex =
		options.codeIndex === undefined ? new CodeIndex({ dataDir: dataDir() }) : (options.codeIndex ?? undefined);
	// whiteboard's server, the service's child (0393): it starts beside the service and stops with Jocasta.
	const whiteboard = options.whiteboard === null ? undefined : startWhiteboard(options.whiteboard);
	let pageUrl = "";
	const service = new JocastaService(openCatalog(), {
		herdr,
		broadcast,
		...(plugin ? { plugin } : {}),
		...(graph ? { graph } : {}),
		...(options.day ? { day: options.day } : {}),
		...(options.omp ? { omp: options.omp } : {}),
		...(options.hunk ? { hunk: options.hunk } : {}),
		...(options.hunkPort !== undefined ? { hunkPort: options.hunkPort } : {}),
		...(options.bossCommand ? { bossCommand: options.bossCommand } : {}),
		...(options.panePrompt !== undefined ? { panePrompt: options.panePrompt } : {}),
		...(options.gh ? { gh: options.gh } : {}),
		...(options.ghTimeoutMs !== undefined ? { ghTimeoutMs: options.ghTimeoutMs } : {}),
		...(options.sessionsDir ? { sessionsDir: options.sessionsDir } : {}),
		...(options.agentWaitMs !== undefined ? { agentWaitMs: options.agentWaitMs } : {}),
		...(options.submitWaitMs !== undefined ? { submitWaitMs: options.submitWaitMs } : {}),
		...(options.refreshMs !== undefined ? { refreshMs: options.refreshMs } : {}),
		...(options.leftoverWaitMs !== undefined ? { leftoverWaitMs: options.leftoverWaitMs } : {}),
		...(options.leftoverAgainMs !== undefined ? { leftoverAgainMs: options.leftoverAgainMs } : {}),
		...(options.mise !== undefined ? { mise: options.mise } : {}),
		...(options.weeklyReviewMs !== undefined ? { weeklyReviewMs: options.weeklyReviewMs } : {}),
		...(options.weeklyReviews === false ? { weeklyReviews: false as const } : {}),
		...(options.confirmWaitMs !== undefined ? { confirmWaitMs: options.confirmWaitMs } : {}),
		...(options.bossKey !== undefined ? { bossKey: options.bossKey } : {}),
		relayPort: relay.port,
		...(options.relayUrlOf ? { relayUrlOf: options.relayUrlOf } : {}),
		...(options.hostsDir ? { hostsDir: options.hostsDir } : {}),
		...(options.hostPollMs !== undefined ? { hostPollMs: options.hostPollMs } : {}),
		...(options.askReport === false ? { askReport: false as const } : {}),
		...(codeIndex ? { codeIndex } : {}),
		// His Allow/Deny before a risky boss action (0484), Deny by default and answered only with its one-time code.
		// When the answer isn't wanted any more (the wait ran out, Jocasta is closing), it goes as Deny.
		confirm: options.confirm ?? (async (ask, signal) => (await question(allowQuestion(ask), signal).answer) === ALLOW),
		// The dashboard's services row (0429, 0444): Graphiti and whiteboard, each up or why not.
		services: async () => {
			const off: Omit<ServiceHealth, "name"> = { state: "down", note: "off in this run" };
			return [
				{ name: "Graphiti", ...(graph ? await graphitiHealth() : off) },
				{ name: "whiteboard", ...(whiteboard ? await whiteboard.check() : off) },
			];
		},
		// The boss's `show` checks the review exists first (whiteboard's `GET /reviews-api/:id`, 404 when none), then
		// opens it in his browser on this server's canvas page (0552).
		...(options.findReview
			? { findReview: options.findReview }
			: whiteboard
				? {
						findReview: async (id: string) => {
							const { url, token: wbToken } = await whiteboard.ready;
							const response = await fetch(`${url}/reviews-api/${encodeURIComponent(id)}?full=true`, {
								headers: { "x-review-token": wbToken },
								signal: AbortSignal.timeout(10_000),
							});
							if (response.status === 404) return null;
							if (!response.ok) throw new Error(`whiteboard answered ${response.status}`);
							const body: unknown = await response.json();
							const title = body && typeof body === "object" && "title" in body ? body.title : undefined;
							return { title: typeof title === "string" && title ? title : id };
						},
					}
				: {}),
		...(options.openReview
			? { openReview: options.openReview }
			: whiteboard
				? { openReview: (id: string) => openInBrowser(`${pageUrl}&review=${encodeURIComponent(id)}`) }
				: {}),
	});

	// Graphiti's container calls back into this process for its model calls, under `/llm/v1` (0334, 0390).
	const llm = createLlmEndpoint();

	/** What else Jocasta started that stops with it (0549), in the order registered. */
	const extraStops = new Set<{ name: string; stop: () => unknown; ms: number }>();
	if (codeIndex) extraStops.add({ name: "the code index", stop: () => codeIndex.stop(), ms: 10_000 });
	const { promise: stopped, resolve: markStopped } = Promise.withResolvers<void>();
	let stopping: Promise<void> | undefined;
	/**
	 * A failed start stops the service only; Jocasta closing as one (`all`, 0434, 0549) stops everything, in this
	 * order. Every step is caught and the outside ones are bounded (LC-5, LC-8), so `stopped` always resolves.
	 */
	const stop = (all = false) => {
		stopping ??= (async () => {
			const bounds = { service: 30_000, herdr: 15_000, containers: 20_000, whiteboard: 5_000, ...options.stopBoundsMs };
			/** Runs one step; a throw or a step outliving `ms` is said in the log, and the close goes on. */
			const step = async (name: string, run: () => unknown, ms?: number): Promise<void> => {
				try {
					const done = Promise.resolve(run());
					await (ms === undefined ? done : within(done, ms));
				} catch (error) {
					log("warn", "shutdown", `${name} didn't finish: ${error instanceof Error ? error.message : String(error)}`);
					// Containers that don't stop in time stop after this process has gone.
					if (error instanceof StepTimeout && name === "Graphiti's containers") stopJocastaContainersDetached();
				}
			};
			try {
				// His open questions go unanswered: Cancel, Deny (0519).
				await step("his questions", () => popups.closeAll());
				await step("goodbye", () => broadcast({ type: "bye" }));
				// herdr is never started again while Jocasta closes (0431's restart, 0434).
				if (all) await step("holding herdr down", () => herdr.keepDown());
				// Threads at work come back with `--restore` on the next start (0549).
				if (all) await step("noting the threads at work", () => service.pauseWorking());
				await step("the service", () => service.stop(), bounds.service);
				// Their omps end before herdr closes their panes, so their turns stay open for `--restore` (0549).
				if (all) await step("the turns at work", () => service.cutPausedTurns());
				if (all) await step(`herdr session ${session}`, () => herdr.stopServer(), bounds.herdr);
				// What the thread panes' omps left, an earlier herdr server's included, once herdr is gone (0486, 0490).
				if (all) await step("the threads' leftovers", () => service.endThreadLeftovers());
				// The host registry entries of the omps that ended (0547).
				if (all) await step("omp's host entries", () => service.removeEndedHosts());
				// Once herdr's hunk panes are gone, the hunk session daemon the service started (0434, 0486).
				if (all) await step("hunk's daemon", () => service.endHunkDaemon());
				if (all) {
					await step(
						"Graphiti's containers",
						() => (options.stopContainers ?? stopJocastaContainers)(),
						bounds.containers,
					);
					for (const extra of extraStops) await step(extra.name, extra.stop, extra.ms);
				}
				if (whiteboard) await step("whiteboard", () => whiteboard.stop(), bounds.whiteboard);
				// Also after a failed start: nothing else would close its port.
				await step("the collab relay", () => relay.stop());
			} finally {
				await step("the model callback", () => setLlmCallback(undefined));
				await step("the server", () => server?.stop(true));
				await step("frontier.json", () => removeFrontierFile(process.pid));
				markStopped();
			}
		})();
		return stopping;
	};

	let asking: Promise<CloseOutcome> | undefined;
	/**
	 * The close (0434, 0438, 0549): threads working or awaiting him ask first (`reattach` first opens a terminal on
	 * herdr's session for it: his client is gone), and Cancel keeps everything; otherwise everything stops. One close
	 * at a time: a second asks nothing more.
	 */
	const close = ({ ask, reattach }: { ask: boolean; reattach: boolean }): Promise<CloseOutcome> => {
		if (stopping) return stopping.then(() => "stopped");
		asking ??= (async (): Promise<CloseOutcome> => {
			// Threads count once herdr's panes were found (LC-9); a start that hangs is waited for 10 s at most.
			if (ask) await within(service.discovered, 10_000).catch(() => {});
			const { working, awaiting } = ask ? service.busyThreads() : { working: 0, awaiting: 0 };
			const asked = working + awaiting > 0;
			if (asked) {
				if (reattach) {
					try {
						(options.reattach ?? openJocastaTerminal)();
					} catch (error) {
						log("warn", "shutdown", `no terminal opened for the close question: ${(error as Error).message}`);
					}
				}
				if ((await question(stopQuestion({ working, awaiting })).answer) !== STOP) {
					const now = service.busyThreads();
					log(
						"info",
						"shutdown",
						`kept running: Cancel chosen with ${now.working} working and ${now.awaiting} awaiting`,
					);
					return "kept";
				}
			}
			const now = asked ? service.busyThreads() : undefined;
			log(
				"info",
				"shutdown",
				now
					? `stopping everything: Stop chosen with ${now.working} working and ${now.awaiting} awaiting`
					: "stopping everything",
			);
			await stop(true);
			return "stopped";
		})().finally(() => {
			asking = undefined;
		});
		return asking;
	};

	/**
	 * `POST /api/shutdown` from bare `jocasta` (0434, 0549): his client closed. Answered at once, since it may come
	 * from a closing terminal with seconds left.
	 */
	const shutdownResponse = (request: Request): Response => {
		if (!sameToken(bearerToken(request), token)) return new Response("invalid token", { status: 401 });
		if (request.method !== "POST") return new Response("POST only", { status: 405 });
		// After the answer has gone out, as it may come from a closing terminal.
		setTimeout(() => void close({ ask: true, reattach: true }), 100);
		return new Response(null, { status: 202 });
	};

	/**
	 * `POST /api/mcp/<tool>` from `jocasta mcp`: bearer token, body `{args, paneId?, bossKey?}`, reply `{text, isError?}`.
	 * A call may wait for his Allow up to 2 minutes (0484), so it has no idle timeout.
	 */
	const mcpResponse = async (request: Request, url: URL, srv: Server<undefined>): Promise<Response> => {
		if (!sameToken(bearerToken(request), token)) {
			return Response.json({ text: "invalid token", isError: true }, { status: 401 });
		}
		if (request.method !== "POST") return Response.json({ text: "POST only", isError: true }, { status: 405 });
		srv.timeout(request, 0);
		let body: { args?: unknown; paneId?: unknown; bossKey?: unknown };
		try {
			body = (await request.json()) as typeof body;
		} catch {
			return Response.json({ text: "body is not JSON", isError: true }, { status: 400 });
		}
		const args =
			body.args !== null && typeof body.args === "object" && !Array.isArray(body.args)
				? (body.args as Record<string, unknown>)
				: {};
		const tool = decodeURIComponent(url.pathname.slice(MCP_PATH.length));
		return Response.json(
			await service.callTool(tool, args, {
				...(typeof body.paneId === "string" ? { paneId: body.paneId } : {}),
				...(typeof body.bossKey === "string" ? { bossKey: body.bossKey } : {}),
			}),
		);
	};

	/**
	 * `POST /api/cli` from `jocasta thread|worktree` (0544): bearer token, body `{command, args}`, reply `{text,
	 * isError?}`. A change waits for his Allow up to 2 minutes, a merge for its hooks, so it has no idle timeout.
	 */
	const cliResponse = async (request: Request, srv: Server<undefined>): Promise<Response> => {
		if (!sameToken(bearerToken(request), token)) {
			return Response.json({ text: "invalid token", isError: true }, { status: 401 });
		}
		if (request.method !== "POST") return Response.json({ text: "POST only", isError: true }, { status: 405 });
		srv.timeout(request, 0);
		let body: { command?: unknown; args?: unknown };
		try {
			body = (await request.json()) as typeof body;
		} catch {
			return Response.json({ text: "body is not JSON", isError: true }, { status: 400 });
		}
		if (typeof body.command !== "string") {
			return Response.json({ text: "command is required", isError: true }, { status: 400 });
		}
		const args =
			body.args !== null && typeof body.args === "object" && !Array.isArray(body.args)
				? (body.args as Record<string, unknown>)
				: {};
		return Response.json(await service.cliCall(body.command, args));
	};

	/** The token as a bearer or as the browser's cookie (0254). */
	const authorized = (request: Request) =>
		sameToken(bearerToken(request), token) || sameToken(cookieToken(request), token);

	/** Everything behind the token: health, questions, the canvas page and its files. */
	const pageResponse = async (request: Request, url: URL): Promise<Response> => {
		const fromQuery = url.searchParams.get(TOKEN_QUERY);
		if (fromQuery !== null) {
			if (!sameToken(fromQuery, token)) return new Response("Forbidden", { status: 403 });
			const rest = new URLSearchParams(url.searchParams);
			rest.delete(TOKEN_QUERY);
			const query = rest.toString();
			return new Response(null, {
				status: 302,
				headers: {
					location: `/${query ? `?${query}` : ""}`,
					"set-cookie": `${TOKEN_COOKIE}=${token}; Path=/; HttpOnly; SameSite=Strict`,
				},
			});
		}
		if (!authorized(request)) return new Response("Forbidden", { status: 403 });
		// `closing`: Jocasta is closing, so a `jocasta` started meanwhile waits instead of attaching to it (LC-3);
		// `herdrRevivals`: a client that exited while it grew lost a crashed herdr server, not him (0534).
		if (url.pathname === "/health")
			return Response.json({
				pid: process.pid,
				herdrRevivals: herdrRevivals(),
				...(stopping ? { closing: true } : {}),
			});
		if (url.pathname.startsWith(POPUP_PATH)) return popupResponse(request, url);
		if (request.method !== "GET" || url.pathname === WS_PATH) return new Response("Not found", { status: 404 });
		if (url.pathname === CONNECTION_PATH) return whiteboardConnection();
		if (url.pathname === WORKER_PATH) return (await monacoWorker()) ?? new Response("Not found", { status: 404 });
		const canvas = url.pathname.startsWith(CANVAS_PATH) ? canvasDir() : undefined;
		const served = canvas ? canvasAsset(canvas, url.pathname) : await uiAsset(url.pathname);
		return served ?? new Response("Not found", { status: 404 });
	};

	/**
	 * A question's texts and its answer (0519), each with the service's token (checked before) and the question's
	 * one-time code: an unknown or used code is refused, an answer is taken once.
	 */
	const popupResponse = async (request: Request, url: URL): Promise<Response> => {
		const code = url.pathname.slice(POPUP_PATH.length);
		const refused = () => new Response("Forbidden", { status: 403 });
		if (code.includes("/")) return new Response("Not found", { status: 404 });
		if (request.method === "GET") {
			const view = popups.view(code);
			return view ? Response.json(view, { headers: { "cache-control": "no-store" } }) : refused();
		}
		if (request.method !== "POST") return new Response("Method not allowed", { status: 405 });
		let body: object = {};
		try {
			const parsed: unknown = await request.json();
			if (parsed !== null && typeof parsed === "object") body = parsed;
		} catch {
			// An answer without a body is no choice.
		}
		const taken = popups.answer(code, body);
		if (taken === "unknown") return refused();
		if (taken === "invalid") return new Response("not one of its choices", { status: 400 });
		// A coded choice with letters that aren't its code (0558, 0568): it stays open, until the third.
		if (taken === "wrong") return new Response("wrong code", { status: 409 });
		return new Response(null, { status: 204 });
	};

	/** What one of his keys did, for `today`, and a refusal as a notification too: he may be on another workspace. */
	const reportAction = (text: string, isError: boolean) => {
		log(isError ? "warn" : "info", "keys", text);
		broadcast({ type: "action", text, isError });
		// Logged in notifications.jsonl as the service's toasts are (0552).
		if (isError) void service.notify({ type: "toast", reason: "failed", level: "warning", text });
	};

	/**
	 * `POST /api/plugin/action` (0544, 0554): one of his thread keys, `{action, paneId?}` with the service's token,
	 * answered at once; what it did follows as an `action` message (`reportAction`).
	 */
	const actionResponse = async (request: Request): Promise<Response> => {
		if (!sameToken(bearerToken(request), token)) return new Response("invalid token", { status: 401 });
		if (request.method !== "POST") return new Response("POST only", { status: 405 });
		let body: { action?: unknown; paneId?: unknown } = {};
		try {
			const parsed: unknown = await request.json();
			if (parsed !== null && typeof parsed === "object") body = parsed;
		} catch {
			// No body: no action.
		}
		const action = PLUGIN_ACTIONS.find((a) => a === body.action);
		if (!action) return new Response(`action is one of ${PLUGIN_ACTIONS.join(", ")}`, { status: 400 });
		const paneId = typeof body.paneId === "string" && body.paneId ? body.paneId : undefined;
		void runKey(action, paneId);
		return new Response(null, { status: 202 });
	};

	const runKey = async (action: PluginAction, paneId: string | undefined): Promise<void> => {
		try {
			reportAction(
				await runAction(action, paneId, {
					service,
					ask: (view) => question(view),
					quit: () => close({ ask: true, reattach: false }),
				}),
				false,
			);
		} catch (error) {
			reportAction(error instanceof Error ? error.message : String(error), true);
		}
	};

	/**
	 * What the canvas page mounts whiteboard's canvas with (0393): the server's url and token, once it answers, and the
	 * canvas build's files; 503 with the reason when either is missing, or when the server stopped answering since (its
	 * process died, B-3): the page then shows its own failed state, not whiteboard's.
	 */
	const whiteboardConnection = async (): Promise<Response> => {
		const dir = canvasDir();
		if (!whiteboard || !dir) {
			return Response.json(
				{ error: whiteboard ? "whiteboard's canvas isn't built: run `bun run build`" : "whiteboard is off" },
				{ status: 503 },
			);
		}
		try {
			const { url, token: wbToken } = await whiteboard.ready;
			if ((await whiteboard.check()).state !== "up") {
				return Response.json({ error: `whiteboard's server at ${url} stopped answering` }, { status: 503 });
			}
			return Response.json({
				url,
				token: wbToken,
				version: WHITEBOARD_VERSION,
				canvas: canvasFiles(dir),
				worker: WORKER_PATH,
			});
		} catch (error) {
			return Response.json({ error: (error as Error).message }, { status: 503 });
		}
	};

	/** The Hosts this server answers as (DNS rebinding, SE-3); Graphiti's container may also use its docker name. */
	const hostAllowed = (host: string | null, port: number, path: string): boolean =>
		host === `127.0.0.1:${port}` ||
		host === `localhost:${port}` ||
		(host === `host.docker.internal:${port}` && (path === LLM_PATH || path.startsWith(`${LLM_PATH}/`)));

	const serve = (port: number): Server<undefined> =>
		Bun.serve<undefined>({
			hostname: "127.0.0.1",
			port,
			development: false,
			error(error) {
				log("error", "server", error.stack ?? String(error));
				return new Response("Internal error", { status: 500 });
			},
			async fetch(request, srv) {
				const url = new URL(request.url);
				if (!hostAllowed(request.headers.get("host"), srv.port ?? port, url.pathname)) {
					return new Response("Forbidden", { status: 403 });
				}
				if (url.pathname.startsWith(MCP_PATH)) return mcpResponse(request, url, srv);
				if (url.pathname === CLI_PATH) return cliResponse(request, srv);
				if (url.pathname === SHUTDOWN_PATH) return shutdownResponse(request);
				if (url.pathname === PLUGIN_ACTION_PATH) return actionResponse(request);
				// The `ask` popup's held request (0558): open for as long as it shows its question.
				if (url.pathname.startsWith(POPUP_PATH) && url.pathname.endsWith(POPUP_HOLD)) {
					if (!authorized(request) || !askPopups) return new Response("Forbidden", { status: 403 });
					srv.timeout(request, 0);
					const code = url.pathname.slice(POPUP_PATH.length, -POPUP_HOLD.length);
					return askPopups.hold(code, request.signal);
				}
				const answered = await llm.handle(request);
				if (answered) return answered;
				if (url.pathname === WS_PATH && authorized(request)) {
					// A cross-site page can't open the socket even with the cookie.
					const origin = request.headers.get("origin");
					if (origin !== null && origin !== `http://127.0.0.1:${srv.port}`)
						return guarded(new Response("Forbidden", { status: 403 }), srv.port ?? port);
					return srv.upgrade(request, { data: undefined })
						? undefined
						: guarded(new Response("Upgrade failed", { status: 400 }), srv.port ?? port);
				}
				return guarded(await pageResponse(request, url), srv.port ?? port, whiteboard?.origin());
			},
			websocket: {
				open(ws: ServerWebSocket<undefined>) {
					ws.subscribe(CLIENTS);
					ws.send(JSON.stringify({ ...service.snapshot(), quietHours: service.day.quiet() } satisfies ServerMessage));
				},
				async message(ws, data: string | Buffer) {
					const reply = (message: ServerMessage) => ws.send(JSON.stringify(message));
					let message: ClientMessage;
					try {
						message = JSON.parse(String(data)) as ClientMessage;
					} catch {
						reply({ type: "error", message: "not JSON" });
						return;
					}
					await service.handle(message, reply);
				},
			},
		});
	const wanted = options.port ?? configuredPort();
	try {
		server = serve(wanted);
	} catch {
		// Busy (another program, or another Jocasta's data dir): a free port for this run (0358).
		server = serve(0);
		log("warn", "server", `port ${wanted} (frontier.port) was busy; Jocasta is on port ${server.port} for this run`);
	}
	const port = server.port as number;
	pageUrl = `http://127.0.0.1:${port}/?${TOKEN_QUERY}=${token}`;
	setLlmCallback({ baseUrl: containerCallbackUrl(port), token: llm.token });
	writeFrontierFile({ port, token, pid: process.pid });
	// The service starts once the server listens: its start (git per pane folder, the first read of every session
	// today) takes seconds on a busy day, and bare `jocasta` must not wait for it.
	const ready = service.start().catch(async (error: unknown) => {
		log("error", "service", `didn't start: ${error instanceof Error ? error.message : String(error)}`);
		throw error;
	});
	// Its failure is the caller's to handle (`ready`); here it only must not go unhandled.
	ready.catch(() => {});
	return {
		port,
		token,
		relayPort: relay.port,
		url: pageUrl,
		service,
		stop: () => stop(),
		shutdown: ({ ask }) => close({ ask, reattach: false }),
		clientClosed: () => close({ ask: true, reattach: true }),
		stopsWith: (name, run, ms = 10_000) => {
			const entry = { name, stop: run, ms };
			extraStops.add(entry);
			return () => extraStops.delete(entry);
		},
		stopped,
		ready,
		whiteboard,
		question,
	};
}
