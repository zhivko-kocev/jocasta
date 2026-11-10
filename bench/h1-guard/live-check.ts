// H1's pre-registered live check (0482, design 4a; omp 18.18.6 per 0535 and 0589, recorder per 0585): real omp, told
// by a cheap model to run 30 random corpus commands once each, against the in-process verdicts of omp-verdicts.ts. For
// the VM; a few cents.
//
//   bun bench/h1-guard/live-check.ts --corpus <corpus.jsonl> --out <dir> [--guard config/guard.yml]
//     [--verdicts <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]
//     [--omp-bin omp] [--omp <package dir>] [--max-cost 0.05] [--any-version] [--dry-run]
//
// Nothing a pick names may run. Each pick is one turn of `omp --mode acp` driven by this script as its ACP client
// (`acp-recorder.ts`): the client advertises a terminal, so every bash call omp's approval let through is sent to the
// client's `terminal/create`, which records it and runs nothing; a prompt verdict reaches the client as
// `session/request_permission`, which it rejects once; a deny ends the call with omp's policy text. The overlay (a second
// `--config` after the guard) sets `tools.approvalMode: yolo` explicitly (the schema's implicit yolo makes ACP ask for
// every bash call; explicit yolo keeps a pane's gate: deny refuses, prompt asks, allow runs, `session/session-tools.ts:
// 1195-1213`) and turns service launch and async jobs off; the recorder extension (`-e`) keeps every bash call on the
// client terminal. Before any model call the script checks with omp's own code that the settings layer so, that the
// interceptor is off, that this omp's bash tool has the client-terminal route and its ACP gate a reject-once option;
// an omp without them is refused. Then a canary (`echo … > canary file`) must reach the client's terminal with no file
// written; anything else stops it.
//
// Outcomes per pick: refused (omp's deny, "is blocked by tool policy"), asked (a permission request for the call),
// ran (the call reached the client's terminal), no-call, other. The git guard acts only when omp's own shell runs the
// command, so the live check compares omp's approval verdict, not the git guard's.
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { removeTree } from "../../src/shared/fsx.ts";
import { BenchUsageError, flag, parseBenchArgs, REPO_ROOT, runScript } from "../lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "../lib/output.ts";
import { DEFAULT_SEED, rate, seededRandom } from "../lib/stats.ts";
import { classifyTurn, LIVE_MARKER, type Outcome, RECORDER_EXTENSION, runAcpTurn } from "./acp-recorder.ts";
import { type CorpusEntry, parseVerdicts, readCorpus, type Verdict } from "./evaluate.ts";
import { findOmpDir, loadOmp, ompJudge } from "./omp-verdicts.ts";

export { LIVE_MARKER };

/** Anthropic's cheapest model in omp 18.18.6's catalog (`pi-catalog/src/models.json`: $0.10/$0.50 per M tokens). */
export const DEFAULT_MODEL = "anthropic/claude-haiku-5-5";
const CANARY_FILE = "h1-live-canary.txt";
const CANARY = `echo h1-live-canary > ${CANARY_FILE}`;

/** The overlay after the guard: a pane's approval gate under ACP, and no bash route but the client's terminal. */
export const LIVE_OVERLAY = [
	"# H1 live check (0585): approvals as in a pane; nothing runs, the ACP client's terminal records every call.",
	"tools:",
	"  approvalMode: yolo",
	"launch:",
	"  enabled: false",
	"async:",
	"  enabled: false",
	"",
].join("\n");

/** What the recorder needs of omp and its layered settings, as omp's own code reports them. */
export interface RecorderState {
	version: string;
	clientTerminalRoute: boolean;
	permissionKinds: string[];
	approvalModeConfigured: boolean;
	approvalMode: string;
	interceptorEnabled: boolean;
	launchEnabled: boolean;
	asyncEnabled: boolean;
	patterns: number;
}

/** Why the recorder can't record this omp with these settings; empty when it can. */
export function recorderProblems(state: RecorderState): string[] {
	return [
		!state.clientTerminalRoute &&
			`omp ${state.version}'s bash tool has no ACP client-terminal route, so the recorder can't see what it runs`,
		!state.permissionKinds.includes("reject_once") &&
			`omp ${state.version}'s ACP permission gate offers no reject_once, so a prompt can't be declined once`,
		(!state.approvalModeConfigured || state.approvalMode !== "yolo") &&
			"tools.approvalMode isn't set to yolo explicitly (ACP would ask for every bash call)",
		state.interceptorEnabled && "bashInterceptor.enabled is on (it refuses in the bash tool's own approval)",
		state.launchEnabled && "launch.enabled is not false",
		state.asyncEnabled && "async.enabled is not false",
		state.patterns === 0 && "the guard's bash.patterns did not load",
	].filter((problem): problem is string => typeof problem === "string");
}

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

export const OUTCOME_OF: Record<Verdict, Outcome> = { deny: "refused", prompt: "asked", allow: "ran" };

/**
 * omp's argv for the live turns: ACP over stdio, the pick's prompt sent as `session/prompt`. ACP makes a session file
 * per `session/new` whatever `--no-session` says, so `--session-dir` keeps them in the run's throwaway folder.
 */
export function ompArgv(options: {
	ompBin: string;
	model: string;
	guard: string;
	overlay: string;
	extension: string;
	sessionDir: string;
	maxCost: string;
}): string[] {
	return [
		options.ompBin,
		"--mode",
		"acp",
		"--session-dir",
		options.sessionDir,
		"--model",
		options.model,
		"--tools",
		"bash",
		"--config",
		options.guard,
		"--config",
		options.overlay,
		"--no-extensions",
		"-e",
		options.extension,
		"--no-skills",
		"--no-rules",
		"--no-context-files",
		"--no-lsp",
		"--max-cost",
		options.maxCost,
	];
}

