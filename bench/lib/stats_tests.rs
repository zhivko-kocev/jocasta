//! `test/bench/stats.test.ts`, case for case.

use super::*;

fn close(actual: f64, expected: f64, digits: i32) {
	let tolerance = 10.0_f64.powi(-digits) / 2.0;
	assert!((actual - expected).abs() < tolerance, "{actual} is not close to {expected} ({digits} digits)");
}

fn pairs(differences: &[f64]) -> Vec<Pair> {
	differences.iter().map(|d| Pair { a: 0.0, b: *d }).collect()
}

#[test]
fn quantiles_interpolate_between_order_statistics() {
	assert_eq!(quantile(&[4.0, 1.0, 3.0, 2.0], 0.5), 2.5);
	let twenty: Vec<f64> = (1..=20).map(f64::from).collect();
	close(quantile(&twenty, 0.95), 19.05, 10);
	assert_eq!(median(&[7.0]), 7.0);
	assert!(median(&[]).is_nan());
}

#[test]
fn the_bootstrap_is_seeded_the_same_input_gives_the_same_ci_and_a_constant_gives_a_point() {
	let values = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0, 5.0, 3.0];
	assert_eq!(
		bootstrap_ci(&values, mean, BootstrapOptions::resamples(500)),
		bootstrap_ci(&values, mean, BootstrapOptions::resamples(500))
	);
	assert_eq!(
		bootstrap_ci(&[2.0, 2.0, 2.0], median, BootstrapOptions::resamples(200)),
		Interval { low: 2.0, high: 2.0 }
	);
	let ci = bootstrap_ci(&values, mean, BootstrapOptions::resamples(2000));
	assert!(ci.low < mean(&values));
	assert!(ci.high > mean(&values));
}

#[test]
fn summarize_reports_n_median_p95_and_the_medians_ci() {
	let summary = summarize(&[10.0, 20.0, 30.0, 40.0], BootstrapOptions::resamples(300));
	assert_eq!((summary.n, summary.median, summary.min, summary.max, summary.mean), (4, 25.0, 10.0, 40.0, 25.0));
	assert!(summary.median_ci95.low >= 10.0);
	assert!(summary.median_ci95.high <= 40.0);
}

#[test]
fn rate_counts_hits_with_a_ci_the_rule_of_three_bounds_zero_events() {
	let r = rate(&[true, false, true, true], BootstrapOptions::resamples(300));
	assert_eq!((r.hits, r.n, r.rate), (3, 4, 0.75));
	close(rule_of_three(5000), 0.0006, 10);
}

#[test]
fn mcnemars_exact_test_is_the_two_sided_binomial_over_discordant_pairs() {
	assert_eq!(mcnemar_exact(0, 0), Mcnemar { discordant: 0, p: 1.0 });
	close(mcnemar_exact(0, 5).p, 2.0 / 32.0, 12);
	close(mcnemar_exact(9, 1).p, 22.0 / 1024.0, 12);
	assert_eq!(mcnemar_exact(3, 3).p, 1.0);
}

#[test]
fn wilcoxon_signed_rank_exact_without_ties_normal_with_ties_zeros_dropped() {
	let all_up = pairs(&[1.0, 2.0, 3.0, 4.0, 5.0]);
	assert_eq!(
		wilcoxon_signed_rank(&all_up),
		Wilcoxon { n: 5, w_plus: 15.0, p: 2.0 / 32.0, method: WilcoxonMethod::Exact }
	);
	assert_eq!(
		wilcoxon_signed_rank(&[Pair { a: 1.0, b: 1.0 }]),
		Wilcoxon { n: 0, w_plus: 0.0, p: 1.0, method: WilcoxonMethod::Exact }
	);
	let mixed = wilcoxon_signed_rank(&pairs(&[1.0, -2.0, 3.0, -4.0, 5.0, 0.0]));
	assert_eq!((mixed.n, mixed.w_plus, mixed.method), (5, 9.0, WilcoxonMethod::Exact));
	assert!(mixed.p > 0.5);
	let tied = wilcoxon_signed_rank(&vec![Pair { a: 0.0, b: 1.0 }; 30]);
	assert_eq!(tied.method, WilcoxonMethod::Normal);
	assert!(tied.p < 0.001);
}

#[test]
fn holm_adjusts_in_step_down_order_and_keeps_the_input_order() {
	let adjusted = holm(&[0.01, 0.04, 0.03]);
	close(adjusted[0], 0.03, 12);
	close(adjusted[1], 0.06, 12);
	close(adjusted[2], 0.06, 12);
	assert_eq!(holm(&[0.5, 0.9]), [1.0, 1.0]);
}

#[test]
fn pass_k_averages_c_c_k_over_c_n_k_over_tasks_with_at_least_k_trials() {
	let tasks = [
		TaskTrials { passed: 3, trials: 3 },
		TaskTrials { passed: 2, trials: 3 },
		TaskTrials { passed: 3, trials: 4 },
		TaskTrials { passed: 1, trials: 1 },
	];
	close(pass_hat_k(&tasks, 3), (1.0 + 0.0 + 0.25) / 3.0, 12);
}

#[test]
fn the_paired_bootstraps_difference_is_the_mean_per_task_b_minus_a() {
	let result = paired_bootstrap(
		&[Pair { a: 0.0, b: 1.0 }, Pair { a: 1.0, b: 1.0 }, Pair { a: 0.5, b: 1.0 }],
		BootstrapOptions::resamples(500),
	);
	close(result.difference, 0.5, 12);
	assert!(result.ci95.low >= 0.0);
	assert!(result.ci95.high <= 1.0);
}

#[test]
fn the_normal_cdf() {
	close(normal_cdf(0.0), 0.5, 6);
	close(normal_cdf(1.959_964), 0.975, 5);
	close(normal_cdf(-1.959_964), 0.025, 5);
}
