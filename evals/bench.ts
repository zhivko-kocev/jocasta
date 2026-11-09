// The task benchmark of 0482, as 0543 amends it: `bun evals/bench.ts --tasks <tasks.jsonl> --cache <dir>
// --out <runDir> [--study main|followup] [--arms a0,a1,a2,a3,a4,a6,a7] [--reps 3] [--set main] [--only id,id]
// [--pilot] [--workers 2] [--seed 482] [--timeout 90] [--profile <omp profile>] [--work <dir>] [--validate]
// [--dry-run]`. Trials (tasks × arms × reps; `--study followup` is the follow-up study's 10 tasks × 2 reps × A6 and
// its four arms) run in a seeded random order through the eval runner's `runTrial`, each in its own worker thread
// (`--workers` at once, never two of one task), the quota pacer awaited before each, their roots in `--work` (default
// the OS temp folder). Results go to `<runDir>/results.jsonl` (with each trial's ledger totals), the ledger's rows to
// `ledger.jsonl`, each attempt's priced spend to `spend.jsonl`, the harnesses' own records to `sessions/`, infra errors
// to `errors.jsonl` (retried, at most twice more), setup and grading output to `logs/`, the run's conditions to
// `conditions.json`; a rerun with the same `--out` and the same conditions resumes, and `<runDir>/run.lock` keeps a
// second bench out while one runs. `--validate` checks each task without a model: its hidden tests fail on the base
// commit and pass three times with the gold patch (logs in `validation/`). Real trials bill model usage: they need
// `JOCASTA_EVALS_REAL=1` and never run in CI (0169). Exits 1 when a trial or task is left failing or an arm stopped on
// a new harness version, 2 on a bad flag or an omp folder that would give the omp arms MCP servers (0587), 3 after
// three sign-in failures in a row, 4 when A7's spend limit stopped its trials.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { hostname, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { isMainThread, parentPort, Worker } from "node:worker_threads";
import { BenchUsageError, flag, isInside, listFlag, parseBenchArgs, REPO_ROOT, runScript } from "../bench/lib/args.ts";
import { git } from "../bench/lib/git.ts";
import { toolCommand } from "../bench/lib/launch.ts";
import { benchPricing, ledgerOfDir } from "../bench/lib/ledger.ts";
import { type Conditions, conditions, readVersion } from "../bench/lib/output.ts";
import { loadPriceTable, type PriceTable } from "../bench/lib/prices.ts";
import { BENCH_PTY_SESSION } from "../bench/lib/sandbox.ts";
import { findHerdr } from "../src/frontier/server/herdr.ts";
import { asObject, removeTree } from "../src/shared/fsx.ts";
import { git as gitRead } from "../src/shared/git.ts";
import { appendJsonl, readJsonlFile } from "../src/shared/jsonl.ts";
import type { TrialLedger } from "../src/shared/ledger.ts";
import { ompAgentDir } from "../src/shared/paths.ts";
import { onPath } from "../src/shared/process.ts";
import { pidAlive, processStartedAt } from "../src/shared/proctree.ts";
import { profileName } from "./args.ts";
import { FOLLOWUP_ARMS, FOLLOWUP_COMPARISONS, FOLLOWUP_REPS, FOLLOWUP_TASKS, pickFollowupTasks } from "./followup.ts";
import { A7_API_KEY_ENV } from "./harness.ts";
import { LAYER_MODULE, layerBuilt, layerMissingMessage, loadBenchLayer } from "./layer.ts";
import { createPacer, PACER_PROVIDER, PAUSE_AT, type Pause, realPacerDeps } from "./pacer.ts";
import {
	type EvalCase,
	GRADING_LIMIT_MS,
	gradeHidden,
	makeTrialRepo,
	makeTrialRoot,
	type ResultRow,
	runTrial,
	SETUP_LIMIT_MS,
	type TrialOptions,
	trialArgs,
	trialEnv,
} from "./runner.ts";
import {
	type BenchRow,
	type BenchTask,
	classifyInfra,
	type InfraRow,
	loadTasks,
	MAX_ATTEMPTS,
	type PreparedTask,
	pendingTrials,
	pickPilot,
	planTrials,
	prepareTask,
	readResumeState,
	scanLeaks,
	type Trial,
	taskCase,
	trialKey,
} from "./tasks.ts";
import { loadVariants, type Variant } from "./variants.ts";

/** One arm as the run uses it: its variant, the command that starts its harness, and that harness's version. */
export interface Arm {
	name: string;
	variant: Variant;
	launcher: string[];
	harnessVersion: string;
}

/** What a worker runs: one trial through `runTrial`. */
export interface TrialJob {
	evalCase: EvalCase;
	rep: number;
	harnessVersion: string;
	options: Omit<TrialOptions, "onHarnessRun">;
}

/** A trial's result as the worker returns it: the row, or the error it threw, and the harness's stderr. */
export interface TrialOutcome {
	row?: ResultRow;
	error?: string;
	stderr: string;
}

export interface BenchOptions {
	tasks: BenchTask[];
	cacheDir: string;
	outDir: string;
	arms: Arm[];
	reps: number;
	seed: number;
	workers: number;
	timeoutMs?: number;
	/** The setup-before-the-harness limit; {@link SETUP_LIMIT_MS} when unset (tests shorten it). Held for the run. */
	setupTimeoutMs?: number;
	profile?: string;
	/** The bench herdr session for plan-review trials' terminals (`--pty-session`, 0567). */
	ptySession?: string;
	/** Recorded in `conditions.json`. */
	pilot?: boolean;
	pace?: () => Promise<void>;
	/** Whether the pacer's last reading had a full account: a rate limit then is an infra error (0543). */
	accountFull?: () => boolean;
	/** The ledger's price table; the committed one by default. */
	prices?: PriceTable;
	/** A7's spend limit in dollars at the price table (0537); {@link A7_SPEND_LIMIT_USD} by default. */
	a7SpendLimitUsd?: number;
	/** Where trial roots go (`--work`); the OS temp folder when unset. Recorded in `conditions.json`. */
	workDir?: string;
	/** Runs one trial; the default runs it in a worker thread. */
	execute?: (job: TrialJob) => Promise<TrialOutcome>;
	/**
	 * Reads an arm's harness version again after each of its trials (0574); a changed version stops that arm's
	 * remaining trials. No re-check when unset.
	 */
	versionOf?: (arm: Arm) => string;
	/** The host's conditions for `conditions.json`; the default asks each tool for its version. */
	describeHost?: () => Conditions;
	log?: (line: string) => void;
	/** Pauses the pacer took, for `conditions.json`. */
	pauses?: Pause[];
}

