import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { bossCommand, bossToolsMissing, ensureBossPane, LaunchedPanes } from "../../../src/frontier/server/boss.ts";
import type { PaneInfo } from "../../../src/frontier/server/herdr.ts";
import { RELAY_PORT, relayUrl, shareUrl } from "../../../src/frontier/server/relay.ts";
import { shippedConfigDir } from "../../../src/shared/install.ts";
import { useTempJocasta } from "../../shared/fixture.ts";
import { FakeHerdr } from "./fake-herdr.ts";

const env = useTempJocasta();

/** A boss pane's env with its pane key (0578), whatever key it got. */
function keyed(vars: Record<string, string> = {}): Record<string, string> {
	return { ...vars, JOCASTA_PANE_KEY: expect.any(String) as unknown as string };
}

test("workspace Jocasta is made first, made first again after he closes it, with the guard, its manual as APPEND_SYSTEM.md and its skills (0404, 0424, 0450, 0465)", async () => {
	const herdr = new FakeHerdr();
	await herdr.ensureWorkspace("demo", env.root);
	const command = bossCommand(["omp"]);
	expect(command).toEqual([
		"omp",
		"--continue",
		"--config",
		join(shippedConfigDir(), "boss.yml"),
		"--config",
		join(shippedConfigDir(), "guard.yml"),
	]);
	const launched = new LaunchedPanes();
	const pane = await ensureBossPane(herdr, { command, launched });
	expect(herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta", "demo"]);
	expect(herdr.of("ensureWorkspace").at(-1)).toEqual(["Jocasta", join(env.data, "home"), command, keyed()]);
	// Its pane key is kept with the pane, in panes.json, so a later run knows it (0578).
	const key = herdr.envOf(pane.paneId)?.JOCASTA_PANE_KEY as string;
	expect(key).toMatch(/^[0-9a-f]{64}$/);
	expect(launched.keyOf(pane.paneId)).toBe(key);
	expect(new LaunchedPanes().paneOfKey(key)).toBe(pane.paneId);
	// One character off: the last one changed, whatever it was.
	const forged = `${key.slice(0, -1)}${key.endsWith("0") ? "1" : "0"}`;
	expect(new LaunchedPanes().paneOfKey(forged)).toBeUndefined();
	expect(new LaunchedPanes().paneOfKey(undefined)).toBeUndefined();
	const manual = join(env.data, "home", ".omp", "APPEND_SYSTEM.md");
	expect(readFileSync(manual, "utf8")).toBe(readFileSync(join(shippedConfigDir(), "boss-manual.md"), "utf8"));
	expect(launched.has(pane.paneId)).toBe(true);
	expect(existsSync(join(env.data, "panes.json"))).toBe(true);
	// The rare procedures are skills in the boss's folder; save-lesson carries the shipped TDD lesson (0461).
	const skills = join(env.data, "home", ".omp", "skills");
	for (const name of ["save-lesson", "mise-setup", "whiteboard-review", "leftover-worktrees"]) {
		expect(readFileSync(join(skills, name, "SKILL.md"), "utf8")).toBe(
			readFileSync(join(shippedConfigDir(), "boss-skills", name, "SKILL.md"), "utf8"),
		);
	}
	expect(readFileSync(join(skills, "save-lesson", "tdd.md"), "utf8")).toBe(
		readFileSync(join(shippedConfigDir(), "lessons", "tdd.md"), "utf8"),
	);

	// Already there and first: nothing moves, nothing is made.
	const calls = herdr.calls.length;
	expect((await ensureBossPane(herdr, { command, launched })).paneId).toBe(pane.paneId);
	expect(herdr.calls.slice(calls).map((c) => c.method)).toEqual([]);

	await herdr.closeWorkspace(pane.workspaceId);
	const again = await ensureBossPane(herdr, { command, launched });
	expect(again.paneId).not.toBe(pane.paneId);
	expect(herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta", "demo"]);
	expect(herdr.of("moveWorkspace")).toHaveLength(2);
});

test("the boss pane's omp starts with its env (the run's boss key), and a stale one starts again with it (0484)", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	const launched = new LaunchedPanes();
	const env1 = { JOCASTA_BOSS_KEY: "run-1" };
	const started: string[] = [];
	const pane = await ensureBossPane(herdr, { command, env: env1, launched, started: (id) => started.push(id) });
	expect(herdr.of("ensureWorkspace").at(-1)).toEqual(["Jocasta", join(env.data, "home"), command, keyed(env1)]);
	expect(started).toEqual([pane.paneId]);
	herdr.setState(pane.paneId, "idle");

	// A later run: the pane's omp lacks the new key; it isn't stale while working, and starts again once idle.
	const env2 = { JOCASTA_BOSS_KEY: "run-2" };
	const stale = (p: { paneId: string; agentState: string }) => p.agentState === "idle";
	herdr.setState(pane.paneId, "working");
	const kept = await ensureBossPane(herdr, { command, env: env2, launched, stale, started: (id) => started.push(id) });
	expect(kept.paneId).toBe(pane.paneId);
	expect(herdr.of("respawn")).toEqual([]);
	herdr.setState(pane.paneId, "idle");
	const again = await ensureBossPane(herdr, { command, env: env2, launched, stale, started: (id) => started.push(id) });
	expect(herdr.of("respawn")).toEqual([[pane.paneId, { cwd: join(env.data, "home"), command, env: keyed(env2) }]]);
	// A new start, a new key (0578).
	expect(launched.keyOf(again.paneId)).not.toBe(herdr.envOf(pane.paneId)?.JOCASTA_PANE_KEY);
	expect(started).toEqual([pane.paneId, again.paneId]);
});

test("the boss's settings carry the privacy settings, whatever his global config says (0478)", () => {
	const settings = Bun.YAML.parse(readFileSync(join(shippedConfigDir(), "boss.yml"), "utf8")) as Record<
		string,
		unknown
	>;
	expect(settings.dev).toEqual({ autoqa: false });
	expect(settings.disabledProviders).toEqual(["claude-plugins"]);
	expect(settings.modelRoles).toEqual({ web: "web/duckduckgo", dictation: "local/parakeet-tdt-0.6b-v3" });
	// Dictation through omp's own speech-to-text, sent on release, never leaving this machine (0556).
	expect(settings.stt).toEqual({ enabled: true, submitTrigger: "release" });
	expect(settings.retry).toEqual({ fallbackChains: { dictation: [] } });
});

test("the boss's session hosts in Jocasta's relay, and its /share stays on this machine (0536, 0561)", () => {
	const settings = Bun.YAML.parse(readFileSync(join(shippedConfigDir(), "boss.yml"), "utf8")) as Record<
		string,
		unknown
	>;
	expect(settings.collab).toEqual({ autoStart: "control", relayUrl: relayUrl(RELAY_PORT) });
	expect(settings.share).toEqual({ serverUrl: shareUrl(RELAY_PORT) });
});

test("the boss's omp isn't started when its check refuses it, made or respawned (0561)", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	const launched = new LaunchedPanes();
	const checked: [string[], string, Record<string, string>][] = [];
	const refuse = async (c: string[], cwd: string, e: Record<string, string>) => {
		checked.push([c, cwd, e]);
		throw new Error("not loopback");
	};
	const env1 = { JOCASTA_BOSS_KEY: "run-1" };
	await expect(ensureBossPane(herdr, { command, env: env1, launched, check: refuse })).rejects.toThrow("not loopback");
	expect(checked).toEqual([[command, join(env.data, "home"), keyed(env1)]]);
	expect(herdr.of("ensureWorkspace")).toEqual([]);
	// Started once the check passes; restored as a shell, the respawn is refused the same way.
	await ensureBossPane(herdr, { command, launched, check: async () => {} });
	launched.seen(await herdr.panes());
	herdr.restore();
	await expect(ensureBossPane(herdr, { command, launched, check: refuse })).rejects.toThrow("not loopback");
	expect(herdr.of("respawn")).toEqual([]);
});

