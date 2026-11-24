//! Ported from `test/evals/compare.test.ts`. "bun run evals compare prints the table and verdict and writes compare.md
//! into B" runs `jocasta-bench evals compare` as a process, so it is `tests/evals_cli.rs`'s.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use super::compare::{PairedStats, RuleMetric, Verdict, compare_rows, paired_stats, render_paired};
use super::harness::Usage;
use super::runner::ResultRow;
use super::variants::{Harness, default_variant, load_variants, variant_args};
use crate::lib::args::REPO_ROOT;
use crate::lib::stats::{
	BootstrapOptions, Pair, TaskTrials, holm, mcnemar_exact, median, pass_hat_k, wilcoxon_signed_rank,
};
use jocasta::shared::pathkey::resolve;
use jocasta::shared::testing::temp_dir;

#[derive(Clone, Copy, Default)]
struct Metrics {
	tokens: Option<f64>,
	wall_ms: Option<f64>,
	lines: Option<f64>,
	cost: Option<f64>,
}

fn row(id: &str, passed: bool, metrics: Metrics) -> ResultRow {
	let zero = Usage::default();
	ResultRow {
		case: id.to_owned(),
		rep: 0,
		variant: "default".to_owned(),
		harness: Harness::Omp,
		harness_version: "omp/0.0.0-test".to_owned(),
		started_at: "2026-10-24T10:00:00.000Z".to_owned(),
		model: Some("faux-1".to_owned()),
		usage: zero,
		total_usage: Usage { total_tokens: metrics.tokens.unwrap_or(100.0), cost: metrics.cost.unwrap_or(0.0), ..zero },
		requests: 1,
		turns: 1,
		tool_calls: 1,
		prompts: 1,
		wall_ms: metrics.wall_ms.unwrap_or(1000.0),
		lines_added: metrics.lines.unwrap_or(10.0) as u64,
		lines_removed: 0,
		grade: serde_json::Map::new(),
		hidden_passed: None,
		findings: None,
		passed,
		plan_review: None,
		prewalk_switched: None,
		key_exposed: None,
	}
}

fn trials(id: &str, passes: usize, total: usize, metrics: Metrics) -> Vec<ResultRow> {
	(0..total).map(|i| row(id, i < passes, metrics)).collect()
}

fn lines(lines: f64) -> Metrics {
	Metrics { lines: Some(lines), ..Metrics::default() }
}

fn tokens(tokens: f64) -> Metrics {
	Metrics { tokens: Some(tokens), ..Metrics::default() }
}

fn close(a: f64, b: f64, digits: i32) -> bool {
	(a - b).abs() < 10f64.powi(-digits) / 2.0
}

#[test]
fn a_mean_ratio_of_exactly_0_90_improves_091_doesnt() {
	let all = |tokens| Metrics { tokens: Some(tokens), wall_ms: Some(1000.0), lines: Some(10.0), cost: None };
	let a = trials("c", 4, 4, all(1000.0));
	let at_boundary = compare_rows(&a, &trials("c", 4, 4, all(900.0)));
	assert!(close(at_boundary.mean_ratios.tokens.unwrap(), 0.9, 12));
	assert_eq!(at_boundary.improved, vec![RuleMetric::Tokens]);
	assert_eq!(at_boundary.verdict, Verdict::Keep);
	let above = compare_rows(&a, &trials("c", 4, 4, all(910.0)));
	assert!(above.improved.is_empty());
	assert_eq!(above.verdict, Verdict::Revert);
}

#[test]
fn the_metric_is_the_mean_over_cases_of_each_cases_b_a_ratio() {
	// 0.8 and 1.0 average to 0.90: improves, though B's total is 1.8/2.
	let a = [trials("x", 1, 1, lines(10.0)), trials("y", 1, 1, lines(30.0))].concat();
	let b = [trials("x", 1, 1, lines(8.0)), trials("y", 1, 1, lines(30.0))].concat();
	let result = compare_rows(&a, &b);
	assert_eq!(result.cases.iter().map(|c| c.ratios.lines).collect::<Vec<_>>(), vec![Some(0.8), Some(1.0)]);
	assert_eq!(result.improved, vec![RuleMetric::Lines]);
}