/** The tools whose versions `conditions.json` records (0482, 0537). */
export const VERSION_TOOLS = ["omp", "claude", "opencode", "git", "bun", "node", "python", "go"] as const;

/**
 * A7's spend limit (0537): once the priced spend of A7's attempts (`spend.jsonl`, the pinned price table) reaches it,
 * no further A7 trial starts. The key's own limit at Anthropic is the backstop for a trial already running.
 */
export const A7_SPEND_LIMIT_USD = 150;
/** A7 runs in 0481's VM only (0537), the machine H1's git-guard check also requires. */
export const VM_MACHINE = "JOCASTA-CLEAN";

/** One `spend.jsonl` row: an attempt's priced cost, whether it counted or was an infra error. */
export interface SpendRow {
	task: string;
	arm: string;
	rep: number;
	attempt: number;
	harness: string;
	cost: number;
	unpricedModels: string[];
}

/** Long-path support as the trials see it: git's effective `core.longpaths` and, on Windows, `LongPathsEnabled`. */
export interface LongPaths {
	gitCoreLongpaths: string | null;
	/** `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem` `LongPathsEnabled`; null off Windows or when unset. */
	windowsLongPathsEnabled: number | null;
}

/** Reads {@link LongPaths} without changing anything. */
export function readLongPaths(): LongPaths {
	const gitCoreLongpaths = gitRead(tmpdir(), ["config", "--get", "core.longpaths"]) ?? null;
	let windowsLongPathsEnabled: number | null = null;
	if (process.platform === "win32") {
		const reg = spawnSync(
			"reg",
			["query", "HKLM\\SYSTEM\\CurrentControlSet\\Control\\FileSystem", "/v", "LongPathsEnabled"],
			{ encoding: "utf8", windowsHide: true, timeout: 30_000 },
		);
		const value = /LongPathsEnabled\s+REG_DWORD\s+(0x[0-9a-f]+)/i.exec(reg.stdout ?? "")?.[1];
		if (value !== undefined) windowsLongPathsEnabled = Number(value);
	}
	return { gitCoreLongpaths, windowsLongPathsEnabled };
}

/** The host, the tools' versions (Go's from `go version`, not `--version`) and long-path support. */
export function hostConditions(): Conditions & { longPaths: LongPaths } {
	const host = conditions(VERSION_TOOLS.filter((tool) => tool !== "go"));
	const go = onPath("go");
	const version =
		go === null ? null : spawnSync(go, ["version"], { encoding: "utf8", windowsHide: true, timeout: 30_000 });
	host.versions.go = version?.status === 0 ? version.stdout.trim() : null;
	return { ...host, longPaths: readLongPaths() };
}

/** Runs one trial in a worker thread, so a setup's or a check's blocking spawn never stalls another trial's harness. */
export function executeInWorker(job: TrialJob): Promise<TrialOutcome> {
	const { promise, resolve } = Promise.withResolvers<TrialOutcome>();
	const worker = new Worker(import.meta.path);
	let settled = false;
	const finish = (outcome: TrialOutcome) => {
		if (settled) return;
		settled = true;
		void worker.terminate();
		resolve(outcome);
	};
	worker.once("message", (outcome: TrialOutcome) => finish(outcome));
	worker.once("error", (error) => finish({ error: String(error), stderr: "" }));
	worker.once("exit", (code) => finish({ error: `trial worker exited ${code}`, stderr: "" }));
	worker.postMessage(job);
	return promise;
}

if (!isMainThread)
	parentPort?.once("message", async (job: TrialJob) => {
		const stderr: string[] = [];
		try {
			const row = await runTrial(job.evalCase, job.rep, job.harnessVersion, {
				...job.options,
				onHarnessRun: (run) => stderr.push(run.stderr),
			});
			parentPort?.postMessage({ row, stderr: stderr.join("\n") } satisfies TrialOutcome);
		} catch (error) {
			parentPort?.postMessage({
				error: error instanceof Error ? error.message : String(error),
				stderr: stderr.join("\n"),
			} satisfies TrialOutcome);
		}
	});

/** What a run holds for its whole length (0574): a resume with other values is refused. */
interface HeldConditions {
	timeoutMinutes: number | null;
	profile: string | null;
	/** The setup-before-the-harness limit (0574, amended); absent in runs from before it. */
	setupLimitMinutes?: number;
	/** Each arm's `--version` at the run's start. */
	versions: Record<string, string>;
}

interface RunRecord {
	seed: number;
	arms: string[];
	reps: number;
	tasks: string[];
	pilot: boolean;
	order: string[];
	/** Absent in runs from before 0574. */
	held?: HeldConditions;
	sessions: (Conditions & {
		workers: number;
		timeoutMinutes: number | null;
		gradingLimitMinutes: number;
		setupLimitMinutes: number;
		profile: string | null;
		workRoot: string;
	})[];
}

/** Infra errors in a row that were sign-in failures; three stop the run (0482). */
const AUTH_STOP = 3;

/** `<out>/run.lock`: the bench that runs in `--out` now (0574). */
export const RUN_LOCK = "run.lock";

interface RunLock {
	pid: number;
	startedAt: string;
	/** The holder's process start (Windows), so a reused pid isn't taken for the holder. */
	processStart?: number;
}

/** A time as `YYYY-MM-DD HH:MM`, local. */
function localMinute(iso: string): string {
	const at = new Date(iso);
	const two = (n: number) => String(n).padStart(2, "0");
	return `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())} ${two(at.getHours())}:${two(at.getMinutes())}`;
}

/**
 * Takes `<outDir>/run.lock` for this process (0574, as the service's lock, 0565): refused while a live bench holds it;
 * a lock whose process is gone (a crashed run) is taken over. Returns its release.
 */
