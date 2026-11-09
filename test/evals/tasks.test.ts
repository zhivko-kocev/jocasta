import { afterEach, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Arm, runBench, runValidation, type TrialJob, type TrialOutcome } from "../../evals/bench.ts";
import type { ResultRow } from "../../evals/runner.ts";
import {
	type BenchTask,
	classifyInfra,
	loadTasks,
	MAX_ATTEMPTS,
	PYTHON_SETUP_PREFIX,
	pendingTrials,
	pickPilot,
	planTrials,
	prepareTask,
	readResumeState,
	scanLeaks,
	shuffle,
	TASK_PROMPT_PREFIX,
	taskCase,
	trialKey,
} from "../../evals/tasks.ts";
import { DEFAULT_VARIANT } from "../../evals/variants.ts";
import { removeTree } from "../../src/shared/fsx.ts";

const temps: string[] = [];
afterEach(() => {
	for (const dir of temps.splice(0)) removeTree(dir);
	delete process.env.JOCASTA_TEST_OMP_LOG;
});

function tempDir(): string {
	const dir = mkdtempSync(join(tmpdir(), "jocasta-bench-"));
	temps.push(dir);
	return dir;
}

function git(cwd: string, ...args: string[]): string {
	const result = spawnSync("git", ["-c", "core.autocrlf=false", ...args], { cwd, encoding: "utf8" });
	if (result.status !== 0) throw new Error(`git ${args.join(" ")}: ${result.stderr}`);
	return result.stdout;
}

function writeFiles(root: string, files: Record<string, string>): void {
	for (const [path, content] of Object.entries(files)) {
		mkdirSync(join(root, path, ".."), { recursive: true });
		writeFileSync(join(root, path), content);
	}
}

const CHECK = `import { readFileSync } from "node:fs";
const ok = readFileSync("lib.txt", "utf8").includes("fixed") && readFileSync("tests/existing.txt", "utf8").includes("new");
process.exit(ok ? 0 : 1);
`;

/**
 * A local upstream: the base commit (the bug), the fix's merge commit (gold change, a changed test and a new one) and a
 * later commit that must never reach the fixture. The task fetches it by path, so nothing touches the network.
 */
function localTask(overrides: Partial<BenchTask> = {}): BenchTask {
	const source = tempDir();
	git(source, "init", "-q", "-b", "main");
	git(source, "config", "user.name", "Acme Dev");
	git(source, "config", "user.email", "dev@example.org");
	writeFiles(source, { "lib.txt": "bug\n", "tests/existing.txt": "old\n", "README.md": "# widget\n" });
	git(source, "add", "-A");
	git(source, "commit", "-q", "-m", "base");
	const base = git(source, "rev-parse", "HEAD").trim();
	git(source, "tag", "v1.0.0");
	writeFiles(source, { "lib.txt": "fixed\n", "tests/existing.txt": "new\n", "tests/check.ts": CHECK });
	git(source, "add", "-A");
	git(source, "commit", "-q", "-m", "fix (#7)");
	const merge = git(source, "rev-parse", "HEAD").trim();
	writeFiles(source, { "later.txt": "after the fix\n" });
	git(source, "add", "-A");
	git(source, "commit", "-q", "-m", "later");
	git(source, "tag", "v1.1.0");
	return {
		instance_id: "acme__widget-7",
		set: "main",
		repo: "acme/widget",
		repo_url: source,
		base_commit: base,
		problem_statement: "lib.txt says bug\n\nIt should say it is fixed.",
		gold_patch: git(source, "diff", base, merge, "--", "lib.txt"),
		test_patch: git(source, "diff", base, merge, "--", "tests"),
		hidden_test_files: ["tests/check.ts", "tests/existing.txt"],
		setup_command: "git --version",
		test_command: `"${process.execPath}" tests/check.ts`,
		language: "JavaScript",
		pr_url: "https://github.com/acme/widget/pull/7",
		issue_urls: ["https://github.com/acme/widget/issues/3"],
		merge_commit: merge,
		...overrides,
	};
}

