//! Tests of `omp_processes.rs`: the omp-process cases of `service-relaunch.test.ts` and the pause cases of
//! `service-regressions.test.ts`, rewritten on the part (the real processes are stand-in trees, 0630), and a case for each
//! method the service's flow reaches.

use std::path::{Path, PathBuf};

use super::*;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::thread::ThreadOmp;
use crate::shared::paths::data_dir;
use crate::shared::proctree::tests::{SleeperTree, settles, sleeper_tree};
use crate::shared::proctree::{pid_alive, process_id_of};
use crate::shared::testing::{TempDir, temp_dir};

/// A thread pane's omp end waited for briefly, and its leftovers looked for again soon (0490): the sleepers never exit.
fn leftovers(options: &mut crate::frontier::server::service::ServiceOptions) {
	options.leftover_wait_ms = Some(500);
	options.leftover_again_ms = Some(200);
}

fn gone(tree: &SleeperTree) -> impl Fn() -> bool + '_ {
	|| tree.all.iter().all(|pid| !pid_alive(*pid))
}

/// A thread in pane `pane` working in `path`, one Jocasta started omp in.
fn started(t: &TestContext, pane: &str, path: &Path) -> ThreadRef {
	let thread = t.with_thread(pane, "Readme line");
	thread.with(|thread| thread.path = path.to_path_buf());
	t.ctx.launched.add(pane, &path.display().to_string(), None).unwrap();
	thread
}

fn omp_of(thread: &ThreadRef) -> ThreadOmp {
	thread.with(|thread| thread.omp.clone()).expect("a remembered omp")
}

fn logged() -> String {
	std::fs::read_dir(crate::shared::log::logs_dir())
		.map(|dir| dir.flatten().filter_map(|entry| std::fs::read_to_string(entry.path()).ok()).collect())
		.unwrap_or_default()
}

/// A process that already ended: its pid, as an omp's that is gone.
fn ended_pid() -> u32 {
	let mut child = std::process::Command::new(std::env::current_exe().unwrap())
		.args(["--exact", "frontier::server::omp_processes::tests::nothing", "-q"])
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.spawn()
		.unwrap();
	let pid = child.id();
	child.wait().unwrap();
	pid
}

/// Runs and ends at once: the process [`ended_pid`] starts.
#[test]
fn nothing() {}

#[tokio::test]
async fn a_thread_panes_omp_is_remembered_by_pid_and_start_with_what_runs_under_it_only_for_a_pane_jocasta_started() {
	let t = test_context(|_| {});
	let folder = temp_dir("omps-remember");
	let thread = t.with_thread("p1", "Readme line");
	// A pane Jocasta didn't start: nothing is read.
	t.ctx.omps.remember(&thread, Some("term_1")).await;
	assert!(thread.with(|thread| thread.omp.is_none()));
	// No root process known yet: remembered by its terminal alone.
	t.ctx.launched.add("p1", &folder.path().display().to_string(), None).unwrap();
	t.ctx.omps.remember(&thread, Some("term_1")).await;
	let omp = omp_of(&thread);
	assert_eq!((omp.terminal.as_deref(), omp.shim, omp.processes), (Some("term_1"), None, None));
	// herdr's root process: the shim by pid and start, and what runs at and under it.
	let tree = sleeper_tree(folder.path(), true).await;
	t.herdr.state().roots.insert("p1".to_owned(), tree.root);
	t.ctx.omps.remember(&thread, None).await;
	let omp = omp_of(&thread);
	assert_eq!(omp.terminal, None);
	assert_eq!(omp.shim.map(|shim| shim.pid), Some(tree.root));
	let processes = omp.processes.expect("the processes under it");
	assert!(processes.omp.iter().any(|id| id.pid == tree.root));
	assert!(processes.tree.len() >= 2, "{processes:?}");
	assert!(processes.tree.iter().all(|id| tree.all.contains(&id.pid)));
}

