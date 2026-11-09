import { expect, test } from "bun:test";
import { appendFileSync, mkdirSync, renameSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { checkedTests, readStats, SessionIndex } from "../../../src/frontier/server/sessions.ts";
import { spendToday } from "../../../src/frontier/server/spend.ts";
import { dayStart, localDay } from "../../../src/shared/day.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

const assistant = (text: string, timestamp: number, cost?: number) =>
	JSON.stringify({
		type: "message",
		message: {
			role: "assistant",
			timestamp,
			content: [{ type: "text", text }],
			...(cost === undefined ? {} : { usage: { cost: { total: cost } } }),
		},
	});
const header = (cwd: string) => JSON.stringify({ type: "session", version: 3, id: "s", cwd });
const title = JSON.stringify({ type: "title", v: 1, title: "demo", pad: "" });

/** Writes a session (or a nested transcript) with its lines and an mtime. */
function write(file: string, lines: string[], mtimeMs = Date.now()): string {
	mkdirSync(join(file, ".."), { recursive: true });
	writeFileSync(file, `${lines.join("\n")}\n`);
	utimesSync(file, mtimeMs / 1000, mtimeMs / 1000);
	return file;
}

test("the review's test result is the last TESTS line of an assistant message, with its time (0425)", () => {
	expect(readStats(assistant("No tests line here.", 1)).tests).toBeNull();
	const text = [
		assistant("Ran the suite.\nTESTS: fail", 10),
		assistant("Ran it again after the note.\n\n- `TESTS: pass`", 20),
		assistant("Quoted, not reported: the line said TESTS: none", 30),
	].join("\n");
	expect(readStats(text).tests).toEqual({ result: "pass", at: 20, command: null });
	expect(readStats(assistant("tests: NONE", 5)).tests).toEqual({ result: "none", at: 5, command: null });
	expect(readStats(assistant("Done.\nTESTS: pass — `bun test --bail`", 7)).tests).toEqual({
		result: "pass",
		at: 7,
		command: "bun test --bail",
	});
});

/** A bash call in an assistant message and its result: exit 0, a failure (an error result), or started in the background. */
function bashRun(id: string, command: string, at: number, outcome: "ok" | "fail" | "background"): string[] {
	return [
		JSON.stringify({
			type: "message",
			message: {
				role: "assistant",
				timestamp: at,
				content: [{ type: "toolCall", id, name: "bash", arguments: { command } }],
			},
		}),
		JSON.stringify({
			type: "message",
			message: {
				role: "toolResult",
				toolCallId: id,
				toolName: "bash",
				timestamp: at + 1,
				...(outcome === "fail" ? { isError: true, details: { exitCode: 1 } } : {}),
				...(outcome === "background" ? { details: { async: { state: "running", jobId: "j1", type: "bash" } } } : {}),
				content: [{ type: "text", text: "output" }],
			},
		}),
	];
}

test("a review's tests count only when the command its TESTS line names ran and exited 0 (0453)", () => {
	const checked = (lines: string[], since = 0) => checkedTests(readStats(lines.join("\n")), since);
	const end = (line: string) => assistant(`Reviewed.\n${line}`, 50);
	expect(checked([...bashRun("c1", "bun test", 10, "ok"), end("TESTS: pass — `bun test`")])).toBe("pass");
	// Whitespace differences and a longer command line still match.
	expect(checked([...bashRun("c1", "cd x &&  bun   test", 10, "ok"), end("TESTS: pass — bun test")])).toBe("pass");
	expect(checked([...bashRun("c1", "bun test", 10, "fail"), end("TESTS: pass — `bun test`")])).toBe("fail");
	// The last run of the command decides.
	expect(
		checked([
			...bashRun("c1", "bun test", 10, "fail"),
			...bashRun("c2", "bun test", 20, "ok"),
			end("TESTS: pass — bun test"),
		]),
	).toBe("pass");
	expect(checked([...bashRun("c1", "bun test", 10, "background"), end("TESTS: pass — bun test")])).toBe("unknown");
	expect(checked([end("TESTS: pass — bun test")])).toBe("unknown");
	expect(checked([...bashRun("c1", "bun test", 10, "ok"), end("TESTS: pass")])).toBe("unknown");
	expect(checked([...bashRun("c1", "bun test", 10, "ok"), end("TESTS: fail — bun test")])).toBe("fail");
	expect(checked([end("TESTS: none")])).toBe("none");
	expect(checked([assistant("No line.", 50)])).toBe("unknown");
	// A run or a line from before the review began doesn't count.
	expect(checked([...bashRun("c1", "bun test", 10, "ok"), end("TESTS: pass — bun test")], 20)).toBe("unknown");
	expect(checked([...bashRun("c1", "bun test", 10, "ok"), end("TESTS: pass — bun test")], 60)).toBe("unknown");
});

test("a test command piped, or followed by another, proves no pass: the exit code is the last command's (0453)", () => {
	const checked = (command: string, outcome: "ok" | "fail" = "ok") =>
		checkedTests(
			readStats(
				[...bashRun("c1", command, 10, outcome), assistant("Reviewed.\nTESTS: pass — bun test", 50)].join("\n"),
			),
			0,
		);
	for (const masked of [
		"bun test 2>&1 | tail -20",
		"bun test || true",
		"bun test | tee out.log",
		"bun test; echo done",
		"bun test &",
		"bun test\necho done",
	]) {
		expect(checked(masked)).toBe("unknown");
	}
	// A masked run that failed still fails the review.
	expect(checked("bun test | tail -20", "fail")).toBe("fail");
	for (const own of [
		"bun test > t.log 2>&1",
		"bun test 2>&1",
		"bun test && echo ok",
		'bun test -t "a|b"',
		"cd x && bun test",
		"bun test | tail; bun test",
	]) {
		expect(checked(own)).toBe("pass");
	}
});

test("side calls (`model_usage`) count toward cost; a superseded retry costs but says nothing; memory_recall is ignored (0452, 0464)", () => {
	const now = Date.now();
	const stats = readStats(
		[
			assistant("First answer", now, 1),
			JSON.stringify({
				type: "model_usage",
				id: "u1",
				timestamp: new Date(now).toISOString(),
				purpose: "memory",
				provider: "anthropic",
				model: "claude-haiku",
				usage: { cost: { total: 0.25 } },
			}),
			JSON.stringify({ type: "memory_recall", id: "m1", timestamp: new Date(now).toISOString(), text: "TESTS: fail" }),
			JSON.stringify({
				type: "message",
				message: {
					role: "assistant",
					timestamp: now + 1,
					retryRecovery: { kind: "auto-retry", status: "superseded", attempt: 1 },
					content: [{ type: "text", text: "Half an answer\nTESTS: fail" }],
					usage: { cost: { total: 0.5 } },
				},
			}),
		].join("\n"),
	);
	expect(stats.cost).toBeCloseTo(1.75);
	expect(stats.costByDay.get(localDay(now))).toBeCloseTo(1.75);
	expect(stats.lastLine).toBe("First answer");
	expect(stats.tests).toBeNull();
});

test("the proposed plan's file stays known after approval; a plan sent back is counted (0453, 0471)", () => {
	const propose = JSON.stringify({
		type: "message",
		message: {
			role: "toolResult",
			toolName: "write",
			timestamp: 5,
			details: { xdev: { tool: "propose", mode: "execute", inner: { title: "Plan", planFilePath: "/plans/p.md" } } },
		},
	});
	const user = JSON.stringify({ type: "message", message: { role: "user", timestamp: 6, content: "Smaller, please" } });
	const approved = JSON.stringify({ type: "mode_change", mode: "none" });
	const sentBack = readStats([propose, user].join("\n"));
	expect(sentBack.planSentBack).toBe(1);
	expect(sentBack.planFile).toBe("/plans/p.md");
	const ok = readStats([propose, approved, user].join("\n"));
	expect(ok.planSentBack).toBe(0);
	expect(ok.planAwaiting).toBeNull();
	expect(ok.planFile).toBe("/plans/p.md");
});

test("a session moved to a sibling file is followed there, its copied entries counted once (0464)", () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	const dir = join(sessions, "--C--moved--");
	const entry = (id: string, text: string, cost: number) =>
		JSON.stringify({
			type: "message",
			id,
			message: {
				role: "assistant",
				timestamp: now,
				content: [{ type: "text", text }],
				usage: { cost: { total: cost } },
			},
		});
	const sessionHeader = (id: string, parent?: string) =>
		JSON.stringify({ type: "session", version: 3, id, cwd: "C:\\moved", ...(parent ? { parentSession: parent } : {}) });
	const old = write(join(dir, "2026-10-23T10-00-00-000Z_old1.jsonl"), [
		title,
		sessionHeader("old1"),
		entry("a", "First", 1),
		entry("b", "Other process", 2),
	]);
	write(join(dir, "2026-10-23T10-00-00-000Z_old1", "0-Explore.jsonl"), [entry("x", "Sub", 0.5)]);
	// The moved session republishes its transcript (and copies its artifacts), then goes on.
	const moved = write(join(dir, "2026-10-23T10-05-00-000Z_new2.jsonl"), [
		title,
		sessionHeader("new2", "old1"),
		entry("a", "First", 1),
		entry("c", "Mine", 4),
	]);
	write(join(dir, "2026-10-23T10-05-00-000Z_new2", "0-Explore.jsonl"), [entry("x", "Sub", 0.5)]);
	write(join(dir, "2026-10-23T10-05-00-000Z_new2", "1-Later.jsonl"), [entry("y", "Later", 0.25)]);
	const index = new SessionIndex(sessions);
	expect(index.moved(old)).toBe(moved);
	expect(index.moved(moved)).toBe(moved);
	expect(index.read(old).cost).toBeCloseTo(3.5);
	const stats = index.read(moved);
	expect(stats.cost).toBeCloseTo(4.25);
	expect(stats.lastLine).toBe("Mine");
	expect(spendToday(index, now)).toBeCloseTo(7.75);
});

