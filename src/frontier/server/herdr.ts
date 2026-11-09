/**
 * The one module that talks to herdr (principle 7; 0392). herdr 0.9.x's socket API: newline-delimited JSON over a named
 * pipe on Windows (a Unix socket elsewhere); a `{id, method, params}` request line is answered by a line echoing `id`
 * with `result` or `error {code, message}`; `events.subscribe` keeps its connection open and pushes bare
 * `{event, data}` lines (herdr `docs/next/website/src/content/docs/socket-api.mdx` "Socket transport", "Event
 * subscriptions"; schema `docs/next/api/herdr-api.schema.json`).
 */
import { spawn } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { createConnection, type Socket } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { LineSplitter } from "../../shared/line-rpc.ts";
import { log } from "../../shared/log.ts";
import { waitFor } from "../../shared/poll.ts";
import { onPath } from "../../shared/process.ts";
import { endProcesses, type ProcessId, processIdOf } from "../../shared/proctree.ts";

interface HerdrOptions {
	/** herdr session name; Jocasta's is `jocasta`. */
	session: string;
	/** `HERDR_CONFIG_PATH` for a server this module starts (`~/.jocasta/agent/herdr.toml`). */
	configPath?: string;
	/** The herdr executable; `findHerdr()` when omitted. */
	herdrBin?: string;
	/** Socket path override (tests); `herdrSocketPath(session)` when omitted. */
	socketPath?: string;
	/** Where warnings go (the version warning, a restarted server); Jocasta's log (`log.ts`) when omitted. */
	onWarning?: (message: string) => void;
	/** Per-request timeout; 30 s when omitted. */
	timeoutMs?: number;
	/** How often `watch` re-reads its pane set; 1 s when omitted. */
	watchIntervalMs?: number;
	/**
	 * Whether `connectHerdr` starts a server when none answers (and again after one stopped, 0431); true when omitted.
	 * False connects to a running one only (the plugin's startup hook): no answer fails with `unreachable`.
	 */
	start?: boolean;
	/** Told the server process each time this module starts one (0582: a hung service's takeover spares it). */
	onServerStarted?: (server: ProcessId) => void;
}

export type AgentState = "working" | "blocked" | "idle" | "done" | "unknown";

export interface PaneInfo {
	paneId: string;
	tabId: string;
	workspaceId: string;
	cwd?: string;
	agentState: AgentState;
	/** The agent herdr detected or an integration reported in the pane (`omp` once omp is up). */
	agent?: string;
	/** The omp session file herdr's omp integration reported (`agent_session` of kind `path`). */
	sessionPath?: string;
	label?: string;
	/** herdr's terminal behind the pane: a new one each time herdr starts the pane's process (a restore, a respawn). */
	terminalId?: string;
	/** The one pane herdr has focused (its workspace, tab and pane), as `pane.list` says (0484). */
	focused?: true;
}

/** herdr's workspaces and panes read together, once (the refresh tick's view). */
export interface HerdrSnapshot {
	workspaces: { id: string; label: string; cwd?: string }[];
	panes: PaneInfo[];
}

/** herdr's answers to `notification.show` (`NotificationShowReason`). */
const NOTIFICATION_RESULTS = ["shown", "disabled", "rate_limited", "no_foreground_client", "busy"] as const;
export type NotificationResult = (typeof NOTIFICATION_RESULTS)[number];

