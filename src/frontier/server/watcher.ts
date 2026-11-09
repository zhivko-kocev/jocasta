import type { Database } from "bun:sqlite";
import { statSync } from "node:fs";
import { basename } from "node:path";
import { asObject, readJsonObject, writeAtomically } from "../../shared/fsx.ts";
import { log } from "../../shared/log.ts";
import { readHeader, replaced, rewritten, SessionTail, SessionWalker } from "../../shared/omp-session.ts";

/**
 * The session watcher (0389, 0400): saves the compaction and handoff summaries of his omp sessions to `catalog.db`
 * `summaries` and sends those from sessions under a watched project to Graphiti as episodes in the project's group.
 *
 * Each session file is read from where the last scan stopped (CR-03): `<data>/watcher.json` keeps `lastScan` and, per
 * file, the byte offset after its last complete line, its cwd and its identity (`ino`, `birthtimeMs`): omp ≥ 18.10.4
 * rewrites a session whole through a temp file and a rename (discards, retry marks, shake), possibly no shorter than
 * the offset, so a file whose identity changed is read again from its start, and the summaries' key absorbs the
 * re-read (SRV-B-3). Graphiti gets the summaries of `handoff` and `soft` compactions (and legacy ones without a
 * method), as 0400 decided; `remote` and `snapcompact` ones stay in `catalog.db` only (stored as already sent). A
 * snapcompact transcript archive (`isTranscriptArchive`) never goes to Graphiti whatever its method, and one already
 * waiting in `catalog.db` is marked sent without a post. When a post fails (Graphiti or Docker down) the scan stops
 * posting and the next attempt waits, doubling from 30 s up to 30 min (CR-13): at most one container start per
 * attempt. At most `POSTS_PER_SCAN` episodes go per scan, so a first start's backlog is paced. A summary keeps the
 * watched project its cwd resolved to (`project_id`, 0440 (4)), so it still posts when that folder is gone. A scan
 * lets the service's other work run every few milliseconds (PERF-3). The state stays in memory after the first read
 * and is written when the set of files changed, else at most every 5 minutes while offsets move, and at stop: a lost
 * offset only re-reads a tail, which INSERT OR IGNORE absorbs.
 *
 * omp 18.10.3 shapes this relies on (omp-src `packages/coding-agent/src/session/`), and 18.12.6's moved sessions:
 * - layout: a session is `<sessions>/<cwd dir>/<stem>.jsonl` (`session-paths.ts:215` `sessionDirForCwd`); its subagents
 *   live in `<sessions>/<cwd dir>/<stem>/<AgentId>.jsonl` beside `__advisor*.jsonl` (`sub-sessions.ts:4-6`), so only
 *   files directly in a cwd dir are sessions.
 * - header: `{type: "session", version, id, timestamp, cwd, …}` (`session-entries.ts:37-58`); since the title slot
 *   (`{type: "title", v: 1, …}`, `session-entries.ts:26-33`) it is the file's second line, not its first.
 * - compaction: `{type: "compaction", id, parentId, timestamp, summary, shortSummary?, firstKeptEntryId, tokensBefore,
 *   method?, …}` (`session-entries.ts:125-149`); the text is `summary`. `method` is one of `remote`, `snapcompact`,
 *   `handoff`, `soft`, `shake` (`compaction-methods.ts:11-38`), absent on legacy and extension compactions; `/handoff`
 *   writes one with `method: "handoff"` into the same session (`session-maintenance.ts:2060-2071`).
 * - message: `{type: "message", id, parentId, timestamp, message}` (`session-entries.ts:80-83`); an assistant message
 *   has `role: "assistant"`, `content` parts (`text` / `thinking` / `toolCall`) and `stopReason` `stop | length |
 *   toolUse | error | aborted` (pi-ai `types.ts:1041`), and is written once, at `message_end` (`agent-session.ts:3945`).
 * - moved session (18.12.6, 0464): when another process wrote a session first, omp goes on in a sibling file whose
 *   header names the old session as `parentSession` and publishes the whole transcript there once
 *   (`session-manager.ts` `#moveOffSessionFile`); its compactions already stored from the old file are skipped. The
 *   service follows the boss's session there (`SessionIndex.moved`). `model_usage` and `memory_recall` entries hold
 *   no summary and are passed over.
 */

/** One Graphiti episode for a summary. */
export interface Episode {
	name: string;
	body: string;
	source: string;
	at: string;
}

