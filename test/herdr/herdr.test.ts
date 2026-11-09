import { afterEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
	type AgentState,
	connectHerdr,
	findHerdr,
	type Herdr,
	HerdrError,
	herdrPipe,
	herdrSocketPath,
	herdrVersionWarning,
	paneText,
	stopHerdrSession,
} from "../../src/frontier/server/herdr.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { pidAlive, processesIn } from "../../src/shared/proctree.ts";
import { killAll, type SleeperTree, settles, sleeperCommand, sleeperTree } from "../frontier/server/sleepers.ts";
import { poll } from "../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";

const cleanup: (() => void | Promise<void>)[] = [];
afterEach(async () => {
	for (const fn of cleanup.splice(0).reverse()) await fn();
});

async function setup(version = "0.9.4"): Promise<{ fake: FakeHerdr; herdr: Herdr; warnings: string[] }> {
	const fake = await FakeHerdr.start();
	cleanup.push(() => fake.stop());
	fake.handlers.ping = () => ({ result: { type: "pong", version, protocol: 22 } });
	const warnings: string[] = [];
	const herdr = await connectHerdr({
		session: "unused",
		socketPath: fake.path,
		herdrBin: "no-such-herdr",
		onWarning: (message) => warnings.push(message),
		watchIntervalMs: 20,
	});
	cleanup.push(() => herdr.close());
	return { fake, herdr, warnings };
}

const until = (check: () => boolean, what: string) => poll(check, { timeoutMs: 5_000, intervalMs: 10, what });

const pane = (pane_id: string, workspace_id: string, agent_status: string, extra: object = {}) => ({
	pane_id,
	tab_id: `${workspace_id}:t1`,
	workspace_id,
	terminal_id: `term-${pane_id}`,
	focused: false,
	revision: 1,
	agent_status,
	state_labels: {},
	tokens: {},
	...extra,
});

test("herdrSocketPath follows herdr's config dir: XDG_CONFIG_HOME, else %APPDATA% as a named pipe on Windows", () => {
	const saved = { xdg: process.env.XDG_CONFIG_HOME, appdata: process.env.APPDATA };
	cleanup.push(() => {
		if (saved.xdg === undefined) delete process.env.XDG_CONFIG_HOME;
		else process.env.XDG_CONFIG_HOME = saved.xdg;
		if (saved.appdata === undefined) delete process.env.APPDATA;
		else process.env.APPDATA = saved.appdata;
	});
	delete process.env.XDG_CONFIG_HOME;
	if (process.platform === "win32") {
		process.env.APPDATA = "C:\\Users\\someone\\AppData\\Roaming";
		expect(herdrSocketPath("jocasta")).toBe(
			"\\\\.\\pipe\\C:\\Users\\someone\\AppData\\Roaming\\herdr\\sessions\\jocasta\\herdr.sock",
		);
		process.env.XDG_CONFIG_HOME = "C:\\cfg";
		expect(herdrSocketPath("s1")).toBe("\\\\.\\pipe\\C:\\cfg\\herdr\\sessions\\s1\\herdr.sock");
		// The socket file herdr hands its plugin commands (`HERDR_SOCKET_PATH`) is a pipe name too.
		expect(herdrPipe("C:\\cfg\\herdr\\sessions\\s1\\herdr.sock")).toBe(herdrSocketPath("s1"));
	} else {
		process.env.XDG_CONFIG_HOME = "/cfg";
		expect(herdrSocketPath("s1")).toBe("/cfg/herdr/sessions/s1/herdr.sock");
		expect(herdrPipe("/cfg/herdr/sessions/s1/herdr.sock")).toBe(herdrSocketPath("s1"));
	}
});

test("the version check warns only off 0.9.x", async () => {
	expect(herdrVersionWarning("0.9.4")).toBeUndefined();
	expect(herdrVersionWarning("0.9.12")).toBeUndefined();
	expect(herdrVersionWarning("0.10.0")).toContain("0.10.0");
	const ok = await setup("0.9.4");
	expect(ok.fake.methods()).toEqual(["ping"]);
	expect(ok.warnings).toEqual([]);
	const off = await setup("1.0.0");
	expect(off.warnings).toEqual(["herdr 1.0.0 is running; Jocasta is built against herdr 0.9.x and may misbehave"]);
});

test("ensureWorkspace reuses a workspace with that label and creates a missing one unfocused", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["workspace.list"] = () => ({
		result: { type: "workspace_list", workspaces: [{ workspace_id: "w1", label: "Jocasta", number: 1 }] },
	});
	fake.handlers["workspace.create"] = () => ({
		result: { type: "workspace_created", workspace: { workspace_id: "w2", label: "demo" }, tab: {}, root_pane: {} },
	});
	expect(await herdr.ensureWorkspace("Jocasta", "C:\\data\\home")).toBe("w1");
	expect(fake.last("workspace.create")).toBeUndefined();
	expect(await herdr.ensureWorkspace("demo", "C:\\src\\demo")).toBe("w2");
	expect(fake.last("workspace.create")?.params).toEqual({ cwd: "C:\\src\\demo", label: "demo", focus: false });
});

