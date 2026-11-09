import { expect, test } from "bun:test";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	bunFailures,
	type CheckRun,
	CheckRunner,
	gateReply,
	gatesOf,
	miseTasks,
	readCheckHits,
	sizeSaid,
} from "../../../src/frontier/server/checks.ts";
import { processStart } from "../../../src/shared/proctree.ts";
import { copyRepo, GIT_TEST_TIMEOUT, useTempJocasta } from "../../shared/fixture.ts";

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

// Waits on real processes: git for each tree key, a copied repo and two Bun checks through the shell; seconds under load.
test(
	"one run per tree for two concurrent callers; a FAIL is stored and replays; a stale lock is taken over",
	async () => {
		const repo = copyRepo(join(env.root, "repo"));
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
		expect(
			Bun.spawnSync(["git", "-C", repo, "status", "--porcelain"], { windowsHide: true }).stdout.toString(),
		).toContain("?? new.txt");
	},
	GIT_TEST_TIMEOUT,
);

test("outside git there is no key and nothing runs", async () => {
	const plain = join(env.root, "plain");
	mkdirSync(plain, { recursive: true });
	expect(
		await runner(join(env.data, "checks")).run(plain, [{ name: "x", command: "echo" }], process.env),
	).toBeUndefined();
});

test("a lock whose pid now names another process (a reused pid) is taken over, not waited on", async () => {
	const repo = copyRepo(join(env.root, "repo"));
	const dir = join(env.data, "checks");
	const r = runner(dir);
	const folder = join(dir, ((await r.treeKey(repo)) as string).slice(0, 12));
	mkdirSync(folder, { recursive: true });
	// This test's own pid is alive, but it started at another time than the lock says.
	writeFileSync(join(folder, "test.lock"), `${process.pid} 1`);
	const run = await r.run(repo, [{ name: "test", command: bunCode("console.log('ok')") }], process.env);
	expect(run?.results[0]?.how).toBe("ran");
	expect(existsSync(join(folder, "test.lock"))).toBe(false);
	expect(readdirSync(folder).filter((name) => name.endsWith(".stale"))).toEqual([]);
});

test("a run waiting on a live holder's lock ends when the runner stops", async () => {
	const repo = copyRepo(join(env.root, "repo"));
	const dir = join(env.data, "checks");
	const r = runner(dir);
	const tree = (await r.treeKey(repo)) as string;
	const folder = join(dir, tree.slice(0, 12));
	mkdirSync(folder, { recursive: true });
	// This process holds it, as itself: a live holder the run waits on.
	writeFileSync(join(folder, "test.lock"), `${process.pid} ${processStart(process.pid) ?? ""}`.trim());
	const run = r.run(repo, [{ name: "test", command: bunCode("console.log('ok')") }], process.env, tree);
	// Lets the run reach its wait (the runner exposes no signal for it); a stop before that ends it the same way.
	await Bun.sleep(100);
	await r.stop();
	await expect(run).rejects.toThrow("the check runner has stopped");
	expect(r.runs).toBe(0);
});

test("a run's checks run one after another", async () => {
	const repo = copyRepo(join(env.root, "repo"));
	const trace = join(env.root, "trace.txt").replaceAll("\\", "/");
	const step = (name: string) => ({
		name,
		command: bunCode(
			`const f = require('node:fs'); f.appendFileSync('${trace}', 'start ${name}\\n'); await Bun.sleep(200); f.appendFileSync('${trace}', 'end ${name}\\n')`,
		),
	});
	const run = await runner(join(env.data, "checks")).run(repo, [step("one"), step("two")], process.env);
	expect(run?.passed).toBe(true);
	expect(readFileSync(trace, "utf8").split("\n").filter(Boolean)).toEqual([
		"start one",
		"end one",
		"start two",
		"end two",
	]);
});