export interface WatcherOptions {
	/** omp's sessions dir, `~/.omp/agent/sessions`. */
	sessionsDir: string;
	/** `<data>/watcher.json`: `{lastScan, files: {<session file>: {offset, cwd, ino, birthtimeMs}}}`. */
	stateFile: string;
	/** `catalog.db` at v7 or later. */
	db: Database;
	/** The watched project holding `cwd`, if any. */
	projectFor(cwd: string): { projectId: string } | undefined;
	/** Sends one episode to the project's Graphiti group; throws when it didn't land. */
	post(projectId: string, episode: Episode): Promise<void>;
	/** Scan interval, default 30 s. */
	intervalMs?: number;
	now?: () => number;
	onError?(e: unknown): void;
	/** Episodes posted per scan at most, default `POSTS_PER_SCAN`. */
	postsPerScan?: number;
}

export interface Watcher {
	scan(): Promise<void>;
	stop(): void;
}

interface SummaryRow {
	session_file: string;
	entry_id: string;
	kind: "compact" | "handoff";
	cwd: string;
	at: number;
	summary: string;
	project_id: string | null;
}

/** What `watcher.json` holds. */
interface WatcherState {
	lastScan: number;
	files: Record<string, FileState>;
}

/** One session file's place: the offset after its last complete line, its cwd and its identity when read. */
interface FileState {
	offset: number;
	cwd?: string;
	ino?: number;
	birthtimeMs?: number;
}

/** A scan holds the event loop at most about this long before it lets other work run (PERF-3). */
const SLICE_MS = 8;

/** watcher.json is rewritten at most this often while only offsets move. */
const STATE_WRITE_MS = 5 * 60_000;

const SOURCE = "omp-session";

/** Episodes posted per scan: each runs Graphiti's LLM extraction on his logins. */
const POSTS_PER_SCAN = 5;

/** The wait after a failed post: doubles from the first to the last. */
const RETRY_FIRST_MS = 30_000;
const RETRY_MAX_MS = 30 * 60_000;

/** Compaction methods whose summaries go to Graphiti (0400); legacy entries carry none. */
const GRAPH_METHODS = new Set([undefined, "handoff", "soft"]);

/**
 * Whether a compaction's text is a snapcompact transcript archive (omp's `snapcompact/src/prompts/snapcompact-summary.md`:
 * "Resume prior conversation. Earlier turns archived under HISTORY …", a scope legend, a file list and a `HISTORY`
 * section whose turns are mostly images) rather than a written summary. It holds no knowledge for the graph, and its
 * legend naming an archived "assistant reasoning" scope makes Anthropic's `reasoning_extraction` classifier refuse the
 * extraction prompt it is quoted in, which then held back every later episode. Rows of this shape were stored as unsent
 * before 0400's method filter (CR-23).
 */
function isTranscriptArchive(summary: string): boolean {
	return (
		summary.startsWith("Resume prior conversation. Earlier turns archived under HISTORY") &&
		/\nHISTORY\r?\n/.test(summary)
	);
}