test("workspaces take each workspace's cwd from its first pane", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["workspace.list"] = () => ({
		result: {
			type: "workspace_list",
			workspaces: [
				{ workspace_id: "w1", label: "Jocasta" },
				{ workspace_id: "w2", label: "empty" },
			],
		},
	});
	fake.handlers["pane.list"] = () => ({
		result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle", { cwd: "C:\\data\\home" })] },
	});
	expect(await herdr.workspaces()).toEqual([
		{ id: "w1", label: "Jocasta", cwd: "C:\\data\\home" },
		{ id: "w2", label: "empty" },
	]);
});

test("createTab applies a one-pane layout running the argv command", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: {
				workspace_id: "w2",
				tab_id: "w2:t3",
				zoomed: false,
				focused_pane_id: "w2:p5",
				root: { type: "pane", pane_id: "w2:p5" },
			},
		},
	});
	const tab = await herdr.createTab("w2", {
		title: "fix login",
		cwd: "C:\\src\\demo",
		command: ["omp", "--continue"],
		env: { A: "1" },
	});
	expect(tab).toEqual({ tabId: "w2:t3", paneId: "w2:p5" });
	expect(fake.last("layout.apply")?.params).toEqual({
		workspace_id: "w2",
		tab_label: "fix login",
		focus: false,
		root: { type: "pane", cwd: "C:\\src\\demo", command: ["omp", "--continue"], env: { A: "1" } },
	});
});

test("createWorktree creates the checkout, then replaces its shell tab with one running the command", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["worktree.create"] = () => ({
		result: {
			type: "worktree_created",
			workspace: { workspace_id: "w4", label: "fix login" },
			tab: { tab_id: "w4:t1" },
			root_pane: pane("w4:p1", "w4", "unknown"),
			worktree: { path: "C:\\wt\\demo\\fix-login", label: "fix-login" },
		},
	});
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: { workspace_id: "w4", tab_id: "w4:t2", root: { type: "pane", pane_id: "w4:p2" } },
		},
	});
	const created = await herdr.createWorktree("w2", {
		repo: "C:\\src\\demo",
		branch: "fix-login",
		path: "C:\\wt\\demo\\fix-login",
		title: "fix login",
		command: ["omp", "--continue", "--config", "C:\\data\\projects\\demo\\omp.yml"],
	});
	expect(created).toEqual({ workspaceId: "w4", tabId: "w4:t2", paneId: "w4:p2", path: "C:\\wt\\demo\\fix-login" });
	expect(fake.methods()).toEqual(["ping", "worktree.create", "pane.list", "layout.apply"]);
	expect(fake.last("worktree.create")?.params).toEqual({
		workspace_id: "w2",
		branch: "fix-login",
		path: "C:\\wt\\demo\\fix-login",
		label: "fix login",
		focus: false,
	});
	expect(fake.last("layout.apply")?.params).toEqual({
		tab_id: "w4:t1",
		tab_label: "fix login",
		focus: false,
		root: {
			type: "pane",
			cwd: "C:\\wt\\demo\\fix-login",
			command: ["omp", "--continue", "--config", "C:\\data\\projects\\demo\\omp.yml"],
		},
	});
});

test("createWorktree passes the base, and what prepare returns joins the pane's env before the pane starts (0454, 0457)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["worktree.create"] = () => ({
		result: {
			type: "worktree_created",
			workspace: { workspace_id: "w4", label: "x" },
			tab: { tab_id: "w4:t1" },
			root_pane: pane("w4:p1", "w4", "unknown"),
			worktree: { path: "C:\\wt\\x", label: "x" },
		},
	});
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: { workspace_id: "w4", tab_id: "w4:t2", root: { type: "pane", pane_id: "w4:p2" } },
		},
	});
	const prepared: string[] = [];
	await herdr.createWorktree("w2", {
		repo: "C:\\src\\demo",
		branch: "feature/x",
		base: "main",
		path: "C:\\wt\\x",
		title: "x",
		command: ["omp"],
		env: { A: "1" },
		prepare: async (path) => {
			// The checkout exists, the pane doesn't yet.
			prepared.push(path, ...fake.methods());
			return { B: "2" };
		},
	});
	expect(prepared).toEqual(["C:\\wt\\x", "ping", "worktree.create"]);
	expect(fake.last("worktree.create")?.params).toMatchObject({ branch: "feature/x", base: "main" });
	expect(fake.last("layout.apply")?.params).toMatchObject({ root: { env: { A: "1", B: "2" } } });
});