function takeRunLock(outDir: string): () => void {
	const file = join(outDir, RUN_LOCK);
	let held: RunLock | undefined;
	try {
		held = JSON.parse(readFileSync(file, "utf8")) as RunLock;
	} catch {
		held = undefined;
	}
	if (held !== undefined) {
		const start = processStartedAt(held.pid);
		const same = held.processStart === undefined || start === undefined || Math.abs(start - held.processStart) < 1_000;
		if (pidAlive(held.pid) && same)
			throw new BenchUsageError(
				`${outDir} is in use by a bench (pid ${held.pid}, since ${localMinute(held.startedAt)}); stop it or use another --out`,
			);
		removeTree(file);
	}
	const mine: RunLock = { pid: process.pid, startedAt: new Date().toISOString() };
	const start = processStartedAt(process.pid);
	if (start !== undefined) mine.processStart = start;
	try {
		writeFileSync(file, `${JSON.stringify(mine)}\n`, { flag: "wx" });
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
		throw new BenchUsageError(`${outDir} is in use by another bench starting now; stop it or use another --out`);
	}
	return () => {
		try {
			if ((JSON.parse(readFileSync(file, "utf8")) as RunLock).pid === process.pid) removeTree(file);
		} catch {
			// Gone already.
		}
	};
}

/** A resume whose held conditions differ from the run's: the refusal's text, or undefined when they match. */
function heldDiffers(outDir: string, before: HeldConditions, now: HeldConditions): string | undefined {
	const profile = (held: HeldConditions) => (held.profile === null ? "no profile" : `profile ${held.profile}`);
	const changed: string[] = [];
	if (now.timeoutMinutes !== before.timeoutMinutes) changed.push(`--timeout ${now.timeoutMinutes ?? "none"}`);
	if (before.setupLimitMinutes !== undefined && now.setupLimitMinutes !== before.setupLimitMinutes)
		changed.push(`setup limit ${now.setupLimitMinutes} min`);
	if (now.profile !== before.profile) changed.push(profile(now));
	for (const [arm, version] of Object.entries(now.versions))
		if (before.versions[arm] !== version && !changed.includes(version)) changed.push(version);
	if (changed.length === 0) return undefined;
	const was = [
		`--timeout ${before.timeoutMinutes ?? "none"}`,
		...(before.setupLimitMinutes === undefined ? [] : [`setup limit ${before.setupLimitMinutes} min`]),
		profile(before),
		...new Set(Object.values(before.versions)),
	];
	return `${outDir} ran with ${was.join(", ")}; this resume has ${changed.join(", ")}; resume with the same flags or use a new --out`;
}

/** Every file's text under `dir`: a trial's saved sessions, subagents' included, for the leak scan (EVL-10). */
function textUnder(dir: string): string {
	if (!existsSync(dir)) return "";
	return readdirSync(dir, { recursive: true, encoding: "utf8" })
		.map((name) => join(dir, name))
		.filter((path) => statSync(path).isFile())
		.map((path) => readFileSync(path, "utf8"))
		.join("\n");
}

/**
 * Runs the benchmark's trials into `outDir`, resuming what is there, while it holds `<outDir>/run.lock`. A free worker
 * takes the next trial in the seeded order whose task isn't running already, so two trials never share a task's
 * caches or tool folders at once. Each attempt's saved sessions are priced into the ledger (0538); A7's trials stop
 * once its priced spend reaches the limit (0537); an arm whose harness reports a new version stops (0574). Returns the
 * exit code: 0 when every trial has a row, 1 when some used up their attempts on infra errors or an arm stopped on a
 * new version, 3 when three sign-in failures in a row stopped the run, 4 when A7's spend limit (or an A7 model the
 * price table lacks) stopped its trials.
 */
export async function runBench(options: BenchOptions): Promise<number> {
	mkdirSync(join(options.outDir, "transcripts"), { recursive: true });
	const release = takeRunLock(options.outDir);
	try {
		return await runLocked(options);
	} finally {
		release();
	}
}

