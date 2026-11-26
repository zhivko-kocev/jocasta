//! The cases of `test/frontier/server/service-plans.test.ts` on the real server: plans awaiting him, threads reopened
//! after Stop everything and the review lines the boss hears.

use std::fs::OpenOptions;
use std::io::Write;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use serde_json::{Value, json};

use crate::frontier::server::bridge::PaneAgent;
use crate::frontier::server::helpers::{
	BRIEF, TestServer, ThreadOptions, append_proposal, boss_idle, boss_lines, boss_pane, call, commit_in, demo_project,
	handoff_entry, plan_mode_file, proposed_plan_file, review, sh, start_test_server_with, start_thread, to_handoff,
	until, until_async, with_server,
};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::status::Busy;
use crate::frontier::server::surface::AgentState;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::frontier::server::tools::ToolReply;
use crate::frontier::server::wording::HANDOFF_PROMPT;
use crate::shared::testing::temp_jocasta;

/// Runs `run` on `server` and always stops it afterwards (the TypeScript's `try { … } finally { server.stop() }`).
async fn finally<T, F, Fut>(server: &TestServer, run: F) -> T
where
	F: FnOnce(TestServer) -> Fut,
	Fut: Future<Output = T>,
{
	let outcome = AssertUnwindSafe(run(server.clone())).catch_unwind().await;
	server.stop().await;
	match outcome {
		Ok(value) => value,
		Err(panic) => std::panic::resume_unwind(panic),
	}
}

fn append_entry(file: &Path, entry: &Value) {
	let mut out = OpenOptions::new().append(true).open(file).unwrap();
	writeln!(out, "{entry}").unwrap();
}