test("splitBeside starts the command in a temporary tab and moves its pane beside the target", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.get"] = () => ({ result: { type: "pane_info", pane: pane("w4:p2", "w4", "idle") } });
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: { workspace_id: "w4", tab_id: "w4:t9", root: { type: "pane", pane_id: "w4:p7" } },
		},
	});
	fake.handlers["pane.move"] = () => ({
		result: { type: "pane_move", move_result: { changed: true, pane: pane("w4:p7", "w4", "unknown") } },
	});
	const id = await herdr.splitBeside("w4:p2", {
		command: ["hunk", "diff", "main...fix-login", "--watch"],
		cwd: "C:\\wt\\demo\\fix-login",
		ratio: 0.4,
	});
	expect(id).toBe("w4:p7");
	expect(fake.last("layout.apply")?.params).toEqual({
		workspace_id: "w4",
		focus: false,
		root: { type: "pane", cwd: "C:\\wt\\demo\\fix-login", command: ["hunk", "diff", "main...fix-login", "--watch"] },
	});
	expect(fake.last("pane.move")?.params).toEqual({
		pane_id: "w4:p7",
		destination: { type: "tab", tab_id: "w4:t1", target_pane_id: "w4:p2", split: "right", ratio: 0.4 },
		focus: false,
	});

	// A refused move (zoomed tab) closes the temporary tab and throws herdr's reason.
	fake.handlers["pane.move"] = () => ({
		result: {
			type: "pane_move",
			move_result: { changed: false, reason: "zoomed_tab", pane: pane("w4:p7", "w4", "unknown") },
		},
	});
	fake.handlers["tab.close"] = () => ({ result: { type: "ok" } });
	const refused = await herdr
		.splitBeside("w4:p2", { command: ["hunk"], cwd: "C:\\wt", direction: "down" })
		.catch((error: unknown) => error);
	expect(refused).toMatchObject({ code: "zoomed_tab" });
	expect(fake.last("tab.close")?.params).toEqual({ tab_id: "w4:t9" });
	expect(fake.last("pane.move")?.params.destination).toMatchObject({ split: "down" });
});

test("splitBeside runs the command with its env and names the new pane", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.get"] = () => ({ result: { type: "pane_info", pane: pane("w4:p2", "w4", "idle") } });
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: { workspace_id: "w4", tab_id: "w4:t9", root: { type: "pane", pane_id: "w4:p7" } },
		},
	});
	fake.handlers["pane.move"] = () => ({
		result: { type: "pane_move", move_result: { changed: true, pane: pane("w4:p7", "w4", "unknown") } },
	});
	fake.handlers["pane.rename"] = () => ({ result: { type: "ok" } });
	await herdr.splitBeside("w4:p2", { command: ["hunk"], cwd: "C:\\wt", env: { HUNK_MCP_PORT: "1" }, label: "review" });
	expect(fake.last("layout.apply")?.params.root).toEqual({
		type: "pane",
		cwd: "C:\\wt",
		command: ["hunk"],
		env: { HUNK_MCP_PORT: "1" },
	});
	expect(fake.last("pane.rename")?.params).toEqual({ pane_id: "w4:p7", label: "review" });
});

test("prompt and sendText strip ESC and other control characters, so outside text can't end the paste (SEC-2)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["agent.prompt"] = () => ({ result: { type: "ok" } });
	fake.handlers["pane.send_text"] = () => ({ result: { type: "ok" } });
	await herdr.prompt("w4:p2", "title\u001b[201~\r!echo pwned\u0007\u009b2J\u007f\nnext\tline «ok» é");
	await herdr.sendText("w4:p2", "a\u001bb");
	expect(fake.last("agent.prompt")?.params.text).toBe("title[201~!echo pwned2J\nnext\tline «ok» é");
	expect(fake.last("pane.send_text")?.params.text).toBe("ab");
	expect(paneText("plain")).toBe("plain");
});

test("snapshot reads workspaces and panes once each; closeWorkspace and ttl'd metadata send herdr's params", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["workspace.list"] = () => ({
		result: { type: "workspace_list", workspaces: [{ workspace_id: "w1", label: "Jocasta" }] },
	});
	fake.handlers["pane.list"] = () => ({
		result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle", { cwd: "C:\\h" })] },
	});
	fake.handlers["workspace.close"] = () => ({ result: { type: "ok" } });
	fake.handlers["pane.report_metadata"] = () => ({ result: { type: "ok" } });
	fake.handlers["workspace.report_metadata"] = () => ({ result: { type: "ok" } });
	const before = fake.requests.length;
	const snapshot = await herdr.snapshot();
	expect(snapshot.workspaces).toEqual([{ id: "w1", label: "Jocasta", cwd: "C:\\h" }]);
	expect(snapshot.panes.map((p) => p.paneId)).toEqual(["w1:p1"]);
	expect(
		fake.requests
			.slice(before)
			.map((r) => r.method)
			.sort(),
	).toEqual(["pane.list", "workspace.list"]);
	await herdr.closeWorkspace("w2");
	expect(fake.last("workspace.close")?.params).toEqual({ workspace_id: "w2" });
	await herdr.reportPaneMetadata("w1:p1", { tokens: { cost: "$1" }, ttlMs: 60_000 });
	expect(fake.last("pane.report_metadata")?.params).toEqual({
		pane_id: "w1:p1",
		source: "jocasta",
		tokens: { cost: "$1" },
		ttl_ms: 60_000,
	});
	await herdr.reportWorkspaceMetadata("w1", { running: null }, 60_000);
	expect(fake.last("workspace.report_metadata")?.params).toEqual({
		workspace_id: "w1",
		source: "jocasta",
		tokens: { running: null },
		ttl_ms: 60_000,
	});
});