export function startWatcher(opts: WatcherOptions): Watcher {
	const now = opts.now ?? Date.now;
	const onError = opts.onError ?? ((e: unknown) => log("warn", "watcher", e instanceof Error ? e.message : String(e)));
	const insert = opts.db.query<
		void,
		[string, string, SummaryRow["kind"], string, number, string, number, string | null]
	>(
		"INSERT OR IGNORE INTO summaries (session_file, entry_id, kind, cwd, at, summary, sent, project_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
	);
	// Without `summary`: a backlog outside any watched project is listed every scan, its text is loaded only to post.
	const unsent = opts.db.query<Omit<SummaryRow, "summary">, []>(
		"SELECT session_file, entry_id, kind, cwd, at, project_id FROM summaries WHERE sent = 0 ORDER BY at, session_file, entry_id",
	);
	const summaryOf = opts.db.query<{ summary: string }, [string, string]>(
		"SELECT summary FROM summaries WHERE session_file = ? AND entry_id = ?",
	);
	const setProject = opts.db.query<void, [string, string, string]>(
		"UPDATE summaries SET project_id = ? WHERE session_file = ? AND entry_id = ?",
	);
	const markSent = opts.db.query<void, [string, string]>(
		"UPDATE summaries SET sent = 1 WHERE session_file = ? AND entry_id = ?",
	);
	// A moved session's copy of a compaction the old file gave already (same entry id, same cwd).
	const storedElsewhere = opts.db.query<{ one: number }, [string, string, string]>(
		"SELECT 1 AS one FROM summaries WHERE entry_id = ? AND cwd = ? AND session_file <> ? LIMIT 1",
	);
	const postsPerScan = opts.postsPerScan ?? POSTS_PER_SCAN;
	let stopped = false;
	let running: Promise<void> | undefined;
	/** Posting waits until then after a failure; `failures` in a row set the wait. */
	let retryAt = 0;
	let failures = 0;
	/** `watcher.json` as last read or written; read once, at the first scan. */
	let state: WatcherState | undefined;
	/** Whether `state` holds offsets watcher.json doesn't yet, and when it was last written (`now()`). */
	let unsaved = false;
	let savedAt = Number.NEGATIVE_INFINITY;
	/** The session files; cwd dirs unchanged since the last scan are answered from its listing (SRV-B-6). */
	const walker = new SessionWalker(opts.sessionsDir);
	const save = () => {
		if (!state || !unsaved) return;
		writeState(opts.stateFile, state);
		unsaved = false;
		savedAt = now();
	};

	async function runScan(): Promise<void> {
		const started = now();
		state ??= readState(opts.stateFile);
		const previous = state;
		const files: WatcherState["files"] = {};
		// Each distinct cwd is resolved once per scan.
		const projects = new Map<string, string | null>();
		const projectOf = (cwd: string): string | null => {
			let id = projects.get(cwd);
			if (id === undefined) {
				id = opts.projectFor(cwd)?.projectId ?? null;
				projects.set(cwd, id);
			}
			return id;
		};
		let slice = performance.now();
		for (const file of walker.files()) {
			if (stopped) return;
			// Let the service's other work run every few milliseconds, unchanged files included (a first scan reads every
			// session once; a steady one stats them all).
			if (performance.now() - slice >= SLICE_MS) {
				const { promise, resolve } = Promise.withResolvers<void>();
				setImmediate(resolve);
				await promise;
				slice = performance.now();
				if (stopped) return;
			}
			const known = previous.files[file];
			try {
				const stat = statSync(file, { throwIfNoEntry: false });
				if (!stat) continue;
				const identity = { ino: stat.ino, birthtimeMs: stat.birthtimeMs };
				// Another file took its name (a rewrite through a rename); a state from before identities were kept can't say.
				if (stat.mtimeMs < previous.lastScan && !(known && replaced(stat, known))) {
					if (known && !rewritten(stat, known)) {
						files[file] = { ...known, ...identity };
						continue;
					}
					if (!known && previous.lastScan > 0) {
						// Read whole by a scan before offsets were kept: it is in `summaries`; follow it from its end.
						const cwd = readHeader(file)?.cwd;
						files[file] = { offset: stat.size, ...(cwd ? { cwd } : {}), ...identity };
						continue;
					}
				}
				// A file that was rewritten (replaced, or shorter than the offset) is read again from its start.
				const from: FileState = known && !rewritten(stat, known) ? known : { offset: 0 };
				const read = summariesIn(file, from.offset, stat.size, from.cwd, stat.mtimeMs);
				files[file] = { offset: read.offset, ...(read.cwd ? { cwd: read.cwd } : {}), ...identity };
				if (read.moved) {
					read.rows = read.rows.filter(({ row }) => !storedElsewhere.get(row.entry_id, row.cwd, file));
				}
				if (read.rows.length > 0) {
					opts.db.transaction(() => {
						for (const { row, graph } of read.rows) {
							insert.run(
								row.session_file,
								row.entry_id,
								row.kind,
								row.cwd,
								row.at,
								row.summary,
								graph ? 0 : 1,
								projectOf(row.cwd),
							);
						}
					})();
				}
			} catch (error) {
				if (known) files[file] = known;
				onError(error);
			}
		}
		if (now() >= retryAt) {
			let posted = 0;
			for (const row of unsent.all()) {
				if (stopped) return;
				if (posted >= postsPerScan) break;
				const projectId = row.project_id ?? projectOf(row.cwd);
				if (!projectId) continue;
				if (row.project_id === null) setProject.run(projectId, row.session_file, row.entry_id);
				const summary = summaryOf.get(row.session_file, row.entry_id)?.summary;
				if (summary === undefined) continue;
				if (isTranscriptArchive(summary)) {
					markSent.run(row.session_file, row.entry_id);
					continue;
				}
				try {
					await opts.post(projectId, {
						name: `${row.kind} ${basename(row.session_file, ".jsonl")}`,
						body: summary,
						source: SOURCE,
						at: new Date(row.at).toISOString(),
					});
				} catch (error) {
					// The row keeps sent = 0; nothing more is posted until the wait is over (Graphiti or Docker down).
					failures++;
					const wait = Math.min(RETRY_FIRST_MS * 2 ** (failures - 1), RETRY_MAX_MS);
					retryAt = now() + wait;
					onError(
						new Error(
							`graph episodes wait ${Math.round(wait / 1000)} s: ${error instanceof Error ? error.message : String(error)}`,
						),
					);
					break;
				}
				failures = 0;
				markSent.run(row.session_file, row.entry_id);
				posted++;
			}
		}
		state = { lastScan: started, files };
		if (!sameFiles(previous.files, files)) unsaved = true;
		// A new or gone file is written at once; moved offsets at most every 5 minutes (and at stop).
		if (unsaved && (!sameNames(previous.files, files) || now() - savedAt >= STATE_WRITE_MS || now() < savedAt)) save();
	}

	/** One scan at a time; a call during a scan runs another after it, so it sees files written before the call. */
	function scan(): Promise<void> {
		const next = (running ?? Promise.resolve()).then(runScan, runScan);
		running = next;
		const clear = () => {
			if (running === next) running = undefined;
		};
		next.then(clear, clear);
		return next;
	}

	const tick = () => {
		if (stopped || running) return;
		scan().catch(onError);
	};
	tick();
	const timer = setInterval(tick, opts.intervalMs ?? 30_000);

	return {
		scan,
		stop() {
			stopped = true;
			clearInterval(timer);
			try {
				save();
			} catch (error) {
				onError(error);
			}
		},
	};
}

