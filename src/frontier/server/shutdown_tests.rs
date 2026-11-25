//! Tests of how the server closes: the cases of `test/frontier/server/shutdown.test.ts` (the stop runner's own case is
//! in `stop_sequence_tests.rs`).

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::{Value, json};

use super::*;
use crate::frontier::server::helpers::{
	TestServer, ThreadOptions, call, demo_project, start_test_server, start_test_server_with, start_thread, until,
	with_test_defaults,
};
use crate::frontier::server::popups::{CANCEL, STOP};
use crate::frontier::server::stop_sequence::StopRun;
use crate::frontier::server::surface::AgentState;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::shared::errors::Error;
use crate::shared::paths::data_dir;
use crate::shared::service_file::{read_service_file, service_file_path};
use crate::shared::testing::temp_jocasta;

const WORKING: &str = "Stop 1 working thread? They pick up where they left off next time.";
const AWAITING: &str = "Stop 1 thread awaiting you? They pick up where they left off next time.";

type Log = Arc<Mutex<Vec<String>>>;

fn lines(log: &Log) -> Vec<String> {
	log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn push(log: &Log, line: String) {
	log.lock().unwrap_or_else(PoisonError::into_inner).push(line);
}

/// A step that runs `run` and finishes.
fn step_run(run: impl Fn() + Send + Sync + 'static) -> StopRun {
	Arc::new(move || {
		run();
		Box::pin(std::future::ready(Ok(())))
	})
}

/// What `guarded` hands a test.
struct Guarded {
	server: TestServer,
	asked: Log,
	steps: Log,
}

/// A server whose close question is answered with `answers` in turn (Stop everything, typed with its code, for true;
/// Cancel for false) as its shell would send it; every
/// question, terminal opened and stop that follows herdr's kept.
async fn guarded(answers: Vec<bool>, options: ServerOptions) -> Guarded {
	let asked: Log = Log::default();
	let steps: Log = Log::default();
	let answers = Arc::new(Mutex::new(answers));
	let address: Arc<OnceLock<JocastaServer>> = Arc::new(OnceLock::new());
	let (told, heard) = (Arc::clone(&asked), Arc::clone(&address));
	let options = ServerOptions {
		on_question: Some(Arc::new(move |popup| {
			push(&told, popup.view().message.clone());
			let answer = {
				let mut answers = answers.lock().unwrap_or_else(PoisonError::into_inner);
				if answers.is_empty() { false } else { answers.remove(0) }
			};
			let choice = if answer { STOP } else { CANCEL };
			let typed = popup.view().code.clone();
			let code = popup.code().to_owned();
			let heard = Arc::clone(&heard);
			tokio::spawn(async move {
				let server = heard.get().expect("the server").clone();
				server.answer(&code, &json!({ "choice": choice, "typed": typed }));
			});
		})),
		..options
	};
	let server = start_test_server(options).await;
	let _ = address.set(server.server.clone());
	// Runs on Stop everything after herdr's own stops, where the containers' stop ran before 0603.
	let (log, herdr) = (Arc::clone(&steps), Arc::clone(&server.herdr));
	let _ = server.stops_with(
		"the test's stop",
		step_run(move || push(&log, format!("stopped after {}", herdr.methods().join(",")))),
		None,
	);
	Guarded { server, asked, steps }
}

fn methods_after(herdr: &FakeSurface, before: usize) -> Vec<String> {
	herdr.methods().split_off(before)
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_thread_at_work_closing_asks_nothing_and_stops_everything() {
	let _j = temp_jocasta();
	let Guarded { server, asked, steps, .. } = guarded(vec![], ServerOptions::default()).await;
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	server.stopped().await;
	assert!(lines(&asked).is_empty());
	assert_eq!(lines(&steps).len(), 1);
	assert!(!service_file_path(None).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_guard_asks_only_while_a_thread_is_working_or_awaiting_him_counting_them_apart_cancel_leaves_everything_running()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let Guarded { server, asked, steps } = guarded(vec![false, false], ServerOptions::default()).await;
	let herdr = Arc::clone(&server.herdr);
	let pane = start_thread(&server, ThreadOptions::default()).await.pane_id;
	herdr.set_state(&pane, AgentState::Working, None, true);
	until(|| server.service.busy_threads().is_ok_and(|busy| busy.working == 1)).await;
	let before = herdr.methods().len();
	assert_eq!(server.shutdown(true).await, CloseOutcome::Kept);
	assert_eq!(lines(&asked), [WORKING]);

	herdr.set_state(&pane, AgentState::Blocked, None, true);
	until(|| server.service.hud(None).is_ok_and(|hud| hud.blocked == 1)).await;
	assert_eq!(server.shutdown(true).await, CloseOutcome::Kept);
	assert_eq!(lines(&asked), [WORKING, AWAITING]);

	// Kept: herdr wasn't stopped, no container stopped, the service still answers.
	let after = methods_after(&herdr, before);
	assert!(!after.contains(&"stopServer".to_owned()));
	assert!(lines(&steps).is_empty());
	assert!(herdr.state().pane_list.iter().any(|p| p.pane_id == pane));
	assert_eq!(read_service_file().map(|file| file.port), Some(server.port));
	assert_ne!(call(&server, "thread_status", &json!({})).await.is_error, Some(true));

	// He answers, the turn ends: nothing to cut off, so closing asks nothing.
	herdr.set_state(&pane, AgentState::Working, None, true);
	herdr.set_state(&pane, AgentState::Idle, None, true);
	until(|| server.service.busy_threads().is_ok_and(|busy| busy.working + busy.awaiting == 0)).await;
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	assert_eq!(lines(&asked).len(), 2);
	assert_eq!(lines(&steps).len(), 1);
	server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_holds_herdr_down_keeps_mid_turn_threads_for_restore_stops_the_service_then_herdrs_server_then_what_else_registered()
 {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let Guarded { server, asked, steps, .. } = guarded(vec![true], ServerOptions::default()).await;
	let log = Arc::clone(&steps);
	let _ = server.stops_with("the relay", step_run(move || push(&log, "relay".to_owned())), None);
	let herdr = Arc::clone(&server.herdr);
	let pane = start_thread(&server, ThreadOptions::default()).await.pane_id;
	let session = j.root().join("s.jsonl");
	herdr.set_state(&pane, AgentState::Working, session.to_str(), true);
	until(|| server.service.busy_threads().is_ok_and(|busy| busy.working == 1)).await;
	// The thread's session path is read by a refresh (the TypeScript's `busyThreads` read herdr itself).
	server.service.refresh().await;
	let before = herdr.methods().len();
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	server.stopped().await;
	assert_eq!(lines(&asked), [WORKING]);
	let methods = methods_after(&herdr, before);
	// The service's last word to herdr (clearing its tokens) comes before the server stops.
	assert_eq!(methods.last().map(String::as_str), Some("stopServer"));
	assert_eq!(methods.iter().filter(|m| *m == "stopServer").count(), 1);
	assert_eq!(lines(&steps), [format!("stopped after {}", herdr.methods().join(",")), "relay".to_owned()]);
	// The thread at work comes back with `omp --restore <session>` on the next start (0549).
	let panes: Value = serde_json::from_str(&std::fs::read_to_string(data_dir().join("panes.json")).unwrap()).unwrap();
	assert_eq!(panes[&pane]["restore"], json!(session.display().to_string()));
	assert!(herdr.state().pane_list.is_empty());
	assert!(!service_file_path(None).exists());
	// Asked again once stopped: the same outcome, nothing more.
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	assert_eq!(herdr.of("stopServer").len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_right_after_a_service_restart_waits_for_the_threads_to_be_found_so_a_working_one_is_still_asked_about()
{
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let herdr = Arc::new(FakeSurface::new());
	let first = start_test_server_with(Arc::clone(&herdr), ServerOptions::default()).await;
	let pane = start_thread(&first, ThreadOptions::default()).await.pane_id;
	herdr.set_state(&pane, AgentState::Working, None, true);
	until(|| first.service.busy_threads().is_ok_and(|busy| busy.working == 1)).await;
	// A failed start's stop: herdr and the thread keep running.
	first.stop().await;

	// The next service's discover() waits on herdr's snapshot.
	let (open, gate) = tokio::sync::watch::channel(false);
	herdr.state().snapshot_gate = Some(gate);
	let asked: Log = Log::default();
	let told = Arc::clone(&asked);
	let options = ServerOptions {
		herdr: Some(herdr.clone()),
		on_question: Some(Arc::new(move |popup| {
			push(&told, popup.view().message.clone());
			popup.close(None);
		})),
		..ServerOptions::default()
	};
	let second = start_server(with_test_defaults(options, &herdr)).await.expect("the second server");
	let outcome = {
		let closing = second.clone();
		tokio::spawn(async move { closing.shutdown(true).await })
	};
	open.send_replace(true);
	assert_eq!(outcome.await.unwrap(), CloseOutcome::Kept);
	assert_eq!(lines(&asked), [WORKING]);
	let _ = second.ready().await;
	second.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_step_that_throws_or_hangs_never_keeps_the_close_from_finishing() {
	let _j = temp_jocasta();
	// A stop step fails: the steps after it run, and the close finishes.
	let Guarded { server, .. } = guarded(vec![], ServerOptions::default()).await;
	let fails: StopRun = Arc::new(|| Box::pin(async { Err(Error::other("a stop step failed")) }));
	let _ = server.stops_with("fails", fails, None);
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	server.stopped().await;
	assert_eq!(server.herdr.of("stopServer").len(), 1);
	assert!(!service_file_path(None).exists());

	// A stop that never answers is left after its bound.
	let hung = start_test_server(ServerOptions::default()).await;
	let never: StopRun = Arc::new(|| Box::pin(std::future::pending()));
	let _ = hung.stops_with("never answers", never, Some(200));
	assert_eq!(hung.shutdown(true).await, CloseOutcome::Stopped);
	hung.stopped().await;
	assert!(!service_file_path(None).exists());
}