test("watch: a state pushed while the subscription's reconcile read is in flight isn't overwritten by that read (CR-09)", async () => {
	const { fake, herdr } = await setup();
	let lists = 0;
	fake.handlers["pane.list"] = () => {
		// The reconcile read (the second list): herdr pushes `working` before answering with its older `idle`.
		if (++lists === 2) {
			fake.push("pane.agent_status_changed", { pane_id: "w2:p1", workspace_id: "w2", agent_status: "working" });
		}
		return { result: { type: "pane_list", panes: [pane("w2:p1", "w2", "idle")] } };
	};
	const seen: AgentState[] = [];
	const stop = herdr.watch(
		() => ["w2:p1"],
		(event) => seen.push(event.state),
	);
	await until(() => seen.length > 0 && lists >= 2, "the subscription and its reconcile read");
	await herdr.panes();
	stop();
	expect(seen).toEqual(["working"]);
});

test("prompt, sendText, stop, focus and metadata send herdr's methods and params", async () => {
	const { fake, herdr } = await setup();
	for (const method of [
		"agent.prompt",
		"pane.send_text",
		"pane.send_keys",
		"pane.focus",
		"pane.report_metadata",
		"workspace.report_metadata",
	]) {
		fake.handlers[method] = () => ({ result: { type: "ok" } });
	}
	await herdr.prompt("w4:p2", "add a README line");
	await herdr.sendText("w4:p2", "/handoff");
	await herdr.stop("w4:p2");
	await herdr.focus("w4:p2");
	await herdr.reportPaneMetadata("w4:p2", {
		tokens: { model: "opus", cost: "$0.42" },
		stateLabels: { done: "review ready" },
	});
	await herdr.reportWorkspaceMetadata("w4", { running: "2" });
	expect(fake.requests.slice(1).map(({ method, params }) => ({ method, params }))).toEqual([
		{ method: "agent.prompt", params: { target: "w4:p2", text: "add a README line" } },
		{ method: "pane.send_text", params: { pane_id: "w4:p2", text: "/handoff" } },
		{ method: "pane.send_keys", params: { pane_id: "w4:p2", keys: ["esc"] } },
		{ method: "pane.focus", params: { pane_id: "w4:p2" } },
		{
			method: "pane.report_metadata",
			params: {
				pane_id: "w4:p2",
				source: "jocasta",
				tokens: { model: "opus", cost: "$0.42" },
				state_labels: { done: "review ready" },
			},
		},
		{
			method: "workspace.report_metadata",
			params: { workspace_id: "w4", source: "jocasta", tokens: { running: "2" } },
		},
	]);
});

test("each request uses its own connection; an error response rejects with herdr's code", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["agent.prompt"] = () => ({
		error: { code: "agent_blocked", message: "agent w4:p2 is blocked and requires interactive input" },
	});
	fake.handlers["pane.list"] = () => ({ result: { type: "pane_list", panes: [] } });
	const blocked = await herdr.prompt("w4:p2", "go").catch((error: unknown) => error);
	expect(blocked).toBeInstanceOf(HerdrError);
	expect(blocked).toMatchObject({ code: "agent_blocked" });
	expect(await herdr.panes()).toEqual([]);
	expect(await herdr.panes()).toEqual([]);
	expect(fake.connections).toBe(4);
});

test("a read herdr closes before answering is asked again once; a second drop fails (0517)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.list"] = () => ({ result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle")] } });
	fake.drops["pane.list"] = 1;
	expect((await herdr.panes()).map((p) => p.paneId)).toEqual(["w1:p1"]);
	expect(fake.methods().filter((m) => m === "pane.list")).toHaveLength(2);
	fake.drops["pane.list"] = 2;
	await expect(herdr.panes()).rejects.toMatchObject({ code: "transport" });
});

test("a change herdr closes before answering is never sent again: herdr may have made it (0517)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["agent.prompt"] = () => ({ result: { type: "ok" } });
	fake.drops["agent.prompt"] = 1;
	await expect(herdr.prompt("w4:p2", "go")).rejects.toThrow("closed the connection before answering agent.prompt");
	expect(fake.methods().filter((m) => m === "agent.prompt")).toHaveLength(1);
});

test("panes map herdr's pane list, with the omp session file when herdr has one", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.list"] = () => ({
		result: {
			type: "pane_list",
			panes: [
				pane("w1:p1", "w1", "working", {
					cwd: "C:\\data\\home",
					agent: "omp",
					label: "boss",
					agent_session: { source: "herdr:omp", agent: "omp", kind: "path", value: "C:\\s\\a.jsonl" },
				}),
				pane("w2:p1", "w2", "something-new", { agent_session: { kind: "id", value: "abc" } }),
			],
		},
	});
	expect(await herdr.panes()).toEqual([
		{
			paneId: "w1:p1",
			tabId: "w1:t1",
			workspaceId: "w1",
			cwd: "C:\\data\\home",
			agentState: "working",
			agent: "omp",
			sessionPath: "C:\\s\\a.jsonl",
			label: "boss",
			terminalId: "term-w1:p1",
		},
		{ paneId: "w2:p1", tabId: "w2:t1", workspaceId: "w2", agentState: "unknown", terminalId: "term-w2:p1" },
	]);
	expect(fake.last("pane.list")?.params).toEqual({});
});