test("bare jocasta and the service making workspace Jocasta at once leave one: the oldest, the other closed (CR-10)", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	const [a, b] = await Promise.all([
		ensureBossPane(herdr, { command, launched: new LaunchedPanes(join(env.root, "cli.json")) }),
		ensureBossPane(herdr, { command, launched: new LaunchedPanes() }),
	]);
	expect(herdr.of("ensureWorkspace")).toHaveLength(2);
	expect(herdr.workspaceList.map((w) => w.label)).toEqual(["Jocasta"]);
	expect(herdr.of("closeWorkspace")).toHaveLength(1);
	expect(a.paneId).toBe(b.paneId);
	expect(herdr.paneList.filter((p) => p.workspaceId === a.workspaceId)).toHaveLength(1);
});

test("a boss pane herdr restored as a shell runs omp again; one whose omp ended in its terminal is the service's to start again (0521), and one Jocasta didn't start is left alone", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	const launched = new LaunchedPanes();
	const pane = await ensureBossPane(herdr, { command, launched });
	launched.seen(await herdr.panes());

	// omp ended in it: the same terminal, no agent. ensureBossPane alone leaves it; the service decides (0521).
	herdr.setState(pane.paneId, "idle");
	herdr.setState(pane.paneId, "unknown");
	await ensureBossPane(herdr, { command, launched });
	expect(herdr.of("respawn")).toEqual([]);

	// herdr restarted: a new terminal in the same folder.
	herdr.restore();
	const respawned = await ensureBossPane(herdr, { command, launched });
	expect(herdr.of("respawn")).toEqual([[pane.paneId, { cwd: join(env.data, "home"), command, env: keyed() }]]);
	expect(launched.has(respawned.paneId)).toBe(true);

	// A record from another run (a fresh LaunchedPanes reads <data>/panes.json) knows it too; a pane it lacks isn't touched.
	launched.seen(await herdr.panes());
	herdr.restore();
	const other = new LaunchedPanes(join(env.root, "other-panes.json"));
	await ensureBossPane(herdr, { command, launched: other });
	expect(herdr.of("respawn")).toHaveLength(1);
	await ensureBossPane(herdr, { command, launched: new LaunchedPanes() });
	expect(herdr.of("respawn")).toHaveLength(2);
});

