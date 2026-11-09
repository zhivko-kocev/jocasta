import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assistantText, readClaudeResponse, readResponse } from "../../evals/harness.ts";
import {
	type Check,
	countFindings,
	gradeCheck,
	loadCases,
	MEASUREMENT_NOTES,
	makeTrialRepo,
	runEvals,
	trialArgs,
} from "../../evals/runner.ts";
import { DEFAULT_VARIANT, type Variant } from "../../evals/variants.ts";
import { removeTree } from "../../src/shared/fsx.ts";
import { readJsonl, sh, useOmpLog, useTempDirs, writeFiles } from "../shared/fixture.ts";

const omp = [process.execPath, join(import.meta.dir, "fixtures", "omp.ts")];
const runScript = join(import.meta.dir, "..", "..", "evals", "run.ts");
const tempDir = useTempDirs("jocasta-evals-");

interface OmpCall {
	argv: string[];
	overlay: Record<string, unknown>;
	cwd: string;
	branch: string;
}

const ompLog = useOmpLog<OmpCall>();

describe("runEvals on omp", () => {
	test("each trial runs omp in a fresh fixture repo and writes results, transcripts and the report", async () => {
		const cases = tempDir();
		writeFiles(cases, {
			"greet/case.yaml":
				"prompt: 'Write greet.txt: hello acme'\ntags: [write]\nchecks:\n  - file: greet.txt\n    contains: hello acme\n  - file: seed.txt\n",
			"greet/fixture/seed.txt": "seed\n",
			"wrong/case.yaml":
				"prompt: 'Write wrong.txt: something else'\nchecks:\n  - file: wrong.txt\n    contains: expected text\n",
		});
		const calls = ompLog().calls;
		const out = join(tempDir(), "run");
		const rows = await runEvals({ casesDir: cases, outDir: out, reps: 2, launcher: omp });

		const written = readJsonl(join(out, "results.jsonl"));
		expect(written).toEqual(JSON.parse(JSON.stringify(rows)));
		expect(written.map((row) => `${row.case}-${row.rep}`)).toEqual(["greet-0", "greet-1", "wrong-0", "wrong-1"]);
		for (const row of written) {
			expect(Object.keys(row).sort()).toEqual([
				"case",
				"findings",
				"grade",
				"harness",
				"harnessVersion",
				"hiddenPassed",
				"linesAdded",
				"linesRemoved",
				"model",
				"passed",
				"prompts",
				"rep",
				"requests",
				"startedAt",
				"toolCalls",
				"totalUsage",
				"turns",
				"usage",
				"variant",
				"wallMs",
			]);
			expect([row.harness, row.harnessVersion, row.prompts, row.hiddenPassed, row.findings]).toEqual([
				"omp",
				"omp/0.0.0-stub",
				1,
				null,
				null,
			]);
			expect(row.model).toBe("stub-model");
			expect(row.variant).toBe("default");
			expect([row.turns, row.toolCalls, row.requests]).toEqual([2, 1, 4]);
			expect(row.wallMs).toBeGreaterThan(0);
			// The written file is one new line; the overlay lives outside the repo.
			expect([row.linesAdded, row.linesRemoved]).toEqual([1, 0]);
			expect(existsSync(join(out, "transcripts", `${row.case}-${row.rep}.jsonl`))).toBe(true);
		}
		const [greet, , wrong] = written;
		expect(greet.passed).toBe(true);
		expect(greet.grade).toEqual({
			"omp exited 0": 1,
			'file: greet.txt contains "hello acme"': 1,
			"file: seed.txt": 1,
		});
		expect(greet.usage).toEqual({ input: 1100, output: 20, cacheRead: 800, cacheWrite: 0, totalTokens: 1920, cost: 1 });
		expect(greet.totalUsage).toEqual({
			input: 1200,
			output: 30,
			cacheRead: 1200,
			cacheWrite: 0,
			totalTokens: 2430,
			cost: 1.25,
		});
		expect(wrong.passed).toBe(false);
		expect(wrong.grade['file: wrong.txt contains "expected text"']).toBe(0);

		// omp ran in the trial repo on its own branch, headless, with the memory-off overlay and nothing else.
		const [first] = calls();
		expect(first?.argv.slice(0, 2)).toEqual(["--config", expect.stringMatching(/eval-run\.yml$/)]);
		expect(first?.argv.slice(2)).toEqual(["-p", "Write greet.txt: hello acme", "--mode", "json"]);
		expect(first?.overlay).toEqual({ memory: { backend: "off" } });
		expect(first?.cwd.endsWith("repo")).toBe(true);
		// No branch names its case: a benchmark task's id ends in its fix's PR number, and git output shows the branch (0508).
		for (const call of calls()) {
			expect(call.branch).toMatch(/^eval\//);
			expect(call.branch).not.toMatch(/greet|wrong/);
		}

		const report = readFileSync(join(out, "report.html"), "utf8");
		expect(report).toContain(
			"omp/0.0.0-stub, variant default: 2 cases × 2 reps. Passed 2 of 4 trials: 50.0% ± 50.0 points.",
		);
		expect(report).toContain('href="transcripts/wrong-1.jsonl"');
		expect(report).toContain("<th>Total tokens</th><th>Requests</th><th>Turns</th><th>Tool calls</th>");
		for (const note of MEASUREMENT_NOTES) expect(report).toContain(note);
	}, 60_000);

	test("with a log folder, lines.log holds the per-file counts the row sums (0508)", async () => {
		const cases = tempDir();
		writeFiles(cases, {
			"greet/case.yaml": "prompt: 'Write greet.txt: hello acme'\nchecks:\n  - file: greet.txt\n",
			"greet/fixture/seed.txt": "seed\n",
		});
		const logs = join(tempDir(), "logs");
		const [row] = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: omp,
			logDir: logs,
		});
		expect(readFileSync(join(logs, "lines.log"), "utf8").trim()).toBe("1\t0\tgreet.txt");
		expect([row?.linesAdded, row?.linesRemoved]).toEqual([1, 0]);
	}, 60_000);

	test("a variant's flags go to omp and its settings layer over the memory-off overlay", async () => {
		const cases = tempDir();
		writeFiles(cases, {
			"branch/case.yaml":
				"prompt: Say done\nsetup: echo made > made.txt\nchecks:\n  - file: made.txt\n    contains: made\n  - git_clean: true\n",
		});
		const calls = ompLog().calls;
		const prompt = join(tempDir(), "acme.md");
		writeFileSync(prompt, "Be brief.\n");
		const rows = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: omp,
			variant: {
				...DEFAULT_VARIANT,
				label: "lean+acme",
				appendSystemPrompt: prompt,
				extensions: ["/ext/a.ts"],
				tools: ["read", "bash"],
				settings: { memory: { backend: "local", extra: 1 }, modelRoles: { default: "acme/model-a" } },
			},
		});
		expect(rows.map((row) => [row.variant, row.passed, row.linesAdded, row.linesRemoved])).toEqual([
			["lean+acme", true, 0, 0],
		]);
		const [call] = calls();
		expect(call?.argv).toEqual([
			"--append-system-prompt",
			// The trial's copy, its leading comment stripped (0577).
			expect.stringMatching(/system-prompt\.md$/),
			"-e",
			"/ext/a.ts",
			"--tools",
			"read,bash",
			"--config",
			expect.stringMatching(/eval-run\.yml$/),
			"-p",
			"Say done",
			"--mode",
			"json",
		]);
		// The variant wins where both set a key.
		expect(call?.overlay).toEqual({
			memory: { backend: "local", extra: 1 },
			modelRoles: { default: "acme/model-a" },
		});
	}, 60_000);

	test("a failing omp fails the trial", async () => {
		const cases = tempDir();
		writeFiles(cases, { "fail/case.yaml": "prompt: Fail now\nchecks:\n  - git_clean: true\n" });
		const rows = await runEvals({ casesDir: cases, outDir: join(tempDir(), "run"), reps: 1, launcher: omp });
		expect(rows[0]?.grade).toEqual({ "omp exited 0": 0, "git_clean: true": 1 });
		expect(rows[0]?.passed).toBe(false);
	}, 60_000);

	test("a trial past its limit is killed with its process tree and graded failed with a timeout", async () => {
		const cases = tempDir();
		writeFiles(cases, { "stuck/case.yaml": "prompt: Never finish\nchecks:\n  - git_clean: true\n" });
		const pidFile = join(tempDir(), "sleeper.pid");
		process.env.JOCASTA_TEST_SLEEPER_PID_FILE = pidFile;
		const started = Date.now();
		try {
			const rows = await runEvals({
				casesDir: cases,
				outDir: join(tempDir(), "run"),
				reps: 1,
				timeoutMs: 1_500,
				launcher: [process.execPath, join(import.meta.dir, "fixtures", "sleeper.ts")],
			});
			expect(Date.now() - started).toBeLessThan(30_000);
			expect(rows).toHaveLength(1);
			expect(rows[0]?.passed).toBe(false);
			expect(rows[0]?.harnessVersion).toBe("omp/0.0.0-sleeper");
			expect(rows[0]?.grade).toEqual({ "omp exited 0": 0, timeout: 0, "git_clean: true": 1 });
			// The grandchild omp started is gone too.
			const grandchild = Number(readFileSync(pidFile, "utf8"));
			expect(() => process.kill(grandchild, 0)).toThrow();
		} finally {
			delete process.env.JOCASTA_TEST_SLEEPER_PID_FILE;
		}
	}, 60_000);

	test("a launcher whose --version fails stops the run before any trial", async () => {
		const cases = tempDir();
		writeFiles(cases, { "c/case.yaml": "prompt: p\nchecks:\n  - git_clean: true\n" });
		const launcher = [process.execPath, join(import.meta.dir, "fixtures", "no-such-omp.ts")];
		await expect(runEvals({ casesDir: cases, outDir: tempDir(), reps: 1, launcher })).rejects.toThrow(
			"--version failed",
		);
	});

	test("hidden checks run on a clean clone of the result with hidden/ copied in, never in the agent's checkout", async () => {
		const cases = tempDir();
		const grader = (expected: string) =>
			`const text = await Bun.file("out.txt").text().catch(() => "");\nprocess.exit(text.trim() === ${JSON.stringify(expected)} ? 0 : 1);\n`;
		const command = JSON.stringify(`"${process.execPath}" grade.ts`);
		writeFiles(cases, {
			"good/case.yaml": `prompt: 'Write out.txt: hello'\nchecks:\n  - file: grade.ts\n    absent: true\nhidden_checks:\n  - command: ${command}\n`,
			"good/hidden/grade.ts": grader("hello"),
			"bad/case.yaml": `prompt: 'Write out.txt: hi'\nchecks:\n  - file: out.txt\nhidden_checks:\n  - command: ${command}\n`,
			"bad/hidden/grade.ts": grader("hello"),
		});
		const rows = await runEvals({ casesDir: cases, outDir: join(tempDir(), "run"), reps: 1, launcher: omp });
		const label = `hidden: command: "${process.execPath}" grade.ts`;
		const [bad, good] = rows;
		expect(good?.grade).toEqual({ "omp exited 0": 1, "file: grade.ts absent": 1, [label]: 1 });
		expect([good?.hiddenPassed, good?.passed]).toEqual([true, true]);
		expect(bad?.grade[label]).toBe(0);
		expect([bad?.hiddenPassed, bad?.passed]).toEqual([false, false]);
		// The grader isn't part of the agent's change.
		expect([good?.linesAdded, good?.linesRemoved]).toEqual([1, 0]);
	}, 60_000);

	test("a hidden/ folder without hidden_checks is refused", () => {
		const cases = tempDir();
		writeFiles(cases, { "c/case.yaml": "prompt: p\nchecks:\n  - git_clean: true\n", "c/hidden/grade.ts": "" });
		expect(() => loadCases(cases)).toThrow("hidden/ folder but no hidden_checks");
	});

	test("plan first, follow-up turns with -c, the profile and the pacer (0482)", async () => {
		const cases = tempDir();
		writeFiles(cases, {
			"sum/case.yaml": "prompt: 'Write sum.txt: 3'\nchecks:\n  - file: sum.txt\n    contains: '3'\n",
		});
		const calls = ompLog().calls;
		let paced = 0;
		const variant: Variant = {
			...DEFAULT_VARIANT,
			label: "a3",
			model: "acme/model-a",
			planFirst: true,
			followups: ["Review git diff {base}"],
		};
		const rows = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 2,
			launcher: omp,
			variant,
			profile: "jocasta-evals",
			pace: async () => {
				paced++;
			},
		});
		expect(paced).toBe(2);
		expect(rows.map((row) => [row.passed, row.prompts, row.turns, row.findings])).toEqual([
			[true, 2, 4, { total: 2, blocking: 1 }],
			[true, 2, 4, { total: 2, blocking: 1 }],
		]);
		const [first, second] = calls();
		expect(first?.argv).toEqual([
			"--profile",
			"jocasta-evals",
			"--model",
			"acme/model-a",
			"--plan-yolo",
			"--plan-yolo-into",
			"acme/model-a",
			"--config",
			expect.stringMatching(/eval-run\.yml$/),
			"-p",
			"Write sum.txt: 3",
			"--mode",
			"json",
		]);
		// The follow-up continues the session, without plan-first, with the fixture commit for {base}.
		expect(second?.argv.slice(0, 5)).toEqual(["--profile", "jocasta-evals", "-c", "--model", "acme/model-a"]);
		expect(second?.argv.at(-3)).toMatch(/^Review git diff [0-9a-f]{40}$/);
		expect(second?.cwd).toBe(first?.cwd ?? "");
	}, 60_000);

	test("a Claude Code variant runs claude -p with stream-json and reads its result events (0482 E4)", async () => {
		const cases = tempDir();
		writeFiles(cases, { "c/case.yaml": "prompt: 'Write a.txt: hi'\nchecks:\n  - file: a.txt\n    contains: hi\n" });
		const calls = ompLog().calls;
		const rows = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: [process.execPath, join(import.meta.dir, "fixtures", "claude.ts")],
			variant: {
				...DEFAULT_VARIANT,
				label: "a4",
				harness: "claude",
				model: "claude-stub",
				args: ["--permission-mode", "bypassPermissions"],
			},
			profile: "ignored-for-claude",
		});
		expect(calls()[0]?.argv).toEqual([
			"--model",
			"claude-stub",
			"--permission-mode",
			"bypassPermissions",
			"-p",
			"Write a.txt: hi",
			"--output-format",
			"stream-json",
			"--verbose",
		]);
		const [row] = rows;
		expect([row?.harness, row?.harnessVersion, row?.model, row?.turns, row?.toolCalls]).toEqual([
			"claude",
			"2.1.296 (Claude Code)",
			"claude-stub",
			2,
			1,
		]);
		expect(row?.grade).toEqual({ "claude exited 0": 1, 'file: a.txt contains "hi"': 1 });
		expect(row?.totalUsage).toEqual({
			input: 10,
			output: 20,
			cacheRead: 300,
			cacheWrite: 40,
			totalTokens: 370,
			cost: 0.5,
		});
	}, 60_000);
});

