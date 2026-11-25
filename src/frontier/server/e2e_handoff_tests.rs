//! `test/frontier/server/service-handoff.test.ts`, case for case: a thread's `/handoff` step, its states, and the panes
//! Jocasta starts omp in again (herdr restored as shells, or omp ended), on the real server with its fake herdr.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use serde_json::{Value, json};

use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::bridge::{PaneAgent, PaneEvent};
use crate::frontier::server::helpers::{
	BRIEF, Msg, Page, README_THREAD_ENV, TEST_BOSS_KEY, TestServer, ThreadOptions, boss_pane, bridge_flags, call,
	commit_in, demo_project, guard, handoff_entry, keyed, pane_reports, project_keyed, review, session_file, sh,
	start_test_server_with, start_thread, thread_omp, thread_pane, to_handoff, until, until_async,
};
use crate::frontier::server::overlay::overlay_file;
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::status::Busy;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::frontier::server::surface::{AgentState, Env};
use crate::frontier::server::tools::ToolReply;
use crate::frontier::server::wording::HANDOFF_PROMPT;
use crate::shared::clock::testing::stand_still_at;
use crate::shared::log::logs_dir;
use crate::shared::pathkey::path_key;
use crate::shared::paths::data_dir;
use crate::shared::projects::Project;
use crate::shared::testing::temp_jocasta;

fn reply(text: &str, is_error: bool) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: is_error.then_some(true) }
}

/// `with_server` on a herdr the test made (so a test can restart the service on the same panes); always stops it.
async fn with_herdr<F, Fut>(herdr: &Arc<FakeSurface>, options: ServerOptions, run: F)
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = ()>,
{
	let server = start_test_server_with(herdr.clone(), options).await;
	let outcome = std::panic::AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	if let Err(panic) = outcome {
		std::panic::resume_unwind(panic);
	}
}

/// The options most cases give: omp up within 10 s.
fn options_with() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

fn prompts_to(server: &TestServer, pane: &str) -> Vec<String> {
	server
		.herdr
		.of("prompt")
		.iter()
		.filter(|args| args[0].as_str() == Some(pane))
		.filter_map(|args| args[1].as_str().map(str::to_owned))
		.collect()
}

/// Every log file's text, joined.
fn log_text() -> String {
	let dir = logs_dir();
	let Ok(names) = std::fs::read_dir(dir) else { return String::new() };
	names.flatten().map(|entry| std::fs::read_to_string(entry.path()).unwrap_or_default()).collect()
}

/// What a respawn call recorded: the pane, its cwd, command and env.
struct Respawn {
	pane: String,
	cwd: String,
	command: Vec<String>,
	env: Option<Env>,
}

/// The respawn calls from call number `before` on.
fn respawned_since(herdr: &FakeSurface, before: usize) -> Vec<Respawn> {
	let state = herdr.state();
	state.calls[before..]
		.iter()
		.filter(|call| call.method == "respawn")
		.map(|call| {
			let options = &call.args[1];
			Respawn {
				pane: call.args[0].as_str().unwrap().to_owned(),
				cwd: options["cwd"].as_str().unwrap().to_owned(),
				command: options["command"]
					.as_array()
					.unwrap()
					.iter()
					.map(|w| w.as_str().unwrap().to_owned())
					.collect(),
				env: options["env"].as_object().map(|map| {
					map.iter().map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned())).collect()
				}),
			}
		})
		.collect()
}

/// The project workspace's pane: the project's own omp in its main checkout.
fn project_pane(herdr: &FakeSurface) -> String {
	let state = herdr.state();
	let workspace = state.workspace_list.iter().find(|w| w.label == "demo").map(|w| w.id.clone());
	state
		.pane_list
		.iter()
		.find(|p| Some(&p.workspace_id) == workspace.as_ref())
		.map(|p| p.pane_id.clone())
		.expect("the project pane")
}

