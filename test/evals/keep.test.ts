import { expect, test } from "bun:test";
import { FOLLOWUP_BASELINE, FOLLOWUP_COMPARISONS } from "../../evals/followup.ts";
import { costPerPass, followupInputs, type KeepTrial, keepRule, keepTrial } from "../../evals/keep.ts";
import type { BenchRow } from "../../evals/tasks.ts";

/** `n` paired trials keyed t0…, passing where `passes[i]`, each costing `cost`. */
const trials = (passes: boolean[], cost: number): KeepTrial[] =>
	passes.map((passed, i) => ({ pair: `t${i}`, passed, cost }));
const all = (n: number, value: boolean) => Array.from({ length: n }, () => value);

test("cost per passing task is summed cost over passes; no pass is infinitely expensive", () => {
	expect(costPerPass(trials([true, true, false, false], 1))).toBe(2);
	expect(costPerPass(trials([false, false], 1))).toBe(Number.POSITIVE_INFINITY);
});

test("a feature stays when cheaper per pass and not significantly worse; it goes when dearer", () => {
	const [cheaper, dearer] = keepRule([
		{ name: "cheaper", withFeature: trials(all(20, true), 0.4), withoutFeature: trials(all(20, true), 0.5) },
		{ name: "dearer", withFeature: trials(all(20, true), 0.6), withoutFeature: trials(all(20, true), 0.5) },
	]);
	expect(cheaper).toMatchObject({ pairs: 20, cheaper: true, significantlyWorse: false, keep: true, p: 1, pHolm: 1 });
	expect(cheaper?.costPerPassWith).toBeCloseTo(0.4, 12);
	expect(dearer).toMatchObject({ cheaper: false, keep: false });
});

test("a cheaper feature whose pass rate is significantly lower after Holm's correction goes (0543)", () => {
	// With the feature, 12 of 20 pairs that passed without it fail: McNemar p ≈ 0.0005, cheap only because it fails fast.
	const withPasses = [...all(8, true), ...all(12, false)];
	const [worse, fine] = keepRule([
		{ name: "worse", withFeature: trials(withPasses, 0.05), withoutFeature: trials(all(20, true), 0.5) },
		{ name: "fine", withFeature: trials(all(20, true), 0.4), withoutFeature: trials(all(20, true), 0.5) },
	]);
	expect(worse).toMatchObject({ onlyWith: 0, onlyWithout: 12, cheaper: true, significantlyWorse: true, keep: false });
	expect(worse?.p).toBeCloseTo(2 * 0.5 ** 12, 12);
	expect(worse?.pHolm).toBeCloseTo(2 * 2 * 0.5 ** 12, 12);
	expect(fine?.keep).toBe(true);
});

test("Holm's correction can leave a small loss not significant across the family", () => {
	// 5 discordant pairs against: p = 0.0625 alone, so not significant; the feature is cheaper and stays.
	const [verdict] = keepRule([
		{
			name: "x",
			withFeature: trials([...all(15, true), ...all(5, false)], 0.3),
			withoutFeature: trials(all(20, true), 0.5),
		},
		{ name: "y", withFeature: trials(all(20, true), 0.5), withoutFeature: trials(all(20, true), 0.5) },
	]);
	expect(verdict).toMatchObject({ significantlyWorse: false, cheaper: true, keep: true });
	expect(verdict?.pHolm).toBeCloseTo(0.125, 12);
});

test("only pairs on both sides count", () => {
	const [verdict] = keepRule([
		{ name: "x", withFeature: trials(all(3, true), 1), withoutFeature: trials(all(2, true), 2) },
	]);
	expect(verdict?.pairs).toBe(2);
});

test("the follow-up study's rows become comparisons against A6, the feature on the side the comparison says", () => {
	const row = (arm: string, task: string, rep: number, passed: boolean, cost: number) =>
		({ arm, task, rep, hiddenPassed: passed, ledger: { cost } }) as unknown as BenchRow;
	const rows = [
		row(FOLLOWUP_BASELINE, "a", 0, true, 1),
		row("f-index-off", "a", 0, false, 2),
		row("f-prewalk", "a", 0, true, 0.5),
	];
	const inputs = followupInputs(rows);
	expect(inputs.map((i) => i.name)).toEqual(FOLLOWUP_COMPARISONS.map((c) => c.arm));
	const indexOff = inputs.find((i) => i.name === "f-index-off");
	expect(indexOff?.withFeature).toEqual([{ pair: "a/0", passed: true, cost: 1 }]);
	expect(indexOff?.withoutFeature).toEqual([{ pair: "a/0", passed: false, cost: 2 }]);
	const prewalk = inputs.find((i) => i.name === "f-prewalk");
	expect(prewalk?.withFeature).toEqual([{ pair: "a/0", passed: true, cost: 0.5 }]);
	expect(() => keepTrial({ task: "a", rep: 0, hiddenPassed: true, ledger: undefined })).toThrow(/no ledger/);
});
