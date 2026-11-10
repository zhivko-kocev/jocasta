import { beforeEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { miseCheck, miseEnv, trustCopies } from "../../../src/frontier/server/mise.ts";
import { readJsonl, useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

const MISE = [process.execPath, join(import.meta.dir, "fake-mise.ts")];
const PARANOID = '["config"]\n';
const TRUSTED = "~/src/demo/mise.toml: trusted\n";
const log = () => join(env.root, "mise.log");
beforeEach(() => rmSync(log(), { force: true }));

/** The env the fake mise answers from, with the pane's own env. */
function base(answers: Record<string, string>, extra: Record<string, string> = {}): Record<string, string | undefined> {
	return { ...process.env, FAKE_MISE_LOG: log(), ...answers, ...extra };
}

function calls(): { args: string; miseEnv?: string }[] {
	return existsSync(log()) ? readJsonl(log()) : [];
}

test("without mise on PATH a pane's env and the review's check are unchanged (0473, 0477)", async () => {
	expect(await miseEnv("demo-1", env.root, base({}), null)).toEqual({});
	expect(await miseCheck("demo-1", env.root, base({}), null)).toBeUndefined();
});

test("a trusted folder under paranoid trust: MISE_ENV gets jocasta-<pid> appended and mise's env is merged (0473, 0477)", async () => {
	const answers = {
		FAKE_MISE_PARANOID: PARANOID,
		FAKE_MISE_TRUST: TRUSTED,
		FAKE_MISE_ENV_JSON: JSON.stringify({ PATH: "C:\\mise\\bun\\bin;C:\\Windows", API_PORT: "3010", MISE_ENV: "x" }),
	};
	const result = await miseEnv("demo-1", env.root, base(answers, { MISE_ENV: "ci, jocasta-demo-1" }), MISE);
	expect(result).toEqual({
		PATH: "C:\\mise\\bun\\bin;C:\\Windows",
		API_PORT: "3010",
		MISE_ENV: "ci,jocasta-demo-1",
	});
	// mise itself ran with the project's environment selected, so Jocasta's own config for it loaded.
	expect(calls().every((c) => c.miseEnv === "ci,jocasta-demo-1")).toBe(true);
});

test("an untrusted repo config or paranoid trust off: only MISE_ENV, and mise's env is never read (0473)", async () => {
	const untrusted = base({ FAKE_MISE_PARANOID: PARANOID, FAKE_MISE_TRUST: "~/src/demo/mise.toml: untrusted\n" });
	expect(await miseEnv("demo-1", env.root, untrusted, MISE)).toEqual({ MISE_ENV: "jocasta-demo-1" });
	const relaxed = base({ FAKE_MISE_PARANOID: "false\n", FAKE_MISE_TRUST: TRUSTED });
	expect(await miseEnv("demo-1", env.root, relaxed, MISE)).toEqual({ MISE_ENV: "jocasta-demo-1" });
	expect(calls().some((c) => c.args === "env --json")).toBe(false);
	expect(calls().some((c) => c.args.startsWith("trust") && c.args !== "trust --show")).toBe(false);
});

test("the review's mise check is `verify`, else `test`, only for a folder mise may load (0453, 0473)", async () => {
	const answers = { FAKE_MISE_PARANOID: PARANOID, FAKE_MISE_TRUST: "" };
	const tasks = (names: string[]) => JSON.stringify(names.map((name) => ({ name, source: "x" })));
	expect(
		await miseCheck("demo-1", env.root, base({ ...answers, FAKE_MISE_TASKS: tasks(["test", "verify"]) }), MISE),
	).toEqual({ task: "verify" });
	expect(
		await miseCheck("demo-1", env.root, base({ ...answers, FAKE_MISE_TASKS: tasks(["lint", "test"]) }), MISE),
	).toEqual({ task: "test" });
	expect(
		await miseCheck("demo-1", env.root, base({ ...answers, FAKE_MISE_TASKS: tasks(["lint"]) }), MISE),
	).toBeUndefined();
});

test("a repo config mise may not load says why the project's check didn't run: untrusted, or paranoid trust off; no repo config says nothing (0473, 0559)", async () => {
	const tasks = JSON.stringify([{ name: "verify", source: "x" }]);
	const untrusted = {
		FAKE_MISE_PARANOID: PARANOID,
		FAKE_MISE_TRUST: "~/x/mise.toml: untrusted\n",
		FAKE_MISE_TASKS: tasks,
	};
	expect(await miseCheck("demo-1", env.root, base(untrusted), MISE)).toEqual({
		skipped: "~/x/mise.toml isn't trusted in mise",
	});
	const relaxed = {
		FAKE_MISE_PARANOID: "false\n",
		FAKE_MISE_TRUST: "C:\\src\\demo: trusted\r\n",
		FAKE_MISE_TASKS: tasks,
	};
	expect(await miseCheck("demo-1", env.root, base(relaxed), MISE)).toEqual({
		skipped: "mise's paranoid config trust is off",
	});
	expect(await miseCheck("demo-1", env.root, base({ ...relaxed, FAKE_MISE_TRUST: "" }), MISE)).toBeUndefined();
	// mise's tasks are never listed for a config it may not load.
	expect(calls().some((c) => c.args === "tasks ls --json")).toBe(false);
});

/** A main checkout and a thread worktree, each with a root `mise.toml` and a `sub/mise.toml`, and mise's answers. */
function checkouts(worktreeText: string) {
	const main = join(env.root, "demo");
	const wt = join(env.root, "demo.wt", "fix-x");
	for (const dir of [main, wt]) mkdirSync(join(dir, "sub"), { recursive: true });
	writeFileSync(join(main, "mise.toml"), '[tools]\nbun = "1.4.3"\n');
	writeFileSync(join(main, "sub", "mise.toml"), "[tasks.x]\nrun = 'x'\n");
	writeFileSync(join(wt, "mise.toml"), worktreeText);
	writeFileSync(join(wt, "sub", "mise.toml"), "[tasks.x]\nrun = 'x'\n");
	const show = (dir: string, mainSub: "trusted" | "untrusted", status: string) =>
		`${join(dir, "mise.toml")}: ${status}\n${join(dir, "sub", "mise.toml")}: ${dir === main ? mainSub : "untrusted"}\n`;
	const answers = (mainSub: "trusted" | "untrusted", paranoid = PARANOID) => ({
		FAKE_MISE_PARANOID: paranoid,
		FAKE_MISE_TRUST_IN: JSON.stringify({
			[main]: show(main, mainSub, "trusted"),
			[wt]: show(wt, mainSub, "untrusted"),
		}),
	});
	return { main, wt, answers };
}

const trusts = () => calls().filter((c) => c.args.startsWith("trust ") && c.args !== "trust --show");

test("a worktree's byte-for-byte copy of a config the main checkout trusts is trusted, under paranoid trust only (0596)", async () => {
	const { main, wt, answers } = checkouts('[tools]\nbun = "1.4.3"\n');
	expect(await trustCopies("demo-1", main, wt, base(answers("trusted")), MISE)).toEqual([
		join(wt, "mise.toml"),
		join(wt, "sub", "mise.toml"),
	]);
	expect(trusts().map((c) => c.args)).toEqual([
		`trust ${join(wt, "mise.toml")}`,
		`trust ${join(wt, "sub", "mise.toml")}`,
	]);
	expect(trusts().every((c) => c.miseEnv === "jocasta-demo-1")).toBe(true);
	rmSync(log(), { force: true });
	expect(await trustCopies("demo-1", main, wt, base(answers("trusted", "false\n")), MISE)).toEqual([]);
	expect(trusts()).toEqual([]);
	expect(await trustCopies("demo-1", main, wt, base({}), null)).toEqual([]);
});

test("a changed copy, or a copy of a config the main checkout doesn't trust, stays untrusted (0596)", async () => {
	const { main, wt, answers } = checkouts('[tools]\nbun = "1.4.3"\n[env]\nX = "1"\n');
	expect(await trustCopies("demo-1", main, wt, base(answers("untrusted")), MISE)).toEqual([]);
	expect(trusts()).toEqual([]);
});

test("configs mise shows under its home folder as `~/…` (Linux and macOS) are found too (0596)", async () => {
	const { main, wt } = checkouts('[tools]\nbun = "1.4.3"\n');
	const shown = (dir: string, status: string) => `~${join(dir, "mise.toml").slice(env.root.length)}: ${status}\n`;
	const answers = {
		FAKE_MISE_PARANOID: PARANOID,
		FAKE_MISE_TRUST_IN: JSON.stringify({ [main]: shown(main, "trusted"), [wt]: shown(wt, "untrusted") }),
	};
	const trusted = await trustCopies("demo-1", main, wt, base(answers, { HOME: env.root }), MISE);
	expect(trusted).toEqual([join(wt, "mise.toml")]);
});
