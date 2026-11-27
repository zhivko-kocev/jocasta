use std::path::PathBuf;

use super::*;
use crate::frontier::server::context::test_context;
use crate::frontier::server::thread::ThreadPlan;
use crate::shared::paths::data_dir;
use crate::shared::projects::Project;

fn thread() -> Thread {
	let project = Project {
		id: "p-acme".to_owned(),
		identity: String::new(),
		name: "acme-api".to_owned(),
		main_path: PathBuf::from("/code/acme-api"),
		git: true,
		paths: Vec::new(),
	};
	Thread::new("1-1", "w1", project, "Readme line", "feature/readme")
}

#[test]
fn a_threads_phase_says_a_stop_first_then_its_step_then_what_today_shows() {
	let mut t = thread();
	assert_eq!(phase(&t), "idle");
	t.shown = ThreadState::Working;
	assert_eq!(phase(&t), "working");
	t.shown = ThreadState::Blocked;
	assert_eq!(phase(&t), "awaiting you");
	t.plan = Some(ThreadPlan { awaiting: Some("Plan".to_owned()), ..ThreadPlan::default() });
	assert_eq!(phase(&t), "plan ready");
	t.shown = ThreadState::Done;
	assert_eq!(phase(&t), "done");
	for (step, said) in [(Phase::Review, "review"), (Phase::Handoff, "handoff"), (Phase::Finish, "finishing")] {
		t.phase = step;
		assert_eq!(phase(&t), said);
	}
	t.stuck = Some("over budget: $5.10 of $5".to_owned());
	assert_eq!(phase(&t), "stuck");
}

#[test]
fn a_phase_line_is_due_once_per_change_never_for_a_thread_first_seen_or_the_same_phase_again() {
	let lines = PhaseLines::default();
	let look = |now: &[(&str, &'static str)]| {
		let now: Vec<(String, &'static str)> = now.iter().map(|(pane, phase)| ((*pane).to_owned(), *phase)).collect();
		lines.changed(&now)
	};
	assert!(look(&[("a", "working")]).is_empty(), "first seen: noted, not said");
	assert!(look(&[("a", "working")]).is_empty(), "the same phase again");
	assert_eq!(look(&[("a", "review"), ("b", "working")]), vec!["a".to_owned()]);
	assert!(look(&[("b", "working")]).is_empty(), "a is gone and forgotten");
	assert!(look(&[("a", "done"), ("b", "working")]).is_empty(), "a came back: first seen again");
}

#[tokio::test]
async fn board_json_has_every_open_thread_and_a_phase_change_is_one_logged_line_0718() {
	let t = test_context(|_| {});
	let thread = t.with_thread("t-1", "Readme line");
	thread.with(|t| t.shown = ThreadState::Working);
	let board: Board = serde_json::from_str(&board_json(&t.ctx).unwrap()).unwrap();
	assert_eq!(board.rows.len(), 1);
	let row = &board.rows[0];
	assert_eq!((row.project.as_str(), row.title.as_str(), row.phase.as_str()), ("demo", "Readme line", "working"));
	assert_eq!((board.checks.running, board.checks.waiting), (0, 0));
	// First seen, then the same phase again: nothing.
	log_phase_changes(&t.ctx).unwrap();
	log_phase_changes(&t.ctx).unwrap();
	thread.with(|t| t.shown = ThreadState::Blocked);
	log_phase_changes(&t.ctx).unwrap();
	log_phase_changes(&t.ctx).unwrap();
	let logged = std::fs::read_to_string(data_dir().join("notifications.jsonl")).unwrap();
	let lines: Vec<serde_json::Value> = logged.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
	assert_eq!(lines.len(), 1, "{logged}");
	let line = &lines[0];
	assert_eq!(
		(&line["reason"], &line["title"], &line["body"], &line["result"], &line["paneId"]),
		(
			&json!("phase"),
			&json!("demo/Readme line: awaiting you · $0.00"),
			&json!(""),
			&json!("logged"),
			&json!("t-1")
		)
	);
	assert!(line["at"].is_i64());
}