describe("bun run evals", () => {
	const run = (env: Record<string, string | undefined>) => {
		const merged: Record<string, string | undefined> = { ...process.env, ...env };
		for (const [key, value] of Object.entries(env)) if (value === undefined) delete merged[key];
		return spawnSync(process.execPath, [runScript], { env: merged, encoding: "utf8" });
	};

	test("needs JOCASTA_EVALS_REAL, refuses CI, and needs omp on PATH", () => {
		const noReal = run({ JOCASTA_EVALS_REAL: undefined, CI: undefined });
		expect(noReal.status).toBe(2);
		expect(noReal.stderr).toContain("set JOCASTA_EVALS_REAL=1");
		const ci = run({ JOCASTA_EVALS_REAL: "1", CI: "true" });
		expect(ci.status).toBe(2);
		expect(ci.stderr).toContain("never run in CI");
		const noOmp = run({ JOCASTA_EVALS_REAL: "1", CI: undefined, PATH: tempDir(), Path: undefined });
		expect(noOmp.status).toBe(2);
		expect(noOmp.stderr).toContain("omp is not on PATH");
	});
});

describe("the trial repo (0482)", () => {
	test("holds every fixture file, ignored ones too, runs setup on a tagged commit, and runs none of setup's hooks", async () => {
		const fixture = tempDir();
		// Upstream tracks internal/gh although `gh` (its binary) is ignored, as cli/cli does; setup installs hooks that
		// refuse every commit, husky's way (`core.hooksPath`) and simple-git-hooks' way (`.git/hooks`).
		writeFiles(fixture, {
			".gitignore": "gh\n",
			"internal/gh/gh.go": "package gh\n",
			"setup.ts": [
				'import { spawnSync } from "node:child_process";',
				'import { chmodSync, mkdirSync, writeFileSync } from "node:fs";',
				'for (const dir of [".husky", ".git/hooks"]) {',
				"\tmkdirSync(dir, { recursive: true });",
				'\twriteFileSync(dir + "/pre-commit", "#!/bin/sh\\nexit 1\\n");',
				'\tchmodSync(dir + "/pre-commit", 0o755);',
				"}",
				'spawnSync("git", ["config", "core.hooksPath", ".husky"]);',
				"",
			].join("\n"),
		});
		const evalCase = {
			id: "trial",
			dir: fixture,
			prompt: "p",
			tags: [],
			checks: [{ git_clean: true }],
			hiddenChecks: [],
			// setuptools-scm reads the version at setup: the commit and its tag must both be there by then.
			setup: `git describe --tags --exact-match && "${process.execPath}" setup.ts`,
			fixture,
			fixtureTag: "v2.3.0",
		};
		const root = tempDir();
		const { repo } = await makeTrialRepo(evalCase, 1, root, process.env);
		const copy = join(tempDir(), "copy");
		expect(spawnSync("git", ["clone", "-q", repo, copy]).status).toBe(0);
		expect(existsSync(join(copy, "internal", "gh", "gh.go"))).toBe(true);
		// The tag moved onto the commit that folds setup's output in, so the clean clone's checkout is still at it.
		const described = spawnSync("git", ["describe", "--tags"], { cwd: copy, encoding: "utf8" });
		expect(described.stdout.trim()).toBe("v2.3.0");
		const commit = spawnSync("git", ["commit", "-q", "--allow-empty", "-m", "the agent's commit"], { cwd: repo });
		expect(commit.status).toBe(0);
	});

	test("a setup past its limit is ended with its tree and the trial is infra (0574, amended)", async () => {
		const cases = tempDir();
		const pidFile = join(tempDir(), "setup-grandchild.pid").replaceAll("\\", "/");
		// Real time on purpose: the limit is the platform clock ending a child process tree, which fake timers can't drive.
		const hang = `const c = require("node:child_process").spawn(process.execPath, ["-e", "setTimeout(() => {}, 120000)"], { stdio: "ignore" }); require("node:fs").writeFileSync(${JSON.stringify(pidFile)}, String(c.pid)); for (;;) {}`;
		writeFiles(cases, {
			"slowsetup/case.yaml": JSON.stringify({
				prompt: "p",
				setup: `"${process.execPath}" -e ${JSON.stringify(hang)}`,
				checks: [{ git_clean: true }],
			}),
		});
		const started = Date.now();
		await expect(
			runEvals({ casesDir: cases, outDir: join(tempDir(), "run"), reps: 1, launcher: omp, setupTimeoutMs: 2_000 }),
		).rejects.toThrow(/^slowsetup: setup `.*` killed at its time limit$/);
		expect(Date.now() - started).toBeLessThan(40_000);
		const grandchild = Number(readFileSync(pidFile, "utf8"));
		expect(() => process.kill(grandchild, 0)).toThrow();
	}, 60_000);
});

