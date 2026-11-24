use super::*;
use crate::frontier::server::task_file::{CloseFacts, TaskClose, TaskReview, close_task};

const HOUR: i64 = 3_600_000;
/// 2026-11-15T12:00:00Z.
const NOW: i64 = 1_794_744_000_000;

struct Fields {
	hours: f64,
	ago: f64,
	spend: Option<f64>,
	first_passed: Option<bool>,
}

fn fields(hours: f64) -> Fields {
	Fields { hours, ago: 1.0, spend: Some(1.0), first_passed: None }
}

/// A closed thread: started `hours` before it closed, `ago` hours before now.
fn thread(how: Option<CloseHow>, f: Fields) -> TaskRecord {
	let at = NOW - (f.ago * HOUR as f64) as i64;
	let review = |passed| TaskReview {
		at,
		passed,
		checks: Vec::new(),
		said: String::new(),
		notes: None,
		blocking: None,
		tests: None,
		proof: None,
	};
	TaskRecord {
		id: "t-0000".to_owned(),
		project_id: "p".to_owned(),
		project: "demo".to_owned(),
		title: "A thread".to_owned(),
		created_at: at - (f.hours * HOUR as f64) as i64,
		state: "finished".to_owned(),
		spend: f.spend,
		reviews: f.first_passed.map_or_else(Vec::new, |passed| vec![review(passed), review(true)]),
		closed: true,
		close: how.map(|how| TaskClose { at, how, commit: (how != CloseHow::Removed).then(|| "abc".to_owned()) }),
		..TaskRecord::default()
	}
}

fn lines(records: &[TaskRecord]) -> Vec<String> {
	production_lines(records, NOW).split('\n').map(str::to_owned).collect()
}

#[test]
fn a_weeks_finished_threads_merged_and_prs_counted_lead_time_and_spend_by_median_first_reviews_passed() {
	let records = [
		thread(Some(CloseHow::Merged), Fields { spend: Some(4.0), first_passed: Some(true), ..fields(3.0) }),
		thread(Some(CloseHow::Merged), Fields { spend: Some(2.0), first_passed: Some(false), ..fields(1.0) }),
		thread(Some(CloseHow::Pr), Fields { spend: Some(10.0), first_passed: Some(true), ..fields(50.0) }),
		// Not finished this week: removed, closed 8 days ago, closed before 0621 (no close), still open.
		thread(Some(CloseHow::Removed), fields(2.0)),
		thread(Some(CloseHow::Merged), Fields { ago: 8.0 * 24.0, ..fields(2.0) }),
		thread(None, fields(2.0)),
		TaskRecord { closed: false, ..thread(None, fields(2.0)) },
	];
	assert_eq!(
		lines(&records),
		[
			"Threads finished this week: 3 (2 merged, 1 PR)",
			"Lead time, median: 3 h (1 h to 2 d 2 h)",
			"Spend a finished thread, median: $4.00 (total $16.00)",
			"Passed the first review: 2 of 3",
		]
	);
}

#[test]
fn a_thread_with_no_spend_read_is_named_not_counted_as_free_a_week_with_none_finished_is_one_line() {
	let records = [
		thread(Some(CloseHow::Merged), Fields { spend: None, ..fields(0.5) }),
		thread(Some(CloseHow::Merged), Fields { spend: Some(3.0), ..fields(2.0) }),
	];
	assert_eq!(
		lines(&records)[1..],
		[
			"Lead time, median: 1 h 15 min (30 min to 2 h)",
			"Spend a finished thread, median: $3.00 (total $3.00; not known for 1)",
			"Passed the first review: 0 of 2",
		]
	);
	assert_eq!(
		production_lines(&[thread(Some(CloseHow::Removed), fields(1.0))], NOW),
		"Threads finished this week: none"
	);
}

