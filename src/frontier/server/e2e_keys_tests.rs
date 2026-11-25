//! The cases of `test/frontier/server/service-keys.test.ts`, case for case on the real server: his keys as the dock
//! and his keybinds send them, the omp hosts' pipes, the relay URL, the overlay's writes, the task file, the approved plan,
//! `report_result`, the review's gates, the check tool, `jocasta thread|worktree` and the thread keys in today's pane.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::FutureExt;
use regex::Regex;
use serde_json::{Value, json};

use crate::frontier::protocol::{PluginAction, ServerMessage};
use crate::frontier::server::allow::{Answer, Confirmation};
use crate::frontier::server::bridge::{PaneAgent, PaneEvent};
use crate::frontier::server::helpers::review_asked;
use crate::frontier::server::helpers::{
	BRIEF, Msg, Page, TestServer, boss_pane, bridge_flags, call, call_as, cli, commit_in, demo_project, guard,
	hand_in_two_notes, pane_reports, proposed_plan_file, push_state, session_file, sh, states_settled, task_dir_of,
	thread_omp, until, with_server,
};
use crate::frontier::server::history::search_history;
use crate::frontier::server::native::NativeServer;
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::popups::{AnswerTaken, Popup, PopupEnd};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::surface::AgentState;
use crate::frontier::server::tools::ToolReply;
use crate::frontier::server::wording::{HANDOFF_PROMPT, REPORT_STEER, WHEN_YOU_FINISH};
use crate::shared::pathkey::same_dir;
use crate::shared::paths::data_dir;
use crate::shared::schema::open_catalog;
use crate::shared::testing::temp_jocasta;

fn read(path: &Path) -> String {
	fs::read_to_string(path).unwrap_or_default()
}

fn words(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

/// A tool's answer as the TypeScript compares it: its text, and `isError` only when set.
fn said(text: &str, is_error: bool) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: is_error.then_some(true) }
}

/// `call` from `pane` (`None`: no pane), sending no boss key: the pane's own key names it (0578).
async fn from_pane(server: &TestServer, tool: &str, args: &Value, pane: Option<&str>) -> ToolReply {
	call_as(server, tool, args, pane, None).await
}

fn prompts(server: &TestServer) -> Vec<(String, String)> {
	let pair = |args: Vec<Value>| {
		let text = |at: usize| args.get(at).and_then(Value::as_str).unwrap_or_default().to_owned();
		(text(0), text(1))
	};
	server.herdr.of("prompt").into_iter().map(pair).collect()
}

fn typed_to(server: &TestServer, pane: &str) -> Vec<String> {
	prompts(server).into_iter().filter(|(to, _)| to == pane).map(|(_, text)| text).collect()
}

fn state_of(pane: &str, server: &TestServer, state: AgentState, session: Option<&Path>) {
	server.herdr.set_state(pane, state, session.and_then(Path::to_str), true);
}

