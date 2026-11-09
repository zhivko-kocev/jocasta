/**
 * omp's session files as every reader sees them (SRV-B-10): the product's session index, the cost ledger, the
 * watcher and today's spend share these rules, so an omp format change (0464 re-checks every bump) is made once.
 *
 * - Layout: a session is `<sessions>/<cwd dir>/<stem>.jsonl` (omp `session-paths.ts` `sessionDirForCwd`); its subagent,
 *   advisor and fork transcripts sit under its artifacts dir `<stem>/`, nested deeper for a subagent's own
 *   (`sub-sessions.ts`), so only files directly in a cwd dir are sessions (`SessionWalker`, `transcriptsOf`).
 * - Header: `{type: "session", version, id, timestamp, cwd, parentSession?}` (`session-entries.ts`); since the title
 *   slot (`{type: "title", v: 1, …}`, 256 bytes rewritten in place) it is the file's second line (`readHeader`).
 * - Rewrites: omp ≥ 18.10.4 rewrites a session whole through a temp file and a rename (discards, retry marks, shake),
 *   possibly no shorter than what was read, so an incremental reader checks the file's identity as well as its size
 *   (`rewritten`, `SessionTail`, SRV-B-3).
 */
import { closeSync, type Dirent, openSync, readdirSync, readSync, type Stats, statSync } from "node:fs";
import { join, relative } from "node:path";
import { parseLine, readLines } from "./jsonl.ts";

/** Scratch copies `omp gc --compress` leaves beside transcripts; never transcripts of their own (omp-stats). */
const COMPRESS_SCRATCH = ".omp-gc-compress-";

/** A session file's header. */
export interface SessionHeader {
	file: string;
	cwd: string;
	title: string | null;
	/** The header's session id, and the session it moved from or forked off (`parentSession`). */
	id?: string;
	parentSession?: string;
	/** When the session began (the header's `timestamp`), epoch ms. */
	started?: number;
}

/** The first `bytes` of a file: enough for the title slot (256 bytes) and the session header. */
function head(file: string, bytes = 8192): string {
	const fd = openSync(file, "r");
	try {
		const buffer = Buffer.alloc(bytes);
		return buffer.subarray(0, readSync(fd, buffer, 0, bytes, 0)).toString("utf8");
	} finally {
		closeSync(fd);
	}
}

/**
 * A session file's header: its cwd, title, id, `parentSession` and start; undefined when it has no `session` line
 * yet. Throws when the file can't be opened.
 */
export function readHeader(file: string): SessionHeader | undefined {
	let title: string | null = null;
	for (const line of head(file).split("\n")) {
		const entry = parseLine(line);
		if (entry?.type === "title" && typeof entry.title === "string" && entry.title.trim()) title = entry.title.trim();
		if (entry?.type !== "session") continue;
		if (typeof entry.cwd !== "string") return undefined;
		const started = typeof entry.timestamp === "string" ? Date.parse(entry.timestamp) : Number.NaN;
		return {
			file,
			cwd: entry.cwd,
			title,
			...(typeof entry.id === "string" ? { id: entry.id } : {}),
			...(typeof entry.parentSession === "string" ? { parentSession: entry.parentSession } : {}),
			...(Number.isFinite(started) ? { started } : {}),
		};
	}
	return undefined;
}

/**
 * The transcripts (`*.jsonl`) anywhere under `sessionFile`'s artifacts dir (the path without `.jsonl`): subagents,
 * the advisor, forks; sorted. `omp gc --compress` scratch copies are left out, a file or a folder whose name holds the
 * marker alike (the ledger's tested rule). None when the dir doesn't exist (yet).
 */
export function transcriptsOf(sessionFile: string): string[] {
	const dir = sessionFile.replace(/\.jsonl$/, "");
	let entries: Dirent[];
	try {
		entries = readdirSync(dir, { recursive: true, withFileTypes: true });
	} catch {
		return [];
	}
	return entries
		.filter(
			(e) =>
				e.isFile() &&
				e.name.endsWith(".jsonl") &&
				!e.name.includes(COMPRESS_SCRATCH) &&
				!relative(dir, e.parentPath).includes(COMPRESS_SCRATCH),
		)
		.map((e) => join(e.parentPath, e.name))
		.sort();
}

/** Where a reader stopped in a file: the offset after its last complete line, and the file's identity then. */
export interface ReadMark {
	offset: number;
	/** Absent when not known (a state kept before identities were): the file then counts as the same one. */
	ino?: number;
	birthtimeMs?: number;
}

/** Whether another file took the name of the one `mark` was read from (a rewrite through a rename). */
export function replaced(stat: Stats, mark: ReadMark): boolean {
	return mark.ino !== undefined && (stat.ino !== mark.ino || stat.birthtimeMs !== mark.birthtimeMs);
}

/** Whether the file was rewritten since `mark`: replaced, or now shorter than the offset; it is read again from 0. */
export function rewritten(stat: Stats, mark: ReadMark): boolean {
	return replaced(stat, mark) || stat.size < mark.offset;
}

