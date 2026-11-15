/**
 * The weekly review's production numbers (0621): computed from a project's `thread.json` records with no model call,
 * before the drift agent runs. A thread counts as finished when it closed merged or as a PR in the 7 days before now; a
 * removed thread, and one closed before 0621 (no close time), are left out.
 */
import type { TaskRecord } from "./task-file.ts";

const WEEK_MS = 7 * 24 * 3_600_000;

/** The middle value of `values` (the mean of the two middle ones for an even count); `values` is not empty. */
function median(values: readonly number[]): number {
	const sorted = [...values].sort((a, b) => a - b);
	const middle = Math.floor(sorted.length / 2);
	return sorted.length % 2 === 1
		? (sorted[middle] as number)
		: ((sorted[middle - 1] as number) + (sorted[middle] as number)) / 2;
}

/** A duration as `40 min`, `3 h 10 min` or `2 d 4 h`, rounded down to the minute. */
function duration(ms: number): string {
	const minutes = Math.floor(ms / 60_000);
	if (minutes < 60) return `${minutes} min`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return minutes % 60 === 0 ? `${hours} h` : `${hours} h ${minutes % 60} min`;
	const days = Math.floor(hours / 24);
	return hours % 24 === 0 ? `${days} d` : `${days} d ${hours % 24} h`;
}

const usd = (value: number): string => `$${value.toFixed(2)}`;

/**
 * The week's lines as 0621 previews them: threads finished (merged and as PRs), the median lead time from start to
 * close with its range, the median spend a finished thread with the total, and how many passed their first review.
 * One line when none finished.
 */
export function productionLines(records: readonly TaskRecord[], now: number): string {
	const finished = records.filter(
		(record) =>
			record.close && record.close.how !== "removed" && now - record.close.at <= WEEK_MS && record.close.at <= now,
	);
	if (finished.length === 0) return "Threads finished this week: none";
	const merged = finished.filter((record) => record.close?.how === "merged").length;
	const prs = finished.length - merged;
	const leads = finished.map((record) => (record.close?.at ?? now) - record.createdAt);
	const spends = finished.flatMap((record) => (record.spend === null ? [] : [record.spend]));
	const passed = finished.filter((record) => record.reviews[0]?.passed === true).length;
	const unknown = finished.length - spends.length;
	return [
		`Threads finished this week: ${finished.length} (${merged} merged, ${prs} PR${prs === 1 ? "" : "s"})`,
		`Lead time, median: ${duration(median(leads))} (${duration(Math.min(...leads))} to ${duration(Math.max(...leads))})`,
		spends.length === 0
			? "Spend a finished thread: not known"
			: `Spend a finished thread, median: ${usd(median(spends))} (total ${usd(spends.reduce((a, b) => a + b, 0))}${unknown > 0 ? `; not known for ${unknown}` : ""})`,
		`Passed the first review: ${passed} of ${finished.length}`,
	].join("\n");
}