fn base_name(path: &Path) -> String {
	path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn his_keys_stop_refuses_an_idle_thread_with_why_and_stops_a_working_one_finish_and_review_refuse_while_it_works_quits_stop_everything_and_finishs_merge_take_the_popups_code_cancel_doesnt_0544_0554_0568()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	let shown: Arc<Mutex<Vec<Popup>>> = Arc::default();
	let told = Arc::clone(&shown);
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		on_question: Some(Arc::new(move |popup| {
			told.lock().unwrap_or_else(PoisonError::into_inner).push(popup.clone());
		})),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let started = crate::frontier::server::helpers::start_thread(&server, Default::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let page = Page::open(&server, "").await;
		let key = |action: &str, pane: Option<&str>| {
			let action: PluginAction = serde_json::from_value(json!(action)).expect("one of his keys");
			let (server, pane) = (server.server.clone(), pane.map(str::to_owned));
			async move {
				tokio::spawn(async move { server.key(action, pane).await });
			}
		};
		let answer = |popup: &Popup, body: Value| server.answer(popup.code(), &body).0;
		let said = |text: &str, pattern: bool| {
			let (text, page) = (text.to_owned(), &page);
			async move {
				let expect = if pattern { Some(Regex::new(&text).unwrap()) } else { None };
				let message = page
					.wait_for(|m| match (m, &expect) {
						(ServerMessage::Action { text: said, .. }, Some(pattern)) => pattern.is_match(said),
						(ServerMessage::Action { text: said, .. }, None) => *said == text,
						_ => false,
					})
					.await;
				let ServerMessage::Action { text, is_error } = message else { unreachable!() };
				(text, is_error)
			}
		};
		let popup = |title: &str| {
			let title = title.to_owned();
			let shown = Arc::clone(&shown);
			async move {
				let find = |shown: &Mutex<Vec<Popup>>| {
					shown
						.lock()
						.unwrap_or_else(PoisonError::into_inner)
						.iter()
						.find(|p| p.view().title == title && p.open())
						.cloned()
				};
				until(|| find(&shown).is_some()).await;
				find(&shown).expect("the popup")
			}
		};

		key("stop", Some(&pane_id)).await;
		assert!(said("Stop: demo/Readme line isn't working; nothing to stop.", false).await.1);
		// The toast reaches herdr after the answer goes out (CI's Ubuntu run read it between the two).
		until(|| {
			server.herdr.of("notify").contains(&vec![
				json!({ "title": "Jocasta", "body": "Stop: demo/Readme line isn't working; nothing to stop.", "sound": "none" }),
			])
		})
		.await;
		// Logged as the toasts' notifications are (0552).
		until(|| {
			read(&data_dir().join("notifications.jsonl"))
				.contains("\"body\":\"Stop: demo/Readme line isn't working; nothing to stop.\"")
		})
		.await;
		push_state(&server, &pane_id, AgentState::Working, None).await;
		key("stop", Some(&pane_id)).await;
		assert!(!said("Stopped demo/Readme line's run.", false).await.1);
		assert_eq!(server.herdr.of("stop"), vec![vec![json!(pane_id)]]);
		key("finish", Some(&pane_id)).await;
		said("demo/Readme line isn't idle (working); finish it once it is.", false).await;
		key("review", Some(&pane_id)).await;
		said("Review: demo/Readme line isn't idle (working); review it once it is.", false).await;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		key("stop", Some(&boss)).await;
		said("Stop: the focused pane isn't a thread's; focus a thread or pick one in today.", false).await;

		// Quit with a thread working: the close question; Stop everything without its code is refused.
		key("quit", None).await;
		let close = popup("Jocasta: Stop everything?").await;
		let code = close.view().code.clone().expect("a code");
		assert!(code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase() && !b"IO".contains(&b)), "{code}");
		assert_eq!(answer(&close, json!({ "choice": "Stop everything" })), AnswerTaken::Wrong);
		assert_eq!(answer(&close, json!({ "choice": "Cancel" })), AnswerTaken::Taken);
		said("Jocasta keeps running.", false).await;

		// Finish once idle with a commit: Merge without its code is refused, with it the thread merges.
		push_state(&server, &pane_id, AgentState::Idle, None).await;
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		key("finish", Some(&pane_id)).await;
		let finish = popup("Jocasta: Finish demo/Readme line").await;
		assert_eq!(finish.view().choices, words(&["Merge", "PR", "Cancel"]));
		assert_eq!(answer(&finish, json!({ "choice": "Merge", "typed": "" })), AnswerTaken::Wrong);
		let typed = finish.view().code.clone().map(|code| code.to_lowercase());
		assert_eq!(answer(&finish, json!({ "choice": "Merge", "typed": typed })), AnswerTaken::Taken);
		let (_, is_error) = said("^Merged demo/Readme line into main", true).await;
		assert!(!is_error);
		assert!(!path.exists());

		page.close();
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_threads_working_asking_and_idle_come_from_its_omps_bridge_herdrs_state_counts_only_while_the_bridge_isnt_connected_0547_0561()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let started = crate::frontier::server::helpers::start_thread(&server, Default::default()).await;
		let pane_id = started.pane_id;
		let reports = |event: PaneEvent, agent: PaneAgent| pane_reports(&server, &pane_id, &event, agent);
		let working = PaneAgent { working: true, ..PaneAgent::default() };
		let busy = || server.service.busy_threads().unwrap();
		// herdr says idle; omp says a turn runs.
		reports(PaneEvent::AgentStart, working.clone());
		until(|| busy().working == 1).await;
		// herdr's idle pushed meanwhile doesn't end it while the bridge answers.
		state_of(&pane_id, &server, AgentState::Idle, None);
		server.service.refresh().await;
		assert_eq!((busy().working, busy().awaiting), (1, 0));
		// omp's `ask` dialog: awaiting him.
		let asking = PaneAgent { input_open: true, input_kind: Some("ask".to_owned()), ..working };
		reports(PaneEvent::Input { open: true, kind: Some("ask".to_owned()) }, asking);
		until(|| busy().awaiting == 1).await;
		// The turn ends.
		let idle = PaneAgent { idle_reported: true, ..PaneAgent::default() };
		reports(PaneEvent::AgentEnd { idle: true, jobs: 0 }, idle);
		until(|| busy().awaiting == 0 && busy().working == 0).await;
		// The bridge is gone: unknown, and herdr's events decide again.
		reports(PaneEvent::Gone, PaneAgent::default());
		until(|| {
			state_of(&pane_id, &server, AgentState::Working, None);
			busy().working == 1
		})
		.await;
	})
	.await;
}

