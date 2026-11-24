//! The service's regression cases through the real server: the cases of
//! `test/frontier/server/service-regressions.test.ts`, case for case (the full review's regressions, 0573).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::FutureExt;
use regex::Regex;
use serde_json::{Map, Value, json};
use tokio::sync::watch;

use crate::frontier::protocol::{QuotaSpan, ServerMessage, ToastLevel, ToastReason};
use crate::frontier::server::allow::{Answer, Confirm, Confirmation};
use crate::frontier::server::bridge::Bridge;
use crate::frontier::server::day::DayOptions;
use crate::frontier::server::events::Toast;
use crate::frontier::server::helpers::{
	BRIEF, Msg, Page, TEST_BOSS_KEY, TestServer, ThreadOptions, boss_idle, boss_lines, boss_pane, call, commit_in,
	demo_project, push_state, relaunch_idle, review, session_file, sh, start_test_server, start_test_server_with,
	start_thread, states_settled, task_dir_of, thread_pane, until, until_async, use_hunk_daemon, with_server,
	write_settings,
};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Env, Herdr, HerdrError, NotificationResult, WorkspaceInfo};
use crate::frontier::server::keys::FinishHow;
use crate::frontier::server::popups::PopupEnd;
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::tools::{Caller, ToolReply};
use crate::frontier::server::wording::{
	COMMIT_LINE, HANDOFF_PROMPT, LESSONS_LINE, REPORT_STEER, WHEN_YOU_FINISH, fallback_slug,
};
use crate::shared::clock::testing::stand_still_at;
use crate::shared::layer::{HOSTS_ENV, LAYER_ENV};
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::testing::{temp_dir, temp_jocasta};

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn reply(text: &str, is_error: bool) -> ToolReply {
	ToolReply { text: text.to_owned(), is_error: is_error.then_some(true) }
}

fn now_ms() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

/// A tool call as `jocasta mcp` sends it from a pane whose `JOCASTA_PANE_KEY` is `pane_key`, without the boss key.
async fn call_with_key(server: &TestServer, tool: &str, args: &Value, pane_key: &str) -> ToolReply {
	let caller = Caller { pane_key: Some(pane_key.to_owned()), boss_key: None };
	server.service.call_tool(tool, args.as_object().unwrap_or(&Map::new()), &caller).await
}

/// `with_server` on a herdr the test made, so the test can reach it (and a wrapper of it).
async fn with_herdr<F, Fut>(herdr: &Arc<FakeHerdr>, options: ServerOptions, run: F)
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

fn workspace_of(herdr: &FakeHerdr, pane_id: &str) -> String {
	herdr.state().pane_list.iter().find(|p| p.pane_id == pane_id).expect("the pane").workspace_id.clone()
}

fn typed_into(herdr: &FakeHerdr, pane: &str, text: &str) -> usize {
	herdr.of("prompt").iter().filter(|args| args[0] == pane && args[1] == text).count()
}

/// Prompts into the boss pane wait while `gate` is set, as the prompt's round trip takes its time.
struct Gate {
	gate: Mutex<Option<watch::Receiver<bool>>>,
	waiting: AtomicBool,
}

