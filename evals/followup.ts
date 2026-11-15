import { BenchUsageError } from "../bench/lib/args.ts";
import { type BenchTask, shuffle, type TaskLanguage } from "./tasks.ts";

/**
 * The follow-up study (0543, amended by 0545, 0575, 0605, 0608 and 0620), run after the main run: each comparison is
 * one arm paired against its own baseline (A6, or A1 for the prewalk and thinking arms) on the same 10 main tasks × 2
 * reps, each baseline itself run again in the same seeded order so the pairs share the hours and the build. Reported
 * as exploratory (20 trials per comparison show only large differences); each part is kept by the keep rule
 * (`keep.ts`). pitchfork is no comparison (0560), and neither are keep-alive pings (0575): a trial has no idle gap for
 * them to fill.
 */

export const FOLLOWUP_TASKS = 10;
export const FOLLOWUP_REPS = 2;

export interface FollowupComparison {
	/** The arm's variant in `evals/variants/`. */
	arm: string;
	/** The arm it is paired against, run again in the study: `a6`, or `a1` for the prewalk and thinking arms (0608, 0620). */
	baseline: string;
	/** What differs from the baseline. */
	change: string;
	records: string;
}

export const FOLLOWUP_COMPARISONS: readonly FollowupComparison[] = [
	{
		arm: "f-keep-context",
		baseline: "a6",
		change: 'omp\'s "Approve and keep context" in place of "Approve and execute"',
		records: "0545",
	},
	{
		arm: "f-keep-smol",
		baseline: "a6",
		change: 'omp\'s "Approve and keep context", the work that follows done on the `smol` role',
		records: "0608",
	},
	{
		arm: "f-ponytail-if",
		baseline: "a6",
		change: 'Ponytail with its "Running tests" rules inside `<important if="you are running or writing tests">`',
		records: "0545",
	},
	{ arm: "f-prewalk-clean", baseline: "a1", change: "prewalk on (`omp --prewalk`) on A1", records: "0608" },
	{
		arm: "f-advisor",
		baseline: "a6",
		change: "omp's advisor on, on the `advisor` role's model and level, every third update",
		records: "0620",
	},
	{ arm: "f-think-auto", baseline: "a1", change: "`--thinking auto` in place of `high` on A1", records: "0620" },
];

/** The study's baselines, in the order they run first: A6 and A1. */
export const FOLLOWUP_BASELINES: readonly string[] = ["a6", "a1"];

/** The study's arms: the baselines, then each comparison's arm. */
export const FOLLOWUP_ARMS: readonly string[] = [...FOLLOWUP_BASELINES, ...FOLLOWUP_COMPARISONS.map((c) => c.arm)];

/** The language strata the follow-up's tasks keep in proportion to the main set. */
const STRATA: readonly (readonly TaskLanguage[])[] = [["Python"], ["TypeScript", "JavaScript"], ["Go"]];

/**
 * The study's tasks: {@link FOLLOWUP_TASKS} of the main set, each language stratum's share by largest remainder (ties
 * to the earlier stratum), each stratum's tasks (in id order) shuffled with `seed`.
 */
export function pickFollowupTasks(tasks: readonly BenchTask[], seed: number, count = FOLLOWUP_TASKS): BenchTask[] {
	const main = tasks.filter((task) => task.set === "main");
	if (main.length < count)
		throw new BenchUsageError(`the follow-up study needs ${count} main tasks; the selection has ${main.length}`);
	const strata = STRATA.map((languages) =>
		main.filter((task) => languages.includes(task.language)).sort((a, b) => a.instance_id.localeCompare(b.instance_id)),
	);
	const exact = strata.map((stratum) => (stratum.length * count) / main.length);
	const shares = exact.map(Math.floor);
	const order = exact
		.map((value, i) => ({ i, rest: value - Math.floor(value) }))
		.sort((a, b) => b.rest - a.rest || a.i - b.i);
	for (let k = 0; shares.reduce((sum, n) => sum + n, 0) < count; k++) {
		const pick = order[k % order.length] as { i: number };
		shares[pick.i] = (shares[pick.i] ?? 0) + 1;
	}
	return strata.flatMap((stratum, i) => shuffle(stratum, seed).slice(0, shares[i] ?? 0));
}