fn now_iso() -> String {
	chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn path_text(file: &Path) -> &str {
	file.to_str().expect("a UTF-8 path")
}

/// A server on `herdr` with `agent_wait_ms` and `plan_settle_ms` as given.
async fn start(herdr: &Arc<FakeSurface>, agent_wait_ms: Option<i64>, plan_settle_ms: Option<i64>) -> TestServer {
	let options = ServerOptions { agent_wait_ms, plan_settle_ms, ..ServerOptions::default() };
	start_test_server_with(Arc::clone(herdr), options).await
}

fn busy(server: &TestServer) -> Busy {
	server.service.busy_threads().unwrap()
}

fn awaiting(server: &TestServer) -> u32 {
	server.service.hud(None).unwrap().awaiting
}

fn typed(herdr: &FakeSurface, pane_id: &str) -> Vec<Vec<Value>> {
	herdr.of("prompt").into_iter().filter(|args| args[0] == json!(pane_id)).collect()
}

fn said(server: &TestServer, line: &str) -> usize {
	boss_lines(server).iter().filter(|l| l.starts_with(line)).count()
}

/// How many times the thread's plan was said ready: a notification, never a boss line (0695).
fn plan_ready_noted(server: &TestServer) -> usize {
	let note = json!({ "title": "demo/Readme line", "body": "plan ready, awaiting you", "sound": "request" });
	server.herdr.of("notify").iter().filter(|args| args[0] == note).count()
}

fn count_prompts(herdr: &FakeSurface, pane_id: &str, text: &str) -> usize {
	herdr.of("prompt").into_iter().filter(|args| args[0] == json!(pane_id) && args[1] == json!(text)).count()
}

fn plan_mode_entry(id: &str) -> Value {
	json!({"type": "mode_change", "id": id, "mode": "plan", "data": {"planFilePath": "local://readme-line-plan.md"}})
}

fn fresh_pane(herdr: &FakeSurface) -> String {
	herdr
		.state()
		.pane_list
		.iter()
		.find(|p| p.cwd.as_ref().is_some_and(|cwd| cwd.to_string_lossy().contains("readme-line")))
		.map(|p| p.pane_id.clone())
		.expect("the thread's pane")
}

#[tokio::test(flavor = "multi_thread")]
async fn b1_a_plan_omp_writes_after_herdrs_idle_event_is_still_plan_ready_and_thread_send_never_types_into_a_plan_awaiting_him_boss_manual_rule_4_0405_0407()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		server.herdr.set_state(&boss_pane(&server.herdr).unwrap(), AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let plan_ready = "[event] demo/Readme line — plan ready";
		let finished = "[event] demo/Readme line — finished its turn";

		// The turn was read when it started; its proposal is in the file only 300 ms after herdr said idle.
		let file = plan_mode_file(&path);
		server.herdr.set_state(&pane_id, AgentState::Working, Some(path_text(&file)), true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		// The race itself: the proposal is written on the real clock after the idle event, as omp's writer did.
		tokio::time::sleep(Duration::from_millis(300)).await;
		append_proposal(&file);
		until(|| plan_ready_noted(&server) == 1).await;
		assert_eq!(said(&server, plan_ready), 0);
		assert_eq!(said(&server, finished), 0);
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });

		// He sends it back; the next proposal comes only after the turn was said finished.
		append_entry(
			&file,
			&json!({ "type": "message", "id": "u1", "message": { "role": "user", "timestamp": chrono::Utc::now().timestamp_millis(), "content": "Smaller." } }),
		);
		append_entry(
			&file,
			&json!({ "type": "message", "id": "a1", "message": { "role": "assistant", "content": [{ "type": "text", "text": "Revised." }] } }),
		);
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| {
			boss_idle(&server);
			said(&server, finished) == 1
		})
		.await;
		append_proposal(&file);
		let before = typed(&server.herdr, &pane_id).len();
		let reply = call(&server, "thread_send", &json!({ "thread": "Readme line", "text": "Carry on and show him a plan." })).await;
		assert_eq!(
			reply,
			ToolReply {
				text: "demo/Readme line's plan is ready and awaiting his approval; he decides in its pane.".to_owned(),
				is_error: Some(true),
			}
		);
		assert_eq!(typed(&server.herdr, &pane_id).len(), before);
		// The plan the idle event missed now waits on him, and the boss hears it.
		until(|| {
			boss_idle(&server);
			plan_ready_noted(&server) == 2
		})
		.await;
		assert_eq!(said(&server, plan_ready), 0);
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn b3_a_plan_awaiting_him_survives_stop_everything_and_reopen_the_resumed_thread_counts_on_the_hud_in_awaiting_you_and_in_the_close_question_0432_0434_0438()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeSurface::new());
	let server = start(&herdr, Some(10_000), None).await;
	let file = finally(&server, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let file = proposed_plan_file(&started.path);
		server.herdr.set_state(&started.pane_id, AgentState::Working, Some(path_text(&file)), true);
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| busy(&server).awaiting == 1).await;
		server.service.refresh().await;
		file
	})
	.await;
	// herdr comes back with every pane a shell; the thread's omp resumes with `--continue`, back in plan mode.
	herdr.restore();
	let server = start(&herdr, None, None).await;
	finally(&server, |server| async move {
		let fresh = fresh_pane(&herdr);
		append_entry(&file, &plan_mode_entry("m3"));
		server.bridge.record(|recording| recording.agents.insert(fresh.clone(), PaneAgent::default()));
		herdr.set_state(&fresh, AgentState::Idle, Some(path_text(&file)), true);
		until_async(|| async {
			if busy(&server).awaiting != 0 {
				return true;
			}
			server.service.refresh().await;
			false
		})
		.await;
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });
		assert_eq!(awaiting(&server), 1);
		assert_eq!(
			call(&server, "thread_status", &json!({})).await.text,
			"demo/Readme line [feature/readme-line] plan ready\n  spend $0.25"
		);
		let sent = call(&server, "thread_send", &json!({ "thread": "Readme line", "text": "Go." })).await;
		assert_eq!(sent.is_error, Some(true));
		// The only text typed into it is omp's `/plan-review`, once (0494).
		assert_eq!(typed(&herdr, &fresh), vec![vec![json!(fresh), json!("/plan-review")]]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_done_and_reviewed_thread_survives_stop_everything_and_reopen_still_in_awaiting_you_on_the_hud_and_in_the_close_question_0432_0549()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeSurface::new());
	let server = start(&herdr, Some(10_000), None).await;
	let file = finally(&server, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let file = to_handoff(&server, &started.pane_id, &started.path).await;
		handoff_entry(&file);
		until(|| awaiting(&server) == 1).await;
		server.service.refresh().await;
		file
	})
	.await;
	// herdr comes back with every pane a shell; the thread's omp starts again, idle.
	herdr.restore();
	let server = start(&herdr, None, None).await;
	finally(&server, |server| async move {
		let fresh = fresh_pane(&herdr);
		herdr.set_state(&fresh, AgentState::Idle, Some(path_text(&file)), true);
		server.service.refresh().await;
		server.service.refresh().await;
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });
		assert_eq!(awaiting(&server), 1);
		let status = call(&server, "thread_status", &json!({})).await.text;
		assert!(status.starts_with("demo/Readme line [feature/readme-line] done"), "{status}");
	})
	.await;
}

