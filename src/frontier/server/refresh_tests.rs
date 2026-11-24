//! The refresh tick's cases, at part level on `context::test_context`: a real git project `demo` with a linked
//! worktree `readme-line`, herdr's workspaces and panes in a `FakeHerdr`, the boss started through `ensure_boss`.
//!
//! Waits for service (they need the HTTP server or the whole service): `service-regressions` "C6b" (the `hosts` gate is
//! `service.ts`'s start of the host poll), "SVC-2" as a timing race between a start and a tick's prompt (its two
//! halves are pinned below on `closed_threads` and on `known`), `service-plans` "a done and reviewed thread survives
//! Stop everything and reopen" (HUD and close question), and every case that starts its thread with `thread_start`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;
use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::bridge::PaneAgent;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::herdr::AgentState::{Idle, Unknown, Working};
use crate::frontier::server::herdr::WorkspaceInfo;
use crate::frontier::server::service::ServiceOptions;
use crate::shared::git::git;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::testing::{TempDir, temp_dir};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

struct World {
	t: TestContext,
	r: Refresher,
	main: PathBuf,
	wt: PathBuf,
	_dir: TempDir,
}

/// A pane Jocasta started: its workspace, its pane (a shell until omp reports) and the launch record.
fn add_pane(t: &TestContext, (pane_id, workspace_id, label): (&str, &str, &str), cwd: &Path) {
	let mut herdr = t.herdr.state();
	herdr.workspace_list.push(WorkspaceInfo {
		id: workspace_id.to_owned(),
		label: label.to_owned(),
		cwd: Some(cwd.to_path_buf()),
	});
	herdr.pane_list.push(PaneInfo {
		pane_id: pane_id.to_owned(),
		tab_id: format!("t-{pane_id}"),
		workspace_id: workspace_id.to_owned(),
		cwd: Some(cwd.to_path_buf()),
		agent_state: Unknown,
		terminal_id: Some(format!("term-{pane_id}")),
		label: Some("shell".to_owned()),
		..PaneInfo::default()
	});
	drop(herdr);
	t.ctx.launched.add(pane_id, &cwd.display().to_string(), Some(&format!("key-{pane_id}"))).unwrap();
}

async fn world() -> World {
	world_with(|_| {}).await
}

async fn world_with(more: impl FnOnce(&mut ServiceOptions)) -> World {
	let t = test_context(|options| {
		options.mise = Some(None);
		more(options);
	});
	let dir = temp_dir("refresh");
	let base = real_path(dir.path());
	let main = base.join("code").join("demo");
	fs::create_dir_all(&main).unwrap();
	sh(&main, &["init", "-q", "-b", "main"]);
	fs::write(main.join("README.md"), "demo\n").unwrap();
	sh(&main, &["add", "."]);
	sh(&main, &["commit", "-q", "-m", "init demo"]);
	let wt = base.join("wt").join("readme-line");
	fs::create_dir_all(wt.parent().unwrap()).unwrap();
	sh(&main, &["worktree", "add", "-q", "-b", "feature/readme-line", &wt.display().to_string()]);
	let wt = real_path(&wt);
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let roots = format!(
		"version: 1\nroots:\n  - path: {}\n    depth: 2\n",
		base.join("code").display().to_string().replace('\\', "/")
	);
	fs::write(file, roots).unwrap();
	t.ctx.places.sync().await;
	t.ctx.panes.ensure_boss(true).await.unwrap();
	add_pane(&t, ("p-demo", "w-demo", "demo"), &main);
	add_pane(&t, ("p-thread", "w-thread", "Readme line"), &wt);
	let r = Refresher::new(t.ctx.clone());
	World { t, r, main, wt, _dir: dir }
}

impl World {
	fn boss(&self) -> String {
		self.t.ctx.boss().unwrap().with(|boss| boss.pane_id.clone())
	}

	async fn tick(&self) {
		self.r.refresh().await;
		self.t.ctx.calls.settled_within(2_000).await;
	}

	/// omp is up in every pane, idle (no event pushed).
	fn up(&self) {
		for id in [self.boss(), "p-demo".to_owned(), "p-thread".to_owned()] {
			self.t.herdr.set_state(&id, Idle, None, false);
		}
	}

	/// omp ended in every pane, which stays open.
	fn ended(&self, panes: &[String]) {
		for id in panes {
			self.t.herdr.set_state(id, Unknown, None, false);
		}
	}

	fn respawned(&self) -> Vec<(String, Value)> {
		self.t
			.herdr
			.of("respawn")
			.into_iter()
			.map(|args| (args[0].as_str().unwrap().to_owned(), args[1].clone()))
			.collect()
	}

	fn thread_pane(&self) -> String {
		self.t.ctx.threads.all().first().unwrap().with(|t| t.pane_id.clone())
	}