/**
 * The compaction entries with summary text in `file` between byte `from` and `to`, each with whether it goes to
 * Graphiti; the offset after the last complete line; the session's cwd (from its header, or `cwd` when the header was
 * read before); whether its header (read in this range) names a `parentSession`. None without a session header.
 */
function summariesIn(
	file: string,
	from: number,
	to: number,
	cwd: string | undefined,
	mtimeMs: number,
): {
	rows: { row: Omit<SummaryRow, "project_id">; graph: boolean }[];
	offset: number;
	cwd: string | undefined;
	moved: boolean;
} {
	const rows: { row: Omit<SummaryRow, "project_id">; graph: boolean }[] = [];
	let moved = false;
	const tail = new SessionTail(file, {
		// Read once from `from`, which the scan judged against a rewrite already.
		reset() {},
		entry(entry) {
			if (cwd === undefined) {
				if (entry.type === "session" && typeof entry.cwd === "string") {
					cwd = entry.cwd;
					moved = typeof entry.parentSession === "string";
				}
				return;
			}
			if (entry.type !== "compaction" || typeof entry.id !== "string") return;
			const summary = typeof entry.summary === "string" ? entry.summary.trim() : "";
			if (!summary) return; // shake, and snapcompact or remote without text
			const at = typeof entry.timestamp === "string" ? Date.parse(entry.timestamp) : Number.NaN;
			rows.push({
				row: {
					session_file: file,
					entry_id: entry.id,
					kind: entry.method === "handoff" ? "handoff" : "compact",
					cwd,
					at: Number.isFinite(at) ? at : mtimeMs,
					summary,
				},
				graph:
					GRAPH_METHODS.has(typeof entry.method === "string" ? entry.method : undefined) &&
					!isTranscriptArchive(summary),
			});
		},
	});
	tail.offset = from;
	// A line still being written isn't complete; the next scan reads it.
	const offset = tail.readTo(to);
	return { rows, offset, cwd, moved };
}

/** The state file; a missing or unreadable one means scan everything. */
function readState(stateFile: string): WatcherState {
	const value = readJsonObject(stateFile);
	const lastScan = value?.lastScan;
	const files: WatcherState["files"] = {};
	for (const [file, entry] of Object.entries(asObject(value?.files) ?? {})) {
		const { offset, cwd, ino, birthtimeMs } = asObject(entry) ?? {};
		if (typeof offset !== "number" || offset < 0) continue;
		files[file] = {
			offset,
			...(typeof cwd === "string" ? { cwd } : {}),
			...(typeof ino === "number" && typeof birthtimeMs === "number" ? { ino, birthtimeMs } : {}),
		};
	}
	return { lastScan: typeof lastScan === "number" && Number.isFinite(lastScan) ? lastScan : 0, files };
}

/** Whether two states hold the same files. */
function sameNames(a: WatcherState["files"], b: WatcherState["files"]): boolean {
	const names = Object.keys(a);
	return names.length === Object.keys(b).length && names.every((name) => b[name] !== undefined);
}

/** Whether two states hold the same files with the same offsets, cwds and identities. */
function sameFiles(a: WatcherState["files"], b: WatcherState["files"]): boolean {
	return (
		sameNames(a, b) &&
		Object.entries(a).every(([name, mine]) => {
			const other = b[name];
			return (
				other?.offset === mine.offset &&
				other.cwd === mine.cwd &&
				other.ino === mine.ino &&
				other.birthtimeMs === mine.birthtimeMs
			);
		})
	);
}

function writeState(stateFile: string, state: WatcherState): void {
	writeAtomically(stateFile, `${JSON.stringify(state)}\n`);
}
