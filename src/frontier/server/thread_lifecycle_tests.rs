//! Tests of `thread_lifecycle.rs`, on `context::test_context`. The TypeScript's cases drive the whole service over HTTP
//! (`service-handoff`, `service-plans`, `service-relaunch`, `service-regressions`, `service-allow`, `service-finish`);
//! each of them whose subject is this part is rewritten here at part level, under its own name, and checks what herdr
//! was told, what the boss and the clients heard, and what the thread and the day hold.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{Local, TimeZone};
use serde_json::{Value, json};

use super::*;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::day::DayOptions;
use crate::frontier::server::herdr::AgentState::{Blocked, Idle, Unknown, Working};
use crate::frontier::server::herdr::PaneInfo;
use crate::frontier::server::hunk::fake_hunk::{fake_hunk, health_daemon};
use crate::frontier::server::sessions::TestsResult;
use crate::frontier::server::task_file::TaskRecord;
use crate::frontier::server::thread::{ReviewResult, ReviewRunning, ThreadPlan, ThreadReview};
use crate::shared::events::queue::EventRow;
use crate::shared::git::git;
use crate::shared::settings::settings_file;
use crate::shared::testing::{TempDir, temp_dir};

const NORMAL: ToastPriority = ToastPriority::Normal;

/// A test context with a boss pane (idle) and the thread `demo/Readme line` in pane `p-1`, in a real git repository.
struct World {
	t: TestContext,
	thread: ThreadRef,
	repo: std::path::PathBuf,
	_folder: TempDir,
	/// The fake hunk's folder, for a world whose reviews run.
	_hunk: Option<TempDir>,
}

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn head_of(repo: &Path) -> String {
	sh(repo, &["rev-parse", "HEAD"])
}

/// A commit on the thread's branch, as its task would make.
fn commit_in(repo: &Path, text: &str, message: &str) {
	fs::write(repo.join("README.md"), text).unwrap();
	sh(repo, &["commit", "-qam", message]);
}

fn add_pane(t: &TestContext, pane_id: &str, state: AgentState) {
	t.herdr.state().pane_list.push(PaneInfo {
		pane_id: pane_id.to_owned(),
		tab_id: "t-1".to_owned(),
		workspace_id: "w-1".to_owned(),
		agent_state: state,
		..PaneInfo::default()
	});
}

fn world(options: impl FnOnce(&mut crate::frontier::server::service::ServiceOptions)) -> World {
	let t = test_context(options);
	t.with_boss(Idle);
	let folder = temp_dir("lifecycle");
	let repo = folder.path().join("wt");
	fs::create_dir_all(&repo).unwrap();
	sh(&repo, &["init", "-q", "-b", "main"]);
	fs::write(repo.join("README.md"), "demo\n").unwrap();
	sh(&repo, &["add", "."]);
	sh(&repo, &["commit", "-q", "-m", "init"]);
	let thread = t.with_thread("p-1", "Readme line");
	thread.with(|th| {
		th.path = repo.clone();
		th.head = Some(head_of(&repo));
		th.state = Idle;
		th.plan = Some(ThreadPlan { sent_back: Some(0), ..ThreadPlan::default() });
	});
	add_pane(&t, "p-1", Idle);
	World { t, thread, repo, _folder: folder, _hunk: None }
}

/// The same world with the real review in place, as the service has it: the fake hunk and its daemon, and trunk `main`
/// (the repository itself is the project's main checkout), so a task end opens the review step instead of skipping it.
async fn reviewing(options: impl FnOnce(&mut crate::frontier::server::service::ServiceOptions)) -> World {
	let dir = temp_dir("lifecycle-hunk");
	let hunk = fake_hunk(dir.path());
	let port = health_daemon().await;
	let mut w = world(|o| {
		o.hunk = Some(Some(hunk));
		o.hunk_port = Some(port);
		o.mise = Some(None);
		options(o);
	});
	let main = w.repo.clone();
	w.thread.with(|t| t.project.main_path = main);
	w._hunk = Some(dir);
	w
}

impl World {
	fn pane(&self) -> String {
		self.thread.with(|t| t.pane_id.clone())
	}

	/// herdr says the pane is in `state`, as its event; the chain is waited for.
	async fn change(&self, state: AgentState) {
		self.change_with(state, None).await;
	}

	/// The same, with the session file omp's integration reported.
	async fn change_with(&self, state: AgentState, session: Option<&Path>) {
		let pane = self.pane();
		self.t.herdr.set_state(&pane, state, session.and_then(Path::to_str), false);
		self.t.ctx.lifecycle.state_changed(&pane, state, StateSource::Herdr);
		self.t.ctx.lifecycle.settled().await;
	}

	fn shown(&self) -> ThreadState {
		self.thread.with(|t| t.shown)
	}

	fn prompts_to(&self, pane: &str) -> Vec<String> {
		let sent = self.t.herdr.of("prompt");
		sent.iter().filter(|args| args[0] == pane).map(|args| args[1].as_str().unwrap().to_owned()).collect()
	}

	/// The boss pane's `[event]` lines so far, one per line.
	fn boss_lines(&self) -> Vec<String> {
		self.prompts_to("boss-1").iter().flat_map(|text| text.lines().map(str::to_owned)).collect()
	}

	fn said(&self, start: &str) -> usize {
		self.boss_lines().iter().filter(|line| line.starts_with(start)).count()
	}

	/// The boss answered and is idle again: lines waiting for it go in.
	async fn boss_idle(&self) {
		self.t.ctx.events.boss_changed(Working).await;
		self.t.ctx.events.boss_changed(Idle).await;
	}

	fn row(&self, pane: &str) -> Option<EventRow> {
		self.t.ctx.day.queue(|queue| queue.by_source(&format!("thread:blocked:{pane}"))).unwrap()
	}

	/// A thread:blocked row in the day's queue, unresolved.
	fn awaits(&self, pane: &str) -> bool {
		self.row(pane).is_some_and(|row| row.resolved_at.is_none())
	}

	fn by_id(&self) -> HashMap<String, PaneInfo> {
		self.t.herdr.state().pane_list.iter().map(|pane| (pane.pane_id.clone(), pane.clone())).collect()
	}

	fn toasts(&self) -> Vec<(ToastReason, String, Option<String>)> {
		let sent = self.t.sent.lock().unwrap();
		sent.iter()
			.filter_map(|message| match message {
				ServerMessage::Toast { reason, text, pane_id, .. } => Some((*reason, text.clone(), pane_id.clone())),
				_ => None,
			})
			.collect()
	}

	/// What herdr was asked about `pane`, by method, in order.
	fn calls_on(&self, pane: &str) -> Vec<String> {
		let state = self.t.herdr.state();
		state
			.calls
			.iter()
			.filter(|call| call.args.first().is_some_and(|arg| arg == pane))
			.map(|c| c.method.clone())
			.collect()
	}
}

fn now() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

fn append(file: &Path, entries: &[Value]) {
	let mut out = OpenOptions::new().append(true).open(file).unwrap();
	for entry in entries {
		writeln!(out, "{entry}").unwrap();
	}
}

