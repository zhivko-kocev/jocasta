import { spawn, spawnSync } from "node:child_process";
import {
	appendFileSync,
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { type Static, Type } from "typebox";
import { describe, parseYamlFile, YamlFileError } from "../src/shared/yaml.ts";
import { type LayerSetup, loadBenchLayer } from "./layer.ts";
import { type PlanReviewSummary, planReviewExtension, readPlanReviewLog } from "./plan-review.ts";
import { BENCH_PTY_SESSION, herdrTerminal, type TerminalRunner } from "./terminal.ts";
import { DEFAULT_VARIANT, deepMerge, type Harness, type Variant, variantArgs } from "./variants.ts";

/**
 * Jocasta's eval runner (1.10) on his daily omp (0386): each trial runs `omp -p <prompt> --mode json` headless in a
 * fresh temp fixture repo on its own branch (0165), with his `~/.omp/agent` logins (or a `--profile`'s, 0482 E8) and a
 * per-trial `--config` overlay that turns memory off and carries the variant's settings. A variant may run Claude
 * Code's `claude -p` or opencode's `opencode run` instead (E4, 0537), plan first (E3) and add follow-up turns (E2). It
 * grades the repo's end state (0164), then the case's hidden checks on a clean clone of the result (E1), and writes the
 * claude-api skill's layout: `results.jsonl`, `transcripts/<case>-<rep>.jsonl` and `report.html` (0165, 0166).
 */

/** The overlay every trial starts from: eval sessions never enter his memory. A variant's settings layer over it. */
export const BASE_OVERLAY: Readonly<Record<string, unknown>> = { memory: { backend: "off" } };

/**
 * Where A7's Anthropic API key waits (0537): the only API key of the run, given to opencode's process as
 * `ANTHROPIC_API_KEY` and removed from every other process a trial starts (and an `ANTHROPIC_API_KEY` of the bench's
 * own environment from every trial).
 */
export const A7_API_KEY_ENV = "JOCASTA_A7_ANTHROPIC_API_KEY";

/** An end-state check (0164): a command that exits 0, a file's content or absence, or whether git is clean. */
const CheckSchema = Type.Object(
	{
		command: Type.Optional(Type.String({ minLength: 1 })),
		file: Type.Optional(Type.String({ minLength: 1 })),
		contains: Type.Optional(Type.String()),
		absent: Type.Optional(Type.Literal(true)),
		git_clean: Type.Optional(Type.Boolean()),
	},
	{ additionalProperties: false },
);

/** `case.yaml`: the prompt, tags for splits and grouping, the end-state checks, and an optional setup command. */
export const CaseSchema = Type.Object(
	{
		prompt: Type.String({ minLength: 1 }),
		tags: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/** A shell command run in the trial repo after the fixture is copied, before the fixture commit (0375). */
		setup: Type.Optional(Type.String({ minLength: 1 })),
		checks: Type.Array(CheckSchema, { minItems: 1 }),
		/**
		 * Checks the agent never sees (0482 E1): run after it exits, on a clean clone of its result with `setup` run again
		 * and the case's `hidden/` folder copied in.
		 */
		hidden_checks: Type.Optional(Type.Array(CheckSchema, { minItems: 1 })),
	},
	{ additionalProperties: false },
);

export type Check = Static<typeof CheckSchema>;

export interface EvalCase {
	id: string;
	dir: string;
	prompt: string;
	tags: string[];
	checks: Check[];
	hiddenChecks: Check[];
	setup?: string;
	/** `fixture/` beside `case.yaml`: files committed into the trial's repo before the run. */
	fixture?: string;
	/** `hidden/` beside `case.yaml`: grader files copied in only for the hidden checks, never into the agent's checkout. */
	hidden?: string;
	/**
	 * A tag on the fixture commit (0482's task benchmark: upstream's nearest tag at the base commit), there before setup
	 * runs and moved onto the commit that folds setup's output in, so tools that read the version from git see upstream's.
	 */
	fixtureTag?: string;
	/**
	 * Folders relative to the trial's root, put first on PATH for the setup, the harness and the checks (0482's task
	 * benchmark: a Python task's `venv/Scripts`, so every `python` is the trial's own).
	 */
	pathPrefix?: string[];
}

export interface Usage {
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite: number;
	totalTokens: number;
	cost: number;
}

/** What a trial's transcript says about its model, usage, turns and tool calls. */
export interface HarnessResponse {
	model: string | null;
	usage: Usage;
	totalUsage: Usage;
	requests: number;
	turns: number;
	toolCalls: number;
}

/**
 * One `results.jsonl` row (0165, 0375, 0482). `grade` maps each check to 1 (passed) or 0; hidden checks are labelled
 * `hidden: …`. `usage` is the main session's, from its assistant `message_end` events; `totalUsage` and `requests` add
 * the subagents omp's `task` tool ran, from each call's result details (see {@link MEASUREMENT_NOTES}). Over a
 * trial's follow-up turns everything sums.
 */
export interface ResultRow {
	case: string;
	rep: number;
	/** The variant label (0375): names joined with "+", or "default". */
	variant: string;
	harness: Harness;
	/** `<harness> --version` of the run, e.g. `omp/18.12.6`. */
	harnessVersion: string;
	/** The trial's start, ISO: the analysis logs time of day (0482, [S16]). */
	startedAt: string;
	model: string | null;
	usage: Usage;
	totalUsage: Usage;
	requests: number;
	/** Assistant turns in the transcript, and the tool calls in them. */
	turns: number;
	toolCalls: number;
	/** The prompt and its follow-ups that ran. */
	prompts: number;
	/** From the first harness spawn to the last one's close, pacing pauses excluded. */
	wallMs: number;
	/** `git diff --numstat` against the fixture commit, untracked files included; binary files count 0. */
	linesAdded: number;
	linesRemoved: number;
	grade: Record<string, number>;
	/** Every hidden check passed on the clean clone; null when the case has none. */
	hiddenPassed: boolean | null;
	/** `[P0]`–`[P3]` findings in the first follow-up's reply (A3's review turn), P0/P1 blocking (0453); null without follow-ups. */
	findings: { total: number; blocking: number } | null;
	passed: boolean;
	/** A plan-review trial's review (0567): requested, the choice given and taken, and why omp was shut down. */
	planReview?: PlanReviewSummary;
}

/** What one trial needs: {@link runEvals} passes its run's options, the task benchmark (`evals/bench.ts`, 0482) its own. */
export interface TrialOptions {
	/** The run folder; transcripts go to its `transcripts/`. */
	outDir: string;
	/** Per-trial limit over all its turns (0318); a trial that reaches it is killed with its process tree and graded failed. */
	timeoutMs?: number;
	/** The command that starts the variant's harness; the runner appends the arguments of {@link trialArgs}. */
	launcher: string[];
	/** The run's variants, layered (0375); {@link DEFAULT_VARIANT} when unset. */
	variant?: Variant;
	/** omp's `--profile <name>` on every trial (0482 E8): its own logins, sessions and settings, apart from his daily omp. */
	profile?: string;
	/** The transcript's file name without `.jsonl`; `<case>-<rep>` when unset. */
	transcriptName?: string;
	/** Called after each harness run with its exit, so a caller can tell a sign-in failure from the task's (0482). */
	onHarnessRun?: (run: { status: number | null; stderr: string; timedOut: boolean }) => void;
	/** Where the trial's root goes; the OS temp folder when unset. A short one keeps deep trees under 260 characters. */
	workDir?: string;
	/**
	 * When set, the setup's and the hidden grading's commands are logged to `setup.log` and `grade.log` in it, and the
	 * per-file line counts behind `linesAdded`/`linesRemoved` to `lines.log` (0508).
	 */
	logDir?: string;
	/**
	 * When set, the harness's own records of the trial are saved there for the ledger (0538): omp's session files (omp
	 * runs with `--session-dir` in the trial root), Claude Code's transcripts and subagent transcripts, and opencode's
	 * `opencode export` of each session. Emptied first, so a retried attempt keeps only its own.
	 */
	sessionsDir?: string;
	/** The bench herdr session whose panes give a plan-review trial's first turn its terminal (0567). */
	ptySession?: string;
	/** Runs that turn instead of a herdr pane (tests; not passed to worker threads). */
	runInTerminal?: TerminalRunner;
}

export interface RunOptions extends TrialOptions {
	/** Case folders, read in order: the repo's cases and then his own (0166, 0375). */
	casesDir: string | readonly string[];
	/** The run folder, `<data>/evals/runs/<time>-<variant label>/` (0166, 0375). */
	outDir: string;
	reps: number;
	/** Called with each row as it is written. */
	onRow?: (row: ResultRow) => void;
	/** Awaited before each trial: the quota pacer (0482) holds the run here while a window is past its limit. */
	pace?: () => Promise<void>;
}

function validateChecks(
	checks: readonly Check[],
	key: "checks" | "hidden_checks",
	file: string,
	lineOf: (path: (string | number)[]) => number,
): void {
	checks.forEach((check, i) => {
		const kinds = [check.command, check.file, check.git_clean].filter((kind) => kind !== undefined).length;
		const fileOnly = check.file === undefined && (check.contains !== undefined || check.absent !== undefined);
		if (kinds !== 1 || fileOnly || (check.contains !== undefined && check.absent !== undefined)) {
			throw new YamlFileError(
				file,
				lineOf([key, i]),
				`${describe([key, i])} needs one of \`command\`, \`file\` (with \`contains\` or \`absent\`) or \`git_clean\``,
			);
		}
	});
}

export function loadCases(casesDir: string): EvalCase[] {
	if (!existsSync(casesDir)) return [];
	// Name order on every OS: Linux and macOS list a folder in no fixed order.
	return readdirSync(casesDir, { withFileTypes: true })
		.filter((entry) => entry.isDirectory() && existsSync(join(casesDir, entry.name, "case.yaml")))
		.sort((a, b) => a.name.localeCompare(b.name))
		.map((entry) => {
			const dir = join(casesDir, entry.name);
			const file = join(dir, "case.yaml");
			const doc = parseYamlFile(CaseSchema, readFileSync(file, "utf8"), file);
			const lineOf = (path: (string | number)[]) => doc.lineOf(path);
			validateChecks(doc.value.checks, "checks", file, lineOf);
			validateChecks(doc.value.hidden_checks ?? [], "hidden_checks", file, lineOf);
			const fixture = join(dir, "fixture");
			const hidden = join(dir, "hidden");
			if (existsSync(hidden) && doc.value.hidden_checks === undefined)
				throw new YamlFileError(file, 1, "the case has a hidden/ folder but no hidden_checks");
			return {
				id: entry.name,
				dir,
				prompt: doc.value.prompt,
				tags: doc.value.tags ?? [],
				checks: doc.value.checks,
				hiddenChecks: doc.value.hidden_checks ?? [],
				setup: doc.value.setup,
				fixture: existsSync(fixture) ? fixture : undefined,
				hidden: existsSync(hidden) ? hidden : undefined,
			};
		});
}

/** Every case in `dirs`, in order; a case id in two folders or no case at all throws. */
export function loadAllCases(dirs: readonly string[]): EvalCase[] {
	const cases = dirs.flatMap((dir) => loadCases(dir));
	const ids = cases.map((c) => c.id);
	const twice = ids.find((id, i) => ids.indexOf(id) !== i);
	if (twice) throw new Error(`eval case ${twice} is in more than one of ${dirs.join(", ")}`);
	if (cases.length === 0) throw new Error(`no eval cases in ${dirs.join(", ")}`);
	return cases;
}

/** Runs every case `reps` times. Real model calls: `bun run evals` gates it behind `JOCASTA_EVALS_REAL` and off CI (0169). */
export async function runEvals(options: RunOptions): Promise<ResultRow[]> {
	const cases = loadAllCases(typeof options.casesDir === "string" ? [options.casesDir] : options.casesDir);
	const harnessVersion = readVersion(options.launcher);
	mkdirSync(join(options.outDir, "transcripts"), { recursive: true });
	const resultsFile = join(options.outDir, "results.jsonl");
	writeFileSync(resultsFile, "");
	const rows: ResultRow[] = [];
	for (const evalCase of cases) {
		for (let rep = 0; rep < options.reps; rep++) {
			await options.pace?.();
			const row = await runTrial(evalCase, rep, harnessVersion, options);
			appendFileSync(resultsFile, `${JSON.stringify(row)}\n`);
			rows.push(row);
			options.onRow?.(row);
		}
	}
	writeFileSync(join(options.outDir, "report.html"), renderReport(rows, cases, { ...options, harnessVersion }));
	return rows;
}

/** `<launcher> --version`'s first line, once per run. */
export function readVersion(launcher: readonly string[]): string {
	const [command = "", ...args] = launcher;
	const result = spawnSync(command, [...args, "--version"], { encoding: "utf8", windowsHide: true, timeout: 30_000 });
	const version = result.stdout?.trim().split(/\r?\n/)[0];
	if (result.status !== 0 || !version)
		throw new Error(`${launcher.join(" ")} --version failed: ${result.stderr || result.error?.message}`);
	return version;
}

/**
 * One turn's harness arguments after the launcher. omp: `[--profile <p>] [-c] <variant flags> [--session-dir <dir>]
 * --config <overlay> -p <prompt> --mode json`; a follow-up continues the trial's session (`-c`) without plan-first.
 * An interactive omp turn (`extension` set: the plan-review extension, 0567) has no `-p`/`--mode`: `<flags>
 * [--session-dir <dir>] --config <overlay> -e <extension> <prompt>`. Claude Code: `[-c] <variant flags> -p <prompt>
 * --output-format stream-json --verbose` (stream-json needs `--verbose` in print mode). opencode: `run [--session <id>]
 * <variant flags> --format json <prompt>`; a follow-up names the first turn's session, since `--continue` takes the
 * machine's last session, which may be another trial's.
 */
export function trialArgs(
	variant: Variant,
	turn: {
		prompt: string;
		followUp: boolean;
		overlay: string;
		profile?: string;
		sessionDir?: string;
		session?: string;
		extension?: string;
	},
): string[] {
	const flags = variantArgs(turn.followUp ? { ...variant, planFirst: false } : variant);
	const resume = turn.followUp ? ["-c"] : [];
	if (variant.harness === "claude")
		return [...resume, ...flags, "-p", turn.prompt, "--output-format", "stream-json", "--verbose"];
	if (variant.harness === "opencode") {
		if (turn.followUp && turn.session === undefined)
			throw new Error("an opencode follow-up needs the first turn's session");
		return [
			"run",
			...(turn.followUp ? ["--session", turn.session as string] : []),
			...flags,
			"--format",
			"json",
			turn.prompt,
		];
	}
	const common = [
		...(turn.profile === undefined ? [] : ["--profile", turn.profile]),
		...resume,
		...flags,
		...(turn.sessionDir === undefined ? [] : ["--session-dir", turn.sessionDir]),
		"--config",
		turn.overlay,
	];
	if (turn.extension !== undefined) return [...common, "-e", turn.extension, turn.prompt];
	return [...common, "-p", turn.prompt, "--mode", "json"];
}

/** What an interactive plan-review turn adds to the overlay: plan mode at startup (`plan.defaultOnStartup`). */
export const PLAN_REVIEW_OVERLAY: Readonly<Record<string, unknown>> = {
	plan: { enabled: true, defaultOnStartup: true },
};

/**
 * One trial: a fresh temp root with the overlay and the fixture repo, the harness's turns, the end-state checks, the
 * line counts and the hidden checks. A failed setup or git step throws; the temp root is removed either way.
 */
export async function runTrial(
	evalCase: EvalCase,
	rep: number,
	harnessVersion: string,
	options: TrialOptions,
): Promise<ResultRow> {
	const variant = options.variant ?? DEFAULT_VARIANT;
	const root = makeTrialRoot(options.workDir);
	const env = trialEnv(evalCase, root, variant.env);
	const startedAt = new Date().toISOString();
	let layer: LayerSetup | undefined;
	try {
		// Outside the repo, so it never shows in a git_clean check or the line counts. JSON is YAML.
		const overlay = join(root, "eval-run.yml");
		const logFile = (phase: string) =>
			options.logDir === undefined ? undefined : join(options.logDir, `${phase}.log`);
		const { repo, fixtureCommit, branch } = makeTrialRepo(evalCase, rep, root, env, logFile("setup"));
		// 0567: plan first answered in omp's interactive plan review; the first turn runs in a terminal.
		const planReview = variant.harness === "omp" ? variant.planReview : undefined;
		if (variant.layer !== undefined) {
			// A6 (0543): Jocasta's layer build sets the trial up; without the build this throws before any model call.
			layer = await (await loadBenchLayer()).setUp({
				root,
				repo,
				baseCommit: fixtureCommit,
				prompt: evalCase.prompt,
				...(variant.appendSystemPrompt === undefined ? {} : { appendSystemPrompt: variant.appendSystemPrompt }),
				switches: variant.layer,
				env,
			});
		}
		const overlaySettings = [variant.settings ?? {}, planReview ? PLAN_REVIEW_OVERLAY : {}, layer?.settings ?? {}];
		writeFileSync(overlay, `${JSON.stringify(overlaySettings.reduce(deepMerge, { ...BASE_OVERLAY }), null, 2)}\n`);
		const planReviewLog = join(root, "plan-review.jsonl");
		const extension = join(root, "plan-review.js");
		if (planReview) writeFileSync(extension, planReviewExtension(planReview, planReviewLog));
		const [command = "", ...launcherArgs] = options.launcher;
		const prompts = [evalCase.prompt, ...variant.followups.map((text) => text.replaceAll("{base}", fixtureCommit))];
		// omp writes its session files into the trial root (the ledger's and a plan-review trial's record); opencode's
		// exports are needed for its usage either way.
		const sessionDir =
			(options.sessionsDir !== undefined || planReview) && variant.harness === "omp"
				? join(root, "sessions")
				: undefined;
		const harnessEnv =
			variant.harness === "opencode"
				? { ...env, ANTHROPIC_API_KEY: process.env[A7_API_KEY_ENV] }
				: { ...env, ...layer?.env };
		const outputs: string[] = [];
		let wallMs = 0;
		let exitedOk = true;
		let timedOut = false;
		for (const [i, prompt] of prompts.entries()) {
			const left = options.timeoutMs === undefined ? undefined : Math.max(1, options.timeoutMs - wallMs);
			const session = variant.harness === "opencode" ? opencodeSessions(outputs.join("\n"))[0] : undefined;
			const args = [
				...launcherArgs,
				...trialArgs(variant, {
					prompt,
					followUp: i > 0,
					overlay,
					profile: options.profile,
					sessionDir,
					session,
					...(planReview && i === 0 ? { extension } : {}),
				}),
				...(layer?.args ?? []),
			];
			let run: HarnessRun;
			if (planReview && i === 0) {
				const terminal = options.runInTerminal ?? herdrTerminal(options.ptySession ?? BENCH_PTY_SESSION);
				const ended = await terminal(command, args, repo, harnessEnv, left);
				const log = existsSync(planReviewLog) ? readFileSync(planReviewLog, "utf8") : "";
				// herdr reports no exit code: the turn counts as ended well when the extension shut omp down.
				const ok = !ended.timedOut && readPlanReviewLog(log, planReview).shutdown !== null;
				run = {
					status: ended.timedOut ? null : ok ? 0 : 1,
					stdout: "",
					stderr: ok ? "" : "omp ended without the plan-review extension's shutdown",
					timedOut: ended.timedOut,
					wallMs: ended.wallMs,
				};
			} else run = await runHarness(command, args, repo, harnessEnv, left);
			options.onHarnessRun?.({ status: run.status, stderr: run.stderr, timedOut: run.timedOut });
			outputs.push(run.stdout);
			wallMs += run.wallMs;
			if (run.timedOut) {
				timedOut = true;
				process.stderr.write(`${evalCase.id}-${rep}: killed after ${options.timeoutMs} ms\n`);
			} else if (run.status !== 0) {
				process.stderr.write(`${evalCase.id}-${rep}: ${variant.harness} exited ${run.status}\n${run.stderr}\n`);
			}
			if (run.status !== 0 || run.timedOut) {
				exitedOk = false;
				break;
			}
		}
		// Each turn's output ends its last line, so one turn's JSON never runs into the next. A plan-review trial's
		// first turn had a terminal, not JSON: its transcript is the session files omp wrote, every turn included.
		const transcript = planReview
			? ompSessionText(sessionDir as string)
			: outputs.map((output) => (output === "" || output.endsWith("\n") ? output : `${output}\n`)).join("");
		writeFileSync(
			join(options.outDir, "transcripts", `${options.transcriptName ?? `${evalCase.id}-${rep}`}.jsonl`),
			transcript,
		);
		const savedSessions =
			options.sessionsDir ?? (variant.harness === "opencode" ? join(root, "opencode-sessions") : undefined);
		if (savedSessions !== undefined)
			saveSessions(variant.harness, savedSessions, {
				transcript,
				sessionDir,
				launcher: options.launcher,
				env: harnessEnv,
				cwd: repo,
			});
		const grade: Record<string, number> = { [`${variant.harness} exited 0`]: exitedOk ? 1 : 0 };
		if (timedOut) grade.timeout = 0;
		for (const check of evalCase.checks) grade[checkLabel(check)] = gradeCheck(check, repo, env) ? 1 : 0;
		// After grading, so staging can't change a git_clean check.
		const lines = lineCounts(repo, fixtureCommit, logFile("lines"));
		let hiddenPassed: boolean | null = null;
		if (evalCase.hiddenChecks.length > 0) {
			const hidden = gradeHidden(evalCase, repo, branch, join(root, "grade"), env, logFile("grade"));
			Object.assign(grade, hidden);
			hiddenPassed = Object.values(hidden).every((score) => score === 1);
		}
		const response =
			variant.harness === "claude"
				? readClaudeResponse(transcript)
				: variant.harness === "opencode"
					? readOpencodeResponse(savedSessions as string)
					: planReview
						? readOmpSessionsResponse(sessionDir as string)
						: readResponse(transcript);
		const review = outputs[1];
		return {
			case: evalCase.id,
			rep,
			variant: variant.label,
			harness: variant.harness,
			harnessVersion,
			startedAt,
			...response,
			prompts: outputs.length,
			wallMs,
			...lines,
			grade,
			hiddenPassed,
			findings:
				prompts.length > 1 ? countFindings(review === undefined ? "" : assistantText(review, variant.harness)) : null,
			passed: Object.values(grade).every((score) => score === 1),
			...(planReview
				? {
						planReview: readPlanReviewLog(
							existsSync(planReviewLog) ? readFileSync(planReviewLog, "utf8") : "",
							planReview,
						),
					}
				: {}),
		};
	} finally {
		await layer?.close();
		removeTrialDir(root);
	}
}

/**
 * A trial's root: a short folder (`t` and 6 characters) in `workDir` (default the OS temp folder), so deep dependency
 * trees stay under Windows' 260 characters. Nothing in it names the case: a benchmark task's id ends in its fix's PR
 * number, and agents list the root (0508); the trial's `setup.log` names the root.
 */
export function makeTrialRoot(workDir: string | undefined): string {
	const parent = workDir ?? tmpdir();
	mkdirSync(parent, { recursive: true });
	return mkdtempSync(join(parent, "t"));
}

/**
 * The trial's environment: `extra` (a variant's env) over this process's, with the case's {@link EvalCase.pathPrefix}
 * folders under `root` first on PATH (Windows names it `Path`).
 */
export function trialEnv(
	evalCase: EvalCase,
	root: string,
	extra: Record<string, string> = {},
): Record<string, string | undefined> {
	const env: Record<string, string | undefined> = { ...process.env, ...extra };
	delete env[A7_API_KEY_ENV];
	delete env.ANTHROPIC_API_KEY;
	if (!evalCase.pathPrefix?.length) return env;
	const key = Object.keys(env).find((name) => name.toUpperCase() === "PATH") ?? "PATH";
	env[key] = [...evalCase.pathPrefix.map((dir) => join(root, dir)), env[key]]
		.filter((part) => part !== undefined && part !== "")
		.join(delimiter);
	return env;
}

/**
 * `<root>/repo`: the fixture copied in and committed as `fixture` on `main`, every file of it, those its own ignore rules
 * match too (upstream tracks them: cli/cli's `internal/gh`, pipx's test package lists), and tagged with `fixtureTag`;
 * the setup run in that checkout (tools that read the version from git, such as setuptools-scm and hatch-vcs, need a
 * commit, and upstream's tag to report upstream's version), and what setup wrote folded
 * into the same commit, so it never counts as the trial's change; then checked out on the trial's own branch. Hooks that
 * setup installs (husky, yorkie, simple-git-hooks) stay off: an arm that commits meets no lint or commit-message check
 * that an arm that doesn't never meets. Git converts no line endings there (`core.autocrlf` off), so its files and its
 * clean clone keep the fixture's bytes on Windows too.
 */
export function makeTrialRepo(
	evalCase: EvalCase,
	rep: number,
	root: string,
	env: Record<string, string | undefined>,
	logFile?: string,
): { repo: string; fixtureCommit: string; branch: string } {
	const repo = join(root, "repo");
	mkdirSync(repo);
	if (evalCase.fixture) cpSync(evalCase.fixture, repo, { recursive: true });
	for (const args of [
		["init", "-q", "-b", "main"],
		["config", "user.name", "Jocasta Evals"],
		["config", "user.email", "evals@example.org"],
		["config", "core.autocrlf", "false"],
		["add", "-A", "--force"],
		["commit", "-q", "--allow-empty", "-m", "fixture"],
	]) {
		git(repo, args);
	}
	if (evalCase.fixtureTag) git(repo, ["tag", evalCase.fixtureTag]);
	runSetup(evalCase, repo, env, logFile);
	// A folder that never exists, so no hook runs, wherever setup put them (husky moves `core.hooksPath` itself).
	git(repo, ["config", "core.hooksPath", join(root, "no-hooks")]);
	git(repo, ["add", "-A"]);
	git(repo, ["commit", "-q", "--amend", "--allow-empty", "--no-edit"]);
	// The amend made a new commit; grading's clean clone takes the tag along with the branch.
	if (evalCase.fixtureTag) git(repo, ["tag", "-f", evalCase.fixtureTag]);
	const fixtureCommit = git(repo, ["rev-parse", "HEAD"]).trim();
	// Not the case id, which for a benchmark task ends in its fix's PR number: git output shows the branch (0508).
	const branch = `eval/trial-${rep}`;
	git(repo, ["checkout", "-q", "-b", branch]);
	return { repo, fixtureCommit, branch };
}

/**
 * Runs a shell command; with `logFile`, appends its command, folder, exit code, seconds, stdout and stderr there, so a
 * failing setup or test can be diagnosed afterwards (0482).
 */
function runCommand(
	command: string,
	cwd: string,
	env: Record<string, string | undefined> | undefined,
	logFile: string | undefined,
): { status: number | null; stdout: string; stderr: string } {
	const started = performance.now();
	const result = spawnSync(command, { cwd, env, shell: true, encoding: "utf8", windowsHide: true, maxBuffer: 1 << 28 });
	const stderr = result.stderr || result.error?.message || "";
	if (logFile !== undefined) {
		mkdirSync(dirname(logFile), { recursive: true });
		const seconds = ((performance.now() - started) / 1000).toFixed(1);
		appendFileSync(
			logFile,
			`=== ${new Date().toISOString()}\n$ ${command}\ncwd: ${cwd}\nexit: ${result.status}\nseconds: ${seconds}\n--- stdout\n${result.stdout ?? ""}\n--- stderr\n${stderr}\n\n`,
		);
	}
	return { status: result.status, stdout: result.stdout ?? "", stderr };
}

function runSetup(
	evalCase: EvalCase,
	cwd: string,
	env: Record<string, string | undefined>,
	logFile: string | undefined,
): void {
	if (evalCase.setup === undefined) return;
	const setup = runCommand(evalCase.setup, cwd, env, logFile);
	if (setup.status !== 0) throw new Error(`${evalCase.id}: setup \`${evalCase.setup}\` failed: ${setup.stderr}`);
}

/**
 * The hidden checks (0482 E1) on a clean clone of the trial's result: its changes (staged by the caller) committed on
 * the trial branch, cloned to `gradeDir` (no ignored or build files the agent left), the case's setup run again,
 * `hidden/` copied in. With `logFile`, the setup's and the checks' commands are logged there.
 */
export function gradeHidden(
	evalCase: EvalCase,
	repo: string,
	branch: string,
	gradeDir: string,
	env: Record<string, string | undefined>,
	logFile?: string,
): Record<string, number> {
	git(repo, ["commit", "-q", "--allow-empty", "--no-verify", "-m", "trial result"]);
	git(repo, ["-c", "core.autocrlf=false", "clone", "-q", "--branch", branch, repo, gradeDir]);
	runSetup(evalCase, gradeDir, env, logFile);
	if (evalCase.hidden) cpSync(evalCase.hidden, gradeDir, { recursive: true });
	return Object.fromEntries(
		evalCase.hiddenChecks.map((check) => [
			`hidden: ${checkLabel(check)}`,
			gradeCheck(check, gradeDir, env, logFile) ? 1 : 0,
		]),
	);
}

/** `[P0]`…`[P3]` markers in a review's reply (0453's priorities); P0 and P1 are blocking. */
export function countFindings(text: string): { total: number; blocking: number } {
	const priorities = [...text.matchAll(/\[P([0-3])\]/g)].map((match) => Number(match[1]));
	return { total: priorities.length, blocking: priorities.filter((p) => p <= 1).length };
}

/**
 * Removes a trial's temp dir. On Windows a process omp started can hold a file in it for a moment after omp exits
 * (EBUSY), so it retries for up to 10 s, then leaves the dir with a warning rather than lose the run.
 */
function removeTrialDir(root: string): void {
	for (let attempt = 0; ; attempt++) {
		try {
			rmSync(root, { recursive: true, force: true });
			return;
		} catch (error) {
			const code = (error as NodeJS.ErrnoException).code;
			if (code !== "EBUSY" && code !== "EPERM" && code !== "ENOTEMPTY") throw error;
			if (attempt === 20) {
				process.stderr.write(`could not remove ${root} (${code}); delete it by hand\n`);
				return;
			}
			Bun.sleepSync(500);
		}
	}
}

/**
 * Lines added and removed in `checkout` since the fixture commit, committed, staged, unstaged and untracked alike;
 * with `logFile`, the per-file `git diff --numstat` they sum goes there (0508).
 */
function lineCounts(
	checkout: string,
	fixtureCommit: string,
	logFile?: string,
): { linesAdded: number; linesRemoved: number } {
	git(checkout, ["add", "-A"]);
	const numstat = git(checkout, ["diff", "--cached", "--numstat", fixtureCommit]);
	if (logFile) {
		mkdirSync(dirname(logFile), { recursive: true });
		writeFileSync(logFile, numstat);
	}
	let linesAdded = 0;
	let linesRemoved = 0;
	for (const line of numstat.split("\n")) {
		// A binary file's counts are "-", which count 0.
		const [added, removed] = line.split("\t");
		linesAdded += Number(added) || 0;
		linesRemoved += Number(removed) || 0;
	}
	return { linesAdded, linesRemoved };
}

const emptyUsage = (): Usage => ({ input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 });

/** omp's usage shape: Jocasta's, with `cost` an object whose `total` is the dollars. */
type OmpUsage = Omit<Usage, "cost"> & { cost?: { total?: number } };

function addUsage(target: Usage, used: Partial<OmpUsage> | undefined): void {
	if (!used) return;
	target.input += used.input ?? 0;
	target.output += used.output ?? 0;
	target.cacheRead += used.cacheRead ?? 0;
	target.cacheWrite += used.cacheWrite ?? 0;
	target.totalTokens += used.totalTokens ?? 0;
	target.cost += used.cost?.total ?? 0;
}

interface HarnessRun {
	status: number | null;
	stdout: string;
	stderr: string;
	timedOut: boolean;
	/** From spawn to close. */
	wallMs: number;
}

/** Runs one trial's harness; past `timeoutMs` the whole process tree is killed (0318). */
function runHarness(
	command: string,
	args: string[],
	cwd: string,
	env: Record<string, string | undefined>,
	timeoutMs: number | undefined,
): Promise<HarnessRun> {
	const { promise, resolve } = Promise.withResolvers<HarnessRun>();
	const started = performance.now();
	// On POSIX the child leads its own process group, so the group can be killed as one tree. Stdin is closed:
	// omp's print mode reads piped stdin to its end.
	const child = spawn(command, args, {
		cwd,
		env,
		detached: process.platform !== "win32",
		stdio: ["ignore", "pipe", "pipe"],
		windowsHide: true,
	});
	const stdout: Buffer[] = [];
	const stderr: Buffer[] = [];
	child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
	child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
	let timedOut = false;
	const timer =
		timeoutMs === undefined
			? undefined
			: setTimeout(() => {
					timedOut = true;
					if (child.pid !== undefined) killTree(child.pid);
				}, timeoutMs);
	let settled = false;
	const finish = (status: number | null, error?: Error) => {
		if (settled) return;
		settled = true;
		clearTimeout(timer);
		resolve({
			status,
			stdout: Buffer.concat(stdout).toString("utf8"),
			stderr: Buffer.concat(stderr).toString("utf8") || (error?.message ?? ""),
			timedOut,
			wallMs: Math.round(performance.now() - started),
		});
	};
	child.once("error", (error) => finish(null, error));
	child.once("close", (status) => finish(status));
	return promise;
}

function killTree(pid: number): void {
	if (process.platform === "win32") {
		spawnSync(
			join(process.env.SystemRoot ?? "C:\\Windows", "System32", "taskkill.exe"),
			["/F", "/T", "/PID", String(pid)],
			{
				stdio: "ignore",
				windowsHide: true,
			},
		);
		return;
	}
	try {
		process.kill(-pid, "SIGKILL");
	} catch {
		// The group is already gone.
	}
}

function git(cwd: string, args: string[]): string {
	const result = spawnSync("git", args, { cwd, encoding: "utf8" });
	if (result.status !== 0)
		throw new Error(`git ${args.join(" ")} failed in ${cwd}: ${result.stderr || result.error?.message}`);
	return result.stdout;
}

export function checkLabel(check: Check): string {
	if (check.command !== undefined) return `command: ${check.command}`;
	if (check.git_clean !== undefined) return `git_clean: ${check.git_clean}`;
	if (check.absent) return `file: ${check.file} absent`;
	if (check.contains !== undefined) return `file: ${check.file} contains ${JSON.stringify(check.contains)}`;
	return `file: ${check.file}`;
}

/**
 * Grades one end-state check against the trial repo after the run (0164); a command runs with `env` (default: this
 * process's) and, with `logFile`, is logged there.
 */
export function gradeCheck(
	check: Check,
	repo: string,
	env?: Record<string, string | undefined>,
	logFile?: string,
): boolean {
	if (check.command !== undefined) {
		return runCommand(check.command, repo, env, logFile).status === 0;
	}
	if (check.git_clean !== undefined) {
		return (git(repo, ["status", "--porcelain"]).trim() === "") === check.git_clean;
	}
	const path = join(repo, check.file ?? "");
	if (check.absent) return !existsSync(path);
	if (!existsSync(path)) return false;
	return check.contains === undefined || readFileSync(path, "utf8").includes(check.contains);
}

/**
 * From omp's `--mode json` events: the model as the responses name it, the main session's usage summed over its
 * assistant `message_end` events, those turns and their `toolCall` blocks, and `totalUsage`/`requests` with the
 * foreground subagents of each `task` call added from its `tool_execution_end` details (`usage`, `results[].requests`).
 */
export function readResponse(transcript: string): HarnessResponse {
	const usage = emptyUsage();
	const subagentUsage = emptyUsage();
	let subagentRequests = 0;
	let model: string | null = null;
	let turns = 0;
	let toolCalls = 0;
	for (const line of transcript.split("\n")) {
		if (!line.startsWith("{")) continue;
		const event = JSON.parse(line) as {
			type?: string;
			toolName?: string;
			result?: { details?: { usage?: OmpUsage; results?: { requests?: number }[] } };
			message?: { role?: string; model?: string; content?: unknown; usage?: OmpUsage };
		};
		if (event.type === "tool_execution_end" && event.toolName === "task") {
			const details = event.result?.details;
			addUsage(subagentUsage, details?.usage);
			for (const result of details?.results ?? []) subagentRequests += result.requests ?? 0;
			continue;
		}
		if (event.type !== "message_end" || event.message?.role !== "assistant") continue;
		turns++;
		const { content } = event.message;
		if (Array.isArray(content))
			toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "toolCall").length;
		model = event.message.model ?? model;
		addUsage(usage, event.message.usage);
	}
	const totalUsage = { ...usage };
	for (const key of Object.keys(totalUsage) as (keyof Usage)[]) totalUsage[key] += subagentUsage[key];
	return { model, usage, totalUsage, requests: turns + subagentRequests, turns, toolCalls };
}

