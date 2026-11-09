import { beforeEach, expect, test } from "bun:test";
import { readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { miseCheck, miseEnv } from "../../../src/frontier/server/mise.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

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
	try {
		return readFileSync(log(), "utf8")
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line));
	} catch {
		return [];
	}
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
