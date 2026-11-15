import { expect, test } from "bun:test";
import { productionLines } from "../../../src/frontier/server/production.ts";
import { closeTask, type TaskRecord } from "../../../src/frontier/server/task-file.ts";

const HOUR = 3_600_000;
const NOW = Date.parse("2026-11-15T12:00:00Z");

/** A closed thread: started `hours` before it closed, `ago` hours before now. */
function thread(
	how: "merged" | "pr" | "removed" | undefined,
	fields: { hours: number; ago?: number; spend?: number | null; firstPassed?: boolean },
): TaskRecord {
	const at = NOW - (fields.ago ?? 1) * HOUR;
	return {
		id: "t-0000",
		projectId: "p",
		project: "demo",
		title: "A thread",
		path: "",
		branch: "",
		base: null,
		session: null,
		parent: null,
		brief: "",
		doneWhen: [],
		createdAt: at - fields.hours * HOUR,
		state: "finished",
		spend: fields.spend === undefined ? 1 : fields.spend,
		plan: null,
		report: null,
		reviews:
			fields.firstPassed === undefined
				? []
				: [
						{ at, passed: fields.firstPassed, checks: [], said: "" },
						{ at, passed: true, checks: [], said: "" },
					],
		closed: true,
		...(how ? { close: { at, how, commit: how === "removed" ? null : "abc" } } : {}),
	};
}

test("a week's finished threads: merged and PRs counted, lead time and spend by median, first reviews passed (0621)", () => {
	const records = [
		thread("merged", { hours: 3, spend: 4, firstPassed: true }),
		thread("merged", { hours: 1, spend: 2, firstPassed: false }),
		thread("pr", { hours: 50, spend: 10, firstPassed: true }),
		// Not finished this week: removed, closed 8 days ago, closed before 0621 (no close), still open.
		thread("removed", { hours: 2 }),
		thread("merged", { hours: 2, ago: 8 * 24 }),
		thread(undefined, { hours: 2 }),
		{ ...thread(undefined, { hours: 2 }), closed: false },
	];
	expect(productionLines(records, NOW).split("\n")).toEqual([
		"Threads finished this week: 3 (2 merged, 1 PR)",
		"Lead time, median: 3 h (1 h to 2 d 2 h)",
		"Spend a finished thread, median: $4.00 (total $16.00)",
		"Passed the first review: 2 of 3",
	]);
});

test("a thread with no spend read is named, not counted as free; a week with none finished is one line (0621)", () => {
	const records = [thread("merged", { hours: 0.5, spend: null }), thread("merged", { hours: 2, spend: 3 })];
	expect(productionLines(records, NOW).split("\n").slice(1)).toEqual([
		"Lead time, median: 1 h 15 min (30 min to 2 h)",
		"Spend a finished thread, median: $3.00 (total $3.00; not known for 1)",
		"Passed the first review: 0 of 2",
	]);
	expect(productionLines([thread("removed", { hours: 1 })], NOW)).toBe("Threads finished this week: none");
});

test("a merged thread whose folder is removed later stays merged, and counts as finished (0621)", () => {
	const record: TaskRecord = { ...thread(undefined, { hours: 2 }), closed: false, state: "running" };
	closeTask(record, { how: "merged", commit: "abc" }, () => {}, NOW - HOUR);
	closeTask(record, { how: "removed", commit: null }, () => {}, NOW);
	expect(record.close).toEqual({ at: NOW - HOUR, how: "merged", commit: "abc" });
	expect(record.state).toBe("finished");
	expect(productionLines([record], NOW).split("\n")[0]).not.toBe("Threads finished this week: none");
});
