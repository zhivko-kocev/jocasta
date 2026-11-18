//! Ported from `test/evals/keep.test.ts`.

use serde_json::{Value, json};

use super::followup::FOLLOWUP_COMPARISONS;
use super::keep::{KeepInput, KeepTrial, cost_per_pass, followup_inputs, keep_rule, keep_trial};
use super::tasks::BenchRow;

const ALPHA: f64 = 0.05;

/// `n` paired trials keyed t0…, passing where `passes[i]`, each costing `cost`.
fn trials(passes: &[bool], cost: f64) -> Vec<KeepTrial> {
	passes.iter().enumerate().map(|(i, passed)| KeepTrial { pair: format!("t{i}"), passed: *passed, cost }).collect()
}

fn all(n: usize, value: bool) -> Vec<bool> {
	vec![value; n]
}

fn input(name: &str, with_feature: Vec<KeepTrial>, without_feature: Vec<KeepTrial>) -> KeepInput {
	KeepInput { name: name.to_owned(), with_feature, without_feature }
}

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 1e-12, "{actual} is not {expected}");
}

#[test]
fn cost_per_passing_task_is_summed_cost_over_passes_no_pass_is_infinitely_expensive() {
	assert_eq!(cost_per_pass(&trials(&[true, true, false, false], 1.0)), 2.0);
	assert_eq!(cost_per_pass(&trials(&[false, false], 1.0)), f64::INFINITY);
}

#[test]
fn a_feature_stays_when_cheaper_per_pass_and_not_significantly_worse_it_goes_when_dearer() {
	let verdicts = keep_rule(
		&[
			input("cheaper", trials(&all(20, true), 0.4), trials(&all(20, true), 0.5)),
			input("dearer", trials(&all(20, true), 0.6), trials(&all(20, true), 0.5)),
		],
		ALPHA,
	);
	let (cheaper, dearer) = (&verdicts[0], &verdicts[1]);
	assert_eq!(
		(cheaper.pairs, cheaper.cheaper, cheaper.significantly_worse, cheaper.keep, cheaper.p, cheaper.p_holm),
		(20, true, false, true, 1.0, 1.0)
	);
	close(cheaper.cost_per_pass_with, 0.4);
	assert_eq!((dearer.cheaper, dearer.keep), (false, false));
}

#[test]
fn a_cheaper_feature_whose_pass_rate_is_significantly_lower_after_holms_correction_goes_0543() {
	// With the feature, 12 of 20 pairs that passed without it fail: McNemar p ≈ 0.0005, cheap only because it fails fast.
	let with_passes: Vec<bool> = all(8, true).into_iter().chain(all(12, false)).collect();
	let verdicts = keep_rule(
		&[
			input("worse", trials(&with_passes, 0.05), trials(&all(20, true), 0.5)),
			input("fine", trials(&all(20, true), 0.4), trials(&all(20, true), 0.5)),
		],
		ALPHA,
	);
	let (worse, fine) = (&verdicts[0], &verdicts[1]);
	assert_eq!(
		(worse.only_with, worse.only_without, worse.cheaper, worse.significantly_worse, worse.keep),
		(0, 12, true, true, false)
	);
	close(worse.p, 2.0 * 0.5_f64.powi(12));
	close(worse.p_holm, 2.0 * 2.0 * 0.5_f64.powi(12));
	assert!(fine.keep);
}

#[test]
fn holms_correction_can_leave_a_small_loss_not_significant_across_the_family() {
	// 5 discordant pairs against: p = 0.0625 alone, so not significant; the feature is cheaper and stays.
	let with_passes: Vec<bool> = all(15, true).into_iter().chain(all(5, false)).collect();
	let verdicts = keep_rule(
		&[
			input("x", trials(&with_passes, 0.3), trials(&all(20, true), 0.5)),
			input("y", trials(&all(20, true), 0.5), trials(&all(20, true), 0.5)),
		],
		ALPHA,
	);
	let verdict = &verdicts[0];
	assert_eq!((verdict.significantly_worse, verdict.cheaper, verdict.keep), (false, true, true));
	close(verdict.p_holm, 0.125);
}

