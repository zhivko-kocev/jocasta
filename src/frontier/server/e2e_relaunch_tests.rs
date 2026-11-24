//! `test/frontier/server/service-relaunch.test.ts`, case for case, on the real server (wave 12): threads relaunched
//! after herdr restored their panes as shells (0438, 0490), Stop everything (0486, 0547, 0549), `thread_finish` after
//! main moved (0453, 0454), `.worktreeinclude` (0457) and overlapping threads with leftover worktrees (0456).

use std::fs;
use std::sync::{Arc, Mutex};

use regex::Regex;
use serde_json::{Value, json};

use crate::frontier::protocol::Leftover;
use crate::frontier::server::close::CloseOutcome;
use crate::frontier::server::helpers::{
	BRIEF, Msg, TestServer, ThreadOptions, boss_idle, boss_pane, call, commit_in, demo_project, handoff_entry,
	proposed_plan_file, relaunch_idle, review, session_file, sh, start_thread, to_handoff, until,
};
use crate::frontier::server::helpers_procs::{Hooked, with_server_on};
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{AgentState, Herdr};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::tools::ToolReply;
use crate::shared::paths::data_dir;
use crate::shared::poll::wait_for;
use crate::shared::proctree::tests::{settles, sleeper_tree};
use crate::shared::proctree::{pid_alive, processes_in};
use crate::shared::testing::temp_jocasta;

/// A thread pane's omp end waited for briefly, and its leftovers looked for again soon (0490): the sleepers never exit.
/// The pids of the thread's omp tree and of the boss's that run.
type Alive = (Vec<u32>, Vec<u32>);

fn leftovers(options: &mut ServerOptions) {
	options.leftover_wait_ms = Some(500);
	options.leftover_again_ms = Some(200);
}

fn prompts(herdr: &FakeHerdr) -> Vec<(String, String)> {
	herdr
		.of("prompt")
		.into_iter()
		.map(|args| {
			let at = |index: usize| args.get(index).and_then(Value::as_str).unwrap_or_default().to_owned();
			(at(0), at(1))
		})
		.collect()
}

fn workspace_of(herdr: &FakeHerdr, pane_id: &str) -> String {
	let state = herdr.state();
	state.pane_list.iter().find(|p| p.pane_id == pane_id).map(|p| p.workspace_id.clone()).expect("the pane")
}

fn awaiting(server: &TestServer) -> usize {
	server.service.busy_threads().unwrap().awaiting
}

