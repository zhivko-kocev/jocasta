import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BenchUsageError } from "../bench/lib/args.ts";
import type { TrialLedger } from "../src/shared/ledger.ts";
import type { EvalCase, ResultRow } from "./runner.ts";

/**
 * The task benchmark's tasks (0482): `D:/thesis/data/bench/tasks/tasks.jsonl` read and checked, the seeded trial
 * order and pilot pick, each task's fixture and hidden tests prepared into a cache, the task as the runner's
 * {@link EvalCase}, the leak scan, the infra classifier and the resume state. `evals/bench.ts` schedules the trials.
 */

export const TASK_LANGUAGES = ["Python", "TypeScript", "JavaScript", "Go"] as const;
export type TaskLanguage = (typeof TASK_LANGUAGES)[number];

/** One line of `tasks.jsonl`: the fields the benchmark uses (the file's README lists them all). */
export interface BenchTask {
	instance_id: string;
	set: "main" | "spare";
	/** `<owner>/<name>`. */
	repo: string;
	/** Where the base commit is fetched from: GitHub's URL, or a local repository's path. */
	repo_url: string;
	base_commit: string;
	problem_statement: string;
	gold_patch: string;
	test_patch: string;
	hidden_test_files: string[];
	setup_command: string;
	test_command: string;
	language: TaskLanguage;
	pr_url: string;
	issue_urls: string[];
	merge_commit: string;
}

const STRING_FIELDS = [
	"instance_id",
	"repo",
	"repo_url",
	"base_commit",
	"problem_statement",
	"gold_patch",
	"test_patch",
	"setup_command",
	"test_command",
	"pr_url",
	"merge_commit",
] as const;

/** Reads `tasks.jsonl`; a missing or malformed field, or an id twice, throws naming the line. */
export function loadTasks(file: string): BenchTask[] {
	if (!existsSync(file)) throw new BenchUsageError(`no tasks file at ${file}`);
	const tasks: BenchTask[] = [];
	readFileSync(file, "utf8")
		.split(/\r?\n/)
		.forEach((line, i) => {
			if (line.trim() === "") return;
			const where = `${file}:${i + 1}`;
			let value: Record<string, unknown>;
			try {
				value = JSON.parse(line) as Record<string, unknown>;
			} catch (error) {
				throw new BenchUsageError(`${where}: not JSON (${(error as Error).message})`);
			}
			for (const key of STRING_FIELDS)
				if (typeof value[key] !== "string" || value[key] === "")
					throw new BenchUsageError(`${where}: \`${key}\` must be a non-empty string`);
			for (const key of ["hidden_test_files", "issue_urls"] as const) {
				const list = value[key];
				if (!Array.isArray(list) || !list.every((item) => typeof item === "string" && item !== ""))
					throw new BenchUsageError(`${where}: \`${key}\` must be a list of strings`);
			}
			if (value.set !== "main" && value.set !== "spare")
				throw new BenchUsageError(`${where}: \`set\` must be main or spare`);
			if (!TASK_LANGUAGES.includes(value.language as TaskLanguage))
				throw new BenchUsageError(`${where}: \`language\` must be one of ${TASK_LANGUAGES.join(", ")}`);
			if (!/^[\w.-]+\/[\w.-]+$/.test(value.repo as string))
				throw new BenchUsageError(`${where}: \`repo\` must be <owner>/<name>`);
			if (!/^[0-9a-f]{40}$/.test(value.base_commit as string) || !/^[0-9a-f]{40}$/.test(value.merge_commit as string))
				throw new BenchUsageError(`${where}: \`base_commit\` and \`merge_commit\` must be full commit SHAs`);
			const task = value as unknown as BenchTask;
			if (tasks.some((other) => other.instance_id === task.instance_id))
				throw new BenchUsageError(`${where}: ${task.instance_id} is in the file twice`);
			tasks.push(task);
		});
	return tasks;
}

/** The prompt of every arm (0482): this line, a blank line, then the task's issue text verbatim. */
export const TASK_PROMPT_PREFIX = "Fix the issue below in the repository in the current folder.\n\n";

export const taskPrompt = (task: BenchTask): string => `${TASK_PROMPT_PREFIX}${task.problem_statement}`;

/** mulberry32: a small seeded PRNG, so a seed gives the same order on every machine. */
export function seededRandom(seed: number): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state + 0x6d2b79f5) >>> 0;
		let t = state;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

