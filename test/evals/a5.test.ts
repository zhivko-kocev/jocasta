import { expect, test } from "bun:test";
import type { FeedEntry } from "../../bench/lib/service-feed.ts";
import { a5Report } from "../../evals/a5.ts";

const T = Date.parse("2026-10-24T09:00:00Z");
const min = (m: number) => T + m * 60_000;
const iso = (at: number) => new Date(at).toISOString();

const header = (at: number) => JSON.stringify({ type: "session", version: 3, id: "s", timestamp: iso(at) });
const user = (at: number, text: string, synthetic = false) =>
	JSON.stringify({
		type: "message",
		timestamp: iso(at),
		message: { role: "user", content: [{ type: "text", text }], ...(synthetic ? { synthetic: true } : {}) },
	});
const planReady = (at: number) =>
	JSON.stringify({
		type: "message",
		timestamp: iso(at),
		message: { role: "toolResult", toolName: "write", details: { xdev: { tool: "propose", mode: "execute" } } },
	});
const modeChange = (at: number, mode: string) => JSON.stringify({ type: "mode_change", timestamp: iso(at), mode });

test("interventions, attention minutes and batch time from the feed and the sessions", () => {
	const feed: FeedEntry[] = [
		{
			at: min(5),
			message: { type: "toast", reason: "blocked", text: "acme/a — plan ready, awaiting you", paneId: "p1" },
		},
		{ at: min(20), message: { type: "toast", reason: "blocked", text: "acme/b — awaiting you", paneId: "p2" } },
		// Not urgent: no attention counted.
		{ at: min(30), message: { type: "toast", reason: "event", priority: "normal", text: "noted" } },
		{ at: min(50), message: { type: "toast", reason: "done", text: "acme/a — done" } },
		{ at: min(90), message: { type: "toast", reason: "blocked", text: "acme/b — awaiting you", paneId: "p2" } },
		{ at: min(1), message: { type: "hud" } },
	];
	const boss = [
		header(min(0)),
		user(min(0), "start a and b"),
		user(min(6), "[event] acme/a — plan ready, awaiting you"),
		user(min(22), "tell b to use the old name"),
	].join("\n");
	const threadA = [
		header(min(1)),
		user(min(1), "the brief"),
		planReady(min(4)),
		modeChange(min(8), "default"),
		user(min(40), "Check this task's work. First run the tests"),
		user(min(45), "/handoff Keep what the next task needs"),
		user(min(46), "a synthetic nudge", true),
	].join("\n");
	const threadB = [header(min(2)), user(min(2), "the brief"), user(min(25), "use the old name")].join("\n");
	const report = a5Report({
		feed,
		boss,
		threads: [
			{ name: "a", session: threadA },
			{ name: "b", session: threadB },
		],
		capMs: 60 * 60_000,
	});
	expect(report.threads).toEqual([
		{ name: "a", interventions: 1, messages: 0, approvals: 1 },
		{ name: "b", interventions: 1, messages: 1, approvals: 0 },
	]);
	expect([report.bossMessages, report.interventions, report.urgentToasts, report.answered]).toEqual([2, 4, 3, 2]);
	// The plan toast at 5 is answered by the approval at 8; the question at 20 by the boss message at 22; the toast at
	// 90 has no answer.
	expect(report.attentionMinutes).toBeCloseTo(5, 10);
	expect(report.batchMinutes).toBeCloseTo(49, 10);
});
