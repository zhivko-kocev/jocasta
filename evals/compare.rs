//! `jocasta-bench evals compare <runDirA> <runDirB>` (0375): per case, passes and the B/A ratios of the mean tokens,
//! wall time, tool calls and lines changed, then the keep rule's verdict; then the pre-registered paired statistics
//! (0482).

use std::fs;
use std::path::Path;

use super::report::{js_text, to_fixed};
use super::runner::{MEASUREMENT_NOTES, ResultRow};
use crate::lib::stats::{
	BootstrapOptions, Interval, Pair, TaskTrials, Wilcoxon, WilcoxonMethod, holm, mcnemar_exact, median,
	paired_bootstrap, pass_hat_k, wilcoxon_signed_rank,
};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::jsonl::read_jsonl_file;
use jocasta::shared::pathkey::resolve;

/// A metric improves when the mean over cases of (mean B ÷ mean A) is at most this (0375).
const IMPROVES_AT: f64 = 0.9;
/// Float slack, so a ratio of exactly 0.90 or a rate exactly at the tolerance counts as met.
const EPSILON: f64 = 1e-9;

/// The metrics the keep rule reads; tool calls are shown only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleMetric {
	Tokens,
	WallMs,
	Lines,
}

const RULE_METRICS: [RuleMetric; 3] = [RuleMetric::Tokens, RuleMetric::WallMs, RuleMetric::Lines];

impl RuleMetric {
	fn name(self) -> &'static str {
		match self {
			Self::Tokens => "Tokens",
			Self::WallMs => "Wall time",
			Self::Lines => "Lines changed",
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum Metric {
	Tokens,
	WallMs,
	ToolCalls,
	Lines,
}

fn metric_of(metric: Metric, row: &ResultRow) -> f64 {
	match metric {
		Metric::Tokens => row.total_usage.total_tokens,
		Metric::WallMs => row.wall_ms,
		Metric::ToolCalls => row.tool_calls as f64,
		Metric::Lines => (row.lines_added + row.lines_removed) as f64,
	}
}

/// mean B ÷ mean A per metric; `None` when A's mean is 0 or either run lacks the case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratios {
	pub tokens: Option<f64>,
	pub wall_ms: Option<f64>,
	pub tool_calls: Option<f64>,
	pub lines: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct CaseComparison {
	pub case: String,
	pub passed_a: usize,
	pub trials_a: usize,
	pub passed_b: usize,
	pub trials_b: usize,
	pub ratios: Ratios,
}

/// The mean over cases of the ratios, per rule metric; `None` when no case has one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeanRatios {
	pub tokens: Option<f64>,
	pub wall_ms: Option<f64>,
	pub lines: Option<f64>,
}

impl MeanRatios {
	fn of(&self, metric: RuleMetric) -> Option<f64> {
		match metric {
			RuleMetric::Tokens => self.tokens,
			RuleMetric::WallMs => self.wall_ms,
			RuleMetric::Lines => self.lines,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
	Keep,
	Revert,
}

impl Verdict {
	fn text(self) -> &'static str {
		match self {
			Self::Keep => "keep",
			Self::Revert => "revert",
		}
	}
}

#[derive(Debug, Clone)]
pub struct Comparison {
	pub cases: Vec<CaseComparison>,
	pub rate_a: f64,
	pub rate_b: f64,
	/// √(100²/nA + 100²/nB) points.
	pub tolerance: f64,
	pub pass_rate_holds: bool,
	pub mean_ratios: MeanRatios,
	pub improved: Vec<RuleMetric>,
	pub verdict: Verdict,
}

fn mean(values: &[f64]) -> f64 {
	values.iter().sum::<f64>() / values.len() as f64
}

fn unique_cases<'a>(rows: impl Iterator<Item = &'a ResultRow>) -> Vec<&'a str> {
	let mut ids: Vec<&str> = Vec::new();
	for row in rows {
		if !ids.contains(&row.case.as_str()) {
			ids.push(&row.case);
		}
	}
	ids
}

fn passed_of(rows: &[&ResultRow]) -> usize {
	rows.iter().filter(|row| row.passed).count()
}