#[test]
fn a_case_whose_a_mean_is_0_is_skipped_for_that_metric() {
	let a = [trials("explain", 1, 1, lines(0.0)), trials("fix", 1, 1, lines(10.0))].concat();
	let b = [trials("explain", 1, 1, lines(50.0)), trials("fix", 1, 1, lines(5.0))].concat();
	let result = compare_rows(&a, &b);
	assert_eq!(result.cases.iter().find(|c| c.case == "explain").unwrap().ratios.lines, None);
	assert_eq!(result.mean_ratios.lines, Some(0.5));
}

#[test]
fn the_pass_rate_holds_down_to_the_square_root_of_100_squared_over_na_plus_100_squared_over_nb_points_below_as_not_past_it()
 {
	let better = tokens(50.0);
	// nA = nB = 8: the tolerance is exactly 50 points.
	let at_edge = compare_rows(&trials("c", 8, 8, Metrics::default()), &trials("c", 4, 8, better));
	assert_eq!(at_edge.tolerance, 50.0);
	assert_eq!((at_edge.rate_a, at_edge.rate_b), (100.0, 50.0));
	assert!(at_edge.pass_rate_holds);
	assert_eq!(at_edge.verdict, Verdict::Keep);
	let past_edge = compare_rows(&trials("c", 8, 8, Metrics::default()), &trials("c", 3, 8, better));
	assert!(!past_edge.pass_rate_holds);
	assert_eq!(past_edge.verdict, Verdict::Revert);
}

fn write(dir: &Path, name: &str, text: &str) {
	fs::write(dir.join(name), text).unwrap();
}

fn text(path: &Path) -> String {
	resolve(path).to_string_lossy().into_owned()
}

