// The keep rule of 0543, written before any run: a tried feature stays when its paired cost per passing task is lower
// and its pass rate is not significantly lower (McNemar's exact test on the paired trials, Holm's correction over the
// study's comparisons, α = 0.05).
//
//   bun evals/keep.ts --out <follow-up runDir> [--alpha 0.05] [--dry-run]
//
// reads `<runDir>/results.jsonl`, pairs each comparison's arm with A6 by task and rep, prints each verdict and writes
// `<runDir>/keep.json` (a dry run prints the verdicts only).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { BenchUsageError, flag, parseBenchArgs, runScript } from "../bench/lib/args.ts";
import { writeJson } from "../bench/lib/output.ts";
import { holm, mcnemarExact } from "../bench/lib/stats.ts";
import { FOLLOWUP_BASELINE, FOLLOWUP_COMPARISONS } from "./followup.ts";
import type { BenchRow } from "./tasks.ts";

export const KEEP_ALPHA = 0.05;

/** One trial as the rule reads it: its pair key (task and rep), whether it passed, and its cost. */
export interface KeepTrial {
	pair: string;
	passed: boolean;
	cost: number;
}

/** One comparison: the trials with the feature and without it. */
export interface KeepInput {
	name: string;
	withFeature: readonly KeepTrial[];
	withoutFeature: readonly KeepTrial[];
}

export interface KeepVerdict {
	name: string;
	/** Pairs present on both sides; only they count. */
	pairs: number;
	passesWith: number;
	passesWithout: number;
	/** Summed cost over the pairs ÷ passes; Infinity with no pass. */
	costPerPassWith: number;
	costPerPassWithout: number;
	/** Pairs where only the side with the feature passed, and only the side without. */
	onlyWith: number;
	onlyWithout: number;
	p: number;
	pHolm: number;
	cheaper: boolean;
	significantlyWorse: boolean;
	keep: boolean;
}

/** Cost per passing task over paired trials: summed cost ÷ passes (Infinity with none). */
export function costPerPass(trials: readonly KeepTrial[]): number {
	const passes = trials.filter((trial) => trial.passed).length;
	return passes === 0 ? Number.POSITIVE_INFINITY : trials.reduce((sum, trial) => sum + trial.cost, 0) / passes;
}

/** Applies the keep rule to every comparison of one study; Holm's correction runs over all of them together. */
export function keepRule(comparisons: readonly KeepInput[], alpha = KEEP_ALPHA): KeepVerdict[] {
	const paired = comparisons.map((comparison) => {
		const without = new Map(comparison.withoutFeature.map((trial) => [trial.pair, trial]));
		const pairs = comparison.withFeature.flatMap((trial) => {
			const other = without.get(trial.pair);
			return other === undefined ? [] : [{ with: trial, without: other }];
		});
		const onlyWith = pairs.filter((p) => p.with.passed && !p.without.passed).length;
		const onlyWithout = pairs.filter((p) => !p.with.passed && p.without.passed).length;
		return { comparison, pairs, onlyWith, onlyWithout, p: mcnemarExact(onlyWithout, onlyWith).p };
	});
	const adjusted = holm(paired.map((entry) => entry.p));
	return paired.map((entry, i) => {
		const withTrials = entry.pairs.map((p) => p.with);
		const withoutTrials = entry.pairs.map((p) => p.without);
		const passesWith = withTrials.filter((t) => t.passed).length;
		const passesWithout = withoutTrials.filter((t) => t.passed).length;
		const costPerPassWith = costPerPass(withTrials);
		const costPerPassWithout = costPerPass(withoutTrials);
		const pHolm = adjusted[i] ?? 1;
		const cheaper = costPerPassWith < costPerPassWithout;
		const significantlyWorse = passesWith < passesWithout && pHolm < alpha;
		return {
			name: entry.comparison.name,
			pairs: entry.pairs.length,
			passesWith,
			passesWithout,
			costPerPassWith,
			costPerPassWithout,
			onlyWith: entry.onlyWith,
			onlyWithout: entry.onlyWithout,
			p: entry.p,
			pHolm,
			cheaper,
			significantlyWorse,
			keep: cheaper && !significantlyWorse,
		};
	});
}

/** A results row as a keep trial: paired by task and rep, passed on the hidden tests, cost from the ledger's price table. */
export function keepTrial(row: Pick<BenchRow, "task" | "rep" | "hiddenPassed" | "ledger">): KeepTrial {
	if (row.ledger === undefined)
		throw new BenchUsageError(`${row.task} rep ${row.rep} has no ledger; it can't be priced`);
	return { pair: `${row.task}/${row.rep}`, passed: row.hiddenPassed === true, cost: row.ledger.cost };
}

/** The follow-up study's comparisons from its results rows: each arm against A6, the feature's side per comparison. */
export function followupInputs(rows: readonly BenchRow[]): KeepInput[] {
	const of = (arm: string) => rows.filter((row) => row.arm === arm).map(keepTrial);
	const baseline = of(FOLLOWUP_BASELINE);
	return FOLLOWUP_COMPARISONS.map((comparison) => {
		const arm = of(comparison.arm);
		return comparison.feature === "arm"
			? { name: comparison.arm, withFeature: arm, withoutFeature: baseline }
			: { name: comparison.arm, withFeature: baseline, withoutFeature: arm };
	});
}

if (import.meta.main)
	await runScript(async () => {
		const args = parseBenchArgs(process.argv.slice(2), { defaultRuns: 1, valued: ["--alpha"] });
		const runDir = args.out;
		if (runDir === undefined) throw new BenchUsageError("--out <follow-up runDir> is required");
		const alpha = Number(flag(args, "alpha", String(KEEP_ALPHA)));
		if (!(alpha > 0 && alpha < 1)) throw new BenchUsageError("--alpha needs a number between 0 and 1");
		const file = join(runDir, "results.jsonl");
		if (!existsSync(file)) throw new BenchUsageError(`no results at ${file}`);
		const rows = readFileSync(file, "utf8")
			.split(/\r?\n/)
			.filter((line) => line.trim() !== "")
			.map((line) => JSON.parse(line) as BenchRow);
		const verdicts = keepRule(followupInputs(rows), alpha);
		for (const v of verdicts)
			console.log(
				`${v.keep ? "KEEP" : "drop"} ${v.name}: $${v.costPerPassWith.toFixed(2)} vs $${v.costPerPassWithout.toFixed(2)} per pass, ${v.passesWith}/${v.pairs} vs ${v.passesWithout}/${v.pairs} passed, McNemar p ${v.p.toFixed(3)} (Holm ${v.pHolm.toFixed(3)})`,
			);
		if (!args.dryRun) console.log(`wrote ${writeJson(runDir, "keep", { alpha, rule: "0543", verdicts })}`);
	});