/** The `.jsonl` files under omp's session folder, the folder's own (the trial's sessions) first, then subagents'. */
function ompSessionFiles(dir: string): { path: string; main: boolean }[] {
	if (!existsSync(dir)) return [];
	return readdirSync(dir, { recursive: true, encoding: "utf8" })
		.filter((name) => name.endsWith(".jsonl"))
		.map((name) => ({ path: join(dir, name), main: !/[\\/]/.test(name) }))
		.sort((a, b) => Number(b.main) - Number(a.main) || a.path.localeCompare(b.path));
}

/** The trial's own omp session files' text, joined: a plan-review trial's transcript (0567). */
export function ompSessionText(dir: string): string {
	return ompSessionFiles(dir)
		.filter((file) => file.main)
		.map((file) => {
			const text = readFileSync(file.path, "utf8");
			return text === "" || text.endsWith("\n") ? text : `${text}\n`;
		})
		.join("");
}

/**
 * From omp's session files in `dir` (a plan-review trial, whose first turn had a terminal, not JSON): as
 * {@link readResponse} counts the JSON stream, the trial's own assistant messages give the model, `usage`, the turns
 * and their `toolCall` blocks; the subagents' files beside them add to `totalUsage` and `requests`.
 */
export function readOmpSessionsResponse(dir: string): HarnessResponse {
	const usage = emptyUsage();
	const subagentUsage = emptyUsage();
	let subagentRequests = 0;
	let model: string | null = null;
	let turns = 0;
	let toolCalls = 0;
	for (const file of ompSessionFiles(dir)) {
		for (const line of readFileSync(file.path, "utf8").split("\n")) {
			if (!line.startsWith("{")) continue;
			const entry = JSON.parse(line) as {
				type?: string;
				message?: { role?: string; model?: string; content?: unknown; usage?: OmpUsage };
			};
			if (entry.type !== "message" || entry.message?.role !== "assistant") continue;
			if (!file.main) {
				subagentRequests++;
				addUsage(subagentUsage, entry.message.usage);
				continue;
			}
			turns++;
			const { content } = entry.message;
			if (Array.isArray(content))
				toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "toolCall").length;
			model = entry.message.model ?? model;
			addUsage(usage, entry.message.usage);
		}
	}
	const totalUsage = { ...usage };
	for (const key of Object.keys(totalUsage) as (keyof Usage)[]) totalUsage[key] += subagentUsage[key];
	return { model, usage, totalUsage, requests: turns + subagentRequests, turns, toolCalls };
}

