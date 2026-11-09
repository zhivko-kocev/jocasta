/**
 * The statistics the pre-registered analysis names (0482): medians and p95 with percentile-bootstrap CIs for the
 * harness benchmarks, and for the task benchmark a paired task bootstrap, McNemar's exact test, the Wilcoxon
 * signed-rank test and Holm's correction. Every resampling takes a seeded generator so a report is reproducible.
 */

/** A seeded uniform [0, 1) generator (mulberry32). */
export function seededRandom(seed: number): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state + 0x6d2b79f5) >>> 0;
		let t = state;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

export const DEFAULT_SEED = 482;
export const DEFAULT_RESAMPLES = 10_000;

export function mean(values: readonly number[]): number {
	if (values.length === 0) return Number.NaN;
	let sum = 0;
	for (const value of values) sum += value;
	return sum / values.length;
}

/** The q-quantile (0–1) with linear interpolation between order statistics (R's type 7); NaN for no values. */
export function quantile(values: readonly number[], q: number): number {
	if (values.length === 0) return Number.NaN;
	const sorted = [...values].sort((a, b) => a - b);
	const position = (sorted.length - 1) * q;
	const below = Math.floor(position);
	const above = Math.ceil(position);
	const low = sorted[below] ?? Number.NaN;
	const high = sorted[above] ?? Number.NaN;
	return low + (high - low) * (position - below);
}

export const median = (values: readonly number[]) => quantile(values, 0.5);

export interface Interval {
	low: number;
	high: number;
}

export interface BootstrapOptions {
	resamples?: number;
	seed?: number;
	/** Two-sided coverage; 0.95 by default. */
	level?: number;
}

/** The percentile-bootstrap CI of `statistic` over `values` resampled with replacement. */
export function bootstrapCi(
	values: readonly number[],
	statistic: (sample: readonly number[]) => number,
	options: BootstrapOptions = {},
): Interval {
	if (values.length === 0) return { low: Number.NaN, high: Number.NaN };
	const random = seededRandom(options.seed ?? DEFAULT_SEED);
	const resamples = options.resamples ?? DEFAULT_RESAMPLES;
	const level = options.level ?? 0.95;
	const estimates: number[] = new Array(resamples);
	const sample: number[] = new Array(values.length);
	for (let r = 0; r < resamples; r++) {
		for (let i = 0; i < values.length; i++) sample[i] = values[Math.floor(random() * values.length)] ?? 0;
		estimates[r] = statistic(sample);
	}
	return { low: quantile(estimates, (1 - level) / 2), high: quantile(estimates, 1 - (1 - level) / 2) };
}

/** n, median, p95, mean and the median's bootstrap 95% CI: the summary every harness benchmark reports (0482). */
export interface Summary {
	n: number;
	median: number;
	p95: number;
	mean: number;
	min: number;
	max: number;
	medianCi95: Interval;
}

export function summarize(values: readonly number[], options: BootstrapOptions = {}): Summary {
	return {
		n: values.length,
		median: median(values),
		p95: quantile(values, 0.95),
		mean: mean(values),
		min: values.length === 0 ? Number.NaN : Math.min(...values),
		max: values.length === 0 ? Number.NaN : Math.max(...values),
		medianCi95: bootstrapCi(values, median, options),
	};
}

/** A proportion with its bootstrap 95% CI over the 0/1 outcomes. */
export interface Rate {
	hits: number;
	n: number;
	rate: number;
	ci95: Interval;
}

export function rate(outcomes: readonly boolean[], options: BootstrapOptions = {}): Rate {
	const values = outcomes.map((outcome) => (outcome ? 1 : 0));
	const hits = values.reduce((sum: number, value) => sum + value, 0);
	return {
		hits,
		n: values.length,
		rate: values.length === 0 ? Number.NaN : hits / values.length,
		ci95: bootstrapCi(values, mean, options),
	};
}

/** The one-sided 95% upper bound on a rate with 0 events in n trials: the rule of three, 3/n (H4). */
export const ruleOfThree = (n: number) => (n === 0 ? Number.NaN : 3 / n);

/**
 * The paired task bootstrap (0482): resamples tasks with replacement and reports the CI of the mean per-task
 * difference `b - a`. Pairs are per task, so both arms move together.
 */
export function pairedBootstrap(
	pairs: readonly { a: number; b: number }[],
	options: BootstrapOptions = {},
): { difference: number; ci95: Interval } {
	const differences = pairs.map((pair) => pair.b - pair.a);
	return { difference: mean(differences), ci95: bootstrapCi(differences, mean, options) };
}

/** log of n choose k. */
function logChoose(n: number, k: number): number {
	let sum = 0;
	for (let i = 1; i <= k; i++) sum += Math.log(n - k + i) - Math.log(i);
	return sum;
}

