use std::cell::Cell;
use std::fs;

use super::*;
use crate::frontier::server::report::{Outcome, ReportedTest};
use crate::shared::testing::{temp_dir, temp_jocasta};

const HOUR: i64 = 3_600_000;
/// `Date.parse("2026-11-15T12:00:00Z")`.
const NOW: i64 = 1_794_744_000_000;

/// A closed thread as `production.test.ts` makes one: started `hours` before it closed, `ago` hours before now.
fn thread(how: Option<CloseHow>, hours: i64, ago: i64) -> TaskRecord {
	let at = NOW - ago * HOUR;
	TaskRecord {
		path: String::new(),
		branch: String::new(),
		base: None,
		session: None,
		parent: None,
		created_at: at - hours * HOUR,
		state: "finished".to_owned(),
		spend: Some(1.0),
		plan: None,
		report: None,
		reviews: Vec::new(),
		closed: true,
		project_id: "p".to_owned(),
		project: "demo".to_owned(),
		title: "A thread".to_owned(),
		brief: String::new(),
		done_when: Vec::new(),
		heavy: None,
		id: "t-0000".to_owned(),
		close: how.map(|how| TaskClose {
			at,
			how,
			commit: if how == CloseHow::Removed { None } else { Some("abc".to_owned()) },
		}),
		extra: serde_json::Map::new(),
	}
}

fn fields(project_id: &str) -> NewTask {
	NewTask {
		project_id: project_id.to_owned(),
		project: "demo".to_owned(),
		title: "Migrate".to_owned(),
		brief: "Migrate it.".to_owned(),
		..NewTask::default()
	}
}

#[test]
fn a_merged_thread_whose_folder_is_removed_later_stays_merged_and_counts_as_finished_0621() {
	let _j = temp_jocasta();
	let mut record = thread(None, 2, 1);
	record.closed = false;
	record.state = "running".to_owned();
	let failed = Cell::new(false);
	let note = |_: Error| failed.set(true);
	close_task(
		&mut record,
		CloseFacts { how: CloseHow::Merged, commit: Some("abc".to_owned()) },
		note,
		Some(NOW - HOUR),
	);
	close_task(&mut record, CloseFacts { how: CloseHow::Removed, commit: None }, note, Some(NOW));
	assert_eq!(record.close, Some(TaskClose { at: NOW - HOUR, how: CloseHow::Merged, commit: Some("abc".to_owned()) }));
	assert_eq!(record.state, "finished");
	assert!(record.closed);
	assert!(!failed.get());
}

#[test]
fn a_removed_thread_says_its_worktree_was_removed_and_a_failed_write_goes_to_failed() {
	// The project's folder is a file, so nothing can be written under it.
	let _j = temp_jocasta();
	let mut record = thread(None, 1, 1);
	record.closed = false;
	record.project_id = "p-blocked".to_owned();
	fs::create_dir_all(project_dir("p-blocked").parent().unwrap()).unwrap();
	fs::write(project_dir("p-blocked"), "in the way").unwrap();
	let said = Cell::new(false);
	close_task(&mut record, CloseFacts { how: CloseHow::Removed, commit: None }, |_| said.set(true), Some(NOW));
	assert_eq!(record.state, "worktree removed");
	assert!(said.get());
}