async function runLocked(options: BenchOptions): Promise<number> {
	const log = options.log ?? ((line: string) => console.log(line));
	const execute = options.execute ?? executeInWorker;
	const order = planTrials(
		options.tasks,
		options.arms.map((arm) => arm.name),
		options.reps,
		options.seed,
	);
	const held: HeldConditions = {
		timeoutMinutes: options.timeoutMs === undefined ? null : options.timeoutMs / 60_000,
		profile: options.profile ?? null,
		setupLimitMinutes: (options.setupTimeoutMs ?? SETUP_LIMIT_MS) / 60_000,
		versions: Object.fromEntries(options.arms.map((arm) => [arm.name, arm.harnessVersion])),
	};
	const record: RunRecord = {
		seed: options.seed,
		arms: options.arms.map((arm) => arm.name),
		reps: options.reps,
		tasks: options.tasks.map((task) => task.instance_id),
		pilot: options.pilot ?? false,
		order: order.map(trialKey),
		held,
		sessions: [],
	};
	const recordFile = join(options.outDir, "conditions.json");
	if (existsSync(recordFile)) {
		const before = JSON.parse(readFileSync(recordFile, "utf8")) as RunRecord;
		if (JSON.stringify(before.order) !== JSON.stringify(record.order))
			throw new BenchUsageError(
				`${options.outDir} holds a run with another plan (seed, arms, reps or tasks); resume it with the same flags or use a new --out`,
			);
		// A run from before 0574 recorded no held conditions: this session's become them.
		const refused = before.held === undefined ? undefined : heldDiffers(options.outDir, before.held, held);
		if (refused !== undefined) throw new BenchUsageError(refused);
		record.held = before.held ?? held;
		record.sessions = before.sessions;
	}
	const session = {
		...(options.describeHost ?? hostConditions)(),
		workers: options.workers,
		timeoutMinutes: held.timeoutMinutes,
		gradingLimitMinutes: GRADING_LIMIT_MS / 60_000,
		setupLimitMinutes: held.setupLimitMinutes ?? SETUP_LIMIT_MS / 60_000,
		profile: held.profile,
		workRoot: options.workDir ?? tmpdir(),
	};
	record.sessions.push(session);
	const saveRecord = () => writeFileSync(recordFile, `${JSON.stringify(record, null, 2)}\n`);
	saveRecord();

	const tasks = new Map<string, { task: BenchTask; evalCase: EvalCase }>();
	for (const task of options.tasks) {
		log(`preparing ${task.instance_id}`);
		tasks.set(task.instance_id, { task, evalCase: taskCase(task, prepareTask(task, options.cacheDir)) });
	}
	const arms = new Map(options.arms.map((arm) => [arm.name, arm]));
	const state = readResumeState(options.outDir);
	const { pending, exhausted } = pendingTrials(order, state);
	log(`${order.length} trials: ${order.length - pending.length - exhausted.length} done, ${pending.length} to run`);
	const queue = [...pending];
	let authInRow = 0;
	let stoppedBy: string | null = null;
	const prices = options.prices ?? loadPriceTable();
	const spendFile = join(options.outDir, "spend.jsonl");
	const spendLimit = options.a7SpendLimitUsd ?? A7_SPEND_LIMIT_USD;
	const isA7 = (trial: Trial) => arms.get(trial.arm)?.variant.harness === "opencode";
	const a7Spend = readJsonlFile<SpendRow>(spendFile).filter((row) => row.harness === "opencode");
	let a7Spent = a7Spend.reduce((sum, row) => sum + row.cost, 0);
	const unpriced = [...new Set(a7Spend.flatMap((row) => row.unpricedModels))];
	// A resume after an unpriced-model stop stays stopped: its spend still can't be counted (EVL-16).
	let a7StoppedWhy: string | null =
		unpriced.length > 0
			? `the price table lacks ${unpriced.join(", ")}, so A7's spend can't be counted`
			: a7Spent >= spendLimit
				? `A7's priced spend is $${a7Spent.toFixed(2)}`
				: null;
	const a7Skipped: Trial[] = [];
	/** Arms stopped because their harness reported a new version (0574), with why; and their trials not run. */
	const versionStopped = new Map<string, string>();
	const versionSkipped: Trial[] = [];
	/** Tasks with a trial running now. */
	const running = new Set<string>();
	let wake = Promise.withResolvers<void>();
	const notify = () => {
		const woken = wake;
		wake = Promise.withResolvers<void>();
		woken.resolve();
	};
	const checkVersion = (arm: Arm) => {
		if (options.versionOf === undefined || versionStopped.has(arm.name)) return;
		let now: string;
		try {
			now = options.versionOf(arm);
		} catch (error) {
			now = `nothing (${(error as Error).message.split("\n")[0]})`;
		}
		if (now !== arm.harnessVersion)
			versionStopped.set(
				arm.name,
				`${arm.variant.harness} --version now says ${now}; the run started on ${arm.harnessVersion} (0482 pins it)`,
			);
	};

	const runOne = async (trial: Trial) => {
		const { task, evalCase } = tasks.get(trial.task) as { task: BenchTask; evalCase: EvalCase };
		const arm = arms.get(trial.arm) as Arm;
		const key = trialKey(trial);
		const attempt = (state.failures.get(key) ?? 0) + 1;
		const transcriptName = `${trial.task}-${trial.arm}-${trial.rep}`;
		const sessionsDir = join(options.outDir, "sessions", transcriptName);
		// An attempt that throws before it saves its sessions never prices the previous attempt's again (EVL-16).
		removeTree(sessionsDir);
		const outcome = await execute({
			evalCase,
			rep: trial.rep,
			harnessVersion: arm.harnessVersion,
			options: {
				outDir: options.outDir,
				launcher: arm.launcher,
				variant: arm.variant,
				profile: options.profile,
				timeoutMs: options.timeoutMs,
				...(options.setupTimeoutMs === undefined ? {} : { setupTimeoutMs: options.setupTimeoutMs }),
				transcriptName,
				workDir: options.workDir,
				ptySession: options.ptySession,
				logDir: join(options.outDir, "logs", transcriptName),
				sessionsDir,
			},
		});
		checkVersion(arm);
		const ledger: TrialLedger | null = existsSync(sessionsDir)
			? ledgerOfDir(sessionsDir, arm.variant.harness, benchPricing(prices))
			: null;
		const spend: SpendRow = {
			task: trial.task,
			arm: trial.arm,
			rep: trial.rep,
			attempt,
			harness: arm.variant.harness,
			cost: ledger?.totals.cost ?? 0,
			unpricedModels: ledger?.totals.unpricedModels ?? [],
		};
		appendJsonl(spendFile, spend);
		if (isA7(trial)) {
			a7Spent += spend.cost;
			if (spend.unpricedModels.length > 0)
				a7StoppedWhy ??= `the price table lacks ${spend.unpricedModels.join(", ")}, so A7's spend can't be counted`;
			else if (a7Spent >= spendLimit) a7StoppedWhy ??= `A7's priced spend is $${a7Spent.toFixed(2)}`;
		}
		const transcriptFile = join(options.outDir, "transcripts", `${transcriptName}.jsonl`);
		const transcript = outcome.row && existsSync(transcriptFile) ? readFileSync(transcriptFile, "utf8") : "";
		const infra = classifyInfra({
			...outcome,
			transcript,
			prompt: evalCase.prompt,
			accountFull: options.accountFull?.() ?? false,
		});
		if (infra) {
			const row: InfraRow = {
				task: trial.task,
				arm: trial.arm,
				rep: trial.rep,
				attempt,
				trialIndex: trial.index,
				harness: arm.variant.harness,
				...infra,
				at: new Date().toISOString(),
			};
			appendJsonl(join(options.outDir, "errors.jsonl"), row);
			state.failures.set(key, attempt);
			log(`infra ${key} (attempt ${attempt}, ${infra.kind}): ${infra.message.split("\n")[0]}`);
			authInRow = infra.kind === "auth" ? authInRow + 1 : 0;
			if (authInRow >= AUTH_STOP) stoppedBy ??= arm.variant.harness;
			else if (attempt < MAX_ATTEMPTS) queue.push(trial);
			else exhausted.push(trial);
			return;
		}
		authInRow = 0;
		const row: BenchRow = {
			...(outcome.row as ResultRow),
			task: trial.task,
			arm: trial.arm,
			language: task.language,
			attempt,
			trialIndex: trial.index,
			// Subagents' own records too (EVL-10): a search an explorer ran reaches the transcript only as its report.
			leak: scanLeaks(task, `${transcript}\n${textUnder(sessionsDir)}`, evalCase.prompt),
			...(ledger ? { ledger: { ...ledger.totals, sessions: ledger.sessions.length } } : {}),
		};
		appendJsonl(join(options.outDir, "results.jsonl"), row);
		for (const ledgerRow of ledger?.rows ?? [])
			appendJsonl(join(options.outDir, "ledger.jsonl"), {
				task: trial.task,
				arm: trial.arm,
				rep: trial.rep,
				...ledgerRow,
			});
		state.done.add(key);
		log(`${row.hiddenPassed ? "pass" : "FAIL"} ${key}${row.leak.length > 0 ? ` (leak: ${row.leak.join(", ")})` : ""}`);
	};

	/** Where a trial goes instead of running: an arm stopped by A7's limit or by a new harness version. */
	const skippedTo = (trial: Trial): Trial[] | undefined =>
		a7StoppedWhy !== null && isA7(trial) ? a7Skipped : versionStopped.has(trial.arm) ? versionSkipped : undefined;
	const worker = async () => {
		for (;;) {
			if (stoppedBy !== null || queue.length === 0) return;
			for (let i = queue.length - 1; i >= 0; i--) {
				const trial = queue[i] as Trial;
				const to = skippedTo(trial);
				if (to !== undefined) to.unshift(...queue.splice(i, 1));
			}
			if (queue.length === 0) return;
			const next = queue.findIndex((trial) => !running.has(trial.task));
			if (next === -1) {
				await wake.promise;
				continue;
			}
			const [trial] = queue.splice(next, 1) as [Trial];
			running.add(trial.task);
			try {
				await options.pace?.();
				const to = skippedTo(trial);
				if (stoppedBy === null && to === undefined) await runOne(trial);
				else if (stoppedBy === null) to?.push(trial);
			} finally {
				running.delete(trial.task);
				notify();
			}
		}
	};
	await Promise.all(Array.from({ length: Math.max(1, options.workers) }, worker));

	Object.assign(session, {
		finishedAt: new Date().toISOString(),
		pauses: options.pauses ?? [],
		a7SpendUsd: Math.round(a7Spent * 100) / 100,
	});
	saveRecord();
	// Exhausted trials are listed whatever else stopped the run (EVL-16).
	if (exhausted.length > 0)
		log(
			`${exhausted.length} trials used up ${MAX_ATTEMPTS} attempts on infra errors: ${exhausted.map(trialKey).join(", ")}`,
		);
	for (const [arm, why] of versionStopped) {
		const left = versionSkipped.filter((trial) => trial.arm === arm).length;
		log(`stopped ${arm}: ${why}; ${left} ${arm} trials not run`);
	}
	if (stoppedBy !== null) {
		log(
			`stopped: ${AUTH_STOP} trials in a row could not sign in to ${stoppedBy}. Sign in to ${stoppedBy} again (a token refresh on another machine may have revoked the copied login), then rerun the same command to resume.`,
		);
		return 3;
	}
	if (a7StoppedWhy !== null && a7Skipped.length > 0) {
		log(
			`A7 stopped (${a7StoppedWhy}; limit $${spendLimit}, 0537): ${a7Skipped.length} A7 trials not run: ${a7Skipped.map(trialKey).join(", ")}`,
		);
		return 4;
	}
	return exhausted.length > 0 || versionSkipped.length > 0 ? 1 : 0;
}

