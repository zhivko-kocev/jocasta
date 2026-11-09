/**
 * The code index behind `graph_search` (0542, 0562): codebase-memory-mcp (CBM) 0.12.1, pinned through Jocasta's mise
 * file, answers relationship questions about a project's MAIN checkout with at most 10 code locations (path, line and
 * symbol, never source). Callers of `locationLines` add their own "verify with grep" note: CBM misses aliased imports.
 *
 * Process model: one CBM stdio server per project, started on demand with cwd = the main checkout (so CBM watches it),
 * all sharing CBM's one daemon, which exits with its last client. Only the stdio server runs (plus `config set`
 * turning off CBM's web UI and auto-indexing, written into its cache); never `install` or `update`, which write agent
 * configs. CBM's state lives in `<dataDir>/code-index/cache` and `<dataDir>/code-index/runtime`; CBM refuses any folder
 * whose ancestors let other users write (D:\ does by default), so on Windows the data dir must be under the profile.
 *
 * Sealed per project: every query names the project CBM derived for that checkout (from `list_projects`' root path
 * or the `index_repository` answer for it), never a name built from anything else.
 *
 * Classification (`relationshipOf`), case-insensitive; the first rule that matches wins:
 * - implementers: "what/which … implements|extends|inherits from X", "implementations|implementers of X",
 *   "subclasses of X";
 * - impact: "what breaks|would break|is affected|is impacted if/when X …", "impact of (changing) X",
 *   "… depends on X", "dependents of X";
 * - callers: "who/what/which … calls X", "callers of X", "where is X called|used|invoked", "usages|uses of X".
 * The symbol is the first backticked identifier anywhere in the question; else, in the part the rule points at, the
 * first identifier with a code shape (camelCase, snake_case, a dot, or `()`), else the first word that isn't filler
 * ("the", "function", "changing", …). A dotted name keeps its last part. Anything else is not a relationship
 * question (undefined): `graph_search` answers it with facts only.
 */
import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join, relative } from "node:path";
import { LineRpc } from "../../shared/line-rpc.ts";
import { log as sharedLog } from "../../shared/log.ts";
import { isInside, pathKey, realPath, samePath } from "../../shared/pathkey.ts";
import { onPath, runProgram } from "../../shared/process.ts";

export interface CodeLocation {
	/** Relative to the main checkout, forward slashes. */
	path: string;
	line: number;
	symbol: string;
	relation: string;
}

export type Relationship = { kind: "callers" | "impact" | "implementers"; symbol: string };

export interface CodeIndexOptions {
	dataDir: string;
	/** argv of CBM's stdio server; default resolved through mise; null turns the index off. */
	server?: string[] | null;
	/** mise's argv; default the one on PATH; null = none. */
	mise?: string[] | null;
	/** Test seam: how long a project's server may sit unused before it is closed; an hour by default. */
	idleMs?: number;
	log?(line: string): void;
}

/** CBM as Jocasta's mise file pins it (`src/setup/mise.ts`). */
const CBM_TOOL = "github:DeusData/codebase-memory-mcp";
const CBM_VERSION = "0.12.1";
/** Most locations one answer carries (0542). */
const MAX_LOCATIONS = 10;
const CONFIG_MS = 15_000;
const START_MS = 30_000;
/** A first full index took 18–34 s on 42–91 MB of source (0562); big repos get headroom. */
const INDEX_MS = 600_000;
const QUERY_MS = 60_000;
/** How long `stop` waits for a server to leave after its stdin closes, before killing it. */
const STOP_MS = 3_000;
/** How long a failed lookup of the binary is remembered before mise is asked again. */
const MISSING_RETRY_MS = 60_000;
/** A project's stdio server unused this long is closed (about 35 MB each serving, 0562); the next question starts it. */
const IDLE_MS = 3_600_000;

// ── Classification ──────────────────────────────────────────────────────────────────────────────────────────────