/// Holds `file` open for reading as omp does while it starts, without letting it be replaced (Windows' FileShare.Read).
/// Dropping it closes the handle.
#[cfg(windows)]
fn hold_open(file: &Path) -> fs::File {
	use std::os::windows::fs::OpenOptionsExt;
	// FILE_SHARE_READ.
	fs::OpenOptions::new().read(true).share_mode(1).open(file).expect("holding the overlay open")
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn parallel_thread_starts_in_one_project_dont_race_on_its_overlay_one_write_at_a_time_none_when_unchanged_a_refused_rename_tried_again_0534()
 {
	use crate::frontier::server::overlay::overlay_text;
	use crate::shared::paths::project_dir;
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	// Five starts at once: the running limit (0687) isn't what this test is about.
	fs::write(crate::shared::paths::config_dir().join("jocasta.yaml"), "threads:\n  maxRunning: 0\n").unwrap();
	let options = ServerOptions { agent_wait_ms: Some(0), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let overlay = overlay_file(&project.id);
		let start = |title: &'static str| {
			let server = server.clone();
			tokio::spawn(async move {
				call(
					&server,
					"thread_start",
					&json!({ "project": "demo", "title": title, "brief": "Add a README line." }),
				)
				.await
			})
		};
		// An overlay from an older build, open in an omp starting: the rename over it is refused until it lets go.
		fs::create_dir_all(project_dir(&project.id)).unwrap();
		fs::write(&overlay, "old: true\n").unwrap();
		let first = hold_open(&overlay);
		let replies: Vec<_> = ["One", "Two", "Three", "Four"].into_iter().map(start).collect();
		// Released once the product's write is waiting on the refused rename, so the retry is what lets it through.
		until(|| {
			fs::read_dir(project_dir(&project.id))
				.unwrap()
				.flatten()
				.any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
		})
		.await;
		drop(first);
		for reply in replies {
			let reply = reply.await.unwrap();
			assert!(reply.text.starts_with("Started "), "{}", reply.text);
		}
		assert_eq!(read(&overlay), overlay_text(&project.id));
		// The overlay as it should be, held open by every omp reading it: never replaced, every start goes on.
		let second = hold_open(&overlay);
		let more: Vec<_> = ["Five", "Six", "Seven", "Eight"].into_iter().map(start).collect();
		for reply in more {
			let reply = reply.await.unwrap();
			assert!(reply.text.starts_with("Started "), "{}", reply.text);
		}
		drop(second);
		assert_eq!(server.herdr.of("createWorktree").len(), 8);
		let left: Vec<String> = fs::read_dir(project_dir(&project.id))
			.unwrap()
			.flatten()
			.map(|entry| entry.file_name().to_string_lossy().into_owned())
			.filter(|name| name.ends_with(".tmp"))
			.collect();
		assert!(left.is_empty(), "{left:?}");
	})
	.await;
}

/// Starts `Readme line` with `done_when` through the boss's tool; its omp comes up idle (0540).
async fn start_with_checks(server: &TestServer, done_when: &[&str]) -> (String, PathBuf) {
	let asked = server.clone();
	let args = json!({ "project": "demo", "title": "Readme line", "brief": BRIEF, "done_when": done_when });
	let replied = tokio::spawn(async move { call(&asked, "thread_start", &args).await });
	until(|| server.herdr.of("createWorktree").len() == 1).await;
	let pane = server.herdr.state().pane_list.last().cloned().expect("a thread pane");
	let cwd = pane.cwd.clone().expect("no thread pane");
	server.herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
	replied.await.expect("the call ran");
	(pane.pane_id, cwd)
}

fn write_readme(path: &Path, text: &str) {
	fs::write(path.join("README.md"), text).unwrap();
}

