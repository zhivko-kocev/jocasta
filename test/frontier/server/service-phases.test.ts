import { expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type Phase, setPhase, type Thread } from "../../../src/frontier/server/thread.ts";
import { logsDir } from "../../../src/shared/log.ts";
import type { Project } from "../../../src/shared/projects.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

useTempJocasta();

/** A thread in `phase`, as far as a phase move reads it. */
function thread(phase: Phase): Thread {
	return { title: "Readme line", project: { name: "demo" } as Project, phase } as Thread;
}

/** Everything Jocasta's log holds so far in this test's data dir. */
function logged(): string {
	const dir = logsDir();
	if (!existsSync(dir)) return "";
	return readdirSync(dir)
		.map((name) => readFileSync(join(dir, name), "utf8"))
		.join("");
}

const PHASES: Phase[] = ["task", "review", "handoff", "finish"];

/** The moves the task cycle makes (SVC-15): the review, its handoff, Finish's PR turn, and back to the task. */
const MOVES: [Phase, Phase][] = [
	["task", "review"],
	["task", "handoff"],
	["task", "finish"],
	["review", "handoff"],
	["review", "task"],
	["handoff", "task"],
	["handoff", "review"],
	["handoff", "finish"],
	["finish", "task"],
];

test("each phase move of the task cycle passes and is logged (SVC-15)", () => {
	for (const [from, to] of MOVES) {
		const t = thread(from);
		expect(setPhase(t, to, "a test")).toBe(true);
		expect(t.phase).toBe(to);
		expect(logged()).toContain(`INFO [phase] demo/Readme line: ${from} → ${to} (a test)`);
	}
});

test("a phase move outside the task cycle is refused and logged as a warning; staying put is no move (SVC-15)", () => {
	const refused = PHASES.flatMap((from) =>
		PHASES.filter((to) => to !== from && !MOVES.some(([f, t]) => f === from && t === to)).map((to) => [from, to]),
	);
	expect(refused).toEqual([
		["review", "finish"],
		["finish", "review"],
		["finish", "handoff"],
	]);
	for (const [from, to] of refused as [Phase, Phase][]) {
		const t = thread(from);
		expect(setPhase(t, to, "a test")).toBe(false);
		expect(t.phase).toBe(from);
		expect(logged()).toContain(`WARN [phase] demo/Readme line: ${from} → ${to} refused (a test)`);
	}
	const before = logged();
	const t = thread("handoff");
	expect(setPhase(t, "handoff", "a test")).toBe(true);
	expect(logged()).toBe(before);
});