/// An omp session file in omp's shape (title, session header, assistant messages), each its own session.
fn session_file(t: &TestContext, cwd: &Path, messages: &[(&str, f64, Option<&str>)]) -> std::path::PathBuf {
	let dir = t.ctx.options.sessions_dir.clone().unwrap().join("--demo--");
	fs::create_dir_all(&dir).unwrap();
	let stem = uuid::Uuid::new_v4().simple().to_string();
	let file = dir.join(format!("2026-10-13T10-00-00-000Z_{stem}.jsonl"));
	let at = now();
	let mut lines = vec![
		json!({"type": "title", "v": 1, "title": "Readme line"}),
		json!({"type": "session", "id": format!("s-{stem}"), "cwd": cwd.display().to_string(), "timestamp": iso(at)}),
	];
	for (i, (text, cost, ask)) in messages.iter().enumerate() {
		let mut content = vec![json!({"type": "text", "text": text})];
		if let Some(question) = ask {
			let html = format!(
				"<form><p>{}</p><label><input type=\"radio\" name=\"answer\" value=\"yes\" checked> Yes</label><label><input type=\"radio\" name=\"answer\" value=\"no\"> No</label></form>",
				question.replace('&', "&amp;").replace('<', "&lt;")
			);
			content.push(json!({"type": "toolCall", "id": "c1", "name": "ask", "arguments": {"html": html}}));
		}
		lines.push(json!({"type": "message", "id": format!("{stem}-m{i}"), "message": {
			"role": "assistant", "provider": "anthropic", "model": "claude-opus-5-5", "timestamp": at,
			"stopReason": "stop", "usage": {"cost": {"total": cost}}, "content": content}}));
	}
	fs::write(&file, "").unwrap();
	append(&file, &lines);
	file
}

fn iso(ms: i64) -> String {
	chrono::DateTime::from_timestamp_millis(ms).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn plan_mode_entry(id: &str) -> Value {
	json!({"type": "mode_change", "id": id, "mode": "plan", "data": {"planFilePath": "local://readme-line-plan.md"}})
}

/// omp's proposal of a plan through `xd://propose`, its turn silently aborted (omp 18.10's shape).
fn append_proposal(file: &Path) {
	append(
		file,
		&[
			json!({"type": "message", "id": "t1", "message": {"role": "toolResult", "toolCallId": "c9", "toolName": "write",
				"content": [{"type": "text", "text": "Plan ready for review."}],
				"details": {"xdev": {"tool": "propose", "mode": "execute", "args": {"title": "readme-line"},
					"inner": {"planFilePath": "local://readme-line-plan.md", "title": "readme-line", "planExists": true}}},
				"isError": false}}),
			json!({"type": "message", "id": "a9", "message": {"role": "assistant", "content": [], "stopReason": "aborted"}}),
			plan_mode_entry("m2"),
		],
	);
}

/// A plan-mode session with no plan proposed yet.
fn plan_mode_file(t: &TestContext, cwd: &Path) -> std::path::PathBuf {
	let file = session_file(t, cwd, &[("I wrote the plan.", 0.25, None)]);
	append(&file, &[plan_mode_entry("m1")]);
	file
}

/// A plan-mode session with its plan proposed and the turn silently aborted.
fn proposed_plan_file(t: &TestContext, cwd: &Path) -> std::path::PathBuf {
	let file = plan_mode_file(t, cwd);
	append_proposal(&file);
	file
}

/// A `/handoff` entry in omp's shape, as `handed_off` reads it.
fn handoff_entry(file: &Path) {
	append(
		file,
		&[
			json!({"type": "compaction", "id": "h1", "method": "handoff", "summary": "Handed off.", "timestamp": iso(now())}),
		],
	);
}

// ── service-handoff.test.ts ─────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn thread_states_stay_true_a_dismissed_question_ends_the_turn_and_the_page_sees_the_boss_take_an_urgent_line() {
	// CO-3, CO-7
	let w = world(|_| {});
	let asked = session_file(&w.t, &w.repo, &[("Which format?", 0.1, Some("CSV or JSON?"))]);
	w.change_with(Working, Some(&asked)).await;
	w.change(Blocked).await;
	assert_eq!(w.shown(), ThreadState::Blocked);
	assert!(w.awaits("p-1"));
	// The urgent line went into the idle boss at once, and the clients got a toast.
	assert_eq!(w.boss_lines(), ["[event] demo/Readme line — awaiting you: «CSV or JSON?»"]);
	assert_eq!(
		w.toasts(),
		[(ToastReason::Blocked, "demo/Readme line — awaiting you: CSV or JSON?".to_owned(), Some("p-1".to_owned()))]
	);
	// He dismisses its question: nothing awaits him any more.
	w.change(Idle).await;
	assert!(!w.awaits("p-1"));
	assert_eq!(w.shown(), ThreadState::Idle);
	w.boss_idle().await;
	assert_eq!(w.boss_lines()[1], "[event] demo/Readme line — finished its turn: «Which format?»");
}

#[tokio::test]
async fn a_handoff_with_no_entry_does_not_swallow_the_next_task_it_ends_as_a_task_end() {
	// CO-4: with no steer to ask for a report, the next task's end is noted at once, and its review runs.
	let w = reviewing(|options| options.ask_report = Some(false)).await;
	let reviewed = w.thread.with(|t| t.head.clone());
	w.thread.with(|t| {
		set_phase(t, Phase::Handoff, "it is asked for /handoff");
		t.handoff_at = Some(now());
	});
	// /handoff wrote no entry; the next task commits.
	w.change(Working).await;
	assert_eq!(w.thread.with(|t| t.phase), Phase::Handoff);
	commit_in(&w.repo, "demo\nA line.\nAnother.\n", "Another line");
	w.change(Idle).await;
	assert_eq!(w.thread.with(|t| t.phase), Phase::Review);
	let head = head_of(&w.repo);
	assert_ne!(Some(head.clone()), reviewed);
	assert_eq!(w.thread.with(|t| (t.head.clone(), t.shown)), (Some(head), ThreadState::Done));
	let prompts = w.prompts_to("p-1");
	assert_eq!(prompts.len(), 1);
	assert!(prompts[0].contains("main...feature/p-1"), "the review turn: {}", prompts[0]);
}

#[tokio::test]
async fn the_handoff_turns_end_with_no_new_commit_closes_the_task_and_is_not_a_task_end() {
	let w = world(|options| options.ask_report = Some(false));
	w.thread.with(|t| {
		set_phase(t, Phase::Handoff, "it is asked for /handoff");
		t.handoff_at = Some(now());
	});
	w.change(Working).await;
	w.change(Idle).await;
	assert_eq!(w.thread.with(|t| (t.phase, t.shown)), (Phase::Task, ThreadState::Idle));
	assert!(w.boss_lines().is_empty());
}

#[tokio::test]
async fn a_thread_at_work_after_its_handoff_entry_is_in_the_session_starts_the_next_task() {
	// F-01: herdr never reports `/handoff` as a turn.
	let w = world(|_| {});
	let file = session_file(&w.t, &w.repo, &[("Handing off.", 0.1, None)]);
	w.thread.with(|t| {
		set_phase(t, Phase::Handoff, "it is asked for /handoff");
		t.handoff_at = Some(now() - 1_000);
	});
	handoff_entry(&file);
	commit_in(&w.repo, "demo\nA line.\n", "Add a README line");
	w.change_with(Working, Some(&file)).await;
	assert_eq!(w.thread.with(|t| t.phase), Phase::Task);
	// Its commits count from here.
	assert_eq!(w.thread.with(|t| t.head.clone()), Some(head_of(&w.repo)));
	assert_eq!(w.shown(), ThreadState::Working);
}

