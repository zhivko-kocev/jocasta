//! The cases of `test/frontier/server/service-plans.test.ts` on the real server: plans awaiting him, threads reopened
//! after Stop everything, the hunk daemon's end and the review lines the boss hears.

use std::fs::OpenOptions;
use std::io::Write;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use serde_json::{Value, json};

use crate::frontier::protocol::POPUP_READER_HEADER;
use crate::frontier::server::close::CloseOutcome;
use crate::frontier::server::helpers::{
	BRIEF, HunkDaemon, TestServer, ThreadOptions, append_proposal, boss_idle, boss_lines, boss_pane, call, commit_in,
	demo_project, handoff_entry, http, hunk, plan_mode_file, proposed_plan_file, review, sh, start_test_server,
	start_test_server_with, start_thread, to_handoff, until, until_async, use_hunk_daemon, with_server,
};
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::popups::{CANCEL, STOP};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::status::Busy;
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

/// A server on `herdr` with the hunk daemon stand-in, `agent_wait_ms` and `plan_settle_ms` as given.
async fn start(
	herdr: &Arc<FakeHerdr>,
	daemon: &HunkDaemon,
	agent_wait_ms: Option<i64>,
	plan_settle_ms: Option<i64>,
) -> TestServer {
	let mut options = ServerOptions { agent_wait_ms, plan_settle_ms, ..ServerOptions::default() };
	daemon.apply(&mut options);
	start_test_server_with(Arc::clone(herdr), options).await
}

fn busy(server: &TestServer) -> Busy {
	server.service.busy_threads().unwrap()
}

fn awaiting(server: &TestServer) -> u32 {
	server.service.hud(None).unwrap().awaiting
}

fn typed(herdr: &FakeHerdr, pane_id: &str) -> Vec<Vec<Value>> {
	herdr.of("prompt").into_iter().filter(|args| args[0] == json!(pane_id)).collect()
}

fn said(server: &TestServer, line: &str) -> usize {
	boss_lines(server).iter().filter(|l| l.starts_with(line)).count()
}

fn count_prompts(herdr: &FakeHerdr, pane_id: &str, text: &str) -> usize {
	herdr.of("prompt").into_iter().filter(|args| args[0] == json!(pane_id) && args[1] == json!(text)).count()
}

fn plan_mode_entry(id: &str) -> Value {
	json!({"type": "mode_change", "id": id, "mode": "plan", "data": {"planFilePath": "local://readme-line-plan.md"}})
}