test("a /handoff compaction's time is the session's handoffAt", () => {
	const handoff = JSON.stringify({ type: "compaction", method: "handoff", timestamp: "2026-10-14T10:00:00.000Z" });
	const soft = JSON.stringify({ type: "compaction", method: "soft", timestamp: "2026-10-14T11:00:00.000Z" });
	expect(readStats(soft).handoffAt).toBeNull();
	expect(readStats([handoff, soft].join("\n")).handoffAt).toBe(Date.parse("2026-10-14T10:00:00.000Z"));
});

test("a session is read incrementally: appended lines add to its stats, a line being written waits (CR-03)", () => {
	const now = Date.now();
	const file = write(join(env.root, "sessions", "--C--demo--", "a.jsonl"), [
		title,
		header("C:\\demo"),
		assistant("One", now, 0.5),
	]);
	let clock = now;
	const index = new SessionIndex(join(env.root, "sessions"), () => clock);
	expect(index.read(file)).toMatchObject({ cost: 0.5, lastLine: "One" });
	// Half a line: not counted yet.
	const next = assistant("Two", now, 0.25);
	appendFileSync(file, next.slice(0, 20));
	clock += 1_000;
	expect(index.read(file)).toMatchObject({ cost: 0.5, lastLine: "One" });
	appendFileSync(file, `${next.slice(20)}\n`);
	// Checked under a second ago: answered from what was read.
	expect(index.read(file)).toMatchObject({ cost: 0.5, lastLine: "One" });
	clock += 1_000;
	expect(index.read(file)).toMatchObject({ cost: 0.75, lastLine: "Two" });
	expect(index.read(file).messageTimes).toHaveLength(2);
	// A rewritten (shorter) file is read again from its start.
	write(file, [title, header("C:\\demo"), assistant("Three", now, 0.1)]);
	clock += 1_000;
	expect(index.read(file)).toMatchObject({ cost: 0.1, lastLine: "Three" });
});

