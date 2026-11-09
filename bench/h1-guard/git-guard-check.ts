// H1 (0482, 0501, 0535): measures omp 18.18.2's git guard instead of trusting git-guard.ts's reimplementation. For
// the VM (a throwaway Windows 11 machine named JOCASTA-CLEAN); it RUNS corpus commands, so it refuses anywhere else.
//
//   bun bench/h1-guard/git-guard-check.ts --corpus <corpus.jsonl> --out <dir> [--omp <package dir>] [--timeout 60]
//     [--any-version] (--dry-run | --run)
//
// Lines: every corpus line made only of git (any spelling the corpus uses: `git`, `git.exe`, the full path, `git -C …`,
// also inside `pwsh -c`/`cmd /c`), `cd …` and `echo …`, split with omp's own tokenizer. For each line, twice, on a
// fresh fixture at C:\work\demo (the corpus's path; recipe below): omp's own `executeBash` from
// `exec/bash-executor.ts`, the call the bash tool makes (`tools/bash.ts:1100-1117,1892-1906`), in C:\work\demo (a main
// checkout, where the guard applies its shared-checkout rules) with `gitGuard: true`, then `gitGuard: false` as the
// control. The tool also passes `ovnfs`, which only fills the refusal's guidance text. git's config is isolated for
// the fixture and the runs (`GIT_CONFIG_GLOBAL` to a temp file, `GIT_CONFIG_NOSYSTEM=1`, a temp HOME and USERPROFILE),
// and omp's settings come from an empty agent folder, so nothing outside C:\work and the temp folder changes. Output
// is the shell's combined stdout and stderr, as the bash tool gets it.
//
// It runs only with `--run`, on a machine named JOCASTA-CLEAN, with C:\work holding nothing but the fixture's two
// folders. Writes `git-guard-check.jsonl` (per line: both runs' exit code and trimmed output, the guard's refusal
// message, git-guard.ts's prediction, agreement) and `git-guard-check.json` (agreement per subcommand and per way of
// writing, every disagreement listed). `omp-verdicts.ts --git-guard <git-guard-check.jsonl>` then uses the measured
// refusals for these lines.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { hostname, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { removeTree } from "../../src/shared/fsx.ts";
import { runProgramSync } from "../../src/shared/process.ts";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "../lib/args.ts";
import { appendRow, conditions, printPlan, writeJson } from "../lib/output.ts";
import { requireVm, VM_MACHINE } from "../lib/sandbox.ts";
import { type CorpusEntry, readCorpus } from "./evaluate.ts";
import { gitGuardRefusalOf, gitOnlyArgv, gitSubcommand } from "./git-guard.ts";
import { findOmpDir, loadOmp } from "./omp-verdicts.ts";

export const WORK_ROOT = "C:\\work";
export const FIXTURE = "C:\\work\\demo";
export const ORIGIN = "C:\\work\\demo-origin.git";
/** What C:\work may hold before a run: nothing, or the fixture's own two folders. */
const FIXTURE_ENTRIES: Record<string, true> = { demo: true, "demo-origin.git": true };

export type Step = { git: string[]; cwd?: string } | { write: string; text: string };

/**
 * The fixture: commits on `main`, the branches and tag the corpus names, `feature/x` (which its force-pushes name)
 * with its own commit, a bare `origin` with everything pushed, one stash entry, `src/app.ts` (the tracked file the
 * corpus names) modified and uncommitted, and an untracked file. Paths are relative to C:\work\demo unless `cwd` says
 * otherwise.
 */
export const FIXTURE_RECIPE: readonly Step[] = [
	{ git: ["init", "-b", "main", FIXTURE], cwd: WORK_ROOT },
	{ write: "README.md", text: "# demo\n" },
	{ write: "src/app.ts", text: "export const app = 1;\n" },
	{ git: ["add", "."] },
	{ git: ["commit", "-m", "initial"] },
	{ write: "src/app.ts", text: "export const app = 2;\n" },
	{ git: ["commit", "-am", "second"] },
	{ git: ["tag", "v0.1.0"] },
	{ git: ["branch", "dev"] },
	{ git: ["branch", "backup"] },
	{ git: ["branch", "old-feature"] },
	// One branch with its own commit. Git can't hold `feature` beside `feature/x` (a ref can't also be a folder).
	{ git: ["switch", "-c", "feature/x"] },
	{ write: "feature.txt", text: "feature work\n" },
	{ git: ["add", "feature.txt"] },
	{ git: ["commit", "-m", "feature work"] },
	{ git: ["switch", "main"] },
	{ git: ["init", "--bare", "-b", "main", ORIGIN], cwd: WORK_ROOT },
	{ git: ["remote", "add", "origin", ORIGIN] },
	{ git: ["push", "origin", "--all"] },
	{ git: ["push", "origin", "--tags"] },
	{ git: ["branch", "--set-upstream-to=origin/main", "main"] },
	{ write: "src/app.ts", text: "export const app = 3; // stashed\n" },
	{ git: ["stash", "push", "-m", "fixture stash"] },
	{ write: "src/app.ts", text: "export const app = 4; // uncommitted\n" },
	{ write: "notes.txt", text: "untracked\n" },
];