export interface Herdr {
	/** `connectHerdr` started this session's server itself (none answered): Jocasta created the session (0495). */
	readonly startedServer: boolean;
	/** herdr's server version (`ping`); throws when the server doesn't answer. */
	ping(): Promise<string | undefined>;
	/**
	 * Workspace id of the workspace labelled `label`; creates it in `cwd` when missing, its one tab running `command`
	 * (with `env`) when given (else the default shell).
	 */
	ensureWorkspace(label: string, cwd: string, command?: string[], env?: Record<string, string>): Promise<string>;
	workspaces(): Promise<{ id: string; label: string; cwd?: string }[]>;
	/** Workspaces and panes from one `workspace.list` and one `pane.list`. */
	snapshot(): Promise<HerdrSnapshot>;
	/**
	 * Closes a workspace with its panes (`workspace.close`) and ends every process they ran, with all they started
	 * (0486), so nothing holds its folder.
	 */
	closeWorkspace(workspaceId: string): Promise<void>;
	createTab(
		workspaceId: string,
		tab: { title: string; cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }>;
	/**
	 * A git worktree for `branch` at `path` (a new branch starts from `base`, else HEAD) and a pane running `command`
	 * in it. herdr opens every worktree as its own workspace, grouped under the repo's workspace, so the result names
	 * that workspace too. `prepare` runs once the checkout exists and before the pane starts; what it returns joins the
	 * pane's env (0457, 0473).
	 */
	createWorktree(
		workspaceId: string,
		opts: {
			repo: string;
			branch: string;
			base?: string;
			path: string;
			title: string;
			command: string[];
			env?: Record<string, string>;
			prepare?(path: string): Promise<Record<string, string>>;
		},
	): Promise<{ workspaceId: string; tabId: string; paneId: string; path: string }>;
	/** Moves a workspace to `index` in herdr's list (0 is the first, Alt+1). */
	moveWorkspace(workspaceId: string, index: number): Promise<void>;
	/**
	 * Replaces the pane's tab with one pane running `command` (the tab keeps its label): a pane herdr restored as a
	 * shell runs omp again. Returns the new pane.
	 */
	respawn(
		paneId: string,
		opts: { cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }>;
	/** A new pane running `command` beside `paneId` in its tab, named `label` when given; returns the new pane id. */
	splitBeside(
		paneId: string,
		opts: {
			command: string[];
			cwd: string;
			env?: Record<string, string>;
			label?: string;
			ratio?: number;
			direction?: "right" | "down";
		},
	): Promise<string>;
	/**
	 * Submits `text` to the pane's agent: a bracketed paste, then Enter. ESC and other control characters are removed
	 * first, so the text can't end the paste early and type keys of its own.
	 */
	prompt(paneId: string, text: string): Promise<void>;
	/** Types `text` into the pane without submitting it (control characters removed, as for `prompt`). */
	sendText(paneId: string, text: string): Promise<void>;
	/** One Escape. */
	stop(paneId: string): Promise<void>;
	/** One Enter: submits what sits in the pane's input (a prompt whose paste landed but whose Enter didn't). */
	submit(paneId: string): Promise<void>;
	/** Keys by herdr's key names (`f9`, `enter`) to the pane, as typed (`pane.send_keys`). */
	sendKeys(paneId: string, keys: string[]): Promise<void>;
	/**
	 * A Windows notification through herdr's client (`notification.show`, 0554): title and body as given, with
	 * herdr's `request` sound or none. Resolves herdr's reason: `shown`, or why not.
	 */
	notify(note: { title: string; body?: string; sound: "request" | "none" }): Promise<NotificationResult>;
	focus(paneId: string): Promise<void>;
	/**
	 * Opens one of a herdr plugin's panes (`plugin.pane.open`, 0553): `split` beside `targetPaneId` resolves the new
	 * pane's id; `popup` (no pane id) resolves undefined. `env` reaches the launched process.
	 */
	openPluginPane(opts: {
		pluginId: string;
		entrypoint: string;
		placement: "split" | "popup";
		targetPaneId?: string;
		direction?: "right" | "down";
		cwd?: string;
		env?: Record<string, string>;
		focus?: boolean;
	}): Promise<string | undefined>;
	/** Closes the session's open popup (`popup.close`); false when none is open. */
	closePopup(): Promise<boolean>;
	/** Sets a tab's top split: `ratio` of it for the first side (`layout.set_split_ratio`, path `[]`). */
	setSplitRatio(tabId: string, ratio: number): Promise<void>;
	/** Installs the Agents view's filter and sort for `source` (`agent.view.set`, 0553). */
	setAgentView(view: { source: string; label?: string; filter?: object; sort?: object[] }): Promise<void>;
	/** What the pane shows now, as plain text (`pane.read`, its visible viewport). */
	screen(paneId: string): Promise<string>;
	/**
	 * Display-only tokens and state labels; `ttlMs` makes herdr drop them when they aren't reported again in time, so a
	 * stopped service leaves nothing stale.
	 */
	reportPaneMetadata(
		paneId: string,
		meta: {
			tokens?: Record<string, string | null>;
			stateLabels?: Partial<Record<AgentState, string>>;
			ttlMs?: number;
		},
	): Promise<void>;
	reportWorkspaceMetadata(workspaceId: string, tokens: Record<string, string | null>, ttlMs?: number): Promise<void>;
	panes(): Promise<PaneInfo[]>;
	/**
	 * The process herdr started for the pane (`pane.process_info`'s `shell_pid`; omp in Jocasta's panes), undefined when
	 * herdr names none. Never starts the server; throws when it doesn't answer within `timeoutMs` (the client's own
	 * timeout when omitted).
	 */
	paneRoot(paneId: string, timeoutMs?: number): Promise<number | undefined>;
	/**
	 * Calls `onChange` when a watched pane's agent state changes, starting with each pane's current state. The pane set
	 * is re-read from `paneIds` every interval; the subscription is reopened when it changes or drops (`events_lost`,
	 * herdr restarted). Returns the function that stops watching.
	 */
	watch(
		paneIds: () => string[],
		onChange: (event: { paneId: string; workspaceId: string; state: AgentState }) => void,
	): () => void;
	/**
	 * From now on a stopped server stays stopped: no request starts it again (0431's restart never runs during a stop,
	 * 0434). Requests to a running server still work.
	 */
	keepDown(): void;
	/**
	 * Stops this session's server with its panes (`server.stop`; only this session, 0434) and waits for it to go; never
	 * starts it again; then ends every process the panes ran, with all they started (0486). Does nothing when it isn't
	 * running.
	 */
	stopServer(): Promise<void>;
	/** Stops every watch. Requests hold no connection between calls. */
	close(): void;
}

/**
 * A herdr error response (`{id, error: {code, message}}`), or a local failure: `unreachable` (no server on the socket),
 * `transport` (closed before answering), `timeout`.
 */
export class HerdrError extends Error {
	constructor(
		readonly code: string,
		message: string,
	) {
		super(message);
		this.name = "HerdrError";
	}
}

/** Shown in herdr's metadata as the reporter (`source` must match `[A-Za-z0-9:._-]{1,80}`). */
const SOURCE = "jocasta";
const SERVER_START_MS = 10_000;
/** Fewest milliseconds between two restarts of a stopped server (0431). */
const RESTART_GAP_MS = 5_000;
/** How long a stopped server may take to close its socket (0434). */
const SERVER_STOP_MS = 10_000;
/** A ping while stopping: a server on its way out answers at once or not at all. */
export const STOP_PING_MS = 2_000;
/** How many panes' processes `paneProcesses` asks herdr for at once (SRV-A-8). */
const PANE_READS_AT_ONCE = 4;
const DEFAULT_TIMEOUT_MS = 30_000;
/** `worktree.create` runs `git worktree add` before it answers. */
const WORKTREE_TIMEOUT_MS = 120_000;
const SUPPORTED_VERSION = /^0\.9\./;

/**
 * herdr's socket for `session`. herdr's config dir is `$XDG_CONFIG_HOME/herdr`, else `%APPDATA%\herdr` on Windows and
 * `~/.config/herdr` elsewhere (herdr `src/config/io.rs:30-66`); a named session's socket is
 * `<config dir>/sessions/<name>/herdr.sock` (`src/session.rs:163-173`).
 */
export function herdrSocketPath(session: string): string {
	const xdg = process.env.XDG_CONFIG_HOME;
	const configDir = xdg
		? join(xdg, "herdr")
		: process.platform === "win32"
			? join(process.env.APPDATA ?? join(homedir(), "AppData", "Roaming"), "herdr")
			: join(homedir(), ".config", "herdr");
	return herdrPipe(join(configDir, "sessions", session, "herdr.sock"));
}

/**
 * What a client connects to for herdr's socket file `file` (`herdrSocketPath`'s, or the `HERDR_SOCKET_PATH` herdr gives
 * its plugin commands): on Windows herdr binds that path string as a namespaced pipe name, so clients connect to
 * `\\.\pipe\<path>` (`src/ipc.rs:44-79`); the file itself elsewhere.
 */
export function herdrPipe(file: string): string {
	return process.platform === "win32" ? `\\\\.\\pipe\\${file}` : file;
}

/** `herdr` on PATH, else the newest `~/.herdr/packages/standalone/releases/<version>-<target>/herdr[.exe]`. */
export function findHerdr(): string | undefined {
	const found = onPath("herdr");
	if (found) return found;
	const releases = join(homedir(), ".herdr", "packages", "standalone", "releases");
	if (!existsSync(releases)) return undefined;
	const exe = process.platform === "win32" ? "herdr.exe" : "herdr";
	const versions = readdirSync(releases)
		.filter((name) => existsSync(join(releases, name, exe)))
		.sort((a, b) => compareVersions(b, a));
	return versions[0] === undefined ? undefined : join(releases, versions[0], exe);
}

function compareVersions(a: string, b: string): number {
	const parts = (name: string) => (name.split("-")[0] ?? "").split(".").map((n) => Number.parseInt(n, 10) || 0);
	const [x, y] = [parts(a), parts(b)];
	for (let i = 0; i < Math.max(x.length, y.length); i++) {
		const diff = (x[i] ?? 0) - (y[i] ?? 0);
		if (diff !== 0) return diff;
	}
	return 0;
}

/** The warning for a herdr server version other than 0.9.x, or undefined. */
export function herdrVersionWarning(version: string): string | undefined {
	return SUPPORTED_VERSION.test(version)
		? undefined
		: `herdr ${version} is running; Jocasta is built against herdr 0.9.x and may misbehave`;
}

/**
 * Connects to herdr's session `options.session`, starting `herdr --session <session> server` detached with
 * `HERDR_CONFIG_PATH` when its socket is absent and waiting up to 10 s for it (unless `start` is false). Warns if the
 * server's version (from `ping`, checked once here) is not 0.9.x. When the server stops later, the next request starts
 * it again the same way (0431), at most once every few seconds; herdr restores its panes as shells, which the service
 * relaunches.
 */
export async function connectHerdr(options: HerdrOptions): Promise<Herdr> {
	const socketPath = options.socketPath ?? herdrSocketPath(options.session);
	// Into Jocasta's log (0431), which a source run also prints.
	const warn = options.onWarning ?? ((message: string) => log("warn", "herdr", message));
	const client = new Client(socketPath, options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
	const start = options.start ?? true;
	let pong: { version?: string };
	let started = false;
	try {
		pong = await client.ping();
	} catch (error) {
		if (!start || !(error instanceof HerdrError && error.code === "unreachable")) throw error;
		pong = await startAndWait(client, options);
		started = true;
	}
	const warning = pong.version === undefined ? undefined : herdrVersionWarning(pong.version);
	if (warning) warn(warning);
	let lastStart = 0;
	if (start) {
		client.revive = async () => {
			if (Date.now() - lastStart < RESTART_GAP_MS) {
				throw new HerdrError("unreachable", `herdr session ${options.session} is down; it was restarted moments ago`);
			}
			lastStart = Date.now();
			await startAndWait(client, options);
			revivedAt = Date.now();
			revivals++;
			warn(`herdr's server for session ${options.session} had stopped; Jocasta started it again`);
		};
	}
	return new HerdrClient(client, options.watchIntervalMs ?? 1000, started);
}

let revivedAt: number | undefined;
let revivals = 0;

/**
 * When this process last started a stopped herdr server again (0431), or undefined: right after it, herdr may list no
 * panes yet while it restores them.
 */
export function lastRevivedAt(): number | undefined {
	return revivedAt;
}

/**
 * How many times this process started a stopped herdr server again (0431). Bare `jocasta` compares it across its
 * client's run (`/health`'s `herdrRevivals`, 0534): a client that exited while it grew lost a crashed server, not him.
 */
export function herdrRevivals(): number {
	return revivals;
}

/**
 * Stops session `session`'s server with its panes (bare `jocasta` closing while no service runs, 0434); never starts
 * one. Resolves true when one was running.
 */
export function stopHerdrSession(
	options: Pick<HerdrOptions, "session" | "socketPath" | "timeoutMs">,
): Promise<boolean> {
	const path = options.socketPath ?? herdrSocketPath(options.session);
	return new Client(path, options.timeoutMs ?? DEFAULT_TIMEOUT_MS).stopServer();
}

/**
 * Whether session `session`'s server answers a `ping`; never starts one. Bare `jocasta` polls it while the service
 * starts a dead server again (0518).
 */
export async function herdrSessionAnswers(
	options: Pick<HerdrOptions, "session" | "socketPath" | "timeoutMs">,
): Promise<boolean> {
	const path = options.socketPath ?? herdrSocketPath(options.session);
	try {
		await new Client(path, options.timeoutMs ?? DEFAULT_TIMEOUT_MS).ping();
		return true;
	} catch {
		return false;
	}
}

/**
 * One request to session `session`'s server, as the service's client sends it (a read herdr dropped is asked again
 * once, 0517); never starts a server. Throws `HerdrError`.
 */
export function herdrRequestOnce<T>(session: string, method: string, params: object, timeoutMs?: number): Promise<T> {
	const ms = timeoutMs ?? DEFAULT_TIMEOUT_MS;
	return new Client(herdrSocketPath(session), ms).request<T>(method, params, ms);
}

/** Starts the session's server and waits up to 10 s for its `ping`. */
async function startAndWait(client: Client, options: HerdrOptions): Promise<{ version?: string }> {
	const server = startServer(options);
	if (server) options.onServerStarted?.(server);
	let answer: { version?: string } | undefined;
	let last: unknown;
	const started = await waitFor(
		async () => {
			try {
				answer = await client.ping();
				return true;
			} catch (retry) {
				last = retry;
				return false;
			}
		},
		{ timeoutMs: SERVER_START_MS, intervalMs: 100 },
	);
	if (!started || !answer) {
		throw new HerdrError("unreachable", `herdr session ${options.session} didn't start: ${String(last)}`);
	}
	return answer;
}

/** Starts the session's server; resolves its process as it runs now, when that can be read. */
function startServer(options: HerdrOptions): ProcessId | undefined {
	const bin = options.herdrBin ?? findHerdr();
	if (!bin) throw new HerdrError("not_installed", "herdr isn't installed (not on PATH, not in ~/.herdr)");
	const env = { ...process.env };
	if (options.configPath) env.HERDR_CONFIG_PATH = options.configPath;
	// `herdr server` runs the headless server (cli-reference.mdx "Server"); `--session` picks the socket. It outlives
	// the service, so it starts in the home dir: inheriting the launcher's cwd would lock the install folder.
	const child = spawn(bin, ["--session", options.session, "server"], {
		cwd: homedir(),
		detached: true,
		stdio: "ignore",
		windowsHide: true,
		env,
	});
	child.unref();
	return child.pid === undefined ? undefined : processIdOf(child.pid);
}

/**
 * A connect error as herdr's state: no socket or pipe (ENOENT) or nothing listening (ECONNREFUSED) is no server
 * (`unreachable`, which may start one); anything else (a busy pipe, EMFILE, EACCES) is a `transport` failure, so a
 * live session never gets a second server (SRV-A-10).
 */
function connectError(path: string, error: unknown): HerdrError {
	const code = (error as NodeJS.ErrnoException | undefined)?.code;
	return code === "ENOENT" || code === "ECONNREFUSED"
		? new HerdrError("unreachable", `herdr isn't reachable at ${path}: ${String(error)}`)
		: new HerdrError("transport", `herdr's socket at ${path} didn't connect: ${String(error)}`);
}

/**
 * Opens one NDJSON connection: the socket at once, and `opened` once it connected (rejected with `connectError`'s
 * `HerdrError`). Every received line goes to `onLine`, and `onClose` runs when it drops after opening.
 */
function openLines(
	path: string,
	onLine: (line: string) => void,
	onClose: () => void,
): { socket: Socket; opened: Promise<void> } {
	const { promise, resolve, reject } = Promise.withResolvers<void>();
	const socket = createConnection(path);
	const lines = new LineSplitter();
	let opened = false;
	socket.setEncoding("utf8");
	socket.once("connect", () => {
		opened = true;
		resolve();
	});
	socket.on("data", (chunk: string) => {
		for (const line of lines.push(chunk)) if (line) onLine(line);
	});
	socket.on("error", (error) => {
		if (!opened) reject(connectError(path, error));
	});
	socket.on("close", () => {
		if (opened) onClose();
		else reject(connectError(path, new Error("closed before connecting")));
	});
	// A caller that gave up first (its timeout) destroyed the socket; the failure then has no one to tell.
	promise.catch(() => {});
	return { socket, opened: promise };
}

interface Response {
	id?: string;
	result?: unknown;
	error?: { code?: string; message?: string };
	event?: string;
	/** Read only for `pane.agent_status_changed` (schema subscription_event `PaneAgentStatusChangedEvent`). */
	data?: { pane_id?: string; workspace_id?: string; agent_status?: string };
}

let nextId = 0;

/**
 * The requests that only read herdr's state. herdr 0.9.5 sometimes closes an accepted connection without answering
 * (seen in the VM under bursts of thread starts and closes, 0517); one of these is then asked again once, while a
 * change never is: herdr may have made it.
 */
const READS: Record<string, true> = {
	ping: true,
	"workspace.list": true,
	"pane.list": true,
	"pane.get": true,
	"pane.read": true,
	"pane.process_info": true,
};

function parse(line: string): Response | undefined {
	try {
		return JSON.parse(line) as Response;
	} catch {
		return undefined;
	}
}

/**
 * Requests. herdr's API server reads one request line per connection, answers it and closes the connection (herdr
 * `src/api/server.rs:328-376`, `read_initial_request_line`); only `events.subscribe` keeps it open. So every request
 * opens its own connection. When no server listens and `revive` is set, it is called once (concurrent callers share
 * it) and the request tried again.
 */
class Client {
	/** Starts the stopped server (`connectHerdr`); unset while connecting. */
	revive: (() => Promise<void>) | undefined;
	private reviving: Promise<void> | undefined;

	constructor(
		readonly path: string,
		readonly timeoutMs: number,
	) {}

	/** ping → `{type: "pong", version, protocol}` (socket-api.mdx "Socket transport"); never revives. */
	ping(): Promise<{ version?: string }> {
		return this.attempt("ping", {}, this.timeoutMs);
	}

	async request<T>(method: string, params: object, timeoutMs = this.timeoutMs): Promise<T> {
		try {
			return await this.attempt<T>(method, params, timeoutMs);
		} catch (error) {
			if (!(error instanceof HerdrError && error.code === "unreachable") || !this.revive) throw error;
			this.reviving ??= this.revive().finally(() => {
				this.reviving = undefined;
			});
			await this.reviving;
			return this.attempt<T>(method, params, timeoutMs);
		}
	}

	/**
	 * `server.stop` (schema `EmptyParams`; what `herdr server stop` sends), then waits for the socket to go. Unsets
	 * `revive` first, so nothing starts the server again. Every process its panes ran, with all they started, is listed
	 * before and ended after (0486): omp's workers outlived the server and held a worktree. False when no server was
	 * running.
	 */
	async stopServer(): Promise<boolean> {
		this.revive = undefined;
		const gone = (error: unknown) => error instanceof HerdrError && error.code === "unreachable";
		try {
			await this.attempt("ping", {}, STOP_PING_MS);
		} catch (error) {
			if (gone(error)) return false;
			throw error;
		}
		const processes = await this.paneProcesses();
		try {
			await this.stopAndWait(gone);
		} finally {
			await endProcesses(processes);
		}
		return true;
	}

	private async stopAndWait(gone: (error: unknown) => boolean): Promise<void> {
		try {
			await this.attempt("server.stop", {}, this.timeoutMs);
		} catch (error) {
			// A server on its way out may close the connection before it answers.
			if (!gone(error) && !(error instanceof HerdrError && error.code === "transport")) throw error;
		}
		const stopped = await waitFor(
			async () => {
				try {
					await this.attempt("ping", {}, STOP_PING_MS);
					return false;
				} catch (error) {
					return gone(error);
				}
			},
			{ timeoutMs: SERVER_STOP_MS, intervalMs: 100 },
		);
		if (!stopped) {
			throw new HerdrError("timeout", `herdr's server at ${this.path} didn't stop in ${SERVER_STOP_MS / 1000} s`);
		}
	}

	/**
	 * The processes herdr started for the panes (those of workspace `workspaceId` or tab `tabId`, else every pane), each
	 * by pid and start, read while herdr still has the panes: each pane's root (`pane.process_info`'s `shell_pid`) and its
	 * foreground processes. `endProcesses` later ends them with everything they started, by the start-checked walk
	 * (SRV-A-1). At most 4 panes are asked at once (0517: herdr drops requests under bursts), and the panes whose
	 * processes couldn't be read are named in one warning (SRV-A-8). Never starts the server; empty when herdr can't say.
	 */
	async paneProcesses(where: { workspaceId?: string; tabId?: string } = {}): Promise<ProcessId[]> {
		let panes: WirePane[];
		try {
			({ panes } = await this.attempt<{ panes: WirePane[] }>("pane.list", {}, this.timeoutMs));
		} catch (error) {
			log("warn", "herdr", `the panes' processes weren't listed: ${(error as Error).message}`);
			return [];
		}
		const asked = panes.filter(
			(pane) =>
				(where.workspaceId === undefined || pane.workspace_id === where.workspaceId) &&
				(where.tabId === undefined || pane.tab_id === where.tabId),
		);
		const pids: number[] = [];
		const unread: string[] = [];
		let next = 0;
		const worker = async () => {
			for (let pane = asked[next++]; pane !== undefined; pane = asked[next++]) {
				try {
					const info = await this.processInfo(pane.pane_id);
					pids.push(info.shell_pid ?? 0, ...(info.foreground_processes ?? []).map((p) => p.pid));
				} catch {
					unread.push(pane.pane_id);
				}
			}
		};
		await Promise.all(Array.from({ length: Math.min(PANE_READS_AT_ONCE, asked.length) }, worker));
		if (unread.length > 0) {
			log("warn", "herdr", `the processes of pane${unread.length === 1 ? "" : "s"} ${unread.join(", ")} weren't read`);
		}
		return [...new Set(pids.filter((pid) => pid > 0))].flatMap((pid) => processIdOf(pid) ?? []);
	}

	/** One pane's processes, within `timeoutMs` (the client's own by default); never starts the server. */
	processInfo(paneId: string, timeoutMs = this.timeoutMs): Promise<WireProcessInfo> {
		// pane.process_info {pane_id} → `{type: "pane_process_info", process_info}` (schema `PaneProcessInfo`).
		return this.attempt<{ process_info: WireProcessInfo }>("pane.process_info", { pane_id: paneId }, timeoutMs).then(
			(answer) => answer.process_info,
		);
	}

	/**
	 * A connection to the server (for `events.subscribe`) opened the same way a request is: the socket at once, so a
	 * caller that gives up can destroy it, and `opened` once it connected.
	 */
	connect(onLine: (line: string) => void, onClose: () => void): { socket: Socket; opened: Promise<void> } {
		return openLines(this.path, onLine, onClose);
	}

	/** A read herdr dropped before answering is asked again once (`READS`); anything else fails as it came. */
	private async attempt<T>(method: string, params: object, timeoutMs: number): Promise<T> {
		try {
			return await this.send<T>(method, params, timeoutMs);
		} catch (error) {
			if (!(error instanceof HerdrError && error.code === "transport") || !READS[method]) throw error;
			await Bun.sleep(100);
			return this.send<T>(method, params, timeoutMs);
		}
	}

	/** One request on its own connection: herdr answers it and closes. `timeoutMs` covers connecting too. */
	private async send<T>(method: string, params: object, timeoutMs: number): Promise<T> {
		const id = `jocasta_${++nextId}`;
		const answer = Promise.withResolvers<Response>();
		const timer = setTimeout(
			() => answer.reject(new HerdrError("timeout", `herdr didn't answer ${method} in ${timeoutMs} ms`)),
			timeoutMs,
		);
		const { socket, opened } = this.connect(
			(line) => {
				// An undecodable request is answered with an empty `id` (socket-api.mdx "Socket transport").
				const response = parse(line);
				if (response?.id === id || response?.id === "") answer.resolve(response);
			},
			() => answer.reject(new HerdrError("transport", `herdr closed the connection before answering ${method}`)),
		);
		try {
			await Promise.race([opened, answer.promise]);
			socket.write(`${JSON.stringify({ id, method, params })}\n`);
			const response = await answer.promise;
			if (response.error) {
				throw new HerdrError(response.error.code ?? "error", response.error.message ?? "herdr error");
			}
			return response.result as T;
		} finally {
			clearTimeout(timer);
			socket.destroy();
		}
	}
}

// Wire shapes (schema success_response $defs), only the fields used here.
interface WirePane {
	pane_id: string;
	tab_id: string;
	workspace_id: string;
	cwd?: string | null;
	agent_status?: string;
	agent?: string | null;
	agent_session?: { kind?: string; value?: string } | null;
	label?: string | null;
	terminal_id?: string | null;
	focused?: boolean;
}
/** `pane.process_info`'s `process_info` (schema `PaneProcessInfo`; `shell_pid` is the process herdr started). */
interface WireProcessInfo {
	shell_pid?: number | null;
	foreground_processes?: { pid: number }[];
}
interface WireWorkspace {
	workspace_id: string;
	label: string;
}
interface WireLayoutNode {
	type: "pane" | "split";
	pane_id?: string | null;
	first?: WireLayoutNode;
	second?: WireLayoutNode;
}
interface WireLayout {
	workspace_id: string;
	tab_id: string;
	root: WireLayoutNode;
}

const STATES: Record<AgentState, true> = { working: true, blocked: true, idle: true, done: true, unknown: true };
const agentState = (status: string | undefined): AgentState =>
	status !== undefined && Object.hasOwn(STATES, status) ? (status as AgentState) : "unknown";

function paneInfo(pane: WirePane): PaneInfo {
	const info: PaneInfo = {
		paneId: pane.pane_id,
		tabId: pane.tab_id,
		workspaceId: pane.workspace_id,
		agentState: agentState(pane.agent_status),
	};
	if (pane.cwd) info.cwd = pane.cwd;
	if (pane.agent) info.agent = pane.agent;
	if (pane.agent_session?.kind === "path" && pane.agent_session.value) info.sessionPath = pane.agent_session.value;
	if (pane.label) info.label = pane.label;
	if (pane.terminal_id) info.terminalId = pane.terminal_id;
	if (pane.focused) info.focused = true;
	return info;
}

function firstPaneId(node: WireLayoutNode): string | undefined {
	if (node.type === "pane") return node.pane_id ?? undefined;
	return (node.first && firstPaneId(node.first)) ?? (node.second && firstPaneId(node.second));
}

/** A layout tree of one pane running `command` (`layout.apply` `LayoutNode` of type `pane`: argv, cwd, env). */
function commandPane(cwd: string, command: string[], env?: Record<string, string>): object {
	return { type: "pane", cwd, command, ...(env ? { env } : {}) };
}

/**
 * `text` for a pane's input with ESC, the other C0 controls (tab and newline kept), DEL and the C1 controls removed:
 * herdr writes it inside a bracketed paste as it is (herdr `src/pane.rs:3708-3715`), so an ESC `[201~` in outside text
 * would end the paste and type the rest as keys (SEC-2).
 */
export function paneText(text: string): string {
	let clean = "";
	for (const char of text) {
		const code = char.charCodeAt(0);
		if (code === 0x09 || code === 0x0a || (code >= 0x20 && code < 0x7f) || code > 0x9f) clean += char;
	}
	return clean;
}

/**
 * `value` with `paneText`'s control characters removed from every string in it (N-3): what a popup, `today` and a
 * notification get can't carry terminal escapes from a title, a brief or a question (SEC-NET-3).
 */
export function cleanText<T>(value: T): T {
	if (typeof value === "string") return paneText(value) as T;
	if (Array.isArray(value)) return value.map(cleanText) as T;
	if (value && typeof value === "object") {
		return Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, cleanText(inner)])) as T;
	}
	return value;
}