/** A Fisher–Yates shuffle of a copy of `items` with {@link seededRandom}. */
export function shuffle<T>(items: readonly T[], seed: number): T[] {
	const random = seededRandom(seed);
	const copy = [...items];
	for (let i = copy.length - 1; i > 0; i--) {
		const j = Math.floor(random() * (i + 1));
		[copy[i], copy[j]] = [copy[j] as T, copy[i] as T];
	}
	return copy;
}

/** The pilot's strata (0482: 25 trials = 5 tasks × 5 arms × 1 rep). */
export const PILOT_STRATA: readonly { languages: readonly TaskLanguage[]; count: number }[] = [
	{ languages: ["Python"], count: 2 },
	{ languages: ["TypeScript", "JavaScript"], count: 2 },
	{ languages: ["Go"], count: 1 },
];

/** The pilot's tasks: per stratum, the first ones of the stratum's tasks (in id order) shuffled with `seed`. */
export function pickPilot(tasks: readonly BenchTask[], seed: number): BenchTask[] {
	return PILOT_STRATA.flatMap(({ languages, count }) => {
		const stratum = tasks
			.filter((task) => languages.includes(task.language))
			.sort((a, b) => a.instance_id.localeCompare(b.instance_id));
		if (stratum.length < count)
			throw new BenchUsageError(
				`the pilot needs ${count} ${languages.join("/")} tasks; the selection has ${stratum.length}`,
			);
		return shuffle(stratum, seed).slice(0, count);
	});
}

/** One trial of the run: a task, an arm, a rep, and its place in the run's order. */
export interface Trial {
	task: string;
	arm: string;
	rep: number;
	/** 0-based position in the seeded order. */
	index: number;
}

export const trialKey = (trial: { task: string; arm: string; rep: number }): string =>
	`${trial.task}/${trial.arm}/${trial.rep}`;

/** Every task × arm × rep, shuffled with `seed` so arms interleave over the run's hours (0482, [S16]). */
export function planTrials(tasks: readonly BenchTask[], arms: readonly string[], reps: number, seed: number): Trial[] {
	const all = tasks.flatMap((task) =>
		arms.flatMap((arm) => Array.from({ length: reps }, (_, rep) => ({ task: task.instance_id, arm, rep }))),
	);
	return shuffle(all, seed).map((trial, index) => ({ ...trial, index }));
}

/** A trial runs once, and twice more at most after infra errors (0482). */
export const MAX_ATTEMPTS = 3;

/** A run folder's `results.jsonl` row: the runner's row plus the benchmark's fields. */
export interface BenchRow extends ResultRow {
	task: string;
	arm: string;
	language: TaskLanguage;
	/** 1 for the first try, 2–3 after infra errors. */
	attempt: number;
	trialIndex: number;
	/** The leak scan's matches in the transcript; empty when none. */
	leak: string[];
	/**
	 * The trial's ledger totals at the pinned price table (0538, 0546): tokens, the priced cost, cache read share, the
	 * waste counts and the peak context, over its saved sessions (`sessions`); its rows are in `ledger.jsonl`.
	 */
	ledger?: TrialLedger["totals"] & { sessions: number };
}

/** An `errors.jsonl` row: an infra error, not a task failure. */
export interface InfraRow {
	task: string;
	arm: string;
	rep: number;
	attempt: number;
	trialIndex: number;
	harness: string;
	kind: InfraKind;
	message: string;
	at: string;
}

export interface ResumeState {
	/** Keys of trials with a row in `results.jsonl`. */
	done: Set<string>;
	/** Infra errors so far per key, from `errors.jsonl`. */
	failures: Map<string, number>;
}

function readJsonl<T>(file: string): T[] {
	if (!existsSync(file)) return [];
	return readFileSync(file, "utf8")
		.split(/\r?\n/)
		.filter((line) => line.trim() !== "")
		.map((line) => JSON.parse(line) as T);
}

export function appendJsonl(file: string, row: unknown): void {
	appendFileSync(file, `${JSON.stringify(row)}\n`);
}

/** What a run folder already holds: done trials and each trial's infra errors. */
export function readResumeState(outDir: string): ResumeState {
	const done = new Set(
		readJsonl<{ task: string; arm: string; rep: number }>(join(outDir, "results.jsonl")).map(trialKey),
	);
	const failures = new Map<string, number>();
	for (const row of readJsonl<{ task: string; arm: string; rep: number }>(join(outDir, "errors.jsonl"))) {
		const key = trialKey(row);
		failures.set(key, (failures.get(key) ?? 0) + 1);
	}
	return { done, failures };
}

