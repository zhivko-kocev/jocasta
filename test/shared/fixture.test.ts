import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { copyRepo, poll, readJsonl, sh, useOmpLog, useTempDirs, useTempJocasta, writeFiles } from "./fixture.ts";

describe("useTempDirs", () => {
	const tempDir = useTempDirs("jocasta-fixture-");
	const made: string[] = [];
	test("each call makes a new dir with the prefix, under the parent given", () => {
		const a = tempDir();
		const b = tempDir(a);
		made.push(a, b);
		expect(a).not.toBe(b);
		expect(basename(a)).toStartWith("jocasta-fixture-");
		expect(resolve(b, "..")).toBe(resolve(a));
		expect(existsSync(b)).toBe(true);
	});
	test("the dirs of the test before are gone", () => {
		expect(made.length).toBe(2);
		expect(made.filter(existsSync)).toEqual([]);
	});
});

describe("files", () => {
	const tempDir = useTempDirs("jocasta-fixture-");
	test("writeFiles makes the folders on the way; readJsonl reads each row and skips blank lines", () => {
		const root = tempDir();
		writeFiles(root, { "a/b/rows.jsonl": '{"n":1}\n\n{"n":2}\n', "c.txt": "c" });
		expect(readFileSync(join(root, "c.txt"), "utf8")).toBe("c");
		expect(readJsonl<{ n: number }>(join(root, "a", "b", "rows.jsonl"))).toEqual([{ n: 1 }, { n: 2 }]);
		writeFileSync(join(root, "torn.jsonl"), '{"n":1}\n{"n"');
		expect(() => readJsonl(join(root, "torn.jsonl"))).toThrow();
	});
});

describe("useOmpLog", () => {
	const saved = process.env.JOCASTA_TEST_OMP_LOG;
	const ompLog = useOmpLog<{ argv: string[] }>();
	test("points JOCASTA_TEST_OMP_LOG at a fresh file per call", () => {
		const { file } = ompLog();
		expect(process.env.JOCASTA_TEST_OMP_LOG).toBe(file);
		expect(ompLog().file).not.toBe(file);
	});
	test("the variable is restored after the test", () => {
		expect(process.env.JOCASTA_TEST_OMP_LOG).toBe(saved);
	});
	test("the reader returns what the harness appended", () => {
		const { file, calls } = ompLog();
		writeFileSync(file, `${JSON.stringify({ argv: ["a"] })}\n${JSON.stringify({ argv: ["b"] })}\n`);
		expect(calls().map((c) => c.argv)).toEqual([["a"], ["b"]]);
	});
});

describe("poll", () => {
	test("resolves true once the check holds, async checks too", async () => {
		let n = 0;
		expect(await poll(() => ++n >= 3, { intervalMs: 1 })).toBe(true);
		expect(n).toBe(3);
		expect(await poll(async () => true)).toBe(true);
	});
	test("resolves false at the deadline, or throws naming `what`", async () => {
		const started = Date.now();
		expect(await poll(() => false, { timeoutMs: 50, intervalMs: 10 })).toBe(false);
		expect(Date.now() - started).toBeGreaterThanOrEqual(45);
		await expect(poll(() => false, { timeoutMs: 20, intervalMs: 5, what: "the pane" })).rejects.toThrow(
			"timed out waiting for the pane",
		);
	});
});

describe("useTempJocasta", () => {
	const env = useTempJocasta();
	test("Jocasta's and omp's dirs all point into the test's temp dir", () => {
		expect(process.env.JOCASTA_CODING_AGENT_DIR).toBe(env.config);
		expect(process.env.JOCASTA_DATA_DIR).toBe(env.data);
		expect(process.env.PI_CODING_AGENT_DIR).toBe(join(env.root, "omp", "agent"));
		// omp joins PI_CONFIG_DIR to the home folder.
		expect(resolve(homedir(), process.env.PI_CONFIG_DIR as string)).toBe(join(env.root, "omp"));
	});

	test("copyRepo copies one template per name and options: clean, its own folder, the name's root commit", () => {
		const a = copyRepo(join(env.root, "one", "demo"));
		const b = copyRepo(join(env.root, "two", "demo"));
		const other = copyRepo(join(env.root, "three", "api"), { origin: "https://github.com/example-org/api.git" });
		expect(a).not.toBe(b);
		for (const repo of [a, b, other]) expect(sh(repo, "status", "--porcelain")).toBe("");
		const root = (repo: string) => sh(repo, "rev-list", "--max-parents=0", "HEAD");
		expect(root(a)).toBe(root(b));
		expect(root(other)).not.toBe(root(a));
		expect(sh(other, "remote", "get-url", "origin")).toBe("https://github.com/example-org/api.git");
		expect(readFileSync(join(a, "README.md"), "utf8")).toBe("demo\n");
		// A commit in one copy stays there.
		sh(a, "commit", "-q", "--allow-empty", "-m", "more");
		expect(sh(b, "rev-list", "--count", "HEAD")).toBe("1");
	});
});