test("moveWorkspace and respawn send workspace.move and a layout over the pane's tab", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["workspace.move"] = () => ({ result: { type: "ok" } });
	fake.handlers["pane.get"] = () => ({ result: { type: "pane_info", pane: pane("w1:p1", "w1", "unknown") } });
	fake.handlers["layout.apply"] = () => ({
		result: {
			type: "layout_apply",
			layout: { workspace_id: "w1", tab_id: "w1:t1", root: { type: "pane", pane_id: "w1:p2" } },
		},
	});
	await herdr.moveWorkspace("w1", 0);
	expect(await herdr.respawn("w1:p1", { cwd: "C:\\data\\home", command: ["omp", "--continue"] })).toEqual({
		tabId: "w1:t1",
		paneId: "w1:p2",
	});
	expect(fake.requests.slice(1).map(({ method, params }) => ({ method, params }))).toEqual([
		{ method: "workspace.move", params: { workspace_id: "w1", insert_index: 0 } },
		{ method: "pane.get", params: { pane_id: "w1:p1" } },
		{ method: "pane.list", params: {} },
		{
			method: "layout.apply",
			params: {
				tab_id: "w1:t1",
				focus: false,
				root: { type: "pane", cwd: "C:\\data\\home", command: ["omp", "--continue"] },
			},
		},
	]);
});

test("watch subscribes per pane, reports the current state then changes, follows the pane set and resubscribes after events_lost", async () => {
	const { fake, herdr } = await setup();
	const states: Record<string, string> = { "w2:p1": "idle", "w3:p1": "working" };
	fake.handlers["pane.list"] = () => ({
		result: { type: "pane_list", panes: Object.entries(states).map(([id, s]) => pane(id, id.split(":")[0] ?? "", s)) },
	});
	let wanted = ["w2:p1", "w9:p9"];
	const seen: { paneId: string; workspaceId: string; state: AgentState }[] = [];
	const stop = herdr.watch(
		() => wanted,
		(event) => seen.push(event),
	);

	await until(() => seen.length === 1, "the initial state");
	expect(seen[0]).toEqual({ paneId: "w2:p1", workspaceId: "w2", state: "idle" });
	// A pane herdr doesn't know is left out of the subscription (herdr would reject the whole request).
	expect(fake.last("events.subscribe")?.params).toEqual({
		subscriptions: [{ type: "pane.agent_status_changed", pane_id: "w2:p1" }],
	});

	fake.push("pane.agent_status_changed", { pane_id: "w2:p1", workspace_id: "w2", agent_status: "working" });
	fake.push("pane.agent_status_changed", { pane_id: "w2:p1", workspace_id: "w2", agent_status: "working" });
	await until(() => seen.length === 2, "the pushed change");
	expect(seen[1]).toEqual({ paneId: "w2:p1", workspaceId: "w2", state: "working" });

	wanted = ["w2:p1", "w3:p1"];
	await until(() => seen.some((e) => e.paneId === "w3:p1"), "the new pane");
	expect(fake.last("events.subscribe")?.params).toEqual({
		subscriptions: [
			{ type: "pane.agent_status_changed", pane_id: "w2:p1" },
			{ type: "pane.agent_status_changed", pane_id: "w3:p1" },
		],
	});

	const subscribes = fake.methods().filter((m) => m === "events.subscribe").length;
	states["w2:p1"] = "blocked";
	fake.loseEvents();
	await until(() => seen.some((e) => e.state === "blocked"), "the state read after resubscribing");
	expect(fake.methods().filter((m) => m === "events.subscribe").length).toBe(subscribes + 1);

	stop();
	await until(() => fake.subscriptions.every((s) => s.socket.closed), "the subscription to close");
});

test("stopServer sends server.stop, waits for the socket to go, and nothing starts the server again (0434)", async () => {
	const { fake, herdr } = await setup();
	// herdr goes right after it took the request (the answer may or may not get out).
	fake.handlers["server.stop"] = () => {
		queueMicrotask(() => fake.stop());
		return { result: { type: "ok" } };
	};
	await herdr.stopServer();
	expect(fake.methods()).toContain("server.stop");
	// A revive would try to start `no-such-herdr` (not_installed); the server stays down instead.
	await expect(herdr.workspaces()).rejects.toMatchObject({ code: "unreachable" });
	// Stopping again: nothing runs, nothing is sent.
	await herdr.stopServer();
});

test("keepDown: a server that stops later isn't started again (0431's restart never runs while Jocasta closes)", async () => {
	const { fake, herdr } = await setup();
	herdr.keepDown();
	fake.stop();
	await expect(herdr.panes()).rejects.toMatchObject({ code: "unreachable" });
});

