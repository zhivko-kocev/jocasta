//! Tests of the weekly brief (0680): its sections from the journal, the records and what each project kept.

use std::fs;

use super::*;
use crate::frontier::brief::model::Section;
use crate::frontier::server::check_costs::CheckCost;
use crate::shared::testing::temp_jocasta;

const HOUR: i64 = 3_600_000;

fn input() -> WeeklyInput {
	WeeklyInput {
		id: "weekly-2026-W47".to_owned(),
		monday: "2026-11-16".to_owned(),
		now: 1,
		projects: vec![ProjectWeek {
			name: "acme-api".to_owned(),
			production: "Threads finished this week: 2 · Lead time, median: 40 min".to_owned(),
			cost: Some(12.5),
			scorecard: Some(("clippy: 3".to_owned(), "/data/scorecard/2026-W47.txt".to_owned())),
			drift: Some("no drift found".to_owned()),
		}],
		numbers: WeeklyNumbers { finished: 2, gates: 4, gates_failed: 1, ..WeeklyNumbers::default() },
		check_costs: Vec::new(),
		trouble: Vec::new(),
		leftovers: Vec::new(),
	}
}

fn rows(brief: &Brief, at: usize) -> Vec<(String, String)> {
	match &brief.sections[at] {
		Section::Facts { rows, .. } => rows.iter().map(|row| (row.label.clone(), row.value.clone())).collect(),
		Section::List { rows, .. } => {
			rows.iter().map(|row| (row.text.clone(), row.note.clone().unwrap_or_default())).collect()
		}
		other => panic!("{other:?}"),
	}
}

#[test]
fn the_brief_has_the_planned_sections_each_with_a_row_even_when_empty() {
	let brief = build(&input());
	assert_eq!(
		(brief.id.as_str(), brief.kind.as_str(), brief.title.as_str()),
		("weekly-2026-W47", "weekly", "Weekly brief")
	);
	let headings: Vec<String> = brief
		.sections
		.iter()
		.map(|section| match section {
			Section::Facts { heading, .. } | Section::List { heading, .. } => heading.clone().unwrap_or_default(),
			other => panic!("{other:?}"),
		})
		.collect();
	assert_eq!(
		headings,
		[
			"Production and cost",
			"The week in the journal",
			"What each check cost",
			"Stuck and over budget",
			"Leftover worktrees",
			"Scorecard and drift review"
		]
	);
	assert_eq!(rows(&brief, 0)[1], ("acme-api cost".to_owned(), "$12.50".to_owned()));
	assert_eq!(rows(&brief, 1)[1], ("Gate fail rate".to_owned(), "1 of 4 (25%)".to_owned()));
	assert_eq!(rows(&brief, 2), [("Checks".to_owned(), "no cost recorded yet".to_owned())]);
	assert_eq!(rows(&brief, 3)[0].0, "No thread was stuck or over budget.");
	assert_eq!(rows(&brief, 4)[0].0, "No worktree is left on disk.");
	assert_eq!(rows(&brief, 5)[0].0, "Scorecard, acme-api: clippy: 3");
	assert_eq!(rows(&brief, 5)[0].1, "/data/scorecard/2026-W47.txt");
	assert_eq!(rows(&brief, 5)[1].0, "Drift review, acme-api: no drift found");
}

#[test]
fn trouble_rows_are_the_weeks_stuck_and_over_budget_lines_named_after_their_thread() {
	let at = 1_000 * HOUR;
	let line = |kind: &str, rest: serde_json::Value| {
		let mut value = serde_json::json!({ "at": at, "kind": kind, "project": "p1", "thread": "t-1" });
		value.as_object_mut().unwrap().extend(rest.as_object().unwrap().clone());
		value.to_string()
	};
	let lines = [
		line("stuck", serde_json::json!({ "gate": "quick" })),
		line("over_budget", serde_json::json!({ "spent": 5.1, "budget": 5 })),
		line("gate", serde_json::json!({})),
		"not json".to_owned(),
		line("stuck", serde_json::json!({ "gate": "old", "at": 1 })),
	];
	let names = HashMap::from([("p1/t-1".to_owned(), ("acme-api".to_owned(), "Retry".to_owned()))]);
	let found = trouble_rows(&lines, at - HOUR, at + HOUR, &names);
	let texts: Vec<&str> = found.iter().map(|row| row.text.as_str()).collect();
	assert_eq!(texts, ["acme-api/Retry: stuck: quick failed 3×", "acme-api/Retry: over budget: $5.10 of $5"]);
	// A thread with no record is named by its ids.
	let unknown = trouble_rows(&lines[..1], at - HOUR, at + HOUR, &HashMap::new());
	assert_eq!(unknown[0].text, "p1/t-1: stuck: quick failed 3×");
}

#[test]
fn check_costs_are_rows_with_the_folder_the_command_and_what_it_used() {
	let cost =
		|mb: u64, cores: f64, runs: u32| CheckCost { peak_rss_bytes: mb * 1_048_576, avg_cores: cores, runs, at: 0 };
	let rows = check_rows(vec![
		("/code/acme-api :: mise run verify".to_owned(), cost(412, 1.84, 7)),
		("bare command".to_owned(), cost(10, 0.5, 1)),
	]);
	assert_eq!(rows[0].label, "acme-api · mise run verify");
	assert_eq!(rows[0].value, "peak 412 MB · 1.8 cores · 7 runs");
	assert_eq!((rows[1].label.as_str(), rows[1].value.as_str()), ("bare command", "peak 10 MB · 0.5 cores · 1 run"));
}

#[test]
fn a_project_week_has_the_lines_the_cost_of_threads_open_in_the_window_and_what_it_kept() {
	let _j = temp_jocasta();
	let project = Project {
		id: "p-weekly-test".to_owned(),
		identity: String::new(),
		name: "acme-api".to_owned(),
		main_path: "/code/acme-api".into(),
		git: true,
		paths: Vec::new(),
	};
	let now = 5_000 * HOUR;
	let record = |created: i64, spend: Option<f64>, closed_at: Option<i64>| TaskRecord {
		created_at: created,
		spend,
		close: closed_at.map(|at| crate::frontier::server::task_file::TaskClose {
			at,
			how: crate::frontier::server::task_file::CloseHow::Merged,
			commit: None,
		}),
		..TaskRecord::default()
	};
	let records = [
		record(now - 3 * HOUR, Some(2.0), None),
		record(now - 40 * HOUR, Some(3.5), Some(now - 30 * HOUR)),
		// Closed before the window: not this week's.
		record(now - 400 * HOUR, Some(99.0), Some(now - 300 * HOUR)),
		record(now - HOUR, None, None),
	];
	for (dir, text) in [("scorecard", "clippy: 3\nunused: 0"), ("drift", "one parser, two callers\n")] {
		let dir = project_dir(&project.id).join(dir);
		fs::create_dir_all(&dir).unwrap();
		fs::write(dir.join("2026-W47.txt"), text).unwrap();
	}
	let week = project_week(&project, &records, "2026-W47", now);
	assert_eq!(week.cost, Some(5.5));
	assert!(week.production.starts_with("Threads finished this week: 1 (1 merged, 0 PRs)"), "{}", week.production);
	assert_eq!(week.scorecard.as_ref().map(|(first, _)| first.as_str()), Some("clippy: 3"));
	assert!(week.scorecard.unwrap().1.ends_with("2026-W47.txt"));
	assert_eq!(week.drift.as_deref(), Some("one parser, two callers"));
	// Another week's kept files are not this week's.
	let other = project_week(&project, &records, "2026-W40", now);
	assert_eq!((other.scorecard, other.drift), (None, None));
}
