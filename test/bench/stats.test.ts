import { expect, test } from "bun:test";
import {
	bootstrapCi,
	holm,
	mcnemarExact,
	mean,
	median,
	normalCdf,
	pairedBootstrap,
	passHatK,
	quantile,
	rate,
	ruleOfThree,
	summarize,
	wilcoxonSignedRank,
} from "../../bench/lib/stats.ts";

test("quantiles interpolate between order statistics", () => {
	expect(quantile([4, 1, 3, 2], 0.5)).toBe(2.5);
	expect(
		quantile(
			Array.from({ length: 20 }, (_, i) => i + 1),
			0.95,
		),
	).toBeCloseTo(19.05, 10);
	expect(median([7])).toBe(7);
	expect(Number.isNaN(median([]))).toBe(true);
});

test("the bootstrap is seeded: the same input gives the same CI, and a constant gives a point", () => {
	const values = [3, 1, 4, 1, 5, 9, 2, 6, 5, 3];
	expect(bootstrapCi(values, mean, { resamples: 500 })).toEqual(bootstrapCi(values, mean, { resamples: 500 }));
	expect(bootstrapCi([2, 2, 2], median, { resamples: 200 })).toEqual({ low: 2, high: 2 });
	const ci = bootstrapCi(values, mean, { resamples: 2000 });
	expect(ci.low).toBeLessThan(mean(values));
	expect(ci.high).toBeGreaterThan(mean(values));
});

test("summarize reports n, median, p95 and the median's CI", () => {
	const summary = summarize([10, 20, 30, 40], { resamples: 300 });
	expect([summary.n, summary.median, summary.min, summary.max, summary.mean]).toEqual([4, 25, 10, 40, 25]);
	expect(summary.medianCi95.low).toBeGreaterThanOrEqual(10);
	expect(summary.medianCi95.high).toBeLessThanOrEqual(40);
});

test("rate counts hits with a CI; the rule of three bounds zero events", () => {
	const r = rate([true, false, true, true], { resamples: 300 });
	expect([r.hits, r.n, r.rate]).toEqual([3, 4, 0.75]);
	expect(ruleOfThree(5000)).toBeCloseTo(0.0006, 10);
});

test("McNemar's exact test is the two-sided binomial over discordant pairs", () => {
	expect(mcnemarExact(0, 0)).toEqual({ discordant: 0, p: 1 });
	expect(mcnemarExact(0, 5).p).toBeCloseTo(2 / 32, 12);
	expect(mcnemarExact(9, 1).p).toBeCloseTo(22 / 1024, 12);
	expect(mcnemarExact(3, 3).p).toBe(1);
});

test("Wilcoxon signed-rank: exact without ties, normal with ties, zeros dropped", () => {
	const allUp = [1, 2, 3, 4, 5].map((d) => ({ a: 0, b: d }));
	expect(wilcoxonSignedRank(allUp)).toEqual({ n: 5, wPlus: 15, p: 2 / 32, method: "exact" });
	expect(wilcoxonSignedRank([{ a: 1, b: 1 }])).toEqual({ n: 0, wPlus: 0, p: 1, method: "exact" });
	const mixed = wilcoxonSignedRank([1, -2, 3, -4, 5, 0].map((d) => ({ a: 0, b: d })));
	expect([mixed.n, mixed.wPlus, mixed.method]).toEqual([5, 9, "exact"]);
	expect(mixed.p).toBeGreaterThan(0.5);
	const tied = wilcoxonSignedRank(Array.from({ length: 30 }, () => ({ a: 0, b: 1 })));
	expect(tied.method).toBe("normal");
	expect(tied.p).toBeLessThan(0.001);
});

test("Holm adjusts in step-down order and keeps the input order", () => {
	const adjusted = holm([0.01, 0.04, 0.03]);
	expect(adjusted[0]).toBeCloseTo(0.03, 12);
	expect(adjusted[1]).toBeCloseTo(0.06, 12);
	expect(adjusted[2]).toBeCloseTo(0.06, 12);
	expect(holm([0.5, 0.9])).toEqual([1, 1]);
});

test("pass^k averages C(c,k)/C(n,k) over tasks with at least k trials", () => {
	expect(
		passHatK(
			[
				{ passed: 3, trials: 3 },
				{ passed: 2, trials: 3 },
				{ passed: 3, trials: 4 },
				{ passed: 1, trials: 1 },
			],
			3,
		),
	).toBeCloseTo((1 + 0 + 0.25) / 3, 12);
});

test("the paired bootstrap's difference is the mean per-task b − a", () => {
	const result = pairedBootstrap(
		[
			{ a: 0, b: 1 },
			{ a: 1, b: 1 },
			{ a: 0.5, b: 1 },
		],
		{ resamples: 500 },
	);
	expect(result.difference).toBeCloseTo(0.5, 12);
	expect(result.ci95.low).toBeGreaterThanOrEqual(0);
	expect(result.ci95.high).toBeLessThanOrEqual(1);
});

test("the normal CDF", () => {
	expect(normalCdf(0)).toBeCloseTo(0.5, 6);
	expect(normalCdf(1.959964)).toBeCloseTo(0.975, 5);
	expect(normalCdf(-1.959964)).toBeCloseTo(0.025, 5);
});
