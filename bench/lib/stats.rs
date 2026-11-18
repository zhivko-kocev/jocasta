//! The statistics the pre-registered analysis names (0482): medians and p95 with percentile-bootstrap CIs for the
//! harness benchmarks, and for the task benchmark a paired task bootstrap, McNemar's exact test, the Wilcoxon
//! signed-rank test and Holm's correction. Every resampling takes a seeded generator so a report is reproducible.

use std::f64::consts::{LN_2, SQRT_2};

use serde::Serialize;

/// A seeded uniform [0, 1) generator (mulberry32).
pub fn seeded_random(seed: u32) -> impl FnMut() -> f64 {
	let mut state = seed;
	move || {
		state = state.wrapping_add(0x6d2b_79f5);
		let mut t = state;
		t = (t ^ (t >> 15)).wrapping_mul(t | 1);
		t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
		f64::from(t ^ (t >> 14)) / 4_294_967_296.0
	}
}

pub const DEFAULT_SEED: u32 = 482;
const DEFAULT_RESAMPLES: usize = 10_000;

pub fn mean(values: &[f64]) -> f64 {
	if values.is_empty() {
		return f64::NAN;
	}
	values.iter().sum::<f64>() / values.len() as f64
}

/// The q-quantile (0–1) with linear interpolation between order statistics (R's type 7); NaN for no values.
pub fn quantile(values: &[f64], q: f64) -> f64 {
	if values.is_empty() {
		return f64::NAN;
	}
	let mut sorted = values.to_vec();
	sorted.sort_by(f64::total_cmp);
	let position = (sorted.len() - 1) as f64 * q;
	let below = position.floor();
	let at = |index: f64| sorted.get(index as usize).copied().unwrap_or(f64::NAN);
	let (low, high) = (at(below), at(position.ceil()));
	low + (high - low) * (position - below)
}

pub fn median(values: &[f64]) -> f64 {
	quantile(values, 0.5)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Interval {
	pub low: f64,
	pub high: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BootstrapOptions {
	/// 10,000 when absent.
	pub resamples: Option<usize>,
	/// [`DEFAULT_SEED`] when absent.
	pub seed: Option<u32>,
	/// Two-sided coverage; 0.95 when absent.
	pub level: Option<f64>,
}

impl BootstrapOptions {
	/// The defaults with `resamples` resamples.
	#[cfg(test)]
	pub fn resamples(resamples: usize) -> Self {
		Self { resamples: Some(resamples), ..Self::default() }
	}
}

/// The percentile-bootstrap CI of `statistic` over `values` resampled with replacement.
pub fn bootstrap_ci(values: &[f64], statistic: impl Fn(&[f64]) -> f64, options: BootstrapOptions) -> Interval {
	if values.is_empty() {
		return Interval { low: f64::NAN, high: f64::NAN };
	}
	let mut random = seeded_random(options.seed.unwrap_or(DEFAULT_SEED));
	let resamples = options.resamples.unwrap_or(DEFAULT_RESAMPLES);
	let level = options.level.unwrap_or(0.95);
	let mut estimates = Vec::with_capacity(resamples);
	let mut sample = vec![0.0; values.len()];
	for _ in 0..resamples {
		for slot in &mut sample {
			*slot = values[(random() * values.len() as f64).floor() as usize];
		}
		estimates.push(statistic(&sample));
	}
	Interval { low: quantile(&estimates, (1.0 - level) / 2.0), high: quantile(&estimates, 1.0 - (1.0 - level) / 2.0) }
}

/// n, median, p95, mean and the median's bootstrap 95% CI: the summary every harness benchmark reports (0482).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
	pub n: usize,
	pub median: f64,
	pub p95: f64,
	pub mean: f64,
	pub min: f64,
	pub max: f64,
	pub median_ci95: Interval,
}

/// `Math.min` or `Math.max` over `values`: NaN when any is NaN or none is given.
fn extreme(values: &[f64], pick: fn(f64, f64) -> f64) -> f64 {
	let mut found: Option<f64> = None;
	for &value in values {
		if value.is_nan() {
			return f64::NAN;
		}
		found = Some(found.map_or(value, |so_far| pick(so_far, value)));
	}
	found.unwrap_or(f64::NAN)
}

pub fn summarize(values: &[f64], options: BootstrapOptions) -> Summary {
	Summary {
		n: values.len(),
		median: median(values),
		p95: quantile(values, 0.95),
		mean: mean(values),
		min: extreme(values, f64::min),
		max: extreme(values, f64::max),
		median_ci95: bootstrap_ci(values, median, options),
	}
}

/// A proportion with its bootstrap 95% CI over the 0/1 outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rate {
	pub hits: usize,
	pub n: usize,
	pub rate: f64,
	pub ci95: Interval,
}