describe("end-state grader", () => {
	/** A committed repo with app.txt; `gradeCheck` only reads it. */
	function repo(dir: string): string {
		sh(dir, "init", "-q", "-b", "main");
		writeFileSync(join(dir, "app.txt"), "version 2 for client-a\n");
		sh(dir, "add", "-A");
		sh(dir, "commit", "-q", "-m", "init");
		return dir;
	}

	// The ten read-only cases share one repo; the test that changes a file makes its own.
	let shared = "";
	beforeAll(() => {
		shared = repo(mkdtempSync(join(tmpdir(), "jocasta-evals-grader-")));
	});
	afterAll(() => removeTree(shared));

	test.each<[string, Check, boolean]>([
		["a command that exits 0", { command: "git rev-parse HEAD" }, true],
		["a command that fails", { command: "git rev-parse no-such-ref" }, false],
		["a file that exists", { file: "app.txt" }, true],
		["a file that is missing", { file: "missing.txt" }, false],
		["a file with the text", { file: "app.txt", contains: "version 2" }, true],
		["a file without the text", { file: "app.txt", contains: "version 3" }, false],
		["a missing file that should be absent", { file: "missing.txt", absent: true }, true],
		["a file that should be absent", { file: "app.txt", absent: true }, false],
		["a clean tree expected clean", { git_clean: true }, true],
		["a clean tree expected dirty", { git_clean: false }, false],
	])("%s", async (_name, check, expected) => {
		expect(await gradeCheck(check, shared)).toBe(expected);
	});

	test("git_clean sees uncommitted changes", async () => {
		const dir = repo(tempDir());
		writeFileSync(join(dir, "app.txt"), "changed\n");
		expect(await gradeCheck({ git_clean: true }, dir)).toBe(false);
		expect(await gradeCheck({ git_clean: false }, dir)).toBe(true);
	});
});