/** One `validation.jsonl` row: the task's hidden tests without a model, on the base commit and with the gold patch. */
export interface ValidationRow {
	task: string;
	language: string;
	/** The hidden tests failed on the base commit (as they must); null when validation stopped before. */
	null_failed: boolean | null;
	/** Gold-patch gradings that passed, of {@link GOLD_GRADES}. */
	gold_passes: number;
	seconds: { prepare: number; setup: number; null: number | null; gold: number[] };
	error: string | null;
}

export const GOLD_GRADES = 3;

const since = (start: number) => Math.round((performance.now() - start) / 100) / 10;

/**
 * Validates one task without a model (0482, the SWE-bench-Live check): the fixture with its setup, graded unchanged on
 * a clean clone (a *null* trial, which must fail), then `gold.patch` applied as the "agent" and graded on a fresh clean
 * clone {@link GOLD_GRADES} times (each must pass). Grading is the runner's own `gradeHidden`, its limit included. With
 * `logDir`, each phase's commands and output go to `setup.log`, `null.log` and `gold-<n>.log` there; the trial root
 * goes in `workDir`.
 */
export async function validateTask(
	task: BenchTask,
	cacheDir: string,
	options: { logDir?: string; workDir?: string } = {},
): Promise<ValidationRow> {
	const row: ValidationRow = {
		task: task.instance_id,
		language: task.language,
		null_failed: null,
		gold_passes: 0,
		seconds: { prepare: 0, setup: 0, null: null, gold: [] },
		error: null,
	};
	let start = performance.now();
	let prepared: PreparedTask;
	try {
		prepared = prepareTask(task, cacheDir);
	} catch (error) {
		row.error = (error as Error).message;
		return row;
	}
	row.seconds.prepare = since(start);
	const evalCase = taskCase(task, prepared);
	const logFile = (phase: string) => (options.logDir === undefined ? undefined : join(options.logDir, `${phase}.log`));
	if (options.logDir !== undefined) removeTree(options.logDir);
	const root = makeTrialRoot(options.workDir);
	try {
		const env = trialEnv(evalCase, root);
		const gradeEnv = trialEnv({ ...evalCase, pathPrefix: evalCase.gradePathPrefix ?? evalCase.pathPrefix }, root);
		start = performance.now();
		const { repo, fixtureCommit } = await makeTrialRepo(evalCase, 0, root, env, { logFile: logFile("setup") });
		row.seconds.setup = since(start);
		const passes = async (gradeDir: string, phase: string) =>
			Object.values(
				await gradeHidden(evalCase, repo, fixtureCommit, join(root, gradeDir), {
					env: gradeEnv,
					logFile: logFile(phase),
				}),
			).every((score) => score === 1);
		start = performance.now();
		row.null_failed = !(await passes("g0", "null"));
		row.seconds.null = since(start);
		git(repo, ["apply", "--whitespace=nowarn", prepared.goldPatch]);
		git(repo, ["add", "-A"]);
		for (let i = 1; i <= GOLD_GRADES; i++) {
			start = performance.now();
			if (await passes(`g${i}`, `gold-${i}`)) row.gold_passes++;
			row.seconds.gold.push(since(start));
		}
	} catch (error) {
		row.error = (error as Error).message;
	} finally {
		removeTree(root);
	}
	return row;
}