test("a session's cost and today's spend include its subagents and advisor, wherever it ran (CR-04, F-06)", () => {
	const now = Date.now();
	const yesterday = now - 36 * 60 * 60_000;
	const sessions = join(env.root, "sessions");
	const main = write(join(sessions, "--C--demo--", "s1.jsonl"), [
		title,
		header("C:\\demo"),
		assistant("Old", yesterday, 4),
		assistant("Main", now, 1),
	]);
	write(join(sessions, "--C--demo--", "s1", "0-Explore.jsonl"), [header("C:\\demo"), assistant("Sub", now, 0.5)]);
	write(join(sessions, "--C--demo--", "s1", "0-Explore", "1-Nested.jsonl"), [assistant("Nested", now, 0.125)]);
	write(join(sessions, "--C--demo--", "s1", "__advisor.jsonl"), [assistant("Advice", now, 0.25)]);
	// A merged thread's worktree is gone; its session still counts.
	write(join(sessions, "--C--gone--", "s2.jsonl"), [title, header("C:\\gone\\worktree"), assistant("Done", now, 2)]);
	const index = new SessionIndex(sessions);
	const stats = index.read(main);
	expect(stats.cost).toBeCloseTo(5.875);
	expect(stats.costByDay.get(localDay(now))).toBeCloseTo(1.875);
	expect(stats.lastLine).toBe("Main");
	expect(spendToday(index, now)).toBeCloseTo(3.875);
});