test("a stopped server is started again on its socket and the request goes through; a second stop within seconds isn't (0431)", async () => {
	const fake = await FakeHerdr.start();
	cleanup.push(() => fake.stop());
	const warnings: string[] = [];
	const herdr = await connectHerdr({
		session: "unused",
		socketPath: fake.path,
		// Something that runs and exits: the restarted fake below plays the server it would start.
		herdrBin: process.execPath,
		onWarning: (message) => warnings.push(message),
		watchIntervalMs: 20,
	});
	cleanup.push(() => herdr.close());
	fake.stop();
	const listed = herdr.panes();
	// A real delay: the request's first failed connect (which triggers the revive) has no signal to await, and a
	// server back before it would serve the request without any revive.
	await Bun.sleep(300);
	const again = await FakeHerdr.start(fake.path);
	cleanup.push(() => again.stop());
	again.handlers["pane.list"] = () => ({ result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle")] } });
	expect((await listed).map((p) => p.paneId)).toEqual(["w1:p1"]);
	expect(warnings).toEqual(["herdr's server for session unused had stopped; Jocasta started it again"]);

	again.stop();
	await expect(herdr.panes()).rejects.toThrow("restarted moments ago");
}, 15_000);

test("stopHerdrSession stops one session's server and never starts one", async () => {
	const fake = await FakeHerdr.start();
	cleanup.push(() => fake.stop());
	fake.handlers["server.stop"] = () => {
		queueMicrotask(() => fake.stop());
		return { result: { type: "ok" } };
	};
	expect(await stopHerdrSession({ session: "unused", socketPath: fake.path })).toBe(true);
	// Its panes' processes are listed first (0486); this fake has no `pane.list`, so none.
	expect(fake.methods().slice(0, 3)).toEqual(["ping", "pane.list", "server.stop"]);
	expect(await stopHerdrSession({ session: "unused", socketPath: fake.path })).toBe(false);
});

/** A temp folder with a real sleeper tree working in it (`sleepers.ts`), killed after the test whatever happens. */
async function treeIn(name: string): Promise<SleeperTree> {
	const dir = mkdtempSync(join(tmpdir(), `jocasta-herdr-${name}-`));
	const tree = await sleeperTree(dir);
	cleanup.push(async () => {
		await tree.stop();
		try {
			removeTree(dir);
		} catch {
			// Under load Windows can hold a just-stopped sleeper's folder (EBUSY); a leftover temp folder is harmless.
		}
	});
	return tree;
}

/** herdr's `pane.process_info` answer for a pane whose process is `pid`. */
const processInfo = (paneId: string, pid: number) => ({
	result: {
		type: "pane_process_info",
		process_info: { pane_id: paneId, shell_pid: pid, foreground_processes: [{ pid, name: "bun.exe" }] },
	},
});

test("closeWorkspace ends every process its panes ran, with all they started; other workspaces' stay (0486)", async () => {
	const { fake, herdr } = await setup();
	const [closing, staying] = [await treeIn("closing"), await treeIn("staying")];
	fake.handlers["pane.list"] = () => ({
		result: { type: "pane_list", panes: [pane("w2:p1", "w2", "working"), pane("w1:p1", "w1", "idle")] },
	});
	fake.handlers["pane.process_info"] = (params) =>
		params.pane_id === "w2:p1" ? processInfo("w2:p1", closing.root) : processInfo("w1:p1", staying.root);
	let aliveAtClose = false;
	fake.handlers["workspace.close"] = () => {
		aliveAtClose = closing.all.every(pidAlive);
		return { result: { type: "ok" } };
	};
	await herdr.closeWorkspace("w2");
	expect(aliveAtClose).toBe(true);
	expect(closing.all.filter(pidAlive)).toEqual([]);
	expect(staying.all.filter(pidAlive)).toEqual(staying.all);
	expect(fake.requests.filter((r) => r.method === "pane.process_info").map((r) => r.params)).toEqual([
		{ pane_id: "w2:p1" },
	]);
}, 30_000);

test("ensureWorkspace ends the default shell its command's layout replaced; another tab's processes stay", async () => {
	// herdr 0.9.5 on Windows drops the replaced pane but can leave its shell running, listed nowhere after.
	const { fake, herdr } = await setup();
	const [shell, otherTab] = [await treeIn("shell"), await treeIn("other-tab")];
	fake.handlers["workspace.list"] = () => ({ result: { type: "workspace_list", workspaces: [] } });
	fake.handlers["workspace.create"] = () => ({
		result: {
			type: "workspace_created",
			workspace: { workspace_id: "w2", label: "demo" },
			tab: { tab_id: "w2:t1" },
			root_pane: pane("w2:p1", "w2", "unknown"),
		},
	});
	fake.handlers["pane.list"] = () => ({
		result: {
			type: "pane_list",
			panes: [pane("w2:p1", "w2", "unknown"), pane("w2:p9", "w2", "idle", { tab_id: "w2:t2" })],
		},
	});
	fake.handlers["pane.process_info"] = (params) =>
		params.pane_id === "w2:p1" ? processInfo("w2:p1", shell.root) : processInfo("w2:p9", otherTab.root);
	let aliveAtApply = false;
	fake.handlers["layout.apply"] = () => {
		aliveAtApply = shell.all.every(pidAlive);
		return {
			result: {
				type: "layout_apply",
				layout: { workspace_id: "w2", tab_id: "w2:t1", root: { type: "pane", pane_id: "w2:p2" } },
			},
		};
	};
	expect(await herdr.ensureWorkspace("demo", "C:\\src\\demo", ["omp", "--continue"])).toBe("w2");
	expect(aliveAtApply).toBe(true);
	expect(shell.all.filter(pidAlive)).toEqual([]);
	expect(otherTab.all.filter(pidAlive)).toEqual(otherTab.all);
	expect(fake.requests.filter((r) => r.method === "pane.process_info").map((r) => r.params)).toEqual([
		{ pane_id: "w2:p1" },
	]);
}, 30_000);

test("stopServer ends every pane's processes, with all they started, once the server is gone (0486)", async () => {
	const { fake, herdr } = await setup();
	const [first, second] = [await treeIn("first"), await treeIn("second")];
	fake.handlers["pane.list"] = () => ({
		result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle"), pane("w2:p1", "w2", "working")] },
	});
	fake.handlers["pane.process_info"] = (params) =>
		params.pane_id === "w1:p1" ? processInfo("w1:p1", first.root) : processInfo("w2:p1", second.root);
	let aliveAtStop = false;
	fake.handlers["server.stop"] = () => {
		aliveAtStop = [...first.all, ...second.all].every(pidAlive);
		queueMicrotask(() => fake.stop());
		return { result: { type: "ok" } };
	};
	await herdr.stopServer();
	expect(aliveAtStop).toBe(true);
	expect([...first.all, ...second.all].filter(pidAlive)).toEqual([]);
}, 30_000);

test("paneRoot names the process herdr started for a pane, or none when herdr names none (0490)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.process_info"] = (params) =>
		params.pane_id === "w1:p1"
			? processInfo("w1:p1", 4242)
			: { result: { type: "pane_process_info", process_info: { pane_id: params.pane_id, shell_pid: null } } };
	expect(await herdr.paneRoot("w1:p1")).toBe(4242);
	expect(await herdr.paneRoot("w2:p1")).toBeUndefined();
});