const stubTask = (id: string, language: BenchTask["language"], set: BenchTask["set"] = "main"): BenchTask => ({
	instance_id: id,
	set,
	repo: "acme/widget",
	repo_url: "https://github.com/acme/widget",
	base_commit: "a".repeat(40),
	problem_statement: `Issue of ${id}`,
	gold_patch: "diff --git a/lib.txt b/lib.txt\n",
	test_patch: "diff --git a/test.txt b/test.txt\n",
	hidden_test_files: [],
	setup_command: "pip install -e .",
	test_command: "pytest",
	language,
	pr_url: "https://github.com/acme/widget/pull/12",
	issue_urls: ["https://github.com/acme/widget/issues/11"],
	merge_commit: "0123456789abcdef0123456789abcdef01234567",
});

function fakeRow(overrides: Partial<ResultRow> = {}): ResultRow {
	const usage = { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: 0 };
	return {
		case: "acme__widget-7",
		rep: 0,
		variant: "a0",
		harness: "omp",
		harnessVersion: "omp/0.0.0-stub",
		startedAt: "2026-10-27T10:00:00.000Z",
		model: "stub-model",
		usage,
		totalUsage: usage,
		requests: 2,
		turns: 2,
		toolCalls: 1,
		prompts: 1,
		wallMs: 10,
		linesAdded: 1,
		linesRemoved: 1,
		grade: { "omp exited 0": 1, "hidden: command: test": 1 },
		hiddenPassed: true,
		findings: null,
		passed: true,
		...overrides,
	};
}

const readJsonl = (file: string) =>
	readFileSync(file, "utf8")
		.trim()
		.split("\n")
		.filter((line) => line !== "")
		.map((line) => JSON.parse(line));

const host = () => ({
	startedAt: "2026-10-27T10:00:00.000Z",
	localHour: 10,
	platform: "win32",
	osRelease: "10.0",
	arch: "x64",
	cpu: "stub",
	cpuCount: 4,
	memoryGb: 8,
	versions: {},
});

const stubArm = (name: string): Arm => ({
	name,
	variant: { ...DEFAULT_VARIANT, label: name },
	launcher: ["omp"],
	harnessVersion: "omp/0.0.0-stub",
});

describe("the trial order (0482)", () => {
	test("a seeded shuffle is deterministic and a permutation", () => {
		const items = Array.from({ length: 50 }, (_, i) => i);
		const once = shuffle(items, 482);
		expect(shuffle(items, 482)).toEqual(once);
		expect([...once].sort((a, b) => a - b)).toEqual(items);
		expect(once).not.toEqual(items);
		expect(shuffle(items, 483)).not.toEqual(once);
	});

	test("trials are every task × arm × rep once, interleaved, indexed in order", () => {
		const tasks = ["t1", "t2", "t3"].map((id) => stubTask(id, "Go"));
		const order = planTrials(tasks, ["a0", "a1"], 3, 482);
		expect(order).toHaveLength(18);
		expect(new Set(order.map(trialKey)).size).toBe(18);
		expect(order.map((trial) => trial.index)).toEqual(Array.from({ length: 18 }, (_, i) => i));
		expect(planTrials(tasks, ["a0", "a1"], 3, 482)).toEqual(order);
		// Not grouped by arm: the first six trials aren't all one arm.
		expect(new Set(order.slice(0, 6).map((trial) => trial.arm)).size).toBe(2);
	});

	test("the pilot picks 2 Python, 2 TypeScript/JavaScript and 1 Go task, the same for a seed", () => {
		const tasks = [
			...["p1", "p2", "p3", "p4"].map((id) => stubTask(id, "Python")),
			stubTask("t1", "TypeScript"),
			stubTask("j1", "JavaScript"),
			stubTask("j2", "JavaScript"),
			...["g1", "g2", "g3"].map((id) => stubTask(id, "Go")),
		];
		const pilot = pickPilot(tasks, 482);
		expect(pilot.map((task) => task.language)).toEqual([
			"Python",
			"Python",
			expect.stringMatching(/^(TypeScript|JavaScript)$/),
			expect.stringMatching(/^(TypeScript|JavaScript)$/),
			"Go",
		]);
		expect(pickPilot([...tasks].reverse(), 482)).toEqual(pilot);
		expect(planTrials(pilot, ["a0", "a1", "a2", "a3", "a4"], 1, 482)).toHaveLength(25);
		expect(() => pickPilot(tasks.slice(0, 7), 482)).toThrow(/the pilot needs 1 Go/);
	});
});