/// A thread whose omp left `session` (a plan proposed, or plan mode only) through Stop everything and a reopen (as B3):
/// herdr restores its pane as a shell, the thread's omp resumes with `--continue` and goes idle, its bridge reporting `agent`.
struct Reopened {
	server: TestServer,
	herdr: Arc<FakeSurface>,
	fresh: String,
}

async fn reopened(root: &Path, session: fn(&Path) -> PathBuf, agent: PaneAgent) -> Reopened {
	demo_project(root, "demo").await;
	let herdr = Arc::new(FakeSurface::new());
	// A plan-mode turn's end reads the session again for 200 ms, not the product's 2 s (TST-5, TST-7).
	let server = start(&herdr, Some(10_000), Some(200)).await;
	let file = finally(&server, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		let file = session(&started.path);
		server.herdr.set_state(&started.pane_id, AgentState::Working, Some(path_text(&file)), true);
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		server.service.refresh().await;
		file
	})
	.await;
	herdr.restore();
	let server = start(&herdr, None, Some(200)).await;
	let fresh = fresh_pane(&herdr);
	// The resumed omp re-enters plan mode in its session.
	append_entry(&file, &plan_mode_entry("m3"));
	server.bridge.record(|recording| recording.agents.insert(fresh.clone(), agent));
	herdr.set_state(&fresh, AgentState::Idle, Some(path_text(&file)), true);
	Reopened { server, herdr, fresh }
}

impl Reopened {
	/// Runs the service's refresh tick until `check` passes.
	async fn look_until(&self, check: impl Fn() -> bool) {
		until_async(|| async {
			self.server.service.refresh().await;
			check()
		})
		.await;
	}

	/// Runs the service's refresh tick `times` times.
	async fn look(&self, times: usize) {
		for _ in 0..times {
			self.server.service.refresh().await;
		}
	}

	fn typed(&self) -> Vec<Vec<Value>> {
		typed(&self.herdr, &self.fresh)
	}

	/// Whether the reopened plan was looked at: the thread no longer waits for its review to be reopened.
	fn looked(&self) -> bool {
		let threads = self.server.service.context().threads.all();
		!threads.is_empty() && threads.iter().all(|t| t.with(|t| t.plan.as_ref().is_some_and(|p| !p.reopened)))
	}