/** An argv as one Windows command line. */
export function displayArgv(argv: readonly string[]): string {
	return argv.map((arg) => (/^[\w./:\\=<>-]+$/.test(arg) ? arg : `"${arg.replaceAll('"', '\\"')}"`)).join(" ");
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

		const { entries: corpus, sha256: corpusSha256 } = readCorpus(corpusPath);
		const stored = new Map(
			verdictsText === undefined
				? []
				: parseVerdicts(readFileSync(verdictsText, "utf8"), verdictsText).map((row) => [row.id, row.verdict]),
		);
		const picks: CorpusEntry[] = pickSample(corpus.length, sample, seed).flatMap((i) => corpus[i] ?? []);

		// A throwaway fixture: the overlay, the recorder extension, omp's session folder and an empty working folder.
		const fixture = mkdtempSync(join(tmpdir(), "h1-live-"));
		const work = join(fixture, "work");
		mkdirSync(work);
		const overlay = join(fixture, "live-check.yml");
		writeFileSync(overlay, LIVE_OVERLAY);
		const extension = join(fixture, "h1-live-recorder.js");
		writeFileSync(extension, RECORDER_EXTENSION);
		const sessionDir = join(fixture, "sessions");
		try {
			// Before any model call: omp's own code must show the settings layer as the recorder needs and this omp has
			// the client-terminal route and a reject-once permission option (0585); an omp without them is refused.
			const omp = await loadOmp(ompDir, args.flags.has("any-version"));
			const { settings, summary, judge } = await ompJudge(omp, { configFiles: [guardPath, overlay], cwd: work });
			const problems = recorderProblems({
				version: omp.version,
				clientTerminalRoute: omp.clientTerminalRoute,
				permissionKinds: omp.acpPermissionOptions.map((option) => option.kind),
				approvalModeConfigured: omp.cfgToolsApprovalMode.isConfigured(settings),
				approvalMode: summary.approvalMode,
				interceptorEnabled: omp.cfgBashInterceptorEnabled.get(settings) === true,
				launchEnabled: omp.cfgLaunchEnabled.get(settings) !== false,
				asyncEnabled: omp.cfgAsyncEnabled.get(settings) !== false,
				patterns: summary.patterns,
			});
			if (problems.length > 0) throw new BenchUsageError(`refusing to start: ${problems.join("; ")}`);

			const argv = ompArgv({ ompBin, model, guard: guardPath, overlay, extension, sessionDir, maxCost });
			if (args.dryRun) {
				printPlan(
					"H1 live check",
					[
						`omp ${omp.version} (in-process check from ${omp.dir}); live runs use \`${ompBin}\` on PATH`,
						`settings as the live runs load them: ${JSON.stringify(summary)}`,
						`model ${model}; ${picks.length} picks + 1 canary, --max-cost ${maxCost} each`,
						"recorder: this script is omp's ACP client with a terminal; a call omp lets through reaches terminal/create and runs nothing, a prompt reaches session/request_permission and is rejected once, a deny ends the call with omp's policy text",
						`overlay (${overlay}, after the guard):\n${LIVE_OVERLAY.trimEnd().replace(/^/gm, "      ")}`,
						`recorder extension (${extension}): drops pty, name, ready and async from bash calls and blocks a URL working folder, so every call reaches the client's terminal`,
						`omp ${omp.version} has the client-terminal route and a reject_once permission option: yes`,
						`cwd: ${work} (a fresh empty folder); omp's sessions in ${sessionDir}`,
						`command line: ${displayArgv(argv)}`,
						`prompt (first pick, sent as session/prompt):\n${livePrompt(picks[0]?.command ?? CANARY).replace(/^/gm, "      ")}`,
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
			const runOmp = async (command: string) => {
				const startedAt = performance.now();
				const turn = await runAcpTurn({
					argv,
					cwd: work,
					prompt: livePrompt(command),
					env: { ...process.env, PI_NO_PTY: "1" },
					timeoutMs: 300_000,
				});
				return {
					run: classifyTurn(turn.events, command),
					stopReason: turn.stopReason,
					failure: turn.failure,
					exitCode: turn.exitCode,
					durationMs: Math.round(performance.now() - startedAt),
				};
			};

			const canary = await runOmp(CANARY);
			if (existsSync(join(work, CANARY_FILE)) || canary.run.outcome !== "ran" || !canary.run.sameCommand)
				throw new Error(
					`canary failed (outcome ${canary.run.outcome}${canary.failure ? `, ${canary.failure}` : ""}, file ${existsSync(join(work, CANARY_FILE)) ? "written" : "absent"}); stopping before any pick`,
				);
			console.log(`canary reached the recorder's terminal (${canary.run.terminal}), nothing written`);

			const rows = [];
			for (const pick of picks) {
				const inProcess = judge(pick.command);
				const { run, stopReason, failure, exitCode, durationMs } = await runOmp(pick.command);
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
					stopReason,
					failure,
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
				corpusSha256,
				settings: summary,
				argv: displayArgv(argv),
				recorder: "ACP client terminal (0585)",
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
				removeTree(fixture);
			} catch (error) {
				// Windows can hold a file there a while longer (seen in the VM after the last omp run); the results are
				// written by now, so the run still counts.
				console.warn(`couldn't remove ${fixture}: ${error instanceof Error ? error.message : String(error)}`);
			}
		}
	});
}