#[tokio::test]
async fn a_thread_whose_omp_host_answers_leaves_the_handoff_step_once_its_session_has_the_handoffs_entry() {
	// F-01, 0547
	let w = world(|_| {});
	let file = session_file(&w.t, &w.repo, &[("Reviewed.", 0.1, None)]);
	let asked = now() - 1_000;
	w.thread.with(|t| {
		set_phase(t, Phase::Handoff, "it is asked for /handoff");
		t.handoff_at = Some(asked);
		t.host = Some(Idle);
		t.session_path = Some(file.clone());
	});
	// /handoff ran with no turn reported by herdr or the host; its entry isn't in the session yet.
	w.t.ctx.lifecycle.reconcile(&w.by_id(), now() + 1_000).await.unwrap();
	assert_eq!(w.thread.with(|t| t.phase), Phase::Handoff);
	assert!(!w.t.ctx.lifecycle.handed_off(&w.thread).await);
	// Its entry is in the session now (a fresh index, as a read a second later sees it).
	let done = session_file(&w.t, &w.repo, &[("Reviewed.", 0.1, None)]);
	handoff_entry(&done);
	w.thread.with(|t| t.session_path = Some(done.clone()));
	commit_in(&w.repo, "demo\nA line.\n", "Add a README line");
	assert!(w.t.ctx.lifecycle.handed_off(&w.thread).await);
	w.t.ctx.lifecycle.reconcile(&w.by_id(), now() + 1_000).await.unwrap();
	assert_eq!(w.thread.with(|t| (t.phase, t.head.clone())), (Phase::Task, Some(head_of(&w.repo))));
}

#[tokio::test]
async fn a_handoff_entry_older_than_the_ask_is_not_this_handoffs() {
	let w = world(|_| {});
	let file = session_file(&w.t, &w.repo, &[("Reviewed.", 0.1, None)]);
	handoff_entry(&file);
	w.thread.with(|t| {
		t.handoff_at = Some(now() + 60_000);
		t.session_path = Some(file.clone());
	});
	assert!(!w.t.ctx.lifecycle.handed_off(&w.thread).await);
	w.thread.with(|t| t.session_path = None);
	assert!(!w.t.ctx.lifecycle.handed_off(&w.thread).await);
}

#[tokio::test]
async fn r3_a_working_thread_whose_pane_herdr_restored_as_a_shell_is_relaunched_and_no_longer_at_work() {
	// 0438: its new omp comes up idle without an event; the thread no longer counts as working.
	let w = world(|_| {});
	w.change(Working).await;
	assert_eq!(w.shown(), ThreadState::Working);
	w.t.herdr.restore();
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	assert_ne!(w.pane(), "p-1");
	assert_eq!(w.shown(), ThreadState::Idle);
	assert_eq!(w.thread.with(|t| t.state), Unknown);
}

// ── service-relaunch.test.ts ────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn r3_a_thread_blocked_on_a_question_relaunched_no_longer_awaits_him() {
	// 0438: --continue doesn't resume the turn that asked.
	let w = world(|_| {});
	let asked = session_file(&w.t, &w.repo, &[("Which format?", 0.1, Some("CSV or JSON?"))]);
	w.change_with(Working, Some(&asked)).await;
	w.change(Blocked).await;
	assert!(w.awaits("p-1"));
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	let fresh = w.pane();
	assert_ne!(fresh, "p-1");
	assert!(!w.awaits("p-1"));
	assert!(!w.awaits(&fresh));
	assert_eq!(w.shown(), ThreadState::Idle);
	// It moved to the new pane, where its omp comes up idle without an event.
	assert!(w.t.ctx.threads.get("p-1").is_none());
	assert!(w.t.ctx.threads.get(&fresh).is_some_and(|open| open.same(&w.thread)));
	assert!(w.thread.with(|t| t.plan.as_ref().is_some_and(|plan| plan.reopened)));
}

#[tokio::test]
async fn r3_a_thread_whose_plan_awaits_him_relaunched_still_awaits_him() {
	// 0438, 0494
	let w = world(|_| {});
	let file = proposed_plan_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	assert!(w.awaits("p-1"));
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	let fresh = w.pane();
	// The row moved to the new pane.
	assert!(!w.awaits("p-1"));
	assert!(w.awaits(&fresh));
	assert_eq!(w.row(&fresh).unwrap().text, "demo/Readme line — plan ready");
	assert_eq!(w.shown(), ThreadState::Blocked);
	assert_eq!(
		w.thread.with(|t| t.plan.as_ref().and_then(|plan| plan.awaiting.clone())),
		Some("readme-line".to_owned())
	);
	// herdr was asked to start omp again in the old pane's tab.
	assert_eq!(w.t.herdr.of("respawn").len(), 1);
	assert_eq!(w.t.herdr.of("respawn")[0][0], "p-1");
}

#[tokio::test]
async fn a_relaunch_forgets_the_review_pane_and_the_old_omp() {
	let w = world(|_| {});
	w.thread.with(|t| {
		t.review = Some(ThreadReview { pane: Some("p-9".to_owned()), ..ThreadReview::default() });
		t.omp = Some(crate::frontier::server::thread::ThreadOmp {
			terminal: Some("term".to_owned()),
			shim: None,
			processes: None,
		});
	});
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	// The old omp's terminal is forgotten; `remember` (0490) notes the new pane's, which names none yet.
	assert_eq!(
		w.thread.with(|t| (
			t.review.as_ref().and_then(|r| r.pane.clone()),
			t.omp.as_ref().and_then(|o| o.terminal.clone()),
			t.host
		)),
		(None, None, None)
	);
}

// ── service-regressions.test.ts ─────────────────────────────────────────────────────────────────────────────────────

/// A thread in `phase` is relaunched, then its new omp comes up idle.
async fn relaunched_idle(w: &World, phase: Phase) {
	w.thread.with(|t| {
		set_phase(t, phase, "a test");
		t.review = Some(ThreadReview {
			running: Some(ReviewRunning {
				head: None,
				at: 0,
				commits: "1 new commit".to_owned(),
				range: "main...x".to_owned(),
				checks: None,
				skipped: None,
			}),
			..ThreadReview::default()
		});
	});
	w.change(Working).await;
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	w.change(Idle).await;
}

#[tokio::test]
async fn svc_6_an_omp_started_again_mid_review_goes_back_to_its_task_to_run_the_review_again() {
	// 0582: the review runs again on the same commits (`review.task_ended`): back in its task, then the review step.
	let w = reviewing(|_| {}).await;
	w.thread.with(|t| {
		set_phase(t, Phase::Review, "a test");
	});
	relaunched_idle(&w, Phase::Review).await;
	assert_eq!(w.thread.with(|t| (t.phase, t.cut)), (Phase::Review, None));
	assert_eq!(w.prompts_to(&w.pane()).len(), 1);
}