/// The default options of the relaunch cases: omp comes up in 10 s at most.
async fn options() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn r3_a_thread_blocked_on_a_question_relaunched_no_longer_awaits_him_continue_doesnt_resume_the_turn_that_asked_0438()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	with_server_on(herdr.clone(), options().await, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let asked = session_file(&started.path, &[Msg::new("Which format?", 0.1).ask("CSV or JSON?")]);
		herdr.set_state(&started.pane_id, AgentState::Working, asked.to_str(), true);
		herdr.set_state(&started.pane_id, AgentState::Blocked, None, true);
		until(|| awaiting(&server) == 1).await;
		relaunch_idle(&server, &herdr, &started.path, asked.to_str()).await;
		until(|| awaiting(&server) == 0).await;
		let busy = server.service.busy_threads().unwrap();
		assert_eq!((busy.working, busy.awaiting), (0, 0));
		assert_eq!(server.service.hud(None).unwrap().awaiting, 0);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn r3_a_thread_whose_plan_awaits_him_relaunched_still_awaits_him_0438_0494() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	with_server_on(herdr.clone(), options().await, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let file = proposed_plan_file(&started.path);
		herdr.set_state(&started.pane_id, AgentState::Working, file.to_str(), true);
		herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| awaiting(&server) == 1).await;
		relaunch_idle(&server, &herdr, &started.path, file.to_str()).await;
		let busy = server.service.busy_threads().unwrap();
		assert_eq!((busy.working, busy.awaiting), (0, 1));
		assert_eq!(server.service.hud(None).unwrap().awaiting, 1);
		assert_eq!(
			call(&server, "thread_status", &json!({})).await.text,
			"demo/Readme line [feature/readme-line] plan ready\n  spend $0.25"
		);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_pane_herdr_restored_as_a_shell_its_old_omp_still_running_and_what_it_left_end_before_omp_starts_again_a_process_something_else_started_in_its_worktree_keeps_running_0490()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeHerdr::new());
	let hooked = Hooked::new(&herdr);
	let mut options = options().await;
	leftovers(&mut options);
	options.herdr = Some(hooked.clone() as Arc<dyn Herdr>);
	with_server_on(herdr.clone(), options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// The old omp with its workers (real processes that outlive it), working in the worktree.
		let tree = sleeper_tree(&path.join("pane"), true).await;
		// He started it in the worktree from another terminal: a real process that stays up until killed.
		let other = sleeper_tree(&path, false).await;
		herdr.state().roots.insert(pane_id.clone(), tree.root);
		// The refresh tick notes the pane's terminal and reads its omp.
		server.service.refresh().await;
		herdr.restore();
		let alive_at_respawn: Arc<Mutex<Option<Vec<u32>>>> = Arc::new(Mutex::new(None));
		let (watched, all, seen) = (pane_id.clone(), tree.all.clone(), Arc::clone(&alive_at_respawn));
		hooked.on_respawn(move |id| {
			if id == watched {
				*seen.lock().unwrap() = Some(all.iter().copied().filter(|pid| pid_alive(*pid)).collect());
			}
		});
		server.service.refresh().await;
		assert_eq!(*alive_at_respawn.lock().unwrap(), Some(Vec::<u32>::new()));
		assert!(herdr.of("respawn").iter().any(|args| args[0] == json!(pane_id)));
		assert!(pid_alive(other.root));
		assert!(call(&server, "thread_status", &json!({})).await.text.contains("demo/Readme line"));
		drop(tree);
		drop(other);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_everything_ends_what_each_thread_panes_remembered_omp_left_also_when_herdrs_server_didnt_a_process_something_else_started_in_its_worktree_keeps_running_0486_0490()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let mut options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	leftovers(&mut options);
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// The omp with its workers (real processes that outlive it), from a herdr server that no longer has them.
		let tree = sleeper_tree(&path.join("pane"), true).await;
		// He started it in the worktree from another terminal: a real process that stays up until killed.
		let other = sleeper_tree(&path, false).await;
		server.herdr.state().roots.insert(pane_id, tree.root);
		server.service.refresh().await;
		// This fake herdr's stop ends no process.
		assert_eq!(server.shutdown(false).await, CloseOutcome::Stopped);
		assert_eq!(tree.all.iter().copied().filter(|pid| pid_alive(*pid)).collect::<Vec<_>>(), Vec::<u32>::new());
		assert!(pid_alive(other.root));
		drop(tree);
		drop(other);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_everything_a_thread_in_its_review_turn_is_saved_for_restore_and_its_omp_ends_forced_before_herdrs_server_stops_the_bosss_ends_with_herdr_0549()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	let herdr = Arc::new(FakeHerdr::new());
	let hooked = Hooked::new(&herdr);
	let mut options = options().await;
	leftovers(&mut options);
	options.herdr = Some(hooked.clone() as Arc<dyn Herdr>);
	let root = j.root().to_path_buf();
	with_server_on(herdr.clone(), options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// The thread's and the boss's omps: real processes that outlive their roots, as omp's workers do.
		let omp = sleeper_tree(&root.join("thread-omp"), true).await;
		let boss = sleeper_tree(&root.join("boss-omp"), true).await;
		let boss_id = boss_pane(&herdr).expect("the boss pane");
		herdr.state().roots.insert(pane_id.clone(), omp.root);
		herdr.state().roots.insert(boss_id, boss.root);
		server.service.refresh().await;
		// Its task ends; its review turn runs.
		herdr.set_state(&pane_id, AgentState::Working, None, true);
		commit_in(&path, "demo\nA line.\n", "Add a README line");
		herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let asked = review("main...feature/readme-line", Some(BRIEF));
		until(|| prompts(&herdr).iter().any(|(p, t)| *p == pane_id && *t == asked)).await;
		let file = session_file(&path, &[Msg::new("Reviewing.", 0.1)]);
		herdr.set_state(&pane_id, AgentState::Working, file.to_str(), true);
		until(|| server.service.busy_threads().unwrap().working == 1).await;
		// What runs as herdr's server stops (this fake's stop ends no process).
		let at_stop: Arc<Mutex<Option<Alive>>> = Arc::new(Mutex::new(None));
		let (thread_pids, boss_pids, seen) = (omp.all.clone(), boss.all.clone(), Arc::clone(&at_stop));
		hooked.on_stop_server(move || {
			let alive = |pids: &[u32]| pids.iter().copied().filter(|pid| pid_alive(*pid)).collect::<Vec<u32>>();
			*seen.lock().unwrap() = Some((alive(&thread_pids), alive(&boss_pids)));
		});
		assert_eq!(server.shutdown(false).await, CloseOutcome::Stopped);
		assert_eq!(*at_stop.lock().unwrap(), Some((Vec::<u32>::new(), boss.all.clone())));
		let panes: Value = serde_json::from_str(&fs::read_to_string(data_dir().join("panes.json")).unwrap()).unwrap();
		assert_eq!(panes[&pane_id]["restore"], json!(file.display().to_string()));
		drop(omp);
		drop(boss);
		// Nothing it started still works in the temp folder, so the folder can go (Windows' EBUSY).
		settles(|| processes_in(&root).is_empty()).await;
	})
	.await;
}