#[test]
fn a_merged_thread_whose_folder_is_removed_later_stays_merged_and_counts_as_finished() {
	let mut record = TaskRecord { closed: false, state: "running".to_owned(), ..thread(None, fields(2.0)) };
	close_task(
		&mut record,
		CloseFacts { how: CloseHow::Merged, commit: Some("abc".to_owned()) },
		drop,
		Some(NOW - HOUR),
	);
	close_task(&mut record, CloseFacts { how: CloseHow::Removed, commit: None }, drop, Some(NOW));
	assert_eq!(record.close, Some(TaskClose { at: NOW - HOUR, how: CloseHow::Merged, commit: Some("abc".to_owned()) }));
	assert_eq!(record.state, "finished");
	assert_ne!(lines(&[record])[0], "Threads finished this week: none");
}

#[test]
fn a_spend_tie_rounds_up_as_javascripts_to_fixed_does() {
	assert_eq!(to_fixed(0.125), "0.13");
	assert_eq!(to_fixed(2.5), "2.50");
	assert_eq!(to_fixed(0.1), "0.10");
}

fn line(at: i64, kind: &str, thread: &str, rest: serde_json::Value) -> String {
	let mut value = serde_json::json!({ "at": at, "kind": kind, "project": "p1", "thread": thread });
	value.as_object_mut().unwrap().extend(rest.as_object().unwrap().clone());
	value.to_string()
}

#[test]
fn the_weeks_numbers_come_from_the_journal_lines_inside_the_window_only() {
	let from = NOW - 7 * 24 * HOUR;
	let at = from + HOUR;
	let gate = |exit: serde_json::Value| line(at, "gate", "t-1", serde_json::json!({ "depth": "full", "exit": exit }));
	let lines = vec![
		line(at, "finished", "t-1", serde_json::json!({ "mode": "merge", "cost_usd": 4.0, "wall_ms": 60_000 })),
		line(at, "finished", "t-2", serde_json::json!({ "mode": "pr", "cost_usd": 10.0, "wall_ms": 180_000 })),
		line(at, "finished", "t-3", serde_json::json!({ "mode": "pr", "cost_usd": null, "wall_ms": null })),
		gate(serde_json::json!(0)),
		gate(serde_json::json!(0)),
		gate(serde_json::json!(1)),
		gate(serde_json::json!(null)),
		line(at, "review_ready", "t-1", serde_json::json!({})),
		line(at, "review_ready", "t-2", serde_json::json!({})),
		line(at, "review_ready", "t-2", serde_json::json!({})),
		line(at, "sent_back", "t-2", serde_json::json!({})),
		line(at, "sent_back", "t-2", serde_json::json!({})),
		line(at, "stuck", "t-2", serde_json::json!({ "gate": "quick" })),
		line(at, "over_budget", "t-3", serde_json::json!({})),
		// Outside the window, and not journal lines at all.
		line(from - 1, "finished", "t-9", serde_json::json!({ "cost_usd": 99.0 })),
		line(NOW + HOUR, "stuck", "t-9", serde_json::json!({})),
		"not json".to_owned(),
		"{\"kind\":\"stuck\"}".to_owned(),
	];
	let numbers = weekly_numbers(&lines, from, NOW);
	assert_eq!(
		numbers,
		WeeklyNumbers {
			finished: 3,
			gates_failed: 2,
			gates: 4,
			sent_back: 1,
			reviewed: 2,
			stuck: 1,
			over_budget: 1,
			cost_median: Some(7.0),
			cost_total: Some(14.0),
			wall_median_ms: Some(120_000.0),
		}
	);
	let rows = numbers.rows();
	assert_eq!(rows[1], ("Gate fail rate".to_owned(), "2 of 4 (50%)".to_owned()));
	assert_eq!(rows[2], ("Sent back after review".to_owned(), "1 of 2 (50%)".to_owned()));
	assert_eq!(rows[5].1, "$7.00");
	assert_eq!(rows[7].1, "2 min");
	// A week with nothing in it says so instead of dividing by zero.
	let empty = weekly_numbers(Vec::<String>::new(), from, NOW);
	assert_eq!(empty.rows()[1].1, "none");
	assert_eq!(empty.rows()[5].1, "not known");
}
