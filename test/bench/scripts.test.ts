import { afterAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, sep } from "node:path";
import { correlate, paneOfBranch, parseNotifications } from "../../bench/h2-toast.ts";
import {
	closedItself,
	endRestart,
	FOLDER_SPACING_MS,
	folderKey,
	foldersIntact,
	herdrRecovery,
	ompBack,
	type PaneState,
	paneBack,
	pickPane,
	samePaneIds,
	sessionParses,
	threadPanes,
} from "../../bench/h3-recovery.ts";
import {
	bankLeaks,
	h4Line,
	leaked,
	makeCanaries,
	makeQueries,
	plantBrief,
	type QueryResult,
	scoreH4,
} from "../../bench/h4-isolation.ts";
import { bossPane, rowServiceMs, schedule } from "../../bench/h5-startup.ts";
import { deepPath, largestPassing, locationPath } from "../../bench/h7-longpath.ts";
import {
	assignments,
	finished,
	fixtureFiles,
	mergedTestsFail,
	readLayerCounters,
	score,
	threadSessionFiles,
} from "../../bench/h8-parallel.ts";
import { diffSets, named, parseRegExport } from "../../bench/h9-footprint.ts";
import { parseBenchArgs } from "../../bench/lib/args.ts";
import { herdrServerOf } from "../../bench/lib/jocasta.ts";
import { readyPatterns, requireTrustedCwd, toolCommand } from "../../bench/lib/launch.ts";
import { claimRows } from "../../bench/lib/output.ts";
import {
	jocastaProcesses,
	parsePsProcesses,
	parseWindowsProcesses,
	treeMemory,
	treeOf,
} from "../../bench/lib/proctree.ts";
import { requireSandbox, requireVm } from "../../bench/lib/sandbox.ts";
import { askTimes } from "../../bench/lib/sessions.ts";
import { parseThreadLine, startedBranch } from "../../bench/lib/threads.ts";
import type { AgentState } from "../../src/frontier/server/herdr.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { useTempDirs } from "../shared/fixture.ts";