test("warm reads sessions and their transcripts in steps, letting the event loop run; read then parses only what's new", async () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	// Over 2 MB, a line longer than a step among them, and a nested transcript.
	const lines = [title, header("C:\\demo")];
	for (let i = 0; i < 1500; i++) lines.push(assistant(`Line ${i} ${"x".repeat(1500)}`, now, 0.001));
	lines.push(assistant(`Long ${"y".repeat(1_200_000)}`, now, 1));
	lines.push(assistant("Last", now, 0.5));
	const main = write(join(sessions, "--C--demo--", "w.jsonl"), lines);
	write(join(sessions, "--C--demo--", "w", "0-Sub.jsonl"), [assistant("Sub", now, 0.25)]);
	let clock = now;
	const index = new SessionIndex(sessions, () => clock);
	let ticks = 0;
	const ticker = setInterval(() => ticks++, 0);
	await index.warm([main, main]);
	clearInterval(ticker);
	expect(ticks).toBeGreaterThan(0);
	const expected = new SessionIndex(sessions).read(main);
	expect(index.read(main)).toMatchObject({ cost: expected.cost, lastLine: "Last" });
	expect(index.read(main).cost).toBeCloseTo(1.5 + 1 + 0.5 + 0.25);
	appendFileSync(main, `${assistant("After", now, 0.125)}\n`);
	clock += 1_000;
	expect(index.read(main)).toMatchObject({ lastLine: "After" });
	// Stopped at once: nothing read, and read still reads it whole.
	const other = new SessionIndex(sessions);
	await other.warm([main], () => true);
	expect(other.read(main).lastLine).toBe("After");
});

test("warm reads past a line over a step and goes on stepping after it (PM-18)", async () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	const lines = [title, header("C:\\demo"), assistant(`Huge ${"z".repeat(2_500_000)}`, now, 1)];
	for (let i = 0; i < 3; i++) lines.push(assistant(`After ${i}`, now, 0.25));
	const main = write(join(sessions, "--C--demo--", "huge.jsonl"), lines);
	const index = new SessionIndex(sessions);
	await index.warm([main]);
	expect(index.read(main)).toMatchObject({ cost: 1.75, lastLine: "After 2" });
	expect(index.read(main).messageTimes).toHaveLength(4);
});

test("today's spend is reused for 30 s unless a read found a session changed (CO-15)", () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	const file = write(join(sessions, "--C--demo--", "spend.jsonl"), [title, header("C:\\demo"), assistant("A", now, 1)]);
	let clock = now;
	const index = new SessionIndex(sessions, () => clock);
	expect(spendToday(index, now)).toBeCloseTo(1);
	appendFileSync(file, `${assistant("B", now, 2)}\n`);
	clock += 2_000;
	// Nothing read the session since: the sum stands.
	expect(spendToday(index, now + 2_000)).toBeCloseTo(1);
	// A read (the service's watched sessions) found the change: summed again.
	index.read(file);
	expect(spendToday(index, now + 2_000)).toBeCloseTo(3);
	appendFileSync(file, `${assistant("C", now, 4)}\n`);
	clock += 30_000;
	expect(spendToday(index, now + 32_000)).toBeCloseTo(7);
});

test("the first read of a new day drops message times before yesterday and forgets files that are gone (PM-17)", () => {
	const today = localDay(Date.now());
	const twoDaysAgo = dayStart(today, -2) + 3_600_000;
	const yesterday = dayStart(today, -1) + 3_600_000;
	const sessions = join(env.root, "sessions");
	const file = write(join(sessions, "--C--demo--", "days.jsonl"), [
		title,
		header("C:\\demo"),
		assistant("Old", twoDaysAgo, 1),
		assistant("Yesterday", yesterday, 1),
	]);
	const gone = write(join(sessions, "--C--demo--", "gone.jsonl"), [
		title,
		header("C:\\demo"),
		assistant("X", yesterday, 1),
	]);
	let clock = dayStart(today) - 60_000;
	const index = new SessionIndex(sessions, () => clock);
	expect(index.read(file).messageTimes).toEqual([twoDaysAgo, yesterday]);
	expect(index.read(gone).lastLine).toBe("X");
	expect(
		index
			.list()
			.map((s) => s.file)
			.sort(),
	).toEqual([file, gone].sort());
	rmSync(gone);
	clock = dayStart(today) + 60_000;
	expect(index.read(file).messageTimes).toEqual([yesterday]);
	// Forgotten: a file of that name is read afresh.
	write(gone, [title, header("C:\\demo"), assistant("Y", yesterday, 1)]);
	expect(index.read(gone).lastLine).toBe("Y");
});