	fn boss_lines(&self) -> Vec<String> {
		let boss = self.boss();
		let sent = self.t.herdr.of("prompt");
		let to_boss = sent.iter().filter(|args| args[0] == boss.as_str());
		to_boss.flat_map(|args| args[1].as_str().unwrap().lines().map(str::to_owned).collect::<Vec<_>>()).collect()
	}

	async fn snapshot(&self) -> HerdrSnapshot {
		self.t.ctx.herdr.snapshot().await.unwrap()
	}

	fn thread_count(&self) -> usize {
		self.t.ctx.threads.all().len()
	}

	async fn close(&self, workspace_id: &str) {
		self.t.ctx.herdr.close_workspace(workspace_id).await.unwrap();
	}
}

fn command_of(call: &Value) -> Vec<String> {
	call["command"].as_array().unwrap().iter().map(|word| word.as_str().unwrap().to_owned()).collect()
}

// ── service-handoff.test.ts ─────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn panes_herdr_restored_as_shells_while_the_service_runs_get_omp_again_with_their_flags_0431() {
	let w = world().await;
	w.up();
	// The refresh tick notes each new pane's terminal and finds the thread.
	w.tick().await;
	assert_eq!(w.thread_count(), 1);
	w.t.herdr.restore();
	w.tick().await;
	let respawned = w.respawned();
	assert_eq!(respawned.len(), 3);
	let boss = command_of(&respawned[0].1);
	assert_eq!(boss[..3], ["omp", "--continue", "--config"]);
	assert!(boss[3].ends_with("boss.yml"));
	assert!(respawned[0].1["cwd"].as_str().unwrap().ends_with("home"));
	let (thread, project) = (&respawned[1].1, &respawned[2].1);
	assert!(same_dir(Path::new(thread["cwd"].as_str().unwrap()), &w.wt));
	assert!(same_dir(Path::new(project["cwd"].as_str().unwrap()), &w.main));
	let overlay =
		|command: &[String]| command.iter().position(|word| word == "--config").map(|at| command[at + 1].clone());
	assert!(overlay(&command_of(thread)).unwrap().ends_with(".yml"));
	assert!(overlay(&command_of(project)).unwrap().ends_with(".yml"));
	// Every pane gets the bridge extension (0675).
	assert!(command_of(project).contains(&"-e".to_owned()));
	assert!(command_of(thread).contains(&"-e".to_owned()));
	// The thread is known by its new pane.
	let fresh =
		w.t.herdr
			.state()
			.pane_list
			.iter()
			.find(|p| p.cwd.as_deref().is_some_and(|c| same_dir(c, &w.wt)))
			.unwrap()
			.pane_id
			.clone();
	assert!(w.t.ctx.threads.get(&fresh).is_some());
	assert_eq!(w.thread_pane(), fresh);
}

#[tokio::test]
async fn an_omp_that_ends_in_a_jocasta_pane_left_open_starts_again_there_once_per_ending_0521() {
	let w = world().await;
	w.up();
	// The refresh tick sees omp running in each pane's terminal.
	w.tick().await;
	let thread = w.thread_pane();
	let (boss, project) = (w.boss(), "p-demo".to_owned());
	w.ended(&[boss.clone(), thread.clone(), project.clone()]);
	w.tick().await;
	let ids: Vec<String> = w.respawned().into_iter().map(|(id, _)| id).collect();
	assert_eq!(ids, [boss.clone(), thread.clone(), project]);
	let first = command_of(&w.respawned()[0].1);
	assert_eq!(first[..3], ["omp", "--continue", "--config"]);
	assert!(first[3].ends_with("boss.yml"));
	// The new omps are still starting: one ending started each once.
	w.tick().await;
	w.tick().await;
	assert_eq!(w.respawned().len(), 3);
	// The thread goes on in its new pane.
	assert_eq!(w.thread_count(), 1);
	assert_ne!(w.thread_pane(), thread);
}

#[tokio::test]
async fn a_jocasta_pane_whose_omp_has_not_been_seen_running_is_left_alone_0521() {
	let w = world().await;
	w.tick().await;
	w.tick().await;
	assert_eq!(w.t.herdr.state().pane_list.iter().find(|p| p.pane_id == "p-demo").unwrap().agent_state, Unknown);
	assert!(w.respawned().is_empty());
}

#[tokio::test]
async fn a_thread_being_finished_its_omp_ending_as_its_workspace_closes_is_not_started_again_0521() {
	let w = world().await;
	w.up();
	w.tick().await;
	let thread = w.thread_pane();
	w.t.ctx.threads.drop_pane(&thread);
	w.t.ctx.set_finishing("w-thread", Finishing::Removing);
	w.ended(std::slice::from_ref(&thread));
	w.tick().await;
	assert!(w.respawned().is_empty());
	w.close("w-thread").await;
	w.t.ctx.set_finishing("w-thread", Finishing::Removed);
	w.tick().await;
	assert!(w.respawned().is_empty());
}

