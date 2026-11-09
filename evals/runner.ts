import { spawn } from "node:child_process";
import {
	appendFileSync,
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { type Static, Type } from "typebox";
import { git } from "../bench/lib/git.ts";
import { readVersion } from "../bench/lib/output.ts";
import { BENCH_PTY_SESSION } from "../bench/lib/sandbox.ts";
import { removeTree } from "../src/shared/fsx.ts";
import { endProcesses, killTree, processesIn, processIdOf } from "../src/shared/proctree.ts";
import { withoutLeadingComment } from "../src/shared/prompt.ts";
import { describe, parseYamlFile, YamlFileError } from "../src/shared/yaml.ts";
import {
	A7_API_KEY_ENV,
	assistantText,
	HARNESSES,
	ompSessionText,
	saveSessions,
	type Turn,
	type Usage,
} from "./harness.ts";
import { type LayerSetup, loadBenchLayer } from "./layer.ts";
import { type PlanReviewSummary, planReviewExtension, readPlanReviewLog } from "./plan-review.ts";
import { herdrTerminal, type TerminalRunner } from "./terminal.ts";
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

/** What A7's key becomes in its transcript and exports when an agent printed it (EVL-13). */
export const A7_KEY_MARK = "<A7 key>";

/**
 * The grading limit (0574): the end-state checks, and the hidden grading's setup and checks together, each get this
 * long; a command still running then is ended with its process tree and its check grades 0.
 */
export const GRADING_LIMIT_MS = 60 * 60_000;

/**
 * The setup before the harness has its own limit (0574, amended): a setup still running then is ended with its process
 * tree and the trial is an infra error (not the agent's doing), which the task bench retries.
 */
export const SETUP_LIMIT_MS = 60 * 60_000;

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
	/** Hidden grading's setup when it differs from `setup` (0574: a Python task's fresh grading venv). */
	gradeSetup?: string;
	/** Hidden grading's {@link pathPrefix} when it differs from the trial's (the grading venv's `Scripts`). */
	gradePathPrefix?: string[];
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
	/** A layered trial's code index build, done during setup before the trial's clock (0575); not in `wallMs`. */
	layer?: { indexBuildMs: number };
	/** A7's key was in the trial's output or exports; it was replaced there with {@link A7_KEY_MARK} (EVL-13). */
	keyExposed?: true;
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
	/** The grading limit; {@link GRADING_LIMIT_MS} when unset. */
	gradingTimeoutMs?: number;
	/** The limit of the setup before the harness; {@link SETUP_LIMIT_MS} when unset. */
	setupTimeoutMs?: number;
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

/**
 * One turn's harness arguments after the launcher, as the variant's harness builds them (its adapter's `args`); a
 * follow-up never plans first.
 */
export function trialArgs(variant: Variant, turn: Turn): string[] {
	return HARNESSES[variant.harness].args(variantArgs(turn.followUp ? { ...variant, planFirst: false } : variant), turn);
}

/** What an interactive plan-review turn adds to the overlay: plan mode at startup (`plan.defaultOnStartup`). */
export const PLAN_REVIEW_OVERLAY: Readonly<Record<string, unknown>> = {
	plan: { enabled: true, defaultOnStartup: true },
};

/**
 * One trial: a fresh temp root with the overlay and the fixture repo, the harness's turns, the end-state checks, the
 * line counts and the hidden checks. A failed setup before the harness, one cut by {@link SETUP_LIMIT_MS}, or a grading
 * setup that fails on the fixture commit too, throws (0482's infra); what the harness and the agent left running is
 * ended and the temp root removed
 * either way.
 */
export async function runTrial(
	evalCase: EvalCase,
	rep: number,
	harnessVersion: string,
	options: TrialOptions,
): Promise<ResultRow> {
	const chosen = options.variant ?? DEFAULT_VARIANT;
	const root = makeTrialRoot(options.workDir);
	const env = trialEnv(evalCase, root, chosen.env);
	const name = options.transcriptName ?? `${evalCase.id}-${rep}`;
	const startedAt = new Date().toISOString();
	const gradingLimitMs = options.gradingTimeoutMs ?? GRADING_LIMIT_MS;
	// EVL-13: A7's key, wherever the agent printed it, never reaches what the run keeps.
	const adapter = HARNESSES[chosen.harness];
	const secret = adapter.apiKeyEnv === undefined ? undefined : process.env[adapter.apiKeyEnv];
	let keyExposed = false;
	const scrub = (text: string): string => {
		if (!secret || !text.includes(secret)) return text;
		keyExposed = true;
		return text.replaceAll(secret, A7_KEY_MARK);
	};
	let layer: LayerSetup | undefined;
	try {
		const variant = withPromptCopy(chosen, root);
		// Outside the repo, so it never shows in a git_clean check or the line counts. JSON is YAML.
		const overlay = join(root, "eval-run.yml");
		const logFile = (phase: string) =>
			options.logDir === undefined ? undefined : join(options.logDir, `${phase}.log`);
		const setupLimitMs = options.setupTimeoutMs ?? SETUP_LIMIT_MS;
		const { repo, fixtureCommit } = await makeTrialRepo(evalCase, rep, root, env, {
			logFile: logFile("setup"),
			limitMs: setupLimitMs,
		}).catch((error: unknown) => {
			if (error instanceof SetupKilled)
				process.stderr.write(`${name}: setup killed after ${Math.round(setupLimitMs / 60_000)} min\n`);
			throw error;
		});
		// 0567: plan first answered in omp's interactive plan review; the first turn runs in a terminal.
		const planReview = adapter.sessionFiles ? variant.planReview : undefined;
		if (variant.layer !== undefined) {
			// A6 (0543): Jocasta's layer build sets the trial up, its code index built before the clock (0575); without
			// the build this throws before any model call.
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
			(options.sessionsDir !== undefined || planReview) && adapter.sessionFiles ? join(root, "sessions") : undefined;
		const harnessEnv =
			adapter.apiKeyEnv !== undefined ? { ...env, ANTHROPIC_API_KEY: secret } : { ...env, ...layer?.env };
		const outputs: string[] = [];
		let wallMs = 0;
		let exitedOk = true;
		let timedOut = false;
		for (const [i, prompt] of prompts.entries()) {
			const left = options.timeoutMs === undefined ? undefined : Math.max(1, options.timeoutMs - wallMs);
			const session = adapter.session(outputs.join("\n"));
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
				const terminal =
					options.runInTerminal ??
					herdrTerminal(options.ptySession ?? BENCH_PTY_SESSION, { workspaceCwd: options.workDir ?? tmpdir() });
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
			run = { ...run, stdout: scrub(run.stdout), stderr: scrub(run.stderr) };
			options.onHarnessRun?.({ status: run.status, stderr: run.stderr, timedOut: run.timedOut });
			outputs.push(run.stdout);
			wallMs += run.wallMs;
			if (run.timedOut) {
				timedOut = true;
				process.stderr.write(`${name}: killed after ${options.timeoutMs} ms\n`);
			} else if (run.status !== 0) {
				process.stderr.write(`${name}: ${variant.harness} exited ${run.status}\n${run.stderr}\n`);
			}
			if (run.status !== 0 || run.timedOut) {
				exitedOk = false;
				break;
			}
		}
		// EVL-12: language servers, watchers and dev servers the harness or the agent left go before grading, and a
		// git index lock a killed process left with them, so grading sees the agent's files, not a held repo.
		await endLeftovers(root);
		removeTree(join(repo, ".git", "index.lock"));
		// Each turn's output ends its last line, so one turn's JSON never runs into the next. A plan-review trial's
		// first turn had a terminal, not JSON: its transcript is the session files omp wrote, every turn included.
		const transcript = planReview
			? ompSessionText(sessionDir as string)
			: outputs.map((output) => (output === "" || output.endsWith("\n") ? output : `${output}\n`)).join("");
		writeFileSync(join(options.outDir, "transcripts", `${name}.jsonl`), transcript);
		const savedSessions =
			options.sessionsDir ?? (adapter.recordsDir === undefined ? undefined : join(root, adapter.recordsDir));
		if (savedSessions !== undefined)
			saveSessions(variant.harness, savedSessions, {
				transcript,
				sessionDir,
				launcher: options.launcher,
				env: harnessEnv,
				cwd: repo,
				scrub,
			});
		const grade: Record<string, number> = { [`${variant.harness} exited 0`]: exitedOk ? 1 : 0 };
		if (timedOut) grade.timeout = 0;
		const killed = () =>
			process.stderr.write(`${name}: grading killed after ${Math.round(gradingLimitMs / 60_000)} min\n`);
		const checksBy = performance.now() + gradingLimitMs;
		for (const check of evalCase.checks) {
			const result = await checkResult(check, repo, env, undefined, checksBy);
			if (result.timedOut) killed();
			grade[checkLabel(check)] = result.passed ? 1 : 0;
		}
		// After grading, so staging can't change a git_clean check.
		const lines = lineCounts(repo, fixtureCommit, logFile("lines"));
		let hiddenPassed: boolean | null = null;
		if (evalCase.hiddenChecks.length > 0) {
			const hidden = await gradeHidden(evalCase, repo, fixtureCommit, join(root, "grade"), {
				env: trialEnv({ ...evalCase, pathPrefix: evalCase.gradePathPrefix ?? evalCase.pathPrefix }, root, chosen.env),
				logFile: logFile("grade"),
				limitMs: gradingLimitMs,
			});
			if (hidden[HIDDEN_TIMEOUT] !== undefined) killed();
			Object.assign(grade, hidden);
			hiddenPassed = Object.values(hidden).every((score) => score === 1);
		}
		const response = adapter.response({ transcript, savedSessions, sessionDir, planReview: planReview !== undefined });
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
			// Only when a review turn ran: a trial whose first turn failed has no review to count.
			findings: outputs.length > 1 ? countFindings(assistantText(review ?? "", variant.harness)) : null,
			passed: Object.values(grade).every((score) => score === 1),
			...(planReview
				? {
						planReview: readPlanReviewLog(
							existsSync(planReviewLog) ? readFileSync(planReviewLog, "utf8") : "",
							planReview,
						),
					}
				: {}),
			...(layer?.indexBuildMs === undefined ? {} : { layer: { indexBuildMs: layer.indexBuildMs } }),
			...(keyExposed ? { keyExposed: true as const } : {}),
		};
	} finally {
		await layer?.close();
		await endLeftovers(root);
		try {
			removeTree(root);
		} catch (error) {
			process.stderr.write(`could not remove ${root} (${(error as NodeJS.ErrnoException).code}); delete it by hand\n`);
		}
	}
}

/** The grade a hidden grading cut by {@link GRADING_LIMIT_MS} gets (0574). */
export const HIDDEN_TIMEOUT = "hidden: timeout";
/** The grade of a grading setup that the trial's change broke: it passes on the fixture commit (0574). */
export const HIDDEN_SETUP_FAILED = "hidden: setup failed";

/**
 * Ends every process whose working folder is in `root` (EVL-12): what a harness or an agent started detached outlives
 * the harness, holds the trial's files and piles up over a run. A failure to list them is reported, not thrown.
 */
async function endLeftovers(root: string): Promise<void> {
	try {
		const ids = processesIn(root).flatMap((found) => processIdOf(found.pid) ?? []);
		await endProcesses(ids);
	} catch (error) {
		process.stderr.write(`could not end the processes left in ${root}: ${(error as Error).message}\n`);
	}
}

/**
 * The variant with its `--append-system-prompt` file copied into the trial root without a leading `<!-- … -->` block
 * (0577): Ponytail's license and file notes stay in the shipped file and out of every arm's system prompt.
 */
function withPromptCopy(variant: Variant, root: string): Variant {
	if (variant.appendSystemPrompt === undefined) return variant;
	const file = join(root, "system-prompt.md");
	writeFileSync(file, withoutLeadingComment(readFileSync(variant.appendSystemPrompt, "utf8")));
	return { ...variant, appendSystemPrompt: file };
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
 * the setup run in that checkout within `limitMs` ({@link SETUP_LIMIT_MS} by default; past it, it is ended with its
 * tree and this throws as infra) (tools that read the version from git, such as setuptools-scm and hatch-vcs, need a
 * commit, and upstream's tag to report upstream's version), and what setup wrote folded
 * into the same commit, so it never counts as the trial's change; then checked out on the trial's own branch. Hooks that
 * setup installs (husky, yorkie, simple-git-hooks) stay off: an arm that commits meets no lint or commit-message check
 * that an arm that doesn't never meets. Git converts no line endings there (`core.autocrlf` off), so its files and its
 * clean clone keep the fixture's bytes on Windows too.
 */
export async function makeTrialRepo(
	evalCase: EvalCase,
	rep: number,
	root: string,
	env: Record<string, string | undefined>,
	options: { logFile?: string; limitMs?: number } = {},
): Promise<{ repo: string; fixtureCommit: string; branch: string }> {
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
	await runSetup(evalCase, repo, env, options.logFile, performance.now() + (options.limitMs ?? SETUP_LIMIT_MS));
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

interface CommandResult {
	status: number | null;
	stdout: string;
	stderr: string;
	/** Ended by its deadline, with its process tree. */
	timedOut: boolean;
}

/** With `logFile`, appends a command's folder, exit code, seconds, stdout and stderr there (0482). */
function logCommand(logFile: string | undefined, command: string, cwd: string, started: number, result: CommandResult) {
	if (logFile === undefined) return;
	mkdirSync(dirname(logFile), { recursive: true });
	const seconds = ((performance.now() - started) / 1000).toFixed(1);
	const exit = result.timedOut ? `${result.status} (killed at its time limit)` : String(result.status);
	appendFileSync(
		logFile,
		`=== ${new Date().toISOString()}\n$ ${command}\ncwd: ${cwd}\nexit: ${exit}\nseconds: ${seconds}\n--- stdout\n${result.stdout}\n--- stderr\n${result.stderr}\n\n`,
	);
}

/**
 * Runs a shell command until `deadline` (a `performance.now()` time), then ends it with its whole process tree (EVL-5:
 * a shell's own kill leaves the test runner under it holding the pipes, and a blocked event loop never fires a test
 * framework's timeout); with `logFile`, logs it there.
 */
function runCommand(
	command: string,
	cwd: string,
	env: Record<string, string | undefined> | undefined,
	logFile: string | undefined,
	deadline: number,
): Promise<CommandResult> {
	const { promise, resolve } = Promise.withResolvers<CommandResult>();
	const started = performance.now();
	const child = spawn(command, {
		cwd,
		env,
		shell: true,
		detached: process.platform !== "win32",
		stdio: ["ignore", "pipe", "pipe"],
		windowsHide: true,
	});
	const stdout: Buffer[] = [];
	const stderr: Buffer[] = [];
	child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
	child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
	let timedOut = false;
	let settled = false;
	const finish = (status: number | null, error?: Error) => {
		if (settled) return;
		settled = true;
		clearTimeout(timer);
		const result = {
			status,
			stdout: Buffer.concat(stdout).toString("utf8"),
			stderr: Buffer.concat(stderr).toString("utf8") || (error?.message ?? ""),
			timedOut,
		};
		logCommand(logFile, command, cwd, started, result);
		resolve(result);
	};
	const timer = setTimeout(
		async () => {
			timedOut = true;
			if (child.pid !== undefined) await killTree(child.pid).catch(() => []);
			// A grandchild that kept a pipe open past the kill doesn't hold the grading up.
			setTimeout(() => finish(null), 5_000).unref();
		},
		Math.max(0, deadline - performance.now()),
	);
	child.once("error", (error) => finish(null, error));
	child.once("close", (status) => finish(status));
	return promise;
}

/** A setup before the harness ended at {@link SETUP_LIMIT_MS}: an infra error (`classifyInfra` reads `: setup \``). */
class SetupKilled extends Error {}

async function runSetup(
	evalCase: EvalCase,
	cwd: string,
	env: Record<string, string | undefined>,
	logFile: string | undefined,
	deadline: number,
): Promise<void> {
	if (evalCase.setup === undefined) return;
	const setup = await runCommand(evalCase.setup, cwd, env, logFile, deadline);
	if (setup.timedOut) throw new SetupKilled(`${evalCase.id}: setup \`${evalCase.setup}\` killed at its time limit`);
	if (setup.status !== 0) throw new Error(setupFailed(evalCase, evalCase.setup, setup.stderr));
}

/** A setup failure as an infra error names it (`classifyInfra` reads `: setup \``). */
const setupFailed = (evalCase: EvalCase, setup: string, stderr: string) =>
	`${evalCase.id}: setup \`${setup}\` failed: ${stderr}`;

/**
 * The hidden checks (0482 E1) on a clean clone of the trial's result: its changes (staged by the caller) committed on
 * whatever HEAD the agent left (EVL-8: a branch it made or a detached HEAD included), that commit cloned to `gradeDir`
 * (no ignored or build files the agent left), the case's grading setup run there, `hidden/` copied in. With `logFile`,
 * the setup's and the checks' commands are logged there.
 *
 * Grading has one limit (0574, `limitMs`, {@link GRADING_LIMIT_MS} by default) for setup and checks together: what runs
 * past it is ended and the grades so far get {@link HIDDEN_TIMEOUT} 0. A grading setup that fails is run again on a
 * clean clone of the fixture commit, within the same limit (EVL-5; cut there, it grades {@link HIDDEN_TIMEOUT} 0 like a
 * cut setup): when it passes there the trial's change broke it, and the grade is {@link HIDDEN_SETUP_FAILED} 0; when it
 * fails there too, the setup itself is broken and this throws (infra, 0482).
 */
export async function gradeHidden(
	evalCase: EvalCase,
	repo: string,
	fixtureCommit: string,
	gradeDir: string,
	options: { env: Record<string, string | undefined>; logFile?: string; limitMs?: number },
): Promise<Record<string, number>> {
	const { env, logFile } = options;
	const deadline = performance.now() + (options.limitMs ?? GRADING_LIMIT_MS);
	git(repo, ["commit", "-q", "--allow-empty", "--no-verify", "-m", "trial result"]);
	const head = git(repo, ["rev-parse", "HEAD"]).trim();
	cloneAt(repo, head, gradeDir);
	const setup = evalCase.gradeSetup ?? evalCase.setup;
	if (setup !== undefined) {
		const run = await runCommand(setup, gradeDir, env, logFile, deadline);
		if (run.timedOut) return { [HIDDEN_TIMEOUT]: 0 };
		if (run.status !== 0) {
			const base = `${gradeDir}-base`;
			cloneAt(repo, fixtureCommit, base);
			const again = await runCommand(setup, base, env, logFile, deadline);
			if (again.timedOut) return { [HIDDEN_TIMEOUT]: 0 };
			if (again.status !== 0) throw new Error(setupFailed(evalCase, setup, again.stderr));
			return { [HIDDEN_SETUP_FAILED]: 0 };
		}
	}
	if (evalCase.hidden) cpSync(evalCase.hidden, gradeDir, { recursive: true });
	const grade: Record<string, number> = {};
	for (const check of evalCase.hiddenChecks) {
		const result = await checkResult(check, gradeDir, env, logFile, deadline);
		grade[`hidden: ${checkLabel(check)}`] = result.passed ? 1 : 0;
		if (result.timedOut) {
			grade[HIDDEN_TIMEOUT] = 0;
			break;
		}
	}
	return grade;
}

/** A clone of `repo` at `commit` (detached), its bytes as committed; the fixture tag comes along. */
function cloneAt(repo: string, commit: string, dir: string): void {
	git(repo, ["clone", "-q", "--no-checkout", "-c", "core.autocrlf=false", repo, dir]);
	git(dir, ["checkout", "-q", "--detach", commit]);
}

/** `[P0]`…`[P3]` markers in a review's reply (0453's priorities); P0 and P1 are blocking. */
export function countFindings(text: string): { total: number; blocking: number } {
	const priorities = [...text.matchAll(/\[P([0-3])\]/g)].map((match) => Number(match[1]));
	return { total: priorities.length, blocking: priorities.filter((p) => p <= 1).length };
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
					// The product's walk (EVL-12): children whose parent already exited go too.
					if (child.pid !== undefined) void killTree(child.pid).catch(() => []);
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

export function checkLabel(check: Check): string {
	if (check.command !== undefined) return `command: ${check.command}`;
	if (check.git_clean !== undefined) return `git_clean: ${check.git_clean}`;
	if (check.absent) return `file: ${check.file} absent`;
	if (check.contains !== undefined) return `file: ${check.file} contains ${JSON.stringify(check.contains)}`;
	return `file: ${check.file}`;
}

/**
 * Grades one end-state check against the trial repo after the run (0164); a command runs with `env` (default: this
 * process's) within {@link GRADING_LIMIT_MS} and, with `logFile`, is logged there.
 */
export async function gradeCheck(
	check: Check,
	repo: string,
	env?: Record<string, string | undefined>,
	logFile?: string,
): Promise<boolean> {
	return (await checkResult(check, repo, env, logFile, performance.now() + GRADING_LIMIT_MS)).passed;
}

/** {@link gradeCheck} until `deadline`: a command still running then is ended with its tree and fails. */
async function checkResult(
	check: Check,
	repo: string,
	env: Record<string, string | undefined> | undefined,
	logFile: string | undefined,
	deadline: number,
): Promise<{ passed: boolean; timedOut: boolean }> {
	if (check.command !== undefined) {
		const run = await runCommand(check.command, repo, env, logFile, deadline);
		return { passed: run.status === 0 && !run.timedOut, timedOut: run.timedOut };
	}
	if (check.git_clean !== undefined) {
		return { passed: (git(repo, ["status", "--porcelain"]).trim() === "") === check.git_clean, timedOut: false };
	}
	const path = join(repo, check.file ?? "");
	if (check.absent) return { passed: !existsSync(path), timedOut: false };
	if (!existsSync(path)) return { passed: false, timedOut: false };
	return {
		passed: check.contains === undefined || readFileSync(path, "utf8").includes(check.contains),
		timedOut: false,
	};
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