#[tokio::test]
async fn svc_6_an_omp_started_again_mid_handoff_runs_handoff_again() {
	let w = world(|_| {});
	w.thread.with(|t| {
		set_phase(t, Phase::Handoff, "a test");
	});
	w.change(Working).await;
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	// The cut step is noted and nothing is typed until the new omp is idle.
	assert_eq!(w.thread.with(|t| t.cut), Some(Phase::Handoff));
	assert!(w.prompts_to(&w.pane()).is_empty());
	w.change(Idle).await;
	assert_eq!(w.thread.with(|t| t.cut), None);
}

#[tokio::test]
async fn svc_6_an_omp_started_again_mid_pr_goes_back_to_its_task_and_the_boss_hears_to_finish_it_again() {
	let w = world(|_| {});
	w.thread.with(|t| {
		set_phase(t, Phase::Finish, "a test");
	});
	w.change(Working).await;
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	w.change(Idle).await;
	assert_eq!(w.thread.with(|t| (t.phase, t.cut)), (Phase::Task, None));
	assert_eq!(
		w.boss_lines(),
		["[event] demo/Readme line — no PR: its omp started again mid-turn; everything stays, finish it again"]
	);
}

#[tokio::test]
async fn a_pane_restored_mid_turn_with_its_session_goes_on_with_it_and_notes_no_cut_0549() {
	let w = world(|_| {});
	w.t.ctx.launched.add("p-1", &w.repo.display().to_string(), None).unwrap();
	w.t.ctx.launched.pause("p-1", "session.jsonl").unwrap();
	w.thread.with(|t| {
		set_phase(t, Phase::Review, "a test");
	});
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	assert_eq!(w.thread.with(|t| t.cut), None);
}

#[tokio::test]
async fn svc_9_a_thread_closed_in_quiet_hours_is_in_todays_activity_at_once_and_waits_for_the_boss() {
	// 0298: the line that waited goes to the boss once quiet hours end (the day's alert, which this context has no hook for).
	let clock =
		Arc::new(AtomicI64::new(Local.with_ymd_and_hms(2026, 10, 24, 23, 0, 0).single().unwrap().timestamp_millis()));
	let now = Arc::clone(&clock);
	let w = world(|options| {
		options.day = Some(DayOptions {
			pollers: Some(false),
			now: Some(Arc::new(move || now.load(Ordering::SeqCst))),
			..DayOptions::default()
		});
	});
	fs::create_dir_all(settings_file().parent().unwrap()).unwrap();
	fs::write(settings_file(), r#"{"service":{"quietHours":"22:00-08:00"}}"#).unwrap();
	w.t.ctx.lifecycle.closed(&w.thread).await.unwrap();
	let what = "demo/Readme line — closed; its worktree and branch stay";
	assert!(w.t.ctx.day.dashboard().activity.events.iter().any(|event| event.text == what));
	assert!(w.boss_lines().is_empty());
}

// ── service-tools.test.ts, service-regressions.test.ts (SVC-2) ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_thread_closed_in_herdr_says_its_worktree_and_branch_stay_and_a_blocked_row_goes() {
	let w = world(|_| {});
	let asked = session_file(&w.t, &w.repo, &[("Which format?", 0.1, Some("CSV or JSON?"))]);
	w.change_with(Working, Some(&asked)).await;
	w.change(Blocked).await;
	assert!(w.awaits("p-1"));
	w.boss_idle().await;
	w.t.ctx.lifecycle.closed(&w.thread).await.unwrap();
	assert!(!w.awaits("p-1"));
	assert_eq!(w.said("[event] demo/Readme line — closed;"), 1);
	assert_eq!(w.boss_lines().last().unwrap(), "[event] demo/Readme line — closed; its worktree and branch stay");
	// Said to today's activity as well.
	let what = "demo/Readme line — closed; its worktree and branch stay";
	assert!(w.t.ctx.day.dashboard().activity.events.iter().any(|event| event.text == what));
}

// ── service-plans.test.ts ───────────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn b1_a_plan_omp_writes_after_herdrs_idle_event_is_still_plan_ready() {
	// 0405, 0407: its proposal is in the file only 300 ms after herdr said idle.
	let w = world(|options| options.plan_settle_ms = Some(600));
	let plan_ready = "[event] demo/Readme line — plan ready, awaiting you";
	let file = plan_mode_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	let late = file.clone();
	let writer = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(300)).await;
		append_proposal(&late);
	});
	w.change(Idle).await;
	writer.await.unwrap();
	assert_eq!(w.said(plan_ready), 1);
	assert_eq!(w.said("[event] demo/Readme line — finished its turn"), 0);
	assert!(w.awaits("p-1"));
	assert_eq!(w.shown(), ThreadState::Blocked);
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|p| p.awaiting.clone())), Some("readme-line".to_owned()));
	assert!(w.toasts().contains(&(
		ToastReason::Blocked,
		"demo/Readme line — plan ready, awaiting you".to_owned(),
		Some("p-1".to_owned())
	)));

	// He sends it back; the next proposal comes only after the turn was said finished.
	append(
		&file,
		&[
			json!({"type": "message", "id": "u1", "message": {"role": "user", "timestamp": now(), "content": "Smaller."}}),
			json!({"type": "message", "id": "a1", "message": {"role": "assistant", "content": [{"type": "text", "text": "Revised."}]}}),
		],
	);
	w.change(Working).await;
	assert!(!w.awaits("p-1"));
	w.change(Idle).await;
	w.boss_idle().await;
	assert_eq!(w.said("[event] demo/Readme line — finished its turn"), 1);
	append_proposal(&file);
	// The plan the idle event missed now waits on him, and the boss hears it (`thread_send` asks `awaiting_him`).
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Plan));
	w.boss_idle().await;
	assert_eq!(w.said(plan_ready), 2);
	assert!(w.awaits("p-1"));
}

#[tokio::test]
async fn b1_a_plan_found_while_a_state_change_of_its_pane_is_handled_is_said_once_that_change_ends() {
	// The end-to-end B1 case under load (2026-11-16): `thread_send` found the plan while the turn's end, past its last
	// read of the session, was still being handled; nothing said it until the next refresh.
	let w = world(|options| options.plan_settle_ms = Some(0));
	let plan_ready = "[event] demo/Readme line — plan ready, awaiting you";
	let file = plan_mode_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	w.boss_idle().await;
	assert_eq!(w.said("[event] demo/Readme line — finished its turn"), 1);
	append_proposal(&file);
	// A state change of its pane is being handled.
	w.t.ctx.lifecycle.queues().by_pane.insert(w.pane(), VecDeque::new());
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Plan));
	assert_eq!(w.said(plan_ready), 0);
	// That change ends.
	w.t.ctx.lifecycle.drain(&w.t.ctx, &w.pane()).await;
	assert_eq!(w.said(plan_ready), 1);
	assert!(w.awaits("p-1"));
	assert_eq!(w.shown(), ThreadState::Blocked);
}