fn same_path(left: &str, right: &Path) -> bool {
	path_key(Path::new(left)) == path_key(right)
}

fn overlay_command(project: &Project) -> Vec<String> {
	let mut command = vec!["omp".to_owned(), "--continue".to_owned()];
	command.extend(guard());
	command.extend(bridge_flags());
	command.extend(["--config".to_owned(), overlay_file(&project.id).display().to_string()]);
	command
}

/// The boss's respawn (with boss.yml) and then the thread's and the project pane's, as the service starts them again.
fn assert_respawned(respawned: &[Respawn], project: &Project, path: &Path) {
	let boss = &respawned[0];
	assert_eq!(boss.command[..3], ["omp", "--continue", "--config"]);
	assert!(boss.command[3].ends_with("boss.yml"), "{}", boss.command[3]);
	let thread_vars = README_THREAD_ENV.to_vec();
	let project_vars: Vec<(&str, &str)> = Vec::new();
	let [_, thread, own] = respawned else { panic!("{} respawns", respawned.len()) };
	assert!(same_path(&thread.cwd, path), "{}", thread.cwd);
	assert_eq!(thread.command, thread_omp(&project.id, "Readme line"));
	assert_eq!(keyed(&thread_vars), thread.env.clone().expect("the thread's env"));
	assert!(same_path(&own.cwd, &project.main_path), "{}", own.cwd);
	assert_eq!(own.command, overlay_command(project));
	assert_eq!(project_keyed(&project_vars), own.env.clone().expect("the project pane's env"));
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_waits_for_a_handoff_that_hasn_t_written_its_entry_then_merges_co_2() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let file = to_handoff(&server, &pane_id, &path).await;
		let finish = || async {
			let args = json!({ "thread": "Readme line", "how": "merge" });
			call(&server, "thread_finish", &args).await
		};
		assert_eq!(
			finish().await,
			reply("demo/Readme line is still writing its handoff; try again in a minute.", true)
		);
		assert!(path.exists());
		handoff_entry(&file);
		// The session index reads a file again at most once a second: asked until it sees the entry.
		let mut said = finish().await;
		let deadline = Instant::now() + Duration::from_secs(15);
		while said.text.ends_with("is still writing its handoff; try again in a minute.") {
			assert!(Instant::now() < deadline, "timed out waiting for thread_finish past the handoff");
			tokio::time::sleep(Duration::from_millis(25)).await;
			said = finish().await;
		}
		assert_eq!(said.is_error, None);
		assert!(said.text.starts_with("Merged demo/Readme line into main"), "{}", said.text);
		assert!(!path.exists());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_states_stay_true_a_dismissed_question_ends_the_turn_a_handoff_with_no_entry_doesn_t_swallow_the_next_task_and_the_page_sees_the_boss_take_an_urgent_line_co_3_co_4_co_7()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let page = Page::open(&server, "").await;
		page.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. })).await;
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		until(|| server.service.boss_state() == AgentState::Idle).await;

		// He dismisses its question: nothing awaits him any more.
		let asked = session_file(&path, &[Msg::new("Which format?", 0.1).ask("CSV or JSON?")]);
		server.herdr.set_state(&pane_id, AgentState::Working, asked.to_str(), true);
		server.herdr.set_state(&pane_id, AgentState::Blocked, None, true);
		until(|| server.service.hud(None).unwrap().blocked == 1).await;
		assert_eq!(server.service.busy_threads().unwrap(), Busy { working: 0, awaiting: 1 });
		// The urgent line went into the idle boss at once.
		until(|| prompts_to(&server, &boss).iter().any(|text| text.contains("— awaiting you"))).await;
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| server.service.hud(None).unwrap().blocked == 0).await;
		assert_eq!(server.service.busy_threads().unwrap(), Busy { working: 0, awaiting: 0 });

		// The task ends, the review runs, /handoff is sent and writes no entry; the next task commits.
		to_handoff(&server, &pane_id, &path).await;
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		commit_in(&path, "demo\nA line.\nAnother.\n", "Another line");
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let review_text = review("main...feature/readme-line", Some(BRIEF));
		until(|| prompts_to(&server, &pane_id).iter().filter(|text| **text == review_text).count() == 2).await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_whose_omp_bridge_reports_idle_leaves_the_handoff_step_once_its_session_has_the_handoff_s_entry_f_01_0547()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = options_with();
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let file = to_handoff(&server, &pane_id, &path).await;
		let ended = || log_text().contains("demo/Readme line: handoff → task (its handoff is written)");
		// /handoff ran with no turn reported by herdr; its omp's bridge says the turn ended idle, and the entry is in the
		// session now.
		let idle = PaneAgent { idle_reported: true, ..PaneAgent::default() };
		pane_reports(&server, &pane_id, &PaneEvent::AgentEnd { idle: true, jobs: 0 }, idle);
		handoff_entry(&file);
		until_async(|| async {
			server.service.refresh().await;
			ended()
		})
		.await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_thread_start_calls_for_one_project_at_once_make_one_project_workspace_co_5() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(ServerOptions::default(), |server| async move {
		let on = &server;
		let start = |title: &'static str| async move {
			let args = json!({ "project": "demo", "title": title, "brief": "Go." });
			call(on, "thread_start", &args).await
		};
		let (one, two) = tokio::join!(start("One"), start("Two"));
		let first_words: Vec<&str> = [&one, &two].iter().map(|r| r.text.split(',').next().unwrap()).collect();
		assert_eq!(first_words, ["Started demo/One on branch feature/one", "Started demo/Two on branch feature/two"]);
		assert_eq!(server.herdr.state().workspace_list.iter().filter(|w| w.label == "demo").count(), 1);
		assert_eq!(server.herdr.of("createWorktree").len(), 2);
	})
	.await;
}