#[tokio::test]
async fn an_omp_that_ends_once_the_service_stopped_is_not_started_again_0521() {
	let w = world().await;
	w.up();
	w.tick().await;
	let thread = w.thread_pane();
	w.t.ctx.set_stopping();
	w.ended(&[thread, w.boss()]);
	w.tick().await;
	assert!(w.respawned().is_empty());
}

#[tokio::test]
async fn an_omp_that_keeps_ending_in_its_pane_starts_again_after_a_wait_said_once_0521() {
	// The wait's own times (10 s, 20 s, a 10-minute run) are `OmpBackoff`'s tests; here the tick keeps to them.
	let w = world().await;
	let ends = || async {
		let boss = w.boss();
		w.t.herdr.set_state(&boss, Idle, None, false);
		w.tick().await;
		w.t.herdr.set_state(&w.boss(), Unknown, None, false);
		w.tick().await;
	};
	ends().await;
	assert_eq!(w.respawned().len(), 1);
	// It ended again: it waits 10 s.
	ends().await;
	w.tick().await;
	assert_eq!(w.respawned().len(), 1);
	assert_eq!(w.thread_count(), 1);
}

#[tokio::test]
async fn r3_a_working_thread_whose_pane_herdr_restored_as_a_shell_is_relaunched_and_comes_up_idle_without_an_event() {
	let w = world().await;
	w.up();
	w.tick().await;
	let thread = w.thread_pane();
	w.t.herdr.set_state(&thread, Working, None, false);
	w.tick().await;
	assert_eq!(w.t.ctx.status.busy_threads().unwrap().working, 1);
	w.t.herdr.restore();
	w.tick().await;
	let fresh = w.thread_pane();
	assert_ne!(fresh, thread);
	// The new omp sits idle; herdr pushes no change for it, its pane list says idle.
	w.t.herdr.set_state(&fresh, Idle, None, false);
	w.tick().await;
	assert_eq!(w.t.ctx.status.busy_threads().unwrap().working, 0);
}

// ── service-regressions.test.ts ─────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn svc_2_a_thread_registered_after_the_list_was_read_is_not_dropped_nor_said_closed() {
	let w = world().await;
	w.up();
	w.tick().await;
	let known_before = w.t.ctx.threads.entries();
	// B starts while the tick runs: it is in the registry, not in herdr's list, and not among the threads known before.
	let late = w.t.with_thread("p-late", "Task B");
	let tick = Tick {
		read_at: 0,
		snapshot: w.snapshot().await,
		by_id: w.snapshot().await.panes.into_iter().map(|p| (p.pane_id.clone(), p)).collect(),
		known: known_before,
	};
	w.r.inner.closed_threads(&tick).await.unwrap();
	assert!(w.t.ctx.threads.get("p-late").is_some_and(|found| found.same(&late)));
	assert!(w.boss_lines().iter().all(|line| !line.contains("closed;")));
}

#[tokio::test]
async fn a_thread_dropped_or_replaced_while_the_list_was_read_is_not_this_lists_to_judge() {
	let w = world().await;
	let old = w.t.with_thread("p-x", "Task A");
	let tick = Tick {
		read_at: 0,
		snapshot: HerdrSnapshot::default(),
		by_id: HashMap::new(),
		known: vec![("p-x".to_owned(), old.clone())],
	};
	// Replaced by a relaunch's thread of the same pane: the old one isn't judged, the new one stays.
	w.t.ctx.threads.drop_pane("p-x");
	let new = w.t.with_thread("p-x", "Task A");
	w.r.inner.closed_threads(&tick).await.unwrap();
	assert!(w.t.ctx.threads.get("p-x").is_some_and(|found| found.same(&new)));
	// Dropped: nothing to drop again.
	w.t.ctx.threads.drop_pane("p-x");
	w.r.inner.closed_threads(&tick).await.unwrap();
	assert_eq!(w.thread_count(), 0);
}

// ── service-tools.test.ts ───────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_thread_closed_in_herdr_the_boss_hears_it_once_and_its_worktree_and_branch_stay_0405() {
	let w = world().await;
	w.up();
	w.tick().await;
	assert_eq!(w.thread_count(), 1);
	w.close("w-thread").await;
	w.tick().await;
	w.tick().await;
	let line = "[event] demo/Readme line — closed; its worktree and branch stay";
	assert_eq!(w.boss_lines().iter().filter(|l| *l == line).count(), 1);
	assert!(w.wt.exists());
	assert!(sh(&w.main, &["branch", "--list", "feature/readme-line"]).contains("feature/readme-line"));
	assert_eq!(w.thread_count(), 0);
}