describe("grading after the harness (0574)", () => {
	/** A harness script: `--version` answers; otherwise it runs `body` (JavaScript) in the trial's checkout. */
	const harness = (body: string) => {
		const file = join(tempDir(), "harness.ts");
		writeFileSync(
			file,
			`import { spawnSync } from "node:child_process";\nimport { writeFileSync } from "node:fs";\nif (process.argv.includes("--version")) { console.log("omp/0.0.0-stub"); process.exit(0); }\n${body}\n`,
		);
		return [process.execPath, file];
	};
	const bun = (script: string) => `"${process.execPath}" -e ${JSON.stringify(script)}`;

	test("a hidden check past the grading limit is ended with its tree and grades 0, and the trial counts", async () => {
		const cases = tempDir();
		const pidFile = join(tempDir(), "grandchild.pid").replaceAll("\\", "/");
		const hang = `const c = require("node:child_process").spawn(process.execPath, ["-e", "setTimeout(() => {}, 120000)"], { stdio: "ignore" }); require("node:fs").writeFileSync(${JSON.stringify(pidFile)}, String(c.pid)); for (;;) {}`;
		writeFiles(cases, {
			"hang/case.yaml": JSON.stringify({
				prompt: "Write out.txt: hello",
				checks: [{ git_clean: false }],
				hidden_checks: [{ command: bun(hang) }],
			}),
		});
		const started = Date.now();
		const [row] = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: omp,
			gradingTimeoutMs: 2_000,
		});
		expect(Date.now() - started).toBeLessThan(40_000);
		expect(row?.grade).toMatchObject({ "omp exited 0": 1, [`hidden: command: ${bun(hang)}`]: 0, "hidden: timeout": 0 });
		expect(row?.hiddenPassed).toBe(false);
		const grandchild = Number(readFileSync(pidFile, "utf8"));
		expect(() => process.kill(grandchild, 0)).toThrow();
	}, 60_000);

	test("a grading setup the change broke grades 0; one that fails on the fixture commit too is infra", async () => {
		const cases = tempDir();
		const setup = bun(`process.exit(require("node:fs").readFileSync("dep.txt", "utf8").trim() === "ok" ? 0 : 1)`);
		writeFiles(cases, {
			"broke/case.yaml": JSON.stringify({
				prompt: "Write dep.txt: broken",
				setup,
				checks: [{ git_clean: false }],
				hidden_checks: [{ command: "git --version" }],
			}),
			"broke/fixture/dep.txt": "ok\n",
		});
		const [row] = await runEvals({ casesDir: cases, outDir: join(tempDir(), "run"), reps: 1, launcher: omp });
		expect(row?.grade).toEqual({ "omp exited 0": 1, "git_clean: false": 1, "hidden: setup failed": 0 });
		expect([row?.hiddenPassed, row?.passed]).toEqual([false, false]);

		// Setup passes before the harness and fails after it on any commit: the setup itself is broken. The harness
		// writes the marker into the trial root, beside the repo and the grading clones.
		const infra = tempDir();
		writeFiles(infra, {
			"flaky/case.yaml": JSON.stringify({
				prompt: "Write ../broken: x",
				setup: bun(`process.exit(require("node:fs").existsSync("../broken") ? 1 : 0)`),
				checks: [{ git_clean: true }],
				hidden_checks: [{ command: "git --version" }],
			}),
		});
		await expect(runEvals({ casesDir: infra, outDir: join(tempDir(), "run"), reps: 1, launcher: omp })).rejects.toThrow(
			/flaky: setup `.*` failed/,
		);
	}, 60_000);

	test("the fixture-commit confirmation runs within the same grading limit and, cut there, grades hidden: timeout (EVL-5)", async () => {
		const cases = tempDir();
		// 1.5 s each way: the trial's setup fails inside the 2.5 s limit; the confirmation would pass, but past it. Real
		// time on purpose: the limit is the platform clock ending a child process tree, which fake timers can't drive.
		const setup = bun(
			`const ok = require("node:fs").readFileSync("dep.txt", "utf8").trim() === "ok"; setTimeout(() => process.exit(ok ? 0 : 1), 1500)`,
		);
		writeFiles(cases, {
			"slow/case.yaml": JSON.stringify({
				prompt: "Write dep.txt: broken",
				setup,
				checks: [{ git_clean: false }],
				hidden_checks: [{ command: "git --version" }],
			}),
			"slow/fixture/dep.txt": "ok\n",
		});
		const [row] = await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: omp,
			gradingTimeoutMs: 2_500,
		});
		expect(row?.grade).toEqual({ "omp exited 0": 1, "git_clean: false": 1, "hidden: timeout": 0 });
		expect(row?.hiddenPassed).toBe(false);
	}, 60_000);

	test("grading takes the commit the agent left, on a branch of its own too (EVL-8)", async () => {
		const cases = tempDir();
		const grader = `process.exit(require("node:fs").readFileSync("out.txt", "utf8").trim() === "hello" ? 0 : 1)`;
		writeFiles(cases, {
			"branch/case.yaml": JSON.stringify({
				prompt: "Fix it",
				checks: [{ git_clean: false }],
				hidden_checks: [{ command: bun(grader) }],
			}),
		});
		const launcher = harness(
			`spawnSync("git", ["checkout", "-q", "-b", "fix/issue-3"]); writeFileSync("out.txt", "hello\\n");`,
		);
		const [row] = await runEvals({ casesDir: cases, outDir: join(tempDir(), "run"), reps: 1, launcher });
		expect(row?.hiddenPassed).toBe(true);
	}, 60_000);

	test("Ponytail's leading comment stays out of the prompt every arm appends (0577)", async () => {
		const cases = tempDir();
		writeFiles(cases, { "c/case.yaml": "prompt: p\nchecks:\n  - git_clean: true\n" });
		const prompt = join(tempDir(), "ponytail.md");
		writeFileSync(
			prompt,
			"<!-- Ponytail, MIT License, example-org -->\n\n# Ponytail, lazy senior dev mode\n\nBe lazy.\n",
		);
		const calls = ompLog().calls;
		await runEvals({
			casesDir: cases,
			outDir: join(tempDir(), "run"),
			reps: 1,
			launcher: omp,
			variant: { ...DEFAULT_VARIANT, label: "a1", appendSystemPrompt: prompt },
		});
		const [call] = calls() as (OmpCall & { appendedText?: string })[];
		expect(call?.appendedText).toBe("# Ponytail, lazy senior dev mode\n\nBe lazy.\n");
		expect(readFileSync(prompt, "utf8")).toStartWith("<!-- Ponytail");
	}, 60_000);
});