/** Claude Code's usage shape in its stream-json `result` events. */
interface ClaudeUsage {
	input_tokens?: number;
	output_tokens?: number;
	cache_read_input_tokens?: number;
	cache_creation_input_tokens?: number;
}

/**
 * From Claude Code's `-p --output-format stream-json --verbose` events (one `result` event per invocation, with
 * `total_cost_usd` and `usage`; `assistant` events carry `message.model` and `tool_use` blocks): usage summed over the
 * results, turns from `num_turns`, tool calls from the `tool_use` blocks. Claude Code's subagents are inside its own
 * totals, so `totalUsage` equals `usage`.
 */
export function readClaudeResponse(transcript: string): HarnessResponse {
	const usage = emptyUsage();
	let model: string | null = null;
	let turns = 0;
	let toolCalls = 0;
	for (const line of transcript.split("\n")) {
		if (!line.startsWith("{")) continue;
		const event = JSON.parse(line) as {
			type?: string;
			model?: string;
			num_turns?: number;
			total_cost_usd?: number;
			usage?: ClaudeUsage;
			message?: { model?: string; content?: unknown };
		};
		if (event.type === "assistant") {
			model = event.message?.model ?? model;
			const content = event.message?.content;
			if (Array.isArray(content))
				toolCalls += content.filter((block) => (block as { type?: unknown } | null)?.type === "tool_use").length;
		} else if (event.type === "result") {
			const used = event.usage ?? {};
			usage.input += used.input_tokens ?? 0;
			usage.output += used.output_tokens ?? 0;
			usage.cacheRead += used.cache_read_input_tokens ?? 0;
			usage.cacheWrite += used.cache_creation_input_tokens ?? 0;
			usage.cost += event.total_cost_usd ?? 0;
			turns += event.num_turns ?? 0;
		}
	}
	usage.totalTokens = usage.input + usage.output + usage.cacheRead + usage.cacheWrite;
	return { model, usage, totalUsage: { ...usage }, requests: turns, turns, toolCalls };
}