test.skipIf(process.env.JOCASTA_HERDR_SMOKE !== "1")(
	"live smoke: a throwaway herdr session runs commands in a workspace's tab, a split and a worktree (JOCASTA_HERDR_SMOKE=1)",
	async () => {
		const bin = findHerdr();
		if (!bin) throw new Error("herdr isn't installed");
		const session = `jocasta-test-${Math.random().toString(36).slice(2, 8)}`;
		const root = mkdtempSync(join(tmpdir(), "jocasta-herdr-smoke-"));
		const cwd = join(root, "demo");
		mkdirSync(cwd);
		// The worktree lands at the path given (0445), else in the temp dir, never herdr's default ~/.herdr/worktrees; no
		// toasts or sounds.
		const configPath = join(root, "herdr.toml");
		writeFileSync(
			configPath,
			`[worktrees]\ndirectory = "${join(root, "wt").replaceAll("\\", "/")}"\n\n[ui.toast]\ndelivery = "off"\n`,
		);
		const git = (...args: string[]) =>
			Bun.spawnSync(["git", "-C", cwd, ...args], { stdout: "ignore", windowsHide: true });
		git("init", "-q", "-b", "main");
		git(
			"-c",
			"user.name=smoke",
			"-c",
			"user.email=smoke@example.invalid",
			"commit",
			"-q",
			"--allow-empty",
			"-m",
			"init",
		);
		const warnings: string[] = [];
		let herdr: Herdr | undefined;
		try {
			herdr = await connectHerdr({ session, configPath, herdrBin: bin, onWarning: (m) => warnings.push(m) });
			const workspaceId = await herdr.ensureWorkspace("smoke", cwd);
			expect(await herdr.ensureWorkspace("smoke", cwd)).toBe(workspaceId);
			const command =
				process.platform === "win32" ? ["cmd", "/c", "echo hi & pause"] : ["sh", "-c", "echo hi; sleep 30"];
			const tab = await herdr.createTab(workspaceId, { title: "echo", cwd, command });
			const split = await herdr.splitBeside(tab.paneId, { command, cwd, ratio: 0.4 });
			const worktreePath = join(root, "demo.wt", "smoke-thread");
			const worktree = await herdr.createWorktree(workspaceId, {
				repo: cwd,
				branch: "smoke-thread",
				path: worktreePath,
				title: "smoke thread",
				command,
			});
			await herdr.reportPaneMetadata(tab.paneId, { tokens: { model: "smoke" }, stateLabels: { done: "review ready" } });
			await herdr.reportWorkspaceMetadata(workspaceId, { running: "1" });
			const seen: string[] = [];
			const stop = herdr.watch(
				() => [tab.paneId],
				(e) => seen.push(`${e.paneId}=${e.state}`),
			);
			await until(() => seen.length > 0, "the watched pane's state");
			stop();
			const panes = await herdr.panes();
			console.log(
				JSON.stringify(
					{ session, warnings, workspaceId, tab, split, worktree, seen, panes, workspaces: await herdr.workspaces() },
					null,
					2,
				),
			);
			expect(panes.find((p) => p.paneId === tab.paneId)?.tabId).toBe(tab.tabId);
			expect(panes.find((p) => p.paneId === split)?.tabId).toBe(tab.tabId);
			expect(panes.find((p) => p.paneId === worktree.paneId)?.workspaceId).toBe(worktree.workspaceId);
			expect(panes.filter((p) => p.workspaceId === worktree.workspaceId)).toHaveLength(1);
			expect(realpathSync.native(worktree.path)).toBe(realpathSync.native(worktreePath));
		} finally {
			herdr?.close();
			for (const args of [
				["--session", session, "server", "stop"],
				["session", "delete", session],
			]) {
				const run = Bun.spawnSync([bin, ...args], { windowsHide: true });
				console.log(
					`herdr ${args.join(" ")} → exit ${run.exitCode} ${run.stdout.toString().trim()} ${run.stderr.toString().trim()}`,
				);
			}
			removeTree(root);
		}
	},
	30_000,
);

