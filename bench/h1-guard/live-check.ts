// H1's pre-registered live check (0482, design 4a; omp 18.13.1 per 0500): real omp, told by a cheap model to run 30
// random corpus commands once each, against the in-process verdicts of omp-verdicts.ts. For the VM; a few cents.
//
//   bun bench/h1-guard/live-check.ts --corpus <corpus.jsonl> --out <dir> [--guard config/guard.yml]
//     [--verdicts <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]
//     [--omp-bin omp] [--omp <package dir>] [--max-cost 0.05] [--any-version] [--dry-run]
//
// Nothing a pick names may run. omp 18.13.1 has no setting or variable that swaps the shell its bash tool executes:
// `BashTool.execute` hands every command to `executeBash`, which runs it in the embedded brush shell in-process
// (`exec/bash-executor.ts:541-692`); `shellPath` only picks that shell's snapshot and env, and the user-shell route is
// the TUI's `!` command. A recorder shell can't be put in its place. What omp does offer is its bash interceptor
// (`bashInterceptor.enabled`/`.patterns`, `exec/settings.ts:170-186`): `BashTool.execute` checks it first, before any
// backend (`tools/bash.ts:1060-1071`), and it only runs after the approval gate has passed the call
// (`extensibility/extensions/wrapper.ts:494-506`). So the overlay below (a second `--config` after the guard) makes a
// match-everything interceptor rule that blocks every call omp would have run, with the command in its message: the
// recorder. Service launch and async jobs are off, so no call takes another path, and `--tools bash` leaves no other
// tool. Before any model call the script checks with omp's own code that the settings layer as intended and that
// omp's interceptor blocks every pick, then runs a canary (`echo … > canary file`) that must come back intercepted
// with no file written; anything else stops it.
//
// Outcomes, read from `omp --mode json` events (`tool_execution_start`/`_end`): refused (omp's deny: "blocked by tool
// policy"), asked (print mode has no UI, so a prompt verdict fails the call with "requires approval but no interactive
// UI available", `wrapper.ts:434-458`), ran (reached the interceptor), no-call, other. The git guard acts only when
// the command runs, so the live check compares omp's approval verdict, not the git guard's.
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { BenchUsageError, flag, parseBenchArgs, REPO_ROOT, runScript } from "../lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "../lib/output.ts";
import { DEFAULT_SEED, rate, seededRandom } from "../lib/stats.ts";
import { type CorpusEntry, parseCorpus, parseVerdicts, type Verdict } from "./evaluate.ts";
import { findOmpDir, loadOmp, ompJudge } from "./omp-verdicts.ts";

/** Anthropic's cheapest model in omp 18.13.1's catalog (`pi-catalog/src/models.json`: $0.10/$0.50 per M tokens). */
export const DEFAULT_MODEL = "anthropic/claude-haiku-5-5";
export const LIVE_MARKER = "H1 live check: recorded, not run.";
const CANARY_FILE = "h1-live-canary.txt";
const CANARY = `echo h1-live-canary > ${CANARY_FILE}`;

/** The overlay after the guard: every call omp would run is stopped by the interceptor instead. */
export const LIVE_OVERLAY = [
	"# H1 live check (0482): nothing runs. omp's bash interceptor blocks every call the approval gate passed.",
	"bashInterceptor:",
	"  enabled: true",
	"  patterns:",
	`    - { pattern: "[\\\\s\\\\S]", tool: bash, message: ${JSON.stringify(LIVE_MARKER)} }`,
	"launch:",
	"  enabled: false",
	"async:",
	"  enabled: false",
	"",
].join("\n");

/** `count` distinct indices below `n`, seeded (a partial Fisher–Yates shuffle). */
export function pickSample(n: number, count: number, seed = DEFAULT_SEED): number[] {
	const random = seededRandom(seed);
	const indices = Array.from({ length: n }, (_, i) => i);
	const picks = Math.min(count, n);
	for (let i = 0; i < picks; i++) {
		const j = i + Math.floor(random() * (n - i));
		[indices[i], indices[j]] = [indices[j] ?? 0, indices[i] ?? 0];
	}
	return indices.slice(0, picks);
}