test("a stored result keeps its gates (a hit needs no log); reruns quote paths with spaces; task names make safe file names", async () => {
	const repo = copyRepo(join(env.root, "repo"));
	const dir = join(env.data, "checks");
	const failing = {
		name: "done_when",
		command: bunCode("console.log('test/a b.test.ts:'); console.log('(fail) adds'); process.exit(1)"),
	};
	const first = await runner(dir).run(repo, [failing], process.env);
	const tree = first?.tree as string;
	const gate = first?.results[0]?.gates[0];
	expect(gate?.rerun).toBe('bun test "test/a b.test.ts"');
	rmSync(first?.results[0]?.log as string);
	const again = await runner(dir).run(repo, [failing], process.env);
	expect(again?.results[0]?.how).toBe("hit");
	expect(again?.results[0]?.gates).toEqual(first?.results[0]?.gates as never);

	// A mise task named `ci/lint` writes `ci-lint.log`, and its result is stored.
	const mise = {
		name: "check",
		command: bunCode("console.log('[ci/lint] boom'); console.log('[ci/lint] ERROR task failed'); process.exit(1)"),
		mise: true,
	};
	const run = await runner(dir).run(repo, [mise], process.env, tree);
	const folder = join(dir, tree.slice(0, 12));
	expect(run?.results[0]?.gates.map((g) => [g.name, g.pass, g.log])).toEqual([
		["ci/lint", false, join(folder, "ci-lint.log")],
	]);
	expect(readFileSync(join(folder, "ci-lint.log"), "utf8")).toContain("boom");
	expect(existsSync(join(folder, "check.json"))).toBe(true);
});

test("stored results untouched for 14 days are removed at the day's first start, with one log line; hits.log and fresh ones stay", async () => {
	const dir = join(env.data, "checks");
	const old = new Date(Date.now() - 15 * 24 * 60 * 60_000);
	const stale = join(dir, "aaaaaaaaaaaa");
	mkdirSync(stale, { recursive: true });
	writeFileSync(join(stale, "test.log"), "x".repeat(2048));
	writeFileSync(join(stale, "test.json"), "");
	for (const file of ["test.log", "test.json"]) utimesSync(join(stale, file), old, old);
	utimesSync(stale, old, old);
	const fresh = join(dir, "bbbbbbbbbbbb");
	mkdirSync(fresh, { recursive: true });
	writeFileSync(join(fresh, "test.log"), "x");
	writeFileSync(join(dir, "hits.log"), "1\n");
	utimesSync(join(dir, "hits.log"), old, old);
	mkdirSync(join(dir, "tmp"), { recursive: true });
	writeFileSync(join(dir, "tmp", "index-old"), "");
	utimesSync(
		join(dir, "tmp", "index-old"),
		new Date(Date.now() - 2 * 60 * 60_000),
		new Date(Date.now() - 2 * 60 * 60_000),
	);
	writeFileSync(join(dir, "tmp", "index-new"), "");

	const r = runner(dir);
	await r.pruning;
	await r.stop();
	expect(existsSync(stale)).toBe(false);
	expect(existsSync(fresh)).toBe(true);
	expect(readCheckHits(dir)).toBe(1);
	expect(readdirSync(join(dir, "tmp"))).toEqual(["index-new"]);
	const logs = join(env.data, "logs");
	const said = readdirSync(logs)
		.map((name) => readFileSync(join(logs, name), "utf8"))
		.join("");
	expect(said).toContain("[checks] removed 1 stored results older than 14 days (2.0 KB)");

	// Another start the same day prunes nothing more: a result gone stale since stays, and nothing is said.
	const later = join(dir, "cccccccccccc");
	mkdirSync(later, { recursive: true });
	writeFileSync(join(later, "test.log"), "x");
	utimesSync(join(later, "test.log"), old, old);
	utimesSync(later, old, old);
	const sameDay = runner(dir);
	await sameDay.pruning;
	await sameDay.stop();
	expect(existsSync(later)).toBe(true);
	const again = readdirSync(logs)
		.map((name) => readFileSync(join(logs, name), "utf8"))
		.join("");
	expect(again.split("removed").length).toBe(2);

	// The next day's first start prunes again.
	writeFileSync(join(dir, "pruned-on"), "2000-01-01\n");
	const nextDay = runner(dir);
	await nextDay.pruning;
	await nextDay.stop();
	expect(existsSync(later)).toBe(false);
});

test("sizes are said in B, KB, MB and GB: whole numbers from 10 up, one decimal below", () => {
	expect(sizeSaid(512)).toBe("512 B");
	expect(sizeSaid(4.2 * 1024)).toBe("4.2 KB");
	expect(sizeSaid(380 * 1024 * 1024)).toBe("380 MB");
	expect(sizeSaid(3 * 1024 ** 3)).toBe("3.0 GB");
});