/** The trials still to run, in order; `exhausted` are those whose infra errors used up {@link MAX_ATTEMPTS}. */
export function pendingTrials(
	order: readonly Trial[],
	state: ResumeState,
	maxAttempts = MAX_ATTEMPTS,
): { pending: Trial[]; exhausted: Trial[] } {
	const pending: Trial[] = [];
	const exhausted: Trial[] = [];
	for (const trial of order) {
		const key = trialKey(trial);
		if (state.done.has(key)) continue;
		if ((state.failures.get(key) ?? 0) >= maxAttempts) exhausted.push(trial);
		else pending.push(trial);
	}
	return { pending, exhausted };
}

/** A task prepared in the cache: the base commit's tree, the hidden test files and both patches. */
export interface PreparedTask {
	dir: string;
	fixture: string;
	hidden: string;
	goldPatch: string;
	testPatch: string;
	/** Upstream's nearest tag at the base commit, which the trial repo's fixture commit carries; none when no tag is reachable. */
	tag?: string;
}

function git(cwd: string, args: string[], env?: Record<string, string | undefined>): string {
	const result = spawnSync("git", args, { cwd, env, encoding: "utf8", windowsHide: true, maxBuffer: 1 << 28 });
	if (result.status !== 0)
		throw new Error(`git ${args.join(" ")} failed in ${cwd}: ${result.stderr || result.error?.message}`);
	return result.stdout;
}

/** Git without line-ending conversion, so the cache holds the commit's bytes on Windows too; long paths allowed. */
const RAW = ["-c", "core.autocrlf=false", "-c", "core.eol=lf", "-c", "core.longpaths=true"];

/**
 * Prepares a task once into `<cache>/<instance_id>/` (0482): the base commit fetched alone into the shared bare repo
 * `<cache>/repos/<owner>__<name>.git` (`git fetch --depth 1 <repo_url> <base_commit>`; GitHub serves reachable SHAs);
 * its tree written to `fixture/` without history (through a throwaway index, not `git archive`, which drops
 * `export-ignore` files); `test.patch` applied in a scratch repo and each `hidden_test_files` entry copied to `hidden/`
 * with its path; `gold.patch` and `test.patch` saved beside them; upstream's nearest tag at the base commit found
 * ({@link nearestTag}). `prepared.json` holds the base commit, the patches' SHA-256 and the tag, so a second call with
 * the same task does nothing. A test patch that deletes a hidden file throws: grading only overlays files (tasks.jsonl
 * has no deletions).
 */
