import { statSync } from "node:fs";
import { join } from "node:path";
import { dayStart, localDay } from "../../shared/day.ts";
import { SessionTail } from "../../shared/omp-session.ts";
import { dataDir } from "../../shared/paths.ts";
import type { SessionIndex } from "./sessions.ts";

/** How often the whole sessions folder is listed for today's files (CO-15, PERF-3); known files are read every call. */
const LIST_MS = 30_000;

/** Today's session files known to `spendToday`, per index, and the files a list found that are being read in steps. */
interface SpendFiles {
	day: string;
	listedAt: number;
	files: Set<string>;
	warming?: Promise<void>;
}

const known = new WeakMap<SessionIndex, SpendFiles>();

/**
 * Today's spend (0274, 0403, 0433, 0452): the `usage.cost.total` of today's model calls (local day) in every omp
 * session, its subagent and advisor transcripts included, as `omp stats` counts it (CR-04, F-06), plus Graphiti's
 * model calls (0582, `graphSpentOn`). Every session counts, wherever it ran: a merged thread's worktree is gone, yet
 * its spend was today's. Each call counts once, by the session index's ledger (0464): a moved session's copies don't
 * count twice, and a call stays counted when the file holding it is rewritten without it, moved or deleted, so the
 * figure never drops within the day.
 *
 * The sessions folder is listed at most every 30 s (PERF-3); in between only the files already known to be today's
 * are read (one stat each when unchanged). A file the list finds that the index doesn't follow yet is read in steps
 * off this call (`SessionIndex.warm`), so a refresh never parses a large session in one block; its spend shows once
 * read (`spendSettled`).
 */
export function spendToday(sessions: SessionIndex, now = Date.now(), graphFile = graphUsagePath()): number {
	const today = localDay(now);
	let state = known.get(sessions);
	if (!state || state.day !== today) {
		state = { day: today, listedAt: Number.NEGATIVE_INFINITY, files: new Set() };
		known.set(sessions, state);
	}
	if (now - state.listedAt >= LIST_MS || now < state.listedAt) {
		state.listedAt = now;
		const fresh: string[] = [];
		for (const session of sessions.list(dayStart(today))) {
			if (sessions.follows(session.file)) state.files.add(session.file);
			else fresh.push(session.file);
		}
		if (fresh.length > 0 && !state.warming) {
			const files = state;
			files.warming = sessions
				.warm(fresh)
				.then(() => {
					for (const file of fresh) files.files.add(file);
				})
				.finally(() => {
					files.warming = undefined;
				});
		}
	}
	for (const file of state.files) sessions.read(file);
	return sessions.spentOn(today) + graphSpentOn(today, graphFile);
}

/** Resolves once the sessions `spendToday` found new are read (tests; the next refresh shows their spend). */
export function spendSettled(sessions: SessionIndex): Promise<void> {
	return known.get(sessions)?.warming ?? Promise.resolve();
}

/** `<data>/graph-usage.jsonl` (0582): one line per Graphiti model call the LLM endpoint answered, with its cost. */
export function graphUsagePath(): string {
	return join(dataDir(), "graph-usage.jsonl");
}

/** The graph usage file as read so far (`SessionTail`: offset and identity) and the cost per local day. */
interface GraphTail {
	reader: SessionTail;
	byDay: Map<string, number>;
}

const graphTails = new Map<string, GraphTail>();

/**
 * What Graphiti's model calls cost on `day` (local, `YYYY-MM-DD`; 0582 SRV-B-12): the `cost` of each line of
 * `graph-usage.jsonl` by its `at`. The file is read incrementally; a replaced or shorter one is read again.
 */
export function graphSpentOn(day: string, file = graphUsagePath()): number {
	const stat = statSync(file, { throwIfNoEntry: false });
	if (!stat) {
		graphTails.delete(file);
		return 0;
	}
	let tail = graphTails.get(file);
	if (!tail) {
		const byDay = new Map<string, number>();
		const reader = new SessionTail(file, {
			entry(row) {
				const at = typeof row.at === "string" ? Date.parse(row.at) : Number.NaN;
				const cost = row.cost;
				if (!Number.isFinite(at) || typeof cost !== "number" || !Number.isFinite(cost)) return;
				const of = localDay(at);
				byDay.set(of, (byDay.get(of) ?? 0) + cost);
			},
			reset: () => byDay.clear(),
		});
		tail = { reader, byDay };
		graphTails.set(file, tail);
	}
	tail.reader.advance(stat);
	return tail.byDay.get(day) ?? 0;
}
