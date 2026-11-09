import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	bunFailures,
	type CheckRun,
	CheckRunner,
	gateReply,
	gatesOf,
	miseTasks,
	readCheckHits,
} from "../../../src/frontier/server/checks.ts";
import { makeRepo, useTempJocasta } from "../../shared/fixture.ts";

const env = useTempJocasta();

/** A runner with no tool versions in its key (mise isn't needed here). */
const runner = (dir: string) => new CheckRunner({ dir, versions: async () => "", pollMs: 20 });

/** A command line that runs Bun code, quoted for cmd.exe and sh alike. */
const bunCode = (code: string) => `"${process.execPath}" -e "${code}"`;

const MISE_LOG = [
	"[lint] $ echo lint ok",
	"[test] $ bun test",
	"[lint] lint ok",
	"[test] test\\auth.test.ts:",
	"[test] 4 |   expect(backoff(2)).toBe(5);",
	"[test]       at <anonymous> (C:\\work\\repo\\test\\auth.test.ts:4:22)",
	"[test] (fail) retries back off [0.24ms]",
	"[test] (pass) keeps going [0.01ms]",
	"[test]       at <anonymous> (C:\\work\\repo\\test\\auth.test.ts:8:22)",
	"[test] (fail) gives up after 5 [0.06ms]",
	"ERROR 1 task(s) failed:",
	"[test] exited with status 1",
	"[test] ERROR task failed",
].join("\n");

test("mise's log names its tasks and the failed ones; bun's test output names failing files and lines", () => {
	expect(miseTasks(MISE_LOG)).toEqual([
		{ name: "lint", failed: false },
		{ name: "test", failed: true },
	]);
	expect(bunFailures(MISE_LOG, "C:\\work\\repo")).toEqual([
		{ file: "test/auth.test.ts", line: 4, test: "retries back off" },
		{ file: "test/auth.test.ts", line: 8, test: "gives up after 5" },
	]);
});

test("the gate reply is 0546's form: one line per gate, failures, log and the narrowest rerun; silent on a pass", () => {
	const spec = { name: "check", command: "mise run -c check", mise: true };
	const log = "C:/data/checks/3f9a1c2b4d5e/check.log";
	const run: CheckRun = {
		tree: "3f9a1c2b4d5e6f708192a3b4c5d6e7f8091a2b3c",
		passed: false,
		results: [
			{ spec, exit: 1, log, ms: 1, how: "ran", gates: gatesOf(spec, 1, MISE_LOG, log, "C:\\work\\repo") },
			{
				spec: { name: "done_when", command: "bun run e2e:login" },
				exit: 0,
				log: "x",
				ms: 1,
				how: "hit",
				gates: gatesOf({ name: "done_when", command: "bun run e2e:login" }, 0, "", "x", "C:\\work\\repo"),
			},
		],
	};
	expect(gateReply(run)).toBe(
		[
			"checks: FAIL (tree 3f9a1c)",
			"  PASS  lint       mise run lint",
			"  FAIL  test       mise run test",
			'        2 failed: test/auth.test.ts:4 "retries back off", test/auth.test.ts:8 "gives up after 5"',
			`        log: ${join(log, "..", "test.log")}`,
			"        rerun: bun test test/auth.test.ts",
			"  PASS  done_when  bun run e2e:login",
		].join("\n"),
	);
	// A failure the runner names no files for reruns the check itself.
	const plain = gatesOf({ name: "done_when", command: "make e2e" }, 2, "boom", "l", "C:\\work\\repo");
	expect(plain).toEqual([
		{ name: "done_when", command: "make e2e", pass: false, failures: [], log: "l", rerun: "make e2e" },
	]);
	expect(gateReply({ ...run, passed: true })).toBe("");
});

test("one run per tree for two concurrent callers; a FAIL is stored and replays; a stale lock is taken over", async () => {
	const repo = makeRepo(join(env.root, "repo"));
	const dir = join(env.data, "checks");
	const slow = { name: "test", command: bunCode("await Bun.sleep(400); console.log('ran'); process.exit(3)") };
	const a = runner(dir);
	const b = runner(dir);
	const [first, second] = await Promise.all([a.run(repo, [slow], process.env), b.run(repo, [slow], process.env)]);
	expect([first?.results[0]?.how, second?.results[0]?.how].sort()).toEqual(["joined", "ran"]);
	expect(a.runs + b.runs).toBe(1);
	expect(first?.passed).toBe(false);
	expect(first?.results[0]?.exit).toBe(3);
	expect(readFileSync(first?.results[0]?.log as string, "utf8")).toContain("ran");

	// The same tree again: the stored FAIL answers, nothing runs.
	const c = runner(dir);
	const again = await c.run(repo, [slow], process.env);
	expect(again?.results[0]?.how).toBe("hit");
	expect(again?.results[0]?.exit).toBe(3);
	expect(c.runs).toBe(0);
	expect(readCheckHits(dir)).toBe(2);

	// An edit is a new tree; a lock left by a process that is gone is taken over.
	writeFileSync(join(repo, "new.txt"), "x\n");
	const quick = { name: "test", command: bunCode("console.log('ok')") };
	const tree = await c.treeKey(repo);
	mkdirSync(join(dir, (tree as string).slice(0, 12)), { recursive: true });
	writeFileSync(join(dir, (tree as string).slice(0, 12), "test.lock"), "999999");
	const fresh = await c.run(repo, [quick], process.env);
	expect(fresh?.results[0]?.how).toBe("ran");
	expect(fresh?.passed).toBe(true);
	expect(existsSync(join(dir, (tree as string).slice(0, 12), "test.lock"))).toBe(false);
	// The real index was never touched: the new file is still untracked.
	expect(Bun.spawnSync(["git", "-C", repo, "status", "--porcelain"]).stdout.toString()).toContain("?? new.txt");
});

test("outside git there is no key and nothing runs", async () => {
	const plain = join(env.root, "plain");
	mkdirSync(plain, { recursive: true });
	expect(
		await runner(join(env.data, "checks")).run(plain, [{ name: "x", command: "echo" }], process.env),
	).toBeUndefined();
});