describe("tasks.jsonl (0482)", () => {
	test("loads the tasks and names a bad line", () => {
		const dir = tempDir();
		const file = join(dir, "tasks.jsonl");
		writeFileSync(file, `${JSON.stringify(stubTask("acme__widget-1", "Go"))}\n\n`);
		expect(loadTasks(file).map((task) => task.instance_id)).toEqual(["acme__widget-1"]);
		writeFileSync(file, `${JSON.stringify({ ...stubTask("acme__widget-1", "Go"), test_command: "" })}\n`);
		expect(() => loadTasks(file)).toThrow(/tasks\.jsonl:1: `test_command`/);
		writeFileSync(file, `${JSON.stringify({ ...stubTask("acme__widget-1", "Go"), language: "Rust" })}\n`);
		expect(() => loadTasks(file)).toThrow(/`language`/);
	});

	test("a task becomes a case: the fixed prompt, hidden test command, a venv for Python", () => {
		const prepared = {
			dir: "/cache/x",
			fixture: "/cache/x/fixture",
			hidden: "/cache/x/hidden",
			goldPatch: "/cache/x/gold.patch",
			testPatch: "/cache/x/test.patch",
		};
		const python = taskCase(stubTask("acme__py-1", "Python"), prepared);
		expect(TASK_PROMPT_PREFIX).toBe("Fix the issue below in the repository in the current folder.\n\n");
		expect(python.prompt).toBe(`${TASK_PROMPT_PREFIX}Issue of acme__py-1`);
		expect(python.setup).toBe(
			"(if not exist ..\\venv\\Scripts\\python.exe python -m venv ..\\venv) && python -m pip install -q --upgrade pip && pip install -e .",
		);
		expect(python.setup?.startsWith(PYTHON_SETUP_PREFIX)).toBe(true);
		expect(python.pathPrefix).toEqual(["venv/Scripts"]);
		expect(python.checks).toEqual([]);
		expect(python.hiddenChecks).toEqual([{ command: "pytest" }]);
		expect([python.fixture, python.hidden, python.id]).toEqual(["/cache/x/fixture", "/cache/x/hidden", "acme__py-1"]);
		const go = taskCase({ ...stubTask("acme__go-1", "Go"), setup_command: "go mod download" }, prepared);
		expect(go.setup).toBe("go mod download");
		expect(go.pathPrefix).toBeUndefined();
		expect(go.tags).toEqual(["Go", "main"]);
	});
});