#[tokio::test]
async fn p8_a_plan_ready_line_still_waiting_for_the_boss_is_dropped_once_he_approved_the_plan() {
	let w = world(|options| options.plan_settle_ms = Some(100));
	let file = proposed_plan_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	assert!(w.awaits("p-1"));
	assert_eq!(w.said("[event] demo/Readme line — plan ready, awaiting you"), 1);
	// herdr refuses lines into a boss that waits on his answer to its own question, so they wait; the test makes one wait
	// because the boss works, with the hold `plan_ready` gives its line.
	assert!(w.t.ctx.lifecycle.plan_waits(&w.thread));
	let (ctx, thread) = (Arc::clone(&w.t.ctx), w.thread.clone());
	let holds: Holds = Arc::new(move || ctx.lifecycle.plan_waits(&thread));
	w.t.ctx.events.tell("[event] demo/Readme line — plan ready, awaiting you", NORMAL, Some(holds)).await;
	assert_eq!(w.said("[event] demo/Readme line — plan ready, awaiting you"), 1);
	// He approves in Plan Review: omp leaves plan mode and works.
	append(&file, &[json!({"type": "mode_change", "id": "m4", "mode": "none"})]);
	w.change(Working).await;
	assert!(!w.awaits("p-1"));
	assert!(!w.t.ctx.lifecycle.plan_waits(&w.thread));
	w.boss_idle().await;
	// A later line reaches the boss: any plan line still waiting would have gone with or before it.
	w.change(Idle).await;
	assert_eq!(w.said("[event] demo/Readme line — finished its turn"), 1);
	assert_eq!(w.said("[event] demo/Readme line — plan ready, awaiting you"), 1);
}

#[tokio::test]
async fn b3_a_resumed_threads_plan_found_idle_counts_as_awaiting_him_and_gets_plan_review_typed_once() {
	// 0432, 0434, 0438: the plan survives Stop everything and reopen.
	let w = world(|_| {});
	let file = proposed_plan_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	w.t.herdr.restore();
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	let fresh = w.pane();
	append(&file, &[plan_mode_entry("m3")]);
	w.change_with(Idle, Some(&file)).await;
	assert!(w.awaits(&fresh));
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|p| p.awaiting.clone())), Some("readme-line".to_owned()));
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	// The only text typed into it is omp's `/plan-review`, once (0494).
	assert_eq!(w.prompts_to(&fresh), ["/plan-review"]);
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert_eq!(w.prompts_to(&fresh), ["/plan-review"]);
}

const PLAN_REVIEW: &str = "Plan: readme-line\n> Approve and execute\n  Approve and compact context\n  Approve and keep context (~12k / 200k)\n  Refine plan\n  Save and quit\n↑↓ navigate  enter select  esc cancel";
const QUESTION: &str =
	"Which file?\n> README.md\n  docs/intro.md\n  Other (type your own)\n↑↓ navigate  enter select  esc cancel";
const FORM_OTHER_ROW: &str = "╭─ Ask ──────────────────────────────────────────╮\n│ Session store                                  │\n│   ◉ SQLite: one file, no new service           │\n│   ○ Redis: shared across instances, needs ops  │\n│   ○ Other: type your own                       │";
const FORM_FOOTER: &str = "│   Submit                                       │\n├────────────────────────────────────────────────┤\n│ ⏎ choose · ↑/↓ move · ⇥ next field · ⎋ cancel  │\n╰────────────────────────────────────────────────╯";

/// A thread whose omp left `session` through Stop everything and a reopen (as B3): its pane herdr restored as a shell,
/// its new omp resumed with `--continue` and idle, its screen `screen`.
async fn reopened(session: fn(&TestContext, &Path) -> std::path::PathBuf, screen: &str) -> (World, String) {
	let w = world(|options| options.plan_settle_ms = Some(100));
	let file = session(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	w.t.herdr.restore();
	w.t.ctx.lifecycle.relaunch(&w.thread).await.unwrap();
	let fresh = w.pane();
	// The resumed omp re-enters plan mode in its session.
	append(&file, &[plan_mode_entry("m3")]);
	w.t.herdr.state().screens.insert(fresh.clone(), screen.to_owned());
	w.change_with(Idle, Some(&file)).await;
	(w, fresh)
}

fn screen_reads(w: &World, pane: &str) -> usize {
	w.calls_on(pane).iter().filter(|method| *method == "screen").count()
}

#[tokio::test]
async fn a_reopened_thread_whose_plan_awaits_him_gets_plan_review_typed_once_after_its_screen_shows_no_picker() {
	// 0494
	let (w, fresh) = reopened(proposed_plan_file, "plan mode  ▸ ").await;
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert_eq!(w.prompts_to(&fresh), ["/plan-review"]);
	// It read the screen before typing.
	let calls = w.calls_on(&fresh);
	let (screen, prompt) = (calls.iter().position(|m| m == "screen"), calls.iter().position(|m| m == "prompt"));
	assert!(screen.unwrap() < prompt.unwrap());
	// Never a second time, even with the screen clear and omp idle again.
	w.t.herdr.state().screens.insert(fresh.clone(), String::new());
	w.change(Working).await;
	w.change(Idle).await;
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert_eq!(w.prompts_to(&fresh).len(), 1);
}

#[tokio::test]
async fn nothing_is_typed_into_a_reopened_thread_while_omps_plan_review_shows_not_after_it_goes_either() {
	// 0494
	let (w, fresh) = reopened(proposed_plan_file, PLAN_REVIEW).await;
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert_eq!(screen_reads(&w, &fresh), 1);
	assert!(w.awaits(&fresh));
	// The picker already open ends the wait.
	assert_eq!(w.thread.with(|t| t.plan.as_ref().map(|plan| plan.reopened)), Some(false));
	w.t.herdr.state().screens.insert(fresh.clone(), String::new());
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert!(w.prompts_to(&fresh).is_empty());
}

#[tokio::test]
async fn nothing_is_typed_into_a_reopened_thread_while_a_question_shows_once_it_is_answered_the_plans_review_comes_back()
 {
	// 0494, 0589: omp 18.18.2's question picker, an ask form's Other row, an ask form's footer.
	for screen in [QUESTION, FORM_OTHER_ROW, FORM_FOOTER] {
		let (w, fresh) = reopened(proposed_plan_file, screen).await;
		for _ in 0..3 {
			w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
		}
		assert_eq!(screen_reads(&w, &fresh), 3);
		assert!(w.awaits(&fresh));
		assert!(w.prompts_to(&fresh).is_empty());
		w.t.herdr.state().screens.insert(fresh.clone(), String::new());
		w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
		assert_eq!(w.prompts_to(&fresh), ["/plan-review"]);
	}
}

#[tokio::test]
async fn a_reopened_thread_whose_plan_isnt_awaiting_him_gets_nothing_typed() {
	// 0494: the reopened omp is idle in plan mode with no proposal: no plan to show, and its screen isn't read.
	let (w, fresh) = reopened(plan_mode_file, "").await;
	assert!(!w.awaits(&fresh));
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|plan| plan.awaiting.clone())), None);
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert!(w.prompts_to(&fresh).is_empty());
	assert!(w.t.herdr.of("screen").is_empty());
}

#[tokio::test]
async fn reopening_plan_review_waits_for_an_idle_omp_with_no_state_change_being_handled() {
	let (w, fresh) = reopened(proposed_plan_file, "").await;
	w.thread.with(|t| t.state = Working);
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	w.thread.with(|t| t.state = Idle);
	// A change of the pane waiting in its queue holds it off.
	w.t.ctx.lifecycle.state_changed(&fresh, Working, StateSource::Herdr);
	assert!(w.t.ctx.lifecycle.busy(&fresh));
	w.t.ctx.lifecycle.reopen_plan_review(&w.thread).await.unwrap();
	assert!(w.t.herdr.of("screen").is_empty());
	w.t.ctx.lifecycle.settled().await;
}