export const validationPassed = (row: ValidationRow): boolean =>
	row.error === null && row.null_failed === true && row.gold_passes === GOLD_GRADES;

/**
 * `--validate`: every task in turn into `<out>/validation.jsonl` (rewritten), each phase's log in
 * `<out>/validation/<task>/`, then a summary; the failing ids.
 */
export async function runValidation(
	tasks: readonly BenchTask[],
	cacheDir: string,
	outDir: string,
	log: (line: string) => void = (line) => console.log(line),
	workDir?: string,
): Promise<string[]> {
	mkdirSync(outDir, { recursive: true });
	const file = join(outDir, "validation.jsonl");
	writeFileSync(file, "");
	const failed: string[] = [];
	for (const task of tasks) {
		log(`validating ${task.instance_id}`);
		const row = await validateTask(task, cacheDir, { logDir: join(outDir, "validation", task.instance_id), workDir });
		appendJsonl(file, row);
		if (!validationPassed(row)) failed.push(task.instance_id);
		log(
			`  ${validationPassed(row) ? "ok" : "FAILED"}: null ${row.null_failed === null ? "not run" : row.null_failed ? "failed" : "PASSED"}, gold ${row.gold_passes}/${GOLD_GRADES}${row.error ? `, error: ${row.error.split("\n")[0]}` : ""}`,
		);
	}
	log(`${tasks.length - failed.length}/${tasks.length} tasks valid; results: ${file}`);
	if (failed.length > 0) log(`swap for a spare of the same language: ${failed.join(", ")}`);
	return failed;
}

/** The main run's arms (0543): A0–A4 as 0482 has them, A6 = A3 + the harness-OS layer, A7 = opencode bare. */
export const DEFAULT_ARMS = ["a0", "a1", "a2", "a3", "a4", "a6", "a7"];
const VARIANTS_DIR = join(import.meta.dir, "variants");

function wholeNumber(text: string | undefined, name: string, fallback: number, min = 1): number {
	if (text === undefined) return fallback;
	if (!/^\d+$/.test(text) || Number(text) < min)
		throw new BenchUsageError(`--${name} needs a whole number of ${min} or more, not "${text}"`);
	return Number(text);
}

/** The layer's own MCP server's name in a layered arm's extension root (`src/frontier/layer/bench.ts`). */
const LAYER_SERVER = "jocasta";

/** omp's `enabled` as it reads it (`discovery/builtin.ts`): false, or the strings "false" and "0", turn a server off. */
function turnedOff(config: unknown): boolean {
	const enabled = asObject(config)?.enabled;
	return enabled === false || (typeof enabled === "string" && ["false", "0"].includes(enabled.toLowerCase()));
}

/**
 * 0587: why the omp arms can't run with omp's folder `agentDir` (the profile's with `--profile`) as it is, or
 * undefined. omp starts every server its `mcp.json` and `.mcp.json` list that's on (`enabled` not false, or named in
 * `enabledServers`; not named in `disabledServers`) in every trial. For the `layered` arms, a `jocasta` entry there,
 * even one turned off, or `jocasta` in `disabledServers` hides the layer's own server: omp keeps its folder's over an
 * extension's (`discovery/omp-plugins.ts`). A file that's missing or isn't JSON gives omp nothing.
 */
export function mcpProblem(agentDir: string, layered: readonly string[]): string | undefined {
	const strings = (value: unknown) =>
		Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
	const files = ["mcp.json", ".mcp.json"].flatMap((name) => {
		const file = join(agentDir, name);
		let config: Record<string, unknown> | undefined;
		try {
			config = asObject(JSON.parse(readFileSync(file, "utf8")));
		} catch {
			return [];
		}
		if (config === undefined) return [];
		return [
			{
				file,
				servers: asObject(config.mcpServers) ?? {},
				disabled: strings(config.disabledServers),
				forced: strings(config.enabledServers),
			},
		];
	});
	const disabled = new Set(files.flatMap((each) => each.disabled));
	const forced = new Set(files.flatMap((each) => each.forced));
	const loaded = files
		.map(({ file, servers }) => ({
			file,
			names: Object.entries(servers)
				.filter(([name, config]) => !disabled.has(name) && (!turnedOff(config) || forced.has(name)))
				.map(([name]) => name),
		}))
		.filter(({ names }) => names.length > 0);
	if (loaded.length > 0) {
		const where = loaded.map(({ file, names }) => `${file}: ${names.join(", ")}`).join("; ");
		return `the omp arms would load MCP servers from ${where}. Remove them (jocasta uninstall removes Jocasta's) or use a --profile without them; no trial ran.`;
	}
	if (layered.length === 0) return undefined;
	const entry = files.find(({ servers }) => LAYER_SERVER in servers);
	const denied = files.find((each) => each.disabled.includes(LAYER_SERVER));
	const why =
		entry !== undefined
			? `${entry.file} has a "${LAYER_SERVER}" entry, which hides the layer's even turned off`
			: denied !== undefined
				? `${denied.file} names "${LAYER_SERVER}" in disabledServers`
				: undefined;
	if (why === undefined) return undefined;
	return `${layered.join(", ")} would lose the layer's own MCP server "${LAYER_SERVER}": ${why}. Remove it or use a --profile without it; no trial ran.`;
}