function workspaceList(workspaces: WireWorkspace[], panes: WirePane[]): HerdrSnapshot["workspaces"] {
	// WorkspaceInfo has no cwd; each workspace's first pane from pane.list supplies it.
	return workspaces.map((ws) => {
		const cwd = panes.find((pane) => pane.workspace_id === ws.workspace_id)?.cwd;
		return cwd ? { id: ws.workspace_id, label: ws.label, cwd } : { id: ws.workspace_id, label: ws.label };
	});
}

class HerdrClient implements Herdr {
	private readonly watches = new Set<() => void>();

	constructor(
		private readonly client: Client,
		private readonly watchIntervalMs: number,
		readonly startedServer: boolean,
	) {}

	async ping(): Promise<string | undefined> {
		return (await this.client.ping()).version;
	}

	async workspaces(): Promise<{ id: string; label: string; cwd?: string }[]> {
		return (await this.snapshot()).workspaces;
	}

	async snapshot(): Promise<HerdrSnapshot> {
		// workspace.list → `{type: "workspace_list", workspaces}`; pane.list across all workspaces.
		const [{ workspaces }, panes] = await Promise.all([
			this.client.request<{ workspaces: WireWorkspace[] }>("workspace.list", {}),
			this.wirePanes(),
		]);
		return { workspaces: workspaceList(workspaces, panes), panes: panes.map(paneInfo) };
	}