test.skipIf(process.env.JOCASTA_HERDR_SMOKE !== "1")(
	"live smoke: closing a workspace and stopping the server end every process its panes ran, a replaced default shell too (0486, JOCASTA_HERDR_SMOKE=1)",
	async () => {
		const bin = findHerdr();
		if (!bin) throw new Error("herdr isn't installed");
		const session = `jocasta-test-${Math.random().toString(36).slice(2, 8)}`;
		const root = mkdtempSync(join(tmpdir(), "jocasta-herdr-0486-"));
		const configPath = join(root, "herdr.toml");
		writeFileSync(configPath, `[ui.toast]\ndelivery = "off"\n`);
		const started: number[] = [];
		let herdr: Herdr | undefined;
		try {
			herdr = await connectHerdr({ session, configPath, herdrBin: bin });
			const trees: number[][] = [];
			for (const name of ["closed", "stopped"]) {
				const cwd = join(root, name);
				mkdirSync(cwd);
				const { command, started: pids } = sleeperCommand(cwd);
				if (name === "closed") {
					const workspaceId = await herdr.ensureWorkspace(name, cwd);
					await herdr.createTab(workspaceId, { title: name, cwd, command });
				} else {
					// Its command replaces the default shell, as the boss and project workspaces are made.
					await herdr.ensureWorkspace(name, cwd, command);
				}
				expect(await settles(() => pids().length === 3)).toBe(true);
				started.push(...pids());
				trees.push(pids());
			}
			const [closed = [], stopped = []] = trees;
			const closing = (await herdr.workspaces()).find((w) => w.label === "closed")?.id as string;
			await herdr.closeWorkspace(closing);
			expect(closed.filter(pidAlive)).toEqual([]);
			expect(await settles(() => processesIn(join(root, "closed")).length === 0)).toBe(true);
			expect(stopped.filter(pidAlive)).toEqual(stopped);
			await herdr.stopServer();
			expect(stopped.filter(pidAlive)).toEqual([]);
			expect(await settles(() => processesIn(join(root, "stopped")).length === 0)).toBe(true);
		} finally {
			herdr?.close();
			killAll(started);
			Bun.spawnSync([bin, "--session", session, "server", "stop"], { windowsHide: true });
			Bun.spawnSync([bin, "session", "delete", session], { windowsHide: true });
			removeTree(root);
		}
	},
	60_000,
);

test("a watch stopped while herdr answers its subscribe leaves no subscription open (SRV-A-7)", async () => {
	const { fake, herdr } = await setup();
	fake.handlers["pane.list"] = () => ({ result: { type: "pane_list", panes: [pane("w1:p1", "w1", "idle")] } });
	let stop: (() => void) | undefined;
	fake.handlers["events.subscribe"] = () => {
		// The stop lands while herdr takes the request, before its answer arrives.
		stop?.();
		return { result: { type: "subscription_started" } };
	};
	stop = herdr.watch(
		() => ["w1:p1"],
		() => {},
	);
	await until(() => fake.subscriptions.length === 1, "the subscribe");
	expect(await settles(() => fake.subscriptions.every((s) => s.socket.destroyed))).toBe(true);
});

test("connecting with start off never starts a server: no answer is unreachable (SRV-A-10)", async () => {
	const name = `jocasta-herdr-none-${process.pid}`;
	const socketPath = process.platform === "win32" ? `\\\\.\\pipe\\${name}` : join(tmpdir(), `${name}.sock`);
	await expect(
		// `no-such-herdr` would fail as not_installed if a start were tried.
		connectHerdr({ session: "unused", socketPath, herdrBin: "no-such-herdr", start: false }),
	).rejects.toMatchObject({ code: "unreachable" });
});