async function main(): Promise<void> {
	const argv = process.argv.slice(2);
	if (argv.includes("--runs")) throw new BenchUsageError("unknown option: --runs (use --reps)");
	const args = parseBenchArgs(argv, {
		defaultRuns: 3,
		valued: [
			"--tasks",
			"--cache",
			"--arms",
			"--reps",
			"--set",
			"--study",
			"--only",
			"--workers",
			"--seed",
			"--timeout",
			"--profile",
			"--work",
			"--pty-session",
		],
		booleans: ["--pilot", "--validate"],
	});
	const tasksFile = flag(args, "tasks");
	const cacheText = flag(args, "cache");
	if (tasksFile === undefined || cacheText === undefined || args.out === undefined)
		throw new BenchUsageError("--tasks <tasks.jsonl>, --cache <dir> and --out <runDir> are required");
	const cacheDir = resolve(cacheText);
	if (isInside(REPO_ROOT, cacheDir)) throw new BenchUsageError(`--cache must be outside the repo, not ${cacheDir}`);
	const outDir = args.out;
	const workText = flag(args, "work");
	const workDir = workText === undefined ? undefined : resolve(workText);
	if (workDir !== undefined && isInside(REPO_ROOT, workDir))
		throw new BenchUsageError(`--work must be outside the repo, not ${workDir}`);
	const pilot = args.flags.get("pilot") === true;
	const validate = args.flags.get("validate") === true;
	const study = flag(args, "study", "main");
	if (study !== "main" && study !== "followup")
		throw new BenchUsageError(`--study must be main or followup, not "${study}"`);
	if (study === "followup")
		for (const name of ["arms", "reps", "set", "only"])
			if (flag(args, name) !== undefined)
				throw new BenchUsageError(`--study followup fixes its own --${name}; drop it`);
	if (study === "followup" && pilot) throw new BenchUsageError("--study followup has no pilot; drop --pilot");
	if (pilot && flag(args, "reps") !== undefined) throw new BenchUsageError("--pilot runs 1 rep; drop --reps");
	const reps = study === "followup" ? FOLLOWUP_REPS : pilot ? 1 : wholeNumber(flag(args, "reps"), "reps", 3);
	const workers = wholeNumber(flag(args, "workers"), "workers", 2);
	const seed = wholeNumber(flag(args, "seed"), "seed", 482, 0);
	const timeoutText = flag(args, "timeout", "90");
	const timeoutMinutes = Number(timeoutText);
	if (!Number.isFinite(timeoutMinutes) || timeoutMinutes <= 0)
		throw new BenchUsageError(`--timeout needs a number of minutes above 0, not "${timeoutText}"`);
	const set = flag(args, "set", "main");
	if (set !== "main" && set !== "spare" && set !== "all")
		throw new BenchUsageError(`--set must be main, spare or all, not "${set}"`);
	const profileText = flag(args, "profile");
	const profile = profileText === undefined ? undefined : profileName(profileText);
	const armNames = study === "followup" ? [...FOLLOWUP_ARMS] : listFlag(args, "arms", DEFAULT_ARMS);
	const variants = new Map<string, Variant>();
	for (const name of armNames) variants.set(name, loadVariants(VARIANTS_DIR, [name]));
	if (armNames.length === 0) throw new BenchUsageError("--arms needs at least one arm");

	const all = loadTasks(resolve(tasksFile));
	const only = listFlag(args, "only", []);
	for (const id of only)
		if (!all.some((task) => task.instance_id === id)) throw new BenchUsageError(`no task ${id} in ${tasksFile}`);
	let tasks =
		only.length > 0
			? all.filter((task) => only.includes(task.instance_id))
			: all.filter((task) => set === "all" || task.set === set);
	if (study === "followup") tasks = pickFollowupTasks(all, seed);
	if (pilot) tasks = pickPilot(tasks, seed);
	if (tasks.length === 0) throw new BenchUsageError("no tasks selected");
	// EVL-1: each layered arm's layer is checked before any trial (its code index's data folder, codebase-memory-mcp).
	const layerProblems = new Map<string, string | undefined>();
	for (const name of armNames) {
		const switches = variants.get(name)?.layer;
		if (switches === undefined) continue;
		layerProblems.set(name, layerBuilt() ? await (await loadBenchLayer()).preflight(switches) : layerMissingMessage());
	}
	// 0587: omp's folder gives the omp arms no MCP server of its own and doesn't hide a layered arm's (dry run too).
	const ompArms = armNames.filter((name) => variants.get(name)?.harness === "omp");
	if (!validate && ompArms.length > 0) {
		const layered = ompArms.filter((name) => variants.get(name)?.layer !== undefined);
		const problem = mcpProblem(ompAgentDir(profile), layered);
		if (problem !== undefined) throw new BenchUsageError(problem);
	}
	if (args.dryRun) {
		printDryRun({
			tasks,
			armNames,
			variants,
			layerProblems,
			reps,
			seed,
			workers,
			timeoutMinutes,
			profile,
			pilot,
			validate,
			study,
			ptySession: flag(args, "pty-session", BENCH_PTY_SESSION),
			cacheDir,
			workDir,
			outDir,
		});
		return;
	}
	if (validate) {
		const failed = await runValidation(tasks, cacheDir, outDir, undefined, workDir);
		if (failed.length > 0) process.exitCode = 1;
		return;
	}
	if (!process.env.JOCASTA_EVALS_REAL)
		throw new BenchUsageError("the benchmark runs real models and bills usage; set JOCASTA_EVALS_REAL=1 to run it");
	if (process.env.CI) throw new BenchUsageError("real-model runs never run in CI (0169)");
	for (const [name, problem] of layerProblems)
		if (problem !== undefined) throw new BenchUsageError(`${name}: ${problem}`);
	const a7 = armNames.filter((name) => variants.get(name)?.harness === "opencode");
	if (a7.length > 0) {
		if (hostname().toUpperCase() !== VM_MACHINE)
			throw new BenchUsageError(`${a7.join(", ")} runs in the VM ${VM_MACHINE} only (0537), not on this machine`);
		if (!process.env[A7_API_KEY_ENV])
			throw new BenchUsageError(`${a7.join(", ")} needs A7's Anthropic API key in ${A7_API_KEY_ENV} (0537)`);
	}
	const arms: Arm[] = armNames.map((name) => {
		const variant = variants.get(name) as Variant;
		const launcher = toolCommand(variant.harness);
		let version: string;
		try {
			version = readVersion(launcher);
		} catch (error) {
			throw new BenchUsageError(`arm ${name}: ${(error as Error).message}`);
		}
		return { name, variant, launcher, harnessVersion: version };
	});
	const ptySession = flag(args, "pty-session", BENCH_PTY_SESSION);
	const interactive = armNames.filter((name) => variants.get(name)?.planReview !== undefined);
	if (interactive.length > 0 && !findHerdr())
		throw new BenchUsageError(
			`${interactive.join(", ")} answer omp's plan review in a herdr pane: herdr is not on PATH`,
		);
	const pauses: Pause[] = [];
	const pacer = createPacer(realPacerDeps, pauses);
	const code = await runBench({
		tasks,
		cacheDir,
		outDir,
		arms,
		reps,
		seed,
		workers,
		timeoutMs: timeoutMinutes * 60_000,
		profile,
		pilot,
		workDir,
		ptySession,
		pace: pacer.pace,
		accountFull: pacer.accountFull,
		pauses,
		// 0574: a harness that updated itself mid-run stops its arm.
		versionOf: (arm) => readVersion(arm.launcher),
	});
	console.log(`results: ${join(outDir, "results.jsonl")}`);
	process.exitCode = code;
}