fn append_line(file: &Path, line: &Value) {
	use std::io::Write;
	let mut out = fs::OpenOptions::new().append(true).open(file).unwrap();
	writeln!(out, "{line}").unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_starts_brief_and_done_when_go_into_the_task_file_and_every_launch_of_the_threads_omp_the_approved_plan_session_state_and_spend_follow_0540()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let (pane_id, path) = start_with_checks(&server, &["bun test test/readme.test.ts"]).await;
		let dir = task_dir_of(&project.id, "Readme line");
		assert!(Regex::new("^t-[0-9a-f]{4}$").unwrap().is_match(&base_name(&dir)));
		let prompt = dir.join("prompt.md");
		let mut expected = words(&["omp", "--append-system-prompt"]);
		expected.push(prompt.display().to_string());
		expected.extend(guard());
		expected.extend(bridge_flags());
		expected.extend(words(&["--config"]));
		expected.push(overlay_file(&project.id).display().to_string());
		expected.extend(words(&["--max-cost", "75", "--warn-cost", "50"]));
		assert_eq!(server.herdr.of("createWorktree")[0][1]["command"], json!(expected));
		// Then how to finish (0577, PRM-7).
		assert_eq!(
			read(&prompt),
			format!(
				"# Your task: Readme line (demo)\n\n## Brief\nAdd a README line.\n\n## Done when\n- `bun test test/readme.test.ts` passes\n\n{}\n",
				WHEN_YOU_FINISH.join("\n")
			)
		);
		let task = || read(&dir.join("task.md"));
		let base = sh(&project.main_path, &["rev-parse", "--short", "main"]);
		until(|| task().contains("- State: idle")).await;
		let thread = base_name(&dir);
		assert_eq!(
			task(),
			[
				"# Readme line (demo)".to_owned(),
				String::new(),
				format!("- Thread: {thread} · session — · parent: — · base {base}"),
				"- State: idle · spend —".to_owned(),
				String::new(),
				"## Brief".to_owned(),
				BRIEF.to_owned(),
				String::new(),
				"## Done when".to_owned(),
				"- `bun test test/readme.test.ts` passes".to_owned(),
				String::new(),
				"## Plan".to_owned(),
				"Not approved yet.".to_owned(),
				String::new(),
				"## Report".to_owned(),
				"None yet.".to_owned(),
				String::new(),
				"## Reviews".to_owned(),
				"None yet.".to_owned(),
				String::new(),
			]
			.join("\n")
		);
		let status = call(&server, "thread_status", &json!({ "thread": "Readme line" })).await;
		assert!(
			status.text.ends_with(&format!("\ntask file: {}\nreport: none yet", dir.join("task.md").display())),
			"{}",
			status.text
		);

		// The plan, proposed in omp's local:// folder and approved: the task file carries it.
		let file = proposed_plan_file(&path);
		let local = file.with_extension("").join("local");
		fs::create_dir_all(&local).unwrap();
		fs::write(local.join("readme-line-plan.md"), "1. Add the line.\n").unwrap();
		state_of(&pane_id, &server, AgentState::Working, Some(&file));
		state_of(&pane_id, &server, AgentState::Idle, Some(&file));
		until(|| task().contains("- State: plan ready")).await;
		append_line(&file, &json!({ "type": "mode_change", "id": "m3", "mode": "none" }));
		state_of(&pane_id, &server, AgentState::Working, Some(&file));
		until(|| task().contains("## Plan (approved ")).await;
		assert!(Regex::new(r"## Plan \(approved \d\d:\d\d\)\n1\. Add the line\.\n").unwrap().is_match(&task()));
		let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
		assert!(task().contains(&format!("- Thread: {thread} · session {stem} · parent: — · base {base}")));
		assert!(Regex::new(r"- State: working · spend \$\d+\.\d\d\n").unwrap().is_match(&task()));

		// herdr restores the pane as a shell: omp starts again with the same appended brief and done-when.
		server.service.refresh().await;
		server.herdr.restore();
		let before = server.herdr.state().calls.len();
		server.service.refresh().await;
		let respawned: Vec<Value> = server.herdr.state().calls[before..]
			.iter()
			.filter(|call| call.method == "respawn")
			.map(|call| call.args[1].clone())
			.collect();
		let found = respawned
			.iter()
			.find(|options| same_dir(Path::new(options["cwd"].as_str().unwrap_or_default()), &path))
			.expect("the thread's pane respawned");
		assert_eq!(found["command"], json!(thread_omp(&project.id, "Readme line")));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_and_execute_the_task_file_records_the_plan_once_omp_left_plan_mode_also_when_herdr_says_working_first_and_the_work_goes_on_in_a_new_session_spend_and_the_ledger_cover_both_sessions_0538_0540_0546()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let (pane_id, path) = start_with_checks(&server, &[]).await;
		let task = || read(&task_dir_of(&project.id, "Readme line").join("task.md"));
		let planning = proposed_plan_file(&path);
		state_of(&pane_id, &server, AgentState::Working, Some(&planning));
		state_of(&pane_id, &server, AgentState::Idle, Some(&planning));
		until(|| task().contains("- State: plan ready")).await;
		// He approves: herdr says working before omp wrote that it left plan mode.
		state_of(&pane_id, &server, AgentState::Working, Some(&planning));
		until(|| !task().contains("- State: plan ready")).await;
		assert!(task().contains("## Plan\nNot approved yet.\n"));
		// omp leaves plan mode, then goes on in a new session (`/clear`, no parent) with the plan copied to its folder.
		append_line(&planning, &json!({ "type": "mode_change", "id": "m3", "mode": "none" }));
		let executing = session_file(&path, &[Msg::new("Added the line.", 0.5)]);
		let local = executing.with_extension("").join("local");
		fs::create_dir_all(&local).unwrap();
		fs::write(local.join("readme-line-plan.md"), "1. Add the line.\n").unwrap();
		state_of(&pane_id, &server, AgentState::Idle, Some(&executing));
		until(|| task().contains("## Plan (approved ")).await;
		assert!(Regex::new(r"## Plan \(approved \d\d:\d\d\)\n1\. Add the line\.\n").unwrap().is_match(&task()));
		assert!(task().contains(&format!("session {}", executing.file_stem().unwrap().to_string_lossy())));
		// Both sessions' spend: the plan's $0.25 and the work's $0.50.
		until(|| task().contains("spend $0.75")).await;
		let status = call(&server, "thread_status", &json!({ "thread": "Readme line" })).await;
		assert!(status.text.contains("\nspend $0.75"), "{}", status.text);
		// The task's commit ends a turn of the new session: its reviewer gets the plan approved in the old one (0453).
		state_of(&pane_id, &server, AgentState::Working, Some(&executing));
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		state_of(&pane_id, &server, AgentState::Idle, Some(&executing));
		let review = || {
			prompts(&server)
				.into_iter()
				.filter(|(pane, _)| *pane == pane_id)
				.map(|(_, text)| text)
				.find(|text| text.starts_with("Check this task's work."))
		};
		until(|| review().is_some()).await;
		assert!(
			review().unwrap().contains("the approved plan (local://readme-line-plan.md), to check the diff against it"),
			"{}",
			review().unwrap()
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn report_result_a_threads_report_is_validated_stored_in_its_task_file_and_shown_by_thread_status_its_decisions_become_searchable_in_the_projects_history_and_go_to_the_boss_for_lessons_0539_0603()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let started = crate::frontier::server::helpers::start_thread(&server, Default::default()).await;
		let pane_id = started.pane_id;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		state_of(&boss, &server, AgentState::Idle, None);
		// The report's files come from git and its tests from the check run, whatever the thread says (0677).
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let report = json!({
			"outcome": "done",
			"summary": "Readme line added",
			"files": ["ignored.txt"],
			"tests": [{ "command": "bun test", "passed": false }],
			"decisions": [{ "question": "Where the line goes", "choice": "The top", "why": "Read first" }],
			"questions": [],
			"followUps": ["Translate it"],
		});
		let not_thread = "report_result: only a thread's pane hands in a report, and this pane isn't a thread's.";
		assert_eq!(call(&server, "report_result", &report).await, said(not_thread, true));
		// A pane without a pane key may be one an earlier Jocasta started (0578).
		let keyless = "report_result: only a thread's pane hands in a report, and this pane isn't a thread's; a pane Jocasta started before panes had keys gets one when Jocasta starts it again, once it is idle.";
		assert_eq!(from_pane(&server, "report_result", &report, None).await, said(keyless, true));
		let mut long = report.clone();
		long["summary"] = json!("x".repeat(201));
		assert_eq!(from_pane(&server, "report_result", &long, Some(&pane_id)).await.is_error, Some(true));
		assert_eq!(
			from_pane(&server, "report_result", &report, Some(&pane_id)).await,
			said("Report stored for Readme line; its review reads it.", false)
		);
		let task = read(&task_dir_of(&project.id, "Readme line").join("task.md"));
		assert!(
			task.contains(
				"## Report\noutcome done · \"Readme line added\" · tests 0/0 · files: README.md · decisions: Where the line goes = The top · follow-ups: Translate it\n"
			),
			"{task}"
		);
		assert_eq!(
			call(&server, "thread_status", &json!({})).await.text,
			"demo/Readme line [feature/readme-line] idle · reported done"
		);
		let status = call(&server, "thread_status", &json!({ "thread": "Readme line" })).await;
		assert!(status.text.ends_with(
			"\nreport: outcome done · \"Readme line added\" · tests 0/0 · files: README.md · decisions: Where the line goes = The top · follow-ups: Translate it"
		));
		let catalog = open_catalog().expect("the catalog");
		let hits = search_history(&catalog, &project.id, "goes", 10).unwrap();
		assert_eq!(hits.len(), 1);
		assert_eq!(hits[0].text, "- Where the line goes: The top. Why: Read first");
		assert!(Regex::new(r"^decisions t-[0-9a-f]{4} Readme line$").unwrap().is_match(&hits[0].source));
		assert!(hits[0].at > 0);
		let told = "[event] demo/Readme line — reported its decisions: «Where the line goes = The top (Read first)» · offer him the lasting ones as lessons, once";
		until(|| prompts(&server).iter().any(|(pane, text)| *pane == boss && text == told)).await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_end_without_report_result_gets_one_steer_the_next_end_goes_to_the_review_whose_done_when_jocastas_runner_checks_first_giving_the_failing_gate_to_the_turn_its_exit_code_goes_in_the_proof_with_no_report_said_0453_0539_0540_0542_0546()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ask_report: Some(true), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let failing = "git rev-parse --verify no-such-ref";
		let (pane_id, path) = start_with_checks(&server, &[failing]).await;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		state_of(&boss, &server, AgentState::Idle, None);
		state_of(&pane_id, &server, AgentState::Working, None);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		state_of(&pane_id, &server, AgentState::Idle, None);
		until(|| prompts(&server).iter().any(|(pane, text)| *pane == pane_id && text == REPORT_STEER)).await;
		assert_eq!(review_asked(&server.herdr), 0);

		// It ends again without the report: the runner checks the done-when on the task's tree, then the review goes
		// on, once, with the brief and the failing gate.
		state_of(&pane_id, &server, AgentState::Working, None);
		state_of(&pane_id, &server, AgentState::Idle, None);
		let review = || {
			typed_to(&server, &pane_id).into_iter().find(|text| text.starts_with("Check this task's work."))
		};
		until(|| review().is_some()).await;
		let asked = review().unwrap();
		let gates = r"they failed; give the reviewer these gates too:\n\nchecks: FAIL \(tree [0-9a-f]{6}\)\n";
		assert!(Regex::new(gates).unwrap().is_match(&asked), "{asked}");
		assert!(asked.contains(&format!("  FAIL  done_when  {failing}\n")), "{asked}");
		assert!(asked.contains(&format!("        rerun: {failing}")), "{asked}");
		// No project check ran here, so the turn still runs the project's tests itself.
		assert!(asked.contains("the way its AGENTS.md or package scripts say"), "{asked}");
		assert!(asked.contains("his brief «Add a README line.», to check the diff against what he asked for"), "{asked}");
		assert!(asked.contains("the fact that the thread handed in no report"), "{asked}");
		assert_eq!(prompts(&server).iter().filter(|(_, text)| text == REPORT_STEER).count(), 1);

		// The review turn: the project's check passes; the done-when's result is the runner's stored one.
		let file = session_file(
			&path,
			&[
				Msg::new("Ran the check.", 0.1).bash("bun run check", true),
				Msg::new("Reviewed.\nTESTS: pass — `bun run check`", 0.1),
			],
		);
		hand_in_two_notes(&task_dir_of(&project.id, "Readme line"));
		state_of(&pane_id, &server, AgentState::Working, Some(&file));
		state_of(&pane_id, &server, AgentState::Idle, None);
		until(|| prompts(&server).iter().any(|(_, text)| text == HANDOFF_PROMPT)).await;
		let to_boss: Vec<String> = typed_to(&server, &boss);
		let line = format!(
			"[event] demo/Readme line — done and reviewed: 1 new commit «Add a README line», +1/−0 in 1 file, no test files changed, 2 notes, tests pass, done-when `{failing}` exit 128, no report"
		);
		assert!(to_boss.contains(&line), "{to_boss:?}");
		let task = read(&task_dir_of(&project.id, "Readme line").join("task.md"));
		let reviews = r"## Reviews\n- \d\d:\d\d not a pass · `bun run check` exit 0 · `git rev-parse --verify no-such-ref` exit 128 · notes: 2 notes, tests pass, no report\n$";
		assert!(Regex::new(reviews).unwrap().is_match(&task), "{task}");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_tool_and_the_review_say_when_the_projects_mise_check_didnt_run_and_why_never_a_silent_pass_0473_0559()
 {
	let mut j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	// mise lists the project's config, but its paranoid trust is off: Jocasta may not load it.
	j.set_var("FAKE_MISE_PARANOID", Some("false\n"));
	j.set_var("FAKE_MISE_TRUST", Some(&format!("{}\\mise.toml: trusted\n", project.main_path.display())));
	let fakes = j.root().join("fake-mise");
	fs::create_dir_all(&fakes).unwrap();
	let shim =
		crate::frontier::server::mise::tests::captured_shim(&fakes, "mise", "frontier::server::mise::tests::fake_mise");
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		mise: Some(Some(vec![shim.display().to_string()])),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let (pane_id, path) = start_with_checks(&server, &["git --version"]).await;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		state_of(&boss, &server, AgentState::Idle, None);
		let skipped = "the project's check didn't run: mise's paranoid config trust is off";
		let checked = from_pane(&server, "check", &json!({}), Some(&pane_id)).await;
		let expected = format!(r"^checks: PASS \(tree [0-9a-f]{{6}}\)\nNote: {}\.$", regex::escape(skipped));
		assert!(Regex::new(&expected).unwrap().is_match(&checked.text), "{}", checked.text);
		state_of(&pane_id, &server, AgentState::Working, None);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		state_of(&pane_id, &server, AgentState::Idle, None);
		until(|| typed_to(&server, &pane_id).iter().any(|text| text.starts_with("Check this task's work."))).await;
		let file = session_file(&path, &[Msg::new("Reviewed.\nTESTS: pass — `bun test`", 0.1).bash("bun test", true)]);
		hand_in_two_notes(&task_dir_of(&project.id, "Readme line"));
		state_of(&pane_id, &server, AgentState::Working, Some(&file));
		state_of(&pane_id, &server, AgentState::Idle, None);
		until(|| prompts(&server).iter().any(|(_, text)| text == HANDOFF_PROMPT)).await;
		let told = typed_to(&server, &boss).into_iter().find(|text| text.contains("done and reviewed"));
		let told = told.expect("the boss was told");
		assert!(told.contains(&format!(", {skipped}, no report")), "{told}");
		let task = read(&task_dir_of(&project.id, "Readme line").join("task.md"));
		assert!(task.contains(&format!("notes: 2 notes, tests pass, {skipped}, no report\n")), "{task}");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_tool_answers_a_thread_with_its_checks_on_its_tree_a_turn_ending_with_failing_edits_gets_the_gates_once_thread_status_says_each_threads_ledger_line_0538_0542_0546()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let clean = "git diff --quiet";
		let (pane_id, path) = start_with_checks(&server, &[clean]).await;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		state_of(&boss, &server, AgentState::Idle, None);
		// From the thread's pane only; a clean tree passes in one line.
		assert_eq!(from_pane(&server, "check", &json!({}), None).await.is_error, Some(true));
		let clean_text = from_pane(&server, "check", &json!({}), Some(&pane_id)).await.text;
		assert!(Regex::new(r"^checks: PASS \(tree [0-9a-f]{6}\)$").unwrap().is_match(&clean_text), "{clean_text}");

		// A turn ends with an uncommitted edit: the turn-end check is the quick one (0677), and this project has no
		// quick, verify or test task, so nothing goes back into the turn; the full check, with the done_when, runs ahead
		// of time on that tree, and the check tool answers from its stored result.
		write_readme(&path, "demo\nchanged\n");
		let file = session_file(&path, &[Msg::new("Edited the README.", 0.25)]);
		state_of(&pane_id, &server, AgentState::Working, Some(&file));
		state_of(&pane_id, &server, AgentState::Idle, None);
		let key = server
			.herdr
			.env_of(&pane_id)
			.and_then(|env| env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key))
			.expect("the pane's key");
		let (hold, signal) = tokio::sync::watch::channel(None::<String>);
		assert_eq!(crate::frontier::server::bridge::BridgeService::stop(&*server.service, key, signal).await, "");
		drop(hold);
		let mut block = from_pane(&server, "check", &json!({}), Some(&pane_id)).await.text;
		for _ in 0..100 {
			if block.starts_with("checks: FAIL") {
				break;
			}
			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
			block = from_pane(&server, "check", &json!({}), Some(&pane_id)).await.text;
		}
		assert!(block.contains(&format!("  FAIL  done_when  {clean}\n")), "{block}");
		assert!(block.contains(&format!("        rerun: {clean}")), "{block}");
		assert_eq!(from_pane(&server, "check", &json!({}), Some(&pane_id)).await.text, block);

		let status = call(&server, "thread_status", &json!({})).await;
		assert!(status.text.contains("\n  spend $0.25"), "{}", status.text);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn jocasta_thread_and_worktree_commands_run_the_services_own_functions_and_rules_worded_for_him_send_stop_finish_and_remove_ask_him_first_and_deny_does_nothing_a_done_when_the_guard_denies_or_asks_about_is_refused_0544_0580_0581()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let asks: Arc<Mutex<Vec<Confirmation>>> = Arc::default();
	let allow = Arc::new(AtomicBool::new(false));
	let (asked_in, allowed) = (Arc::clone(&asks), Arc::clone(&allow));
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		confirm: Some(Arc::new(move |ask, _| {
			asked_in.lock().unwrap_or_else(PoisonError::into_inner).push(ask);
			let allow = allowed.load(Ordering::SeqCst);
			async move { Ok(Answer { allow, how: PopupEnd::Answered }) }.boxed()
		})),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let asks_now = || asks.lock().unwrap_or_else(PoisonError::into_inner).clone();
		let brief = "Add a README line. Keep it short.";
		let asked = server.clone();
		let args = json!({ "project": "demo", "brief": brief, "done_when": ["bun test"] });
		let replied = tokio::spawn(async move { cli(&asked, "thread new", &args).await });
		until(|| server.herdr.of("createWorktree").len() == 1).await;
		let pane = server.herdr.state().pane_list.last().cloned().expect("a thread pane");
		pane.cwd.as_ref().expect("no thread pane");
		server.herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
		assert_eq!(
			replied.await.unwrap(),
			said(
				"Started demo/Add a README line on feature/add-a-readme-line: it plans first; approve its plan in its pane.",
				false
			)
		);
		states_settled(&server).await;
		// No Allow for a new thread, even outside the project he last focused: it waits at its plan for him.
		assert!(asks_now().is_empty());
		let task = read(&task_dir_of(&project.id, "Add a README line").join("task.md"));
		assert!(task.contains(&format!("## Brief\n{brief}\n\n## Done when\n- `bun test` passes\n")), "{task}");
		let line = "demo/Add a README line [feature/add-a-readme-line] idle";
		assert_eq!(cli(&server, "thread list", &json!({})).await, said(line, false));
		assert_eq!(cli(&server, "thread list", &json!({ "project": "demo" })).await, said(line, false));
		let status = cli(&server, "thread status", &json!({ "thread": "Add a README line" })).await;
		assert!(status.text.ends_with("\nreport: none yet"), "{}", status.text);

		// Deny: nothing stops, nothing is typed.
		assert_eq!(
			cli(&server, "thread stop", &json!({ "thread": "Add a README line" })).await,
			said("You chose Deny; nothing was stopped.", true)
		);
		assert!(server.herdr.of("stop").is_empty());
		assert_eq!(
			asks_now().last(),
			Some(&Confirmation {
				message: "Stop demo/Add a README line's run?".to_owned(),
				detail: "A `jocasta thread stop` command asks to stop the thread's current run (one Escape into its omp)."
					.to_owned(),
			})
		);
		assert_eq!(
			cli(&server, "thread send", &json!({ "thread": "Add a README line", "text": "Go on." })).await,
			said("You chose Deny; nothing was sent.", true)
		);
		assert!(!prompts(&server).iter().any(|(_, text)| text == "Go on."));
		assert_eq!(
			asks_now().last().map(|ask| ask.detail.clone()),
			Some("A `jocasta thread send` command asks to type it into the thread's pane:\n\nGo on.".to_owned())
		);

		// The tools' rules hold, and refuse before anything is asked.
		let asked = asks_now().len();
		assert_eq!(
			cli(&server, "thread finish", &json!({ "thread": "Add a README line", "how": "merge" })).await,
			said("demo/Add a README line has no commits ahead of main; nothing to finish.", true)
		);
		let nope = j.root().join("nope").display().to_string();
		assert_eq!(cli(&server, "worktree remove", &json!({ "path": nope })).await.is_error, Some(true));
		assert_eq!(asks_now().len(), asked);
		assert_eq!(cli(&server, "worktree list", &json!({})).await, said("No leftover worktrees.", false));

		// Allow: it runs.
		allow.store(true, Ordering::SeqCst);
		assert_eq!(
			cli(&server, "thread stop", &json!({ "thread": "Add a README line" })).await,
			said("Stopped demo/Add a README line's run.", false)
		);
		assert_eq!(server.herdr.of("stop"), vec![vec![json!(pane.pane_id)]]);
		assert_eq!(
			cli(&server, "thread send", &json!({ "thread": "Add a README line", "text": "Go on." })).await,
			said("Sent to demo/Add a README line.", false)
		);
		assert!(server.herdr.of("prompt").contains(&vec![json!(pane.pane_id), json!("Go on.")]));
		assert_eq!(cli(&server, "nope", &json!({})).await, said("no command jocasta nope.", true));
		// done_when runs outside omp's guard, so the guard's own rules decide first (0580): deny refuses, and one
		// it would ask about is refused here, as this door asks nothing. Nothing is made either way.
		let worktrees = server.herdr.of("createWorktree").len();
		assert_eq!(
			cli(
				&server,
				"thread new",
				&json!({ "project": "demo", "brief": "Push it.", "done_when": ["git push --force origin main"] })
			)
			.await,
			said(
				"jocasta thread new: done_when `git push --force origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started.",
				true
			)
		);
		let publish =
			cli(&server, "thread new", &json!({ "project": "demo", "brief": "Ship it.", "done_when": ["npm publish"] }))
				.await;
		assert!(
			publish.text.starts_with("jocasta thread new: done_when `npm publish` needs your Allow under Jocasta's guard"),
			"{}",
			publish.text
		);
		assert_eq!(server.herdr.of("createWorktree").len(), worktrees);
		assert_eq!(asks_now().len(), asked + 2);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_key_on_a_pane_that_isnt_a_threads_gets_the_plain_refusal_0581_ux_16() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		let refused = |result: crate::shared::errors::Result<String>| result.expect_err("a refusal").to_string();
		assert!(
			refused(server.service.key_stop(Some(&boss)).await)
				.contains("Stop: the focused pane isn't a thread's; focus a thread or pick one in today.")
		);
	})
	.await;
}