#[test]
fn only_pairs_on_both_sides_count() {
	let verdicts = keep_rule(&[input("x", trials(&all(3, true), 1.0), trials(&all(2, true), 2.0))], ALPHA);
	assert_eq!(verdicts[0].pairs, 2);
}

/// A row with the fields `keepTrial` reads, the rest of a full row filled in.
fn row(arm: &str, task: &str, rep: u64, passed: bool, cost: Option<f64>) -> BenchRow {
	let usage = json!({ "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 2, "cost": 0 });
	let ledger = cost.map(|cost| {
		json!({
			"requests": 1, "toolCalls": 0, "input": 1, "cacheRead": 0, "cacheWrite": 0, "output": 1, "cost": cost,
			"cacheReadShare": null, "unpricedModels": [],
			"peakContext": { "tokens": 1, "window": null, "percent": null },
			"waste": {
				"rereads": 0, "repeatedCalls": 0, "oversizedResults": 0, "repeatedFailures": 0, "cacheMisses": 0,
				"cacheMissIdleMinutes": [], "compactions": 0,
			},
			"sessions": 1,
		})
	});
	serde_json::from_value(json!({
		"case": task, "rep": rep, "variant": arm, "harness": "omp", "harnessVersion": "omp/0.0.0-stub",
		"startedAt": "2026-10-27T10:00:00.000Z", "model": "stub-model", "usage": usage, "totalUsage": usage,
		"requests": 1, "turns": 1, "toolCalls": 0, "prompts": 1, "wallMs": 1, "linesAdded": 0, "linesRemoved": 0,
		"grade": {}, "hiddenPassed": passed, "findings": null, "passed": passed,
		"task": task, "arm": arm, "language": "Go", "attempt": 1, "trialIndex": 0, "leak": [],
		"ledger": ledger.unwrap_or(Value::Null),
	}))
	.unwrap()
}

#[test]
fn each_follow_up_comparison_pairs_with_its_own_baseline_a6s_arms_with_a6_the_prewalk_and_thinking_arms_with_a1_0608_0620()
 {
	let rows = [
		row("a6", "a", 0, false, Some(2.0)),
		row("a1", "a", 0, true, Some(3.0)),
		row("f-prewalk-clean", "a", 0, true, Some(0.5)),
		row("f-keep-smol", "a", 0, true, Some(1.0)),
	];
	let inputs = followup_inputs(&rows).unwrap();
	assert_eq!(
		inputs.iter().map(|input| input.name.as_str()).collect::<Vec<_>>(),
		FOLLOWUP_COMPARISONS.iter().map(|comparison| comparison.arm).collect::<Vec<_>>()
	);
	let baselines: Vec<(&str, &str)> =
		FOLLOWUP_COMPARISONS.iter().map(|comparison| (comparison.arm, comparison.baseline)).collect();
	assert_eq!(
		baselines,
		[
			("f-keep-context", "a6"),
			("f-keep-smol", "a6"),
			("f-ponytail-if", "a6"),
			("f-prewalk-clean", "a1"),
			("f-advisor", "a6"),
			("f-think-auto", "a1"),
		]
	);
	let named = |name: &str| inputs.iter().find(|input| input.name == name).unwrap();
	let prewalk = named("f-prewalk-clean");
	assert_eq!(prewalk.with_feature, [KeepTrial { pair: "a/0".to_owned(), passed: true, cost: 0.5 }]);
	assert_eq!(prewalk.without_feature, [KeepTrial { pair: "a/0".to_owned(), passed: true, cost: 3.0 }]);
	let smol = named("f-keep-smol");
	assert_eq!(smol.without_feature, [KeepTrial { pair: "a/0".to_owned(), passed: false, cost: 2.0 }]);
	let error = keep_trial(&row("a1", "a", 0, true, None)).unwrap_err().to_string();
	assert!(error.contains("no ledger"), "{error}");
}