	async closeWorkspace(workspaceId: string): Promise<void> {
		// Listed while herdr still has the panes; ended once it closed them (0486).
		const processes = await this.client.paneProcesses({ workspaceId });
		// workspace.close {workspace_id} → `{type: "ok"}` (schema `WorkspaceCloseParams`).
		await this.client.request("workspace.close", { workspace_id: workspaceId });
		await endProcesses(processes);
	}

	async ensureWorkspace(label: string, cwd: string, command?: string[], env?: Record<string, string>): Promise<string> {
		const { workspaces } = await this.client.request<{ workspaces: WireWorkspace[] }>("workspace.list", {});
		const existing = workspaces.find((ws) => ws.label === label);
		if (existing) return existing.workspace_id;
		// workspace.create {cwd, label, focus} → `{type: "workspace_created", workspace, tab, root_pane}`.
		const created = await this.client.request<{ workspace: WireWorkspace; tab: { tab_id: string } }>(
			"workspace.create",
			{ cwd, label, focus: false },
		);
		// Its root tab runs the default shell; layout.apply replaces it with one running `command`.
		if (command) {
			await this.replaceTab(created.tab.tab_id, {
				tab_label: label,
				focus: false,
				root: commandPane(cwd, command, env),
			});
		}
		return created.workspace.workspace_id;
	}

