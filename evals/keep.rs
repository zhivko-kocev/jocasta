//! The keep rule of 0543, written before any run: a tried feature stays when its paired cost per passing task is lower
//! and its pass rate is not significantly lower (McNemar's exact test on the paired trials, Holm's correction over the
//! study's comparisons, α = 0.05).
//!
//! ```text
//! jocasta-bench evals keep --out <follow-up runDir> [--alpha 0.05] [--dry-run]
//! ```
//!
//! reads `<runDir>/results.jsonl`, pairs each comparison's arm with A6 by task and rep, prints each verdict and writes
//! `<runDir>/keep.json` (a dry run prints the verdicts only).

use std::collections::HashMap;

use serde::{Serialize, Serializer};

use crate::evals::followup::FOLLOWUP_COMPARISONS;
use crate::evals::runner::to_fixed;
use crate::evals::tasks::BenchRow;
use crate::lib::args::{ArgSpec, BenchUsageError, flag_or, parse_bench_args};
use crate::lib::output::write_json;
use crate::lib::stats::{holm, mcnemar_exact};
use jocasta::frontier::protocol::js_number;
use jocasta::shared::errors::Result;
use jocasta::shared::jsonl::read_jsonl_file;

/// One trial as the rule reads it: its pair key (task and rep), whether it passed, and its cost.
#[derive(Debug, Clone, PartialEq)]
pub struct KeepTrial {
	pub pair: String,
	pub passed: bool,
	pub cost: f64,
}

/// One comparison: the trials with the feature and without it.
#[derive(Debug, Clone)]
pub struct KeepInput {
	pub name: String,
	pub with_feature: Vec<KeepTrial>,
	pub without_feature: Vec<KeepTrial>,
}

/// A number as `JSON.stringify` writes it: a whole value without a fraction, an infinite one as `null`.
fn json_number<S: Serializer>(value: &f64, serializer: S) -> std::result::Result<S::Ok, S::Error> {
	if value.is_finite() { js_number(value, serializer) } else { serializer.serialize_none() }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepVerdict {
	pub name: String,
	/// Pairs present on both sides; only they count.
	pub pairs: usize,
	pub passes_with: usize,
	pub passes_without: usize,
	/// Summed cost over the pairs ÷ passes; infinite with no pass (`null` in `keep.json`, as JSON has no infinity).
	#[serde(serialize_with = "json_number")]
	pub cost_per_pass_with: f64,
	#[serde(serialize_with = "json_number")]
	pub cost_per_pass_without: f64,
	/// Pairs where only the side with the feature passed, and only the side without.
	pub only_with: u64,
	pub only_without: u64,
	#[serde(serialize_with = "json_number")]
	pub p: f64,
	#[serde(serialize_with = "json_number")]
	pub p_holm: f64,
	pub cheaper: bool,
	pub significantly_worse: bool,
	pub keep: bool,
}

/// Cost per passing task over paired trials: summed cost ÷ passes (infinite with none).
pub fn cost_per_pass(trials: &[KeepTrial]) -> f64 {
	let passes = trials.iter().filter(|trial| trial.passed).count();
	if passes == 0 { f64::INFINITY } else { trials.iter().map(|trial| trial.cost).sum::<f64>() / passes as f64 }
}

/// Applies the keep rule to every comparison of one study; Holm's correction runs over all of them together.
pub fn keep_rule(comparisons: &[KeepInput], alpha: f64) -> Vec<KeepVerdict> {
	struct Paired<'a> {
		comparison: &'a KeepInput,
		pairs: Vec<(&'a KeepTrial, &'a KeepTrial)>,
		only_with: u64,
		only_without: u64,
		p: f64,
	}
	let paired: Vec<Paired> = comparisons
		.iter()
		.map(|comparison| {
			// A later trial of one pair replaces an earlier one, as a `Map` built from the list does.
			let without: HashMap<&str, &KeepTrial> =
				comparison.without_feature.iter().map(|trial| (trial.pair.as_str(), trial)).collect();
			let pairs: Vec<(&KeepTrial, &KeepTrial)> = comparison
				.with_feature
				.iter()
				.filter_map(|trial| without.get(trial.pair.as_str()).map(|other| (trial, *other)))
				.collect();
			let only_with = pairs.iter().filter(|(w, o)| w.passed && !o.passed).count() as u64;
			let only_without = pairs.iter().filter(|(w, o)| !w.passed && o.passed).count() as u64;
			Paired { comparison, pairs, only_with, only_without, p: mcnemar_exact(only_without, only_with).p }
		})
		.collect();
	let adjusted = holm(&paired.iter().map(|entry| entry.p).collect::<Vec<_>>());
	paired
		.iter()
		.enumerate()
		.map(|(i, entry)| {
			let with_trials: Vec<KeepTrial> = entry.pairs.iter().map(|(w, _)| (*w).clone()).collect();
			let without_trials: Vec<KeepTrial> = entry.pairs.iter().map(|(_, o)| (*o).clone()).collect();
			let passes_with = with_trials.iter().filter(|t| t.passed).count();
			let passes_without = without_trials.iter().filter(|t| t.passed).count();
			let cost_per_pass_with = cost_per_pass(&with_trials);
			let cost_per_pass_without = cost_per_pass(&without_trials);
			let p_holm = adjusted.get(i).copied().unwrap_or(1.0);
			let cheaper = cost_per_pass_with < cost_per_pass_without;
			let significantly_worse = passes_with < passes_without && p_holm < alpha;
			KeepVerdict {
				name: entry.comparison.name.clone(),
				pairs: entry.pairs.len(),
				passes_with,
				passes_without,
				cost_per_pass_with,
				cost_per_pass_without,
				only_with: entry.only_with,
				only_without: entry.only_without,
				p: entry.p,
				p_holm,
				cheaper,
				significantly_worse,
				keep: cheaper && !significantly_worse,
			}
		})
		.collect()
}

