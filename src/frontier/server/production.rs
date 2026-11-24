//! The weekly review's production numbers (0621): computed from a project's `thread.json` records with no model call,
//! before the drift agent runs. A thread counts as finished when it closed merged or as a PR in the 7 days before now; a
//! removed thread, and one closed before 0621 (no close time), are left out.

use std::collections::HashSet;

use serde_json::Value;

use crate::frontier::server::task_file::{CloseHow, TaskRecord};

const WEEK_MS: i64 = 7 * 24 * 3_600_000;

/// The middle value of `values` (the mean of the two middle ones for an even count); `values` is not empty.
fn median(values: &[f64]) -> f64 {
	let mut sorted = values.to_vec();
	sorted.sort_by(f64::total_cmp);
	let middle = sorted.len() / 2;
	if sorted.len() % 2 == 1 { sorted[middle] } else { (sorted[middle - 1] + sorted[middle]) / 2.0 }
}

/// A duration as `40 min`, `3 h 10 min` or `2 d 4 h`, rounded down to the minute.
fn duration(ms: f64) -> String {
	let minutes = (ms / 60_000.0).floor() as i64;
	if minutes < 60 {
		return format!("{minutes} min");
	}
	let hours = minutes / 60;
	if hours < 24 {
		return if minutes % 60 == 0 { format!("{hours} h") } else { format!("{hours} h {} min", minutes % 60) };
	}
	let days = hours / 24;
	if hours % 24 == 0 { format!("{days} d") } else { format!("{days} d {} h", hours % 24) }
}

/// `value.toFixed(2)` for a value of 0 or more: JavaScript rounds an exact tie up, Rust's formatting to even.
fn to_fixed(value: f64) -> String {
	let eighths = value * 8.0;
	let tie = eighths.fract() == 0.0 && (eighths as i64) % 2 != 0;
	format!("{:.2}", if tie { (value * 100.0).ceil() / 100.0 } else { value })
}

fn usd(value: f64) -> String {
	format!("${}", to_fixed(value))
}

/// The week's lines as 0621 previews them: threads finished (merged and as PRs), the median lead time from start to
/// close with its range, the median spend a finished thread with the total, and how many passed their first review.
/// One line when none finished.
pub fn production_lines(records: &[TaskRecord], now: i64) -> String {
	let finished: Vec<&TaskRecord> = records
		.iter()
		.filter(|record| {
			record
				.close
				.as_ref()
				.is_some_and(|close| close.how != CloseHow::Removed && now - close.at <= WEEK_MS && close.at <= now)
		})
		.collect();
	if finished.is_empty() {
		return "Threads finished this week: none".to_owned();
	}
	let merged =
		finished.iter().filter(|record| record.close.as_ref().is_some_and(|c| c.how == CloseHow::Merged)).count();
	let prs = finished.len() - merged;
	let leads: Vec<f64> = finished
		.iter()
		.map(|record| (record.close.as_ref().map_or(now, |close| close.at) - record.created_at) as f64)
		.collect();
	let spends: Vec<f64> = finished.iter().filter_map(|record| record.spend).collect();
	let passed = finished.iter().filter(|record| record.reviews.first().is_some_and(|review| review.passed)).count();
	let unknown = finished.len() - spends.len();
	let lead_min = leads.iter().copied().fold(f64::INFINITY, f64::min);
	let lead_max = leads.iter().copied().fold(f64::NEG_INFINITY, f64::max);
	let spend = if spends.is_empty() {
		"Spend a finished thread: not known".to_owned()
	} else {
		let not_known = if unknown > 0 { format!("; not known for {unknown}") } else { String::new() };
		format!(
			"Spend a finished thread, median: {} (total {}{not_known})",
			usd(median(&spends)),
			usd(spends.iter().sum())
		)
	};
	[
		format!(
			"Threads finished this week: {} ({merged} merged, {prs} PR{})",
			finished.len(),
			if prs == 1 { "" } else { "s" }
		),
		format!("Lead time, median: {} ({} to {})", duration(median(&leads)), duration(lead_min), duration(lead_max)),
		spend,
		format!("Passed the first review: {passed} of {}", finished.len()),
	]
	.join("\n")
}