const RULES: { kind: Relationship["kind"]; pattern: RegExp }[] = [
	{ kind: "implementers", pattern: /\b(?:what|which|who)\b.*?\b(?:implements?|extends?|inherits?\s+from)\s+(.+)/i },
	{ kind: "implementers", pattern: /\bimplement(?:ations?|ers?)\s+of\s+(.+)/i },
	{ kind: "implementers", pattern: /\bsubclass(?:es)?\s+of\s+(.+)/i },
	{
		kind: "impact",
		pattern:
			/\bwhat\s+(?:breaks|would\s+break|will\s+break|is\s+affected|would\s+be\s+affected|is\s+impacted)\s+(?:if|when)\s+(.+)/i,
	},
	{ kind: "impact", pattern: /\bimpact\s+of\s+(.+)/i },
	{ kind: "impact", pattern: /\bdepends?\s+on\s+(.+)/i },
	{ kind: "impact", pattern: /\bdependents\s+of\s+(.+)/i },
	{ kind: "callers", pattern: /\b(?:who|what|which)\b.*?\bcalls?\s+(.+)/i },
	{ kind: "callers", pattern: /\bcallers?\s+of\s+(.+)/i },
	{ kind: "callers", pattern: /\bwhere\s+(?:is|are|does|do)\s+(.+?)\s+(?:called|used|invoked)\b/i },
	{ kind: "callers", pattern: /\b(?:usages?|uses)\s+of\s+(.+)/i },
];

const IDENTIFIER = /[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*(?:\(\))?/g;
const FILLER = new Set(
	"a an the this that these those my our your its i we you it function functions method methods class classes interface interfaces type types symbol variable const constant changing change changes changed is are be".split(
		" ",
	),
);

/** The graph's name for a token: the last part of a dotted name, without `()`. */
function bare(token: string): string {
	return token.replace(/\(\)$/, "").split(".").at(-1) ?? token;
}

export function relationshipOf(query: string): Relationship | undefined {
	for (const { kind, pattern } of RULES) {
		const region = pattern.exec(query)?.[1];
		if (region === undefined) continue;
		const ticked = /`([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)(?:\(\))?`/.exec(query)?.[1];
		if (ticked) return { kind, symbol: bare(ticked) };
		const tokens = region.match(IDENTIFIER) ?? [];
		const pick =
			tokens.find((token) => /[a-z][A-Z]|_|\.|\(\)$/.test(token)) ??
			tokens.find((token) => !FILLER.has(token.toLowerCase()));
		return pick ? { kind, symbol: bare(pick) } : undefined;
	}
	return undefined;
}

// ── Output ──────────────────────────────────────────────────────────────────────────────────────────────────────

export function locationLines(locations: CodeLocation[]): string[] {
	return locations.map((at) => `- ${at.path}:${at.line} ${at.relation} (${at.symbol})`);
}

const KIND_SAID: Record<Relationship["kind"], string> = {
	callers: "callers of",
	impact: "what is affected if this changes:",
	implementers: "implementers of",
};

/**
 * `graph_search`'s code part (0542, 0562): at most 10 locations from the project's main checkout, no source, with
 * the index's known blind spot said: `<project>'s code index, callers of buildPrompt (main checkout; locations only;
 * it misses aliased imports, so verify with grep before relying on it):` then one line each, or that it found none.
 */
export function codeAnswer(project: string, relationship: Relationship, locations: CodeLocation[]): string {
	const head = `${project}'s code index, ${KIND_SAID[relationship.kind]} ${relationship.symbol} (main checkout; locations only; it misses aliased imports, so verify with grep before relying on it)`;
	return locations.length === 0 ? `${head}: none found.` : [`${head}:`, ...locationLines(locations)].join("\n");
}

/** `<dataDir>/code-index/reuse.log`. */
function reuseLog(dataDir: string): string {
	return join(dataDir, "code-index", "reuse.log");
}

export function readIndexReuse(dataDir: string): number {
	try {
		return readFileSync(reuseLog(dataDir), "utf8").split("\n").filter(Boolean).length;
	} catch {
		return 0;
	}
}

// ── One CBM stdio server ────────────────────────────────────────────────────────────────────────────────────────

interface Rpc {
	id?: number;
	result?: { content?: { text?: string }[]; structuredContent?: unknown; isError?: boolean };
	error?: { message?: string };
}

class StdioServer {
	private readonly rpc: LineRpc<Rpc>;

	constructor(argv: string[], cwd: string, env: Record<string, string | undefined>, onExit: () => void) {
		this.rpc = new LineRpc<Rpc>(argv, {
			cwd,
			env,
			exitError: (code, stderr) => {
				const tail = stderr.trim().split(/\r?\n/).at(-1) ?? "";
				return new Error(`codebase-memory-mcp stopped (exit ${code})${tail ? `: ${tail}` : ""}`);
			},
			onExit,
		});
	}

	request(method: string, params: unknown, timeoutMs: number): Promise<Rpc> {
		return this.rpc.request((id) => ({ jsonrpc: "2.0", id, method, params }), {
			timeoutMs,
			timeoutMessage: `codebase-memory-mcp didn't answer ${method} within ${timeoutMs / 1000} s`,
		});
	}

	async initialize(): Promise<void> {
		const answer = await this.request(
			"initialize",
			{ protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "jocasta", version: "1" } },
			START_MS,
		);
		if (answer.error)
			throw new Error(`codebase-memory-mcp didn't start: ${answer.error.message ?? "initialize failed"}`);
		this.rpc.send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
	}

	/** One tool call's structured answer; throws CBM's own error as one line. */
	async call(tool: string, args: Record<string, unknown>, timeoutMs: number): Promise<Record<string, unknown>> {
		const answer = await this.request("tools/call", { name: tool, arguments: args }, timeoutMs);
		const text = answer.result?.content?.map((part) => part.text ?? "").join("\n") ?? "";
		if (answer.error || answer.result?.isError) {
			throw new Error(`codebase-memory-mcp ${tool} failed: ${answer.error?.message ?? text.split("\n")[0] ?? ""}`);
		}
		let body = answer.result?.structuredContent;
		if (body === undefined) {
			try {
				body = JSON.parse(text);
			} catch {
				throw new Error(`codebase-memory-mcp ${tool} answered something that isn't JSON`);
			}
		}
		if (!body || typeof body !== "object") throw new Error(`codebase-memory-mcp ${tool} answered nothing`);
		const record = body as Record<string, unknown>;
		if (typeof record.error === "string") throw new Error(`codebase-memory-mcp ${tool} failed: ${record.error}`);
		return record;
	}

	/** Closes stdin (the server leaves, and the daemon with its last client), killing it after `STOP_MS`. */
	async close(): Promise<void> {
		this.rpc.close();
		const timer = setTimeout(() => this.rpc.kill(), STOP_MS);
		try {
			await Promise.race([this.rpc.exited, Bun.sleep(STOP_MS + 2_000)]);
		} finally {
			clearTimeout(timer);
		}
	}
}