fn fresh_pane(herdr: &FakeHerdr) -> String {
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
		let plan_ready = "[event] demo/Readme line — plan ready, awaiting you";
		let finished = "[event] demo/Readme line — finished its turn";

		// The turn was read when it started; its proposal is in the file only 300 ms after herdr said idle.
		let file = plan_mode_file(&path);
		server.herdr.set_state(&pane_id, AgentState::Working, Some(path_text(&file)), true);
		server.herdr.set_state(&pane_id, AgentState::Idle, None, true);
		// The race itself: the proposal is written on the real clock after the idle event, as omp's writer did.
		tokio::time::sleep(Duration::from_millis(300)).await;
		append_proposal(&file);
		until(|| said(&server, plan_ready) == 1).await;
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
			said(&server, plan_ready) == 2
		})
		.await;
		assert_eq!(busy(&server), Busy { working: 0, awaiting: 1 });
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn b3_a_plan_awaiting_him_survives_stop_everything_and_reopen_the_resumed_thread_counts_on_the_hud_in_awaiting_you_and_in_the_close_question_0432_0434_0438()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let daemon = use_hunk_daemon().await;
	let herdr = Arc::new(FakeHerdr::new());
	let server = start(&herdr, &daemon, Some(10_000), None).await;
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
	let server = start(&herdr, &daemon, None, None).await;
	finally(&server, |server| async move {
		let fresh = fresh_pane(&herdr);
		append_entry(&file, &plan_mode_entry("m3"));
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
	let daemon = use_hunk_daemon().await;
	let herdr = Arc::new(FakeHerdr::new());
	let server = start(&herdr, &daemon, Some(10_000), None).await;
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
	let server = start(&herdr, &daemon, None, None).await;
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
/// herdr restores its pane as a shell, the thread's omp resumes with `--continue` and goes idle, its screen `screen`.
struct Reopened {
	server: TestServer,
	herdr: Arc<FakeHerdr>,
	fresh: String,
}

async fn reopened(root: &Path, session: fn(&Path) -> PathBuf, screen: &str) -> Reopened {
	demo_project(root, "demo").await;
	let daemon = use_hunk_daemon().await;
	let herdr = Arc::new(FakeHerdr::new());
	// A plan-mode turn's end reads the session again for 200 ms, not the product's 2 s (TST-5, TST-7).
	let server = start(&herdr, &daemon, Some(10_000), Some(200)).await;
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
	let server = start(&herdr, &daemon, None, Some(200)).await;
	let fresh = fresh_pane(&herdr);
	// The resumed omp re-enters plan mode in its session.
	append_entry(&file, &plan_mode_entry("m3"));
	herdr.state().screens.insert(fresh.clone(), screen.to_owned());
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

	fn screen_reads(&self) -> usize {
		self.herdr.of("screen").iter().filter(|args| args[0] == json!(self.fresh)).count()
	}
}

/// omp's Plan Review as `pane read` shows it.
const PLAN_REVIEW: &str = "Plan: readme-line\n> Approve and execute\n  Approve and compact context\n  Approve and keep context (~12k / 200k)\n  Refine plan\n  Save and quit\n↑↓ navigate  enter select  esc cancel";

/// A question of omp ≤ 18.18.2's `ask` as `pane read` shows it.
const QUESTION: &str =
	"Which file?\n> README.md\n  docs/intro.md\n  Other (type your own)\n↑↓ navigate  enter select  esc cancel";

/// omp 18.20.2's `ask` form as its TUI draws it (pi-tui `html-form/dialog.ts`, 0589), cut so each test screen shows only
/// one of the two lines that tell a form: its Other row, or its footer.
const FORM_OTHER_ROW: &str = "╭─ Ask ──────────────────────────────────────────╮\n│ Session store                                  │\n│   ◉ SQLite: one file, no new service           │\n│   ○ Redis: shared across instances, needs ops  │\n│   ○ Other: type your own                       │";
const FORM_FOOTER: &str = "│   Submit                                       │\n├────────────────────────────────────────────────┤\n│ ⏎ choose · ↑/↓ move · ⇥ next field · ⎋ cancel  │\n╰────────────────────────────────────────────────╯";

#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_thread_whose_plan_awaits_him_gets_plan_review_typed_once_after_its_screen_shows_no_picker_0494() {
	let j = temp_jocasta();
	let r = reopened(j.root(), proposed_plan_file, "plan mode  ▸ ").await;
	finally(&r.server, |_| async {
		r.look_until(|| !r.typed().is_empty()).await;
		assert_eq!(r.typed(), vec![vec![json!(r.fresh), json!("/plan-review")]]);
		// It read the screen before typing.
		let calls: Vec<String> = r
			.herdr
			.state()
			.calls
			.iter()
			.map(|c| format!("{} {}", c.method, c.args.first().and_then(Value::as_str).unwrap_or("undefined")))
			.collect();
		let position = |text: String| calls.iter().position(|c| *c == text);
		assert!(position(format!("screen {}", r.fresh)) < position(format!("prompt {}", r.fresh)));
		// Never a second time, even with the screen clear and omp idle again.
		r.herdr.state().screens.insert(r.fresh.clone(), String::new());
		r.herdr.set_state(&r.fresh, AgentState::Working, None, true);
		r.herdr.set_state(&r.fresh, AgentState::Idle, None, true);
		r.look(10).await;
		assert_eq!(r.typed().len(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_omps_plan_review_shows_not_after_it_goes_either_0494() {
	let j = temp_jocasta();
	let r = reopened(j.root(), proposed_plan_file, PLAN_REVIEW).await;
	finally(&r.server, |_| async {
		r.look_until(|| r.screen_reads() > 0).await;
		assert_eq!(busy(&r.server).awaiting, 1);
		// The picker already open ends the wait.
		r.herdr.state().screens.insert(r.fresh.clone(), String::new());
		r.look(10).await;
		assert!(r.typed().is_empty());
		assert_eq!(r.screen_reads(), 1);
	})
	.await;
}

async fn a_question_shows(screen: &str) {
	let j = temp_jocasta();
	let r = reopened(j.root(), proposed_plan_file, screen).await;
	finally(&r.server, |_| async {
		r.look_until(|| r.screen_reads() >= 3).await;
		assert_eq!(busy(&r.server).awaiting, 1);
		assert!(r.typed().is_empty());
		r.herdr.state().screens.insert(r.fresh.clone(), String::new());
		r.look_until(|| !r.typed().is_empty()).await;
		assert_eq!(r.typed(), vec![vec![json!(r.fresh), json!("/plan-review")]]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_a_question_shows_omp_18_18_2s_question_picker_once_it_is_answered_the_plans_review_comes_back_0494_0589()
 {
	a_question_shows(QUESTION).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_a_question_shows_an_ask_forms_other_row_once_it_is_answered_the_plans_review_comes_back_0494_0589()
 {
	a_question_shows(FORM_OTHER_ROW).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_typed_into_a_reopened_thread_while_a_question_shows_an_ask_forms_footer_once_it_is_answered_the_plans_review_comes_back_0494_0589()
 {
	a_question_shows(FORM_FOOTER).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_thread_whose_plan_isnt_awaiting_him_gets_nothing_typed_0494() {
	let j = temp_jocasta();
	let r = reopened(j.root(), plan_mode_file, "").await;
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
		assert!(r.herdr.of("screen").is_empty());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn b4_the_hunk_session_daemon_the_service_started_ends_with_stop_everything_also_when_keep_came_first_0434_0486()
{
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	// A free loopback port for the daemon, never hunk's or Jocasta's real one.
	let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
	let up = || async move {
		match http().get(format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_millis(500)).send().await {
			Ok(response) => response.status().is_success(),
			Err(_) => false,
		}
	};
	let answers = Arc::new(Mutex::new(vec![false, true]));
	let address: Arc<OnceLock<(u16, String)>> = Arc::new(OnceLock::new());
	let heard = Arc::clone(&address);
	let asked = Arc::new(AtomicUsize::new(0));
	let counted = Arc::clone(&asked);
	let server = start_test_server(ServerOptions {
		hunk: Some(Some(hunk())),
		hunk_port: Some(port),
		agent_wait_ms: Some(10_000),
		on_question: Some(Arc::new(move |popup| {
			counted.fetch_add(1, Ordering::SeqCst);
			let answer = {
				let mut answers = answers.lock().unwrap_or_else(PoisonError::into_inner);
				if answers.is_empty() { false } else { answers.remove(0) }
			};
			let choice = if answer { STOP } else { CANCEL };
			let typed = popup.view().code.clone();
			let code = popup.code().to_owned();
			let heard = Arc::clone(&heard);
			// One reader per question (0578): the view first, then the answer with its reader secret.
			tokio::spawn(async move {
				let (port, token) = heard.get().expect("the server's address").clone();
				let url = format!("http://127.0.0.1:{port}/api/popup/{code}");
				let client = http();
				let view = client.get(&url).bearer_auth(&token).send().await.expect("the read");
				let secret =
					view.headers().get(POPUP_READER_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
				let _ = client
					.post(&url)
					.bearer_auth(&token)
					.header(POPUP_READER_HEADER, secret)
					.body(json!({ "choice": choice, "typed": typed }).to_string())
					.send()
					.await;
			});
		})),
		..ServerOptions::default()
	})
	.await;
	let _ = address.set((server.port, server.token.clone()));
	finally(&server, |server| async move {
		let started = start_thread(&server, ThreadOptions::default()).await;
		// A task ends: the review starts hunk's daemon before its pane opens.
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		commit_in(&started.path, "demo\nA line.\n", "Add a README line");
		server.herdr.set_state(&started.pane_id, AgentState::Idle, None, true);
		until(|| server.herdr.of("splitBeside").len() == 1).await;
		assert!(up().await);
		// The review turn runs: he is asked, keeps it running, and the daemon stays.
		server.herdr.set_state(&started.pane_id, AgentState::Working, None, true);
		assert_eq!(server.shutdown(true).await, CloseOutcome::Kept);
		assert!(up().await);
		// Later he stops everything: the daemon goes too.
		assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
		server.stopped().await;
		assert!(!up().await);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn p6_b6_a_reviewed_thread_counts_in_the_close_question_as_on_the_hud_work_the_bosss_thread_send_started_isnt_said_as_his_rejection_work_he_started_is_0432_0438_0471()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let daemon = use_hunk_daemon().await;
	let mut options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	daemon.apply(&mut options);
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
	let daemon = use_hunk_daemon().await;
	let mut options = ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() };
	daemon.apply(&mut options);
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