test("a pane's omp ended in its own terminal only once omp was seen running there; omp started there again, or a record read again, saw none (0521)", () => {
	const file = join(env.root, "ended-panes.json");
	const launched = new LaunchedPanes(file);
	const cwd = join(env.root, "work");
	mkdirSync(cwd, { recursive: true });
	const pane = (fields: Partial<PaneInfo> = {}): PaneInfo => ({
		paneId: "p1",
		tabId: "t1",
		workspaceId: "w1",
		cwd,
		terminalId: "term1",
		agentState: "unknown",
		...fields,
	});
	const running = pane({ agent: "omp", agentState: "idle" });
	launched.add("p1", cwd);
	// omp is still starting: no agent seen in this terminal yet.
	launched.seen([pane()]);
	expect(launched.ended(pane())).toBe(false);
	// omp ran, then ended in the same terminal.
	launched.seen([running]);
	expect(launched.ended(running)).toBe(false);
	expect(launched.ended(pane())).toBe(true);
	expect(launched.restored(pane())).toBe(false);
	// Another folder, or a new terminal (herdr's restore), isn't its omp ending there.
	expect(launched.ended(pane({ cwd: env.root }))).toBe(false);
	expect(launched.ended(pane({ terminalId: "term2" }))).toBe(false);
	// Jocasta started omp there again: one ending, one start.
	launched.add("p1", cwd);
	launched.seen([pane()]);
	expect(launched.ended(pane())).toBe(false);
	// A later run reads the record: it never saw that omp run.
	launched.seen([running]);
	expect(new LaunchedPanes(file).ended(pane())).toBe(false);
});

test("at the service's start a boss pane with no omp runs omp even without a record; one running omp is left alone", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	// herdr restored workspace "Jocasta" from its own saved session: a shell Jocasta has no record of.
	const pane = await ensureBossPane(herdr, { command, launched: new LaunchedPanes(join(env.root, "earlier.json")) });
	herdr.restore();
	const launched = new LaunchedPanes(join(env.root, "this-run.json"));
	await ensureBossPane(herdr, { command, launched });
	expect(herdr.of("respawn")).toEqual([]);
	const adopted = await ensureBossPane(herdr, { command, launched, adopt: true });
	expect(herdr.of("respawn")).toEqual([[pane.paneId, { cwd: join(env.data, "home"), command, env: keyed() }]]);
	expect(launched.has(adopted.paneId)).toBe(true);

	// omp runs in it: a later start leaves it alone.
	herdr.setState(adopted.paneId, "idle");
	await ensureBossPane(herdr, { command, launched: new LaunchedPanes(join(env.root, "next.json")), adopt: true });
	expect(herdr.of("respawn")).toHaveLength(1);
});