// ── finishing (service-finish's "a refresh meanwhile") ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_thread_whose_workspace_is_being_finished_is_dropped_but_never_said_closed() {
	let w = world().await;
	w.up();
	w.tick().await;
	w.t.ctx.set_finishing("w-thread", Finishing::Removing);
	w.close("w-thread").await;
	w.tick().await;
	assert_eq!(w.thread_count(), 0);
	assert!(w.boss_lines().iter().all(|line| !line.contains("closed;")));
	// A removed workspace's mark goes once herdr no longer lists it; one still being removed stays.
	w.t.ctx.set_finishing("w-gone", Finishing::Removed);
	w.t.ctx.set_finishing("w-demo", Finishing::Removed);
	w.tick().await;
	assert_eq!(w.t.ctx.finishing("w-gone"), None);
	assert_eq!(w.t.ctx.finishing("w-demo"), Some(Finishing::Removed));
	assert_eq!(w.t.ctx.finishing("w-thread"), Some(Finishing::Removing));
}

// ── discover ────────────────────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn discover_finds_a_pane_in_a_linked_worktree_that_runs_omp() {
	let w = world().await;
	w.up();
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 1);
	let t = w.t.ctx.threads.get("p-thread").unwrap().get();
	assert_eq!(
		(t.title.as_str(), t.branch.as_str(), t.workspace_id.as_str()),
		("Readme line", "feature/readme-line", "w-thread")
	);
	assert_eq!((t.state, t.shown, t.phase, t.catch_up), (Idle, ThreadState::Idle, Phase::Task, true));
	assert!(same_dir(&t.path, &w.wt));
	assert_eq!(t.head.as_deref(), Some(sh(&w.wt, &["rev-parse", "HEAD"]).as_str()));
	assert_eq!(t.plan, Some(ThreadPlan { reopened: true, ..ThreadPlan::default() }));
	// Not the boss, not the project's own pane.
	assert!(w.t.ctx.threads.get("p-demo").is_none());
}

#[tokio::test]
async fn discover_takes_a_pane_jocasta_started_before_omp_reports_but_never_a_plain_shell() {
	let w = world().await;
	// Started by Jocasta, no agent yet: found.
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 1);
	// A shell he opened in the worktree himself, in another workspace: not found.
	w.t.ctx.threads.drop_pane("p-thread");
	w.t.herdr.state().pane_list.retain(|p| p.pane_id != "p-thread");
	let shell = PaneInfo {
		pane_id: "p-shell".to_owned(),
		workspace_id: "w-other".to_owned(),
		cwd: Some(w.wt.clone()),
		..PaneInfo::default()
	};
	w.t.herdr.state().pane_list.push(shell);
	w.r.rediscover();
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 0);
}

#[tokio::test]
async fn discover_skips_a_workspace_with_a_thread_or_being_finished_and_looks_again_only_when_asked() {
	let w = world().await;
	w.up();
	// A thread already holds the workspace.
	let held = w.t.with_thread("p-held", "Held");
	held.with(|t| t.workspace_id = "w-thread".to_owned());
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert!(w.t.ctx.threads.get("p-thread").is_none());
	// Being finished.
	w.t.ctx.threads.drop_pane("p-held");
	w.t.ctx.set_finishing("w-thread", Finishing::Removing);
	w.r.rediscover();
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 0);
	w.t.ctx.clear_finishing("w-thread");
	// The pane set is unchanged since the last look: not looked at again until asked (CR-12).
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 0);
	w.t.ctx.rediscover.store(true, Ordering::SeqCst);
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.thread_count(), 1);
	assert!(!w.t.ctx.rediscover.load(Ordering::SeqCst));
}

#[tokio::test]
async fn discover_shows_a_working_pane_as_working_and_a_blocked_one_as_blocked() {
	let w = world().await;
	w.t.herdr.set_state("p-thread", Working, None, false);
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.t.ctx.threads.get("p-thread").unwrap().with(|t| t.shown), ThreadState::Working);
	w.t.ctx.threads.drop_pane("p-thread");
	w.t.herdr.set_state("p-thread", crate::frontier::server::herdr::AgentState::Blocked, None, false);
	w.r.rediscover();
	w.r.discover(&w.snapshot().await).await.unwrap();
	assert_eq!(w.t.ctx.threads.get("p-thread").unwrap().with(|t| t.shown), ThreadState::Blocked);
}

// ── the tick ────────────────────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn one_tick_runs_at_a_time_and_every_caller_gets_the_running_one() {
	let w = world().await;
	assert!(w.r.running().is_none());
	let (a, b) = (w.r.refresh(), w.r.refresh());
	assert!(w.r.running().is_some());
	a.await;
	b.await;
	assert!(w.r.running().is_none());
	// The tick ran though the first caller did not wait for it.
	drop(w.r.refresh());
	w.t.ctx.calls.settled_within(1_000).await;
	while w.r.running().is_some() {
		tokio::task::yield_now().await;
	}
	assert_eq!(w.thread_count(), 1);
}