#[test]
fn a_new_task_has_a_fresh_t_id_the_defaults_and_nothing_on_disk() {
	let _j = temp_jocasta();
	let record = new_task(fields("p-1"), Some(NOW));
	assert!(record.id.starts_with("t-") && record.id.len() == 6);
	assert!(record.id[2..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
	assert_eq!((record.path.as_str(), record.branch.as_str(), record.state.as_str()), ("", "", "starting"));
	assert_eq!((record.created_at, record.spend, record.closed), (NOW, None, false));
	assert!(!task_dir("p-1", &record.id).exists());
}

#[test]
fn thread_json_is_tab_indented_with_the_keys_in_new_task_order_and_whole_numbers_bare() {
	let _j = temp_jocasta();
	let mut record = new_task(NewTask { heavy: Some(true), ..fields("p-1") }, Some(NOW));
	record.id = "t-0a0b".to_owned();
	record.spend = Some(2.0);
	save_task(&record).unwrap();
	let text = fs::read_to_string(task_dir("p-1", "t-0a0b").join("thread.json")).unwrap();
	let expected = format!(
		"{{\n\t\"path\": \"\",\n\t\"branch\": \"\",\n\t\"base\": null,\n\t\"session\": null,\n\t\"parent\": null,\n\t\"createdAt\": {NOW},\n\t\"state\": \"starting\",\n\t\"spend\": 2,\n\t\"plan\": null,\n\t\"report\": null,\n\t\"reviews\": [],\n\t\"closed\": false,\n\t\"projectId\": \"p-1\",\n\t\"project\": \"demo\",\n\t\"title\": \"Migrate\",\n\t\"brief\": \"Migrate it.\",\n\t\"doneWhen\": [],\n\t\"heavy\": true,\n\t\"id\": \"t-0a0b\"\n}}\n"
	);
	assert_eq!(text, expected);
	assert_eq!(project_tasks("p-1"), [record]);
}

#[test]
fn the_task_file_renders_the_sections_in_0540s_format() {
	let _j = temp_jocasta();
	let mut record = new_task(
		NewTask {
			done_when: vec!["bun test".to_owned(), "bun run lint".to_owned()],
			base: Some("abc1234".to_owned()),
			session: Some("D:/s/2026_abc.jsonl".to_owned()),
			..fields("p-1")
		},
		Some(NOW),
	);
	record.id = "t-0001".to_owned();
	record.spend = Some(1.5);
	record.report = Some(TaskReport {
		at: NOW,
		value: ThreadReport {
			outcome: Outcome::Done,
			summary: "Did it".to_owned(),
			files: vec!["a.ts".to_owned()],
			tests: vec![ReportedTest { command: "bun test".to_owned(), passed: true }],
			decisions: Vec::new(),
			questions: Vec::new(),
			follow_ups: Vec::new(),
		},
	});
	record.reviews = vec![TaskReview {
		at: NOW,
		passed: false,
		checks: vec![
			CheckExit { command: "bun test".to_owned(), exit: Some(1) },
			CheckExit { command: "bun run lint".to_owned(), exit: None },
		],
		said: "2 notes (1 blocking), tests fail".to_owned(),
		notes: Some(2),
		blocking: Some(1),
		tests: Some(TestsResult::Fail),
		proof: None,
	}];
	save_task(&record).unwrap();
	let text = fs::read_to_string(task_file("p-1", "t-0001")).unwrap();
	let at = hhmm(NOW);
	let expected = format!(
		"# Migrate (demo)\n\n- Thread: t-0001 · session 2026_abc · parent: — · base abc1234\n- State: starting · spend $1.50\n\n## Brief\nMigrate it.\n\n## Done when\n- `bun test` passes\n- `bun run lint` passes\n\n## Plan\nNot approved yet.\n\n## Report\n{}\n\n## Reviews\n- {at} not a pass · `bun test` exit 1 · `bun run lint` not run · notes: 2 notes (1 blocking), tests fail\n",
		report_line(&record.report.as_ref().unwrap().value)
	);
	assert_eq!(text, expected);
	// The review's outcome survives a restart.
	assert_eq!(project_tasks("p-1"), [record]);
}

#[test]
fn an_empty_brief_and_no_commands_say_so() {
	let _j = temp_jocasta();
	let record = new_task(NewTask { brief: "  \n".to_owned(), ..fields("p-1") }, Some(NOW));
	save_task(&record).unwrap();
	let text = fs::read_to_string(task_file("p-1", &record.id)).unwrap();
	assert!(text.contains(
		"## Brief\nNot recorded.\n\n## Done when\n- No commands given; the review's project check decides.\n"
	));
	assert!(text.contains("## Report\nNone yet.\n\n## Reviews\nNone yet.\n"));
	assert!(text.contains("- State: starting · spend —\n"));
}

#[test]
fn an_approved_plan_is_read_from_the_sessions_artifacts_dir_or_named_when_unreadable() {
	let j = temp_jocasta();
	let sessions = j.root().join("sessions");
	let local = sessions.join("s1").join("local");
	fs::create_dir_all(&local).unwrap();
	fs::write(local.join("m-plan.md"), "\n# The plan\n\nDo it.\n\n").unwrap();
	let session = sessions.join("s1.jsonl").to_string_lossy().into_owned();
	assert_eq!(plan_text(Some("local://m-plan.md"), Some(&session)), "# The plan\n\nDo it.");
	assert_eq!(plan_text(Some("local://gone-plan.md"), Some(&session)), "In local://gone-plan.md.");
	assert_eq!(plan_text(Some("local://m-plan.md"), None), "In local://m-plan.md.");
	assert_eq!(plan_text(None, Some(&session)), "The plan's file isn't recorded.");
	let direct = j.root().join("plan.md");
	fs::write(&direct, "Direct.\n").unwrap();
	let direct = direct.to_string_lossy().into_owned();
	assert_eq!(plan_text(Some(&direct), None), "Direct.");
	let mut record = new_task(fields("p-1"), Some(NOW));
	record.plan = Some(TaskPlan { approved_at: NOW, title: "T".to_owned(), file: Some(direct) });
	assert!(task_markdown(&record).contains(&format!("## Plan (approved {})\nDirect.\n", hhmm(NOW))));
}

#[test]
fn spend_rounds_like_to_fixed() {
	assert_eq!(fixed2(0.0), "0.00");
	assert_eq!(fixed2(1.5), "1.50");
	assert_eq!(fixed2(0.125), "0.13");
	assert_eq!(fixed2(0.375), "0.38");
	assert_eq!(fixed2(2.0049), "2.00");
	assert_eq!(fixed2(12.3456), "12.35");
}

#[test]
fn project_tasks_leaves_out_an_unreadable_record_and_find_task_takes_the_newest_open_one() {
	let _j = temp_jocasta();
	let worktree = temp_dir("task-file-wt");
	let path = worktree.path().to_string_lossy().into_owned();
	let mut ids = Vec::new();
	for (created_at, closed) in [(10, false), (30, false), (40, true), (20, false)] {
		let mut record = new_task(NewTask { path: Some(path.clone()), ..fields("p-1") }, Some(created_at));
		record.closed = closed;
		save_task(&record).unwrap();
		ids.push((record.id, created_at));
	}
	fs::create_dir_all(task_dir("p-1", "t-bad")).unwrap();
	fs::write(task_dir("p-1", "t-bad").join("thread.json"), "{ not json").unwrap();
	fs::write(threads_dir("p-1").join("stray.txt"), "x").unwrap();
	assert_eq!(project_tasks("p-1").len(), 4);
	let found = find_task("p-1", worktree.path()).unwrap();
	assert_eq!(found.created_at, 30);
	assert!(find_task("p-1", &worktree.path().join("elsewhere")).is_none());
	assert!(find_task("p-none", worktree.path()).is_none());
}

#[test]
fn the_launch_prompt_is_the_shipped_prompt_without_its_comment_then_the_task() {
	let _j = temp_jocasta();
	let dir = temp_dir("task-file-prompt");
	let shipped = dir.path().join("pane.md");
	fs::write(&shipped, "<!-- licence -->\n\n# Ponytail\nBe brief.\n").unwrap();
	let mut record = new_task(NewTask { done_when: vec!["bun test".to_owned()], ..fields("p-1") }, Some(NOW));
	record.id = "t-0002".to_owned();
	let fields = PromptFields::from(&record);
	let task = "# Your task: Migrate (demo)\n\n## Brief\nMigrate it.\n\n## Done when\n- `bun test` passes\n";
	let finish = WHEN_YOU_FINISH.join("\n");
	assert_eq!(task_prompt(&fields, Some(&shipped), true), format!("# Ponytail\nBe brief.\n\n{task}\n{finish}\n"));
	assert_eq!(
		task_prompt(&fields, Some(&shipped), false),
		"# Ponytail\nBe brief.\n\n# Your task: Migrate (demo)\n\n## Brief\nMigrate it.\n"
	);
	// A missing shipped prompt leaves the task alone.
	assert_eq!(
		task_prompt(&fields, Some(&dir.path().join("none.md")), false),
		"# Your task: Migrate (demo)\n\n## Brief\nMigrate it.\n"
	);
	assert_eq!(task_prompt(&fields, None, true), format!("{task}\n{finish}\n"));
	let file = launch_prompt(&record, Some(&shipped), true).unwrap();
	assert_eq!(file, task_dir("p-1", "t-0002").join("prompt.md"));
	assert_eq!(fs::read_to_string(file).unwrap(), format!("# Ponytail\nBe brief.\n\n{task}\n{finish}\n"));
}

#[test]
fn an_unknown_key_survives_a_save_and_an_older_record_without_closed_is_listed() {
	let _j = temp_jocasta();
	let worktree = temp_dir("task-file-old");
	let dir = task_dir("p-1", "t-0003");
	fs::create_dir_all(&dir).unwrap();
	let old = serde_json::json!({
		"id": "t-0003", "projectId": "p-1", "project": "demo", "title": "Old", "brief": "b",
		"path": worktree.path().to_string_lossy(), "createdAt": 5, "state": "running", "futureKey": {"a": [1, 2]},
	});
	fs::write(dir.join("thread.json"), old.to_string()).unwrap();
	let listed = project_tasks("p-1");
	assert_eq!(listed.len(), 1);
	assert!(!listed[0].closed && listed[0].reviews.is_empty() && listed[0].plan.is_none() && listed[0].heavy.is_none());
	assert_eq!(find_task("p-1", worktree.path()).unwrap().id, "t-0003");
	save_task(&listed[0]).unwrap();
	let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("thread.json")).unwrap()).unwrap();
	assert_eq!(saved["futureKey"], serde_json::json!({"a": [1, 2]}));
	// A file that isn't a JSON object is still left out.
	let bad = task_dir("p-1", "t-0004");
	fs::create_dir_all(&bad).unwrap();
	fs::write(bad.join("thread.json"), "[1]").unwrap();
	assert_eq!(project_tasks("p-1").len(), 1);
}