function printDryRun(plan: {
	tasks: BenchTask[];
	armNames: string[];
	variants: Map<string, Variant>;
	/** Each layered arm's preflight: why it can't run here, or undefined. */
	layerProblems: Map<string, string | undefined>;
	reps: number;
	seed: number;
	workers: number;
	timeoutMinutes: number;
	profile?: string;
	pilot: boolean;
	validate: boolean;
	study: "main" | "followup";
	ptySession: string;
	cacheDir: string;
	workDir?: string;
	outDir: string;
}): void {
	const quote = (arg: string) => (/\s/.test(arg) ? JSON.stringify(arg) : arg);
	console.log(
		`task benchmark (0482, 0543${plan.study === "followup" ? ": the follow-up study" : ""}) dry run: nothing is fetched, prepared or run, and no model is called.`,
	);
	console.log(
		`  tasks: ${plan.tasks.length}${plan.pilot ? ` (pilot, seed ${plan.seed}: 2 Python, 2 TypeScript/JavaScript, 1 Go)` : ""}${plan.study === "followup" ? ` (${FOLLOWUP_TASKS} main tasks, seed ${plan.seed}, languages in the main set's proportions)` : ""}`,
	);
	for (const task of plan.tasks) console.log(`    ${task.instance_id} (${task.language}, ${task.set})`);
	if (plan.validate) {
		console.log(
			`  validate: per task, the hidden tests on the base commit (must fail), then with gold.patch ${GOLD_GRADES} times (must pass)`,
		);
	} else {
		const order = planTrials(plan.tasks, plan.armNames, plan.reps, plan.seed);
		console.log(
			`  trials: ${order.length} = ${plan.tasks.length} tasks × ${plan.armNames.length} arms × ${plan.reps} rep${plan.reps === 1 ? "" : "s"}, seed ${plan.seed}, ${plan.workers} at once, ${plan.timeoutMinutes} min limit each`,
		);
		if (plan.study === "followup") {
			console.log("  comparisons, each paired with a6 by task and rep, kept by 0543's rule (`bun evals/keep.ts`):");
			for (const c of FOLLOWUP_COMPARISONS)
				console.log(
					`    ${c.arm}: ${c.change} (${c.records}; the feature is ${c.feature === "arm" ? "the arm's" : "a6's"})`,
				);
			console.log("    not compared: pitchfork (0560), keep-alive pings (no idle gap in a trial)");
		}
		console.log(`  order (first ${Math.min(15, order.length)}):`);
		for (const trial of order.slice(0, 15))
			console.log(`    ${trial.index + 1}. ${trial.task} ${trial.arm} rep ${trial.rep}`);
		for (const name of plan.armNames) {
			const variant = plan.variants.get(name) as Variant;
			const turns = ["<prompt>", ...variant.followups.map((_, i) => `<follow-up ${i + 1}>`)];
			turns.forEach((prompt, i) => {
				const args = trialArgs(variant, {
					prompt,
					followUp: i > 0,
					overlay: "<trial>/eval-run.yml",
					profile: plan.profile,
					sessionDir: "<trial>/sessions",
					session: "<session>",
					...(variant.planReview !== undefined && i === 0 ? { extension: "<trial>/plan-review.js" } : {}),
				});
				console.log(`  arm ${name} turn ${i + 1}: ${variant.harness} ${args.map(quote).join(" ")}`);
			});
			if (variant.planReview !== undefined)
				console.log(
					`  arm ${name} plan review (0567): turn 1 runs omp interactive in a pane of the herdr session ${plan.ptySession}, plan mode at startup; the extension answers "${variant.planReview === "execute" ? "Approve and execute" : "Approve and keep context"}" and shuts omp down after the execution`,
				);
			if (variant.layer !== undefined) {
				const problem = plan.layerProblems.get(name);
				console.log(
					`  arm ${name} layer: index ${variant.layer.index ? "on" : "off"}; ${problem === undefined ? `${LAYER_MODULE} adds its flags, settings and env` : `NOT RUNNABLE: ${problem}`}`,
				);
			}
			if (variant.harness === "opencode")
				console.log(
					`  arm ${name}: Anthropic API key from ${A7_API_KEY_ENV} (as ANTHROPIC_API_KEY, opencode only), VM ${VM_MACHINE} only, stops at $${A7_SPEND_LIMIT_USD} priced spend (0537)`,
				);
		}
		console.log(
			`  quota pacer: omp usage --json before each trial, waits only while every ${PACER_PROVIDER} account has its 5h or 7d window at ${PAUSE_AT * 100}% or more; a trial that ended on a rate limit while an account is full is an infra error, retried`,
		);
		console.log(
			"  ledger: each trial's sessions saved to <out>/sessions/, priced with bench/prices/ (LiteLLM, pinned), rows in ledger.jsonl, totals in each results row, spend per attempt in spend.jsonl",
		);
		console.log(`  profile: ${plan.profile ?? "his default"} (omp arms)`);
		if (plan.study === "main" && !plan.pilot)
			console.log(
				`  follow-up study (after the main run, --study followup): ${FOLLOWUP_TASKS} tasks × ${FOLLOWUP_REPS} reps × ${FOLLOWUP_ARMS.length} arms (${FOLLOWUP_ARMS.join(", ")}) = ${FOLLOWUP_TASKS * FOLLOWUP_REPS * FOLLOWUP_ARMS.length} trials`,
			);
	}
	console.log(`  cache: ${plan.cacheDir}`);
	console.log(`  trial roots: ${plan.workDir ?? `${tmpdir()} (the OS temp folder; --work sets a short root)`}`);
	console.log(`  results: ${plan.outDir}`);
}

if (isMainThread && import.meta.main) await runScript(main);