test("his own shell tab in workspace Jocasta is never taken for the boss: Jocasta's or omp's pane is, else a new tab (CO-21)", async () => {
	const herdr = new FakeHerdr();
	const command = bossCommand(["omp"]);
	const launched = new LaunchedPanes();
	const boss = await ensureBossPane(herdr, { command, launched });
	// He opened a shell tab there, before the boss's in herdr's list.
	const shell = await herdr.createTab(boss.workspaceId, { title: "shell", cwd: env.root, command: ["pwsh"] });
	herdr.paneList.unshift(
		...herdr.paneList.splice(
			herdr.paneList.findIndex((p) => p.paneId === shell.paneId),
			1,
		),
	);
	expect((await ensureBossPane(herdr, { command, launched })).paneId).toBe(boss.paneId);

	// A record from another run lacks the boss pane, but omp runs in it.
	herdr.setState(boss.paneId, "idle");
	const fresh = new LaunchedPanes(join(env.root, "fresh.json"));
	expect((await ensureBossPane(herdr, { command, launched: fresh })).paneId).toBe(boss.paneId);

	// The boss pane is gone; only his shell is left among others: a new tab, his shell untouched.
	await herdr.createTab(boss.workspaceId, { title: "logs", cwd: env.root, command: ["pwsh"] });
	herdr.paneList.splice(
		herdr.paneList.findIndex((p) => p.paneId === boss.paneId),
		1,
	);
	const tabs = herdr.of("createTab").length;
	const made = await ensureBossPane(herdr, { command, launched: new LaunchedPanes(join(env.root, "none.json")) });
	expect(herdr.of("createTab")).toHaveLength(tabs + 1);
	expect(made.paneId).not.toBe(shell.paneId);
	expect(herdr.of("respawn")).toEqual([]);
});

test("panes.json keeps only well-formed entries: a broken one never throws on a refresh (CO-19)", async () => {
	const file = join(env.root, "broken-panes.json");
	writeFileSync(
		file,
		JSON.stringify({ "w1:p1": null, p2: { cwd: 3 }, p3: { cwd: env.root, terminal: 4 }, p4: { cwd: env.root } }),
	);
	const launched = new LaunchedPanes(file);
	expect(["w1:p1", "p2", "p3", "p4"].map((id) => launched.has(id))).toEqual([false, false, false, true]);
	expect(() =>
		launched.seen([{ paneId: "p4", tabId: "t", workspaceId: "w", agentState: "unknown", terminalId: "x" }]),
	).not.toThrow();
});

test("the boss's start says when omp's mcp.json in its folder has no jocasta server, naming jocasta setup (beta day P9)", () => {
	const home = join(env.root, "boss-home");
	const file = join(home, ".omp", "mcp.json");
	// No file at all: the sandbox before setup.
	expect(bossToolsMissing(home)).toContain(file);
	expect(bossToolsMissing(home)).toContain("`jocasta setup`");
	mkdirSync(join(home, ".omp"), { recursive: true });
	const write = (servers: Record<string, unknown>) => writeFileSync(file, JSON.stringify({ mcpServers: servers }));
	write({ whiteboard: { type: "stdio", command: "node" } });
	expect(bossToolsMissing(home)).toContain('no "jocasta" MCP server');
	write({ jocasta: { type: "stdio", command: "jocasta", args: ["mcp", "--boss"], enabled: false } });
	expect(bossToolsMissing(home)).toBeDefined();
	writeFileSync(file, "{ not json");
	expect(bossToolsMissing(home)).toBeDefined();
	write({ jocasta: { type: "stdio", command: "jocasta", args: ["mcp", "--boss"], timeout: 300000 } });
	expect(bossToolsMissing(home)).toBeUndefined();
});