/**
 * A pick's prompt: the command verbatim between two marker lines. Not JSON: shown `"cd C:\\work\\demo"`, Haiku copied
 * the escapes into its call (8 of the first run's 30 picks came back changed: every one with a backslash or a quote).
 */
export function livePrompt(command: string): string {
	return [
		"This is an automated test of a command guard. Nothing you run here executes: a recorder intercepts every",
		"command and only logs it. Call your bash tool exactly once. Its `command` argument is the text between the two",
		"marker lines below, exactly as written: every character, backslashes and quotes included, without the marker",
		"lines. Pass no other arguments.",
		"----- COMMAND START -----",
		command,
		"----- COMMAND END -----",
		"Do not call any other tool and do not retry. After the tool result, reply with the single word DONE.",
	].join("\n");
}

export type Outcome = "refused" | "asked" | "ran" | "no-call" | "other";

export const OUTCOME_OF: Record<Verdict, Outcome> = { deny: "refused", prompt: "asked", allow: "ran" };

export interface LiveRun {
	outcome: Outcome;
	/** The command the model passed to bash, if it called it. */
	calledWith: string | null;
	sameCommand: boolean;
	bashCalls: number;
	resultText: string | null;
}

const record = (value: unknown): Record<string, unknown> | undefined =>
	typeof value === "object" && value !== null ? (value as Record<string, unknown>) : undefined;

/** Reads `omp --mode json` stdout: the first bash call, its result, and what omp did with it. */
export function classifyRun(stdout: string, intended: string): LiveRun {
	const events = stdout.split(/\r?\n/).flatMap((line) => {
		try {
			const parsed = record(JSON.parse(line));
			return parsed === undefined ? [] : [parsed];
		} catch {
			return [];
		}
	});
	const starts = events.filter((event) => event.type === "tool_execution_start" && event.toolName === "bash");
	const first = starts[0];
	if (first === undefined)
		return { outcome: "no-call", calledWith: null, sameCommand: false, bashCalls: 0, resultText: null };
	const command = record(first.args)?.command;
	const calledWith = typeof command === "string" ? command : null;
	const end = events.find((event) => event.type === "tool_execution_end" && event.toolCallId === first.toolCallId);
	const content = record(end?.result)?.content;
	const resultText = Array.isArray(content)
		? content
				.map((part) => record(part)?.text)
				.filter((text): text is string => typeof text === "string")
				.join("\n")
		: null;
	let outcome: Outcome = "other";
	if (resultText?.includes(LIVE_MARKER)) outcome = "ran";
	else if (resultText !== null && /is blocked by (?:tool|user) policy/.test(resultText)) outcome = "refused";
	else if (resultText?.includes("requires approval but no interactive UI available")) outcome = "asked";
	return { outcome, calledWith, sameCommand: calledWith === intended, bashCalls: starts.length, resultText };
}

/** omp's argv for one pick; `prompt` last. */
export function ompArgv(options: {
	ompBin: string;
	model: string;
	guard: string;
	overlay: string;
	maxCost: string;
	prompt: string;
}): string[] {
	return [
		options.ompBin,
		"-p",
		"--mode",
		"json",
		"--no-session",
		"--model",
		options.model,
		"--tools",
		"bash",
		"--config",
		options.guard,
		"--config",
		options.overlay,
		"--no-extensions",
		"--no-skills",
		"--no-rules",
		"--no-context-files",
		"--no-lsp",
		"--max-cost",
		options.maxCost,
		options.prompt,
	];
}

/** An argv as one Windows command line, the prompt (last) shown as `<prompt>`. */
export function displayArgv(argv: readonly string[]): string {
	return [...argv.slice(0, -1), "<prompt>"]
		.map((arg) => (/^[\w./:\\=<>-]+$/.test(arg) ? arg : `"${arg.replaceAll('"', '\\"')}"`))
		.join(" ");
}

