//! The thread list's lines and a start's reply, as the benchmarks read them.

use super::*;

#[test]
fn a_started_threads_branch_is_read_from_the_reply() {
	assert_eq!(started_branch("Started T in acme on branch feature/t-1."), Some("feature/t-1".to_owned()));
	assert_eq!(started_branch("Started T in acme on branch feature/t-1, plan mode"), Some("feature/t-1".to_owned()));
	assert_eq!(started_branch("Started T in acme on branch feature/t-1"), Some("feature/t-1".to_owned()));
	let cli = "Started acme/T 1 on feature/t-1: it plans first; approve its plan in its pane.";
	assert_eq!(started_branch(cli), Some("feature/t-1".to_owned()));
	assert_eq!(started_branch("refused"), None);
}

#[test]
fn a_thread_list_line_gives_title_branch_state_overlaps_and_report() {
	let line = "acme/Fix it [feature/fix] working · overlaps Other: a.ts, b.ts; Third: c.ts · reported done";
	assert_eq!(
		parse_thread_line(line),
		Some(ThreadLine {
			title: "Fix it".into(),
			branch: "feature/fix".into(),
			state: "working".into(),
			overlaps: vec!["Other".into(), "Third".into()],
			reported: Some("done".into()),
		})
	);
	let plain = parse_thread_line("acme/Plain [feature/p] plan ready  ").unwrap();
	assert_eq!((plain.state.as_str(), plain.overlaps.len(), plain.reported), ("plan ready", 0, None));
	assert_eq!(parse_thread_line("  ledger line"), None);
	assert_eq!(parse_thread_line("No threads."), None);
}