pub fn rate(outcomes: &[bool], options: BootstrapOptions) -> Rate {
	let values: Vec<f64> = outcomes.iter().map(|&outcome| f64::from(u8::from(outcome))).collect();
	let hits = outcomes.iter().filter(|&&outcome| outcome).count();
	Rate {
		hits,
		n: values.len(),
		rate: if values.is_empty() { f64::NAN } else { hits as f64 / values.len() as f64 },
		ci95: bootstrap_ci(&values, mean, options),
	}
}

/// The one-sided 95% upper bound on a rate with 0 events in n trials: the rule of three, 3/n (H4).
pub fn rule_of_three(n: usize) -> f64 {
	if n == 0 { f64::NAN } else { 3.0 / n as f64 }
}

/// One task's two arms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
	pub a: f64,
	pub b: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PairedBootstrap {
	pub difference: f64,
	pub ci95: Interval,
}

/// The paired task bootstrap (0482): resamples tasks with replacement and reports the CI of the mean per-task
/// difference `b - a`. Pairs are per task, so both arms move together.
pub fn paired_bootstrap(pairs: &[Pair], options: BootstrapOptions) -> PairedBootstrap {
	let differences: Vec<f64> = pairs.iter().map(|pair| pair.b - pair.a).collect();
	PairedBootstrap { difference: mean(&differences), ci95: bootstrap_ci(&differences, mean, options) }
}