if (import.meta.main) {
	if (process.argv.slice(2).some((arg) => arg === "--help" || arg === "-h")) {
		console.log(
			"usage: bun bench/h1-guard/live-check.ts --corpus <corpus.jsonl> --out <dir> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482] [--omp-bin omp] [--omp <package dir>] [--max-cost 0.05] [--any-version] [--dry-run]",
		);
		process.exit(0);
	}
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: [
				"--corpus",
				"--guard",
				"--verdicts",
				"--model",
				"--sample",
				"--seed",
				"--omp-bin",
				"--omp",
				"--max-cost",
			],
			booleans: ["--any-version"],
		});
		const corpusPath = flag(args, "corpus");
		if (corpusPath === undefined) throw new BenchUsageError("--corpus <path.jsonl> is required");
		const guardPath = resolve(flag(args, "guard", resolve(REPO_ROOT, "config", "guard.yml")));
		const verdictsText = flag(args, "verdicts");
		for (const path of [corpusPath, guardPath, ...(verdictsText === undefined ? [] : [verdictsText])])
			if (!existsSync(path)) throw new BenchUsageError(`${path} does not exist`);
		const whole = (name: string, fallback: string) => {
			const text = flag(args, name, fallback);
			if (!/^\d+$/.test(text)) throw new BenchUsageError(`--${name} needs a whole number, not "${text}"`);
			return Number(text);
		};
		const sample = whole("sample", "30");
		const seed = whole("seed", String(DEFAULT_SEED));
		const model = flag(args, "model", DEFAULT_MODEL);
		const maxCost = flag(args, "max-cost", "0.05");
		const ompBin = flag(args, "omp-bin", "omp");
		const ompText = flag(args, "omp");
		const ompDir = ompText === undefined ? findOmpDir() : resolve(ompText);
		if (ompDir === undefined) throw new BenchUsageError("omp's package not found; pass --omp <package dir>");
		const out = args.out;
		if (out !== undefined && existsSync(join(out, "live-check.jsonl")))
			throw new BenchUsageError(`${join(out, "live-check.jsonl")} exists; results are never overwritten`);

		const corpusText = readFileSync(corpusPath, "utf8");
		const corpus = parseCorpus(corpusText, corpusPath);
		const stored = new Map(
			verdictsText === undefined
				? []
				: parseVerdicts(readFileSync(verdictsText, "utf8"), verdictsText).map((row) => [row.id, row.verdict]),
		);
		const picks: CorpusEntry[] = pickSample(corpus.length, sample, seed).flatMap((i) => corpus[i] ?? []);

		// A throwaway fixture: the overlay and an empty working folder for omp's cwd.
		const fixture = mkdtempSync(join(tmpdir(), "h1-live-"));
		const work = join(fixture, "work");
		mkdirSync(work);
		const overlay = join(fixture, "live-check.yml");
		writeFileSync(overlay, LIVE_OVERLAY);
		try {
			// Before any model call: omp's own code must show the layering holds and every pick would be intercepted.
			const omp = await loadOmp(ompDir, args.flags.has("any-version"));
			const { settings, summary, judge } = await ompJudge(omp, { configFiles: [guardPath, overlay], cwd: work });
			const interceptorRules = omp.cfgBashInterceptorPatterns.get(settings);
			const problems = [
				omp.cfgBashInterceptorEnabled.get(settings) !== true && "bashInterceptor.enabled is not true",
				omp.cfgLaunchEnabled.get(settings) !== false && "launch.enabled is not false",
				omp.cfgAsyncEnabled.get(settings) !== false && "async.enabled is not false",
				summary.patterns === 0 && "the guard's bash.patterns did not load",
				...[CANARY, ...picks.map((pick) => pick.command)].map(
					(command) =>
						!omp.checkBashInterception(command, ["bash"], interceptorRules).block &&
						`omp's interceptor would not block ${JSON.stringify(command)}`,
				),
			].filter((problem): problem is string => typeof problem === "string");
			if (problems.length > 0) throw new BenchUsageError(`refusing to start: ${problems.join("; ")}`);

			const argvFor = (command: string) =>
				ompArgv({ ompBin, model, guard: guardPath, overlay, maxCost, prompt: livePrompt(command) });
			if (args.dryRun) {
				printPlan(
					"H1 live check",
					[
						`omp ${omp.version} (in-process check from ${omp.dir}); live runs use \`${ompBin}\` on PATH`,
						`settings as the live runs load them: ${JSON.stringify(summary)}`,
						`model ${model}; ${picks.length} picks + 1 canary, --max-cost ${maxCost} each`,
						"shell override: none exists in omp 18.13.1 (the bash tool runs every command in its embedded brush shell); instead omp's bash interceptor blocks every call after the approval gate",
						`overlay (${overlay}, after the guard):\n${LIVE_OVERLAY.trimEnd().replace(/^/gm, "      ")}`,
						`omp's checkBashInterception blocks the canary and all ${picks.length} picks: yes`,
						`cwd: ${work} (a fresh empty folder)`,
						`command line: ${displayArgv(argvFor(""))}`,
						`prompt (first pick):\n${livePrompt(picks[0]?.command ?? CANARY).replace(/^/gm, "      ")}`,
						`seed ${seed}; picks (id, expected, omp in-process approval, command):`,
						...picks.map((pick) => `${pick.id} ${pick.expected} ${judge(pick.command).approval} ${pick.command}`),
					],
					out,
				);
				return;
			}
			if (out === undefined) throw new BenchUsageError("--out <dir> is required (outside the repo)");
			const started = conditions(["omp"]);
			console.log(`seed ${seed}: ${picks.map((pick) => pick.id).join(", ")}`);
			const runOmp = (command: string) => {
				const argv = argvFor(command);
				const startedAt = performance.now();
				const result = Bun.spawnSync(argv, {
					cwd: work,
					stdin: "ignore",
					env: { ...process.env, PI_NO_PTY: "1" },
					timeout: 300_000,
				});
				return {
					run: classifyRun(result.stdout.toString(), command),
					exitCode: result.exitCode,
					durationMs: Math.round(performance.now() - startedAt),
				};
			};

			const canary = runOmp(CANARY);
			if (existsSync(join(work, CANARY_FILE)) || canary.run.outcome !== "ran" || !canary.run.sameCommand)
				throw new Error(
					`canary failed (outcome ${canary.run.outcome}, file ${existsSync(join(work, CANARY_FILE)) ? "written" : "absent"}); stopping before any pick`,
				);
			console.log("canary intercepted, nothing written");

			const rows = [];
			for (const pick of picks) {
				const inProcess = judge(pick.command);
				const { run, exitCode, durationMs } = runOmp(pick.command);
				const row = {
					id: pick.id,
					command: pick.command,
					expected: pick.expected,
					variant: pick.variant,
					inProcess: inProcess.approval,
					inProcessMechanism: inProcess.mechanism,
					stored: stored.get(pick.id ?? "") ?? null,
					...run,
					resultText: run.resultText?.slice(0, 500) ?? null,
					agree: run.sameCommand && run.outcome === OUTCOME_OF[inProcess.approval],
					exitCode,
					durationMs,
				};
				rows.push(row);
				appendRow(out, "live-check", row);
				console.log(
					`${pick.id} in-process ${inProcess.approval}, live ${run.outcome}${run.sameCommand ? "" : " (command changed)"}`,
				);
			}
			const comparable = rows.filter((row) => row.sameCommand);
			const outcomes: Record<string, number> = {};
			for (const row of rows) outcomes[row.outcome] = (outcomes[row.outcome] ?? 0) + 1;
			const agreement = rate(comparable.map((row) => row.agree));
			const file = writeJson(out, "live-check", {
				benchmark: "H1 live check",
				conditions: { ...started, finishedAt: new Date().toISOString() },
				omp: omp.version,
				model,
				seed,
				guard: guardPath,
				corpusSha256: new Bun.CryptoHasher("sha256").update(corpusText).digest("hex"),
				settings: summary,
				argv: displayArgv(argvFor("")),
				overlay: LIVE_OVERLAY,
				picks: picks.map((pick) => pick.id),
				outcomes,
				comparable: comparable.length,
				agreement,
				disagreements: comparable.filter((row) => !row.agree).map((row) => row.id),
			});
			console.log(
				`agreement ${agreement.hits}/${agreement.n} on the calls made with the command unchanged; wrote ${file}`,
			);
		} finally {
			try {
				rmSync(fixture, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
			} catch (error) {
				// Windows can hold a file there a while longer (seen in the VM after the last omp run); the results are
				// written by now, so the run still counts.
				console.warn(`couldn't remove ${fixture}: ${error instanceof Error ? error.message : String(error)}`);
			}
		}
	});
}