#[tokio::test]
async fn p6_b6_work_the_bosss_thread_send_started_isnt_said_as_his_rejection_work_he_started_is() {
	// 0432, 0438, 0471
	let reviewed = |sent_at: Option<i64>| ReviewThen { at: now() - 1_000, sent_at };
	struct ReviewThen {
		at: i64,
		sent_at: Option<i64>,
	}
	for (case, relay) in [("relayed", reviewed(Some(now()))), ("his", reviewed(None))] {
		let w = world(|_| {});
		w.thread.with(|t| {
			t.review = Some(ThreadReview {
				result: Some(ReviewResult {
					notes: Some(0),
					blocking: Some(0),
					tests: TestsResult::Pass,
					proof: None,
					checks: Vec::new(),
					at: Some(relay.at),
				}),
				..ThreadReview::default()
			});
			t.sent_at = relay.sent_at;
		});
		w.change(Working).await;
		let line = if case == "relayed" {
			"[event] demo/Readme line — at work again on its reviewed work, after your thread_send · offer once to save a lesson if he rejected that work"
		} else {
			"[event] demo/Readme line — he sent its reviewed work back · offer once to save a lesson"
		};
		assert_eq!(w.boss_lines(), [line], "{case}");
		// Said once: the thread's result no longer counts as waiting on him.
		assert_eq!(w.thread.with(|t| t.review.as_ref().and_then(|r| r.result.as_ref()).map(|r| r.at)), Some(None));
		w.change(Idle).await;
		w.change(Working).await;
		assert_eq!(w.said("[event] demo/Readme line — at work again"), usize::from(case == "relayed"));
	}
}

#[tokio::test]
async fn the_review_and_handoff_turns_keep_the_finished_tasks_place_in_today() {
	// 0394, 0400
	let w = world(|_| {});
	w.thread.with(|t| {
		set_phase(t, Phase::Review, "a test");
		show(t, ThreadState::Blocked);
	});
	w.change(Working).await;
	assert_eq!(w.shown(), ThreadState::Done);
	w.thread.with(|t| show(t, ThreadState::Done));
	w.change(Idle).await;
	w.change(Working).await;
	assert_eq!(w.shown(), ThreadState::Done);
}

// ── service-allow.test.ts (0471) ────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_commit_by_hand_on_an_idle_threads_branch_reaches_the_boss_once_with_the_lesson_offer() {
	// 0471
	let w = world(|_| {});
	let hand = "[event] demo/Readme line — he committed by hand on its branch (1 commit) · offer once to save a lesson";
	commit_in(&w.repo, "demo\nBy hand.\n", "By hand");
	w.t.ctx.lifecycle.hand_commits(&w.by_id()).await.unwrap();
	assert_eq!(w.boss_lines(), [hand]);
	assert_eq!(w.thread.with(|t| t.head.clone()), Some(head_of(&w.repo)));
	w.boss_idle().await;
	w.t.ctx.lifecycle.hand_commits(&w.by_id()).await.unwrap();
	assert_eq!(w.said(hand), 1);
}

#[tokio::test]
async fn commits_by_hand_are_counted_and_a_reset_is_no_commit() {
	let w = world(|_| {});
	commit_in(&w.repo, "demo\nOne.\n", "One");
	commit_in(&w.repo, "demo\nOne.\nTwo.\n", "Two");
	w.t.ctx.lifecycle.hand_commits(&w.by_id()).await.unwrap();
	assert_eq!(
		w.boss_lines(),
		["[event] demo/Readme line — he committed by hand on its branch (2 commits) · offer once to save a lesson"]
	);
	w.boss_idle().await;
	// Reset away, not a commit: the HEAD is the thread's, and nothing is said.
	sh(&w.repo, &["reset", "-q", "--hard", "HEAD~1"]);
	w.t.ctx.lifecycle.hand_commits(&w.by_id()).await.unwrap();
	assert_eq!(w.thread.with(|t| t.head.clone()), Some(head_of(&w.repo)));
	assert_eq!(w.boss_lines().len(), 1);
}

#[tokio::test]
async fn commits_on_a_thread_that_is_not_quiet_are_left_alone() {
	// 0471: busy, in a review or handoff, waiting on a plan or found in herdr, or its pane at work.
	let w = world(|_| {});
	let was = w.thread.with(|t| t.head.clone());
	commit_in(&w.repo, "demo\nBy hand.\n", "By hand");
	let lifecycle = &w.t.ctx.lifecycle;
	let mut working = w.by_id();
	working.get_mut("p-1").unwrap().agent_state = Working;
	lifecycle.hand_commits(&working).await.unwrap();
	w.thread.with(|t| set_phase(t, Phase::Review, "a test"));
	lifecycle.hand_commits(&w.by_id()).await.unwrap();
	w.thread.with(|t| {
		t.phase = Phase::Task;
		plan_of(t).awaiting = Some("plan".to_owned());
	});
	lifecycle.hand_commits(&w.by_id()).await.unwrap();
	w.thread.with(|t| {
		plan_of(t).awaiting = None;
		t.catch_up = true;
	});
	lifecycle.hand_commits(&w.by_id()).await.unwrap();
	assert_eq!(w.thread.with(|t| t.head.clone()), was);
	assert!(w.boss_lines().is_empty());
}

#[tokio::test]
async fn a_plan_he_sent_back_reaches_the_boss_once_with_the_lesson_offer() {
	// 0471: Plan Review's "Refine plan" sends his message while the plan waits.
	let w = world(|_| {});
	let file = session_file(&w.t, &w.repo, &[("I wrote the plan.", 0.1, None)]);
	append(
		&file,
		&[
			json!({"type": "message", "id": "t1", "message": {"role": "toolResult", "toolName": "write", "timestamp": now(),
				"details": {"xdev": {"tool": "propose", "mode": "execute", "inner": {"title": "Plan", "planFilePath": "p.md"}}}}}),
			json!({"type": "message", "id": "u1", "message": {"role": "user", "timestamp": now() + 1, "content": "Smaller."}}),
		],
	);
	let sent_back = "[event] demo/Readme line — he sent its plan back · offer once to save a lesson";
	w.change_with(Working, Some(&file)).await;
	assert_eq!(w.boss_lines(), [sent_back]);
	w.boss_idle().await;
	w.change(Idle).await;
	assert_eq!(w.said(sent_back), 1);
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|plan| plan.sent_back)), Some(1));
}

// ── service-finish.test.ts (CR-09) ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_task_end_whose_idle_event_herdr_lost_is_caught_on_the_refresh_tick_one_pushed_after_the_read_isnt_undone() {
	// CR-09: without ask_report, the task end is noted as soon as the reconcile gives the idle state.
	let w = world(|options| options.ask_report = Some(false));
	w.change(Working).await;
	commit_in(&w.repo, "demo\nA line.\n", "Add a README line");
	// herdr's idle event never arrives.
	w.t.herdr.set_state("p-1", Idle, None, false);
	let read_at = now() + 100;
	w.t.ctx.lifecycle.reconcile(&w.by_id(), read_at).await.unwrap();
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(
		w.thread.with(|t| (t.state, t.head.clone(), t.shown)),
		(Idle, Some(head_of(&w.repo)), ThreadState::Done)
	);
	// A state pushed after the read isn't undone by it: the read is older than the event.
	w.t.herdr.set_state("p-1", Working, None, false);
	w.t.ctx.lifecycle.state_changed("p-1", Idle, StateSource::Herdr);
	w.t.ctx.lifecycle.settled().await;
	w.t.herdr.set_state("p-1", Working, None, false);
	w.t.ctx.lifecycle.reconcile(&w.by_id(), now() - 60_000).await.unwrap();
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Idle);
}

