import { dayStart, localDay } from "../../shared/day.ts";
import type { SessionIndex } from "./sessions.ts";

/** How long a sum is reused while no session read found a change (CO-15). */
const REUSE_MS = 30_000;

const lastSums = new WeakMap<SessionIndex, { at: number; day: string; changes: number; total: number }>();

/**
 * Today's spend (0274, 0403, 0433, 0452): the `usage.cost.total` of today's model calls (local day) in every omp
 * session, its subagent and advisor transcripts included, as `omp stats` counts it (CR-04, F-06). Every session
 * counts, wherever it ran: a merged thread's worktree is gone, yet its spend was today's. Each call counts once, by
 * the session index's ledger (0464): a moved session's copies don't count twice, and a call stays counted when the
 * file holding it is rewritten without it, moved or deleted, so the figure never drops within the day. The sum is
 * reused for 30 s while no read of `sessions` found a session changed.
 */
export function spendToday(sessions: SessionIndex, now = Date.now()): number {
	const today = localDay(now);
	const last = lastSums.get(sessions);
	if (last && last.day === today && last.changes === sessions.changes && now >= last.at && now - last.at < REUSE_MS) {
		return last.total;
	}
	for (const session of sessions.list(dayStart(today))) sessions.read(session.file);
	const total = sessions.spentOn(today);
	lastSums.set(sessions, { at: now, day: today, changes: sessions.changes, total });
	return total;
}
