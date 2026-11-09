import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { join } from "node:path";
import type { Server } from "bun";
import { dockerEngineRunning, isConnectionError, nativeLinuxDocker } from "../../shared/containers/index.ts";
import { graphitiPort, jocastaContainers, stopJocastaContainers } from "../../shared/containers/resources.ts";
import { takeCalendarEnv } from "../../shared/credentials.ts";
import { herdrSession } from "../../shared/herdr-session.ts";
import { openInBrowser } from "../../shared/link.ts";
import { log } from "../../shared/log.ts";
import { CALLBACK_UNREACHABLE } from "../../shared/memory/graph/callback.ts";
import { addEpisodes, defaultGraphDeps, searchGraph } from "../../shared/memory/graph/index.ts";
import { configDir, dataDir } from "../../shared/paths.ts";
import type { ProcessId } from "../../shared/proctree.ts";
import { openCatalog } from "../../shared/schema.ts";
import { removeServiceFile, writeServiceFile } from "../../shared/service-file.ts";
import { readSettings, serviceSetting } from "../../shared/settings.ts";
import { type PopupView, type ServerMessage, type ServiceHealth, TOKEN_QUERY } from "../protocol.ts";
import { CONFIRM_WAIT_MS } from "./allow.ts";
import { type CloseOutcome, closeFlow } from "./close.ts";
import { CodeIndex } from "./code-index.ts";
import { doorRoutes } from "./door-routes.ts";
import { feedRoute, feedSocket, publish } from "./feed.ts";
import { healthRoute } from "./health-route.ts";
import { connectHerdr, type Herdr } from "./herdr.ts";
import { llmCallback } from "./llm-route.ts";
import { LaunchCodes, pageResponder, whiteboardReviews } from "./page-routes.ts";
import { AskPopups, PluginPanes } from "./plugin.ts";
import { type Ask, actionRoute, asker, popupRoutes } from "./popup-routes.ts";
import { ALLOW, allowQuestion, type Popup, Popups } from "./popups.ts";
import { RELAY_PORT, startRelay } from "./relay.ts";
import { listen, type Route } from "./routes.ts";
import { type Graph, JocastaService, type ServiceOptions } from "./service.ts";
import { type StopStep, stopper, stopSteps, undoStart } from "./stop-sequence.ts";
import { startWhiteboard, type Whiteboard, type WhiteboardOptions } from "./whiteboard.ts";

export interface JocastaServer {
	port: number;
	/** The API token: the bearer of every call (`jocasta-service.json`); never a cookie or in a URL (SEC-NET-4). */
	token: string;
	/** The collab relay's port on 127.0.0.1 (0561). */
	relayPort: number;
	/**
	 * A browser page's URL, `http://127.0.0.1:<port>/?t=<code>`, with a fresh one-time launch code each time it is read
	 * (SEC-LOC-11): used once within a minute, it becomes the page cookie.
	 */
	readonly url: string;
	service: JocastaService;
	/** The service closes and `jocasta-service.json` goes away; herdr and every pane keep running (a failed start). */
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
	 * answer comes through `POST /api/popup/<code>`. Every open one ends unanswered when the server stops. `fromKey`: one
	 * of his keys opened it (no notification; Allow questions go first, 0578, 0581).
	 */
	question(view: PopupView, signal?: AbortSignal, options?: { fromKey?: boolean }): Popup;
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
	/** Told each herdr server process this service starts (0582: a hung service's takeover spares it). */
	onHerdrServer?: (server: ProcessId) => void;
	/** The knowledge graph (test seam): Graphiti's by default; null for none, so tests never start Docker. */
	graph?: Graph | null;
	/** The code index behind `graph_search` (0562; test seam): codebase-memory-mcp under `<data>/code-index` by default; null for none. */
	codeIndex?: CodeIndex | null;
	/** Phase 8's pollers and clock (test seam); tests pass `{ pollers: false }` so nothing calls the real gh or az. */
	day?: ServiceOptions["day"];
	/** The port to try first (test seam): `service.port`, else 47400 (0358). */
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
	/** Whether this is Linux with its own Docker engine, whose containers can't reach the model callback (SEC-LOC-10). */
	nativeDocker(): boolean;
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
	nativeDocker: () => nativeLinuxDocker(),
};

/**
 * Graphiti on the services row (0444): "up" while it answers; "starting" while Jocasta starts it; "idle · starts when
 * needed" while nothing listens and Docker runs (it starts on first use and stops after 15 min idle, 0073); "down ·
 * Docker isn't running"; "down · not answering" when it listens but `/health` isn't OK. On Linux with its own Docker
 * engine it is down with why, before anything else: its container can't call back for its model calls (SEC-LOC-10).
 */
