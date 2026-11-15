import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pushHookEnv } from "../../src/shared/push-hook.ts";
import { GIT_TEST_TIMEOUT, makeRepo, sh, useTempJocasta } from "./fixture.ts";

const env = useTempJocasta();
const ENTRY = join(import.meta.dir, "..", "..", "src", "entry.ts");

/** git's version as [major, minor]; [0, 0] when git can't run. */
function gitVersion(): [number, number] {
	const found = /(\d+)\.(\d+)/.exec(Bun.spawnSync(["git", "--version"]).stdout.toString());
	return [Number(found?.[1] ?? 0), Number(found?.[2] ?? 0)];
}
const [major, minor] = gitVersion();
const hookCapableGit = major > 2 || (major === 2 && minor >= 54);

/** `git push` in `dir` with `extra` added to the environment; its exit code and stderr. */
function push(dir: string, extra: Record<string, string>, ...args: string[]) {
	const run = Bun.spawnSync(["git", "-C", dir, "push", ...args], {
		env: { ...process.env, GIT_TERMINAL_PROMPT: "0", ...extra },
	});
	return { code: run.exitCode, stderr: run.stderr.toString() };
}

test.skipIf(!hookCapableGit)(
	"git runs a hook given through GIT_CONFIG_*: a push to main is refused, one to a feature branch passes, and git's arguments reach the program (0613)",
	() => {
		const origin = join(env.root, "origin.git");
		mkdirSync(origin);
		sh(origin, "init", "-q", "--bare");
		const work = makeRepo(join(env.root, "my work"));
		sh(work, "remote", "add", "origin", origin);
		// A stand-in for `jocasta push-hook`: records its arguments and refuses updates of main.
		const stub = join(env.root, "stub dir", "stub.ts");
		mkdirSync(join(env.root, "stub dir"));
		writeFileSync(
			stub,
			[
				'import { appendFileSync } from "node:fs";',
				`appendFileSync(${JSON.stringify(join(env.root, "args.log"))}, JSON.stringify(process.argv.slice(2)) + "\\n");`,
				"const refs = await Bun.stdin.text();",
				'if (refs.includes(" refs/heads/main ")) { console.error("stub refused main"); process.exit(1); }',
			].join("\n"),
		);
		const hook = pushHookEnv([process.execPath, stub], {});

		const refused = push(work, hook, "origin", "main");
		expect(refused.code).not.toBe(0);
		expect(refused.stderr).toContain("stub refused main");
		expect(() => sh(origin, "rev-parse", "--verify", "-q", "refs/heads/main")).toThrow();

		sh(work, "checkout", "-q", "-b", "feature/x");
		expect(push(work, hook, "origin", "feature/x").code).toBe(0);
		expect(sh(origin, "rev-parse", "refs/heads/feature/x")).toBe(sh(work, "rev-parse", "HEAD"));

		const logged = readFileSync(join(env.root, "args.log"), "utf8").trim().split("\n");
		expect(logged.map((l) => JSON.parse(l))).toEqual([
			["push-hook", "origin", origin],
			["push-hook", "origin", origin],
		]);
		// Without the environment nothing stands in the way: git's own behaviour is unchanged.
		expect(push(work, {}, "origin", "feature/x:main").code).toBe(0);
		expect(existsSync(origin)).toBe(true);
	},
	GIT_TEST_TIMEOUT,
);

/** The real `jocasta push-hook` with `address` and one updated ref, in a sandbox with no service running. */
function realHook(address: string, remoteRef: string) {
	const run = Bun.spawnSync([process.execPath, ENTRY, "push-hook", "origin", address], {
		stdin: new TextEncoder().encode(`refs/heads/work ${"1".repeat(40)} ${remoteRef} ${"0".repeat(40)}\n`),
		env: { ...process.env, JOCASTA_PANE_KEY: "key-1" },
	});
	return { code: run.exitCode, stderr: run.stderr.toString() };
}

test("jocasta push-hook passes a feature branch or a folder remote at once and refuses a protected branch with Jocasta not running (0613)", () => {
	expect(realHook("https://example.org/acme/app.git", "refs/heads/feature/x")).toEqual({ code: 0, stderr: "" });
	expect(realHook("/srv/git/app.git", "refs/heads/main")).toEqual({ code: 0, stderr: "" });
	const refused = realHook("https://example.org/acme/app.git", "refs/heads/main");
	expect(refused.code).toBe(1);
	expect(refused.stderr).toContain("Jocasta isn't running; start it with jocasta.");
}, 30_000);