test("a session replaced through a rename by a larger file is read again from its start, counted once", async () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	const file = write(join(sessions, "--C--demo--", "strip.jsonl"), [
		title,
		header("C:\\demo"),
		assistant("One", now, 1),
	]);
	let clock = now;
	const index = new SessionIndex(sessions, () => clock);
	expect(index.read(file)).toMatchObject({ cost: 1, messageTimes: [now] });
	await Bun.sleep(5);
	// omp's snapcompact strip: the whole session rewritten to a temp file, renamed over it, larger than before.
	const temp = write(`${file}.tmp`, [
		title,
		header("C:\\demo"),
		assistant(`One, rewritten ${"r".repeat(200)}`, now, 1),
		assistant("Two", now + 1, 2),
	]);
	renameSync(temp, file);
	clock += 1_000;
	expect(index.read(file)).toMatchObject({ cost: 3, lastLine: "Two", messageTimes: [now, now + 1] });
	expect(spendToday(index, clock)).toBeCloseTo(3);
});

test("today's spend never drops: a file rewritten without some calls, its session moved on, or the file deleted keeps what was spent (0464)", async () => {
	const now = Date.now();
	const sessions = join(env.root, "sessions");
	const dir = join(sessions, "--C--boss--");
	const call = (id: string, cost: number, at = now) =>
		JSON.stringify({
			type: "message",
			id,
			message: {
				role: "assistant",
				timestamp: at,
				content: [{ type: "text", text: id }],
				usage: { cost: { total: cost } },
			},
		});
	const side = (id: string, cost: number) =>
		JSON.stringify({
			type: "model_usage",
			id,
			timestamp: new Date(now).toISOString(),
			usage: { cost: { total: cost } },
		});
	const sessionHeader = (id: string, parent?: string) =>
		JSON.stringify({ type: "session", version: 3, id, cwd: "C:\\boss", ...(parent ? { parentSession: parent } : {}) });
	const old = write(join(dir, "2026-10-24T15-00-00-000Z_old1.jsonl"), [
		title,
		sessionHeader("old1"),
		call("a", 1),
		call("b", 2),
		side("m", 0.25),
	]);
	let clock = now;
	const index = new SessionIndex(sessions, () => clock);
	expect(spendToday(index, clock)).toBeCloseTo(3.25);
	// A real delay: the replacing file must be born at a later time than the first, as on his disk.
	await Bun.sleep(5);
	// A second process writes the session: its own version replaces the file through a rename, without "b".
	const temp = write(`${old}.tmp`, [title, sessionHeader("old1"), call("a", 1), side("m", 0.25), call("f", 0.5)]);
	renameSync(temp, old);
	clock += 31_000;
	expect(spendToday(index, clock)).toBeCloseTo(3.75);
	// The first process moves on to a sibling file (0464), its whole transcript republished there.
	write(join(dir, "2026-10-24T15-05-00-000Z_new2.jsonl"), [
		title,
		sessionHeader("new2", "old1"),
		call("a", 1),
		call("b", 2),
		side("m", 0.25),
		call("c", 4),
	]);
	clock += 31_000;
	expect(spendToday(index, clock)).toBeCloseTo(7.75);
	// The old file deleted (omp's /delete): what it cost was still spent today.
	rmSync(old);
	clock += 31_000;
	expect(spendToday(index, clock)).toBeCloseTo(7.75);
	// Yesterday's calls never count today.
	appendFileSync(
		join(dir, "2026-10-24T15-05-00-000Z_new2.jsonl"),
		`${call("y", 8, dayStart(localDay(now)) - 60_000)}\n`,
	);
	clock += 31_000;
	expect(spendToday(index, clock)).toBeCloseTo(7.75);
});

/** A session entry on omp's tree (0509): its id, its parent's (null for a root), and its body. */
const node = (id: string, parentId: string | null, body: Record<string, unknown>) =>
	JSON.stringify({ id, parentId, timestamp: new Date(0).toISOString(), ...body });
const said = (role: "user" | "assistant", text: string, at: number, cost?: number) => ({
	type: "message",
	message: {
		role,
		timestamp: at,
		content: [{ type: "text", text }],
		...(cost === undefined ? {} : { usage: { cost: { total: cost } } }),
	},
});
const proposal = (title: string, file: string, at: number) => ({
	type: "message",
	message: {
		role: "toolResult",
		toolName: "write",
		timestamp: at,
		details: { xdev: { tool: "propose", mode: "execute", inner: { title, planFilePath: file } } },
	},
});