#[tokio::test]
async fn a_facts_refresh_drops_a_closed_thread_but_catches_up_nothing_and_a_full_one_asked_meanwhile_follows_it() {
	let w = world().await;
	w.up();
	w.r.discover(&w.snapshot().await).await.unwrap();
	let thread = w.t.ctx.threads.get("p-thread").unwrap();
	assert!(thread.with(|t| t.catch_up));
	w.r.facts().await;
	assert!(thread.with(|t| t.catch_up), "plans and catch-ups wait for the full refresh");
	// A full refresh asked while a facts one runs comes after it; a facts one asked then gets the full one.
	let facts = w.r.facts();
	let full = w.r.refresh();
	let again = w.r.facts();
	full.await;
	assert!(w.r.running().is_none(), "the full refresh ended last");
	facts.await;
	again.await;
	w.t.ctx.calls.settled_within(2_000).await;
	assert!(!thread.with(|t| t.catch_up));
	w.close("w-thread").await;
	w.r.facts().await;
	assert_eq!(w.thread_count(), 0);
}

#[tokio::test]
async fn a_step_that_fails_is_said_once_per_reason_and_the_rest_still_run() {
	let w = world().await;
	let inner = &w.r.inner;
	let fail = |text: &'static str| inner.step::<(), _>("plans", async move { Err(Error::other(text)) });
	assert_eq!(fail("one").await, None);
	assert_eq!(inner.step_warned.lock().unwrap().get("plans").map(String::as_str), Some("one"));
	assert_eq!(fail("one").await, None);
	assert_eq!(fail("two").await, None);
	assert_eq!(inner.step_warned.lock().unwrap().get("plans").map(String::as_str), Some("two"));
	// A step that works again forgets its reason.
	assert_eq!(inner.step("plans", async { Ok::<_, Error>(7) }).await, Some(7));
	assert!(inner.step_warned.lock().unwrap().is_empty());
	// And a tick runs on past a thread whose pane herdr no longer lists, step after step.
	w.t.with_thread("p-gone", "Gone");
	w.tick().await;
	assert!(w.t.ctx.threads.get("p-gone").is_none());
	assert!(w.t.sent.lock().unwrap().iter().any(|m| matches!(m, ServerMessage::Hud { .. })));
}

#[tokio::test]
async fn the_tick_notes_the_workspaces_and_reads_each_thread_panes_omp_once_per_terminal() {
	let w = world().await;
	w.up();
	w.tick().await;
	assert_eq!(w.t.ctx.status.workspaces().len(), w.snapshot().await.workspaces.len());
	let thread = w.t.ctx.threads.get("p-thread").unwrap();
	let terminal = thread.with(|t| t.omp.as_ref().map(|omp| omp.terminal.clone()));
	assert_eq!(terminal, Some(Some("term-p-thread".to_owned())));
	// herdr started a process in the pane again: its omp is read again for the new terminal.
	w.t.herdr.state().pane_list.iter_mut().find(|p| p.pane_id == "p-thread").unwrap().terminal_id =
		Some("term-new".to_owned());
	w.tick().await;
	assert_eq!(thread.with(|t| t.omp.as_ref().map(|omp| omp.terminal.clone())), Some(Some("term-new".to_owned())));
}

#[tokio::test]
async fn found_threads_idle_now_get_their_catch_up_once_and_a_waiting_plan_holds_it_back() {
	let w = world().await;
	w.t.herdr.set_state("p-thread", Working, None, false);
	w.r.discover(&w.snapshot().await).await.unwrap();
	let thread = w.t.ctx.threads.get("p-thread").unwrap();
	// Working: not yet.
	w.r.inner.catch_ups().await.unwrap();
	assert!(thread.with(|t| t.catch_up));
	// A plan awaiting him: not yet.
	thread.with(|t| {
		t.state = Idle;
		t.plan.as_mut().unwrap().awaiting = Some("readme-line".to_owned());
	});
	w.r.inner.catch_ups().await.unwrap();
	assert!(thread.with(|t| t.catch_up));
	// A review phase: not yet.
	thread.with(|t| {
		t.plan.as_mut().unwrap().awaiting = None;
		t.phase = Phase::Review;
	});
	w.r.inner.catch_ups().await.unwrap();
	assert!(thread.with(|t| t.catch_up));
	thread.with(|t| t.phase = Phase::Task);
	w.r.inner.catch_ups().await.unwrap();
	assert!(!thread.with(|t| t.catch_up));
}

#[tokio::test]
async fn a_reopened_thread_idle_with_no_plan_awaiting_in_its_session_has_none_to_show() {
	let w = world().await;
	let file = w._dir.path().join("session.jsonl");
	fs::write(&file, "").unwrap();
	let thread = w.t.with_thread("p-r", "Reopened");
	thread.with(|t| {
		t.state = Idle;
		t.session_path = Some(file.clone());
		t.plan = Some(ThreadPlan { reopened: true, ..ThreadPlan::default() });
	});
	w.r.inner.reopened_plans().await.unwrap();
	assert_eq!(thread.with(|t| t.plan.as_ref().map(|plan| plan.reopened)), Some(false));
	// A thread that is working keeps it.
	let busy = w.t.with_thread("p-w", "Working");
	busy.with(|t| {
		t.state = Working;
		t.session_path = Some(file.clone());
		t.plan = Some(ThreadPlan { reopened: true, ..ThreadPlan::default() });
	});
	w.r.inner.reopened_plans().await.unwrap();
	assert_eq!(busy.with(|t| t.plan.as_ref().map(|plan| plan.reopened)), Some(true));
}