	/// The pane's omp reports `agent` over its bridge.
	fn report(&self, agent: PaneAgent) {
		self.server.bridge.record(|recording| recording.agents.insert(self.fresh.clone(), agent));
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_thread_whose_plan_awaits_him_gets_plan_review_typed_once_when_its_omp_shows_no_dialog_0494() {
	let j = temp_jocasta();
	let r = reopened(j.root(), proposed_plan_file, PaneAgent::default()).await;
	finally(&r.server, |_| async {
		r.look_until(|| !r.typed().is_empty()).await;
		assert_eq!(r.typed(), vec![vec![json!(r.fresh), json!("/plan-review")]]);
		// Never a second time, even with omp idle again.
		r.herdr.set_state(&r.fresh, AgentState::Working, None, true);
		r.herdr.set_state(&r.fresh, AgentState::Idle, None, true);
		r.look(10).await;
		assert_eq!(r.typed().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_omps_plan_review_is_open_not_after_it_goes_either_0494() {
	let j = temp_jocasta();
	let open = PaneAgent { input_open: true, input_kind: Some("plan".to_owned()), ..PaneAgent::default() };
	let r = reopened(j.root(), proposed_plan_file, open).await;
	finally(&r.server, |_| async {
		r.look_until(|| r.looked()).await;
		assert_eq!(busy(&r.server).awaiting, 1);
		// The Plan Review already open ends the wait.
		r.report(PaneAgent::default());
		r.look(10).await;
		assert!(r.typed().is_empty());
	})
	.await;
}

/// A dialog of omp's (`kind`: `ask`) is open when the reopened thread's plan is looked at, as its bridge reports it.
async fn a_question_shows(kind: &str) {
	let j = temp_jocasta();
	let open = PaneAgent { input_open: true, input_kind: Some(kind.to_owned()), ..PaneAgent::default() };
	let r = reopened(j.root(), proposed_plan_file, open).await;
	finally(&r.server, |_| async {
		r.look(3).await;
		assert_eq!(busy(&r.server).awaiting, 1);
		assert!(r.typed().is_empty());
		// It is answered: the dialog closes, and the plan's review comes back.
		r.report(PaneAgent::default());
		r.look_until(|| !r.typed().is_empty()).await;
		assert_eq!(r.typed(), vec![vec![json!(r.fresh), json!("/plan-review")]]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_a_question_dialog_is_open_once_it_is_answered_the_plans_review_comes_back_0494_0589()
 {
	a_question_shows("ask").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_thread_whose_plan_isnt_awaiting_him_gets_nothing_typed_0494() {
	let j = temp_jocasta();
	let r = reopened(j.root(), plan_mode_file, PaneAgent::default()).await;
	finally(&r.server, |_| async {
		// The reopened omp is idle in plan mode with no proposal: the tick looks at it and finds no plan to show. Ten
		// ticks were not always enough on a loaded runner (the thread wasn't adopted and idle yet, so a proposal written
		// next still counted as one awaiting him at the reopen); wait for the look itself.
		r.look_until(|| {
			let threads = r.server.service.context().threads.all();
			!threads.is_empty() && threads.iter().all(|t| t.with(|t| t.plan.as_ref().is_some_and(|p| !p.reopened)))
		})
		.await;
		let status = call(&r.server, "thread_status", &json!({})).await.text;
		assert!(status.starts_with("demo/Readme line [feature/readme-line] idle"), "{status}");
		assert_eq!(busy(&r.server).awaiting, 0);
		// A plan he gets later in this run is omp's own Plan Review, open: nothing is typed into it either.
		let session = r
			.herdr
			.state()
			.pane_list
			.iter()
			.find(|p| p.pane_id == r.fresh)
			.and_then(|p| p.session_path.clone())
			.expect("the pane's session");
		append_proposal(&session);
		r.herdr.set_state(&r.fresh, AgentState::Working, None, true);
		r.herdr.set_state(&r.fresh, AgentState::Idle, None, true);
		r.look_until(|| busy(&r.server).awaiting == 1).await;
		r.look(10).await;
		assert!(r.typed().is_empty(), "typed: {:?}", r.typed());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn p6_b6_a_reviewed_thread_counts_in_the_close_question_as_on_the_hud_work_the_bosss_thread_send_started_isnt_said_as_his_rejection_work_he_started_is_0432_0438_0471()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		server.herdr.set_state(&boss_pane(&server.herdr).unwrap(), AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		let file = to_handoff(&server, &pane_id, &path).await;
		handoff_entry(&file);
		until(|| awaiting(&server) == 1).await;
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });

		// The boss relays his word into the reviewed thread.
		let sent = call(&server, "thread_send", &json!({ "thread": "Readme line", "text": "Fix the conflict." })).await;
		assert_eq!(sent.text, "Sent to demo/Readme line.");
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		let relayed = "[event] demo/Readme line — at work again on its reviewed work, after your thread_send · offer once to save a lesson if he rejected that work";
		until(|| {
			boss_idle(&server);
			said(&server, relayed) == 1
		})
		.await;
		assert_eq!(said(&server, "[event] demo/Readme line — he sent its reviewed work back"), 0);

		// The next task is reviewed; then he sets it to work himself.
		commit_in(&path, "demo\nA line.\nAnother.\n", "Another line");
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let review_text = review("main...feature/readme-line", Some(BRIEF));
		until(|| count_prompts(&server.herdr, &pane_id, &review_text) == 2).await;
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| count_prompts(&server.herdr, &pane_id, HANDOFF_PROMPT) == 2).await;
		append_entry(
			&file,
			&json!({ "type": "compaction", "id": "h2", "method": "handoff", "summary": "Handed off.", "timestamp": now_iso() }),
		);
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		until(|| {
			boss_idle(&server);
			said(&server, "[event] demo/Readme line — he sent its reviewed work back · offer once to save a lesson") == 1
		})
		.await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn p3_a_tasks_review_line_counts_the_threads_own_commits_not_mains_it_merged_in() {
	let j = temp_jocasta();
	let project = demo_project(j.root(), "demo").await;
	sh(&project.main_path, &["config", "user.name", "Test"]);
	sh(&project.main_path, &["config", "user.email", "test@example.com"]);
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		server.herdr.set_state(&boss_pane(&server.herdr).unwrap(), AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// Its /handoff writes no entry (CO-4): the next run's commits count from the reviewed HEAD.
		to_handoff(&server, &pane_id, &path).await;
		// main moves; the thread merges it in, then commits its own work.
		std::fs::write(project.main_path.join("other.txt"), "other\n").unwrap();
		sh(&project.main_path, &["add", "other.txt"]);
		sh(&project.main_path, &["commit", "-qm", "Other work"]);
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		sh(&path, &["merge", "--no-edit", "main"]);
		commit_in(&path, "demo\nA line.\nOwn.\n", "Own line");
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		let review_text = review("main...feature/readme-line", Some(BRIEF));
		until(|| count_prompts(&server.herdr, &pane_id, &review_text) == 2).await;
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| {
			boss_idle(&server);
			boss_lines(&server)
				.iter()
				.any(|l| l.starts_with("[event] demo/Readme line — done and reviewed: 1 new commit «Own line»"))
		})
		.await;
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn p8_a_plan_ready_line_still_waiting_for_the_boss_is_dropped_once_he_approved_the_plan() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let boss = boss_pane(&server.herdr).unwrap();
		server.herdr.set_state(&boss, AgentState::Idle, None, true);
		let started = start_thread(&server, ThreadOptions::default()).await;
		let (pane_id, path) = (started.pane_id, started.path);
		// The boss waits on his answer to its own question: herdr refuses lines into it, so they wait.
		server.herdr.state().prompt_refused = Some(boss.clone());
		let file = proposed_plan_file(&path);
		server.herdr.set_state(&pane_id, AgentState::Working, Some(path_text(&file)), true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| busy(&server).awaiting == 1).await;
		// He approves in Plan Review: omp leaves plan mode and works.
		append_entry(&file, &json!({ "type": "mode_change", "id": "m4", "mode": "none" }));
		server.herdr.set_state(&pane_id, AgentState::Working, None, true);
		until(|| busy(&server).awaiting == 0).await;
		server.herdr.state().prompt_refused = None;
		boss_idle(&server);
		// A later line reaches the boss: any plan line still waiting would have gone with or before it.
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		until(|| boss_lines(&server).iter().any(|l| l.starts_with("[event] demo/Readme line — finished its turn")))
			.await;
		assert!(boss_lines(&server).iter().all(|l| !l.contains("plan ready")));
	})
	.await;
}