#[tokio::test]
async fn an_omp_remembered_as_only_the_process_herdr_started_learns_what_runs_under_it_from_one_snapshot() {
	let t = test_context(|_| {});
	let folder = temp_dir("omps-complete");
	let tree = sleeper_tree(folder.path(), false).await;
	let shim = process_id_of(tree.root).expect("the root runs");
	let only_shim = OmpProcesses { omp: vec![shim], tree: vec![shim] };
	let waiting = started(&t, "p1", folder.path());
	let other_terminal = started(&t, "p2", folder.path());
	let no_pane = started(&t, "p3", folder.path());
	for (thread, terminal) in [(&waiting, "term_1"), (&other_terminal, "term_other"), (&no_pane, "term_3")] {
		thread.with(|thread| {
			thread.omp = Some(ThreadOmp {
				terminal: Some(terminal.to_owned()),
				shim: Some(shim),
				processes: Some(only_shim.clone()),
			});
		});
	}
	let pane = |id: &str, terminal: &str| PaneInfo {
		pane_id: id.to_owned(),
		terminal_id: Some(terminal.to_owned()),
		..PaneInfo::default()
	};
	let by_id: HashMap<String, PaneInfo> =
		[pane("p1", "term_1"), pane("p2", "term_2")].into_iter().map(|pane| (pane.pane_id.clone(), pane)).collect();
	t.ctx.omps.complete(&by_id);
	let learned = omp_of(&waiting);
	let processes = learned.processes.expect("processes");
	assert!(processes.omp.len() >= 2, "{processes:?}");
	assert_eq!(learned.shim, Some(shim));
	// Another terminal than the pane's, or no pane in the snapshot: left as it was.
	assert_eq!(omp_of(&other_terminal).processes, Some(only_shim.clone()));
	assert_eq!(omp_of(&no_pane).processes, Some(only_shim));
	// Once two are known, nothing is read again.
	let known = omp_of(&waiting).processes;
	t.ctx.omps.complete(&by_id);
	assert_eq!(omp_of(&waiting).processes, known);
}

#[tokio::test]
async fn a_shim_that_ended_without_a_child_is_forgotten_by_complete() {
	let t = test_context(|_| {});
	let folder = temp_dir("omps-shim");
	let dead = ProcessId { pid: ended_pid(), start: 1.0 };
	let thread = started(&t, "p1", folder.path());
	thread.with(|thread| {
		thread.omp = Some(ThreadOmp {
			terminal: Some("term_1".to_owned()),
			shim: Some(dead),
			processes: Some(OmpProcesses { omp: vec![dead], tree: vec![dead] }),
		});
	});
	let pane = PaneInfo { pane_id: "p1".to_owned(), terminal_id: Some("term_1".to_owned()), ..PaneInfo::default() };
	t.ctx.omps.complete(&HashMap::from([("p1".to_owned(), pane)]));
	assert_eq!(omp_of(&thread).shim, None);
}

#[tokio::test]
async fn a_thread_pane_herdr_restored_as_a_shell_its_old_omp_and_what_it_left_end_a_process_something_else_started_keeps_running()
 {
	let t = test_context(leftovers);
	let folder = temp_dir("omps-end");
	let path = folder.path().join("demo.wt").join("readme-line");
	let thread = started(&t, "p1", &path);
	// The old omp with its workers (real processes that outlive it), working in the worktree.
	let omp = sleeper_tree(&path.join("pane"), true).await;
	// He started it in the worktree from another terminal: a real process that stays up until killed.
	let other = sleeper_tree(&path.join("other"), false).await;
	t.herdr.state().roots.insert("p1".to_owned(), omp.root);
	t.ctx.omps.remember(&thread, Some("term_1")).await;
	let processes = omp_of(&thread).processes.expect("processes");
	// The first pass resolves with the omp and its leftovers ended.
	t.ctx.omps.end(processes).await;
	assert!(settles(gone(&omp)).await, "the old omp's processes are still running");
	assert!(pid_alive(other.root));
	// Its second look comes `leftover_again_ms` later; then it is no longer held for Stop everything.
	assert!(settles(|| t.ctx.omps.state().ending.is_empty()).await);
	assert!(pid_alive(other.root));
}

#[tokio::test]
async fn stop_everything_ends_what_each_thread_panes_remembered_omp_left_also_when_herdrs_server_didnt_what_something_else_started_keeps_running()
 {
	let t = test_context(leftovers);
	let folder = temp_dir("omps-leftovers");
	let path = folder.path().join("demo.wt").join("readme-line");
	// Nothing to end: it returns at once.
	t.ctx.omps.end_leftovers().await.unwrap();
	let thread = started(&t, "p1", &path);
	// The omp with its workers, from a herdr server that no longer has them.
	let omp = sleeper_tree(&path.join("pane"), true).await;
	let other = sleeper_tree(&path.join("other"), false).await;
	t.herdr.state().roots.insert("p1".to_owned(), omp.root);
	t.ctx.omps.remember(&thread, Some("term_1")).await;
	t.ctx.omps.end_leftovers().await.unwrap();
	assert!(settles(gone(&omp)).await, "the omp's processes are still running");
	assert!(pid_alive(other.root));
}