#[tokio::test]
async fn plans_leaves_a_thread_without_a_session_or_at_work_alone() {
	let w = world().await;
	let no_session = w.t.with_thread("p-a", "No session");
	no_session.with(|t| t.state = Idle);
	let busy = w.t.with_thread("p-b", "Busy");
	busy.with(|t| t.state = Working);
	w.r.inner.plans().await.unwrap();
	assert!(no_session.with(|t| t.plan.is_none()));
	assert!(busy.with(|t| t.plan.is_none()));
	w.r.inner.approved_plans().await.unwrap();
}

#[tokio::test]
async fn set_herdr_default_is_kept_until_the_default_workspace_is_closed_or_kept() {
	let w = world().await;
	assert!(!w.r.herdr_default());
	w.r.set_herdr_default(true);
	assert!(w.r.herdr_default());
	// herdr's default workspace is looked at: Jocasta's exists, the default is untouched, so it is closed or kept.
	w.tick().await;
	w.r.set_herdr_default(false);
	assert!(!w.r.herdr_default());
}

#[tokio::test]
async fn a_boss_pane_that_keeps_closing_opens_again_at_once_then_waits() {
	let w = world().await;
	let boss_workspace = || w.t.ctx.boss().unwrap().with(|boss| boss.workspace_id.clone());
	w.close(&boss_workspace()).await;
	w.tick().await;
	assert_eq!(w.t.herdr.of("ensureWorkspace").len(), 2, "the first close opens it at once");
	assert!(w.snapshot().await.panes.iter().any(|pane| pane.pane_id == w.boss()));
	w.close(&boss_workspace()).await;
	w.tick().await;
	w.tick().await;
	assert_eq!(w.t.herdr.of("ensureWorkspace").len(), 2, "the second waits 10 s");
}

// ── panes that sleep (0676) ─────────────────────────────────────────────────────────────────────────────────────────

/// A world whose machine has plenty of free memory and whose idle limit is `minutes`.
async fn sleepy(minutes: f64) -> World {
	world_with(|options| {
		options.pane_idle_minutes = Some(minutes);
		options.available_memory = Some(Arc::new(|| Some(1 << 40)));
	})
	.await
}

impl World {
	/// What omp in `pane` reported: idle with nothing running, since `ago_ms` ago, and the session file it runs.
	fn reports_idle(&self, pane: &str, ago_ms: i64, session: Option<&str>) {
		let agent = PaneAgent {
			idle_reported: true,
			jobs: Some(0),
			active_at: crate::shared::clock::now_ms() - ago_ms,
			..PaneAgent::default()
		};
		let session = session.map(str::to_owned);
		self.t
			.ctx
			.bridge
			.record(|recording| {
				recording.agents.insert(pane.to_owned(), agent);
				if let Some(session) = session {
					recording.sessions.insert(pane.to_owned(), session);
				}
			})
			.unwrap();
	}

	fn sleepers(&self) -> Vec<(String, Vec<String>)> {
		self.respawned()
			.into_iter()
			.map(|(pane, call)| (pane, command_of(&call)))
			.filter(|(_, command)| command.iter().any(|word| word == "pane-sleep"))
			.collect()
	}

	/// The pane now in `cwd`'s place.
	fn pane_in(&self, cwd: &Path) -> String {
		self.t
			.herdr
			.state()
			.pane_list
			.iter()
			.find(|pane| pane.cwd.as_deref().is_some_and(|dir| same_dir(dir, cwd)))
			.unwrap()
			.pane_id
			.clone()
	}
}

#[tokio::test]
async fn an_idle_project_pane_sleeps_after_the_idle_minutes_and_a_key_brings_the_same_session_back_0676() {
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	// Idle for a minute: more than 0.02 minutes.
	w.reports_idle("p-demo", 60_000, Some("session-a.jsonl"));
	w.tick().await;
	let asleep = w.sleepers();
	assert_eq!(asleep.len(), 1);
	assert_eq!(asleep[0].0, "p-demo");
	assert_eq!(asleep[0].1.last().map(String::as_str), Some("demo · asleep"));
	assert_eq!(w.t.herdr.of("shutdown"), [vec![serde_json::json!("p-demo")]], "omp is asked to end first");
	let sleeper = w.pane_in(&w.main);
	assert!(w.t.ctx.launched.asleep(&sleeper));
	assert_eq!(w.t.ctx.launched.paused(&sleeper).as_deref(), Some("session-a.jsonl"));
	// A key in the sleeper: its pane key asks for the wake.
	let key = w.t.ctx.launched.key_of(&sleeper).unwrap();
	assert_eq!(w.t.ctx.sleep.wake_by_key(&key).await.unwrap(), "woke");
	let woken = w.respawned().pop().unwrap();
	assert_eq!(woken.0, sleeper);
	let command = command_of(&woken.1);
	let at = command.iter().position(|word| word == "--restore").expect("omp restores the session");
	assert_eq!(command[at + 1], "session-a.jsonl");
	assert!(!command.iter().any(|word| word == "pane-sleep"));
	let omp_pane = w.pane_in(&w.main);
	assert!(!w.t.ctx.launched.asleep(&omp_pane));
	// Asked again, it is awake: nothing happens.
	assert_eq!(w.t.ctx.sleep.wake_by_key(&key).await.unwrap(), "not asleep");
}