/// log of n choose k.
fn log_choose(n: u64, k: u64) -> f64 {
	(1..=k).map(|i| ((n - k + i) as f64).ln() - (i as f64).ln()).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Mcnemar {
	pub discordant: u64,
	pub p: f64,
}

/// McNemar's exact test on paired binary outcomes: `only_a` pairs where only A succeeded, `only_b` where only B did.
/// Two-sided binomial p-value with p = 0.5 over the discordant pairs; 1 when there are none.
pub fn mcnemar_exact(only_a: u64, only_b: u64) -> Mcnemar {
	let n = only_a + only_b;
	if n == 0 {
		return Mcnemar { discordant: 0, p: 1.0 };
	}
	let k = only_a.min(only_b);
	let tail: f64 = (0..=k).map(|i| (log_choose(n, i) - n as f64 * LN_2).exp()).sum();
	Mcnemar { discordant: n, p: (2.0 * tail).min(1.0) }
}

/// The standard normal CDF (Abramowitz–Stegun 7.1.26 on erf; error below 1.5e-7).
pub fn normal_cdf(z: f64) -> f64 {
	let x = z.abs() / SQRT_2;
	let t = 1.0 / (1.0 + 0.327_591_1 * x);
	let erf = 1.0
		- ((((1.061_405_429 * t - 1.453_152_027) * t + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592)
			* t
			* (-x * x).exp();
	if z >= 0.0 { (1.0 + erf) / 2.0 } else { (1.0 - erf) / 2.0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WilcoxonMethod {
	Exact,
	Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wilcoxon {
	/// Pairs with a non-zero difference.
	pub n: usize,
	/// The sum of the ranks of the positive differences (b > a).
	pub w_plus: f64,
	pub p: f64,
	pub method: WilcoxonMethod,
}

/// Average ranks of `differences` by absolute value, and the tie correction `Σ (size³ − size)`.
fn ranks_of(differences: &[f64]) -> (Vec<f64>, f64) {
	let n = differences.len();
	let mut order: Vec<(f64, usize)> = differences.iter().enumerate().map(|(i, d)| (d.abs(), i)).collect();
	order.sort_by(|x, y| x.0.total_cmp(&y.0));
	let mut ranks = vec![0.0; n];
	let mut tie_correction = 0.0;
	let mut start = 0;
	while start < n {
		let mut end = start;
		while end + 1 < n && order[end + 1].0 == order[start].0 {
			end += 1;
		}
		let rank = (start + end) as f64 / 2.0 + 1.0;
		for entry in &order[start..=end] {
			ranks[entry.1] = rank;
		}
		let size = (end - start + 1) as f64;
		tie_correction += size.powi(3) - size;
		start = end + 1;
	}
	(ranks, tie_correction)
}

/// The Wilcoxon signed-rank test on `b - a`, two-sided. Zero differences are dropped (Wilcoxon's method), ties get
/// average ranks. Exact for up to 25 non-zero pairs without ties; otherwise the normal approximation with the tie
/// correction and a continuity correction.
pub fn wilcoxon_signed_rank(pairs: &[Pair]) -> Wilcoxon {
	let differences: Vec<f64> = pairs.iter().map(|pair| pair.b - pair.a).filter(|d| *d != 0.0).collect();
	let n = differences.len();
	if n == 0 {
		return Wilcoxon { n: 0, w_plus: 0.0, p: 1.0, method: WilcoxonMethod::Exact };
	}
	let (ranks, tie_correction) = ranks_of(&differences);
	let w_plus: f64 = differences.iter().zip(&ranks).filter(|(d, _)| **d > 0.0).map(|(_, rank)| rank).sum();
	let total = (n * (n + 1) / 2) as f64;
	if n <= 25 && tie_correction == 0.0 {
		// The exact null distribution of W+ by counting subsets of ranks 1..n.
		let size = total as usize;
		let mut counts = vec![0.0_f64; size + 1];
		counts[0] = 1.0;
		for r in 1..=n {
			for s in (r..=size).rev() {
				counts[s] += counts[s - r];
			}
		}
		let low = w_plus.min(total - w_plus);
		let tail: f64 = counts.iter().take(low as usize + 1).sum();
		return Wilcoxon {
			n,
			w_plus,
			p: (2.0 * tail / 2.0_f64.powi(n as i32)).min(1.0),
			method: WilcoxonMethod::Exact,
		};
	}
	let expected = total / 2.0;
	let size = n as f64;
	let variance = (size * (size + 1.0) * (2.0 * size + 1.0)) / 24.0 - tie_correction / 48.0;
	let z = if variance == 0.0 { 0.0 } else { ((w_plus - expected).abs() - 0.5) / variance.sqrt() };
	Wilcoxon { n, w_plus, p: (2.0 * (1.0 - normal_cdf(z.max(0.0)))).min(1.0), method: WilcoxonMethod::Normal }
}

/// Holm's step-down adjusted p-values, in the input order.
pub fn holm(p_values: &[f64]) -> Vec<f64> {
	let m = p_values.len();
	let mut order: Vec<(f64, usize)> = p_values.iter().enumerate().map(|(i, p)| (*p, i)).collect();
	order.sort_by(|x, y| x.0.total_cmp(&y.0));
	let mut adjusted = vec![0.0; m];
	let mut running = 0.0_f64;
	for (rank, (p, i)) in order.into_iter().enumerate() {
		running = running.max(((m - rank) as f64 * p).min(1.0));
		adjusted[i] = running;
	}
	adjusted
}

/// One task's trials for [`pass_hat_k`].
#[derive(Debug, Clone, Copy)]
pub struct TaskTrials {
	pub passed: u64,
	pub trials: u64,
}

/// pass^k over tasks (0482): per task, the chance that k trials drawn without replacement from its n all pass,
/// C(c, k) / C(n, k), averaged over the tasks that have at least k trials.
pub fn pass_hat_k(tasks: &[TaskTrials], k: u64) -> f64 {
	let chances: Vec<f64> = tasks
		.iter()
		.filter(|task| task.trials >= k)
		.map(|task| if task.passed < k { 0.0 } else { (log_choose(task.passed, k) - log_choose(task.trials, k)).exp() })
		.collect();
	if chances.is_empty() { f64::NAN } else { mean(&chances) }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
