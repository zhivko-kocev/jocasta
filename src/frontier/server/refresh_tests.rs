//! The refresh tick's cases, at part level on `context::test_context`: a real git project `demo` with a linked
//! worktree `readme-line`, herdr's workspaces and panes in a `FakeHerdr`, the boss started through `ensure_boss`.
//!
//! Waits for service (they need the HTTP server or the whole service): `service-regressions` "C6b" (the `hosts` gate is
//! `service.ts`'s start of the host poll), "SVC-2" as a timing race between a start and a tick's prompt (its two
//! halves are pinned below on `closed_threads` and on `known`), `service-plans` "a done and reviewed thread survives
//! Stop everything and reopen" (HUD and close question), and every case that starts its thread with `thread_start`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use super::*;
use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::herdr::AgentState::{Idle, Unknown, Working};
use crate::frontier::server::herdr::WorkspaceInfo;
use crate::frontier::server::service::RelayUrlOf;
use crate::shared::git::git;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::testing::{TempDir, temp_dir};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

/// A relay URL that is public for a folder named `public_for` and loopback for any other.
fn relay(public_for: &'static str) -> RelayUrlOf {
	Arc::new(move |_, cwd, _| {
		let public = cwd.file_name().is_some_and(|name| name == public_for);
		Box::pin(async move { Ok(if public { "wss://relay.example.org" } else { "ws://127.0.0.1:47499" }.to_owned()) })
	})
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

async fn world(public_relay_for: &'static str) -> World {
	let t = test_context(|options| {
		options.mise = Some(None);
		options.hunk = Some(None);
		options.relay_url_of = Some(relay(public_relay_for));
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
	let w = world("none").await;
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
	// Only the project pane gets the turn-end extension (0617).
	assert!(command_of(project).contains(&"-e".to_owned()));
	assert!(!command_of(thread).contains(&"-e".to_owned()));
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
	let w = world("none").await;
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
	let w = world("none").await;
	w.tick().await;
	w.tick().await;
	assert_eq!(w.t.herdr.state().pane_list.iter().find(|p| p.pane_id == "p-demo").unwrap().agent_state, Unknown);
	assert!(w.respawned().is_empty());
}

#[tokio::test]
async fn a_thread_being_finished_its_omp_ending_as_its_workspace_closes_is_not_started_again_0521() {
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
async fn svc_3_a_project_pane_whose_omp_cant_start_again_does_not_end_the_tick() {
	// The project's own omp config names a public relay: its pane is refused (0536, 0561); the thread's isn't.
	let w = world("demo").await;
	w.up();
	w.tick().await;
	w.t.herdr.restore();
	w.tick().await;
	let respawned = w.respawned();
	assert!(respawned.iter().any(|(_, call)| same_dir(Path::new(call["cwd"].as_str().unwrap()), &w.wt)));
	assert!(!respawned.iter().any(|(_, call)| same_dir(Path::new(call["cwd"].as_str().unwrap()), &w.main)));
	let sent = w.t.sent.lock().unwrap();
	assert!(sent.iter().any(|m| matches!(m, ServerMessage::Hud { hud } if hud.awaiting == 0)), "{sent:?}");
}

#[tokio::test]
async fn svc_2_a_thread_registered_after_the_list_was_read_is_not_dropped_nor_said_closed() {
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
async fn discover_finds_a_pane_in_a_linked_worktree_that_runs_omp_with_its_review_pane() {
	let w = world("none").await;
	w.up();
	{
		let mut herdr = w.t.herdr.state();
		let mut hunk = herdr.pane_list.iter().find(|p| p.pane_id == "p-thread").unwrap().clone();
		hunk.pane_id = "p-hunk".to_owned();
		hunk.label = Some("Hunk review".to_owned());
		hunk.agent = None;
		herdr.pane_list.push(hunk);
	}
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
	assert_eq!(t.review.and_then(|review| review.pane), Some("p-hunk".to_owned()));
	// Not the boss, not the project's own pane, not the hunk pane.
	assert!(w.t.ctx.threads.get("p-demo").is_none() && w.t.ctx.threads.get("p-hunk").is_none());
}

#[tokio::test]
async fn discover_takes_a_pane_jocasta_started_before_omp_reports_but_never_a_plain_shell() {
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
async fn a_step_that_fails_is_said_once_per_reason_and_the_rest_still_run() {
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
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
	let w = world("none").await;
	assert!(!w.r.herdr_default());
	w.r.set_herdr_default(true);
	assert!(w.r.herdr_default());
	// herdr's default workspace is looked at: Jocasta's exists, the default is untouched, so it is closed or kept.
	w.tick().await;
	w.r.set_herdr_default(false);
	assert!(!w.r.herdr_default());
}