// ── The index ───────────────────────────────────────────────────────────────────────────────────────────────────

interface Entry {
	/** The main checkout's long-name spelling (CBM's own, so 8.3 names in %TEMP% still match its root path). */
	root: string;
	server?: Promise<StdioServer>;
	/** CBM's project name, once known to be indexed. */
	name?: string;
	/** The first lookup or build, single-flight; `existed` = the index was there before. */
	ready?: Promise<{ name: string; existed: boolean }>;
	refreshing?: Promise<void>;
	/** A merge happened while no index was known here, or during the first lookup: re-index before the next answer. */
	stale?: boolean;
	/** Questions, builds and refreshes running on this entry; its server is never closed for idleness while any run. */
	busy: number;
	/** Closes the server after `IDLE_MS` without use; reset on each use. */
	idleTimer?: Timer;
	/** Dropped (`forget`, or its id now names another checkout): its server closes once the uses still running end. */
	dropped?: boolean;
}

function cypherFor(relationship: Relationship, limit: number): string {
	// The symbol is an identifier (no quotes can reach the query).
	const target = `{name:'${relationship.symbol}'}`;
	switch (relationship.kind) {
		case "callers":
			return `MATCH (c)-[r:CALLS]->(f ${target}) RETURN DISTINCT c.name, c.file_path, r.line, c.start_line LIMIT ${limit}`;
		case "impact":
			return `MATCH (c)-[:CALLS*1..2]->(f ${target}) RETURN DISTINCT c.name, c.file_path, c.start_line LIMIT ${limit}`;
		case "implementers":
			return `MATCH (c)-[r:IMPLEMENTS|INHERITS]->(i ${target}) RETURN DISTINCT c.name, c.file_path, c.start_line, type(r) LIMIT ${limit}`;
	}
}

function relationText(relationship: Relationship, edge: string | undefined): string {
	switch (relationship.kind) {
		case "callers":
			return `caller of ${relationship.symbol}`;
		case "impact":
			return `affected if ${relationship.symbol} changes`;
		case "implementers":
			return `${edge === "INHERITS" ? "extends" : "implements"} ${relationship.symbol}`;
	}
}