/// Applies 0375's keep rule to two runs' rows.
pub fn compare_rows(rows_a: &[ResultRow], rows_b: &[ResultRow]) -> Comparison {
	let cases: Vec<CaseComparison> = unique_cases(rows_a.iter().chain(rows_b))
		.into_iter()
		.map(|id| {
			let a: Vec<&ResultRow> = rows_a.iter().filter(|row| row.case == id).collect();
			let b: Vec<&ResultRow> = rows_b.iter().filter(|row| row.case == id).collect();
			let ratio = |metric: Metric| {
				if a.is_empty() || b.is_empty() {
					return None;
				}
				let mean_a = mean(&a.iter().map(|row| metric_of(metric, row)).collect::<Vec<_>>());
				if mean_a == 0.0 {
					None
				} else {
					Some(mean(&b.iter().map(|row| metric_of(metric, row)).collect::<Vec<_>>()) / mean_a)
				}
			};
			CaseComparison {
				case: id.to_owned(),
				passed_a: passed_of(&a),
				trials_a: a.len(),
				passed_b: passed_of(&b),
				trials_b: b.len(),
				ratios: Ratios {
					tokens: ratio(Metric::Tokens),
					wall_ms: ratio(Metric::WallMs),
					tool_calls: ratio(Metric::ToolCalls),
					lines: ratio(Metric::Lines),
				},
			}
		})
		.collect();
	// compare_run_dirs refuses empty runs, so neither divides by 0.
	let rate_a = 100.0 * rows_a.iter().filter(|row| row.passed).count() as f64 / rows_a.len() as f64;
	let rate_b = 100.0 * rows_b.iter().filter(|row| row.passed).count() as f64 / rows_b.len() as f64;
	let tolerance = (100f64.powi(2) / rows_a.len() as f64 + 100f64.powi(2) / rows_b.len() as f64).sqrt();
	let pass_rate_holds = rate_b >= rate_a - tolerance - EPSILON;
	let mean_ratio = |pick: fn(&Ratios) -> Option<f64>| {
		let ratios: Vec<f64> = cases.iter().filter_map(|c| pick(&c.ratios)).collect();
		if ratios.is_empty() { None } else { Some(mean(&ratios)) }
	};
	let mean_ratios = MeanRatios {
		tokens: mean_ratio(|r| r.tokens),
		wall_ms: mean_ratio(|r| r.wall_ms),
		lines: mean_ratio(|r| r.lines),
	};
	let improved: Vec<RuleMetric> = RULE_METRICS
		.into_iter()
		.filter(|metric| mean_ratios.of(*metric).is_some_and(|value| value <= IMPROVES_AT + EPSILON))
		.collect();
	let verdict = if pass_rate_holds && !improved.is_empty() { Verdict::Keep } else { Verdict::Revert };
	Comparison { cases, rate_a, rate_b, tolerance, pass_rate_holds, mean_ratios, improved, verdict }
}

