// Runs Jocasta's evals (1.10) on his daily omp (0386): the repo's cases in `evals/cases/`, then his own in
// `~/.jocasta/agent/evals/cases/`, each trial a real `omp -p <prompt> --mode json` run (or `claude -p` for a Claude
// Code variant, 0482) that bills model usage, so it needs `JOCASTA_EVALS_REAL=1` and never runs in CI (0163, 0166,
// 0169). The harness is resolved on PATH. 3 reps and a 10-minute limit per trial by default; `--reps <n>` and
// `--timeout <minutes>` change them (0318). `--variant <name>`, repeatable, layers `evals/variants/<name>.yaml` (0375;
// the arms A0–A4 of 0482 are `a0`…`a4`). `--profile <name>` runs every omp trial under that omp profile (0482 E8).
// Before each trial the quota pacer reads `omp usage --json` and waits while every Anthropic account has its 5h or 7d
// window at 85% or more (0482, 0543).
// `--dry-run` prints the plan and runs nothing. Results go to `<data>/evals/runs/<time>-<label>/` (0166, 0375).
// `compare <runDirA> <runDirB>` applies the keep rule and the paired statistics (0375, 0482). Exits 1 when a trial
// fails, 2 on a bad flag, variant or environment (0140).
import { join } from "node:path";
import { toolCommand } from "../bench/lib/launch.ts";
import { UserError } from "../src/shared/errors.ts";
import { configDir, dataDir } from "../src/shared/paths.ts";
import { parseRunArgs, type RunSettings, takeVariantFlags } from "./args.ts";
import { compareRunDirs } from "./compare.ts";
import { createPacer, PAUSE_AT } from "./pacer.ts";
import { loadAllCases, runEvals, trialArgs } from "./runner.ts";
import { loadVariants, type Variant } from "./variants.ts";

const argv = process.argv.slice(2);
const casesDirs = [join(import.meta.dir, "cases"), join(configDir(), "evals", "cases")];
let settings: RunSettings;
let variant: Variant;
let launcher: string[];
try {
	if (argv[0] === "compare") {
		const [, dirA, dirB, ...extra] = argv;
		if (dirA === undefined || dirB === undefined || extra.length > 0)
			throw new UserError("usage: bun run evals compare <runDirA> <runDirB>");
		console.log(compareRunDirs(dirA, dirB));
		process.exit(0);
	}
	const { variants, rest } = takeVariantFlags(argv);
	settings = parseRunArgs(rest);
	variant = loadVariants(join(import.meta.dir, "variants"), variants);
	if (settings.profile !== undefined && variant.harness !== "omp")
		throw new UserError("--profile is omp's; a Claude Code variant can't take it");
	if (settings.dryRun) {
		const cases = loadAllCases(casesDirs);
		const first = cases[0];
		console.log(`evals dry run (0482): nothing runs and no model is called.`);
		console.log(
			`  variant ${variant.label}, harness ${variant.harness}, ${cases.length} cases × ${settings.reps} reps`,
		);
		console.log(
			`  limit ${settings.timeoutMinutes ?? "none"} min per trial; profile ${settings.profile ?? "his default"}`,
		);
		console.log(
			`  quota pacer: omp usage --json before each trial, waits while every account has its 5h or 7d window at ${PAUSE_AT * 100}% or more`,
		);
		for (const c of cases) console.log(`  case ${c.id}: ${c.checks.length} checks, ${c.hiddenChecks.length} hidden`);
		if (first) {
			const turns = [first.prompt, ...variant.followups];
			turns.forEach((prompt, i) => {
				const args = trialArgs(variant, {
					prompt,
					followUp: i > 0,
					overlay: "<trial>/eval-run.yml",
					profile: settings.profile,
				});
				console.log(
					`  turn ${i + 1}: ${variant.harness} ${args.map((arg) => (/\s/.test(arg) ? JSON.stringify(arg) : arg)).join(" ")}`,
				);
			});
		}
		console.log(`  results: ${join(dataDir(), "evals", "runs", `<time>-${variant.label}`)}`);
		process.exit(0);
	}
	if (!process.env.JOCASTA_EVALS_REAL)
		throw new UserError("evals run omp against real models and bill usage; set JOCASTA_EVALS_REAL=1 to run them");
	if (process.env.CI) throw new UserError("real-model evals never run in CI (0169)");
	// The bench's launcher (EVL-14): npm's `.cmd` shim is started through its `.exe`, or `cmd.exe /d /c`.
	launcher = toolCommand(variant.harness, (name) => {
		const found = Bun.which(name);
		if (!found) throw new UserError(`${name} is not on PATH; install ${name} or add it to PATH to run the evals`);
		return found;
	});
} catch (error) {
	if (!(error instanceof UserError)) throw error;
	console.error(`jocasta: ${error.message}`);
	process.exit(error.exitCode);
}
const outDir = join(dataDir(), "evals", "runs", `${new Date().toISOString().replace(/[:.]/g, "-")}-${variant.label}`);
const rows = await runEvals({
	casesDir: casesDirs,
	outDir,
	reps: settings.reps,
	timeoutMs: settings.timeoutMinutes === undefined ? undefined : settings.timeoutMinutes * 60_000,
	launcher,
	variant,
	profile: settings.profile,
	pace: createPacer().pace,
	onRow: (row) => console.log(`${row.passed ? "pass" : "FAIL"} ${row.case} rep ${row.rep}`),
});
const passed = rows.filter((row) => row.passed).length;
console.log(`${passed}/${rows.length} passed; report: ${join(outDir, "report.html")}`);
if (passed < rows.length) process.exitCode = 1;