/** What a `SessionTail` hands its entries to. */
export interface TailVisitor {
	/** One complete JSON object line, in file order. */
	entry(entry: Record<string, unknown>): void;
	/** The file was rewritten: everything handed over so far is void; it is read again from its start. */
	reset(): void;
}

/**
 * One JSONL file followed incrementally (CR-03): its offset, its identity and its size and mtime when last read; only
 * lines appended since are parsed, a line still being written waits (`jsonl.ts`), and a rewritten file
 * (`rewritten`) resets the visitor and is read again from its start.
 */
export class SessionTail implements ReadMark {
	offset = 0;
	ino?: number;
	birthtimeMs?: number;
	/** Size and mtime when last read to the end; -1 before. */
	size = -1;
	mtimeMs = -1;

	constructor(
		readonly file: string,
		private readonly visitor: TailVisitor,
	) {}

	/**
	 * Whether the file behind `stat` changed since it was last read; a rewritten one resets the tail first. Keeps the
	 * file's identity.
	 */
	changed(stat: Stats): boolean {
		const again = rewritten(stat, this);
		if (!again && stat.size === this.size && stat.mtimeMs === this.mtimeMs) return false;
		if (again) {
			this.offset = 0;
			this.visitor.reset();
		}
		this.ino = stat.ino;
		this.birthtimeMs = stat.birthtimeMs;
		return true;
	}

	/** Hands the complete lines between the offset and byte `to` to the visitor and moves the offset past them. */
	readTo(to: number): number {
		this.offset = readLines(this.file, this.offset, to, (line) => {
			const entry = parseLine(line);
			if (entry) this.visitor.entry(entry);
		});
		return this.offset;
	}

	/**
	 * Brings the tail up to the file on disk (`stat`, when the caller has it); whether anything changed. A missing file
	 * leaves it as it was.
	 */
	advance(stat: Stats | undefined = statSync(this.file, { throwIfNoEntry: false })): boolean {
		if (!stat || !this.changed(stat)) return false;
		this.readTo(stat.size);
		this.size = stat.size;
		this.mtimeMs = stat.mtimeMs;
		return true;
	}

	/**
	 * As `advance`, `step` bytes at a time with the event loop let run between steps; stops early when `stopped` says
	 * so, and `advance` then reads the rest when asked.
	 */
	async advanceInSteps(step: number, stopped: () => boolean): Promise<void> {
		const stat = statSync(this.file, { throwIfNoEntry: false });
		if (!stat || !this.changed(stat)) return;
		while (this.offset < stat.size && !stopped()) {
			// A line longer than a step: the window doubles until the line is whole, then stepping goes on (PM-18).
			const from = this.offset;
			for (let window = step; ; window *= 2) {
				const to = Math.min(stat.size, from + window);
				if (this.readTo(to) !== from || to === stat.size) break;
			}
			if (this.offset === from) break; // only a line still being written is left
			await Bun.sleep(0);
		}
		if (this.offset < stat.size && stopped()) return;
		this.size = stat.size;
		this.mtimeMs = stat.mtimeMs;
	}
}

/**
 * A cwd dir listed this soon after its mtime is listed again next walk: a file made within the file system's clock
 * tick of the listing leaves the dir's mtime as it was (NTFS gives a create right after another the same time).
 */
const RACY_MS = 2_000;

/**
 * Lists omp's session files: `*.jsonl` directly inside each cwd dir of `sessionsDir` (subagent and advisor
 * transcripts sit one level deeper). A cwd dir whose mtime hasn't changed since this walker last listed it is answered
 * from that listing (SRV-B-6): a new, removed or renamed session file changes its dir's mtime; an append doesn't, and
 * a reader stats the files it follows anyway.
 */
export class SessionWalker {
	private readonly dirs = new Map<string, { mtimeMs: number; listedAt: number; files: string[] }>();

	constructor(readonly sessionsDir: string) {}

	files(): string[] {
		let cwdDirs: Dirent[];
		try {
			cwdDirs = readdirSync(this.sessionsDir, { withFileTypes: true });
		} catch {
			this.dirs.clear();
			return [];
		}
		const files: string[] = [];
		const seen = new Set<string>();
		for (const cwdDir of cwdDirs) {
			if (!cwdDir.isDirectory()) continue;
			const path = join(this.sessionsDir, cwdDir.name);
			seen.add(path);
			const mtimeMs = statSync(path, { throwIfNoEntry: false })?.mtimeMs;
			if (mtimeMs === undefined) continue; // removed since the listing
			const known = this.dirs.get(path);
			if (known && known.mtimeMs === mtimeMs && known.listedAt - mtimeMs >= RACY_MS) {
				files.push(...known.files);
				continue;
			}
			const listedAt = Date.now();
			let names: Dirent[];
			try {
				names = readdirSync(path, { withFileTypes: true });
			} catch {
				this.dirs.delete(path);
				continue; // removed since the listing
			}
			const own = names.filter((e) => e.isFile() && e.name.endsWith(".jsonl")).map((e) => join(path, e.name));
			this.dirs.set(path, { mtimeMs, listedAt, files: own });
			files.push(...own);
		}
		for (const path of this.dirs.keys()) if (!seen.has(path)) this.dirs.delete(path);
		return files;
	}
}