/// A results row as a keep trial: paired by task and rep, passed on the hidden tests, cost from the ledger's price table.
pub fn keep_trial(row: &BenchRow) -> Result<KeepTrial> {
	let Some(ledger) = &row.ledger else {
		return Err(BenchUsageError::err(format!(
			"{} rep {} has no ledger; it can't be priced",
			row.task, row.row.rep
		)));
	};
	Ok(KeepTrial {
		pair: format!("{}/{}", row.task, row.row.rep),
		passed: row.row.hidden_passed == Some(true),
		cost: ledger.totals.cost,
	})
}

/// The follow-up study's comparisons from its results rows: each arm, the feature, against its own baseline, without it.
pub fn followup_inputs(rows: &[BenchRow]) -> Result<Vec<KeepInput>> {
	let of =
		|arm: &str| -> Result<Vec<KeepTrial>> { rows.iter().filter(|row| row.arm == arm).map(keep_trial).collect() };
	FOLLOWUP_COMPARISONS
		.iter()
		.map(|comparison| {
			Ok(KeepInput {
				name: comparison.arm.to_owned(),
				with_feature: of(comparison.arm)?,
				without_feature: of(comparison.baseline)?,
			})
		})
		.collect()
}

/// `jocasta-bench evals keep`'s arguments, as `keep.ts` took them.
pub async fn run(argv: &[String]) -> Result<()> {
	let spec = ArgSpec { default_runs: 1, valued: &["--alpha"], ..ArgSpec::default() };
	let args = parse_bench_args(argv, &spec)?;
	let Some(run_dir) = args.out.clone() else {
		return Err(BenchUsageError::err("--out <follow-up runDir> is required"));
	};
	let text = flag_or(&args, "alpha", "0.05");
	let alpha = text.trim().parse::<f64>().ok().filter(|alpha| *alpha > 0.0 && *alpha < 1.0);
	let Some(alpha) = alpha else {
		return Err(BenchUsageError::err("--alpha needs a number between 0 and 1"));
	};
	let file = run_dir.join("results.jsonl");
	if !file.exists() {
		return Err(BenchUsageError::err(format!("no results at {}", file.display())));
	}
	let rows: Vec<BenchRow> = read_jsonl_file(&file)?;
	let verdicts = keep_rule(&followup_inputs(&rows)?, alpha);
	for v in &verdicts {
		println!(
			"{} {}: ${} vs ${} per pass, {}/{} vs {}/{} passed, McNemar p {} (Holm {})",
			if v.keep { "KEEP" } else { "drop" },
			v.name,
			to_fixed(v.cost_per_pass_with, 2),
			to_fixed(v.cost_per_pass_without, 2),
			v.passes_with,
			v.pairs,
			v.passes_without,
			v.pairs,
			to_fixed(v.p, 3),
			to_fixed(v.p_holm, 3),
		);
	}
	if !args.dry_run {
		let written =
			write_json(&run_dir, "keep", &serde_json::json!({ "alpha": alpha, "rule": "0543", "verdicts": verdicts }))?;
		println!("wrote {}", written.display());
	}
	Ok(())
}