export class CodeIndex {
	private readonly options: CodeIndexOptions;
	private readonly dir: string;
	private readonly entries = new Map<string, Entry>();
	private resolved: { argv?: string[]; reason?: string; at: number } | undefined;
	/** The binary lookup in flight (`mise where`), shared by every caller while it runs. */
	private resolving: Promise<{ argv?: string[]; reason?: string }> | undefined;
	private configured: Promise<void> | undefined;
	private reused = 0;

	constructor(options: CodeIndexOptions) {
		this.options = options;
		this.dir = join(options.dataDir, "code-index");
	}

	get reuse(): number {
		return this.reused;
	}

	async unavailable(): Promise<string | undefined> {
		if (this.options.server === null) return "The code index is off.";
		if (process.platform === "win32" && !isInside(realPath(homedir()), realPath(this.dir))) {
			return `The code index can't keep its data in ${this.dir}: codebase-memory-mcp refuses folders other users may write to (D:\\ lets them by default), so Jocasta's data folder must be under ${homedir()}.`;
		}
		return (await this.server()).reason;
	}

	/** Builds (or finds) the project's index now, so its first answer doesn't wait on it (0575: the bench layer builds before a trial's clock). Throws `unavailable()`'s reason. */
	async build(project: { id: string; mainPath: string }): Promise<void> {
		const reason = await this.unavailable();
		if (reason) throw new Error(reason);
		const entry = this.entry(project);
		await this.using(entry, () => this.ready(entry));
	}

	async locations(
		project: { id: string; mainPath: string },
		query: string,
		limit = MAX_LOCATIONS,
	): Promise<CodeLocation[] | undefined> {
		const relationship = relationshipOf(query);
		if (!relationship) return undefined;
		const reason = await this.unavailable();
		if (reason) throw new Error(reason);
		const cap = Math.max(1, Math.min(MAX_LOCATIONS, Math.floor(limit)));
		const entry = this.entry(project);
		return this.using(entry, async () => {
			const { name, existed } = await this.ready(entry);
			const server = await this.client(entry);
			const answer = await server.call(
				"query_graph",
				{ project: name, query: cypherFor(relationship, cap), format: "json", max_rows: cap },
				QUERY_MS,
			);
			const locations = this.rows(answer, entry, relationship).slice(0, cap);
			if (existed) this.countReuse(project.id, relationship);
			return locations;
		});
	}

	async refresh(project: { id: string; mainPath: string }): Promise<void> {
		const entry = this.entries.get(project.id);
		if (!entry || !samePath(entry.root, realPath(project.mainPath))) {
			// Not looked up in this run: if an index exists it is brought up to date before its next answer.
			this.entry(project).stale = true;
			return;
		}
		if (entry.name === undefined) {
			// No index known yet: the lookup running now (or the next one) re-indexes before answering, since a lookup
			// that finds an existing index never re-reads the files and a build may have read them before the merge.
			entry.stale = true;
			return;
		}
		if (entry.refreshing) return entry.refreshing;
		entry.refreshing = this.using(entry, async () => {
			try {
				const server = await this.client(entry);
				await server.call("index_repository", { repo_path: entry.root }, INDEX_MS);
			} catch (error) {
				this.warn(`re-indexing ${project.id} failed: ${error instanceof Error ? error.message : String(error)}`);
			} finally {
				entry.refreshing = undefined;
			}
		});
		return entry.refreshing;
	}

	/** Closes the project's server and drops what is known of it: for a project no longer in projects.yaml. */
	async forget(projectId: string): Promise<void> {
		const entry = this.entries.get(projectId);
		if (!entry) return;
		this.entries.delete(projectId);
		await this.drop(entry);
	}

	async stop(): Promise<void> {
		const servers = [...this.entries.values()].flatMap((entry) => {
			clearTimeout(entry.idleTimer);
			entry.idleTimer = undefined;
			const server = entry.server;
			entry.server = undefined;
			return server ? [server] : [];
		});
		await Promise.race([
			Promise.allSettled(servers.map(async (starting) => (await starting).close())),
			Bun.sleep(STOP_MS + 3_000),
		]).catch(() => undefined);
	}

	// ── internals ──

	private warn(line: string): void {
		(this.options.log ?? ((text: string) => sharedLog("warn", "code-index", text)))(line);
	}