/// The week's numbers as the journal tells them (0688, 0680): what the weekly brief shows beside 0621's production lines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeklyNumbers {
	/// `finished` lines.
	pub finished: usize,
	/// `gate` lines with a non-zero exit (a gate that ran and has no exit code counts as failed) and all `gate` lines.
	pub gates_failed: usize,
	pub gates: usize,
	/// Threads with a `sent_back` line, and threads with a `review_ready` line.
	pub sent_back: usize,
	pub reviewed: usize,
	pub stuck: usize,
	pub over_budget: usize,
	/// Median and total of the `finished` lines' `cost_usd`; none when no line had one.
	pub cost_median: Option<f64>,
	pub cost_total: Option<f64>,
	/// Median of the `finished` lines' `wall_ms`.
	pub wall_median_ms: Option<f64>,
}

/// `numerator` of `denominator` as `3 of 12 (25%)`, `none` for no denominator.
fn rate(numerator: usize, denominator: usize) -> String {
	if denominator == 0 {
		return "none".to_owned();
	}
	let percent = (numerator as f64 / denominator as f64 * 100.0).round();
	format!("{numerator} of {denominator} ({percent}%)")
}

impl WeeklyNumbers {
	/// The numbers as labelled rows, in the order the brief shows them.
	pub fn rows(&self) -> Vec<(String, String)> {
		let cost = |value: Option<f64>| value.map_or_else(|| "not known".to_owned(), usd);
		vec![
			("Threads finished".to_owned(), self.finished.to_string()),
			("Gate fail rate".to_owned(), rate(self.gates_failed, self.gates)),
			("Sent back after review".to_owned(), rate(self.sent_back, self.reviewed)),
			("Stuck".to_owned(), self.stuck.to_string()),
			("Over budget".to_owned(), self.over_budget.to_string()),
			("Cost a finished thread, median".to_owned(), cost(self.cost_median)),
			("Cost of the finished threads, total".to_owned(), cost(self.cost_total)),
			(
				"Wall time a finished thread, median".to_owned(),
				self.wall_median_ms.map_or_else(|| "not known".to_owned(), duration),
			),
		]
	}
}

/// The numbers of the journal's `lines` whose `at` is in `from_ms..to_ms` (0688): threads finished, the gate-fail rate,
/// the sent-back rate (threads sent back over threads with a review ready), the `stuck` and `over_budget` counts and the
/// median and total cost and median wall time of what finished. A line that isn't JSON or has no `at` is skipped.
pub fn weekly_numbers<S: AsRef<str>>(lines: impl IntoIterator<Item = S>, from_ms: i64, to_ms: i64) -> WeeklyNumbers {
	let mut numbers = WeeklyNumbers::default();
	let (mut costs, mut walls): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
	let (mut sent_back, mut reviewed): (HashSet<String>, HashSet<String>) = (HashSet::new(), HashSet::new());
	for line in lines {
		let Ok(value) = serde_json::from_str::<Value>(line.as_ref()) else { continue };
		let in_week = value.get("at").and_then(Value::as_i64).is_some_and(|at| (from_ms..to_ms).contains(&at));
		if !in_week {
			continue;
		}
		let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default();
		let thread = format!("{}/{}", text("project"), text("thread"));
		match text("kind") {
			"finished" => {
				numbers.finished += 1;
				costs.extend(value.get("cost_usd").and_then(Value::as_f64));
				walls.extend(value.get("wall_ms").and_then(Value::as_f64));
			}
			"gate" => {
				numbers.gates += 1;
				numbers.gates_failed += usize::from(value.get("exit").and_then(Value::as_i64) != Some(0));
			}
			"sent_back" => {
				sent_back.insert(thread);
			}
			"review_ready" => {
				reviewed.insert(thread);
			}
			"stuck" => numbers.stuck += 1,
			"over_budget" => numbers.over_budget += 1,
			_ => {}
		}
	}
	numbers.sent_back = sent_back.len();
	numbers.reviewed = reviewed.len();
	if !costs.is_empty() {
		numbers.cost_median = Some(median(&costs));
		numbers.cost_total = Some(costs.iter().sum());
	}
	if !walls.is_empty() {
		numbers.wall_median_ms = Some(median(&walls));
	}
	numbers
}

#[cfg(test)]
#[path = "production_tests.rs"]
mod tests;