describe("case files", () => {
	test.each([
		["no check kind", "prompt: p\nchecks:\n  - contains: x\n", "case.yaml:3: checks[0] needs one of"],
		[
			"two check kinds",
			"prompt: p\nchecks:\n  - file: a\n  - command: b\n    git_clean: true\n",
			"case.yaml:4: checks[1] needs one of",
		],
		["an unknown key", "prompt: p\nchecks:\n  - file: a\nsplit: test\n", "case.yaml:4: unknown key `split`"],
		["no checks", "prompt: p\nchecks: []\n", "case.yaml:2: checks must"],
	])("%s is reported with file and line", (_name, source, message) => {
		const dir = tempDir();
		writeFiles(dir, { "c/case.yaml": source });
		expect(() => loadCases(dir)).toThrow(message);
	});
});

test("readResponse takes the model from the responses, sums usage, counts turns and tool calls, and adds task subagents", () => {
	const usage = (input: number, total: number) => ({
		input,
		output: 2,
		cacheRead: 3,
		cacheWrite: 4,
		totalTokens: input + 9,
		cost: { total },
	});
	const call = { type: "toolCall", id: "c1", name: "read", arguments: {} };
	const transcript = [
		JSON.stringify({ type: "session", version: 3, id: "s", cwd: "/repo" }),
		JSON.stringify({ type: "message_end", message: { role: "user", content: "hi" } }),
		JSON.stringify({
			type: "message_end",
			message: {
				role: "assistant",
				model: "model-a",
				content: [{ type: "text", text: "x" }, call, call],
				usage: usage(10, 0.5),
			},
		}),
		JSON.stringify({
			type: "message_update",
			message: { role: "assistant", model: "ignored", content: [call], usage: usage(99, 9) },
		}),
		JSON.stringify({
			type: "tool_execution_end",
			toolName: "task",
			result: { details: { results: [{ requests: 2 }, { requests: 1 }], usage: usage(100, 1) } },
		}),
		// Another tool's details never count, nor does the tool result's own message_end.
		JSON.stringify({ type: "tool_execution_end", toolName: "read", result: { details: { usage: usage(7, 7) } } }),
		JSON.stringify({ type: "message_end", message: { role: "toolResult", details: { usage: usage(100, 1) } } }),
		JSON.stringify({ type: "message_end", message: { role: "assistant", model: "model-b", usage: usage(20, 0.25) } }),
	].join("\n");
	expect(readResponse(transcript)).toEqual({
		model: "model-b",
		usage: { input: 30, output: 4, cacheRead: 6, cacheWrite: 8, totalTokens: 48, cost: 0.75 },
		totalUsage: { input: 130, output: 6, cacheRead: 9, cacheWrite: 12, totalTokens: 157, cost: 1.75 },
		requests: 5,
		turns: 2,
		toolCalls: 2,
	});
});