#[tokio::test]
async fn an_old_omp_still_being_ended_is_ended_by_stop_everything_too() {
	let t = test_context(|options| {
		options.leftover_wait_ms = Some(1);
		options.leftover_again_ms = Some(5_000);
	});
	let folder = temp_dir("omps-ending");
	let omp = sleeper_tree(folder.path(), true).await;
	let shim = process_id_of(omp.root).unwrap();
	let processes = omps_of(shim);
	// Its second look is far off; the first pass ended it already, so a new tree stands for what it started on its way out.
	t.ctx.omps.end(processes.clone()).await;
	assert_eq!(t.ctx.omps.state().ending.len(), 1);
	let worker = sleeper_tree(&folder.path().join("worker"), false).await;
	t.ctx.omps.state().ending[0].1.tree.extend(worker.all.iter().map(|pid| process_id_of(*pid).unwrap()));
	let begun = std::time::Instant::now();
	// Stop everything's own first pass ends the worker long before the end's second look (5 s) would.
	let (ended, killed) = tokio::join!(t.ctx.omps.end_leftovers(), async {
		let killed = settles(gone(&worker)).await;
		(killed, begun.elapsed())
	});
	ended.unwrap();
	assert!(settles(gone(&omp)).await);
	assert!(killed.0, "what an omp being ended started late is ended by Stop everything");
	assert!(killed.1 < Duration::from_secs(4), "{:?}", killed.1);
}

fn omps_of(shim: ProcessId) -> OmpProcesses {
	omp_processes(&[shim]).into_iter().next().expect("a read")
}

struct Pause {
	t: TestContext,
	_folder: TempDir,
	thread: ThreadRef,
	omp: SleeperTree,
	boss: SleeperTree,
	session: PathBuf,
}

/// A thread in its review turn and the boss, each with its omp (real processes that outlive their roots, as omp's
/// workers do): the thread's omp and the boss's.
async fn pause_world() -> Pause {
	let folder = temp_dir("omps-pause");
	let t = test_context(move |options| {
		leftovers(options);
	});
	let path = folder.path().join("demo.wt").join("readme-line");
	let thread = started(&t, "p1", &path);
	t.ctx.launched.add("boss-1", &folder.path().display().to_string(), None).unwrap();
	let omp = sleeper_tree(&folder.path().join("thread-omp"), true).await;
	let boss = sleeper_tree(&folder.path().join("boss-omp"), true).await;
	t.herdr.state().roots.insert("p1".to_owned(), omp.root);
	t.herdr.state().roots.insert("boss-1".to_owned(), boss.root);
	t.ctx.omps.remember(&thread, Some("term_1")).await;
	// Its task ended and its review turn runs.
	let session = folder.path().join("session.jsonl");
	thread.with(|thread| {
		thread.state = AgentState::Working;
		thread.session_path = Some(session.clone());
	});
	Pause { t, _folder: folder, thread, omp, boss, session }
}

#[tokio::test]
async fn a_thread_in_its_review_turn_is_saved_for_restore_and_its_omp_ends_forced_the_bosss_stays() {
	let Pause { t, thread, omp, boss, session, _folder } = pause_world().await;
	t.ctx.omps.pause().await.unwrap();
	// Saved in `panes.json`: the next start brings its omp back with `--restore`.
	assert_eq!(t.ctx.launched.paused("p1"), Some(session.display().to_string()));
	assert_eq!(t.ctx.launched.paused("boss-1"), None);
	let panes: serde_json::Value =
		serde_json::from_str(&std::fs::read_to_string(data_dir().join("panes.json")).unwrap()).unwrap();
	assert_eq!(panes["p1"]["restore"], session.display().to_string());
	assert!(panes["boss-1"].get("restore").is_none());
	// Its omp ends at once with what runs under it; the boss's ends with herdr.
	t.ctx.omps.cut_paused().await;
	assert!(settles(gone(&omp)).await, "the thread's omp is still running");
	assert!(boss.all.iter().all(|pid| pid_alive(*pid)));
	assert!(logged().contains("demo/Readme line: its omp ended mid-turn, for --restore"), "{}", logged());
	drop(thread);
}