/// A gh that never answers (`fake-gh.ts` with `hang: true`): a script that waits, then exits 0.
fn hanging_gh(dir: &Path) -> Vec<String> {
	let (file, text) = if cfg!(windows) {
		("gh.cmd", "@echo off\r\nping -n 30 127.0.0.1 >nul\r\nexit /b 0\r\n")
	} else {
		("gh", "#!/bin/sh\nsleep 30\n")
	};
	let path = dir.join(file);
	std::fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gh_that_never_answers_thread_finish_pr_answers_within_its_bound_nothing_changes_and_the_service_still_stops_co_8()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let gh = hanging_gh(j.root());
	let options = ServerOptions {
		agent_wait_ms: Some(10_000),
		gh: Some(gh),
		gh_timeout_ms: Some(500),
		..ServerOptions::default()
	};
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let said = call(&server, "thread_finish", &json!({ "thread": "Readme line", "how": "pr" })).await;
		assert_eq!(said, reply("gh didn't answer in a minute; nothing was changed.", true));
		let prompted = server
			.herdr
			.of("prompt")
			.iter()
			.any(|args| args.get(1).and_then(Value::as_str).is_some_and(|text| text.contains("gh pr create")));
		assert!(!prompted);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn panes_herdr_restored_as_shells_while_the_service_runs_get_omp_again_with_their_flags_the_boss_with_boss_yml_the_thread_and_the_project_pane_with_their_overlay_0431()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let herdr = server.herdr.clone();
		let started = start_thread(&server, ThreadOptions::default()).await;
		let path = started.path;
		// A new project pane starts as a sleeper (0676); a key brings its omp.
		let ctx = server.service.context().clone();
		let sleeper = project_pane(&herdr);
		ctx.sleep.wake_by_key(&ctx.launched.key_of(&sleeper).expect("the sleeper's key")).await.unwrap();
		// The refresh tick notes each new pane's terminal.
		server.service.refresh().await;
		herdr.restore();
		let before = herdr.state().calls.len();
		server.service.refresh().await;
		let respawned = respawned_since(&herdr, before);
		assert_eq!(respawned.len(), 3);
		assert!(same_path(&respawned[0].cwd, &data_dir().join("home")) || respawned[0].cwd.ends_with("home"));
		assert_respawned(&respawned, &project, &path);
		// The thread is known by its new pane.
		let fresh = thread_pane(&herdr, &path);
		assert!(call(&server, "thread_status", &json!({})).await.text.contains("demo/Readme line"));
		assert!(!fresh.is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_that_ends_in_a_jocasta_pane_left_open_starts_again_there_resuming_its_session_the_boss_the_thread_and_the_project_pane_once_per_ending_0521()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let herdr = server.herdr.clone();
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let boss = boss_pane(&herdr).expect("the boss pane");
		// A new project pane starts as a sleeper (0676); a key brings its omp.
		let ctx = server.service.context().clone();
		let sleeper = project_pane(&herdr);
		ctx.sleep.wake_by_key(&ctx.launched.key_of(&sleeper).expect("the sleeper's key")).await.unwrap();
		let own = project_pane(&herdr);
		herdr.set_state(&boss, AgentState::Idle, None, true);
		herdr.set_state(&own, AgentState::Idle, None, true);
		// The refresh tick sees omp running in each pane's terminal.
		server.service.refresh().await;
		for id in [&boss, &pane_id, &own] {
			herdr.set_state(id, AgentState::Unknown, None, false);
		}
		let before = herdr.state().calls.len();
		server.service.refresh().await;
		let respawned = respawned_since(&herdr, before);
		let ids: Vec<&str> = respawned.iter().map(|r| r.pane.as_str()).collect();
		assert_eq!(ids, [boss.as_str(), pane_id.as_str(), own.as_str()]);
		assert_eq!(keyed(&[("JOCASTA_BOSS_KEY", TEST_BOSS_KEY)]), respawned[0].env.clone().expect("the boss's env"));
		assert_respawned(&respawned, &project, &path);
		// The new omps are still starting: one ending started each once.
		server.service.refresh().await;
		server.service.refresh().await;
		assert_eq!(respawned_since(&herdr, before).len(), 3);
		// The thread goes on in its new pane.
		assert!(call(&server, "thread_status", &json!({})).await.text.contains("demo/Readme line"));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_jocasta_pane_whose_omp_hasn_t_been_seen_running_is_left_alone_the_project_pane_before_its_omp_reports_0521()
{
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let herdr = server.herdr.clone();
		start_thread(&server, ThreadOptions::default()).await;
		let before = herdr.state().calls.len();
		server.service.refresh().await;
		server.service.refresh().await;
		let own = project_pane(&herdr);
		let state = herdr.state().pane_list.iter().find(|p| p.pane_id == own).map(|p| p.agent_state);
		assert_eq!(state, Some(AgentState::Unknown));
		assert!(respawned_since(&herdr, before).is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_being_finished_its_omp_ending_as_its_workspace_closes_isn_t_started_again_0521() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let boss = boss_pane(&server.herdr).expect("the boss pane");
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		server.service.refresh().await;
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		// herdr stops the workspace's terminals: omp ends while its pane is still listed, and a refresh runs.
		let stopping = server.clone();
		server.herdr.state().on_close_workspace = Some(Arc::new(move |_workspace| {
			let stopping = stopping.clone();
			let pane_id = pane_id.clone();
			async move {
				stopping.herdr.set_state(&pane_id, AgentState::Unknown, None, false);
				stopping.service.refresh().await;
			}
			.boxed()
		}));
		let said = call(&server, "thread_finish", &json!({ "thread": "Readme line", "how": "merge" })).await;
		assert!(said.text.contains("its worktree, workspace and branch are removed"), "{}", said.text);
		server.service.refresh().await;
		assert!(server.herdr.of("respawn").is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_that_ends_once_the_service_stopped_isn_t_started_again_by_it_or_by_the_next_start_0521() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeSurface::new());
	let first = start_test_server_with(herdr.clone(), options_with()).await;
	let thread = start_thread(&first, ThreadOptions::default()).await.pane_id;
	first.service.refresh().await;
	first.stop().await;
	herdr.set_state(&thread, AgentState::Unknown, None, false);
	first.service.refresh().await;
	assert!(herdr.of("respawn").is_empty());
	with_herdr(&herdr.clone(), options_with(), |server| async move {
		server.service.refresh().await;
		server.service.refresh().await;
		let again = herdr.of("respawn").iter().filter(|args| args[0].as_str() == Some(thread.as_str())).count();
		assert_eq!(again, 0);
		assert!(call(&server, "thread_status", &json!({})).await.text.contains("demo/Readme line"));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_omp_that_keeps_ending_in_its_pane_starts_again_after_a_wait_that_doubles_said_once_in_jocasta_s_log_one_that_ran_10_minutes_starts_again_at_once_0521()
 {
	let _j = temp_jocasta();
	crate::frontier::server::helpers::with_server(ServerOptions::default(), |server| async move {
		let herdr = server.herdr.clone();
		let ends = || async {
			let boss = boss_pane(&herdr).expect("the boss pane");
			herdr.set_state(&boss, AgentState::Idle, None, false);
			server.service.refresh().await;
			herdr.set_state(&boss, AgentState::Unknown, None, false);
			server.service.refresh().await;
			boss
		};
		let starts = || herdr.of("respawn").len();
		let before = starts();
		// The clock stands still between the steps, so each wait is exact.
		let t = chrono::Utc::now().timestamp_millis();
		let stand = stand_still_at(t);
		// The first ending: at once.
		ends().await;
		assert_eq!(starts(), before + 1);
		// It ended again: it waits 10 s, said once in the log.
		let held = ends().await;
		server.service.refresh().await;
		assert_eq!(starts(), before + 1);
		let log = log_text();
		assert_eq!(log.split(&format!("omp ended in pane {held}")).count(), 2, "{log}");
		stand.at(t + 10_000);
		server.service.refresh().await;
		assert_eq!(starts(), before + 2);
		// Once more: the wait is 20 s now.
		ends().await;
		stand.at(t + 20_000);
		server.service.refresh().await;
		assert_eq!(starts(), before + 2);
		stand.at(t + 30_000);
		server.service.refresh().await;
		assert_eq!(starts(), before + 3);
		// It ran 10 minutes before it ended: at once again.
		stand.at(t + 30_000 + 10 * 60_000);
		ends().await;
		assert_eq!(starts(), before + 4);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn r3_a_working_thread_whose_pane_herdr_restored_as_a_shell_is_relaunched_its_new_omp_comes_up_idle_without_an_event_and_the_close_question_counts_0_working_0438()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	crate::frontier::server::helpers::with_server(options_with(), |server| async move {
		let herdr = server.herdr.clone();
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		herdr.set_state(&pane_id, AgentState::Working, None, true);
		until(|| server.service.busy_threads().unwrap().working == 1).await;
		server.service.refresh().await;
		herdr.restore();
		server.service.refresh().await;
		let fresh = thread_pane(&herdr, &path);
		assert_ne!(fresh, pane_id);
		// The new omp sits idle; herdr pushes no change for it, its pane list says idle.
		herdr.set_state(&fresh, AgentState::Idle, None, false);
		server.service.refresh().await;
		until(|| server.service.busy_threads().unwrap().working == 0).await;
		assert_eq!(server.service.busy_threads().unwrap(), Busy { working: 0, awaiting: 0 });
	})
	.await;
}

#[allow(dead_code, reason = "HANDOFF_PROMPT is what to_handoff waits for; named here for the cases that read its text")]
const HANDOFF_SENT: &str = HANDOFF_PROMPT;