describe("prepareTask (0482)", () => {
	test("fetches the base commit alone, writes its tree and the hidden test files, once", () => {
		const task = localTask();
		const cache = tempDir();
		const prepared = prepareTask(task, cache);
		expect(prepared.fixture).toBe(join(cache, "acme__widget-7", "fixture"));
		expect(readFileSync(join(prepared.fixture, "lib.txt"), "utf8")).toBe("bug\n");
		expect(readFileSync(join(prepared.fixture, "tests", "existing.txt"), "utf8")).toBe("old\n");
		expect(existsSync(join(prepared.fixture, "tests", "check.ts"))).toBe(false);
		expect(existsSync(join(prepared.fixture, "later.txt"))).toBe(false);
		expect(existsSync(join(prepared.fixture, ".git"))).toBe(false);
		expect(readFileSync(join(prepared.hidden, "tests", "check.ts"), "utf8")).toBe(CHECK);
		expect(readFileSync(join(prepared.hidden, "tests", "existing.txt"), "utf8")).toBe("new\n");
		expect(existsSync(join(prepared.hidden, "lib.txt"))).toBe(false);
		expect(readFileSync(prepared.goldPatch, "utf8")).toBe(task.gold_patch);
		expect(readFileSync(prepared.testPatch, "utf8")).toBe(task.test_patch);
		expect(existsSync(join(cache, "repos", "acme__widget.git"))).toBe(true);
		// Upstream's nearest tag at the base commit, not the later one.
		expect(prepared.tag).toBe("v1.0.0");
		expect(existsSync(join(cache, "acme__widget-7", "scratch"))).toBe(false);
		// The marker matches, so a second call leaves the cache alone, even with the upstream gone.
		writeFileSync(join(prepared.fixture, "sentinel.txt"), "kept\n");
		expect(prepareTask({ ...task, repo_url: join(cache, "missing") }, cache)).toEqual(prepared);
		expect(existsSync(join(prepared.fixture, "sentinel.txt"))).toBe(true);
		// Changed patches prepare again, from the bare repo's copy of the commit, and keep the tag.
		const again = prepareTask({ ...task, gold_patch: `${task.gold_patch}\n`, repo_url: join(cache, "missing") }, cache);
		expect(existsSync(join(prepared.fixture, "sentinel.txt"))).toBe(false);
		expect(again.tag).toBe("v1.0.0");
	}, 60_000);

	test("a hidden test file the patch deletes fails preparation", () => {
		const task = localTask();
		expect(() => prepareTask({ ...task, hidden_test_files: ["tests/gone.txt"] }, tempDir())).toThrow(
			/deletes tests\/gone\.txt/,
		);
	}, 60_000);
});

describe("the leak scan (0482)", () => {
	const task = stubTask("acme__widget-12", "Go");
	test("finds the PR, issues, merge commit and its short hash outside the prompt", () => {
		const prompt = `${TASK_PROMPT_PREFIX}see https://github.com/acme/widget/issues/11`;
		expect(scanLeaks(task, JSON.stringify({ prompt }), prompt)).toEqual([]);
		expect(scanLeaks(task, "fetched https://github.com/acme/widget/pull/12/files")).toEqual([
			"https://github.com/acme/widget/pull/12",
			"github.com/acme/widget/pull/12",
		]);
		expect(scanLeaks(task, "see https://github.com/acme/widget/pull/123 and /issues/110")).toEqual([]);
		expect(scanLeaks(task, "HTTPS://GITHUB.COM/acme/widget/issues/11")).toEqual([
			"https://github.com/acme/widget/issues/11",
		]);
		expect(scanLeaks(task, `git show 0123456789ab`)).toEqual(["0123456789"]);
		expect(scanLeaks(task, `/commit/${task.merge_commit}`)).toEqual([`/commit/${task.merge_commit}`, "0123456789"]);
		expect(scanLeaks(task, "ff0123456789")).toEqual([]);
	});
});