	/** The project's entry; a different main checkout for the same id starts over. */
	private entry(project: { id: string; mainPath: string }): Entry {
		const root = realPath(project.mainPath);
		const known = this.entries.get(project.id);
		if (known && samePath(known.root, root)) return known;
		if (known) void this.drop(known);
		const fresh: Entry = { root, busy: 0 };
		this.entries.set(project.id, fresh);
		return fresh;
	}

	/** Runs `work` as a use of the entry's server: the idle clock stops while it runs and restarts when the last use ends. */
	private async using<T>(entry: Entry, work: () => Promise<T>): Promise<T> {
		entry.busy++;
		clearTimeout(entry.idleTimer);
		entry.idleTimer = undefined;
		try {
			return await work();
		} finally {
			entry.busy--;
			if (entry.busy === 0 && entry.dropped) void this.closeServer(entry);
			else if (entry.busy === 0) {
				// Unref'd: an idle clock never keeps the process alive.
				entry.idleTimer = setTimeout(() => {
					entry.idleTimer = undefined;
					if (entry.busy === 0) void this.closeServer(entry);
				}, this.options.idleMs ?? IDLE_MS);
				entry.idleTimer.unref();
			}
		}
	}

	/** An entry no longer in `entries`: its server closes now, or when its last running use ends. */
	private async drop(entry: Entry): Promise<void> {
		entry.dropped = true;
		if (entry.busy === 0) await this.closeServer(entry);
	}

	/** Closes the entry's server, if one runs or starts; the entry keeps its CBM name, so the next use starts a fresh one. */
	private async closeServer(entry: Entry): Promise<void> {
		clearTimeout(entry.idleTimer);
		entry.idleTimer = undefined;
		const server = entry.server;
		entry.server = undefined;
		await server?.then((started) => started.close()).catch(() => undefined);
	}

	/** The stdio server's argv, or why there is none; one lookup at a time, a missing binary remembered for a minute. */
	private server(): Promise<{ argv?: string[]; reason?: string }> {
		const resolved = this.resolved;
		if (resolved?.argv || (resolved && Date.now() - resolved.at < MISSING_RETRY_MS)) return Promise.resolve(resolved);
		this.resolving ??= this.lookup()
			.then((found) => {
				this.resolved = { ...found, at: Date.now() };
				return this.resolved;
			})
			.finally(() => {
				this.resolving = undefined;
			});
		return this.resolving;
	}

	private async lookup(): Promise<{ argv?: string[]; reason?: string }> {
		const missing = `codebase-memory-mcp ${CBM_VERSION} isn't installed: run \`jocasta setup\`, then \`mise install\`.`;
		const given = this.options.server;
		if (given) {
			const [program = ""] = given;
			return existsSync(program) || onPath(program) ? { argv: given } : { reason: missing };
		}
		const found = onPath("mise");
		const mise = this.options.mise === undefined ? (found ? [found] : null) : this.options.mise;
		if (!mise) return { reason: "codebase-memory-mcp can't be found: mise isn't on PATH." };
		try {
			// Async, so a slow mise (up to `CONFIG_MS`) never blocks the service's event loop.
			const run = await runProgram([...mise, "where", `${CBM_TOOL}@${CBM_VERSION}`], { timeoutMs: CONFIG_MS });
			const folder = run.stdout.trim();
			if (run.code !== 0 || !folder) return { reason: missing };
			const exe = join(folder, process.platform === "win32" ? "codebase-memory-mcp.exe" : "codebase-memory-mcp");
			return existsSync(exe) ? { argv: [exe] } : { reason: missing };
		} catch {
			return { reason: missing };
		}
	}

	private env(): Record<string, string | undefined> {
		return { ...process.env, CBM_CACHE_DIR: join(this.dir, "cache"), CBM_RUNTIME_DIR: join(this.dir, "runtime") };
	}

	/** Once per run: CBM's web UI and auto-indexing off (both stored in its cache). */
	private configure(argv: string[]): Promise<void> {
		this.configured ??= (async () => {
			mkdirSync(join(this.dir, "cache"), { recursive: true });
			// CBM refuses to start when its runtime folder doesn't exist yet.
			mkdirSync(join(this.dir, "runtime"), { recursive: true });
			for (const [key, value] of [
				["ui_enabled", "false"],
				["auto_index", "false"],
			] as const) {
				const run = await runProgram([...argv, "config", "set", key, value], { env: this.env(), timeoutMs: CONFIG_MS });
				if (run.code !== 0)
					this.warn(`codebase-memory-mcp config set ${key} failed (${run.code ?? "timed out"}): ${run.stderr.trim()}`);
			}
		})();
		return this.configured;
	}