const GITCONFIG = "[user]\n\tname = H1 Check\n\temail = h1-check@example.invalid\n[core]\n\tautocrlf = false\n";

/** The env that isolates git's config for the fixture and the runs. */
export function isolatedGitEnv(temp: string): Record<string, string> {
	const home = join(temp, "home");
	return {
		GIT_CONFIG_GLOBAL: join(temp, "gitconfig"),
		GIT_CONFIG_NOSYSTEM: "1",
		GIT_TERMINAL_PROMPT: "0",
		HOME: home,
		USERPROFILE: home,
	};
}

export interface CheckLine {
	entry: CorpusEntry;
	subcommand: string;
	/** git-guard.ts's prediction: its refusal, or null. */
	reimplemented: string | null;
}

/** The corpus lines the check runs, with their git subcommand and git-guard.ts's prediction. */
export function checkLines(corpus: readonly CorpusEntry[], tokenize: (command: string) => string[][]): CheckLine[] {
	return corpus.flatMap((entry) => {
		const argv = gitOnlyArgv(entry.command, tokenize);
		return argv === null
			? []
			: [{ entry, subcommand: gitSubcommand(argv), reimplemented: gitGuardRefusalOf(tokenize(entry.command)) }];
	});
}

const BLOCKED = /[^\n]*\bis blocked\b[^\n]*/;

/** The git guard's refusal message in a run's output, or null. */
export const refusalIn = (output: string): string | null => output.match(BLOCKED)?.[0].trim() ?? null;

/** One run of a line through omp's `executeBash`. */
export interface RunResult {
	exitCode: number | null;
	timedOut: boolean;
	/** Combined stdout and stderr, trimmed to 2,000 characters. */
	output: string;
	/** The git guard's message in the output, or null. */
	refusal: string | null;
}

/**
 * omp's shell error when it stopped a line before git ran, else null: its `cd` reads an unquoted `C:\work\demo` with
 * `\` as an escape, and a missing program (`pwsh` where PowerShell 7 isn't installed) is "command not found".
 */
export function shellStopped(output: string): string | null {
	const first = output.trimStart().split(/\r?\n/, 1)[0] ?? "";
	return /^error: (cd:|command not found:)/.test(first) ? first : null;
}

export interface CheckRow {
	id: string;
	command: string;
	variant: string;
	subcommand: string;
	/** Both runs completed on a fresh fixture. */
	measured: boolean;
	refusal: string | null;
	reimplemented: string | null;
	agree: boolean;
}

export interface Agreement {
	key: string;
	n: number;
	agree: number;
	disagreements: { id: string; command: string; measured: string | null; reimplemented: string | null }[];
}

/** Measured vs reimplemented refusal per `keyOf`, over measured rows, in first-seen order. */
export function agreement(rows: readonly CheckRow[], keyOf: (row: CheckRow) => string): Agreement[] {
	const byKey = new Map<string, Agreement>();
	for (const row of rows) {
		if (!row.measured) continue;
		const key = keyOf(row);
		let entry = byKey.get(key);
		if (entry === undefined) {
			entry = { key, n: 0, agree: 0, disagreements: [] };
			byKey.set(key, entry);
		}
		entry.n += 1;
		if (row.agree) entry.agree += 1;
		else
			entry.disagreements.push({
				id: row.id,
				command: row.command,
				measured: row.refusal,
				reimplemented: row.reimplemented,
			});
	}
	return [...byKey.values()];
}

const describeStep = (step: Step) =>
	"git" in step
		? `git ${step.git.map((arg) => (/\s/.test(arg) ? JSON.stringify(arg) : arg)).join(" ")}${step.cwd ? ` (in ${step.cwd})` : ""}`
		: `write ${step.write}: ${JSON.stringify(step.text)}`;

/** Refuses unless this is the VM and C:\work holds nothing but the fixture. */
export function requireCleanMachine(machine: string, workEntries: readonly string[] | null): void {
	requireVm(machine, "run corpus commands");
	const strangers = (workEntries ?? []).filter((name) => !Object.hasOwn(FIXTURE_ENTRIES, name));
	if (strangers.length > 0)
		throw new BenchUsageError(`refusing: ${WORK_ROOT} holds more than the fixture (${strangers.join(", ")})`);
}