#[test]
fn later_variants_layer_over_earlier_ones() {
	let held = temp_dir("jocasta-evals-compare");
	let dir = held.path();
	write(dir, "prompt-a.md", "a");
	write(dir, "prompt-b.md", "b");
	write(dir, "ext.ts", "");
	write(
		dir,
		"base.yaml",
		"append_system_prompt: prompt-a.md\nextensions: [ext.ts]\ntools: [read, bash]\nsettings:\n  defaultProvider: github-copilot\n  roles:\n    default: [a]\n    cheap: [b]\nenv:\n  ACME_A: one\n  ACME_B: one\n",
	);
	write(
		dir,
		"lean.yaml",
		"append_system_prompt: prompt-b.md\nextensions: [ext.ts]\ntools: [read]\nsettings:\n  roles:\n    cheap: [c]\nenv:\n  ACME_B: two\n",
	);
	let variant = load_variants(dir, &["base".to_owned(), "lean".to_owned()]).unwrap();
	let mut expected = default_variant();
	expected.label = "base+lean".to_owned();
	expected.append_system_prompt = Some(text(&dir.join("prompt-b.md")));
	expected.extensions = vec![text(&dir.join("ext.ts")), text(&dir.join("ext.ts"))];
	expected.tools = Some(vec!["read".to_owned()]);
	expected.settings = json!({ "defaultProvider": "github-copilot", "roles": { "default": ["a"], "cheap": ["c"] } })
		.as_object()
		.cloned();
	expected.env = [("ACME_A", "one"), ("ACME_B", "two")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
	assert_eq!(variant, expected);
	assert_eq!(
		variant_args(&variant),
		vec![
			"--append-system-prompt".to_owned(),
			text(&dir.join("prompt-b.md")),
			"-e".to_owned(),
			text(&dir.join("ext.ts")),
			"-e".to_owned(),
			text(&dir.join("ext.ts")),
			"--tools".to_owned(),
			"read".to_owned(),
		]
	);
	assert_eq!(load_variants(dir, &[]).unwrap(), default_variant());
}

fn refused(name: &str, variant: &str, message: &str) {
	let held = temp_dir("jocasta-evals-compare");
	let dir = held.path();
	write(dir, "base.yaml", "tools: [read]\n");
	write(dir, "bad.yaml", "modell: x\n");
	write(dir, "plan.yaml", "plan_first: true\n");
	write(dir, "both.yaml", "extends: [base]\nharness: claude\nconfigs: [base.yaml]\n");
	write(dir, "loop.yaml", "extends: [loop]\n");
	let error = load_variants(dir, &[variant.to_owned()]).expect_err(name);
	assert!(error.to_string().contains(message), "{name}: {error}");
}

#[test]
fn an_unknown_name_is_refused() {
	refused("an unknown name", "nope", "unknown variant: nope");
}

#[test]
fn a_path_for_a_name_is_refused() {
	refused("a path for a name", "../base", "unknown variant: ../base");
}

#[test]
fn an_unknown_key_is_refused() {
	refused("an unknown key", "bad", "bad.yaml:1: unknown key `modell`");
}

#[test]
fn plan_first_without_a_model_is_refused() {
	refused("plan first without a model", "plan", "plan: plan_first needs a model");
}

#[test]
fn omp_keys_on_claude_is_refused() {
	refused("omp keys on claude", "both", "both: configs, tools apply to omp only, not claude");
}

#[test]
fn a_loop_of_extends_is_refused() {
	refused("a loop of extends", "loop", "variant loop extends itself: loop → loop");
}

fn shipped(name: &str) -> super::variants::Variant {
	load_variants(&REPO_ROOT.join("evals").join("variants"), &[name.to_owned()]).unwrap()
}

fn settings_of(variant: &super::variants::Variant) -> Value {
	Value::Object(variant.settings.clone().unwrap())
}

#[test]
fn the_shipped_copilot_and_anthropic_variants_load() {
	assert_eq!(
		settings_of(&shipped("copilot")),
		json!({ "modelRoles": { "default": "github-copilot/claude-opus-5.5" } })
	);
	assert_eq!(settings_of(&shipped("anthropic")), json!({ "modelRoles": { "default": "anthropic/claude-opus-5-5" } }));
}

#[test]
fn the_arms_a0_a4_build_on_each_other_and_give_each_harness_its_flags_0482() {
	let config = |name: &str| text(&REPO_ROOT.join("config").join(name));
	let words = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect::<Vec<_>>();
	let a0 = shipped("a0");
	assert_eq!(variant_args(&a0), words(&["--model", "anthropic/claude-opus-5-5", "--thinking", "high"]));
	let a1 = shipped("a1");
	assert_eq!(
		variant_args(&a1),
		vec![
			"--model".to_owned(),
			"anthropic/claude-opus-5-5".to_owned(),
			"--config".to_owned(),
			config("guard.yml"),
			"--append-system-prompt".to_owned(),
			config("ponytail.md"),
			"--thinking".to_owned(),
			"high".to_owned(),
		]
	);
	let settings = settings_of(&a1);
	assert_eq!(settings["compaction"]["thresholdTokens"], json!(200_000));
	assert_eq!(settings["dev"]["autoqa"], json!(false));
	let a2 = shipped("a2");
	assert_eq!(
		variant_args(&a2)[..5],
		words(&[
			"--model",
			"anthropic/claude-opus-5-5",
			"--plan-yolo",
			"--plan-yolo-into",
			"anthropic/claude-opus-5-5"
		])[..]
	);
	let a3 = shipped("a3");
	assert!(a3.plan_first);
	assert_eq!(a3.followups.len(), 2);
	assert!(a3.followups[0].contains("git diff {base}"));
	assert_eq!(settings_of(&a3)["task"]["agentModelOverrides"]["reviewer"], json!(["anthropic/claude-opus-5-5"]));
	let a4 = shipped("a4");
	assert_eq!(a4.harness, Harness::Claude);
	assert_eq!(
		variant_args(&a4),
		words(&["--model", "claude-opus-5-5", "--permission-mode", "bypassPermissions", "--effort", "high"])
	);
}

/// Four tasks: A wins t1, B wins t2, a tie in majority on t3, and t4 with 2 of 4 on A (no majority) and 3 of 4 on B.
struct Sides {
	id: &'static str,
	a: (usize, usize),
	b: (usize, usize),
}

const SIDES: [Sides; 4] = [
	Sides { id: "t1", a: (3, 3), b: (0, 3) },
	Sides { id: "t2", a: (0, 3), b: (3, 3) },
	Sides { id: "t3", a: (2, 3), b: (2, 3) },
	Sides { id: "t4", a: (2, 4), b: (3, 4) },
];

fn metrics_a(i: usize) -> Metrics {
	Metrics {
		wall_ms: Some(100.0 * (i + 1) as f64),
		cost: Some(1.0),
		tokens: Some(1000.0 * (i + 1) as f64),
		lines: None,
	}
}

fn metrics_b(i: usize) -> Metrics {
	Metrics {
		wall_ms: Some(150.0 * (i + 1) as f64),
		cost: Some(0.5),
		tokens: Some(900.0 * (i + 1) as f64),
		lines: None,
	}
}

fn paired_rows() -> (Vec<ResultRow>, Vec<ResultRow>) {
	let rows_a = SIDES.iter().enumerate().flat_map(|(i, t)| trials(t.id, t.a.0, t.a.1, metrics_a(i))).collect();
	let rows_b = SIDES.iter().enumerate().flat_map(|(i, t)| trials(t.id, t.b.0, t.b.1, metrics_b(i))).collect();
	(rows_a, rows_b)
}

fn seeded() -> BootstrapOptions {
	BootstrapOptions { seed: Some(482), ..BootstrapOptions::default() }
}

#[test]
fn pass_rates_per_task_majority_votes_per_task_medians_and_holm_over_the_four_p_values_in_order() {
	let (rows_a, rows_b) = paired_rows();
	let stats = paired_stats(&rows_a, &rows_b, seeded());
	assert_eq!(stats.tasks, 4);
	assert!(close(stats.pass_rate_a, (1.0 + 0.0 + 2.0 / 3.0 + 0.5) / 4.0, 12));
	assert!(close(stats.pass_rate_b, (0.0 + 1.0 + 2.0 / 3.0 + 0.75) / 4.0, 12));
	assert!(close(stats.difference.value, stats.pass_rate_b - stats.pass_rate_a, 12));
	assert_eq!((stats.mcnemar.only_a, stats.mcnemar.only_b), (1, 2));
	assert_eq!(stats.mcnemar.p, mcnemar_exact(1, 2).p);
	let a_trials: Vec<TaskTrials> =
		SIDES.iter().map(|t| TaskTrials { passed: t.a.0 as u64, trials: t.a.1 as u64 }).collect();
	assert!(close(stats.pass_hat3.a, pass_hat_k(&a_trials, 3), 12));
	type Pick = fn(&Metrics) -> f64;
	let picks: [(&str, Pick); 3] =
		[("wallMs", |m| m.wall_ms.unwrap()), ("cost", |m| m.cost.unwrap()), ("tokens", |m| m.tokens.unwrap())];
	for (key, pick) in picks {
		let a: Vec<f64> = (0..4).map(|i| pick(&metrics_a(i))).collect();
		let b: Vec<f64> = (0..4).map(|i| pick(&metrics_b(i))).collect();
		let expected = wilcoxon_signed_rank(&a.iter().zip(&b).map(|(a, b)| Pair { a: *a, b: *b }).collect::<Vec<_>>());
		let got = match key {
			"wallMs" => stats.wilcoxon.wall_ms,
			"cost" => stats.wilcoxon.cost,
			_ => stats.wilcoxon.tokens,
		};
		assert_eq!(got.test, expected, "{key}");
		assert_eq!(got.median_a, median(&a), "{key}");
		assert_eq!(got.median_b, median(&b), "{key}");
	}
	let wall = stats.wilcoxon.wall_ms;
	assert_eq!((wall.test.n, wall.test.w_plus, wall.median_a, wall.median_b), (4, 10.0, 250.0, 375.0));
	assert_eq!(stats.wilcoxon.tokens.test.w_plus, 0.0);
	let adjusted = holm(&[
		stats.mcnemar.p,
		stats.wilcoxon.wall_ms.test.p,
		stats.wilcoxon.cost.test.p,
		stats.wilcoxon.tokens.test.p,
	]);
	assert_eq!(
		vec![
			stats.mcnemar.p_holm,
			stats.wilcoxon.wall_ms.p_holm,
			stats.wilcoxon.cost.p_holm,
			stats.wilcoxon.tokens.p_holm
		],
		adjusted
	);
}

#[test]
fn render_paired_writes_a_p_below_0_001_as_below_0_001() {
	let (rows_a, rows_b) = paired_rows();
	let stats = paired_stats(&rows_a, &rows_b, seeded());
	let mut changed: PairedStats = stats;
	changed.mcnemar.p = 0.0004;
	changed.mcnemar.p_holm = 0.0012;
	let lines = render_paired(&changed);
	assert!(lines.contains(
		&"- McNemar (exact) on per-task majority votes: only A 1, only B 2, p < 0.001, Holm 0.001.".to_owned()
	));
	assert_eq!(lines[0], "## Paired statistics (0482), 4 tasks in both runs");
}