const root = join(import.meta.dir, "..", "..");
const tempDir = useTempDirs("jocasta-bench-");

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

	test("live benchmarks refuse his own data folder, herdr session and config folder (0576)", () => {
		const his = join(homedir(), "his-data");
		const sandbox = {
			JOCASTA_DATA_DIR: "/sandbox",
			JOCASTA_HERDR_SESSION: "bench",
			JOCASTA_CODING_AGENT_DIR: "/sb-cfg",
		};
		expect(() => requireSandbox({}, "linux", his)).toThrow("JOCASTA_DATA_DIR");
		expect(() => requireSandbox({ ...sandbox, JOCASTA_DATA_DIR: his }, "linux", his)).toThrow("not his own");
		expect(() => requireSandbox({ ...sandbox, JOCASTA_HERDR_SESSION: "jocasta" }, "linux", his)).toThrow(
			"not jocasta or jarvis",
		);
		const refusal =
			"set JOCASTA_CODING_AGENT_DIR to the sandbox's config folder; with ~/.jocasta/agent the sandbox uses his projects, herdr config and Graphiti containers";
		expect(() => requireSandbox({ ...sandbox, JOCASTA_CODING_AGENT_DIR: undefined }, "linux", his)).toThrow(refusal);
		expect(() => requireSandbox({ ...sandbox, JOCASTA_CODING_AGENT_DIR: "~/.jocasta/agent" }, "linux", his)).toThrow(
			refusal,
		);
		// Windows compares folders without case.
		expect(() =>
			requireSandbox({ ...sandbox, JOCASTA_CODING_AGENT_DIR: join(homedir(), ".JOCASTA", "Agent") }, "win32", his),
		).toThrow(refusal);
		expect(requireSandbox(sandbox, "linux", his)).toEqual({
			dataDir: "/sandbox",
			herdrSession: "bench",
			configDir: "/sb-cfg",
		});
	});

	test("the VM-only scripts refuse any other machine; a results folder is claimed once", () => {
		expect(() => requireVm("HIS-PC", "install and uninstall Jocasta")).toThrow(
			"refusing to install and uninstall Jocasta on HIS-PC: only on the VM JOCASTA-CLEAN",
		);
		expect(() => requireVm("jocasta-clean", "x")).not.toThrow();
		const out = tempDir();
		expect(() => claimRows(out, "h3-recovery")).not.toThrow();
		writeFileSync(join(out, "h3-recovery.jsonl"), "{}\n");
		expect(() => claimRows(out, "h2-toast", "h3-recovery")).toThrow("exists; results are never overwritten");
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
		expect(parseWindowsProcesses(JSON.stringify({ ProcessId: 5, ParentProcessId: 1, Name: "a" }))).toHaveLength(1);
	});

	test("Jocasta's processes: the service, herdr's server and the attached surfaces, each once and apart (0552)", () => {
		const proc = (pid: number, ppid: number, workingSet: number) => ({
			pid,
			ppid,
			name: "p",
			commandLine: "p",
			workingSet,
			privateBytes: workingSet,
		});
		// The surface (30) started the service (31, with a child 32) and its herdr client (33); herdr's server (40) runs
		// the panes (41).
		const list = [
			proc(30, 1, 10),
			proc(31, 30, 100),
			proc(32, 31, 5),
			proc(33, 30, 20),
			proc(40, 1, 50),
			proc(41, 40, 7),
		];
		const { all, parts } = jocastaProcesses({ service: 31, herdrServer: 40, surfaces: [30, 99] }, list);
		expect(all.map((p) => p.pid).sort((a, b) => a - b)).toEqual([30, 31, 32, 33, 40, 41]);
		expect([parts.service.workingSet, parts.herdr.workingSet, parts.surfaces.workingSet]).toEqual([105, 57, 30]);
	});

	test("herdr's server is the parent of the first pane process that is running (BEN-18)", () => {
		const list = parseWindowsProcesses(
			JSON.stringify([
				{ ProcessId: 40, ParentProcessId: 1, Name: "herdr.exe" },
				{ ProcessId: 41, ParentProcessId: 40, Name: "omp.exe" },
			]),
		);
		// Pane 77's process is gone; pane 41's names the server.
		expect(herdrServerOf([77, 41], list)).toBe(40);
		expect(herdrServerOf([77], list)).toBeUndefined();
		expect(herdrServerOf([], list)).toBeUndefined();
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

	test("an ask's t0 is when omp completed the message, not when it started the request (omp 18.18.2's shape)", () => {
		const start = Date.parse("2026-11-09T10:00:00Z");
		const ask = (message: Record<string, unknown>, at: number) =>
			JSON.stringify({
				type: "message",
				timestamp: new Date(at).toISOString(),
				message: { role: "assistant", content: [{ type: "toolCall", name: "ask", arguments: {} }], ...message },
			});
		// omp stamps `timestamp` before the request and `completedAt` at message_end, when the entry is written.
		expect(askTimes(ask({ timestamp: start, completedAt: start + 4_000 }, start + 4_010))).toEqual([start + 4_000]);
		// Without completedAt, the entry's time (written at message_end), never the request's start.
		expect(askTimes(ask({ timestamp: start }, start + 4_010))).toEqual([start + 4_010]);
	});

	test("a started thread's pane is the one in its worktree, <project>.wt/<slug> (BEN-23)", () => {
		// herdr reports a pane's folder in the host's own form.
		const src = join(tmpdir(), "src");
		const panes = [
			{ paneId: "boss", cwd: join(tmpdir(), "data", "home") },
			{ paneId: "other", cwd: join(src, "acme.wt", "h2-trial-10") },
			{ paneId: "mine", cwd: `${join(src, "acme.wt", "h2-trial-1")}${sep}` },
		];
		expect(paneOfBranch(panes, "feature/h2-trial-1")).toBe("mine");
		expect(paneOfBranch(panes, "feature/h2-trial-2")).toBeUndefined();
		expect(startedBranch("Started h2 trial 1 in acme on branch feature/h2-trial-1. It opens in plan mode…")).toBe(
			"feature/h2-trial-1",
		);
		expect(startedBranch("Started x in acme on branch feature/x, but omp didn't come up")).toBe("feature/x");
		expect(startedBranch("no project acme.")).toBeUndefined();
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
		status: AgentState = "unknown",
		workspaceId = "w1",
		cwd: string | null = null,
	): PaneState => ({
		paneId,
		workspaceId,
		...(cwd === null ? {} : { cwd }),
		...(agent === null ? {} : { agent }),
		agentState: status,
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

	test("the pane fault's pane is back only when omp runs in its workspace under another pane id (0521)", () => {
		const killed = pane("w2:p1", "omp", "working", "w2");
		// The dying omp, still listed under its old id, isn't back.
		expect(paneBack(killed, [pane("w2:p1", "omp", "working", "w2")])).toBe(false);
		expect(paneBack(killed, [pane("w2:p2", null, "unknown", "w2"), pane("w1:p1", "omp", "idle", "w1")])).toBe(false);
		expect(paneBack(killed, [pane("w2:p2", "omp", "idle", "w2")])).toBe(true);
	});

	test("a run counts as Jocasta closing itself only with no service and no panes (0530)", () => {
		const listed = [pane("w1:p1", "omp")];
		// Herdr gone with Jocasta, or answering with nothing: closed itself.
		expect(closedItself(false, false, null)).toBe(true);
		expect(closedItself(false, false, [])).toBe(true);
		// The service fault's own kill leaves herdr's panes: not a close, so no second restart.
		expect(closedItself(false, false, listed)).toBe(false);
		// A slow relaunch with the service up, or a recovered run: not a close.
		expect(closedItself(false, true, [])).toBe(false);
		expect(closedItself(true, false, null)).toBe(false);
	});

	test("the pane fault kills each folder's omp at most once in 10 minutes, the longest-rested folder first (0528)", () => {
		const boss = pane("boss", "omp", "working", "w1", "/r/boss");
		const t1 = pane("t1", "omp", "idle", "w2", "/r/t1");
		const list = [boss, t1, pane("sh", null, "unknown", "w3", "/r/sh")];
		// A folder never killed goes before one at work that was.
		expect(pickPane(list, new Map([[folderKey(boss), 1_000]]), 2_000)).toEqual({ pane: t1, waitMs: 0 });
		// Both killed: the one whose last run ended first, waiting until it is FOLDER_SPACING_MS past that.
		const both = new Map([
			[folderKey(boss), 5_000],
			[folderKey(t1), 3_000],
		]);
		expect(pickPane(list, both, 10_000)).toEqual({ pane: t1, waitMs: 3_000 + FOLDER_SPACING_MS - 10_000 });
		expect(pickPane(list, both, 3_000 + FOLDER_SPACING_MS + 1)?.waitMs).toBe(0);
		expect(pickPane([pane("sh", null)], new Map(), 0)).toBeUndefined();
		// 0521 keys folders without case or slash differences on Windows, so the bench does too.
		const windows = process.platform === "win32";
		expect(
			folderKey(pane("a", "omp", "unknown", "w1", "/r/Demo")) ===
				folderKey(pane("b", "omp", "unknown", "w1", "/r/demo")),
		).toBe(windows);
		expect(
			folderKey(pane("a", "omp", "unknown", "w1", "C:\\Users\\T\\demo")) ===
				folderKey(pane("b", "omp", "unknown", "w1", "c:/users/t/demo")),
		).toBe(windows);
	});

	test("a restart's client ends root first and never the new service's tree (0528)", () => {
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
		const before = [
			pane("a", "omp", "idle", "w1"),
			pane("b", "omp", "working", "w2"),
			pane("c", null, "unknown", "w3"),
		];
		// Relaunched panes with new ids count; the shell-only workspace needs nothing.
		expect(ompBack(before, [pane("x", "omp", "idle", "w2"), pane("y", "omp", "idle", "w1")])).toBe(true);
		// w2's pane is back but without omp.
		expect(ompBack(before, [pane("x", "omp", "idle", "w1"), pane("b", null, "unknown", "w2")])).toBe(false);
		// w2 lists no pane at all.
		expect(ompBack(before, [pane("a", "omp", "idle", "w1"), pane("c", null, "unknown", "w3")])).toBe(false);
	});

	test("the plugin's today pane is no thread: left out of the panes H3 compares (0553)", () => {
		const listed = [
			{ ...pane("w1:p1", "omp", "idle", "w1"), label: "boss" },
			{ ...pane("w1:p2", null, "unknown", "w1"), label: "today" },
			{ ...pane("w2:p1", "omp", "working", "w2"), label: "today" },
		];
		expect(threadPanes(listed).map((p) => p.paneId)).toEqual(["w1:p1", "w2:p1"]);
		// today opened again under a new id doesn't make the panes differ.
		const after = threadPanes([listed[0], { ...listed[1], paneId: "w1:p3" }, listed[2]] as typeof listed);
		expect(samePaneIds(threadPanes(listed), after)).toBe(true);
	});

	test("the herdr fault notes when the old pane ids were first listed, beside omp being back (0522)", () => {
		const before = [pane("a", "omp", "working", "w1"), pane("b", "omp", "idle", "w2")];
		let clock = 1_000;
		const watch = herdrRecovery(before, 1_000, () => clock);
		clock = 1_250;
		expect(watch.check(null)).toBe(false);
		clock = 3_300;
		// herdr is back with the old panes, not yet running omp.
		expect(watch.check([pane("a", null, "unknown", "w1"), pane("b", null, "unknown", "w2")])).toBe(false);
		clock = 3_600;
		expect(watch.check([pane("x", "omp", "idle", "w1"), pane("b", null, "unknown", "w2")])).toBe(false);
		clock = 8_800;
		expect(watch.check([pane("x", "omp", "idle", "w1"), pane("y", "omp", "idle", "w2")])).toBe(true);
		expect(watch.oldPaneIdsMs()).toBe(2_300);

		const missed = herdrRecovery(before, 0, () => 500);
		expect(missed.check([pane("x", "omp", "idle", "w1"), pane("y", "omp", "idle", "w2")])).toBe(true);
		expect(missed.oldPaneIdsMs()).toBeNull();
	});

	test("a session file parses when every line is JSON and a session entry is anywhere in it (0523)", () => {
		expect(sessionParses('{"type":"session"}\n{"type":"message"}\n')).toBe(true);
		// omp writes a title entry first (seen on 18.13.1).
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

	test("the h4 plant thread's brief names each canary as a decision for report_result (0576)", () => {
		const canaries = makeCanaries(2, () => "000000000001");
		const brief = plantBrief(canaries);
		expect(brief).toContain("report_result tool exactly once");
		expect(brief).toContain(
			'[{"question":"build server codename","choice":"CANARY-000000000001","why":"H4 canary"},{"question":"release train codename","choice":"CANARY-000000000001","why":"H4 canary"}]',
		);
	});

	test("only answered queries count, and no bound without a positive control (0576)", () => {
		const result = (leakedTokens: string[], isError = false): QueryResult => ({
			query: "q",
			leaked: leakedTokens,
			isError,
		});
		// Graphiti down: every query errored, nothing was checked, and nothing bounds a leak rate.
		const down = scoreH4([result([], true), result([], true)], [result([], true)], 50);
		expect(down).toMatchObject({ checks: 0, erroredQueries: 2, inconclusive: true, upperBound95: null });
		// B leaked nothing, but A's own pane found no canary either: inconclusive.
		const blind = scoreH4([result([]), result([])], [result([]), result([])], 50);
		expect(blind).toMatchObject({ checks: 100, graphLeaks: 0, inconclusive: true, upperBound95: null });
		expect(h4Line(blind)).toBe(
			"0 leaks in 100 answered checks (0 queries errored); positive control found no canary: inconclusive",
		);
		// A found canaries; B's 98 answered queries leaked none: the bound is over answered checks only.
		const fromB = [...Array.from({ length: 98 }, () => result([])), result([], true), result([], true)];
		const fromA = [
			...Array.from({ length: 92 }, () => result(["CANARY-1"])),
			...Array.from({ length: 8 }, () => result([])),
		];
		const clean = scoreH4(fromB, fromA, 50);
		expect(clean).toMatchObject({ checks: 4_900, inconclusive: false, upperBound95: 3 / 4_900 });
		expect(h4Line(clean)).toBe(
			"0 leaks in 4,900 answered checks (2 queries errored); positive control found canaries in 92/100 queries; 95% upper bound 3/4,900",
		);
	});

	test("a bank leak is found in mnemopi.db or its write-ahead log", () => {
		const dir = tempDir();
		const canaries = makeCanaries(
			2,
			(() => {
				let n = 0;
				return () => `${++n}`.padStart(12, "0");
			})(),
		);
		expect(bankLeaks(dir, canaries)).toEqual([]);
		writeFileSync(join(dir, "mnemopi.db"), "SQLite format 3\0nothing here");
		writeFileSync(join(dir, "mnemopi.db-wal"), `frames\0${canaries[1]?.token}\0`);
		expect(bankLeaks(dir, canaries)).toEqual(["CANARY-000000000002"]);
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

	test("Jocasta's ready point is the boss pane in workspace Jocasta, not the today pane; old rows' companionMs reads as serviceMs", () => {
		const workspaces = [
			{ id: "w1", label: "Jocasta" },
			{ id: "w2", label: "acme" },
		];
		const panes = [
			{ paneId: "w2:p1", workspaceId: "w2", agent: "omp" },
			{ paneId: "w1:p2", workspaceId: "w1", label: "today" },
			{ paneId: "w1:p1", workspaceId: "w1", label: "boss", agent: "omp" },
		];
		expect(bossPane(workspaces, panes)?.paneId).toBe("w1:p1");
		expect(bossPane([{ id: "w2", label: "acme" }], panes)).toBeUndefined();
		expect(rowServiceMs({ tool: "jocasta", mode: "cold", run: 0, readyMs: 8123, companionMs: 1450 })).toBe(1450);
		expect(rowServiceMs({ tool: "jocasta", mode: "cold", run: 0, readyMs: 8410, serviceMs: 1460 })).toBe(1460);
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
			{ encoding: "utf8", windowsHide: true },
		);
		expect(run.status).toBe(0);
		const summary = JSON.parse(readFileSync(join(out, "h7-longpath.json"), "utf8"));
		expect(summary.largestPassing).toEqual({ jocasta: 40, herdr: 40, claude: 40 });
	}, 60_000);
});

describe("H8", () => {
	test("K11: a thread's sessions since the run started, its planning session included; layer-on counters are the service's hit and reuse logs", () => {
		const dir = tempDir();
		const worktree = (n: number) => join(tmpdir(), "repo.wt", `h8-r0-n4-t${n}`);
		const header = (id: string, at: string, extra = {}) =>
			`${JSON.stringify({ type: "session", id, cwd: worktree(0), timestamp: at, ...extra })}\n`;
		const before = join(dir, "2026-11-09T09-00_old.jsonl");
		const planning = join(dir, "2026-11-09T10-00_plan.jsonl");
		const execution = join(dir, "2026-11-09T10-05_exec.jsonl");
		writeFileSync(before, header("old", "2026-11-09T09:00:00Z"));
		writeFileSync(planning, header("plan", "2026-11-09T10:00:00Z"));
		// "Approve and execute" starts a new session with no parentSession (omp's /clear).
		writeFileSync(execution, header("exec", "2026-11-09T10:05:00Z"));
		const files = threadSessionFiles(
			[
				// herdr's form of the folder: a trailing separator, and on Windows (0521) other case and slashes.
				{
					cwd: process.platform === "win32" ? `${worktree(0).toUpperCase().replaceAll("\\", "/")}/` : `${worktree(0)}/`,
					sessionPath: execution,
				},
				{ cwd: join(tmpdir(), "elsewhere"), sessionPath: join(dir, "other.jsonl") },
			],
			new Map([
				["h8-r0-n4-t0", worktree(0)],
				["h8-r0-n4-t1", worktree(1)],
			]),
			Date.parse("2026-11-09T09:30:00Z"),
		);
		expect([...files]).toEqual([["h8-r0-n4-t0", [planning, execution]]]);
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
			"test/pair1.test.ts",
		]);
		const line = parseThreadLine(
			"acme/h8 r0 n4 t0 [h8-r0-n4-t0] idle — finished its turn · overlaps h8 r0 n4 t1: src/mod0.ts; other: a.ts, b.ts · reported done",
		);
		expect(line).toEqual({
			title: "h8 r0 n4 t0",
			branch: "h8-r0-n4-t0",
			state: "idle",
			overlaps: ["h8 r0 n4 t1", "other"],
			reported: "done",
		});
		expect(parseThreadLine("No threads.")).toBeNull();
		expect(parseThreadLine("  spend $1.84 · peak context 41% (82k of 200k)")).toBeNull();
		const lines = work.map((a) => ({
			title: a.title,
			branch: a.title.replaceAll(" ", "-"),
			state: "idle",
			overlaps: a.thread === 0 ? [work[1]?.title ?? ""] : [],
			reported: null,
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
			new Map([[1, true]]),
		);
		expect(result).toMatchObject({
			threads: 4,
			wrongThreadEdits: 1,
			flaggedBeforeMerge: 1,
			conflictsAtMerge: 1,
			semanticFailures: 1,
			foundOnlyAtMerge: 1,
		});
	});

	test("an interface pair passes alone and fails together: the merged tree's bun test catches it (0576)", () => {
		const repo = tempDir();
		const git = (cwd: string, args: string[]) =>
			spawnSync("git", ["-c", "user.name=Bench", "-c", "user.email=bench@example.org", ...args], {
				cwd,
				encoding: "utf8",
				windowsHide: true,
			});
		for (const [path, text] of Object.entries(fixtureFiles(4))) {
			mkdirSync(join(repo, path, ".."), { recursive: true });
			writeFileSync(join(repo, path), text);
		}
		for (const args of [
			["init", "-q", "-b", "main"],
			["add", "-A"],
			["commit", "-q", "-m", "fixture"],
		])
			git(repo, args);
		const shared = readFileSync(join(repo, "src", "shared.ts"), "utf8");
		const use = readFileSync(join(repo, "src", "use1.ts"), "utf8");
		const side = (branch: string, file: string, text: string) => {
			git(repo, ["checkout", "-q", "-b", branch, "main"]);
			writeFileSync(join(repo, file), text);
			git(repo, ["commit", "-q", "-am", branch]);
		};
		// The rename side changes Item1 and makeItem1 in shared.ts; the user side upper-cases the label in use1.ts.
		side(
			"rename",
			"src/shared.ts",
			shared.replace(/(Item1 \{[^}]*?)label/s, "$1title").replace('label: "item 1"', 'title: "item 1"'),
		);
		side("upper", "src/use1.ts", use.replace("return item.label;", "return item.label.toUpperCase();"));
		git(repo, ["checkout", "-q", "main"]);
		const testsPass = (branch: string) => {
			git(repo, ["checkout", "-q", branch]);
			return spawnSync(process.execPath, ["test"], { cwd: repo, encoding: "utf8", windowsHide: true }).status === 0;
		};
		expect([testsPass("rename"), testsPass("upper")]).toEqual([true, true]);
		git(repo, ["checkout", "-q", "main"]);
		expect(git(repo, ["merge-tree", "--write-tree", "rename", "upper"]).status).toBe(0);
		expect(mergedTestsFail(git, repo, "rename", "upper")).toBe(true);
		// An unrelated clean pair keeps the tests passing.
		expect(mergedTestsFail(git, repo, "rename", "main")).toBe(false);
	}, 60_000);

	test("a thread is finished only idle or done with a commit on its branch (0529)", () => {
		const at = (state: string) => ({
			title: "h8 r0 n8 t0",
			branch: "h8-r0-n8-t0",
			state,
			overlaps: [],
			reported: null,
		});
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
		// Setup's mise files name mise; a word that only contains it doesn't (0468).
		expect(
			named([
				"C:\\Users\\t\\.config\\mise\\conf.d\\jocasta.toml",
				"C:\\Users\\t\\.config\\mise\\config.p-1.override.toml",
				"C:\\Users\\t\\AppData\\Local\\acme\\promise\\x.js",
			]),
		).toEqual([
			"C:\\Users\\t\\.config\\mise\\conf.d\\jocasta.toml",
			"C:\\Users\\t\\.config\\mise\\config.p-1.override.toml",
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
	afterAll(() => removeTree(config));
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
		const run = spawnSync(process.execPath, [join(root, file), ...rest, "--dry-run"], {
			env,
			encoding: "utf8",
			windowsHide: true,
		});
		expect(run.stderr).toBe("");
		expect(run.status).toBe(0);
		expect(run.stdout).toContain(expected);
	});
});