describe("the infra classifier (0482)", () => {
	const failed = fakeRow({ grade: { "omp exited 0": 0 }, turns: 0, toolCalls: 0, hiddenPassed: false });
	test("setup and runner errors", () => {
		expect(classifyInfra({ error: "acme__widget-7: setup `npm install` failed: ENOTFOUND" })?.kind).toBe("setup");
		expect(classifyInfra({ error: "git clone failed" })?.kind).toBe("error");
	});
	test("sign-in failures in stderr or the transcript's end", () => {
		expect(classifyInfra({ row: failed, stderr: "Error: 401 Unauthorized" })?.kind).toBe("auth");
		expect(classifyInfra({ row: failed, stderr: "OAuth token refresh failed: invalid_grant" })?.kind).toBe("auth");
		expect(
			classifyInfra({
				row: fakeRow({ harness: "claude", grade: { "claude exited 0": 0 }, turns: 0, toolCalls: 0 }),
				transcript: `${JSON.stringify({ type: "result", is_error: true, result: "Invalid API key · Please run /login" })}\n`,
			})?.kind,
		).toBe("auth");
		// A trial that ran and exited 0 counts, whatever its transcript says about OAuth.
		expect(classifyInfra({ row: fakeRow(), transcript: "fixed the OAuth refresh token bug (401)" })).toBeNull();
	});
	test("rate limits without a tool call, and no model response", () => {
		expect(classifyInfra({ row: failed, stderr: "429 Too Many Requests: rate_limit_error" })?.kind).toBe("rate-limit");
		expect(classifyInfra({ row: failed, stderr: "API Error: 529 overloaded_error" })?.kind).toBe("rate-limit");
		expect(classifyInfra({ row: fakeRow({ toolCalls: 3 }), stderr: "overloaded" })).toBeNull();
		expect(classifyInfra({ row: failed, stderr: "segmentation fault" })?.kind).toBe("no-response");
		// The issue's own text never makes a rate limit.
		const prompt = `${TASK_PROMPT_PREFIX}undici retries 429 responses forever`;
		expect(
			classifyInfra({ row: fakeRow({ toolCalls: 0 }), transcript: JSON.stringify({ prompt }), prompt }),
		).toBeNull();
		expect(classifyInfra({ row: fakeRow({ grade: { "omp exited 0": 0 }, hiddenPassed: false }) })).toBeNull();
	});
	test("a rate limit at the end while an account was full is an infra error even after tool calls (0543)", () => {
		const worked = fakeRow({ toolCalls: 5, grade: { "omp exited 0": 0 }, hiddenPassed: false });
		const tail = `${JSON.stringify({ type: "message_end", message: { role: "assistant", errorMessage: "429 rate_limit_error" } })}\n`;
		expect(classifyInfra({ row: worked, transcript: tail, accountFull: true })?.kind).toBe("rate-limit");
		expect(classifyInfra({ row: worked, stderr: "usage limit reached", accountFull: true })?.kind).toBe("rate-limit");
		// No full account: the trial counts as it ended.
		expect(classifyInfra({ row: worked, transcript: tail, accountFull: false })).toBeNull();
	});
});