/// A recording bridge whose prompts into the boss pane wait for `gate`; everything else is recorded as the fake does.
fn gated_bridge(fake: &Arc<FakeHerdr>, gate: &Arc<Gate>) -> Bridge {
	let bridge = fake.recording_bridge();
	let (fake, gate) = (Arc::clone(fake), Arc::clone(gate));
	bridge.record(|recording| {
		recording.sink = Some(Arc::new(move |method, args| {
			let held = locked(&gate.gate).clone();
			if let Some(open) = held
				&& method == "prompt"
				&& args.first().and_then(Value::as_str) == boss_pane(&fake).as_deref()
			{
				gate.waiting.store(true, Ordering::SeqCst);
				while !*open.borrow() {
					std::thread::sleep(std::time::Duration::from_millis(5));
				}
			}
			fake.bridge_call(method, args)
		}));
	});
	bridge
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_2_a_thread_started_while_the_tick_tells_the_boss_of_a_closed_one_is_kept_and_never_said_closed() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let fake = Arc::new(FakeHerdr::new());
	let gated = Arc::new(Gate { gate: Mutex::new(None), waiting: AtomicBool::new(false) });
	let options = ServerOptions { bridge: Some(gated_bridge(&fake, &gated)), ..ServerOptions::default() };
	let fake_in = fake.clone();
	with_herdr(&fake, options, |server| async move {
		let fake = fake_in;
		fake.set_state(&boss_pane(&fake).expect("a boss pane"), AgentState::Idle, None, true);
		let a = start_thread(&server, ThreadOptions { title: Some("Task A"), ..ThreadOptions::default() }).await;
		fake.close_workspace(&workspace_of(&fake, &a.pane_id)).await.unwrap();
		// The tick that sees A gone tells the boss, and that prompt takes its time; B starts meanwhile.
		let (open, closed) = watch::channel(false);
		*locked(&gated.gate) = Some(closed);
		let service = server.service.clone();
		let tick = tokio::spawn(async move { service.refresh().await });
		until(|| gated.waiting.load(Ordering::SeqCst)).await;
		let b = start_thread(&server, ThreadOptions { title: Some("Task B"), ..ThreadOptions::default() }).await;
		*locked(&gated.gate) = None;
		open.send_replace(true);
		tick.await.unwrap();
		boss_idle(&server);
		server.service.refresh().await;
		boss_idle(&server);
		server.service.refresh().await;
		states_settled(&server).await;
		assert!(fake.state().pane_list.iter().any(|p| p.pane_id == b.pane_id));
		assert_eq!(call(&server, "thread_status", &json!({})).await.text, "demo/Task B [feature/task-b] idle");
		let closed: Vec<String> = boss_lines(&server).into_iter().filter(|l| l.contains("closed;")).collect();
		assert_eq!(closed, ["[event] demo/Task A — closed; its worktree and branch stay"]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_3_a_thread_pane_runs_omp_again_after_a_restart_and_the_sidebar_is_reported() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let fake = Arc::new(FakeHerdr::new());
	let server = start_test_server_with(fake.clone(), ServerOptions::default()).await;
	let path = start_thread(&server, ThreadOptions::default()).await.path;
	server.service.refresh().await;
	server.stop().await;
	let reports = || fake.of("reportPaneMetadata").len() + fake.of("reportWorkspaceMetadata").len();
	let cleared = reports();
	fake.restore();
	let server = start_test_server_with(fake.clone(), ServerOptions::default()).await;
	server.service.refresh().await;
	let respawned =
		fake.of("respawn").iter().any(|args| path_key(Path::new(args[1]["cwd"].as_str().unwrap())) == path_key(&path));
	assert!(respawned);
	// The tick reached its last step: the sidebar's tokens, cleared at the stop, are reported again.
	assert!(reports() > cleared);
	server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_5_thread_finish_looks_again_after_his_allow_a_thread_at_work_meanwhile_isnt_merged_0544() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let (allow, allowed) = watch::channel(false);
	let asked = Arc::new(AtomicBool::new(false));
	let (told, wait) = (asked.clone(), allowed);
	let confirm: Confirm = Arc::new(move |ask: Confirmation, _| {
		let (told, mut wait) = (told.clone(), wait.clone());
		async move {
			if ask.message.starts_with("Start the thread") {
				return Ok(Answer { allow: true, how: PopupEnd::Answered });
			}
			told.store(true, Ordering::SeqCst);
			let _ = wait.wait_for(|open| *open).await;
			Ok(Answer { allow: true, how: PopupEnd::Answered })
		}
		.boxed()
	});
	with_server(ServerOptions { confirm: Some(confirm), ..ServerOptions::default() }, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let trunk_head = sh(&project.main_path, &["rev-parse", "HEAD"]);
		let asking = server.clone();
		let finishing = tokio::spawn(async move {
			call(&asking, "thread_finish", &json!({ "thread": "Readme line", "how": "merge" })).await
		});
		until(|| asked.load(Ordering::SeqCst)).await;
		// While his Allow waits, the thread is at work again.
		push_state(&server, &started.pane_id, AgentState::Working, None).await;
		allow.send_replace(true);
		assert_eq!(
			finishing.await.unwrap(),
			reply("demo/Readme line isn't idle (working); finish it once it is.", true)
		);
		assert_eq!(sh(&project.main_path, &["rev-parse", "HEAD"]), trunk_head);
		assert!(server.herdr.of("closeWorkspace").is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_6_an_omp_started_again_mid_review_runs_the_review_again_and_mid_handoff_runs_handoff_again_0582() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let hunk = use_hunk_daemon().await;
	let mut options = ServerOptions::default();
	hunk.apply(&mut options);
	with_server(options, |server| async move {
		let herdr = server.herdr.clone();
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let range = "main...feature/readme-line";
		let asked_review = review(range, Some(BRIEF));
		herdr.set_state(&pane_id, AgentState::Working, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| typed_into(&herdr, &pane_id, &asked_review) == 1).await;
		// Mid-review, herdr restores the pane as a shell: omp starts again, idle, its review turn gone.
		herdr.set_state(&pane_id, AgentState::Working, None, true);
		relaunch_idle(&server, &herdr, &path, None).await;
		let reviewing = thread_pane(&herdr, &path);
		assert_ne!(reviewing, pane_id);
		until(|| typed_into(&herdr, &reviewing, &asked_review) == 1).await;
		// The review turn ends on its test line: /handoff. omp starts again mid-handoff: /handoff goes in again.
		let file =
			session_file(&path, &[Msg::new("Reviewed.\n\n**TESTS: pass — bun test**", 0.1).bash("bun test", true)]);
		herdr.set_state(&reviewing, AgentState::Working, file.to_str(), true);
		herdr.set_state(&reviewing, AgentState::Idle, None, true);
		until(|| typed_into(&herdr, &reviewing, HANDOFF_PROMPT) == 1).await;
		herdr.set_state(&reviewing, AgentState::Working, None, true);
		relaunch_idle(&server, &herdr, &path, file.to_str()).await;
		let handing = thread_pane(&herdr, &path);
		assert_ne!(handing, reviewing);
		until(|| typed_into(&herdr, &handing, HANDOFF_PROMPT) == 1).await;
	})
	.await;
}

/// A stand-in for GitHub CLI as `fake-gh.ts` is with an empty state file: `auth status` passes, `pr view` finds no PR.
fn fake_gh(dir: &Path) -> Vec<String> {
	let (file, text) = if cfg!(windows) {
		let script = "@echo off\r\nif \"%1 %2\"==\"auth status\" goto auth\r\nif \"%1 %2\"==\"pr view\" goto view\r\nexit /b 0\r\n:auth\r\necho github.com\r\nexit /b 0\r\n:view\r\necho no pull requests found for branch \"%3\" 1>&2\r\nexit /b 1\r\n";
		("gh.cmd", script)
	} else {
		let script = "#!/bin/sh\ncase \"$1 $2\" in\n\"auth status\") echo github.com;;\n\"pr view\") echo \"no pull requests found for branch \\\"$3\\\"\" >&2; exit 1;;\nesac\n";
		("gh", script)
	};
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_6_an_omp_started_again_mid_pr_goes_back_to_its_task_and_the_boss_hears_to_finish_it_again_0582() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["remote", "add", "origin", "https://github.com/test/demo.git"]);
	let gh_dir = temp_dir("regressions-gh");
	let options = ServerOptions { gh: Some(fake_gh(gh_dir.path())), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let herdr = server.herdr.clone();
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		let pr_args = json!({ "thread": "Readme line", "how": "pr" });
		let pr = || call(&server, "thread_finish", &pr_args);
		let pushed = "Asked demo/Readme line to push feature/readme-line";
		assert!(pr().await.text.starts_with(pushed));
		herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		relaunch_idle(&server, &herdr, &started.path, None).await;
		let line =
			"[event] demo/Readme line — no PR: its omp started again mid-turn; everything stays, finish it again";
		until(|| {
			boss_idle(&server);
			boss_lines(&server).iter().any(|l| l == line)
		})
		.await;
		assert!(started.path.exists());
		// Back at its task: finishing it asks for the PR again.
		assert!(pr().await.text.starts_with(pushed));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_7_a_start_that_fails_once_herdr_made_its_worktree_takes_back_the_workspace_worktree_and_branch_the_next_start_gets_the_same_name()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let root = j.root().to_path_buf();
	with_server(ServerOptions::default(), |server| async move {
		server.herdr.state().create_worktree_fails = Some(HerdrError::new("error", "herdr's layout failed"));
		let failed =
			call(&server, "thread_start", &json!({ "project": "demo", "title": "Readme line", "brief": BRIEF })).await;
		assert_eq!(failed.is_error, Some(true));
		assert!(failed.text.contains("herdr's layout failed"), "{}", failed.text);
		let folder = root.join("demo.wt").join("readme-line");
		assert!(!folder.exists());
		assert_eq!(sh(&project.main_path, &["branch", "--list", "feature/readme-line"]), "");
		assert!(!server.herdr.state().workspace_list.iter().any(|w| w.label == "Readme line"));
		assert_eq!(call(&server, "thread_status", &json!({})).await.text, "No threads.");
		server.herdr.state().create_worktree_fails = None;
		let path = start_thread(&server, ThreadOptions::default()).await.path;
		// The worktree's path is git's, long names resolved; the temp root may be an 8.3 short name (CI's Windows).
		assert_eq!(path_key(&path), path_key(&real_path(&folder)));
	})
	.await;
}

fn local_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
	use chrono::TimeZone;
	chrono::Local.with_ymd_and_hms(year, month, day, hour, minute, 0).single().expect("a local time").timestamp_millis()
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_9_a_thread_closed_in_quiet_hours_is_in_todays_activity_at_once_and_reaches_the_boss_once_they_end_0298() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	write_settings(&config_dir(), &json!({ "service": { "quietHours": "22:00-08:00" } }));
	let at = Arc::new(AtomicI64::new(local_ms(2026, 10, 24, 17, 0)));
	let clock = at.clone();
	let day = DayOptions {
		pollers: Some(false),
		now: Some(Arc::new(move || clock.load(Ordering::SeqCst))),
		..DayOptions::default()
	};
	with_server(ServerOptions { day: Some(day), ..ServerOptions::default() }, |server| async move {
		server.herdr.set_state(&boss_pane(&server.herdr).expect("a boss pane"), AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let day = server.service.day();
		at.store(local_ms(2026, 10, 24, 23, 0), Ordering::SeqCst);
		day.tick().unwrap();
		let what = "demo/Readme line — closed; its worktree and branch stay";
		let to_boss = || boss_lines(&server).iter().filter(|l| **l == format!("[event] {what}")).count();
		server.herdr.close_workspace(&workspace_of(&server.herdr, &started.pane_id)).await.unwrap();
		server.service.refresh().await;
		until(|| day.dashboard().activity.events.iter().any(|e| e.text == what)).await;
		boss_idle(&server);
		assert_eq!(to_boss(), 0);
		// Quiet hours over: the line that waited goes in, once.
		at.store(local_ms(2026, 10, 25, 8, 1), Ordering::SeqCst);
		day.tick().unwrap();
		until(|| to_boss() == 1).await;
		boss_idle(&server);
		server.service.refresh().await;
		assert_eq!(to_boss(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_13_jocasta_closing_refuses_new_calls_and_waits_for_a_merge_in_flight_which_lands_and_removes_its_thread_0549()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let main = project.main_path.clone();
	sh(&main, &["config", "user.name", "Test"]);
	sh(&main, &["config", "user.email", "test@example.com"]);
	let server = start_test_server(ServerOptions::default()).await;
	let path = start_thread(&server, ThreadOptions::default()).await.path;
	commit_in(&path, "demo\nA line.\n", "Add a README line");
	// The repo's pre-commit hook takes a while (CO-10); Jocasta is closed meanwhile.
	let hooks = j.root().join("hooks");
	let marker = j.root().join("hook-ran");
	fs::create_dir_all(&hooks).unwrap();
	let hook = hooks.join("pre-commit");
	fs::write(&hook, format!("#!/bin/sh\ntouch \"{}\"\nsleep 1\n", marker.display().to_string().replace('\\', "/")))
		.unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
	}
	sh(&main, &["config", "core.hooksPath", &hooks.display().to_string()]);
	let boss = Caller { pane_key: None, boss_key: Some(TEST_BOSS_KEY.to_owned()) };
	let args: Map<String, Value> =
		json!({ "thread": "Readme line", "how": "merge" }).as_object().expect("an object").clone();
	let service = server.service.clone();
	let merging_boss = boss.clone();
	let finishing = tokio::spawn(async move { service.call_tool("thread_finish", &args, &merging_boss).await });
	until(|| marker.exists()).await;
	let closing = server.clone();
	let stopping = tokio::spawn(async move { closing.stop().await });
	// A call once the close began is refused, nothing done.
	let refused = Arc::new(Mutex::new(String::new()));
	let (asking, said) = (server.service.clone(), refused.clone());
	let asker = tokio::spawn(async move {
		loop {
			let answer = asking.call_tool("thread_status", &Map::new(), &boss).await;
			if answer.is_error == Some(true) {
				*locked(&said) = answer.text;
				return;
			}
		}
	});
	until(|| !locked(&refused).is_empty()).await;
	assert_eq!(*locked(&refused), "Jocasta is closing; nothing was changed.");
	let merged = finishing.await.unwrap().text;
	let shape = Regex::new(
		r"^Merged demo/Readme line into main as [0-9a-f]+ \(1 commit squashed\); its worktree, workspace and branch are removed\.$",
	)
	.unwrap();
	assert!(shape.is_match(&merged), "{merged}");
	assert!(!path.exists());
	stopping.await.unwrap();
	asker.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn svc_14_a_projects_running_count_goes_to_its_own_workspace_not_a_later_one_with_the_same_label() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let root = j.root().to_path_buf();
	with_server(ServerOptions::default(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let project_ws = server
			.herdr
			.state()
			.workspace_list
			.iter()
			.find(|w| w.label == "demo")
			.map(|w| w.id.clone())
			.expect("the project's workspace");
		server.herdr.state().workspace_list.push(WorkspaceInfo {
			id: "w_late".to_owned(),
			label: "demo".to_owned(),
			cwd: Some(root),
		});
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		until(|| server.service.busy_threads().unwrap().working == 1).await;
		// A tick already running when the thread went to work may be the one `refresh` joins: tick until it is reported.
		let reports = |herdr: &FakeHerdr| -> Vec<Value> {
			herdr.of("reportWorkspaceMetadata").into_iter().map(|args| args[0].clone()).collect()
		};
		until_async(|| {
			let (server, project_ws) = (server.clone(), project_ws.clone());
			async move {
				server.service.refresh().await;
				reports(&server.herdr).contains(&json!(project_ws))
			}
		})
		.await;
		let reported = reports(&server.herdr);
		assert!(!reported.contains(&json!("w_late")));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_loc_6_svc_14_worktree_remove_keeps_a_folder_whose_worktrees_git_cant_list_says_why_and_the_folder_keeps_its_number()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let config = project.main_path.join(".git").join("config");
	let saved = Arc::new(Mutex::new(String::new()));
	let (kept, config_file) = (saved.clone(), config.clone());
	// The thread's start asks too (no project pane focused yet); only the removal's Allow breaks git.
	let confirm: Confirm = Arc::new(move |ask: Confirmation, _| {
		let (kept, config) = (kept.clone(), config_file.clone());
		async move {
			if ask.message.starts_with("Remove") {
				let text = fs::read_to_string(&config).unwrap();
				fs::write(&config, format!("{text}\n[broken\n")).unwrap();
				*locked(&kept) = text;
			}
			Ok(Answer { allow: true, how: PopupEnd::Answered })
		}
		.boxed()
	});
	with_server(ServerOptions { confirm: Some(confirm), ..ServerOptions::default() }, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let path = started.path;
		server.herdr.close_workspace(&workspace_of(&server.herdr, &started.pane_id)).await.unwrap();
		server.service.refresh().await;
		// His Allow comes; git then fails on the repo's config: the folder may hold his work, so it stays.
		let removed = call(&server, "worktree_remove", &json!({ "path": path.display().to_string() })).await;
		assert_eq!(
			removed,
			reply(
				&format!(
					"{} couldn't be removed (git couldn't list demo's worktrees, so the folder stays); it stays.",
					path.display()
				),
				true
			)
		);
		fs::write(&config, locked(&saved).as_str()).unwrap();
		assert!(path.join("README.md").exists());
		let numbers: Value =
			serde_json::from_str(&fs::read_to_string(data_dir().join("thread-numbers.json")).unwrap()).unwrap();
		let values: Vec<&Value> = numbers.as_object().unwrap().values().collect();
		assert_eq!(values, [&json!(1)]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_loc_7_finish_refuses_a_thread_whose_worktree_was_switched_to_another_branch_nothing_is_merged() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		let path = start_thread(&server, ThreadOptions::default()).await.path;
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		sh(&path, &["checkout", "-q", "-b", "elsewhere"]);
		let trunk_head = sh(&project.main_path, &["rev-parse", "HEAD"]);
		assert_eq!(
			call(&server, "thread_finish", &json!({ "thread": "Readme line", "how": "merge" })).await,
			reply(
				"demo/Readme line's worktree is on elsewhere, not feature/readme-line; finish it on its branch.",
				true
			)
		);
		assert_eq!(sh(&project.main_path, &["rev-parse", "HEAD"]), trunk_head);
		assert!(sh(&project.main_path, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
		assert!(server.herdr.of("closeWorkspace").is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_loc_5_at_the_bosss_door_a_done_when_the_guard_denies_is_refused_unasked_one_it_asks_about_waits_for_his_allow_which_names_it_0580()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let asks: Arc<Mutex<Vec<Confirmation>>> = Arc::default();
	let allow = Arc::new(AtomicBool::new(false));
	let (kept, decides) = (asks.clone(), allow.clone());
	let confirm: Confirm = Arc::new(move |ask: Confirmation, _| {
		locked(&kept).push(ask);
		let allow = decides.load(Ordering::SeqCst);
		async move { Ok(Answer { allow, how: PopupEnd::Answered }) }.boxed()
	});
	with_server(ServerOptions { confirm: Some(confirm), ..ServerOptions::default() }, |server| async move {
		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		let start = |done_when: &str| {
			let args = json!({ "project": "demo", "title": "Ship", "brief": "Ship it.", "done_when": [done_when] });
			let server = server.clone();
			async move { call(&server, "thread_start", &args).await }
		};
		assert_eq!(
			start("git push --force origin main").await,
			reply(
				"thread_start: done_when `git push --force origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started.",
				true
			)
		);
		// Quotes don't launder a denied command: omp's own matcher sees it without them.
		assert_eq!(
			start("git push --for\"ce\" origin main").await,
			reply(
				"thread_start: done_when `git push --for\"ce\" origin main` is denied by Jocasta's guard (`*git*push* --force *`); no thread was started.",
				true
			)
		);
		assert!(locked(&asks).is_empty());
		assert_eq!(start("npm publish").await, reply("He chose Deny; no thread was started.", true));
		assert_eq!(locked(&asks).len(), 1);
		assert_eq!(locked(&asks)[0].message, "Start the thread \"Ship\" in demo?");
		assert!(locked(&asks)[0].detail.contains("It runs when the work is reviewed:  `npm publish`"));
		assert!(server.herdr.of("createWorktree").is_empty());
		allow.store(true, Ordering::SeqCst);
		assert!(start("npm publish").await.text.starts_with("Started demo/Ship on branch feature/ship."));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sec_net_6_a_pane_key_names_only_its_own_pane_a_forged_one_gets_no_threads_tools_and_a_threads_key_reports_for_that_thread_0578()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		start_thread(&server, ThreadOptions::default()).await;
		let second =
			start_thread(&server, ThreadOptions { title: Some("Second"), brief: Some("More."), ..Default::default() })
				.await;
		let report = json!({
			"outcome": "done",
			"summary": "Second done",
			"files": ["README.md"],
			"tests": [{ "command": "bun test", "passed": true }],
			"decisions": [],
			"questions": [],
			"followUps": [],
		});
		let forged = "f".repeat(64);
		assert_eq!(
			call_with_key(&server, "report_result", &report, &forged).await,
			reply("report_result: only a thread's pane hands in a report, and this pane isn't a thread's.", true)
		);
		assert_eq!(
			call_with_key(&server, "check", &json!({}), &forged).await,
			reply("check: only a thread's pane runs its checks, and this pane isn't a thread's.", true)
		);
		let key = server
			.herdr
			.env_of(&second.pane_id)
			.and_then(|env: Env| env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key))
			.expect("the pane's key");
		assert!(Regex::new("^[0-9a-f]{64}$").unwrap().is_match(&key), "{key}");
		assert_eq!(
			call_with_key(&server, "report_result", &report, &key).await,
			reply("Report stored for Second; its review reads it.", false)
		);
		let text = call(&server, "thread_status", &json!({})).await.text;
		let status: Vec<&str> = text.split('\n').collect();
		assert!(status.contains(&"demo/Second [feature/second] idle · reported done"), "{status:?}");
		assert!(status.contains(&"demo/Readme line [feature/readme-line] idle"), "{status:?}");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ux_11_c11_his_new_thread_key_starts_a_macedonian_title_on_a_transliterated_branch_and_another_scripts_on_thread_date_n_each_answer_is_text_is_error_a_half_done_start_an_error_0581()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		let mk = server.service.key_start("demo", "Поправи го најавувањето кога сесијата е истечена.").await.unwrap();
		assert!(!mk.is_error);
		let shape = Regex::new(
			r"^Started demo/Поправи го најавувањето.* on feature/popravi-go-najavuvanjeto[a-z-]*: it plans first; approve its plan in its pane\.$",
		)
		.unwrap();
		assert!(shape.is_match(&mk.text), "{}", mk.text);
		let zh = server.service.key_start("demo", "修复登录问题").await.unwrap();
		assert!(!zh.is_error);
		assert_eq!(
			zh.text,
			format!(
				"Started demo/修复登录问题 on feature/{}: it plans first; approve its plan in its pane.",
				fallback_slug(now_ms(), 1)
			)
		);
		// omp never comes up in the pane: started, its brief not sent; half done.
		server.herdr.state().omp_up_on_create = None;
		let half = server.service.key_start("demo", "Add a README line.").await.unwrap();
		assert!(half.is_error);
		assert!(
			half.text.ends_with(", but omp didn't come up in its pane within 0 s; the brief wasn't sent."),
			"{}",
			half.text
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn c11_his_finish_keys_merge_that_landed_but_left_its_workspace_is_half_done_text_is_error_true_0581() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	with_server(ServerOptions::default(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		server.herdr.state().close_workspace_fails = Some(HerdrError::new("error", "herdr is gone"));
		let outcome = server.service.key_finish(&started.pane_id, FinishHow::Merge).await.unwrap();
		assert!(outcome.is_error);
		assert!(
			outcome.text.ends_with(
				", but herdr couldn't close its workspace (herdr is gone); its worktree, workspace and branch stay."
			),
			"{}",
			outcome.text
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ux_13_fe_4_the_hud_says_what_awaits_him_and_todays_spend_the_page_gets_it_when_only_the_quota_changed_0581() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let usage = json!({ "reports": [{ "limits": [{ "window": { "id": "5h" }, "amount": { "usedFraction": 0.5 } }] }] })
		.to_string();
	let day = DayOptions {
		pollers: Some(false),
		usage: Some(Arc::new(move || {
			let usage = usage.clone();
			Box::pin(async move { Ok(usage) })
		})),
		..DayOptions::default()
	};
	with_server(ServerOptions { day: Some(day), ..ServerOptions::default() }, |server| async move {
		let page = Page::open(&server, "").await;
		server.service.refresh().await;
		let before = server.service.hud(None).unwrap();
		assert_eq!(before.awaiting, 0);
		server.service.day().read_quota().await;
		server.service.refresh().await;
		page.wait_for(|m| {
			matches!(m, ServerMessage::Hud { hud } if hud.quota.iter().any(|q| q.window == QuotaSpan::FiveHours && q.used == 0.5))
		})
		.await;
		assert_eq!(server.service.hud(None).unwrap().awaiting, before.awaiting);
		page.close();
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn prm_11_a_threads_spend_passing_50_dollars_reaches_the_boss_once_0541() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	with_server(ServerOptions::default(), |server| async move {
		server.herdr.set_state(&boss_pane(&server.herdr).expect("a boss pane"), AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let file = session_file(&started.path, &[Msg::new("Spent a lot.", 51.2)]);
		server.herdr.set_state(&started.pane_id, AgentState::Working, file.to_str(), true);
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		let line = "[event] demo/Readme line — spend passed $50.00 ($51.20); omp warned in its pane";
		until(|| {
			boss_idle(&server);
			let service = server.service.clone();
			tokio::spawn(async move { service.refresh().await });
			boss_lines(&server).iter().any(|l| l == line)
		})
		.await;
		boss_idle(&server);
		server.service.refresh().await;
		assert_eq!(boss_lines(&server).iter().filter(|l| *l == line).count(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn c6_with_the_layer_off_a_thread_pane_gets_jocasta_layer_off_its_prompt_no_done_when_or_when_you_finish_its_brief_the_commit_line_and_no_report_steer_0576()
 {
	let mut j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	j.set_var(LAYER_ENV, Some("off"));
	let hunk = use_hunk_daemon().await;
	// The server's own default for the report steer; the layer off must still send none.
	let mut options = ServerOptions { ask_report: Some(true), ..ServerOptions::default() };
	hunk.apply(&mut options);
	with_server(options, |server| async move {
		server.herdr.state().omp_up_on_create = Some(AgentState::Idle);
		call(
			&server,
			"thread_start",
			&json!({ "project": "demo", "title": "Readme line", "brief": BRIEF, "done_when": ["git diff --quiet"] }),
		)
		.await;
		let pane = server.herdr.state().pane_list.last().cloned().expect("a thread pane");
		let (pane_id, path) = (pane.pane_id, pane.cwd.expect("a thread pane"));
		let env = server.herdr.env_of(&pane_id).expect("its env");
		assert_eq!(env.iter().find(|(name, _)| name == LAYER_ENV).map(|(_, value)| value.as_str()), Some("off"));
		let prompt = fs::read_to_string(task_dir_of(&project.id, "Readme line").join("prompt.md")).unwrap();
		assert!(!prompt.contains("## Done when"));
		assert!(!prompt.contains(WHEN_YOU_FINISH[0]));
		let brief = format!("{BRIEF}\n\n{LESSONS_LINE}\n{COMMIT_LINE}");
		assert!(server.herdr.of("prompt").contains(&vec![json!(pane_id), json!(brief)]));
		// A task's end without a report goes to its review: no steer asks for one.
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let review_start = review("main...feature/readme-line", Some(BRIEF)).split('\n').next().unwrap().to_owned();
		until(|| {
			server.herdr.of("prompt").iter().any(|args| {
				args[0] == pane_id.as_str() && args[1].as_str().is_some_and(|text| text.starts_with(&review_start))
			})
		})
		.await;
		assert!(!server.herdr.of("prompt").iter().any(|args| args[1] == REPORT_STEER));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn c6b_with_omps_host_registry_off_a_thread_whose_omp_reports_over_its_bridge_follows_herdrs_state_and_the_report_is_never_read_0576()
 {
	use crate::frontier::server::bridge::{PaneAgent, PaneEvent};
	use crate::frontier::server::helpers::pane_reports;
	use crate::frontier::server::status::Busy;
	let mut j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	j.set_var(HOSTS_ENV, Some("off"));
	with_server(ServerOptions::default(), |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		// herdr's working, then idle, decide: the brief's turn runs and ends.
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		until(|| server.service.busy_threads().unwrap().working == 1).await;
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| server.service.busy_threads().unwrap().working == 0).await;
		// omp's bridge says a turn runs; it is never read, and herdr's idle stands.
		let working = PaneAgent { working: true, ..PaneAgent::default() };
		pane_reports(&server, &started.pane_id, &PaneEvent::AgentStart, working);
		server.service.refresh().await;
		server.service.refresh().await;
		assert_eq!(server.service.busy_threads().unwrap(), Busy { working: 0, awaiting: 0 });
	})
	.await;
}

fn settings_notes(herdr: &FakeHerdr) -> Vec<String> {
	herdr
		.of("notify")
		.iter()
		.map(|args| args[0]["body"].as_str().unwrap_or_default().to_owned())
		.filter(|body| body.starts_with("settings: "))
		.collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn c7_each_problem_in_jocasta_yaml_is_said_once_as_a_notification_at_the_start_0584() {
	let _j = temp_jocasta();
	write_settings(&config_dir(), &json!({ "service": { "colour": "red" } }));
	with_server(ServerOptions::default(), |server| async move {
		until(|| !settings_notes(&server.herdr).is_empty()).await;
		server.service.refresh().await;
		let said = settings_notes(&server.herdr);
		assert_eq!(said.len(), 1, "{said:?}");
		assert!(
			Regex::new(r"^settings: jocasta\.yaml:\d+: unknown key colour.*; ignored$").unwrap().is_match(&said[0]),
			"{}",
			said[0]
		);
	})
	.await;
}

fn jsonl_results(file: &Path) -> Vec<String> {
	let Ok(text) = fs::read_to_string(file) else { return Vec::new() };
	text.lines()
		.filter_map(|line| serde_json::from_str::<Value>(line).ok())
		.filter(|row| row["body"].as_str().is_some_and(|body| body.starts_with("settings: ")))
		.map(|row| row["result"].as_str().unwrap_or_default().to_owned())
		.collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_notification_herdr_couldnt_show_for_want_of_a_client_is_sent_again_once_a_client_is_attached_once_a_later_one_isnt_0584()
 {
	let _j = temp_jocasta();
	write_settings(&config_dir(), &json!({ "service": { "colour": "red" } }));
	let herdr = Arc::new(FakeHerdr::new());
	// Bare `jocasta` starts the service before his terminal attaches.
	herdr.state().notify_result = NotificationResult::NoForegroundClient;
	with_herdr(&herdr, ServerOptions::default(), |server| async move {
		// Each call's line in notifications.jsonl is written once herdr answered it.
		let file = data_dir().join("notifications.jsonl");
		// The start's own refresh ticks may already have tried it again; each try while no client is kept.
		until(|| !jsonl_results(&file).is_empty()).await;
		let tried = jsonl_results(&file).len();
		until_async(|| {
			let (server, file) = (server.clone(), file.clone());
			async move {
				server.service.refresh().await;
				jsonl_results(&file).len() > tried
			}
		})
		.await;
		server.herdr.state().notify_result = NotificationResult::Shown;
		until_async(|| {
			let (server, file) = (server.clone(), file.clone());
			async move {
				server.service.refresh().await;
				jsonl_results(&file).iter().any(|result| result == "shown")
			}
		})
		.await;
		// Shown: nothing is kept, so later ticks send nothing more.
		server.service.refresh().await;
		server.service.refresh().await;
		let said = jsonl_results(&file);
		assert_eq!(said.last().map(String::as_str), Some("shown"));
		assert!(said[..said.len() - 1].iter().all(|result| result == "no_foreground_client"));
		assert!(said.len() > tried);
		// A client was attached: one herdr can't show later (his terminal closed since) isn't kept for the next.
		let later = Toast::new(ToastReason::Blocked, ToastLevel::Warning, "Ship — waits for you");
		server.herdr.state().notify_result = NotificationResult::NoForegroundClient;
		server.service.notify(&later, None).await;
		server.herdr.state().notify_result = NotificationResult::Shown;
		server.service.refresh().await;
		server.service.refresh().await;
		let once = json!({ "title": "Ship", "body": "waits for you", "sound": "request" });
		assert_eq!(server.herdr.of("notify").iter().filter(|args| args[0] == once).count(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_starts_notifications_wait_10_minutes_for_a_client_then_theyre_dropped_and_later_ones_arent_kept_0584() {
	let _j = temp_jocasta();
	let herdr = Arc::new(FakeHerdr::new());
	herdr.state().notify_result = NotificationResult::NoForegroundClient;
	with_herdr(&herdr, ServerOptions::default(), |server| async move {
		let toast = |text: &str| Toast::new(ToastReason::Event, ToastLevel::Warning, text);
		let sent = |body: &str| {
			let expected = json!({ "title": "Jocasta", "body": body, "sound": "none" });
			server.herdr.of("notify").iter().filter(|args| args[0] == expected).count()
		};
		server.service.notify(&toast("early"), None).await;
		let _clock = stand_still_at(now_ms() + 10 * 60_000 + 1_000);
		// Two ticks: one may find a resend from the service's own tick still running.
		server.service.refresh().await;
		server.service.refresh().await;
		server.service.notify(&toast("late"), None).await;
		let early = sent("early");
		server.herdr.state().notify_result = NotificationResult::Shown;
		server.service.refresh().await;
		server.service.refresh().await;
		assert_eq!(sent("early"), early);
		assert_eq!(sent("late"), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn c10_the_new_thread_form_picks_the_picked_threads_project_else_the_one_of_the_pane_he_last_focused_else_the_last_one_started_in_else_the_first_0581()
 {
	let j = temp_jocasta();
	let mut mains: Vec<(&str, PathBuf)> = Vec::new();
	for name in ["demo", "client-a"] {
		mains.push((name, demo_project(j.root(), name).await.main_path));
	}
	with_server(ServerOptions::default(), |server| async move {
		let form = |pane: Option<&str>| {
			let service = server.service.clone();
			let pane = pane.map(str::to_owned);
			async move { service.new_thread_form(pane.as_deref()).await.unwrap() }
		};
		let all = form(None).await;
		let sorted: BTreeSet<&str> = all.projects.iter().map(String::as_str).collect();
		assert_eq!(sorted.into_iter().collect::<Vec<_>>(), ["client-a", "demo"]);
		let (first, other) = (all.projects[0].clone(), all.projects[1].clone());
		assert_eq!(all.project, first);
		let thread = start_thread(&server, ThreadOptions { project: Some(&other), ..Default::default() }).await;
		assert_eq!(form(None).await.project, other);
		// He focuses a pane in the first project's main checkout.
		let main = mains.iter().find(|(name, _)| *name == first).map(|(_, main)| main.clone()).expect("its main");
		server.herdr.ensure_workspace(&first, &main, None, None).await.unwrap();
		server.herdr.state().pane_list.last_mut().expect("a pane").focused = true;
		assert_eq!(form(None).await.project, first);
		// The thread picked in today wins.
		assert_eq!(form(Some(&thread.pane_id)).await.project, other);
	})
	.await;
}