if (import.meta.main) {
	if (process.argv.slice(2).some((arg) => arg === "--help" || arg === "-h")) {
		console.log(
			"usage: bun bench/h1-guard/git-guard-check.ts --corpus <corpus.jsonl> --out <dir> [--omp <package dir>] [--timeout 60] [--any-version] (--dry-run | --run)",
		);
		process.exit(0);
	}
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), {
			defaultRuns: 1,
			valued: ["--corpus", "--omp", "--timeout"],
			booleans: ["--any-version", "--run"],
		});
		const corpusPath = flag(args, "corpus");
		if (corpusPath === undefined) throw new BenchUsageError("--corpus <path.jsonl> is required");
		if (!existsSync(corpusPath)) throw new BenchUsageError(`${corpusPath} does not exist`);
		const timeoutText = flag(args, "timeout", "60");
		if (!/^\d+$/.test(timeoutText) || Number(timeoutText) < 1)
			throw new BenchUsageError(`--timeout needs whole seconds, not "${timeoutText}"`);
		const timeoutMs = Number(timeoutText) * 1000;
		const ompText = flag(args, "omp");
		const ompDir = ompText === undefined ? findOmpDir() : resolve(ompText);
		if (ompDir === undefined) throw new BenchUsageError("omp's package not found; pass --omp <package dir>");
		const omp = await loadOmp(ompDir, args.flags.has("any-version"));
		const { entries: corpus, sha256: corpusSha256 } = readCorpus(corpusPath);
		const lines = checkLines(corpus, omp.tokenizeShellSegments);
		const temp = args.dryRun ? join(tmpdir(), "h1-git-guard-<random>") : mkdtempSync(join(tmpdir(), "h1-git-guard-"));
		const gitEnv = isolatedGitEnv(temp);
		const call = (gitGuard: boolean) =>
			`executeBash(<line>, ${JSON.stringify({ cwd: FIXTURE, timeout: timeoutMs, gitGuard, env: gitEnv })})`;

		if (args.dryRun) {
			const bySubcommand: Record<string, number> = {};
			for (const line of lines) bySubcommand[line.subcommand] = (bySubcommand[line.subcommand] ?? 0) + 1;
			printPlan(
				"H1 git guard check",
				[
					`omp ${omp.version} from ${omp.dir}; refuses to run without --run, off ${VM_MACHINE}, or with ${WORK_ROOT} holding more than demo and demo-origin.git`,
					`${lines.length} lines made only of git, cd and echo (by subcommand: ${JSON.stringify(bySubcommand)})`,
					`per line, twice (guard on, then the control), a fresh fixture: remove ${FIXTURE} and ${ORIGIN}, then, with git's config isolated (${JSON.stringify(gitEnv)}; ${join(temp, "gitconfig")} sets user.name/email):`,
					...FIXTURE_RECIPE.map((step, i) => `  ${i + 1}. ${describeStep(step)}`),
					`omp: Settings.init({ cwd: "${join(temp, "cwd")}", agentDir: "${join(temp, "agent")}" }), then from ${join(omp.dir, "src", "exec", "bash-executor.ts")}:`,
					`  ${call(true)}`,
					`  ${call(false)}   (the control)`,
					"lines (id, variant, subcommand, git-guard.ts's prediction, command):",
					...lines.map(
						(line) =>
							`${line.entry.id} ${line.entry.variant} ${line.subcommand} ${line.reimplemented === null ? "runs" : `refused (${line.reimplemented})`} ${line.entry.command}`,
					),
				],
				args.out,
			);
			return;
		}
		if (!args.flags.has("run"))
			throw new BenchUsageError("this runs corpus commands: pass --run (in the VM) or --dry-run");
		if (process.platform !== "win32") throw new BenchUsageError("the fixture path is C:\\work\\demo: run on Windows");
		requireCleanMachine(hostname(), existsSync(WORK_ROOT) ? readdirSync(WORK_ROOT) : null);
		const out = args.out;
		if (out === undefined) throw new BenchUsageError("--out <dir> is required (outside the repo)");
		if (existsSync(join(out, "git-guard-check.jsonl")))
			throw new BenchUsageError(`${join(out, "git-guard-check.jsonl")} exists; results are never overwritten`);

		const started = conditions(["git"]);
		mkdirSync(join(temp, "home"), { recursive: true });
		mkdirSync(join(temp, "cwd"), { recursive: true });
		mkdirSync(join(temp, "agent"), { recursive: true });
		writeFileSync(join(temp, "gitconfig"), GITCONFIG);
		// The shell sessions omp starts inherit this process's env; the runs also pass it explicitly.
		Object.assign(process.env, gitEnv);
		delete process.env.PI_CONFIG_FILES;
		const load = (path: string) => import(pathToFileURL(join(omp.dir, "src", path)).href);
		const { Settings } = await load("config/settings.ts");
		await Settings.init({ cwd: join(temp, "cwd"), agentDir: join(temp, "agent") });
		const { executeBash } = await load("exec/bash-executor.ts");

		const resetFixture = () => {
			for (const dir of [FIXTURE, ORIGIN]) removeTree(dir);
			mkdirSync(WORK_ROOT, { recursive: true });
			for (const step of FIXTURE_RECIPE) {
				if ("write" in step) {
					const path = join(FIXTURE, step.write);
					mkdirSync(dirname(path), { recursive: true });
					writeFileSync(path, step.text);
					continue;
				}
				const result = runProgramSync(["git", ...step.git], {
					cwd: step.cwd ?? FIXTURE,
					env: { ...process.env, ...gitEnv },
				});
				if (result.code !== 0) throw new Error(`fixture step "${describeStep(step)}" failed: ${result.stderr.trim()}`);
			}
		};
		const runOnce = async (command: string, gitGuard: boolean): Promise<RunResult> => {
			resetFixture();
			const result = await executeBash(command, { cwd: FIXTURE, timeout: timeoutMs, gitGuard, env: gitEnv });
			const output = String(result.output ?? "");
			return {
				exitCode: result.exitCode ?? null,
				timedOut: result.timedOut === true,
				output: output.length > 2000 ? `${output.slice(0, 2000)}…` : output,
				refusal: refusalIn(output),
			};
		};

		const rows: CheckRow[] = [];
		try {
			for (const line of lines) {
				const { entry } = line;
				let guarded: RunResult | null = null;
				let control: RunResult | null = null;
				let error: string | null = null;
				try {
					guarded = await runOnce(entry.command, true);
					control = await runOnce(entry.command, false);
				} catch (caught) {
					error = caught instanceof Error ? caught.message : String(caught);
				}
				// A line omp's shell stops before git (the control fails the same way) says nothing about the guard.
				const stopped = control === null ? null : shellStopped(control.output);
				if (stopped !== null) error = `omp's shell stopped before git: ${stopped}`;
				const measured = guarded !== null && control !== null && stopped === null;
				const refusal = guarded?.refusal ?? null;
				const row: CheckRow = {
					id: entry.id ?? "",
					command: entry.command,
					variant: entry.variant ?? "",
					subcommand: line.subcommand,
					measured,
					refusal,
					reimplemented: line.reimplemented,
					agree: measured && (refusal !== null) === (line.reimplemented !== null),
				};
				rows.push(row);
				appendRow(out, "git-guard-check", { ...row, expected: entry.expected, guarded, control, error });
				console.log(
					`${row.id} ${measured ? (refusal === null ? "ran" : "refused") : `error: ${error}`}${control?.refusal ? " (control also blocked)" : ""} ${entry.command}`,
				);
			}
		} finally {
			for (const dir of [FIXTURE, ORIGIN]) removeTree(dir);
		}
		const bySubcommand = agreement(rows, (row) => row.subcommand);
		const byVariant = agreement(rows, (row) => row.variant);
		const measuredRows = rows.filter((row) => row.measured);
		const file = writeJson(out, "git-guard-check", {
			benchmark: "H1 git guard check",
			conditions: {
				...started,
				finishedAt: new Date().toISOString(),
				versions: { ...started.versions, omp: omp.version },
			},
			omp: omp.version,
			corpusSha256,
			fixture: FIXTURE_RECIPE.map(describeStep),
			timeoutMs,
			lines: rows.length,
			measured: measuredRows.length,
			agree: measuredRows.filter((row) => row.agree).length,
			bySubcommand,
			byVariant,
			disagreements: bySubcommand.flatMap((entry) => entry.disagreements),
			errors: rows.filter((row) => !row.measured).map((row) => row.id),
		});
		console.log(`agreement ${measuredRows.filter((row) => row.agree).length}/${measuredRows.length}; wrote ${file}`);
		try {
			removeTree(temp);
		} catch (error) {
			// Windows can hold a file there a while longer (seen in the VM with nothing working in the folder); the
			// results are written by now, so the run still counts.
			console.warn(`couldn't remove ${temp}: ${error instanceof Error ? error.message : String(error)}`);
		}
	});
}
