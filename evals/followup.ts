import { BenchUsageError } from "../bench/lib/args.ts";
import { type BenchTask, shuffle, type TaskLanguage } from "./tasks.ts";

/**
 * The follow-up study (0543, amended by 0545), run after the main run: each comparison is one arm paired against A6
 * on the same 10 main tasks × 2 reps, A6 itself run again in the same seeded order so the pairs share the hours and
 * the build. Reported as exploratory (20 trials per comparison show only large differences); each part is kept by the
 * keep rule (`keep.ts`). pitchfork is no comparison (0560).
 */

export const FOLLOWUP_BASELINE = "a6";
export const FOLLOWUP_TASKS = 10;
export const FOLLOWUP_REPS = 2;

export interface FollowupComparison {
	/** The arm's variant in `evals/variants/`. */
	arm: string;
	/** What differs from A6. */
	change: string;
	records: string;
	/** Which side carries the feature the keep rule decides on: A6 (a part switched off) or the arm (a part tried on). */
	feature: "baseline" | "arm";
}

export const FOLLOWUP_COMPARISONS: readonly FollowupComparison[] = [
	{
		arm: "f-index-off",
		change: "the code index off: graph_search answers with facts only",
		records: "0542, 0562",
		feature: "baseline",
	},
	{ arm: "f-prewalk", change: "prewalk on (`omp --prewalk`)", records: "0541", feature: "arm" },
	{
		arm: "f-keepalive",
		change: "keep-alive pings on (`providers.cacheWarming: idle` in place of A6's `streaming`)",
		records: "0541",
		feature: "arm",
	},
	{
		arm: "f-keep-context",
		change: 'omp\'s "Approve and keep context" in place of "Approve and execute"',
		records: "0545",
		feature: "arm",
	},
	{
		arm: "f-ponytail-if",
		change: 'Ponytail with its "Running tests" rules inside `<important if="you are running or writing tests">`',
		records: "0545",
		feature: "arm",
	},
];

/** The study's arms: A6 and each comparison's arm. */
export const FOLLOWUP_ARMS: readonly string[] = [FOLLOWUP_BASELINE, ...FOLLOWUP_COMPARISONS.map((c) => c.arm)];

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