test("countFindings counts [P0]–[P3] markers; P0 and P1 block", () => {
	expect(countFindings("[P0] a\n[P1] b\n[P2] c\n[P3] d\n[P4] not one")).toEqual({ total: 4, blocking: 2 });
	expect(countFindings("No findings.")).toEqual({ total: 0, blocking: 0 });
});

test("readClaudeResponse sums result events over turns and counts tool_use blocks", () => {
	const result = (cost: number) =>
		JSON.stringify({
			type: "result",
			num_turns: 3,
			total_cost_usd: cost,
			usage: { input_tokens: 1, output_tokens: 2, cache_read_input_tokens: 3, cache_creation_input_tokens: 4 },
		});
	const transcript = [
		JSON.stringify({ type: "assistant", message: { model: "m-1", content: [{ type: "tool_use" }, { type: "text" }] } }),
		result(0.25),
		JSON.stringify({ type: "assistant", message: { model: "m-2", content: [{ type: "tool_use" }] } }),
		result(0.5),
	].join("\n");
	const usage = { input: 2, output: 4, cacheRead: 6, cacheWrite: 8, totalTokens: 20, cost: 0.75 };
	expect(readClaudeResponse(transcript)).toEqual({
		model: "m-2",
		usage,
		totalUsage: usage,
		requests: 6,
		turns: 6,
		toolCalls: 2,
	});
});