describe("resume (0482)", () => {
	test("done trials are skipped; infra errors retry up to the limit", () => {
		const out = tempDir();
		const order = planTrials([stubTask("t1", "Go"), stubTask("t2", "Go")], ["a0"], 2, 482);
		const [first, second, third] = order as [(typeof order)[0], (typeof order)[0], (typeof order)[0]];
		writeFileSync(join(out, "results.jsonl"), `${JSON.stringify({ ...first, attempt: 1 })}\n`);
		const errors = [second, third, third, third, first].map((trial) => JSON.stringify({ ...trial, kind: "setup" }));
		writeFileSync(join(out, "errors.jsonl"), `${errors.join("\n")}\n`);
		const state = readResumeState(out);
		expect(state.done).toEqual(new Set([trialKey(first)]));
		expect(state.failures.get(trialKey(third))).toBe(MAX_ATTEMPTS);
		const { pending, exhausted } = pendingTrials(order, state);
		expect(pending.map(trialKey)).toEqual(order.filter((t) => t !== first && t !== third).map(trialKey));
		expect(exhausted).toEqual([third]);
		expect(pendingTrials(order, readResumeState(tempDir())).pending).toEqual(order);
	});

	test("runBench retries an infra error later, writes rows with the benchmark's fields, and resumes", async () => {
		const task = localTask();
		const out = join(tempDir(), "run");
		const jobs: TrialJob[] = [];
		let calls = 0;
		const execute = async (job: TrialJob): Promise<TrialOutcome> => {
			jobs.push(job);
			calls++;
			if (calls === 1) return { error: "acme__widget-7: setup `git --version` failed: boom", stderr: "" };
			writeFileSync(
				join(job.options.outDir, "transcripts", `${job.options.transcriptName}.jsonl`),
				`${JSON.stringify({ text: "read https://github.com/acme/widget/pull/7" })}\n`,
			);
			return { row: fakeRow({ rep: job.rep, variant: job.options.variant?.label }), stderr: "" };
		};
		const options = {
			tasks: [task],
			cacheDir: tempDir(),
			outDir: out,
			arms: [stubArm("a0"), stubArm("a1")],
			reps: 1,
			seed: 482,
			workers: 2,
			execute,
			describeHost: host,
			log: () => {},
		};
		expect(await runBench(options)).toBe(0);
		expect(calls).toBe(3);
		const errors = readJsonl(join(out, "errors.jsonl"));
		expect(errors).toHaveLength(1);
		expect(errors[0]).toMatchObject({ kind: "setup", attempt: 1, harness: "omp" });
		const rows = readJsonl(join(out, "results.jsonl"));
		expect(rows).toHaveLength(2);
		const retried = rows.find((row) => trialKey(row) === trialKey(errors[0]));
		expect(retried).toMatchObject({
			attempt: 2,
			language: "JavaScript",
			leak: ["https://github.com/acme/widget/pull/7", "github.com/acme/widget/pull/7"],
		});
		expect(rows.map((row) => row.trialIndex).sort()).toEqual([0, 1]);
		expect(jobs[0]?.options.transcriptName).toMatch(/^acme__widget-7-a[01]-0$/);
		expect(jobs[0]?.evalCase.prompt).toBe(`${TASK_PROMPT_PREFIX}${task.problem_statement}`);
		const conditions = JSON.parse(readFileSync(join(out, "conditions.json"), "utf8"));
		expect(conditions).toMatchObject({ seed: 482, arms: ["a0", "a1"], reps: 1, tasks: ["acme__widget-7"] });
		expect(conditions.order).toHaveLength(2);
		expect(conditions.sessions[0].finishedAt).toBeString();
		// Run again: nothing left to do.
		expect(await runBench(options)).toBe(0);
		expect(calls).toBe(3);
		expect(JSON.parse(readFileSync(join(out, "conditions.json"), "utf8")).sessions).toHaveLength(2);
		await expect(runBench({ ...options, seed: 1 })).rejects.toThrow(/another plan/);
	}, 60_000);

	test("with two workers, no two trials of one task run at once; the rest keeps the seeded order", async () => {
		const tasks = [localTask(), { ...localTask(), instance_id: "acme__widget-8" }];
		const arms = [stubArm("a0"), stubArm("a1"), stubArm("a2")];
		const running = new Set<string>();
		const started: string[] = [];
		let overlap = 0;
		let parallel = 0;
		const code = await runBench({
			tasks,
			cacheDir: tempDir(),
			outDir: join(tempDir(), "run"),
			arms,
			reps: 2,
			seed: 482,
			workers: 2,
			describeHost: host,
			log: () => {},
			execute: async (job) => {
				const id = job.evalCase.id;
				if (running.has(id)) overlap++;
				running.add(id);
				parallel = Math.max(parallel, running.size);
				started.push(`${id}/${job.options.transcriptName}`);
				// Yield some microtasks, so the other worker starts its trial while this one is still running.
				for (let i = 0; i < 10; i++) await Promise.resolve();
				running.delete(id);
				return { row: fakeRow({ rep: job.rep }), stderr: "" };
			},
		});
		expect(code).toBe(0);
		expect(overlap).toBe(0);
		expect(parallel).toBe(2);
		expect(started).toHaveLength(12);
		const order = planTrials(tasks, ["a0", "a1", "a2"], 2, 482);
		// The first trial in the order starts first; the second worker takes the first one of the other task.
		expect(started[0]).toBe(`${order[0]?.task}/${order[0]?.task}-${order[0]?.arm}-${order[0]?.rep}`);
		const other = order.find((trial) => trial.task !== order[0]?.task);
		expect(started[1]).toBe(`${other?.task}/${other?.task}-${other?.arm}-${other?.rep}`);
	}, 60_000);

	test("three sign-in failures in a row stop the run with exit code 3", async () => {
		const lines: string[] = [];
		let calls = 0;
		const code = await runBench({
			tasks: [localTask()],
			cacheDir: tempDir(),
			outDir: join(tempDir(), "run"),
			arms: [stubArm("a0")],
			reps: 5,
			seed: 482,
			workers: 1,
			describeHost: host,
			log: (line) => lines.push(line),
			execute: async () => {
				calls++;
				return {
					row: fakeRow({ grade: { "omp exited 0": 0 }, turns: 0, toolCalls: 0 }),
					stderr: "Error: not logged in; run omp and /login",
				};
			},
		});
		expect(code).toBe(3);
		expect(calls).toBe(3);
		expect(lines.at(-1)).toMatch(/3 trials in a row could not sign in to omp/);
	}, 60_000);

	test("a trial runs through runTrial in a worker thread with the fake omp, its root in --work, logs kept", async () => {
		const out = join(tempDir(), "run");
		// The real path: macOS's temp folder is a link (/var → /private/var), and the harness reports where it runs.
		const work = join(realpathSync.native(tempDir()), "w");
		const ompLog = join(tempDir(), "omp.jsonl");
		process.env.JOCASTA_TEST_OMP_LOG = ompLog;
		const code = await runBench({
			tasks: [localTask()],
			cacheDir: tempDir(),
			workDir: work,
			outDir: out,
			arms: [{ ...stubArm("a0"), launcher: [process.execPath, join(import.meta.dir, "fixtures", "omp.ts")] }],
			reps: 1,
			seed: 482,
			workers: 1,
			describeHost: host,
			log: () => {},
		});
		expect(code).toBe(0);
		const [row] = readJsonl(join(out, "results.jsonl"));
		expect(row).toMatchObject({
			case: "acme__widget-7",
			task: "acme__widget-7",
			arm: "a0",
			rep: 0,
			attempt: 1,
			trialIndex: 0,
			leak: [],
			model: "stub-model",
			hiddenPassed: false,
			passed: false,
		});
		expect(row.grade["omp exited 0"]).toBe(1);
		expect(existsSync(join(out, "transcripts", "acme__widget-7-a0-0.jsonl"))).toBe(true);
		const logs = join(out, "logs", "acme__widget-7-a0-0");
		expect(readFileSync(join(logs, "setup.log"), "utf8")).toMatch(/\$ git --version\ncwd: .*\nexit: 0\n/);
		const grade = readFileSync(join(logs, "grade.log"), "utf8");
		expect(grade).toContain("tests/check.ts");
		expect(grade).toMatch(/exit: 1\n/);
		// The harness ran in <work>\t??????\repo, a short root.
		const [call] = readJsonl(ompLog);
		expect(call.cwd).toMatch(/[\\/]w[\\/]t[^\\/]{6}[\\/]repo$/);
		expect(call.cwd.startsWith(work)).toBe(true);
		expect(JSON.parse(readFileSync(join(out, "conditions.json"), "utf8")).sessions[0].workRoot).toBe(work);
		// The ledger (0538): omp ran with --session-dir in the root, its session file was saved and priced.
		expect(call.argv).toContain("--session-dir");
		expect(existsSync(join(out, "sessions", "acme__widget-7-a0-0", "stub-session.jsonl"))).toBe(true);
		expect(row.ledger).toMatchObject({ sessions: 1, requests: 1, toolCalls: 1, input: 1000, output: 100 });
		expect(row.ledger.cost).toBeCloseTo(1000 * 4e-6 + 100 * 2e-5, 12);
		expect(readJsonl(join(out, "ledger.jsonl")).map((r) => [r.kind, r.task, r.arm])).toEqual([
			["request", "acme__widget-7", "a0"],
			["tool", "acme__widget-7", "a0"],
		]);
		expect(readJsonl(join(out, "spend.jsonl"))).toHaveLength(1);
	}, 60_000);

	/** An A7 trial: writes an opencode export with one step of `tokens` on `model` into the trial's sessions folder. */
	const a7Execute =
		(model: string, input: number) =>
		async (job: TrialJob): Promise<TrialOutcome> => {
			const dir = job.options.sessionsDir as string;
			mkdirSync(dir, { recursive: true });
			const exported = {
				info: { id: `ses_${job.rep}` },
				messages: [
					{
						info: { role: "assistant", providerID: "anthropic", modelID: model, time: { created: 1 } },
						parts: [{ type: "step-finish", tokens: { input, output: 0, reasoning: 0, cache: { read: 0, write: 0 } } }],
					},
				],
			};
			writeFileSync(join(dir, `ses_${job.rep}.json`), JSON.stringify(exported));
			return { row: fakeRow({ rep: job.rep, harness: "opencode", grade: { "opencode exited 0": 1 } }), stderr: "" };
		};
	const a7Arm: Arm = {
		name: "a7",
		variant: { ...DEFAULT_VARIANT, label: "a7", harness: "opencode" },
		launcher: ["opencode"],
		harnessVersion: "1.18.22",
	};

	test("A7 stops once its priced spend reaches the limit; the other arms run on; exit code 4 (0537)", async () => {
		const lines: string[] = [];
		const out = join(tempDir(), "run");
		// 100,000 input tokens on claude-opus-5-5 at $4/M: $0.40 a trial; the limit $1 stops A7 after 3 trials.
		const code = await runBench({
			tasks: [localTask()],
			cacheDir: tempDir(),
			outDir: out,
			arms: [a7Arm, stubArm("a0")],
			reps: 5,
			seed: 482,
			workers: 1,
			a7SpendLimitUsd: 1,
			describeHost: host,
			log: (line) => lines.push(line),
			execute: async (job) =>
				job.options.variant?.harness === "opencode"
					? a7Execute("claude-opus-5-5", 100_000)(job)
					: { row: fakeRow({ rep: job.rep }), stderr: "" },
		});
		expect(code).toBe(4);
		const rows = readJsonl(join(out, "results.jsonl"));
		expect(rows.filter((r) => r.arm === "a7")).toHaveLength(3);
		expect(rows.filter((r) => r.arm === "a0")).toHaveLength(5);
		expect(rows.find((r) => r.arm === "a7").ledger.cost).toBeCloseTo(0.4, 12);
		expect(lines.at(-1)).toMatch(/^A7 stopped \(A7's priced spend is \$1\.20; limit \$1, 0537\): 2 A7 trials not run/);
		expect(JSON.parse(readFileSync(join(out, "conditions.json"), "utf8")).sessions[0].a7SpendUsd).toBe(1.2);
	}, 60_000);

	test("an A7 model the price table lacks stops A7, since its spend can't be counted", async () => {
		const lines: string[] = [];
		const code = await runBench({
			tasks: [localTask()],
			cacheDir: tempDir(),
			outDir: join(tempDir(), "run"),
			arms: [a7Arm],
			reps: 3,
			seed: 482,
			workers: 1,
			describeHost: host,
			log: (line) => lines.push(line),
			execute: a7Execute("no-such-model", 10),
		});
		expect(code).toBe(4);
		expect(lines.at(-1)).toMatch(/the price table lacks no-such-model/);
	}, 60_000);
});

describe("--validate (0482)", () => {
	test("the hidden tests fail on the base commit and pass three times with the gold patch", () => {
		const good = localTask();
		const broken = { ...localTask(), instance_id: "acme__widget-8", test_command: "git --version" };
		const out = join(tempDir(), "run");
		const work = join(tempDir(), "w");
		const failed = runValidation([good, broken], tempDir(), out, () => {}, work);
		expect(failed).toEqual(["acme__widget-8"]);
		const [goodRow, brokenRow] = readJsonl(join(out, "validation.jsonl"));
		expect(goodRow).toMatchObject({ task: "acme__widget-7", null_failed: true, gold_passes: 3, error: null });
		expect(goodRow.seconds.gold).toHaveLength(3);
		expect(brokenRow).toMatchObject({ task: "acme__widget-8", null_failed: false, gold_passes: 3 });
		const logs = join(out, "validation", "acme__widget-7");
		expect(readFileSync(join(logs, "setup.log"), "utf8")).toContain("$ git --version");
		expect(readFileSync(join(logs, "null.log"), "utf8")).toMatch(/exit: 1\n/);
		for (const i of [1, 2, 3]) expect(readFileSync(join(logs, `gold-${i}.log`), "utf8")).toMatch(/exit: 0\n/);
		expect(existsSync(join(work))).toBe(true);
	}, 120_000);
});