#[tokio::test]
async fn a_wake_with_no_session_known_continues_the_last_one_0676() {
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	w.reports_idle("p-demo", 60_000, None);
	w.tick().await;
	let sleeper = w.pane_in(&w.main);
	let key = w.t.ctx.launched.key_of(&sleeper).unwrap();
	w.t.ctx.sleep.wake_by_key(&key).await.unwrap();
	let command = command_of(&w.respawned().pop().unwrap().1);
	assert!(command.iter().any(|word| word == "--continue"));
	assert!(!command.iter().any(|word| word == "--restore"));
}

#[tokio::test]
async fn a_pane_idle_for_less_than_the_limit_a_working_one_and_an_open_dialog_stay_awake_0676() {
	let w = sleepy(15.0).await;
	w.up();
	w.tick().await;
	w.reports_idle("p-demo", 60_000, None);
	w.tick().await;
	assert!(w.sleepers().is_empty(), "a minute is less than 15");
	drop(w);
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	let mut agent = PaneAgent { idle_reported: true, jobs: Some(0), active_at: 1, ..PaneAgent::default() };
	agent.working = true;
	w.t.ctx.bridge.record(|r| r.agents.insert("p-demo".to_owned(), agent.clone())).unwrap();
	w.tick().await;
	agent.working = false;
	agent.input_open = true;
	w.t.ctx.bridge.record(|r| r.agents.insert("p-demo".to_owned(), agent.clone())).unwrap();
	w.tick().await;
	agent.input_open = false;
	agent.jobs = Some(2);
	w.t.ctx.bridge.record(|r| r.agents.insert("p-demo".to_owned(), agent.clone())).unwrap();
	w.tick().await;
	assert!(w.sleepers().is_empty());
}

#[tokio::test]
async fn zero_idle_minutes_never_sleeps_by_itself_and_the_boss_never_sleeps_0676() {
	let w = sleepy(0.0).await;
	w.up();
	w.tick().await;
	w.reports_idle("p-demo", 3_600_000, None);
	w.tick().await;
	assert!(w.sleepers().is_empty());
	drop(w);
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	w.reports_idle(&w.boss(), 3_600_000, None);
	w.tick().await;
	assert!(w.sleepers().is_empty(), "the boss is never put to sleep");
}

#[tokio::test]
async fn an_omp_that_cannot_shut_down_has_its_tree_ended_and_is_still_put_to_sleep_without_a_message_0684() {
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	w.reports_idle("p-demo", 60_000, Some("s.jsonl"));
	w.t.ctx.bridge.record(|r| r.no_shutdown.insert("p-demo".to_owned())).unwrap();
	w.tick().await;
	assert!(w.t.herdr.of("shutdown").is_empty(), "no shutdown message");
	assert_eq!(w.sleepers().len(), 1, "the pane still goes to the sleeper");
}

#[tokio::test]
async fn a_pane_being_put_to_sleep_is_no_crashed_omp_0676() {
	let w = sleepy(15.0).await;
	w.up();
	w.tick().await;
	// omp ended in the project pane while a sleep of it is under way: not started again.
	let claim = w.t.ctx.sleep.claim("p-demo").expect("free");
	w.ended(&["p-demo".to_owned()]);
	w.tick().await;
	assert!(w.respawned().iter().all(|(pane, _)| pane != "p-demo"));
	drop(claim);
	// Without the sleep it is a crash, and omp starts again.
	w.tick().await;
	assert!(w.respawned().iter().any(|(pane, _)| pane == "p-demo"));
}