// ── added: what the TypeScript's service tests reach through the whole service ─────────────────────────────────────

#[tokio::test]
async fn a_task_end_without_its_report_gets_one_steer_asking_for_it_and_the_next_end_goes_to_review() {
	// 0539
	let w = reviewing(|_| {}).await;
	let began = w.thread.with(|t| t.head.clone());
	w.change(Working).await;
	commit_in(&w.repo, "demo\nA line.\n", "Add a README line");
	w.change(Idle).await;
	assert_eq!(w.prompts_to("p-1"), [REPORT_STEER]);
	// HEAD stays where the task began, so that end still counts the task's commits.
	assert_eq!(w.thread.with(|t| (t.head.clone(), t.report_asked)), (began, true));
	w.change(Working).await;
	w.change(Idle).await;
	let prompts = w.prompts_to("p-1");
	assert_eq!(prompts.len(), 2);
	assert_eq!(prompts[0], REPORT_STEER);
	assert!(prompts[1].contains("main...feature/p-1"), "the review turn: {}", prompts[1]);
	assert_eq!(
		w.thread.with(|t| (t.head.clone(), t.report_asked, t.shown)),
		(Some(head_of(&w.repo)), false, ThreadState::Done)
	);
}

#[tokio::test]
async fn a_task_end_with_the_report_steer_off_goes_straight_to_review() {
	// 0576
	let w = reviewing(|options| options.ask_report = Some(false)).await;
	w.change(Working).await;
	commit_in(&w.repo, "demo\nA line.\n", "Add a README line");
	w.change(Idle).await;
	let prompts = w.prompts_to("p-1");
	assert_eq!(prompts.len(), 1);
	assert!(prompts[0].contains("main...feature/p-1"), "the review turn: {}", prompts[0]);
	assert_eq!(w.thread.with(|t| (t.head.clone(), t.shown)), (Some(head_of(&w.repo)), ThreadState::Done));
}

#[tokio::test]
async fn a_run_end_with_no_new_commit_is_a_turn_said_with_its_last_line() {
	let w = world(|_| {});
	let file = session_file(&w.t, &w.repo, &[("All clear.\nSecond line.", 0.1, None)]);
	w.change_with(Working, Some(&file)).await;
	w.change(Idle).await;
	assert_eq!(w.boss_lines(), ["[event] demo/Readme line — finished its turn: «All clear.»"]);
	assert_eq!(w.shown(), ThreadState::Idle);
}

#[tokio::test]
async fn states_are_handled_one_after_another_in_the_order_herdr_sent_them() {
	// CR-09
	let w = world(|_| {});
	let asked = session_file(&w.t, &w.repo, &[("Which format?", 0.1, Some("CSV or JSON?"))]);
	w.t.herdr.set_state("p-1", Working, asked.to_str(), false);
	let lifecycle = &w.t.ctx.lifecycle;
	lifecycle.state_changed("p-1", Working, StateSource::Herdr);
	lifecycle.state_changed("p-1", Blocked, StateSource::Herdr);
	lifecycle.state_changed("p-1", Idle, StateSource::Herdr);
	assert!(lifecycle.busy("p-1"));
	lifecycle.settled().await;
	assert!(!lifecycle.busy("p-1"));
	assert_eq!(w.thread.with(|t| t.state), Idle);
	assert!(!w.awaits("p-1"));
	assert!(w.row("p-1").is_some_and(|row| row.resolved_at.is_some()));
	w.boss_idle().await;
	assert_eq!(w.said("[event] demo/Readme line — awaiting you"), 1);
	assert_eq!(w.said("[event] demo/Readme line — finished its turn"), 1);
}

#[tokio::test]
async fn a_state_change_stamps_when_herdr_pushed_it_and_a_reconcile_read_doesnt() {
	let w = world(|_| {});
	let lifecycle = &w.t.ctx.lifecycle;
	let boss = w.t.ctx.boss().unwrap();
	lifecycle.state_changed("p-1", Working, StateSource::Reconcile);
	lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.event_at), None);
	let before = now();
	lifecycle.state_changed("p-1", Idle, StateSource::Host);
	lifecycle.state_changed("boss-1", Working, StateSource::Herdr);
	lifecycle.settled().await;
	assert!(w.thread.with(|t| t.event_at).unwrap() >= before);
	assert!(boss.with(|b| b.event_at).unwrap() >= before);
	assert_eq!(boss.with(|b| b.state), Working);
}

#[tokio::test]
async fn nothing_is_handled_once_the_service_stops_and_unknown_panes_are_ignored() {
	let w = world(|_| {});
	w.t.ctx.lifecycle.state_changed("p-404", Working, StateSource::Herdr);
	w.t.ctx.set_stopping();
	w.change(Working).await;
	assert_eq!(w.thread.with(|t| t.state), Idle);
}

#[tokio::test]
async fn herdrs_state_doesnt_override_a_thread_whose_omp_host_answers_unless_it_says_no_agent_runs() {
	// 0547, 0561
	let w = world(|_| {});
	w.thread.with(|t| t.host = Some(Idle));
	w.t.ctx.lifecycle.herdr_state("p-1", Working);
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| (t.state, t.host)), (Idle, Some(Idle)));
	w.t.ctx.lifecycle.herdr_state("p-1", Unknown);
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| (t.state, t.host)), (Unknown, None));
	w.t.ctx.lifecycle.herdr_state("p-1", Working);
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Working);
}

#[tokio::test]
async fn a_reconcile_changes_a_pane_whose_state_differs_unless_its_host_answers_or_an_event_came_since_the_read() {
	// CR-09, 0547
	let w = world(|_| {});
	let lifecycle = &w.t.ctx.lifecycle;
	w.t.herdr.set_state("p-1", Working, None, false);
	// An event after the read: left alone.
	w.thread.with(|t| t.event_at = Some(now() + 10_000));
	lifecycle.reconcile(&w.by_id(), now()).await.unwrap();
	lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Idle);
	// Its host answers: omp's own state isn't overridden.
	w.thread.with(|t| {
		t.event_at = None;
		t.host = Some(Idle);
	});
	lifecycle.reconcile(&w.by_id(), now()).await.unwrap();
	lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Idle);
	w.thread.with(|t| t.host = None);
	lifecycle.reconcile(&w.by_id(), now()).await.unwrap();
	lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Working);
	// herdr's `done` is idle.
	w.t.herdr.set_state("p-1", AgentState::Done, None, false);
	w.thread.with(|t| t.state = Idle);
	lifecycle.reconcile(&w.by_id(), now()).await.unwrap();
	lifecycle.settled().await;
	assert_eq!(w.thread.with(|t| t.state), Idle);
}