/// One secondary metric's Wilcoxon test with the per-task medians and the Holm-adjusted p.
#[derive(Debug, Clone, Copy)]
pub struct PairedWilcoxon {
	pub test: Wilcoxon,
	pub median_a: f64,
	pub median_b: f64,
	pub p_holm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PairedMcnemar {
	pub only_a: u64,
	pub only_b: u64,
	pub p: f64,
	pub p_holm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Difference {
	pub value: f64,
	pub ci95: Interval,
}

#[derive(Debug, Clone, Copy)]
pub struct PassHat3 {
	pub a: f64,
	pub b: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PairedWilcoxons {
	pub wall_ms: PairedWilcoxon,
	pub cost: PairedWilcoxon,
	pub tokens: PairedWilcoxon,
}

/// The pre-registered paired analysis (0482) over the tasks both runs have: the difference in per-task pass rate (B − A)
/// with a paired task bootstrap CI, pass^3 per arm, McNemar's exact test on per-task majority votes, and Wilcoxon
/// signed-rank tests on per-task median wall time, cost and tokens; the four secondary p-values Holm-adjusted together.
/// The mixed-effects regression runs outside this tool on `results.jsonl`.
#[derive(Debug, Clone, Copy)]
pub struct PairedStats {
	pub tasks: usize,
	pub pass_rate_a: f64,
	pub pass_rate_b: f64,
	pub difference: Difference,
	pub pass_hat3: PassHat3,
	pub mcnemar: PairedMcnemar,
	pub wilcoxon: PairedWilcoxons,
}

fn pass_rate(rows: &[&ResultRow]) -> f64 {
	passed_of(rows) as f64 / rows.len() as f64
}

fn majority(rows: &[&ResultRow]) -> bool {
	passed_of(rows) * 2 > rows.len()
}

pub fn paired_stats(rows_a: &[ResultRow], rows_b: &[ResultRow], options: BootstrapOptions) -> PairedStats {
	let ids: Vec<&str> =
		unique_cases(rows_a.iter()).into_iter().filter(|id| rows_b.iter().any(|row| row.case == *id)).collect();
	let tasks: Vec<(Vec<&ResultRow>, Vec<&ResultRow>)> = ids
		.iter()
		.map(|id| {
			(
				rows_a.iter().filter(|row| row.case == *id).collect(),
				rows_b.iter().filter(|row| row.case == *id).collect(),
			)
		})
		.collect();
	let rates: Vec<Pair> = tasks.iter().map(|(a, b)| Pair { a: pass_rate(a), b: pass_rate(b) }).collect();
	let bootstrap = paired_bootstrap(&rates, options);
	let pass_hat3 = |first: bool| {
		let trials: Vec<TaskTrials> = tasks
			.iter()
			.map(|task| {
				let side = if first { &task.0 } else { &task.1 };
				TaskTrials { passed: passed_of(side) as u64, trials: side.len() as u64 }
			})
			.collect();
		pass_hat_k(&trials, 3)
	};
	let only_a = tasks.iter().filter(|(a, b)| majority(a) && !majority(b)).count() as u64;
	let only_b = tasks.iter().filter(|(a, b)| !majority(a) && majority(b)).count() as u64;
	let mcnemar = mcnemar_exact(only_a, only_b);
	let test = |metric: fn(&ResultRow) -> f64| {
		let pairs: Vec<Pair> = tasks
			.iter()
			.map(|(a, b)| Pair {
				a: median(&a.iter().map(|row| metric(row)).collect::<Vec<_>>()),
				b: median(&b.iter().map(|row| metric(row)).collect::<Vec<_>>()),
			})
			.collect();
		(
			wilcoxon_signed_rank(&pairs),
			median(&pairs.iter().map(|pair| pair.a).collect::<Vec<_>>()),
			median(&pairs.iter().map(|pair| pair.b).collect::<Vec<_>>()),
		)
	};
	let tests = [test(|row| row.wall_ms), test(|row| row.total_usage.cost), test(|row| row.total_usage.total_tokens)];
	let adjusted = holm(&[mcnemar.p, tests[0].0.p, tests[1].0.p, tests[2].0.p]);
	let adjust = |i: usize| adjusted.get(i).copied().unwrap_or(1.0);
	let paired = |i: usize| PairedWilcoxon {
		test: tests[i].0,
		median_a: tests[i].1,
		median_b: tests[i].2,
		p_holm: adjust(i + 1),
	};
	let average = |pick: fn(&Pair) -> f64| {
		if rates.is_empty() { f64::NAN } else { rates.iter().map(pick).sum::<f64>() / rates.len() as f64 }
	};
	PairedStats {
		tasks: tasks.len(),
		pass_rate_a: average(|r| r.a),
		pass_rate_b: average(|r| r.b),
		difference: Difference { value: bootstrap.difference, ci95: bootstrap.ci95 },
		pass_hat3: PassHat3 { a: pass_hat3(true), b: pass_hat3(false) },
		mcnemar: PairedMcnemar { only_a, only_b, p: mcnemar.p, p_holm: adjust(0) },
		wilcoxon: PairedWilcoxons { wall_ms: paired(0), cost: paired(1), tokens: paired(2) },
	}
}

fn points(value: f64) -> String {
	if value.is_nan() { "—".to_owned() } else { to_fixed(100.0 * value, 1) }
}

fn p_text(value: f64) -> String {
	if value < 0.001 { "< 0.001".to_owned() } else { to_fixed(value, 3) }
}

/// The paired statistics as markdown.
pub fn render_paired(stats: &PairedStats) -> Vec<String> {
	let ci = &stats.difference.ci95;
	let mut lines = vec![
		format!("## Paired statistics (0482), {} tasks in both runs", stats.tasks),
		String::new(),
		format!(
			"- Pass rate per task: A {}%, B {}%; B − A {} points, paired task bootstrap 95% CI [{}, {}].",
			points(stats.pass_rate_a),
			points(stats.pass_rate_b),
			points(stats.difference.value),
			points(ci.low),
			points(ci.high)
		),
		format!(
			"- pass^3: A {}%, B {}% (tasks with at least 3 trials).",
			points(stats.pass_hat3.a),
			points(stats.pass_hat3.b)
		),
		format!(
			"- McNemar (exact) on per-task majority votes: only A {}, only B {}, p {}, Holm {}.",
			stats.mcnemar.only_a,
			stats.mcnemar.only_b,
			p_text(stats.mcnemar.p),
			p_text(stats.mcnemar.p_holm)
		),
		String::new(),
		"| Metric (per-task median) | Median A | Median B | Wilcoxon n | W+ | p | Holm p |".to_owned(),
		"|---|---|---|---|---|---|---|".to_owned(),
	];
	let rows = [
		("Wall time (ms)", &stats.wilcoxon.wall_ms, false),
		("Cost ($)", &stats.wilcoxon.cost, true),
		("Tokens", &stats.wilcoxon.tokens, false),
	];
	for (name, test, is_cost) in rows {
		let value = |n: f64| if is_cost { to_fixed(n, 4) } else { js_text((n + 0.5).floor()) };
		let method = match test.test.method {
			WilcoxonMethod::Exact => "exact",
			WilcoxonMethod::Normal => "normal",
		};
		lines.push(format!(
			"| {name} | {} | {} | {} | {} | {} ({method}) | {} |",
			value(test.median_a),
			value(test.median_b),
			test.test.n,
			js_text(test.test.w_plus),
			p_text(test.test.p),
			p_text(test.p_holm)
		));
	}
	lines.push(String::new());
	lines
}

fn ratio_text(value: Option<f64>) -> String {
	value.map_or_else(|| "—".to_owned(), |value| to_fixed(value, 2))
}

/// The comparison as markdown: one row per case, the summary, the verdict, the paired statistics and the notes.
fn render_comparison(comparison: &Comparison, label_a: &str, label_b: &str, paired: Option<&PairedStats>) -> String {
	let mut lines: Vec<String> = vec![
		format!("# Eval comparison: A {label_a}, B {label_b}"),
		String::new(),
		"| Case | Passes A | Passes B | Tokens B/A | Wall time B/A | Tool calls B/A | Lines B/A |".to_owned(),
		"|---|---|---|---|---|---|---|".to_owned(),
	];
	for c in &comparison.cases {
		lines.push(format!(
			"| {} | {}/{} | {}/{} | {} | {} | {} | {} |",
			c.case,
			c.passed_a,
			c.trials_a,
			c.passed_b,
			c.trials_b,
			ratio_text(c.ratios.tokens),
			ratio_text(c.ratios.wall_ms),
			ratio_text(c.ratios.tool_calls),
			ratio_text(c.ratios.lines)
		));
	}
	lines.push(String::new());
	lines.push(format!(
		"- Pass rate: A {}%, B {}%, tolerance {} points: {}.",
		to_fixed(comparison.rate_a, 1),
		to_fixed(comparison.rate_b, 1),
		to_fixed(comparison.tolerance, 1),
		if comparison.pass_rate_holds { "holds" } else { "drops" }
	));
	for metric in RULE_METRICS {
		let value = comparison.mean_ratios.of(metric);
		let state = if value.is_none() {
			"no case to compare"
		} else if comparison.improved.contains(&metric) {
			"improves"
		} else {
			"doesn't improve"
		};
		lines.push(format!(
			"- {}: mean B/A {}, {state} (≤ {} improves).",
			metric.name(),
			ratio_text(value),
			to_fixed(IMPROVES_AT, 2)
		));
	}
	lines.push(String::new());
	lines.push(format!("**Verdict: {}.**", comparison.verdict.text()));
	lines.push(String::new());
	if let Some(paired) = paired {
		lines.extend(render_paired(paired));
	}
	lines.push("Notes:".to_owned());
	lines.extend(MEASUREMENT_NOTES.iter().map(|note| format!("- {note}")));
	lines.push(String::new());
	lines.join("\n")
}

fn read_rows(dir: &Path) -> Result<Vec<ResultRow>> {
	let file = dir.join("results.jsonl");
	if !file.exists() {
		return Err(Error::user(format!("no results.jsonl in {}", dir.display())));
	}
	read_jsonl_file(&file)
}

fn label_of(dir: &Path) -> String {
	resolve(dir).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Compares two run folders; writes `<runDirB>/compare.md` and returns its text.
pub fn compare_run_dirs(dir_a: &Path, dir_b: &Path) -> Result<String> {
	let rows_a = read_rows(dir_a)?;
	let rows_b = read_rows(dir_b)?;
	if rows_a.is_empty() || rows_b.is_empty() {
		return Err(Error::user("a run to compare has no trials"));
	}
	let text = render_comparison(
		&compare_rows(&rows_a, &rows_b),
		&label_of(dir_a),
		&label_of(dir_b),
		Some(&paired_stats(&rows_a, &rows_b, BootstrapOptions::default())),
	);
	fs::write(dir_b.join("compare.md"), &text)?;
	Ok(text)
}