/**
 * The assistant's text in one turn's transcript, joined by newlines: omp's assistant `message_end` text blocks, Claude
 * Code's `assistant` text blocks, opencode's `text` parts.
 */
export function assistantText(transcript: string, harness: Harness): string {
	const texts: string[] = [];
	for (const line of transcript.split("\n")) {
		if (!line.startsWith("{")) continue;
		const event = JSON.parse(line) as {
			type?: string;
			message?: { role?: string; content?: unknown };
			part?: { type?: unknown; text?: unknown };
		};
		if (harness === "opencode") {
			if (event.part?.type === "text" && typeof event.part.text === "string") texts.push(event.part.text);
			continue;
		}
		const assistant =
			harness === "claude"
				? event.type === "assistant"
				: event.type === "message_end" && event.message?.role === "assistant";
		if (!assistant || !Array.isArray(event.message?.content)) continue;
		for (const block of event.message.content as { type?: unknown; text?: unknown }[])
			if (block?.type === "text" && typeof block.text === "string") texts.push(block.text);
	}
	return texts.join("\n");
}

/** The opencode sessions a `--format json` transcript names (`sessionID`), in first-seen order. */
export function opencodeSessions(transcript: string): string[] {
	return [...new Set([...transcript.matchAll(/"sessionID"\s*:\s*"(ses_[A-Za-z0-9]+)"/g)].map((m) => m[1] as string))];
}