export async function graphitiHealth(checks: GraphitiChecks = GRAPHITI_CHECKS): Promise<Omit<ServiceHealth, "name">> {
	if (checks.nativeDocker()) return { state: "down", note: CALLBACK_UNREACHABLE };
	const health = await checks.health();
	if (health === "ok") return { state: "up" };
	if (checks.starting()) return { state: "starting" };
	if (health === "bad") return { state: "down", note: "not answering" };
	return checks.dockerRunning()
		? { state: "idle", note: "starts when needed" }
		: { state: "down", note: "Docker isn't running" };
}

/** `service.port`'s default (0358). */
export const DEFAULT_PORT = 47_400;

/**
 * `service.port` in `jocasta.yaml` (0358, 0387; `frontier.port` still read, 0584), checked by the settings schema: a
 * port number, else 47400. Never throws (0584): a malformed file or a bad value is a settings problem the service
 * notifies once at start, and the start goes on with the default.
 */
function configuredPort(): number {
	try {
		return serviceSetting(readSettings(), "port") ?? DEFAULT_PORT;
	} catch {
		return DEFAULT_PORT;
	}
}

/** The options that pass through to the service: all but the server's own. */
function passedOn(options: ServerOptions) {
	const {
		relayPort,
		herdr,
		herdrSession,
		onHerdrServer,
		graph,
		codeIndex,
		port,
		whiteboard,
		onQuestion,
		plugin,
		reattach,
		stopContainers,
		stopBoundsMs,
		...passed
	} = options;
	return passed;
}

/** herdr's `session`, started with `~/.jocasta/agent/herdr.toml` when absent; `onServerStarted` told its server. */
function connectSession(session: string, onServerStarted?: (server: ProcessId) => void): Promise<Herdr> {
	const configPath = join(configDir(), "herdr.toml");
	return connectHerdr({
		session,
		...(existsSync(configPath) ? { configPath } : {}),
		...(onServerStarted ? { onServerStarted } : {}),
	});
}

/**
 * His Allow/Deny before a risky boss action (0484), Deny by default and answered only with its one-time code. When the
 * answer isn't wanted any more (the wait ran out, Jocasta is closing), it goes as Deny. How it ended tells the boss the
 * true reason of a Deny (0578).
 */
function allowBox(question: Ask): NonNullable<ServiceOptions["confirm"]> {
	return async (ask, signal) => {
		const popup = question(allowQuestion(ask), signal);
		const answer = await popup.answer;
		return { allow: answer === ALLOW, how: await popup.how };
	};
}