export function prepareTask(task: BenchTask, cacheDir: string): PreparedTask {
	const dir = join(cacheDir, task.instance_id);
	const prepared: PreparedTask = {
		dir,
		fixture: join(dir, "fixture"),
		hidden: join(dir, "hidden"),
		goldPatch: join(dir, "gold.patch"),
		testPatch: join(dir, "test.patch"),
	};
	const marker = join(dir, "prepared.json");
	const stamp = {
		base_commit: task.base_commit,
		patches_sha256: createHash("sha256").update(`${task.gold_patch}\0${task.test_patch}`).digest("hex"),
	};
	const done = readMarker(marker);
	// The tag depends on the base commit alone: changed patches keep it, with no fetch.
	const knownTag =
		done?.base_commit === stamp.base_commit && (typeof done.tag === "string" || done.tag === null)
			? done.tag
			: undefined;
	if (knownTag !== undefined && done?.patches_sha256 === stamp.patches_sha256)
		return knownTag === null ? prepared : { ...prepared, tag: knownTag };
	rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
	mkdirSync(dir, { recursive: true });

	const bare = join(cacheDir, "repos", `${task.repo.replace("/", "__")}.git`);
	if (!existsSync(bare)) {
		mkdirSync(bare, { recursive: true });
		git(bare, ["init", "-q", "--bare"]);
	}
	const has = spawnSync("git", ["cat-file", "-e", `${task.base_commit}^{commit}`], { cwd: bare, stdio: "ignore" });
	if (has.status !== 0)
		git(bare, ["fetch", "-q", "--depth", "1", task.repo_url, `${task.base_commit}:refs/tasks/${task.instance_id}`]);

	mkdirSync(prepared.fixture);
	const index = { ...process.env, GIT_INDEX_FILE: join(dir, "fixture.index") };
	git(bare, ["read-tree", task.base_commit], index);
	git(bare, [...RAW, "--work-tree", prepared.fixture, "checkout-index", "-a", "-f"], index);
	rmSync(join(dir, "fixture.index"), { force: true });

	writeFileSync(prepared.goldPatch, task.gold_patch);
	writeFileSync(prepared.testPatch, task.test_patch);
	const scratch = join(dir, "scratch");
	cpSync(prepared.fixture, scratch, { recursive: true });
	for (const args of [
		["init", "-q", "-b", "main"],
		["config", "core.autocrlf", "false"],
		["config", "user.name", "Jocasta Evals"],
		["config", "user.email", "evals@example.org"],
		["add", "-A"],
		["commit", "-q", "--allow-empty", "-m", "fixture"],
		["apply", "--whitespace=nowarn", prepared.testPatch],
	])
		git(scratch, args);
	mkdirSync(prepared.hidden);
	for (const file of task.hidden_test_files) {
		const from = join(scratch, file);
		if (!existsSync(from))
			throw new Error(`${task.instance_id}: the test patch deletes ${file}; grading can only add or replace files`);
		cpSync(from, join(prepared.hidden, file));
	}
	rmSync(scratch, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
	const tag = knownTag === undefined ? nearestTag(task, dir) : (knownTag ?? undefined);
	writeFileSync(marker, `${JSON.stringify({ ...stamp, tag: tag ?? null })}\n`);
	return tag === undefined ? prepared : { ...prepared, tag };
}

function readMarker(file: string): { base_commit?: unknown; patches_sha256?: unknown; tag?: unknown } | undefined {
	try {
		return JSON.parse(readFileSync(file, "utf8"));
	} catch {
		return undefined;
	}
}

/**
 * Upstream's nearest tag at the base commit (`git describe --tags --abbrev=0`), from a throwaway bare repo that fetches
 * only commits and tags (`--filter=tree:0`; 1–9 MB for the Python tasks). The trial repo's fixture commit carries it,
 * so tools that read the version from git (setuptools-scm, hatch-vcs) see upstream's version, as a full clone does,
 * not a tagless `0.1.dev1` (which fails pytest's dev extras and tox's provisioning tests).
 */
function nearestTag(task: BenchTask, dir: string): string | undefined {
	const bare = join(dir, "tags.git");
	mkdirSync(bare);
	try {
		git(bare, ["init", "-q", "--bare"]);
		git(bare, ["fetch", "-q", "--filter=tree:0", task.repo_url, task.base_commit, "+refs/tags/*:refs/tags/*"]);
		const described = spawnSync("git", ["describe", "--tags", "--abbrev=0", task.base_commit], {
			cwd: bare,
			encoding: "utf8",
			windowsHide: true,
		});
		return described.status === 0 && described.stdout.trim() !== "" ? described.stdout.trim() : undefined;
	} finally {
		rmSync(bare, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
	}
}

/**
 * A Python task's setup (0482): a venv at the trial root's `venv/` (made once: grading's setup, in `<root>/grade`, finds
 * it and re-points the editable install to the clean clone), pip upgraded, then the task's setup. cmd.exe resolves
 * `python` per command, so once the venv exists every later `python` is the venv's ({@link PYTHON_PATH_PREFIX}).
 */
export const PYTHON_SETUP_PREFIX =
	"(if not exist ..\\venv\\Scripts\\python.exe python -m venv ..\\venv) && python -m pip install -q --upgrade pip && ";
export const PYTHON_PATH_PREFIX = ["venv/Scripts"];

/**
 * The task as the runner's case: the fixed prompt, the task's setup (wrapped in a venv for Python), no visible checks
 * and the task's test command as the one hidden check, run on the clean clone with `hidden/` overlaid; the fixture
 * commit tagged with upstream's nearest tag. Node's and Go's setups run as they are: their package and module caches are
 * safe to share between trials.
 */
export function taskCase(task: BenchTask, prepared: PreparedTask): EvalCase {
	const python = task.language === "Python";
	return {
		id: task.instance_id,
		dir: prepared.dir,
		prompt: taskPrompt(task),
		tags: [task.language, task.set],
		checks: [],
		hiddenChecks: [{ command: task.test_command }],
		setup: python ? `${PYTHON_SETUP_PREFIX}${task.setup_command}` : task.setup_command,
		fixture: prepared.fixture,
		hidden: prepared.hidden,
		...(prepared.tag === undefined ? {} : { fixtureTag: prepared.tag }),
		...(python ? { pathPrefix: [...PYTHON_PATH_PREFIX] } : {}),
	};
}

/** The transcript without the prompt (it is echoed back), so the issue's own text can't match a scan. */
function withoutPrompt(transcript: string, prompt: string | undefined): string {
	if (prompt === undefined) return transcript;
	return transcript.split(JSON.stringify(prompt).slice(1, -1)).join("").split(prompt).join("");
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * The leak scan (0482): the fix's PR, the task's issues, the PR's path and the merge commit (as `/commit/<sha>` and
 * its first 10 hex) anywhere in the transcript besides the prompt. Returns the matched patterns; empty when none.
 */
export function scanLeaks(task: BenchTask, transcript: string, prompt?: string): string[] {
	const text = withoutPrompt(transcript, prompt);
	const number = /\/pull\/(\d+)/.exec(task.pr_url)?.[1];
	const patterns: { label: string; regex: RegExp }[] = [
		...[task.pr_url, ...task.issue_urls].map((url) => ({
			label: url,
			regex: new RegExp(`${escapeRegExp(url.replace(/\/+$/, ""))}(?![0-9])`, "i"),
		})),
		...(number === undefined
			? []
			: [
					{
						label: `github.com/${task.repo}/pull/${number}`,
						regex: new RegExp(`github\\.com/${escapeRegExp(task.repo)}/pull/${number}(?![0-9])`, "i"),
					},
				]),
		{ label: `/commit/${task.merge_commit}`, regex: new RegExp(`/commit/${task.merge_commit}`, "i") },
		{
			label: task.merge_commit.slice(0, 10),
			regex: new RegExp(`(?<![0-9a-f])${task.merge_commit.slice(0, 10)}`, "i"),
		},
	];
	return [...new Set(patterns.filter(({ regex }) => regex.test(text)).map(({ label }) => label))];
}

export type InfraKind = "setup" | "auth" | "rate-limit" | "no-response" | "error";

const AUTH =
	/\b40[13]\b|unauthori[sz]ed|forbidden|not logged in|not signed in|please (?:log|sign) ?in|\/login\b|oauth|refresh(?:ing)? (?:the )?token|token refresh|invalid[_ ]grant|authentication (?:failed|error)|invalid api key/i;
const RATE = /rate[ _-]?limit|\b429\b|too many requests|overloaded|\b529\b|usage limit|quota exceeded/i;

/**
 * Whether a trial failed for infrastructure, not the task (0482): retried later instead of graded. A thrown error is
 * `setup` (the task's setup failed) or `error`; a sign-in failure (401/403, "not logged in", OAuth or token refresh in
 * stderr or the transcript's end, the harness exiting non-zero) is `auth`; a rate-limit or overloaded error without a
 * tool call is `rate-limit`, and so is one in stderr or at the transcript's end, after any number of tool calls, when
 * an account was full (0543: `accountFull`, the pacer's last reading); a harness that ended before its first model
 * response is `no-response`. Null: the trial counts.
 */
export function classifyInfra(outcome: {
	error?: string;
	row?: ResultRow;
	stderr?: string;
	transcript?: string;
	prompt?: string;
	accountFull?: boolean;
}): { kind: InfraKind; message: string } | null {
	const stderr = outcome.stderr ?? "";
	const cut = (text: string) => (text.length > 4000 ? `…${text.slice(-4000)}` : text);
	if (outcome.error !== undefined)
		return { kind: /: setup `/.test(outcome.error) ? "setup" : "error", message: cut(outcome.error) };
	const row = outcome.row;
	if (row === undefined) return { kind: "error", message: "no result" };
	const transcript = withoutPrompt(outcome.transcript ?? "", outcome.prompt);
	const tail = transcript.trimEnd().split("\n").slice(-20).join("\n");
	const exitedOk = row.grade[`${row.harness} exited 0`] === 1;
	const detail = cut([stderr.trim(), tail.trim()].filter((part) => part !== "").join("\n"));
	if ((!exitedOk || row.turns === 0) && (AUTH.test(stderr) || AUTH.test(tail)))
		return { kind: "auth", message: detail };
	if (row.toolCalls === 0 && (RATE.test(stderr) || RATE.test(transcript)))
		return { kind: "rate-limit", message: detail };
	if (outcome.accountFull === true && (RATE.test(stderr) || RATE.test(tail)))
		return { kind: "rate-limit", message: detail };
	if (row.turns === 0) return { kind: "no-response", message: detail };
	return null;
}