	async createTab(
		workspaceId: string,
		tab: { title: string; cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }> {
		// tab.create starts the default shell only; layout.apply {workspace_id, tab_label, root} creates a tab whose pane
		// runs an argv `command` → `{type: "layout_apply", layout: {tab_id, root}}` (socket-api.mdx "layout.apply").
		return this.applyLayout({
			workspace_id: workspaceId,
			tab_label: tab.title,
			focus: false,
			root: commandPane(tab.cwd, tab.command, tab.env),
		});
	}

	async createWorktree(
		workspaceId: string,
		opts: {
			repo: string;
			branch: string;
			base?: string;
			path: string;
			title: string;
			command: string[];
			env?: Record<string, string>;
			prepare?(path: string): Promise<Record<string, string>>;
		},
	): Promise<{ workspaceId: string; tabId: string; paneId: string; path: string }> {
		// worktree.create {workspace_id | cwd, branch, base, path, label, focus} → `{type: "worktree_created", workspace,
		// tab, root_pane, worktree: {path}}`; it opens the checkout at `path` (schema `WorktreeCreateParams`, else herdr's
		// worktrees folder) as a new workspace grouped with the source workspace and checks out `branch`, creating it
		// from `base` or HEAD when missing (socket-api.mdx "Worktree methods"; `src/api/schema/worktrees.rs:22`).
		const source = workspaceId ? { workspace_id: workspaceId } : { cwd: opts.repo };
		const created = await this.client.request<{
			workspace: WireWorkspace;
			tab: { tab_id: string };
			worktree: { path: string };
		}>(
			"worktree.create",
			{
				...source,
				branch: opts.branch,
				...(opts.base ? { base: opts.base } : {}),
				path: opts.path,
				label: opts.title,
				focus: false,
			},
			WORKTREE_TIMEOUT_MS,
		);
		const extra = opts.prepare ? await opts.prepare(created.worktree.path) : {};
		const env = opts.env || Object.keys(extra).length > 0 ? { ...opts.env, ...extra } : undefined;
		// Its root tab runs the default shell; layout.apply replaces it with a tab running `command`.
		const layout = await this.replaceTab(created.tab.tab_id, {
			tab_label: opts.title,
			focus: false,
			root: commandPane(created.worktree.path, opts.command, env),
		});
		return { workspaceId: created.workspace.workspace_id, ...layout, path: created.worktree.path };
	}

	async moveWorkspace(workspaceId: string, index: number): Promise<void> {
		// workspace.move {workspace_id, insert_index} (schema `WorkspaceMoveParams`).
		await this.client.request("workspace.move", { workspace_id: workspaceId, insert_index: index });
	}

	async respawn(
		paneId: string,
		opts: { cwd: string; command: string[]; env?: Record<string, string> },
	): Promise<{ tabId: string; paneId: string }> {
		// layout.apply {tab_id, root} replaces the tab's layout; without `tab_label` the tab keeps its own name (herdr
		// `src/app/api/layouts.rs:72-80`).
		const target = await this.client.request<{ pane: WirePane }>("pane.get", { pane_id: paneId });
		return this.replaceTab(target.pane.tab_id, { focus: false, root: commandPane(opts.cwd, opts.command, opts.env) });
	}

	async splitBeside(
		paneId: string,
		opts: {
			command: string[];
			cwd: string;
			env?: Record<string, string>;
			label?: string;
			ratio?: number;
			direction?: "right" | "down";
		},
	): Promise<string> {
		// pane.split starts the default shell only and takes no argv. So: a temporary tab running `command`
		// (layout.apply), then pane.move {pane_id, destination: {type: "tab", tab_id, target_pane_id, split, ratio}} into
		// the target's tab; moving a tab's only pane closes that tab → `{type: "pane_move", move_result: {changed,
		// reason, pane}}` (socket-api.mdx "pane.move").
		const target = await this.client.request<{ pane: WirePane }>("pane.get", { pane_id: paneId });
		const temp = await this.applyLayout({
			workspace_id: target.pane.workspace_id,
			focus: false,
			root: commandPane(opts.cwd, opts.command, opts.env),
		});
		const moved = await this.client
			.request<{ move_result: { changed: boolean; reason?: string | null; pane: WirePane } }>("pane.move", {
				pane_id: temp.paneId,
				destination: {
					type: "tab",
					tab_id: target.pane.tab_id,
					target_pane_id: paneId,
					split: opts.direction ?? "right",
					...(opts.ratio === undefined ? {} : { ratio: opts.ratio }),
				},
				focus: false,
			})
			.catch(async (error: unknown) => {
				await this.client.request("tab.close", { tab_id: temp.tabId }).catch(() => {});
				throw error;
			});
		if (!moved.move_result.changed) {
			await this.client.request("tab.close", { tab_id: temp.tabId }).catch(() => {});
			throw new HerdrError(
				moved.move_result.reason ?? "unchanged",
				`herdr didn't split ${paneId}: ${moved.move_result.reason ?? "unchanged"}`,
			);
		}
		const split = moved.move_result.pane.pane_id;
		// pane.rename {pane_id, label} (schema `PaneRenameParams`).
		if (opts.label) await this.client.request("pane.rename", { pane_id: split, label: opts.label });
		return split;
	}

	private async applyLayout(params: object): Promise<{ tabId: string; paneId: string }> {
		const { layout } = await this.client.request<{ layout: WireLayout }>("layout.apply", params);
		const paneId = firstPaneId(layout.root);
		if (!paneId) throw new HerdrError("invalid_response", "herdr's layout.apply returned no pane id");
		return { tabId: layout.tab_id, paneId };
	}

	/**
	 * `layout.apply` over tab `tabId`, replacing its panes. herdr (0.9.5 on Windows) drops a replaced pane but can leave
	 * the process it ran running, such as the default shell a new workspace or worktree starts with; nothing lists it
	 * after that. So the tab's processes are listed while it still has them and ended once the new layout is in (0486).
	 */
	private async replaceTab(tabId: string, layout: object): Promise<{ tabId: string; paneId: string }> {
		const replaced = await this.client.paneProcesses({ tabId });
		const applied = await this.applyLayout({ tab_id: tabId, ...layout });
		await endProcesses(replaced);
		return applied;
	}

	async prompt(paneId: string, text: string): Promise<void> {
		// agent.prompt {target, text}: a bracketed paste then Enter into the pane's agent; refuses with `agent_blocked`
		// or `agent_not_ready` (socket-api.mdx "Raw methods"; herdr `src/app/api/agents.rs:140-207`).
		await this.client.request("agent.prompt", { target: paneId, text: paneText(text) });
	}

	async sendText(paneId: string, text: string): Promise<void> {
		await this.client.request("pane.send_text", { pane_id: paneId, text: paneText(text) });
	}

	async stop(paneId: string): Promise<void> {
		// pane.send_keys {pane_id, keys}: herdr key-combo strings; `esc` is Escape (socket-api.mdx "pane.send_keys").
		await this.client.request("pane.send_keys", { pane_id: paneId, keys: ["esc"] });
	}

	async submit(paneId: string): Promise<void> {
		// `enter` is herdr's key name for Enter (herdr `src/app/api/agents.rs:835-846`).
		await this.client.request("pane.send_keys", { pane_id: paneId, keys: ["enter"] });
	}

	async sendKeys(paneId: string, keys: string[]): Promise<void> {
		await this.client.request("pane.send_keys", { pane_id: paneId, keys });
	}

	async notify(note: { title: string; body?: string; sound: "request" | "none" }): Promise<NotificationResult> {
		// notification.show {title, body?, sound?} → `{type: "notification_show", shown, reason}` (schema
		// `NotificationShowParams`, `NotificationShowReason`; herdr 0.9.5).
		const answer = await this.client.request<{ reason?: unknown }>("notification.show", {
			title: note.title,
			...(note.body ? { body: note.body } : {}),
			sound: note.sound,
		});
		return NOTIFICATION_RESULTS.find((r) => r === answer.reason) ?? "shown";
	}

	async focus(paneId: string): Promise<void> {
		// pane.focus {pane_id} switches to the pane's workspace and tab and marks it seen.
		await this.client.request("pane.focus", { pane_id: paneId });
	}

	async openPluginPane(opts: {
		pluginId: string;
		entrypoint: string;
		placement: "split" | "popup";
		targetPaneId?: string;
		direction?: "right" | "down";
		cwd?: string;
		env?: Record<string, string>;
		focus?: boolean;
	}): Promise<string | undefined> {
		// plugin.pane.open {plugin_id, entrypoint, placement, target_pane_id, direction, cwd, env, focus} →
		// `{type: "plugin_pane_opened", pane: {plugin_id, entrypoint, pane: PaneInfo}}` for a split, `{type: "ok"}` for a
		// popup (socket-api.mdx "plugin.pane.open"; schema `PluginPaneOpenParams`, `PluginPaneInfo`).
		const answer = await this.client.request<{ pane?: { pane?: { pane_id?: string } } }>("plugin.pane.open", {
			plugin_id: opts.pluginId,
			entrypoint: opts.entrypoint,
			placement: opts.placement,
			...(opts.targetPaneId ? { target_pane_id: opts.targetPaneId } : {}),
			...(opts.direction ? { direction: opts.direction } : {}),
			...(opts.cwd ? { cwd: opts.cwd } : {}),
			...(opts.env ? { env: opts.env } : {}),
			focus: opts.focus ?? false,
		});
		return answer.pane?.pane?.pane_id ?? undefined;
	}

	async closePopup(): Promise<boolean> {
		// popup.close {} → `{type: "ok"}`, or error `popup_not_open`.
		try {
			await this.client.request("popup.close", {});
			return true;
		} catch (error) {
			if (error instanceof HerdrError && error.code === "popup_not_open") return false;
			throw error;
		}
	}

	async setSplitRatio(tabId: string, ratio: number): Promise<void> {
		// layout.set_split_ratio {tab_id, path, ratio}: `path` [] is the tab's top split; `ratio` is the first side's share.
		await this.client.request("layout.set_split_ratio", { tab_id: tabId, path: [], ratio });
	}

	async setAgentView(view: { source: string; label?: string; filter?: object; sort?: object[] }): Promise<void> {
		// agent.view.set {source, label, filter, sort} → `{type: "agent_view", active, source}` (schema
		// `AgentViewSetParams`); a plugin's source is `plugin:<id>`, refused while the plugin is missing or disabled.
		await this.client.request("agent.view.set", view);
	}

	async screen(paneId: string): Promise<string> {
		// pane.read {pane_id, source} → `{type: "pane_read", read: {text, …}}` (schema `PaneReadParams`, `PaneReadResult`);
		// `visible` is the rendered viewport, ANSI stripped by default.
		const answer = await this.client.request<{ read: { text: string } }>("pane.read", {
			pane_id: paneId,
			source: "visible",
		});
		return answer.read.text;
	}

	async reportPaneMetadata(
		paneId: string,
		meta: {
			tokens?: Record<string, string | null>;
			stateLabels?: Partial<Record<AgentState, string>>;
			ttlMs?: number;
		},
	): Promise<void> {
		// pane.report_metadata {pane_id, source, tokens, state_labels, ttl_ms}: display-only; tokens render as `$name` in
		// sidebar rows; `null` clears a token; `ttl_ms` (1 ms to 24 h) expires the report (socket-api.mdx "Agent state
		// reporting"; schema `PaneReportMetadataParams`).
		await this.client.request("pane.report_metadata", {
			pane_id: paneId,
			source: SOURCE,
			...(meta.tokens ? { tokens: meta.tokens } : {}),
			...(meta.stateLabels ? { state_labels: meta.stateLabels } : {}),
			...(meta.ttlMs ? { ttl_ms: meta.ttlMs } : {}),
		});
	}

	async reportWorkspaceMetadata(
		workspaceId: string,
		tokens: Record<string, string | null>,
		ttlMs?: number,
	): Promise<void> {
		// workspace.report_metadata {workspace_id, source, tokens, ttl_ms}.
		await this.client.request("workspace.report_metadata", {
			workspace_id: workspaceId,
			source: SOURCE,
			tokens,
			...(ttlMs ? { ttl_ms: ttlMs } : {}),
		});
	}

	private async wirePanes(): Promise<WirePane[]> {
		// pane.list {} → `{type: "pane_list", panes}` across all workspaces, each with `agent_status` and, once herdr's
		// omp integration reported it, `agent_session {kind: "path", value: <session file>}`.
		const { panes } = await this.client.request<{ panes: WirePane[] }>("pane.list", {});
		return panes;
	}

	async panes(): Promise<PaneInfo[]> {
		return (await this.wirePanes()).map(paneInfo);
	}

	async paneRoot(paneId: string, timeoutMs?: number): Promise<number | undefined> {
		const pid = (await this.client.processInfo(paneId, timeoutMs)).shell_pid;
		return pid && pid > 0 ? pid : undefined;
	}

	watch(
		paneIds: () => string[],
		onChange: (event: { paneId: string; workspaceId: string; state: AgentState }) => void,
	): () => void {
		const last = new Map<string, AgentState>();
		let stopped = false;
		let syncing = false;
		let subscription: { key: string; socket: Socket } | undefined;
		/** The connection of a subscribe still waiting for herdr's answer. */
		let subscribing: Socket | undefined;
		let missing = false;
		let wantedKey = "";

		const emit = (paneId: string, workspaceId: string, state: AgentState) => {
			if (stopped || last.get(paneId) === state) return;
			last.set(paneId, state);
			onChange({ paneId, workspaceId, state });
		};

		/** Subscribes; returns the panes whose state a pushed event has carried since `subscription_started`. */
		const subscribe = async (ids: string[]): Promise<Set<string>> => {
			// events.subscribe {subscriptions: [{type: "pane.agent_status_changed", pane_id}]}: one entry per pane (the
			// type requires `pane_id`); acknowledged with `{type: "subscription_started"}`, then pushes `{event:
			// "pane.agent_status_changed", data: {pane_id, workspace_id, agent_status}}`. An unknown pane rejects the
			// whole request; falling behind sends error `events_lost`; both close the connection (socket-api.mdx "Event
			// subscriptions").
			const id = `jocasta_${++nextId}`;
			const started = Promise.withResolvers<void>();
			const pushed = new Set<string>();
			const { socket, opened } = this.client.connect(
				(line) => {
					const message = parse(line);
					if (message?.id === id) {
						if (message.error) {
							started.reject(new HerdrError(message.error.code ?? "error", message.error.message ?? ""));
						} else started.resolve();
					} else if (message?.event === "pane.agent_status_changed" && message.data?.pane_id) {
						pushed.add(message.data.pane_id);
						emit(message.data.pane_id, message.data.workspace_id ?? "", agentState(message.data.agent_status));
					}
				},
				() => started.reject(new HerdrError("transport", "subscription closed")),
			);
			// Destroyed by `stop()` while it subscribes, so a stop never leaves this connection open (SRV-A-7).
			subscribing = socket;
			// A server that accepts and never answers mustn't hold the watch in `syncing` forever.
			const timer = setTimeout(
				() => started.reject(new HerdrError("timeout", "herdr didn't answer events.subscribe")),
				this.client.timeoutMs,
			);
			try {
				await Promise.race([opened, started.promise]);
				socket.write(
					`${JSON.stringify({
						id,
						method: "events.subscribe",
						params: { subscriptions: ids.map((pane_id) => ({ type: "pane.agent_status_changed", pane_id })) },
					})}\n`,
				);
				await started.promise;
			} catch (error) {
				socket.destroy();
				throw error;
			} finally {
				clearTimeout(timer);
				if (subscribing === socket) subscribing = undefined;
			}
			if (stopped) {
				socket.destroy();
				return pushed;
			}
			subscription?.socket.destroy();
			subscription = { key: ids.join(","), socket };
			return pushed;
		};

		const sync = async () => {
			if (stopped || syncing) return;
			syncing = true;
			try {
				const wanted = [...new Set(paneIds())].sort();
				const key = wanted.join(",");
				const live = subscription !== undefined && !subscription.socket.destroyed;
				if (key === wantedKey && !missing && (live || !wanted.length)) return;
				const present = new Set((await this.wirePanes()).map((pane) => pane.pane_id));
				const ids = wanted.filter((id) => present.has(id));
				missing = ids.length < wanted.length;
				wantedKey = key;
				for (const id of last.keys()) if (!ids.includes(id)) last.delete(id);
				if (!ids.length) {
					subscription?.socket.destroy();
					subscription = undefined;
					return;
				}
				if (live && subscription?.key === ids.join(",")) return;
				const pushed = await subscribe(ids);
				// Reconcile after `subscription_started` with an authoritative read: the state now, for each pane. A pane
				// whose event arrived meanwhile already has a newer state than this read.
				const fresh = new Map((await this.wirePanes()).map((pane) => [pane.pane_id, pane]));
				for (const id of ids) {
					const pane = fresh.get(id);
					if (pane && !pushed.has(id)) emit(id, pane.workspace_id, agentState(pane.agent_status));
				}
			} catch {
				// herdr down, or a pane closed while subscribing (herdr rejects the whole request): the next tick retries.
				wantedKey = "";
			} finally {
				syncing = false;
			}
		};

		void sync();
		const timer = setInterval(() => void sync(), this.watchIntervalMs);
		const stop = () => {
			stopped = true;
			clearInterval(timer);
			subscribing?.destroy();
			subscribing = undefined;
			subscription?.socket.destroy();
			subscription = undefined;
			this.watches.delete(stop);
		};
		this.watches.add(stop);
		return stop;
	}

	keepDown(): void {
		this.client.revive = undefined;
	}

	async stopServer(): Promise<void> {
		await this.client.stopServer();
	}

	close(): void {
		for (const stop of [...this.watches]) stop();
	}
}