/// What a thread over its budget left: the question it asked, its record, `thread_status` and the aborts sent.
struct OverBudget {
	view: crate::frontier::protocol::PopupView,
	record: Value,
	status: String,
	stops: Vec<Vec<Value>>,
}

/// A thread started with `budget: 5` whose ledger reaches $5.10 at a turn's end (0686), asking him a question that
/// `choice` answers by its choices' index.
async fn over_budget(choice: usize) -> OverBudget {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let shown: Arc<Mutex<Vec<Popup>>> = Arc::default();
	let told = Arc::clone(&shown);
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		on_question: Some(Arc::new(move |popup| {
			told.lock().unwrap_or_else(PoisonError::into_inner).push(popup.clone())
		})),
		..ServerOptions::default()
	};
	let project_id = project.id.clone();
	let out: Arc<Mutex<Option<OverBudget>>> = Arc::default();
	let keep = Arc::clone(&out);
	with_server(options, |server| async move {
		let asked = server.clone();
		let args = json!({ "project": "demo", "title": "Readme line", "brief": BRIEF, "budget": 5 });
		let replied = tokio::spawn(async move { call(&asked, "thread_start", &args).await });
		until(|| server.herdr.of("createWorktree").len() == 1).await;
		let pane = server.herdr.state().pane_list.last().cloned().expect("a thread pane");
		let path = pane.cwd.clone().expect("no thread pane");
		server.herdr.set_state(&pane.pane_id, AgentState::Idle, None, true);
		replied.await.expect("the call ran");
		states_settled(&server).await;
		let boss = boss_pane(&server.herdr).expect("a boss pane");
		state_of(&boss, &server, AgentState::Idle, None);
		let record_file = task_dir_of(&project_id, "Readme line").join("thread.json");
		// The ledger reaches $5.10; the turn ends.
		let file = session_file(&path, &[Msg::new("A long turn.", 5.10)]);
		state_of(&pane.pane_id, &server, AgentState::Working, Some(&file));
		let idle = PaneAgent { idle_reported: true, ..PaneAgent::default() };
		pane_reports(&server, &pane.pane_id, &PaneEvent::AgentEnd { idle: true, jobs: 0 }, idle);
		let find = || shown.lock().unwrap_or_else(PoisonError::into_inner).iter().find(|p| p.open()).cloned();
		until(|| find().is_some()).await;
		let popup = find().expect("the question");
		let view = popup.view().clone();
		let status = call(&server, "thread_status", &json!({})).await.text;
		assert_eq!(server.answer(popup.code(), &json!({ "choice": view.choices[choice] })).0, AnswerTaken::Taken);
		until(|| !popup.open()).await;
		states_settled(&server).await;
		if choice == 0 {
			until(|| read(&record_file).contains("\"budget\": 80") || read(&record_file).contains("\"budget\":80"))
				.await;
		} else {
			until(|| !server.herdr.of("stop").is_empty()).await;
		}
		let record = serde_json::from_str(&read(&record_file)).unwrap_or(Value::Null);
		let stops = server.herdr.of("stop");
		*keep.lock().unwrap_or_else(PoisonError::into_inner) = Some(OverBudget { view, record, status, stops });
	})
	.await;
	let left = out.lock().unwrap_or_else(PoisonError::into_inner).take();
	left.expect("the test ran")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_at_its_budget_is_stuck_over_budget_and_asked_to_raise_or_stop_and_a_raise_adds_the_setting_0686() {
	let left = over_budget(0).await;
	assert_eq!(left.view.choices, ["Raise by $75", "Stop the thread"]);
	assert_eq!(left.view.detail.as_deref(), Some("over budget: $5.10 of $5"));
	assert!(left.status.contains("spend $5.10"), "{}", left.status);
	// The raise adds $75 to the thread's own budget.
	assert_eq!(left.record["budget"].as_f64(), Some(80.0), "{}", left.record);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_at_its_budget_that_he_stops_gets_its_turn_aborted_0686() {
	let left = over_budget(1).await;
	assert!(!left.stops.is_empty(), "no abort reached the pane");
	assert_eq!(left.record["budget"].as_f64(), Some(5.0), "{}", left.record);
}