/** A plan approved, work begun, a second plan proposed and waiting; then rewound to before that proposal. */
const planTree = [
	node("u1", null, said("user", "Plan it", 1)),
	node("p0", "u1", proposal("First", "/plans/first.md", 2)),
	node("m0", "p0", { type: "mode_change", mode: "none" }),
	node("a1", "m0", said("assistant", "Working", 3, 1)),
	node("p1", "a1", proposal("Second", "/plans/second.md", 4)),
	node("e1", "p1", { type: "message", message: { role: "assistant", timestamp: 5, content: [] } }),
];
const planRewind = [
	node("b1", "a1", { type: "branch_summary", fromId: "e1", summary: "Dropped the second plan" }),
	node("u2", "b1", said("user", "Do something else", 6)),
];

test("a plan proposed on a branch that was rewound waits for nothing; the plan file is the active branch's (0509)", () => {
	const waiting = readStats(planTree.join("\n"));
	expect(waiting.planAwaiting).toBe("Second");
	expect(waiting.planFile).toBe("/plans/second.md");
	const summarized = readStats([...planTree, planRewind[0]].join("\n"));
	expect(summarized.planAwaiting).toBeNull();
	expect(summarized.planFile).toBe("/plans/first.md");
	const rewound = readStats([...planTree, ...planRewind].join("\n"));
	expect(rewound.planAwaiting).toBeNull();
	expect(rewound.planFile).toBe("/plans/first.md");
	expect(rewound.planSentBack).toBe(0);
	expect(rewound.lastLine).toBe("Working");
});

/** A review turn that ran `bun test` and said it passed, abandoned for a new branch from its user message. */
const reviewTree = [
	node("u1", null, said("user", "Review it", 1)),
	node("a1", "u1", {
		type: "message",
		message: {
			role: "assistant",
			timestamp: 10,
			content: [{ type: "toolCall", id: "c1", name: "bash", arguments: { command: "bun test" } }],
			usage: { cost: { total: 0.25 } },
		},
	}),
	node("r1", "a1", {
		type: "message",
		message: { role: "toolResult", toolCallId: "c1", toolName: "bash", timestamp: 11, content: [] },
	}),
	node("a2", "r1", said("assistant", "Reviewed.\nTESTS: pass — bun test", 12, 1)),
	node("u2", "u1", said("user", "Review it again", 13)),
	node("a3", "u2", said("assistant", "Different answer", 14, 0.5)),
];

test("an abandoned branch's review proves nothing; its cost still counts (0509)", () => {
	const stats = readStats(reviewTree.join("\n"));
	expect(checkedTests(stats, 0)).toBe("unknown");
	expect(stats.tests).toBeNull();
	expect(stats.bashRuns).toEqual([]);
	expect(stats.lastLine).toBe("Different answer");
	expect(stats.cost).toBeCloseTo(1.75);
	expect(stats.messageTimes).toEqual([1, 10, 11, 12, 13, 14]);
});

test("an off-branch entry leaves the state as it was; its cost counts (0509)", () => {
	const stats = readStats(
		[
			...reviewTree,
			node("late", "a2", {
				type: "model_usage",
				activeLeafId: "a3",
				timestamp: new Date(15).toISOString(),
				usage: { cost: { total: 0.125 } },
			}),
			node("x1", "a2", { ...said("assistant", "Late answer\nTESTS: fail", 16, 2), activeLeafId: "a3" }),
		].join("\n"),
	);
	expect(stats.lastLine).toBe("Different answer");
	expect(stats.tests).toBeNull();
	expect(stats.cost).toBeCloseTo(3.875);
});

test("a rewind read across two incremental reads gives what reading the whole file gives (0509)", () => {
	const sessions = join(env.root, "sessions");
	const file = write(join(sessions, "--C--demo--", "rewind.jsonl"), [title, header("C:\\demo"), ...planTree]);
	let clock = Date.now();
	const index = new SessionIndex(sessions, () => clock);
	expect(index.read(file).planAwaiting).toBe("Second");
	appendFileSync(file, `${planRewind.join("\n")}\n`);
	clock += 1_000;
	const whole = [title, header("C:\\demo"), ...planTree, ...planRewind].join("\n");
	expect(index.read(file)).toEqual(readStats(whole));
	expect(index.read(file).planFile).toBe("/plans/first.md");
});