/// `thread_finish` merge, asked again until the thread's handoff entry is seen (a read under a second ago is reused).
async fn finish(server: &TestServer) -> ToolReply {
	let reply = Arc::new(Mutex::new(ToolReply { text: String::new(), is_error: None }));
	let held = wait_for(
		|| {
			let reply = Arc::clone(&reply);
			async move {
				let got = call(server, "thread_finish", &json!({"thread": "Readme line", "how": "merge"})).await;
				let past = !got.text.contains("still writing its handoff");
				*reply.lock().unwrap() = got;
				Ok(past)
			}
		},
		15_000,
		100,
	)
	.await
	.unwrap();
	assert!(held, "thread_finish past the handoff");
	reply.lock().unwrap().clone()
}

fn count_prompts(server: &TestServer, pane: &str, text: &str) -> usize {
	prompts(&server.herdr).iter().filter(|(p, t)| p == pane && t == text).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_finish_merge_after_main_moved_main_is_merged_into_the_branch_tested_and_reviewed_again_and_merged_only_when_that_passes_0453_0454()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let main = project.main_path.clone();
	sh(&main, &["config", "user.name", "Test"]);
	sh(&main, &["config", "user.email", "test@example.com"]);
	let herdr = Arc::new(FakeHerdr::new());
	with_server_on(herdr.clone(), options().await, |server| async move {
		let boss = boss_pane(&herdr).expect("the boss pane");
		herdr.set_state(&boss, AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		handoff_entry(&to_handoff(&server, &pane_id, &path).await);
		let range = "main...feature/readme-line";
		let asked = review(range, Some(BRIEF));
		// Another thread's work landed on main meanwhile.
		fs::write(main.join("other.txt"), "other\n").unwrap();
		sh(&main, &["add", "other.txt"]);
		sh(&main, &["commit", "-qm", "Other work"]);
		assert_eq!(
			finish(&server).await.text,
			"main moved since demo/Readme line's review: it was merged into feature/readme-line, which is being tested and reviewed again; it merges into main only when that passes, and you hear either way."
		);
		assert!(path.join("other.txt").exists());
		until(|| count_prompts(&server, &pane_id, &asked) == 2).await;
		assert_eq!(sh(&main, &["log", "-1", "--format=%s"]), "Other work");

		// The new review's tests fail by their run: nothing is merged, he hears it with why, in one line (P3).
		let failed = session_file(
			&path,
			&[Msg::new("Reviewed.\nTESTS: pass — bun test", 0.0).bash("bun test", false)],
		);
		herdr.set_state(&pane_id, AgentState::Working, failed.to_str(), true);
		herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let not_merged = Regex::new(
			r"(?m)^\[event\] demo/Readme line — done and reviewed: main merged in, .*; not merged: its new review after main moved didn't pass \(tests fail\)$",
		)
		.unwrap();
		until(|| {
			boss_idle(&server);
			prompts(&herdr).iter().any(|(p, t)| *p == boss && not_merged.is_match(t))
		})
		.await;
		assert_eq!(sh(&main, &["log", "-1", "--format=%s"]), "Other work");
		assert!(path.exists());

		// main moves again; this time the new review passes: the squash follows, as an event.
		handoff_entry(&failed);
		server.service.refresh().await;
		fs::write(main.join("third.txt"), "third\n").unwrap();
		sh(&main, &["add", "third.txt"]);
		sh(&main, &["commit", "-qm", "Third work"]);
		assert!(finish(&server).await.text.starts_with("main moved since demo/Readme line's review"));
		until(|| count_prompts(&server, &pane_id, &asked) == 3).await;
		let passed =
			session_file(&path, &[Msg::new("Reviewed.\nTESTS: pass — bun test", 0.0).bash("bun test", true)]);
		herdr.set_state(&pane_id, AgentState::Working, passed.to_str(), true);
		herdr.set_state(&pane_id, AgentState::Idle, None, true);
		// One event for the merge: the review's outcome and the merge together, no separate "done and reviewed" (P3).
		let merged = Regex::new(
			r"(?m)^\[event\] demo/Readme line — its new review passed \(main merged in, .*tests pass, no report\); merged demo/Readme line into main as [0-9a-f]+",
		)
		.unwrap();
		until(|| {
			boss_idle(&server);
			prompts(&herdr).iter().any(|(p, t)| *p == boss && merged.is_match(t))
		})
		.await;
		let reviewed = prompts(&herdr)
			.into_iter()
			.filter(|(p, _)| *p == boss)
			.flat_map(|(_, t)| t.split('\n').map(str::to_owned).collect::<Vec<_>>())
			.filter(|line| line.contains("done and reviewed: main merged in"))
			.count();
		assert_eq!(reviewed, 1);
		assert_eq!(sh(&main, &["log", "-1", "--format=%s"]), "Readme line");
		assert_eq!(fs::read_to_string(main.join("README.md")).unwrap().replace("\r\n", "\n"), "demo\nA line.\n");
		assert!(!path.exists());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_start_copies_the_worktreeinclude_files_that_are_gitignored_into_the_new_worktree_0457() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let main = project.main_path.clone();
	fs::write(main.join(".gitignore"), ".env\nlocal/\n").unwrap();
	fs::write(main.join(".worktreeinclude"), ".env\nlocal/\nnotes.txt\n").unwrap();
	sh(&main, &["add", ".gitignore", ".worktreeinclude"]);
	sh(&main, &["commit", "-qm", "ignore"]);
	fs::write(main.join(".env"), "SECRET=1\n").unwrap();
	fs::create_dir(main.join("local")).unwrap();
	fs::write(main.join("local").join("settings.json"), "{}\n").unwrap();
	// Matched but not gitignored: never copied.
	fs::write(main.join("notes.txt"), "mine\n").unwrap();
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		assert_eq!(fs::read_to_string(started.path.join(".env")).unwrap(), "SECRET=1\n");
		assert_eq!(fs::read_to_string(started.path.join("local").join("settings.json")).unwrap(), "{}\n");
		assert!(!started.path.join("notes.txt").exists());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_status_names_open_threads_touching_the_same_files_projects_lists_leftover_worktrees_removed_only_by_worktree_remove_and_never_when_dirty_0456()
 {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	let root = j.root().to_path_buf();
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	crate::frontier::server::helpers::with_server(options, |server| async move {
		let first = start_thread(&server, ThreadOptions::default()).await;
		let second = start_thread(
			&server,
			ThreadOptions { title: Some("Second"), brief: Some("More."), ..ThreadOptions::default() },
		)
		.await;
		commit_in(&first.path, "demo\nOne.\n", "One");
		fs::write(second.path.join("README.md"), "demo\nTwo.\n").unwrap();
		let status = call(&server, "thread_status", &json!({})).await.text;
		assert!(
			status.contains("demo/Readme line [feature/readme-line] idle · overlaps Second: README.md"),
			"{status}"
		);
		assert!(status.contains("demo/Second [feature/second] idle · overlaps Readme line: README.md"), "{status}");

		// The second thread is closed in herdr: its worktree is left on disk, with its change.
		let workspace = workspace_of(&server.herdr, &second.pane_id);
		server.herdr.close_workspace(&workspace).await.unwrap();
		server.service.refresh().await;
		let leftover = root.join("demo.wt").join("second");
		let shown = call(&server, "projects", &json!({})).await.text;
		assert!(
			shown.contains(&format!("  left on disk: {} [feature/second] 0 ahead, 1 changed", leftover.display())),
			"{shown}"
		);
		assert_eq!(
			server.service.leftovers().await.unwrap(),
			vec![Leftover {
				project: "demo".to_owned(),
				folder: leftover.clone(),
				branch: Some("feature/second".to_owned()),
				ahead: 0,
				dirty: 1,
			}]
		);
		let dirty = call(&server, "worktree_remove", &json!({"path": leftover.display().to_string()})).await;
		assert_eq!(
			dirty,
			ToolReply {
				text: format!("{} has 1 uncommitted change; nothing removed.", leftover.display()),
				is_error: Some(true)
			}
		);
		let held = call(&server, "worktree_remove", &json!({"path": first.path.display().to_string()})).await;
		assert_eq!(
			held,
			ToolReply {
				text: format!(
					"{} isn't a leftover worktree (a thread folder no open thread holds); nothing removed.",
					first.path.display()
				),
				is_error: Some(true)
			}
		);
		sh(&leftover, &["checkout", "--", "README.md"]);
		assert_eq!(
			call(&server, "worktree_remove", &json!({"path": leftover.display().to_string()})).await.text,
			format!("Removed {}; branch feature/second stays (0 ahead).", leftover.display())
		);
		assert!(!leftover.exists());
		assert!(sh(&project.main_path, &["branch", "--list", "feature/second"]).contains("feature/second"));
	})
	.await;
}
