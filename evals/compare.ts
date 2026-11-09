import { existsSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import {
	holm,
	type Interval,
	mcnemarExact,
	median,
	pairedBootstrap,
	passHatK,
	type WilcoxonResult,
	wilcoxonSignedRank,
} from "../bench/lib/stats.ts";
import { UserError } from "../src/shared/errors.ts";
import { readJsonlFile } from "../src/shared/jsonl.ts";
import { MEASUREMENT_NOTES, type ResultRow } from "./runner.ts";

/**
 * `bun run evals compare <runDirA> <runDirB>` (0375): per case, passes and the B/A ratios of the mean tokens, wall
 * time, tool calls and lines changed, then the keep rule's verdict; then the pre-registered paired statistics (0482).
 */

/** A metric improves when the mean over cases of (mean B ÷ mean A) is at most this (0375). */
export const IMPROVES_AT = 0.9;
/** Float slack, so a ratio of exactly 0.90 or a rate exactly at the tolerance counts as met. */
const EPSILON = 1e-9;

type Metric = "tokens" | "wallMs" | "toolCalls" | "lines";
/** The metrics the keep rule reads; tool calls are shown only. */
const RULE_METRICS = ["tokens", "wallMs", "lines"] as const satisfies readonly Metric[];

const metricOf: Record<Metric, (row: ResultRow) => number> = {
	tokens: (row) => row.totalUsage.totalTokens,
	wallMs: (row) => row.wallMs,
	toolCalls: (row) => row.toolCalls,
	lines: (row) => row.linesAdded + row.linesRemoved,
};

export interface CaseComparison {
	case: string;
	passedA: number;
	trialsA: number;
	passedB: number;
	trialsB: number;
	/** mean B ÷ mean A; undefined when A's mean is 0 or either run lacks the case. */
	ratios: Record<Metric, number | undefined>;
}

export interface Comparison {
	cases: CaseComparison[];
	rateA: number;
	rateB: number;
	/** √(100²/nA + 100²/nB) points. */
	tolerance: number;
	passRateHolds: boolean;
	/** The mean over cases of the ratios, per rule metric; undefined when no case has one. */
	meanRatios: Record<(typeof RULE_METRICS)[number], number | undefined>;
	improved: (typeof RULE_METRICS)[number][];
	verdict: "keep" | "revert";
}

const mean = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;

/** Applies 0375's keep rule to two runs' rows. */
export function compareRows(rowsA: ResultRow[], rowsB: ResultRow[]): Comparison {
	const ids = [...new Set([...rowsA, ...rowsB].map((row) => row.case))];
	const cases = ids.map((id): CaseComparison => {
		const a = rowsA.filter((row) => row.case === id);
		const b = rowsB.filter((row) => row.case === id);
		const ratio = (metric: Metric) => {
			if (a.length === 0 || b.length === 0) return undefined;
			const meanA = mean(a.map(metricOf[metric]));
			return meanA === 0 ? undefined : mean(b.map(metricOf[metric])) / meanA;
		};
		return {
			case: id,
			passedA: a.filter((row) => row.passed).length,
			trialsA: a.length,
			passedB: b.filter((row) => row.passed).length,
			trialsB: b.length,
			ratios: {
				tokens: ratio("tokens"),
				wallMs: ratio("wallMs"),
				toolCalls: ratio("toolCalls"),
				lines: ratio("lines"),
			},
		};
	});
	// compareRunDirs refuses empty runs, so neither divides by 0.
	const rateA = (100 * rowsA.filter((row) => row.passed).length) / rowsA.length;
	const rateB = (100 * rowsB.filter((row) => row.passed).length) / rowsB.length;
	const tolerance = Math.sqrt(100 ** 2 / rowsA.length + 100 ** 2 / rowsB.length);
	const passRateHolds = rateB >= rateA - tolerance - EPSILON;
	const meanRatio = (metric: Metric) => {
		const ratios = cases.map((c) => c.ratios[metric]).filter((r): r is number => r !== undefined);
		return ratios.length === 0 ? undefined : mean(ratios);
	};
	const meanRatios = { tokens: meanRatio("tokens"), wallMs: meanRatio("wallMs"), lines: meanRatio("lines") };
	const improved = RULE_METRICS.filter((metric) => {
		const value = meanRatios[metric];
		return value !== undefined && value <= IMPROVES_AT + EPSILON;
	});
	return {
		cases,
		rateA,
		rateB,
		tolerance,
		passRateHolds,
		meanRatios,
		improved,
		verdict: passRateHolds && improved.length > 0 ? "keep" : "revert",
	};
}

/** The secondary metrics compared per task by the Wilcoxon signed-rank test (0482). */
const PAIRED_METRICS = {
	wallMs: (row: ResultRow) => row.wallMs,
	cost: (row: ResultRow) => row.totalUsage.cost,
	tokens: (row: ResultRow) => row.totalUsage.totalTokens,
} as const;

type PairedMetric = keyof typeof PAIRED_METRICS;

/**
 * The pre-registered paired analysis (0482) over the tasks both runs have: the difference in per-task pass rate (B − A)
 * with a paired task bootstrap CI, pass^3 per arm, McNemar's exact test on per-task majority votes, and Wilcoxon
 * signed-rank tests on per-task median wall time, cost and tokens; the four secondary p-values Holm-adjusted together.
 * The mixed-effects regression runs outside this tool on `results.jsonl`.
 */
export interface PairedStats {
	tasks: number;
	passRateA: number;
	passRateB: number;
	difference: { value: number; ci95: Interval };
	passHat3: { a: number; b: number };
	mcnemar: { onlyA: number; onlyB: number; p: number; pHolm: number };
	wilcoxon: Record<PairedMetric, WilcoxonResult & { medianA: number; medianB: number; pHolm: number }>;
}

export function pairedStats(
	rowsA: ResultRow[],
	rowsB: ResultRow[],
	options: { resamples?: number; seed?: number } = {},
): PairedStats {
	const ids = [...new Set(rowsA.map((row) => row.case))].filter((id) => rowsB.some((row) => row.case === id));
	const tasks = ids.map((id) => ({
		a: rowsA.filter((row) => row.case === id),
		b: rowsB.filter((row) => row.case === id),
	}));
	const passRate = (rows: ResultRow[]) => rows.filter((row) => row.passed).length / rows.length;
	const majority = (rows: ResultRow[]) => rows.filter((row) => row.passed).length * 2 > rows.length;
	const rates = tasks.map((task) => ({ a: passRate(task.a), b: passRate(task.b) }));
	const bootstrap = pairedBootstrap(rates, options);
	const passHat3 = (side: "a" | "b") =>
		passHatK(
			tasks.map((task) => ({ passed: task[side].filter((row) => row.passed).length, trials: task[side].length })),
			3,
		);
	const onlyA = tasks.filter((task) => majority(task.a) && !majority(task.b)).length;
	const onlyB = tasks.filter((task) => !majority(task.a) && majority(task.b)).length;
	const mcnemar = mcnemarExact(onlyA, onlyB);
	const metrics = Object.keys(PAIRED_METRICS) as PairedMetric[];
	const tests = metrics.map((metric) => {
		const pairs = tasks.map((task) => ({
			a: median(task.a.map(PAIRED_METRICS[metric])),
			b: median(task.b.map(PAIRED_METRICS[metric])),
		}));
		return {
			...wilcoxonSignedRank(pairs),
			medianA: median(pairs.map((pair) => pair.a)),
			medianB: median(pairs.map((pair) => pair.b)),
		};
	});
	const adjusted = holm([mcnemar.p, ...tests.map((test) => test.p)]);
	return {
		tasks: tasks.length,
		passRateA: rates.length === 0 ? Number.NaN : rates.reduce((sum, r) => sum + r.a, 0) / rates.length,
		passRateB: rates.length === 0 ? Number.NaN : rates.reduce((sum, r) => sum + r.b, 0) / rates.length,
		difference: { value: bootstrap.difference, ci95: bootstrap.ci95 },
		passHat3: { a: passHat3("a"), b: passHat3("b") },
		mcnemar: { onlyA, onlyB, p: mcnemar.p, pHolm: adjusted[0] ?? 1 },
		wilcoxon: Object.fromEntries(
			metrics.map((metric, i) => [metric, { ...(tests[i] as (typeof tests)[number]), pHolm: adjusted[i + 1] ?? 1 }]),
		) as PairedStats["wilcoxon"],
	};
}

const points = (value: number) => (Number.isNaN(value) ? "—" : `${(100 * value).toFixed(1)}`);
const pText = (value: number) => (value < 0.001 ? "< 0.001" : value.toFixed(3));

const PAIRED_NAMES: Record<PairedMetric, string> = { wallMs: "Wall time (ms)", cost: "Cost ($)", tokens: "Tokens" };

/** The paired statistics as markdown. */
export function renderPaired(stats: PairedStats): string[] {
	return [
		`## Paired statistics (0482), ${stats.tasks} tasks in both runs`,
		"",
		`- Pass rate per task: A ${points(stats.passRateA)}%, B ${points(stats.passRateB)}%; B − A ${points(stats.difference.value)} points, paired task bootstrap 95% CI [${points(stats.difference.ci95.low)}, ${points(stats.difference.ci95.high)}].`,
		`- pass^3: A ${points(stats.passHat3.a)}%, B ${points(stats.passHat3.b)}% (tasks with at least 3 trials).`,
		`- McNemar (exact) on per-task majority votes: only A ${stats.mcnemar.onlyA}, only B ${stats.mcnemar.onlyB}, p ${pText(stats.mcnemar.p)}, Holm ${pText(stats.mcnemar.pHolm)}.`,
		"",
		"| Metric (per-task median) | Median A | Median B | Wilcoxon n | W+ | p | Holm p |",
		"|---|---|---|---|---|---|---|",
		...(Object.keys(PAIRED_NAMES) as PairedMetric[]).map((metric) => {
			const test = stats.wilcoxon[metric];
			const value = (n: number) => (metric === "cost" ? n.toFixed(4) : String(Math.round(n)));
			return `| ${PAIRED_NAMES[metric]} | ${value(test.medianA)} | ${value(test.medianB)} | ${test.n} | ${test.wPlus} | ${pText(test.p)} (${test.method}) | ${pText(test.pHolm)} |`;
		}),
		"",
	];
}

const ratioText = (value: number | undefined) => (value === undefined ? "—" : value.toFixed(2));

const METRIC_NAMES: Record<(typeof RULE_METRICS)[number], string> = {
	tokens: "Tokens",
	wallMs: "Wall time",
	lines: "Lines changed",
};

/** The comparison as markdown: one row per case, the summary, the verdict, the paired statistics and the notes. */
export function renderComparison(comparison: Comparison, labelA: string, labelB: string, paired?: PairedStats): string {
	const rows = comparison.cases.map(
		(c) =>
			`| ${c.case} | ${c.passedA}/${c.trialsA} | ${c.passedB}/${c.trialsB} | ${ratioText(c.ratios.tokens)} | ${ratioText(c.ratios.wallMs)} | ${ratioText(c.ratios.toolCalls)} | ${ratioText(c.ratios.lines)} |`,
	);
	const metrics = RULE_METRICS.map((metric) => {
		const value = comparison.meanRatios[metric];
		const state =
			value === undefined
				? "no case to compare"
				: comparison.improved.includes(metric)
					? "improves"
					: "doesn't improve";
		return `- ${METRIC_NAMES[metric]}: mean B/A ${ratioText(value)}, ${state} (≤ ${IMPROVES_AT.toFixed(2)} improves).`;
	});
	return [
		`# Eval comparison: A ${labelA}, B ${labelB}`,
		"",
		"| Case | Passes A | Passes B | Tokens B/A | Wall time B/A | Tool calls B/A | Lines B/A |",
		"|---|---|---|---|---|---|---|",
		...rows,
		"",
		`- Pass rate: A ${comparison.rateA.toFixed(1)}%, B ${comparison.rateB.toFixed(1)}%, tolerance ${comparison.tolerance.toFixed(1)} points: ${comparison.passRateHolds ? "holds" : "drops"}.`,
		...metrics,
		"",
		`**Verdict: ${comparison.verdict}.**`,
		"",
		...(paired ? renderPaired(paired) : []),
		"Notes:",
		...MEASUREMENT_NOTES.map((note) => `- ${note}`),
		"",
	].join("\n");
}

function readRows(dir: string): ResultRow[] {
	const file = join(dir, "results.jsonl");
	if (!existsSync(file)) throw new UserError(`no results.jsonl in ${dir}`);
	return readJsonlFile<ResultRow>(file);
}

/** Compares two run folders; writes `<runDirB>/compare.md` and returns its text. */
export function compareRunDirs(dirA: string, dirB: string): string {
	const rowsA = readRows(dirA);
	const rowsB = readRows(dirB);
	if (rowsA.length === 0 || rowsB.length === 0) throw new UserError("a run to compare has no trials");
	const text = renderComparison(
		compareRows(rowsA, rowsB),
		basename(resolve(dirA)),
		basename(resolve(dirB)),
		pairedStats(rowsA, rowsB),
	);
	writeFileSync(join(dirB, "compare.md"), text);
	return text;
}
