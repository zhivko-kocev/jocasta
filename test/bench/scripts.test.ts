import { afterAll, afterEach, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { correlate, parseNotifications } from "../../bench/h2-toast.ts";
import {
	closedItself,
	endRestart,
	FOLDER_SPACING_MS,
	folderKey,
	foldersIntact,
	herdrRecovery,
	ompBack,
	type PaneState,
	paneStates,
	pickPane,
	samePaneIds,
	sessionParses,
} from "../../bench/h3-recovery.ts";
import { leaked, makeCanaries, makeQueries } from "../../bench/h4-isolation.ts";
import { schedule } from "../../bench/h5-startup.ts";
import { unionTrees } from "../../bench/h6-memory.ts";
import { deepPath, largestPassing, locationPath } from "../../bench/h7-longpath.ts";
import {
	assignments,
	finished,
	fixtureFiles,
	parseThreadLine,
	readLayerCounters,
	score,
	threadSessionFiles,
} from "../../bench/h8-parallel.ts";
import { diffSets, named, parseRegExport } from "../../bench/h9-footprint.ts";
import { parseBenchArgs } from "../../bench/lib/args.ts";
import { readyPatterns, requireTrustedCwd, toolCommand } from "../../bench/lib/launch.ts";
import { parsePsProcesses, parseWindowsProcesses, treeMemory, treeOf } from "../../bench/lib/proctree.ts";
import { requireSandbox } from "../../bench/lib/sandbox.ts";

const root = join(import.meta.dir, "..", "..");
const temps: string[] = [];
afterEach(() => {
	for (const dir of temps.splice(0)) rmSync(dir, { recursive: true, force: true });
});
const tempDir = () => {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-bench-"));
	temps.push(dir);
	return dir;
};

describe("shared flags", () => {
	test("--out must be outside the repo and is required unless dry-running; --runs is a whole number", () => {
		expect(parseBenchArgs(["--dry-run"], { defaultRuns: 5 })).toMatchObject({ runs: 5, dryRun: true, out: undefined });
		expect(() => parseBenchArgs([], { defaultRuns: 1 })).toThrow("--out <dir> is required");
		expect(() => parseBenchArgs(["--out", join(root, "x")], { defaultRuns: 1 })).toThrow("outside the repo");
		expect(() => parseBenchArgs(["--out", tmpdir(), "--runs", "0"], { defaultRuns: 1 })).toThrow("--runs needs");
		expect(() => parseBenchArgs(["--out", tmpdir(), "--nope"], { defaultRuns: 1 })).toThrow("unknown option: --nope");
		const args = parseBenchArgs(["--out", tmpdir(), "--runs", "3", "--threads", "2,4"], {
			defaultRuns: 1,
			valued: ["--threads"],
		});
		expect([args.runs, args.flags.get("threads")]).toEqual([3, "2,4"]);
	});

	test("live benchmarks refuse his own data and herdr session", () => {
		expect(() => requireSandbox({})).toThrow("JOCASTA_DATA_DIR");
		expect(() => requireSandbox({ JOCASTA_DATA_DIR: "/sandbox", JOCASTA_HERDR_SESSION: "jocasta" })).toThrow(
			"not jocasta or jarvis",
		);
		expect(requireSandbox({ JOCASTA_DATA_DIR: "/sandbox", JOCASTA_HERDR_SESSION: "bench" })).toEqual({
			dataDir: "/sandbox",
			herdrSession: "bench",
		});
	});

	test("--ready replaces one tool's pattern", () => {
		expect(readyPatterns("claude=ready>").claude).toBe("ready>");
		expect(() => readyPatterns("vim=x")).toThrow("--ready takes tool=<regex>");
	});

	test("a tool starts as a command line runs it: an npm .cmd shim for an .exe as that .exe, other shims through cmd", () => {
		const dir = tempDir();
		// npm's cmd-shim for a native binary (opencode's), and for a node script.
		writeFileSync(
			join(dir, "opencode.cmd"),
			'@ECHO off\r\n"%dp0%\\node_modules\\opencode-ai\\bin\\opencode.exe"   %*\r\n',
		);
		writeFileSync(join(dir, "claude.cmd"), '@ECHO off\r\n"%_prog%"  "%dp0%\\node_modules\\cli\\cli.js" %*\r\n');
		const found: Record<string, string> = {
			opencode: join(dir, "opencode.cmd"),
			claude: join(dir, "claude.cmd"),
			omp: join(dir, "omp.exe"),
		};
		const which = (name: string) => found[name] ?? null;
		expect(toolCommand("opencode", which)).toEqual([join(dir, "node_modules", "opencode-ai", "bin", "opencode.exe")]);
		expect(toolCommand("claude", which)).toEqual(["cmd.exe", "/d", "/c", join(dir, "claude.cmd")]);
		expect(toolCommand("omp", which)).toEqual([join(dir, "omp.exe")]);
		expect(() => toolCommand("omp", () => null)).toThrow("omp is not on PATH");
	});

	test("with Claude Code in the tools, a bench pane works only in a --cwd folder: a fresh one asks for trust", () => {
		expect(() => requireTrustedCwd(["omp", "claude"], undefined)).toThrow("pass --cwd");
		expect(() => requireTrustedCwd(["omp", "claude"], "C:\\bench")).not.toThrow();
		expect(() => requireTrustedCwd(["omp", "opencode"], undefined)).not.toThrow();
	});
});

describe("process trees", () => {
	const processes = parseWindowsProcesses(
		JSON.stringify([
			{ ProcessId: 1, ParentProcessId: 0, Name: "explorer.exe", WorkingSetSize: 100, PrivatePageCount: 50 },
			{ ProcessId: 10, ParentProcessId: 1, Name: "omp.exe", WorkingSetSize: 1000, PrivatePageCount: 800 },
			{ ProcessId: 11, ParentProcessId: 10, Name: "bun.exe", WorkingSetSize: 500, PrivatePageCount: 400 },
			{ ProcessId: 12, ParentProcessId: 11, Name: "omp.exe", WorkingSetSize: 200, PrivatePageCount: 100 },
			{ ProcessId: 20, ParentProcessId: 1, Name: "herdr.exe", WorkingSetSize: 300, PrivatePageCount: null },
		]),
	);

	test("a tree, its memory, and roots by name", () => {
		expect(treeOf(10, processes).map((p) => p.pid)).toEqual([10, 11, 12]);
		expect(treeMemory(treeOf(10, processes))).toEqual({ processes: 3, workingSet: 1700, privateBytes: 1300 });
		expect(treeMemory(treeOf(20, processes)).privateBytes).toBeNull();
		expect(
			unionTrees([10, 11, 20], processes)
				.map((p) => p.pid)
				.sort(),
		).toEqual([10, 11, 12, 20]);
		expect(parseWindowsProcesses(JSON.stringify({ ProcessId: 5, ParentProcessId: 1, Name: "a" }))).toHaveLength(1);
	});

	test("ps output", () => {
		expect(parsePsProcesses("  10     1  2048 omp omp --mode json\n")).toEqual([
			{ pid: 10, ppid: 1, workingSet: 2048 * 1024, name: "omp", commandLine: "omp --mode json", privateBytes: null },
		]);
	});
});

describe("H2", () => {
	test("pairs each ask with herdr's change and the pane's blocked notification that herdr answered shown (t3, 0552)", () => {
		const t0 = Date.parse("2026-10-24T10:00:00Z");
		const session = [
			JSON.stringify({ type: "session", timestamp: new Date(t0 - 60_000).toISOString() }),
			JSON.stringify({
				type: "message",
				timestamp: new Date(t0).toISOString(),
				message: { role: "assistant", content: [{ type: "toolCall", name: "ask", arguments: {} }] },
			}),
		].join("\n");
		const note = (at: number, paneId: string, reason: string, result: string) => ({
			at,
			paneId,
			reason,
			title: "acme/a — awaiting you",
			result,
		});
		const trials = correlate(
			new Map([["p1", session]]),
			[
				{ at: t0 + 100, paneId: "p1", state: "working" },
				{ at: t0 + 250, paneId: "p1", state: "blocked" },
			],
			[
				note(t0 + 300, "p2", "blocked", "shown"),
				note(t0 + 500, "p1", "done", "shown"),
				note(t0 + 700, "p1", "blocked", "busy"),
				note(t0 + 900, "p1", "blocked", "shown"),
			],
		);
		expect(trials).toEqual([
			{ paneId: "p1", kind: "ask", t0, herdrMs: 250, notificationMs: 900, notificationResult: "busy" },
		]);
		// No foreground client: herdr didn't show it, so there is no t3, and the row says why.
		expect(
			correlate(new Map([["p1", session]]), [], [note(t0 + 400, "p1", "blocked", "no_foreground_client")]),
		).toEqual([
			{
				paneId: "p1",
				kind: "ask",
				t0,
				herdrMs: null,
				notificationMs: null,
				notificationResult: "no_foreground_client",
			},
		]);
	});

	test("the service's notification log parses; partial and malformed lines don't", () => {
		const lines = [
			'{"at":1761300000000,"paneId":"p1","reason":"blocked","title":"acme/a — awaiting you","result":"shown"}',
			'{"at":"soon","paneId":"p1","reason":"blocked","title":"x","result":"shown"}',
			"not JSON",
			'{"at":1761300000500,"paneId":null,"reason":"event","title":"noted","body":"b","result":"error"}',
			'{"at":1761300000900,"paneId":"p1","rea',
		].join("\r\n");
		expect(parseNotifications(lines)).toEqual([
			{ at: 1761300000000, paneId: "p1", reason: "blocked", title: "acme/a — awaiting you", result: "shown" },
			{ at: 1761300000500, paneId: null, reason: "event", title: "noted", body: "b", result: "error" },
		]);
	});
});

describe("H3", () => {
	const pane = (
		paneId: string,
		agent: string | null,
		status: string | null = null,
		workspaceId: string | null = "w1",
		cwd: string | null = null,
	): PaneState => ({
		paneId,
		workspaceId,
		cwd,
		agent,
		status,
		sessionPath: null,
	});

	test("same panes, intact folders, and the pane to kill", () => {
		expect(samePaneIds([pane("a", null), pane("b", null)], [pane("b", null), pane("a", null)])).toBe(true);
		expect(samePaneIds([pane("a", null)], [pane("a", null), pane("c", null)])).toBe(false);
		const state = { status: " M a.ts\n", diffHash: "x" };
		expect(foldersIntact(new Map([["/r", state]]), new Map([["/r", { ...state }]]))).toBe(true);
		expect(foldersIntact(new Map([["/r", state]]), new Map([["/r", { ...state, diffHash: "y" }]]))).toBe(false);
		expect(
			pickPane([pane("a", "omp", "idle"), pane("b", "omp", "working"), pane("c", null)], new Map(), 0)?.pane.paneId,
		).toBe("b");
	});

	test("a run counts as Jocasta closing itself only with no companion and no panes (0530)", () => {
		const listed = [pane("w1:p1", "omp")];
		// Herdr gone with Jocasta, or answering with nothing: closed itself.
		expect(closedItself(false, false, null)).toBe(true);
		expect(closedItself(false, false, [])).toBe(true);
		// The companion fault's own kill leaves herdr's panes: not a close, so no second restart.
		expect(closedItself(false, false, listed)).toBe(false);
		// A slow relaunch with the companion up, or a recovered run: not a close.
		expect(closedItself(false, true, [])).toBe(false);
		expect(closedItself(true, false, null)).toBe(false);
	});

	test("the pane fault kills each folder's omp at most once in 10 minutes, the longest-rested folder first (0528)", () => {
		const boss = pane("boss", "omp", "working", "w1", "/r/boss");
		const t1 = pane("t1", "omp", "idle", "w2", "/r/t1");
		const list = [boss, t1, pane("sh", null, null, "w3", "/r/sh")];
		// A folder never killed goes before one at work that was.
		expect(pickPane(list, new Map([["/r/boss", 1_000]]), 2_000)).toEqual({ pane: t1, waitMs: 0 });
		// Both killed: the one whose last run ended first, waiting until it is FOLDER_SPACING_MS past that.
		const both = new Map([
			["/r/boss", 5_000],
			["/r/t1", 3_000],
		]);
		expect(pickPane(list, both, 10_000)).toEqual({ pane: t1, waitMs: 3_000 + FOLDER_SPACING_MS - 10_000 });
		expect(pickPane(list, both, 3_000 + FOLDER_SPACING_MS + 1)?.waitMs).toBe(0);
		expect(pickPane([pane("sh", null)], new Map(), 0)).toBeUndefined();
		// 0521 keys folders without case or slash differences on Windows, so the bench does too.
		expect(folderKey(pane("a", "omp", null, "w1", "C:\\Users\\T\\demo"), "win32")).toBe(
			folderKey(pane("b", "omp", null, "w1", "c:/users/t/demo"), "win32"),
		);
		expect(folderKey(pane("a", "omp", null, "w1", "/r/Demo"), "linux")).not.toBe(
			folderKey(pane("b", "omp", null, "w1", "/r/demo"), "linux"),
		);
	});

	test("a restart's client ends root first and never the new companion's tree (0528)", () => {
		const proc = (pid: number, ppid: number, name: string) => ({
			pid,
			ppid,
			name,
			commandLine: name,
			workingSet: 0,
			privateBytes: null,
		});
		const processes = [
			proc(10, 1, "cmd.exe"),
			proc(11, 10, "jocasta.exe"),
			proc(12, 11, "herdr.exe"),
			proc(13, 11, "bun.exe"),
			proc(14, 13, "node.exe"),
			proc(20, 1, "launcher.exe"),
		];
		const killed: number[] = [];
		const kill = (pid: number) => {
			if (pid === 12) throw new Error("gone");
			killed.push(pid);
		};
		// jocasta.exe goes before its herdr client, so it can't see the client exit and ask to close; the kept tree stays.
		expect(endRestart(10, new Set([13, 14]), processes, kill)).toEqual([10, 11]);
		expect(killed).toEqual([10, 11]);
	});

	test("recovered when every workspace that had omp has omp again, under any pane id (0522)", () => {
		const before = [pane("a", "omp", "idle", "w1"), pane("b", "omp", "working", "w2"), pane("c", null, null, "w3")];
		// Relaunched panes with new ids count; the shell-only workspace needs nothing.
		expect(ompBack(before, [pane("x", "omp", "idle", "w2"), pane("y", "omp", "idle", "w1")])).toBe(true);
		// w2's pane is back but without omp.
		expect(ompBack(before, [pane("x", "omp", "idle", "w1"), pane("b", null, null, "w2")])).toBe(false);
		// w2 lists no pane at all.
		expect(ompBack(before, [pane("a", "omp", "idle", "w1"), pane("c", null, null, "w3")])).toBe(false);
	});

	test("the plugin's today pane is no thread: left out of the panes H3 compares (0553)", () => {
		const wire = [
			{ pane_id: "w1:p1", workspace_id: "w1", label: "boss", agent: "omp", agent_status: "idle" },
			{ pane_id: "w1:p2", workspace_id: "w1", label: "today", agent: null, agent_status: "unknown" },
			{ pane_id: "w2:p1", workspace_id: "w2", label: "today", agent: "omp", agent_status: "working" },
		];
		expect(paneStates(wire).map((p) => p.paneId)).toEqual(["w1:p1", "w2:p1"]);
		// today opened again under a new id doesn't make the panes differ.
		const after = paneStates([wire[0], { ...wire[1], pane_id: "w1:p3" }, wire[2]] as typeof wire);
		expect(samePaneIds(paneStates(wire), after)).toBe(true);
	});

	test("the herdr fault notes when the old pane ids were first listed, beside omp being back (0522)", () => {
		const before = [pane("a", "omp", "working", "w1"), pane("b", "omp", "idle", "w2")];
		let clock = 1_000;
		const watch = herdrRecovery(before, 1_000, () => clock);
		clock = 1_250;
		expect(watch.check(null)).toBe(false);
		clock = 3_300;
		// herdr is back with the old panes, not yet running omp.
		expect(watch.check([pane("a", null, null, "w1"), pane("b", null, null, "w2")])).toBe(false);
		clock = 3_600;
		expect(watch.check([pane("x", "omp", "idle", "w1"), pane("b", null, null, "w2")])).toBe(false);
		clock = 8_800;
		expect(watch.check([pane("x", "omp", "idle", "w1"), pane("y", "omp", "idle", "w2")])).toBe(true);
		expect(watch.oldPaneIdsMs()).toBe(2_300);

		const missed = herdrRecovery(before, 0, () => 500);
		expect(missed.check([pane("x", "omp", "idle", "w1"), pane("y", "omp", "idle", "w2")])).toBe(true);
		expect(missed.oldPaneIdsMs()).toBeNull();
	});

	test("a session file parses when every line is JSON and a session entry is anywhere in it (0523)", () => {
		expect(sessionParses('{"type":"session"}\n{"type":"message"}\n')).toBe(true);
		// omp 18.13.1 writes a title entry first.
		expect(sessionParses('{"type":"title","title":"acme"}\r\n{"type":"session"}\r\n{"type":"message"}\r\n')).toBe(true);
		expect(sessionParses('{"type":"title"}\n{"type":"session"}\n{"type":"mess')).toBe(false);
		expect(sessionParses('{"type":"title"}\nnot json\n{"type":"session"}\n')).toBe(false);
		expect(sessionParses('{"type":"title"}\n{"type":"message","text":"session"}\n')).toBe(false);
		expect(sessionParses("")).toBe(false);
	});
});

describe("H4", () => {
	test("canaries, queries and leak checks", () => {
		let n = 0;
		const canaries = makeCanaries(3, () => `${++n}`.padStart(12, "0"));
		expect(canaries[0]).toEqual({
			token: "CANARY-000000000001",
			topic: "build server",
			fact: "In the acme project the build server codename is CANARY-000000000001.",
		});
		const queries = makeQueries(canaries, 20);
		expect(queries).toHaveLength(20);
		expect(queries).toContain("CANARY-000000000002");
		expect(leaked("found CANARY-000000000003 here", canaries)).toEqual(["CANARY-000000000003"]);
		expect(leaked("nothing", canaries)).toEqual([]);
	});
});

describe("H5", () => {
	test("cold, then warm, per run", () => {
		expect(schedule(2)).toEqual([
			{ mode: "cold", run: 0 },
			{ mode: "warm", run: 0 },
			{ mode: "cold", run: 1 },
			{ mode: "warm", run: 1 },
		]);
	});
});

describe("H7", () => {
	test("the deep path has exactly L characters", () => {
		for (let length = 10; length <= 260; length++) expect(deepPath(length)).toHaveLength(length);
		expect(() => deepPath(9)).toThrow("below 10");
	});

	test("locations and the largest passing L", () => {
		expect(locationPath("<parent>/<name>.wt/<slug>", "/src/acme-app", "/home", "h7-1")).toBe("/src/acme-app.wt/h7-1");
		expect(locationPath("<home>/.herdr/worktrees/<name>/<slug>", "/src/acme-app", "/home", "b")).toBe(
			"/home/.herdr/worktrees/acme-app/b",
		);
		expect(
			largestPassing([
				{ location: "jocasta", length: 100, ok: true },
				{ location: "jocasta", length: 200, ok: true },
				{ location: "herdr", length: 100, ok: true },
				{ location: "herdr", length: 200, ok: false },
				{ location: "claude", length: 100, ok: false },
			]),
		).toEqual({ jocasta: 200, herdr: 100, claude: null });
	});

	test("a real run with git only: every location checks out short paths", () => {
		const out = join(tempDir(), "out");
		const run = spawnSync(
			process.execPath,
			[
				join(root, "bench", "h7-longpath.ts"),
				"--out",
				out,
				"--root",
				tempDir(),
				"--home",
				tempDir(),
				"--lengths",
				"20,40",
			],
			{ encoding: "utf8" },
		);
		expect(run.status).toBe(0);
		const summary = JSON.parse(readFileSync(join(out, "h7-longpath.json"), "utf8"));
		expect(summary.largestPassing).toEqual({ jocasta: 40, herdr: 40, claude: 40 });
	}, 60_000);
});

describe("H8", () => {
	test("K11: each thread's session files come from the pane in its worktree; layer-on counters are the service's hit and reuse logs", () => {
		const dir = tempDir();
		const session = join(dir, "2026_abc.jsonl");
		writeFileSync(session, "{}\n");
		mkdirSync(join(dir, "2026_abc", "task"), { recursive: true });
		writeFileSync(join(dir, "2026_abc", "task", "sub.jsonl"), "{}\n");
		const files = threadSessionFiles(
			[
				{ cwd: "C:\\repo.wt\\H8-R0-N4-T0", sessionPath: session },
				{ cwd: "C:\\elsewhere", sessionPath: join(dir, "other.jsonl") },
			],
			new Map([
				["h8-r0-n4-t0", "C:/repo.wt/h8-r0-n4-t0"],
				["h8-r0-n4-t1", "C:/repo.wt/h8-r0-n4-t1"],
			]),
		);
		expect([...files]).toEqual([["h8-r0-n4-t0", [session, join(dir, "2026_abc", "task", "sub.jsonl")]]]);
		expect(readLayerCounters(dir)).toEqual({ checkHits: 0, indexReuse: 0 });
		mkdirSync(join(dir, "checks"), { recursive: true });
		mkdirSync(join(dir, "code-index"), { recursive: true });
		writeFileSync(join(dir, "checks", "hits.log"), "1\n2\n3\n");
		writeFileSync(join(dir, "code-index", "reuse.log"), "1\n");
		expect(readLayerCounters(dir)).toEqual({ checkHits: 3, indexReuse: 1 });
	});

	test("pairs share a file or an interface; status lines parse; the score counts", () => {
		const work = assignments(4, 0);
		expect(work.map((a) => [a.pair, a.files])).toEqual([
			[0, ["src/mod0.ts"]],
			[0, ["src/mod0.ts"]],
			[1, ["src/shared.ts"]],
			[1, ["src/use1.ts"]],
		]);
		expect(Object.keys(fixtureFiles(4)).sort()).toEqual([
			"package.json",
			"src/mod0.ts",
			"src/mod1.ts",
			"src/shared.ts",
			"src/use0.ts",
			"src/use1.ts",
		]);
		const line = parseThreadLine(
			"acme/h8 r0 n4 t0 [h8-r0-n4-t0] idle — finished its turn · overlaps h8 r0 n4 t1: src/mod0.ts; other: a.ts, b.ts",
		);
		expect(line).toEqual({
			title: "h8 r0 n4 t0",
			branch: "h8-r0-n4-t0",
			state: "idle",
			overlaps: ["h8 r0 n4 t1", "other"],
		});
		expect(parseThreadLine("No threads.")).toBeNull();
		const lines = work.map((a) => ({
			title: a.title,
			branch: a.title.replaceAll(" ", "-"),
			state: "idle",
			overlaps: a.thread === 0 ? [work[1]?.title ?? ""] : [],
		}));
		const changed = new Map(work.map((a) => [a.title, a.thread === 3 ? [...a.files, "src/shared.ts"] : a.files]));
		const result = score(
			work,
			lines,
			changed,
			new Map([
				[0, true],
				[1, false],
			]),
			0,
		);
		expect(result).toMatchObject({
			threads: 4,
			wrongThreadEdits: 1,
			flaggedBeforeMerge: 1,
			conflictsAtMerge: 1,
			foundOnlyAtMerge: 0,
		});
	});

	test("a thread is finished only idle or done with a commit on its branch (0529)", () => {
		const at = (state: string) => ({ title: "h8 r0 n8 t0", branch: "h8-r0-n8-t0", state, overlaps: [] });
		// A fresh thread reads idle before its brief lands, and a waiting plan reads done: neither has a commit.
		expect(finished(at("idle"), 0)).toBe(false);
		expect(finished(at("done"), 0)).toBe(false);
		expect(finished(at("idle"), 1)).toBe(true);
		expect(finished(at("done"), 2)).toBe(true);
		// At work, or waiting on him, isn't finished even with a commit.
		for (const state of ["working", "plan ready", "awaiting you", "blocked"])
			expect(finished(at(state), 1)).toBe(false);
	});
});

describe("H9", () => {
	test("reg export lines, continuations, diffs and named leftovers", () => {
		const text =
			'\uFEFFWindows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\Acme]\r\n"Path"="C:\\\\acme"\r\n"List"=hex:01,02,\\\r\n  03\r\n';
		expect([...parseRegExport(text)]).toEqual([
			"[HKEY_CURRENT_USER\\Software\\Acme]",
			'[HKEY_CURRENT_USER\\Software\\Acme]\\"Path"="C:\\\\acme"',
			'[HKEY_CURRENT_USER\\Software\\Acme]\\"List"=hex:01,02,03',
		]);
		expect(diffSets(["a", "b"], ["b", "c"])).toEqual({ added: ["c"], removed: ["a"] });
		expect(named(["C:\\x\\jocasta\\a", "C:\\y\\other", 'HKCU\\Run\\"Jocasta"'])).toEqual([
			"C:\\x\\jocasta\\a",
			'HKCU\\Run\\"Jocasta"',
		]);
	});
});

describe("every script dry-runs without a sandbox, a model or an --out", () => {
	const env: Record<string, string | undefined> = { ...process.env };
	delete env.JOCASTA_DATA_DIR;
	delete env.JOCASTA_HERDR_SESSION;
	delete env.JOCASTA_EVALS_REAL;
	// His own eval cases stay unread: the config folder is an empty temp one.
	const config = mkdtempSync(join(tmpdir(), "jocasta-bench-config-"));
	env.JOCASTA_CODING_AGENT_DIR = config;
	afterAll(() => rmSync(config, { recursive: true, force: true }));
	test.each([
		["bench/h2-toast.ts", "H2 toast latency (0482) dry run"],
		["bench/h3-recovery.ts", "H3 crash recovery (0482) dry run"],
		["bench/h4-isolation.ts", "H4 memory isolation (0482) dry run"],
		["bench/h5-startup.ts", "H5 startup (0482) dry run"],
		["bench/h6-memory.ts", "H6 memory (0482) dry run"],
		["bench/h7-longpath.ts", "H7 long paths (0482) dry run"],
		["bench/h8-parallel.ts", "H8 parallel isolation (0482) dry run"],
		["bench/h9-footprint.ts", "H9 install footprint (0482) dry run"],
		["bench/h8-parallel.ts --layer off", "K11 (0543): the layer off in the sandbox"],
		["bench/tokscale-parity.ts", "tokscale parity (0482) dry run"],
		["evals/a5.ts record", "A5 record (0482) dry run"],
		["evals/a5.ts report", "A5 report (0482) dry run"],
		["evals/run.ts --variant a3", "evals dry run (0482)"],
		["evals/run.ts --variant a4", "turn 1: claude --model claude-opus-5-5"],
	])("%s", (command, expected) => {
		const [file = "", ...rest] = command.split(" ");
		const run = spawnSync(process.execPath, [join(root, file), ...rest, "--dry-run"], { env, encoding: "utf8" });
		expect(run.stderr).toBe("");
		expect(run.status).toBe(0);
		expect(run.stdout).toContain(expected);
	});
});