/**
 * McNemar's exact test on paired binary outcomes: `onlyA` pairs where only A succeeded, `onlyB` where only B did.
 * Two-sided binomial p-value with p = 0.5 over the discordant pairs; 1 when there are none.
 */
export function mcnemarExact(onlyA: number, onlyB: number): { discordant: number; p: number } {
	const n = onlyA + onlyB;
	if (n === 0) return { discordant: 0, p: 1 };
	const k = Math.min(onlyA, onlyB);
	let tail = 0;
	for (let i = 0; i <= k; i++) tail += Math.exp(logChoose(n, i) - n * Math.LN2);
	return { discordant: n, p: Math.min(1, 2 * tail) };
}

/** The standard normal CDF (Abramowitz–Stegun 7.1.26 on erf; error below 1.5e-7). */
export function normalCdf(z: number): number {
	const x = Math.abs(z) / Math.SQRT2;
	const t = 1 / (1 + 0.3275911 * x);
	const erf =
		1 -
		((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x);
	return z >= 0 ? (1 + erf) / 2 : (1 - erf) / 2;
}

export interface WilcoxonResult {
	/** Pairs with a non-zero difference. */
	n: number;
	/** The sum of the ranks of the positive differences (b > a). */
	wPlus: number;
	p: number;
	method: "exact" | "normal";
}

/**
 * The Wilcoxon signed-rank test on `b - a`, two-sided. Zero differences are dropped (Wilcoxon's method), ties get
 * average ranks. Exact for up to 25 non-zero pairs without ties; otherwise the normal approximation with the tie
 * correction and a continuity correction.
 */
export function wilcoxonSignedRank(pairs: readonly { a: number; b: number }[]): WilcoxonResult {
	const differences = pairs.map((pair) => pair.b - pair.a).filter((d) => d !== 0);
	const n = differences.length;
	if (n === 0) return { n: 0, wPlus: 0, p: 1, method: "exact" };
	const order = differences.map((d, i) => ({ abs: Math.abs(d), i })).sort((x, y) => x.abs - y.abs);
	const ranks: number[] = new Array(n);
	let tieCorrection = 0;
	for (let start = 0; start < n; ) {
		let end = start;
		while (end + 1 < n && order[end + 1]?.abs === order[start]?.abs) end++;
		const rank = (start + end) / 2 + 1;
		for (let j = start; j <= end; j++) ranks[order[j]?.i ?? 0] = rank;
		const size = end - start + 1;
		tieCorrection += size ** 3 - size;
		start = end + 1;
	}
	let wPlus = 0;
	for (let i = 0; i < n; i++) if ((differences[i] ?? 0) > 0) wPlus += ranks[i] ?? 0;
	const total = (n * (n + 1)) / 2;
	if (n <= 25 && tieCorrection === 0) {
		// The exact null distribution of W+ by counting subsets of ranks 1..n.
		const counts = new Array<number>(total + 1).fill(0);
		counts[0] = 1;
		for (let r = 1; r <= n; r++) for (let s = total; s >= r; s--) counts[s] = (counts[s] ?? 0) + (counts[s - r] ?? 0);
		const low = Math.min(wPlus, total - wPlus);
		let tail = 0;
		for (let s = 0; s <= low; s++) tail += counts[s] ?? 0;
		return { n, wPlus, p: Math.min(1, (2 * tail) / 2 ** n), method: "exact" };
	}
	const expected = total / 2;
	const variance = (n * (n + 1) * (2 * n + 1)) / 24 - tieCorrection / 48;
	const z = variance === 0 ? 0 : (Math.abs(wPlus - expected) - 0.5) / Math.sqrt(variance);
	return { n, wPlus, p: Math.min(1, 2 * (1 - normalCdf(Math.max(0, z)))), method: "normal" };
}

/** Holm's step-down adjusted p-values, in the input order. */
export function holm(pValues: readonly number[]): number[] {
	const m = pValues.length;
	const order = pValues.map((p, i) => ({ p, i })).sort((x, y) => x.p - y.p);
	const adjusted: number[] = new Array(m);
	let running = 0;
	order.forEach(({ p, i }, rank) => {
		running = Math.max(running, Math.min(1, (m - rank) * p));
		adjusted[i] = running;
	});
	return adjusted;
}

/**
 * pass^k over tasks (0482): per task, the chance that k trials drawn without replacement from its n all pass,
 * C(c, k) / C(n, k), averaged over the tasks that have at least k trials.
 */
export function passHatK(tasks: readonly { passed: number; trials: number }[], k: number): number {
	const eligible = tasks.filter((task) => task.trials >= k);
	if (eligible.length === 0) return Number.NaN;
	return mean(
		eligible.map((task) => (task.passed < k ? 0 : Math.exp(logChoose(task.passed, k) - logChoose(task.trials, k)))),
	);
}
