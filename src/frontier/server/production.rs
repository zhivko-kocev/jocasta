//! The weekly review's production numbers (0621): computed from a project's `thread.json` records with no model call,
//! before the drift agent runs. A thread counts as finished when it closed merged or as a PR in the 7 days before now; a
//! removed thread, and one closed before 0621 (no close time), are left out.

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

#[cfg(test)]
#[path = "production_tests.rs"]
mod tests;