/** The dashboard's services row (0429, 0444): Graphiti and whiteboard, each up or why not. */
async function servicesRow(graph: Graph | undefined, whiteboard: Whiteboard | undefined): Promise<ServiceHealth[]> {
	const off: Omit<ServiceHealth, "name"> = { state: "down", note: "off in this run" };
	return [
		{ name: "Graphiti", ...(graph ? await graphitiHealth() : off) },
		{ name: "whiteboard", ...(whiteboard ? await whiteboard.check() : off) },
	];
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

/**
 * The Jocasta service's server (4.1, 0392, 0552): 127.0.0.1 on `service.port` (0358), or a free port for this run
 * when that one is busy; `<data>/jocasta-service.json` with `port`, `token` and `pid`. Every API call carries the
 * token as a bearer (`jocasta mcp`, bare `jocasta`, the plugin's panes); the browser's pages carry a separate page
 * token as a per-port cookie, set from a one-time launch code (`/?t=<code>`, SEC-NET-4, SEC-LOC-11). It serves the MCP
 * tools, the state feed, his questions, whiteboard's canvas page (0393) and Graphiti's `/llm/v1` (0390), and runs the
 * herdr-backed service. A start that fails stops what it had started, its relay included (SRV-A-2).
 */
export async function startServer(options: ServerOptions = {}): Promise<JocastaServer> {
	// The calendar's secret leaves this process's environment before anything is spawned (SEC-5, 0430).
	takeCalendarEnv();
	const token = randomBytes(32).toString("hex");
	// The browser's own token (SEC-NET-4): its cookie authorizes the pages and whiteboard's connection only.
	const pageToken = randomBytes(32).toString("hex");
	// The collab relay every omp pane hosts its session in (0536, 0561), first: a port another program holds stops the
	// start before herdr or any pane starts. It stops with Jocasta, and with a failed start.
	const relay = startRelay(options.relayPort ?? RELAY_PORT);
	/** What a failed start undoes, each registered as its resource starts, run newest first (SRV-A-2). */
	const undo: StopStep[] = [{ name: "the collab relay", run: () => relay.stop() }];
	try {
		return await startRest();
	} catch (error) {
		await undoStart(undo);
		throw error;
	}

	async function startRest(): Promise<JocastaServer> {
		let server: Server<undefined> | undefined;
		const broadcast = (message: ServerMessage) => publish(server, message);
		const popups = new Popups();
		const session = options.herdrSession ?? herdrSession();
		const herdr = options.herdr ?? (await connectSession(session, options.onHerdrServer));
		undo.push({ name: "herdr's connection", run: () => herdr.close() });
		const plugin = (options.plugin ?? options.herdr === undefined) ? new PluginPanes(herdr) : undefined;
		const askPopups = plugin ? new AskPopups(herdr) : undefined;
		/** What shows his questions: the test's, else the plugin's popup once herdr is connected (0553). */
		const question = asker(popups, options.onQuestion ?? (askPopups ? (popup) => askPopups.show(popup) : undefined));
		const graph = options.graph === undefined ? graphiti() : (options.graph ?? undefined);
		// The code index (0562): its stdio servers start on the first relationship question and stop with Jocasta.
		const codeIndex =
			options.codeIndex === undefined ? new CodeIndex({ dataDir: dataDir() }) : (options.codeIndex ?? undefined);
		// whiteboard's server, the service's child (0393): it starts beside the service and stops with Jocasta.
		const whiteboard = options.whiteboard === null ? undefined : startWhiteboard(options.whiteboard);
		if (whiteboard) undo.push({ name: "whiteboard", run: () => whiteboard.stop() });
		let port = 0;
		const codes = new LaunchCodes();
		/** A page URL with a fresh launch code; `query` goes after it (`&review=…`). */
		const pageUrl = (query = "") => `http://127.0.0.1:${port}/?${TOKEN_QUERY}=${codes.mint()}${query}`;
		const catalog = openCatalog();
		undo.push({ name: "the catalog", run: () => catalog.close() });
		const passed = passedOn(options);
		const service = new JocastaService(catalog, {
			...passed,
			herdr,
			broadcast,
			plugin,
			graph,
			codeIndex,
			relayPort: relay.port,
			confirm: passed.confirm ?? allowBox(question),
			services: () => servicesRow(graph, whiteboard),
			// The boss's `show` on this server's canvas page (0552), unless the test's.
			...(whiteboard ? whiteboardReviews(whiteboard, (query) => openInBrowser(pageUrl(query))) : {}),
			...(passed.findReview ? { findReview: passed.findReview } : {}),
			...(passed.openReview ? { openReview: passed.openReview } : {}),
		});
		// Its stop closes what it opened (the catalog, the day's stores) and herdr's connection.
		undo.push({ name: "the service", run: () => service.stop(), ms: 30_000 });
		const llm = llmCallback();
		const stops = stopper((extraStops) =>
			stopSteps({
				popups,
				broadcast,
				herdr,
				session,
				service,
				stopContainers: () => (options.stopContainers ?? stopJocastaContainers)(),
				extraStops,
				whiteboard,
				relay,
				endLlmCallback: llm.end,
				server: () => server,
				bounds: options.stopBoundsMs,
			}),
		);
		if (codeIndex) stops.add({ name: "the code index", run: () => codeIndex.stop(), ms: 10_000, when: "all" });
		const stopping = () => stops.stopping;
		const close = closeFlow({
			service,
			question,
			stopping,
			stopAll: () => stops.stop(true),
			reattach: options.reattach,
		});

		const popupRoute = popupRoutes({ token, popups, askPopups, service });
		const routes: Route[] = [
			...doorRoutes({ token, service, clientClosed: () => void close({ ask: true, reattach: true }) }),
			actionRoute({
				token,
				service,
				ask: question,
				askPopups,
				broadcast,
				quit: () => close({ ask: true, reattach: false }),
				waitMs: passed.confirmWaitMs ?? CONFIRM_WAIT_MS,
			}),
			popupRoute.hold,
			llm.route,
			feedRoute(token, guarded),
		];
		const pages = pageResponder({
			token,
			pageToken,
			codes,
			port: () => port,
			whiteboard,
			bearerRoutes: [healthRoute(token, () => stopping() !== undefined), popupRoute.question],
		});
		server = listen(options.port ?? configuredPort(), {
			routes,
			page: async (request, url, at) => guarded(await pages(request, url), at, whiteboard?.origin()),
			websocket: feedSocket(service),
		});
		const listening = server;
		undo.push({ name: "the server", run: () => listening.stop(true) });
		port = server.port as number;
		llm.listening(port);
		undo.push({ name: "the model callback", run: llm.end });
		undo.push({ name: "jocasta-service.json", run: () => removeServiceFile(process.pid) });
		writeServiceFile({ port, token, pid: process.pid });
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
			get url() {
				return pageUrl();
			},
			service,
			stop: () => stops.stop(),
			shutdown: ({ ask }) => close({ ask, reattach: false }),
			clientClosed: () => close({ ask: true, reattach: true }),
			stopsWith: (name, run, ms = 10_000) => stops.add({ name, run, ms, when: "all" }),
			stopped: stops.stopped,
			ready,
			whiteboard,
			question,
		};
	}
}