#[tokio::test]
async fn a_thread_not_at_work_or_without_a_session_is_not_saved_and_one_awaiting_him_neither() {
	let t = test_context(|_| {});
	let folder = temp_dir("omps-idle");
	let session = folder.path().join("s.jsonl");
	let idle = started(&t, "p1", folder.path());
	idle.with(|thread| {
		thread.state = AgentState::Idle;
		thread.session_path = Some(session.clone());
	});
	let no_session = started(&t, "p2", folder.path());
	no_session.with(|thread| thread.state = AgentState::Working);
	let shown = started(&t, "p3", folder.path());
	shown.with(|thread| {
		thread.state = AgentState::Idle;
		thread.shown = crate::frontier::protocol::ThreadState::Working;
		thread.session_path = Some(session.clone());
	});
	t.ctx.omps.pause().await.unwrap();
	assert_eq!(t.ctx.launched.paused("p1"), None);
	assert_eq!(t.ctx.launched.paused("p2"), None);
	// A turn started and not yet reported is at work too.
	assert_eq!(t.ctx.launched.paused("p3"), Some(session.display().to_string()));
	// A thread paused but whose omp is not known: it ends with herdr.
	t.ctx.omps.cut_paused().await;
	assert!(
		logged().contains("demo/Readme line's omp isn't known; it ends with herdr, and its turn may not go on"),
		"{}",
		logged()
	);
}

#[tokio::test]
async fn a_herdr_that_cannot_read_a_pane_doesnt_hold_pause_and_nothing_to_remove_returns_at_once() {
	let t = test_context(leftovers);
	let started_at = std::time::Instant::now();
	assert!(started_at.elapsed() < Duration::from_secs(2));
	// A pane Jocasta started whose root herdr doesn't know: no omp to end later.
	t.ctx.launched.add("boss-1", "/somewhere", None).unwrap();
	t.ctx.omps.pause().await.unwrap();
}

/// A thread in pane `p1`, idle, one Jocasta started omp in.
fn bridged() -> (TestContext, ThreadRef, TempDir) {
	let folder = temp_dir("omps-bridge");
	let t = test_context(|_| {});
	let path = folder.path().join("demo.wt").join("readme-line");
	std::fs::create_dir_all(&path).unwrap();
	let thread = started(&t, "p1", &path);
	thread.with(|thread| thread.state = AgentState::Idle);
	(t, thread, folder)
}

fn host_of(thread: &ThreadRef) -> Option<AgentState> {
	thread.with(|thread| thread.host)
}

#[tokio::test]
async fn each_thread_panes_omp_as_its_bridge_reports_it_working_blocked_or_idle_and_none_when_the_pipe_is_gone() {
	let (t, thread, _folder) = bridged();
	t.ctx.omps.host_reports("p1", Some(AgentState::Working));
	assert_eq!(host_of(&thread), Some(AgentState::Working));
	assert!(logged().contains("demo/Readme line: omp reports working"), "{}", logged());
	t.ctx.omps.host_reports("p1", Some(AgentState::Blocked));
	assert_eq!(host_of(&thread), Some(AgentState::Blocked));
	t.ctx.omps.host_reports("p1", Some(AgentState::Idle));
	assert_eq!(host_of(&thread), Some(AgentState::Idle));
	// The pipe ends: the thread's own state decides again.
	t.ctx.omps.host_reports("p1", None);
	assert_eq!(host_of(&thread), None);
	assert!(logged().contains("demo/Readme line: omp's bridge is gone; herdr decides"), "{}", logged());
}

#[tokio::test]
async fn a_report_for_a_pane_that_is_no_thread_is_ignored_and_a_stopping_service_takes_none() {
	let (t, thread, _folder) = bridged();
	t.ctx.omps.host_reports("nowhere", Some(AgentState::Working));
	assert_eq!(host_of(&thread), None);
	t.ctx.set_stopping();
	t.ctx.omps.host_reports("p1", Some(AgentState::Working));
	assert_eq!(host_of(&thread), None);
}