/** The Claude Code sessions a stream-json transcript names (`session_id`), in first-seen order. */
export function claudeSessions(transcript: string): string[] {
	return [...new Set([...transcript.matchAll(/"session_id"\s*:\s*"([0-9a-f-]{36})"/g)].map((m) => m[1] as string))];
}

/**
 * Saves the trial's records into `dir` (emptied first) for the ledger (0538): omp's `--session-dir` folder as it is;
 * Claude Code's `<session>.jsonl` and `<session>/subagents/*.jsonl` from `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*`;
 * for opencode, `opencode export <session>` of each session into `<session>.json`. A session that can't be found or
 * exported is reported on stderr; the ledger then lacks it.
 */
export function saveSessions(
	harness: Harness,
	dir: string,
	trial: {
		transcript: string;
		sessionDir?: string;
		launcher: readonly string[];
		env: Record<string, string | undefined>;
		cwd: string;
	},
): void {
	rmSync(dir, { recursive: true, force: true });
	mkdirSync(dir, { recursive: true });
	if (harness === "omp") {
		if (trial.sessionDir !== undefined && existsSync(trial.sessionDir))
			cpSync(trial.sessionDir, dir, { recursive: true });
		return;
	}
	if (harness === "claude") {
		const projects = join(trial.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude"), "projects");
		const folders = existsSync(projects) ? readdirSync(projects).map((name) => join(projects, name)) : [];
		for (const id of claudeSessions(trial.transcript)) {
			const folder = folders.find((path) => existsSync(join(path, `${id}.jsonl`)));
			if (folder === undefined) {
				process.stderr.write(`no Claude Code transcript for session ${id} under ${projects}\n`);
				continue;
			}
			cpSync(join(folder, `${id}.jsonl`), join(dir, `${id}.jsonl`));
			const subagents = join(folder, id, "subagents");
			if (existsSync(subagents)) cpSync(subagents, join(dir, id, "subagents"), { recursive: true });
		}
		return;
	}
	const [command = "", ...launcherArgs] = trial.launcher;
	for (const id of opencodeSessions(trial.transcript)) {
		const exported = spawnSync(command, [...launcherArgs, "export", id], {
			cwd: trial.cwd,
			env: trial.env,
			encoding: "utf8",
			windowsHide: true,
			maxBuffer: 256 * 1024 * 1024,
			timeout: 120_000,
		});
		if (exported.status !== 0 || !exported.stdout.trimStart().startsWith("{")) {
			process.stderr.write(`opencode export ${id} failed: ${exported.stderr || exported.error?.message}\n`);
			continue;
		}
		writeFileSync(join(dir, `${id}.json`), exported.stdout);
	}
}

/**
 * From opencode's exports in `dir` (`opencode export` JSON, one per session): the model, each `step-finish` part's
 * tokens and cost summed (reasoning counted as output), assistant messages as turns and steps as requests, `tool`
 * parts as tool calls. The cost is opencode's own: on A7's API key, the bill.
 */
export function readOpencodeResponse(dir: string): HarnessResponse {
	const usage = emptyUsage();
	let model: string | null = null;
	let turns = 0;
	let requests = 0;
	let toolCalls = 0;
	const files = existsSync(dir) ? readdirSync(dir).filter((name) => name.endsWith(".json")) : [];
	for (const name of files) {
		const exported = JSON.parse(readFileSync(join(dir, name), "utf8")) as {
			messages?: { info?: { role?: string; modelID?: string }; parts?: Record<string, unknown>[] }[];
		};
		for (const message of exported.messages ?? []) {
			if (message.info?.role !== "assistant") continue;
			turns++;
			model = message.info.modelID ?? model;
			for (const part of message.parts ?? []) {
				if (part.type === "tool") toolCalls++;
				if (part.type !== "step-finish") continue;
				requests++;
				const tokens = (part.tokens ?? {}) as {
					input?: number;
					output?: number;
					reasoning?: number;
					cache?: { read?: number; write?: number };
				};
				usage.input += tokens.input ?? 0;
				usage.output += (tokens.output ?? 0) + (tokens.reasoning ?? 0);
				usage.cacheRead += tokens.cache?.read ?? 0;
				usage.cacheWrite += tokens.cache?.write ?? 0;
				usage.cost += typeof part.cost === "number" ? part.cost : 0;
			}
		}
	}
	usage.totalTokens = usage.input + usage.output + usage.cacheRead + usage.cacheWrite;
	return { model, usage, totalUsage: { ...usage }, requests, turns, toolCalls };
}

/** What the numbers leave out (0375); the report and `compare.md` both say so. */
export const MEASUREMENT_NOTES = [
	"Total tokens and requests are omp's JSON stream: the main session's assistant turns plus the subagents each foreground `task` call ran, from its result details. Background task jobs, subagents' own subagents, and model calls that are not session turns (compaction summaries, titles, memory) are not counted.",
	"A variant's settings reach omp through the trial's `--config` overlay, layered over `memory.backend: off`; trials use his `~/.omp/agent` logins and settings otherwise, or the named `--profile`'s.",
	"Claude Code trials read its stream-json `result` events: cost and tokens as Claude Code reports them, subagents included, turns from `num_turns`.",
	"opencode trials read `opencode export` of each session: tokens and cost as opencode reports them, reasoning counted as output.",
	"The task benchmark also prices every arm's saved records with one pinned price table (`bench/prices/`, 0538): its `ledger` field and `ledger.jsonl`.",
];

const escapeHtml = (text: string) =>
	text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** A static page: the pass rate with its noise floor (about 1/sqrt(trials), 0079) and one row per trial. */
export function renderReport(
	rows: ResultRow[],
	cases: EvalCase[],
	options: Pick<RunOptions, "reps" | "variant"> & { harnessVersion: string },
): string {
	const variant = options.variant ?? DEFAULT_VARIANT;
	const passed = rows.filter((row) => row.passed).length;
	const rate = rows.length === 0 ? 0 : (100 * passed) / rows.length;
	const noise = rows.length === 0 ? 0 : 100 / Math.sqrt(rows.length);
	const tags = new Map(cases.map((c) => [c.id, c.tags.join(", ")]));
	const body = rows
		.map((row) => {
			const failed = Object.entries(row.grade)
				.filter(([, score]) => score !== 1)
				.map(([label]) => label)
				.join("; ");
			const transcript = `transcripts/${row.case}-${row.rep}.jsonl`;
			const cells = [
				escapeHtml(row.case),
				escapeHtml(tags.get(row.case) ?? ""),
				row.rep,
				escapeHtml(row.model ?? ""),
				row.usage.input,
				row.usage.output,
				row.usage.cacheRead,
				row.totalUsage.totalTokens,
				row.requests,
				row.turns,
				row.toolCalls,
				(row.wallMs / 1000).toFixed(1),
				`+${row.linesAdded} −${row.linesRemoved}`,
				row.passed ? "pass" : "fail",
				escapeHtml(failed),
				`<a href="${escapeHtml(transcript)}">${escapeHtml(transcript)}</a>`,
			];
			return `<tr class="${row.passed ? "pass" : "fail"}">${cells.map((cell) => `<td>${cell}</td>`).join("")}</tr>`;
		})
		.join("\n");
	return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Jocasta evals</title>
<style>
body { font-family: system-ui, sans-serif; margin: 2rem; }
table { border-collapse: collapse; }
th, td { border: 1px solid #ccc; padding: 0.25rem 0.5rem; text-align: left; }
tr.fail td { background: #fde8e8; }
</style>
</head>
<body>
<h1>Jocasta evals</h1>
<p>${escapeHtml(options.harnessVersion)}, variant ${escapeHtml(variant.label)}: ${cases.length} cases × ${options.reps} reps. Passed ${passed} of ${rows.length} trials: ${rate.toFixed(1)}% ± ${noise.toFixed(1)} points.</p>
<table>
<thead><tr><th>Case</th><th>Tags</th><th>Rep</th><th>Model</th><th>Input</th><th>Output</th><th>Cache read</th><th>Total tokens</th><th>Requests</th><th>Turns</th><th>Tool calls</th><th>Wall (s)</th><th>Lines</th><th>Result</th><th>Failed checks</th><th>Transcript</th></tr></thead>
<tbody>
${body}
</tbody>
</table>
${MEASUREMENT_NOTES.map((note) => `<p>${escapeHtml(note)}</p>`).join("\n")}
</body>
</html>
`;
}