#[tokio::test]
async fn under_the_memory_reserve_one_idle_pane_sleeps_per_tick_the_longest_idle_first_and_never_a_working_one_0676() {
	// No idle limit: only the memory pressure puts panes to sleep.
	let w = world_with(|options| {
		options.pane_idle_minutes = Some(0.0);
		options.available_memory = Some(Arc::new(|| Some(0)));
	})
	.await;
	w.up();
	w.tick().await;
	let thread = w.t.ctx.threads.all().remove(0);
	thread.with(|t| t.sleep_when_idle = true);
	w.reports_idle("p-demo", 5_000, Some("demo.jsonl"));
	w.reports_idle("p-thread", 60_000, Some("thread.jsonl"));
	w.tick().await;
	let first = w.sleepers();
	assert_eq!(first.len(), 1, "one pane per tick");
	assert_eq!(first[0].0, "p-thread", "the longest idle");
	assert!(w.t.ctx.launched.asleep(&w.thread_pane()), "the thread's row follows its new pane");
	w.tick().await;
	assert_eq!(w.sleepers().len(), 2);
	// A working omp is never ended for memory.
	drop(w);
	let w = world_with(|options| {
		options.pane_idle_minutes = Some(0.0);
		options.available_memory = Some(Arc::new(|| Some(0)));
	})
	.await;
	w.up();
	w.tick().await;
	let working = PaneAgent { working: true, active_at: 1, ..PaneAgent::default() };
	w.t.ctx.bridge.record(|r| r.agents.insert("p-demo".to_owned(), working)).unwrap();
	w.tick().await;
	assert!(w.sleepers().is_empty());
}

#[tokio::test]
async fn the_focused_pane_is_not_put_to_sleep_for_memory_0676() {
	let w = world_with(|options| {
		options.pane_idle_minutes = Some(0.0);
		options.available_memory = Some(Arc::new(|| Some(0)));
	})
	.await;
	w.up();
	w.tick().await;
	for pane in &mut w.t.herdr.state().pane_list {
		pane.focused = pane.pane_id == "p-demo";
	}
	w.reports_idle("p-demo", 60_000, None);
	w.tick().await;
	assert!(w.sleepers().is_empty());
}

#[tokio::test]
async fn the_focused_pane_does_not_sleep_past_the_idle_minutes_and_does_once_he_looks_elsewhere_0676() {
	let w = sleepy(0.02).await;
	w.up();
	w.tick().await;
	for pane in &mut w.t.herdr.state().pane_list {
		pane.focused = pane.pane_id == "p-demo";
	}
	// He may be reading it, or typing a prompt omp hasn't been sent: no turn starts, so the idle clock alone can't tell.
	w.reports_idle("p-demo", 60_000, Some("session-a.jsonl"));
	w.tick().await;
	assert!(w.sleepers().is_empty());
	assert!(w.t.herdr.of("shutdown").is_empty());
	for pane in &mut w.t.herdr.state().pane_list {
		pane.focused = false;
	}
	w.tick().await;
	assert_eq!(w.sleepers().len(), 1);
}

#[tokio::test]
async fn a_thread_that_reported_sleeps_at_once_and_wakes_before_he_sends_it_something_0676() {
	let w = sleepy(15.0).await;
	w.up();
	w.tick().await;
	let thread = w.t.ctx.threads.all().remove(0);
	w.reports_idle("p-thread", 100, Some("thread.jsonl"));
	w.tick().await;
	assert!(w.sleepers().is_empty(), "a thread that hasn't reported stays awake");
	thread.with(|t| t.sleep_when_idle = true);
	w.tick().await;
	assert_eq!(w.sleepers().len(), 1);
	let asleep = w.thread_pane();
	assert_ne!(asleep, "p-thread");
	assert!(w.t.ctx.launched.asleep(&asleep));
	assert!(w.t.ctx.threads.get(&asleep).is_some_and(|found| found.same(&thread)));
	// A message for it wakes it first, with its session.
	w.t.ctx.sleep.wake_thread(&thread).await.unwrap();
	let command = command_of(&w.respawned().pop().unwrap().1);
	assert!(command.iter().any(|word| word == "--restore"));
	assert!(!thread.with(|t| t.sleep_when_idle));
	assert!(!w.t.ctx.launched.asleep(&w.thread_pane()));
}

#[tokio::test]
async fn nothing_sleeps_while_one_of_jocastas_own_questions_waits_for_his_answer_0676() {
	use crate::frontier::server::allow::{Confirm, Confirmation};
	use crate::frontier::server::wording::Door;
	// An Allow box that nobody answers.
	let waiting: Confirm = Arc::new(|_, _| Box::pin(std::future::pending()));
	let w = world_with(|options| {
		options.pane_idle_minutes = Some(0.02);
		options.available_memory = Some(Arc::new(|| Some(0)));
		options.confirm = Some(waiting);
	})
	.await;
	w.up();
	w.tick().await;
	let thread = w.t.ctx.threads.all().remove(0);
	thread.with(|t| t.sleep_when_idle = true);
	w.reports_idle("p-thread", 600_000, Some("thread.jsonl"));
	w.reports_idle("p-demo", 600_000, None);
	let ctx = w.t.ctx.clone();
	let asking = tokio::spawn(async move {
		let ask = Confirmation { message: "Finish it?".to_owned(), detail: String::new() };
		let _ = ctx.allow.ask(&ask, "nothing was done", Door::Boss).await;
	});
	while w.t.ctx.allow.open() == 0 {
		tokio::time::sleep(std::time::Duration::from_millis(10)).await;
	}
	w.tick().await;
	assert!(w.sleepers().is_empty(), "past the idle limit and under memory pressure, with a question open");
	asking.abort();
}