	/** The project's running stdio server, started (cwd = its main checkout) when there is none. */
	private client(entry: Entry): Promise<StdioServer> {
		if (entry.server) return entry.server;
		const starting = (async () => {
			const { argv, reason } = await this.server();
			if (!argv) throw new Error(reason);
			await this.configure(argv);
			const server = new StdioServer(argv, entry.root, this.env(), () => {
				if (entry.server === starting) entry.server = undefined;
			});
			try {
				await server.initialize();
			} catch (error) {
				void server.close();
				throw error;
			}
			return server;
		})();
		entry.server = starting;
		starting.catch(() => {
			if (entry.server === starting) entry.server = undefined;
		});
		return starting;
	}

	/** CBM's name for the checkout: found among its projects (re-indexed if a merge came first), else built now. */
	private ready(entry: Entry): Promise<{ name: string; existed: boolean }> {
		if (entry.name !== undefined && !entry.stale) return Promise.resolve({ name: entry.name, existed: true });
		entry.ready ??= this.lookUpIndex(entry).finally(() => {
			entry.ready = undefined;
		});
		return entry.ready;
	}

	/**
	 * One lookup or build. The stale flag is taken when it starts, so a merge reported while it runs (`refresh`) stays
	 * set and the next answer re-indexes; a failed lookup puts back the flag it took.
	 */
	private async lookUpIndex(entry: Entry): Promise<{ name: string; existed: boolean }> {
		const stale = entry.stale === true;
		entry.stale = false;
		try {
			const server = await this.client(entry);
			const found = await this.findProject(server, entry.root);
			let result: { name: string; existed: boolean };
			if (found && !stale) result = { name: found, existed: true };
			else {
				const built = await server.call("index_repository", { repo_path: entry.root }, INDEX_MS);
				if (typeof built.project !== "string" || !built.project) {
					throw new Error("codebase-memory-mcp indexed the checkout but didn't name its project");
				}
				result = { name: built.project, existed: found !== undefined };
			}
			entry.name = result.name;
			return result;
		} catch (error) {
			if (stale) entry.stale = true;
			throw error;
		}
	}

	private async findProject(server: StdioServer, root: string): Promise<string | undefined> {
		const want = pathKey(root);
		for (let offset = 0; ; ) {
			const page = await server.call("list_projects", { format: "json", limit: 500, offset }, QUERY_MS);
			const projects = Array.isArray(page.projects) ? (page.projects as Record<string, unknown>[]) : [];
			for (const project of projects) {
				if (typeof project.name === "string" && typeof project.root_path === "string") {
					if (pathKey(project.root_path) === want) return project.name;
				}
			}
			if (page.has_more !== true || projects.length === 0) return undefined;
			offset += projects.length;
		}
	}

	private rows(answer: Record<string, unknown>, entry: Entry, relationship: Relationship): CodeLocation[] {
		const columns = Array.isArray(answer.columns) ? (answer.columns as unknown[]).map(String) : [];
		const rows = Array.isArray(answer.rows) ? (answer.rows as unknown[][]) : [];
		const at = (row: unknown[], column: string): string | undefined => {
			const index = columns.indexOf(column);
			const value = index >= 0 ? row[index] : undefined;
			return value === undefined || value === null ? undefined : String(value);
		};
		const out: CodeLocation[] = [];
		for (const row of rows) {
			if (!Array.isArray(row)) continue;
			const file = at(row, "c.file_path");
			const symbol = at(row, "c.name");
			if (!file || !symbol) continue;
			const line = Number(at(row, "r.line") ?? at(row, "c.start_line"));
			out.push({
				path: (isAbsolute(file) ? relative(entry.root, realPath(file)) : file).replaceAll("\\", "/"),
				line: Number.isFinite(line) ? line : 0,
				symbol,
				relation: relationText(relationship, at(row, "type(r)")),
			});
		}
		return out;
	}

	private countReuse(projectId: string, relationship: Relationship): void {
		this.reused++;
		try {
			mkdirSync(this.dir, { recursive: true });
			appendFileSync(reuseLog(this.options.dataDir), `${new Date().toISOString()} ${projectId} ${relationship.kind}\n`);
		} catch (error) {
			this.warn(`couldn't write ${reuseLog(this.options.dataDir)}: ${String(error)}`);
		}
	}
}