#[tokio::test]
async fn a_reconcile_leaves_the_boss_pane_just_given_a_line_alone_a_moment() {
	let w = world(|_| {});
	let boss = w.t.ctx.boss().unwrap();
	add_pane(&w.t, "boss-1", Working);
	let read_at = now();
	// A line went in 1 s before the read: herdr shows it idle until omp starts on the line.
	boss.with(|b| {
		b.state = Idle;
		b.prompted_at = Some(read_at - 1_000);
	});
	w.t.ctx.lifecycle.reconcile(&w.by_id(), read_at).await.unwrap();
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(boss.with(|b| b.state), Idle);
	boss.with(|b| b.prompted_at = Some(read_at - 20_000));
	w.t.ctx.lifecycle.reconcile(&w.by_id(), read_at).await.unwrap();
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(boss.with(|b| b.state), Working);
}

#[tokio::test]
async fn an_idle_boss_gets_what_waits_for_it_when_herdr_says_so() {
	let w = world(|_| {});
	let boss = w.t.ctx.boss().unwrap();
	boss.with(|b| b.state = Working);
	w.t.ctx.events.tell("[event] later", NORMAL, None).await;
	assert!(w.prompts_to("boss-1").is_empty());
	w.t.ctx.lifecycle.state_changed("boss-1", Idle, StateSource::Herdr);
	w.t.ctx.lifecycle.settled().await;
	assert_eq!(w.prompts_to("boss-1"), ["[event] later"]);
}

#[tokio::test]
async fn awaiting_him_names_a_question_a_plan_or_nothing_and_finds_a_plan_the_service_missed() {
	// B1
	let w = world(|_| {});
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), None);
	// herdr says it is blocked.
	w.t.herdr.set_state("p-1", Blocked, None, false);
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Question));
	w.t.herdr.set_state("p-1", Idle, None, false);
	// Its session shows a plan no state change reported: found, and said.
	let file = proposed_plan_file(&w.t, &w.repo);
	w.t.herdr.set_state("p-1", Idle, file.to_str(), false);
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Plan));
	assert_eq!(w.said("[event] demo/Readme line — plan ready, awaiting you"), 1);
	assert!(w.awaits("p-1"));
	// Counted from now on, with no second line.
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Plan));
	assert_eq!(w.said("[event] demo/Readme line — plan ready, awaiting you"), 1);
	// A thread blocked by its own state.
	w.thread.with(|t| t.state = Blocked);
	assert_eq!(w.t.ctx.lifecycle.awaiting_him(&w.thread).await.unwrap(), Some(Awaiting::Question));
}

#[tokio::test]
async fn a_plan_found_is_counted_quietly_unless_announced_and_left_to_a_busy_or_working_thread() {
	let w = world(|_| {});
	let lifecycle = &w.t.ctx.lifecycle;
	// Not idle, or not in its task phase: left alone.
	w.thread.with(|t| t.state = Working);
	lifecycle.plan_found(&w.thread, "Plan", true).await.unwrap();
	assert!(!w.awaits("p-1"));
	w.thread.with(|t| {
		t.state = Idle;
		set_phase(t, Phase::Review, "a test");
	});
	lifecycle.plan_found(&w.thread, "Plan", true).await.unwrap();
	assert!(!w.awaits("p-1"));
	w.thread.with(|t| t.phase = Phase::Task);
	// Counted, not said (a thread found in herdr).
	lifecycle.plan_found(&w.thread, "Plan", false).await.unwrap();
	assert!(w.awaits("p-1"));
	assert!(w.boss_lines().is_empty());
	assert_eq!(w.shown(), ThreadState::Blocked);
	// Already awaiting: nothing again.
	lifecycle.plan_found(&w.thread, "Other", true).await.unwrap();
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|p| p.awaiting.clone())), Some("Plan".to_owned()));
}

fn approved_world() -> World {
	let w = world(|_| {});
	w.thread.with(|t| {
		t.task = Some(TaskRecord::default());
		plan_of(t).proposed = Some(PlanProposed { title: "readme-line".to_owned(), file: Some("/plans/p.md".into()) });
	});
	w
}

fn plan_of_task(w: &World) -> Option<(String, Option<String>)> {
	w.thread.with(|t| t.task.as_ref().and_then(|task| task.plan.as_ref()).map(|p| (p.title.clone(), p.file.clone())))
}

#[tokio::test]
async fn the_task_files_approved_plan_is_the_last_proposed_once_the_session_is_out_of_plan_mode() {
	// 0540
	let w = approved_world();
	let lifecycle = &w.t.ctx.lifecycle;
	let stats = |plan_mode: bool, awaiting: Option<&str>| SessionStats {
		plan_mode,
		plan_awaiting: awaiting.map(str::to_owned),
		plan_file: Some("/plans/stats.md".to_owned()),
		..SessionStats::default()
	};
	// Refining keeps it in plan mode; a plan waiting isn't approved; no session read says nothing.
	lifecycle.note_approval(&w.thread, Some(&stats(true, None)));
	lifecycle.note_approval(&w.thread, Some(&stats(false, Some("readme-line"))));
	lifecycle.note_approval(&w.thread, None);
	assert_eq!(plan_of_task(&w), None);
	// Still awaiting him in his record: not approved either.
	w.thread.with(|t| plan_of(t).awaiting = Some("readme-line".to_owned()));
	lifecycle.note_approval(&w.thread, Some(&stats(false, None)));
	assert_eq!(plan_of_task(&w), None);
	w.thread.with(|t| plan_of(t).awaiting = None);
	let before = now();
	lifecycle.note_approval(&w.thread, Some(&stats(false, None)));
	assert_eq!(plan_of_task(&w), Some(("readme-line".to_owned(), Some("/plans/p.md".to_owned()))));
	assert!(w.thread.with(|t| t.task.as_ref().unwrap().plan.as_ref().unwrap().approved_at) >= before);
	assert_eq!(w.thread.with(|t| t.plan.as_ref().and_then(|p| p.proposed.clone())), None);
}

#[tokio::test]
async fn an_approved_plan_without_a_file_takes_the_sessions_and_a_thread_without_a_task_records_nothing() {
	let w = approved_world();
	w.thread.with(|t| plan_of(t).proposed = Some(PlanProposed { title: "readme-line".to_owned(), file: None }));
	let stats = SessionStats { plan_file: Some("/plans/stats.md".to_owned()), ..SessionStats::default() };
	w.t.ctx.lifecycle.note_approval(&w.thread, Some(&stats));
	assert_eq!(plan_of_task(&w), Some(("readme-line".to_owned(), Some("/plans/stats.md".to_owned()))));
	drop(w);
	let w = approved_world();
	w.thread.with(|t| t.task = None);
	w.t.ctx.lifecycle.note_approval(&w.thread, Some(&stats));
	assert!(w.thread.with(|t| t.plan.as_ref().and_then(|p| p.proposed.clone())).is_some());
}

#[tokio::test]
async fn a_thread_that_is_approved_through_a_state_change_records_its_plan_in_the_task_file() {
	let w = approved_world();
	let file = proposed_plan_file(&w.t, &w.repo);
	w.change_with(Working, Some(&file)).await;
	append(&file, &[json!({"type": "mode_change", "id": "m4", "mode": "none"})]);
	w.thread.with(|t| plan_of(t).awaiting = None);
	w.change(Idle).await;
	assert_eq!(plan_of_task(&w).map(|(title, _)| title), Some("readme-line".to_owned()));
}