test("a Claude Code run killed at the limit has no result: its turns are its distinct assistant messages (EVL-3)", () => {
	const block = (id: string, type: string) =>
		JSON.stringify({ type: "assistant", message: { id, content: [{ type }] } });
	const transcript = [
		JSON.stringify({ type: "system", subtype: "init" }),
		block("msg_1", "text"),
		block("msg_1", "tool_use"),
		block("msg_2", "tool_use"),
	].join("\n");
	expect(readClaudeResponse(transcript)).toMatchObject({ turns: 2, requests: 2, toolCalls: 2 });
});

test("a last line torn by a kill is skipped by every transcript reader (DUP-1)", () => {
	const omp = [
		JSON.stringify({ type: "message_end", message: { role: "assistant", content: [{ type: "text", text: "Done." }] } }),
		'{"type":"message_end","message":{"role":"assistant","content":[{"type":"te',
	].join("\n");
	expect(readResponse(omp).turns).toBe(1);
	expect(assistantText(omp, "omp")).toBe("Done.");
	const claude = `${JSON.stringify({ type: "assistant", message: { id: "m", content: [{ type: "text", text: "Hi" }] } })}\n{"type":"res`;
	expect(readClaudeResponse(claude).turns).toBe(1);
	expect(assistantText(claude, "claude")).toBe("Hi");
});

test("trialArgs: a claude follow-up continues with -c and never takes omp's overlay or profile", () => {
	const variant: Variant = { ...DEFAULT_VARIANT, harness: "claude", model: "m" };
	expect(trialArgs(variant, { prompt: "next", followUp: true, overlay: "/o.yml", profile: "p" })).toEqual([
		"-c",
		"--model",
		"m",
		"-p",
		"next",
		"--output-format",
		"stream-json",
		"--verbose",
	]);
});
