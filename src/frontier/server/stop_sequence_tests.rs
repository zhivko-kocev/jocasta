//! Tests of `stop_sequence.rs`. The runner's case is ported from `test/frontier/server/shutdown.test.ts`; that file's
//! other cases run a whole server and are in `shutdown_tests.rs` beside `server.rs` (its HTTP-free half of "a stop
//! step that throws or hangs never keeps the close from finishing" is `a_step_past_its_bound_never_keeps_the_stop_from_finishing`
//! below).

use std::sync::atomic::AtomicBool;

use super::*;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::shared::service_file::{ServiceFile, service_file_path, write_service_file};
use crate::shared::testing::temp_jocasta;

type Log = Arc<Mutex<Vec<String>>>;

fn note(log: &Log, text: &str) {
	log.lock().unwrap_or_else(PoisonError::into_inner).push(text.to_owned());
}

fn seen(log: &Log) -> Vec<String> {
	log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// A step that logs its name.
fn step(log: &Log, name: &str) -> StopStep {
	let (log, text) = (Arc::clone(log), name.to_owned());
	StopStep::sync(name, move || {
		note(&log, &text);
		Ok(())
	})
}

fn hangs() -> StopRun {
	Arc::new(|| -> BoxFuture<'static, Result<()>> { Box::pin(std::future::pending()) })
}

#[tokio::test]
async fn the_stop_runner_runs_all_steps_only_on_stop_everything_and_finally_steps_last() {
	let ran: Log = Arc::default();
	let failed: Log = Arc::default();
	let throws = {
		let ran = Arc::clone(&ran);
		StopStep::sync("throws", move || {
			note(&ran, "throws");
			Err(Error::other("no"))
		})
	};
	let steps = vec![
		step(&ran, "last").when(StopWhen::Finally),
		step(&ran, "first"),
		step(&ran, "only all").when(StopWhen::All),
		throws,
		StopStep::new("hangs", hangs()).bounded(20),
		step(&ran, "after"),
	];
	let report = |log: &Log| {
		let log = Arc::clone(log);
		move |name: &str, _: &Error| note(&log, name)
	};
	run_stop_steps(&steps, false, &mut report(&failed)).await;
	assert_eq!(seen(&ran), ["first", "throws", "after", "last"]);
	assert_eq!(seen(&failed), ["throws", "hangs"]);
	ran.lock().unwrap().clear();
	run_stop_steps(&steps[..3], true, &mut |_, _| {}).await;
	assert_eq!(seen(&ran), ["first", "only all", "last"]);
}

#[tokio::test]
async fn a_failed_starts_undo_runs_newest_first() {
	let ran: Log = Arc::default();
	undo_start(&[step(&ran, "relay"), step(&ran, "herdr"), step(&ran, "server")]).await;
	assert_eq!(seen(&ran), ["server", "herdr", "relay"]);
}

#[tokio::test]
async fn a_failed_starts_undo_goes_on_after_a_step_that_fails_and_one_that_is_never_done() {
	let ran: Log = Arc::default();
	let broken = StopStep::sync("broken", || Err(Error::other("no")));
	undo_start(&[step(&ran, "relay"), StopStep::new("hangs", hangs()).bounded(20), broken, step(&ran, "server")]).await;
	assert_eq!(seen(&ran), ["server", "relay"]);
}

#[tokio::test]
async fn within_answers_the_promise_or_a_timeout_naming_the_bound() {
	assert_eq!(within(async { 7 }, 1_000).await.unwrap(), 7);
	let late = within(std::future::pending::<()>(), 20).await.unwrap_err();
	assert_eq!(late.to_string(), "no answer in 0.02 s");
	let late = within(std::future::pending::<()>(), 5_000);
	assert!(tokio::time::timeout(Duration::from_millis(30), late).await.is_err());
}

/// A service that logs what the sequence asks of it, optionally failing its stop.
struct FakeService {
	log: Log,
	fail_stop: AtomicBool,
}

impl FakeService {
	fn call(&self, name: &'static str) -> BoxFuture<'_, Result<()>> {
		Box::pin(async move {
			note(&self.log, name);
			if name == "stop" && self.fail_stop.load(Ordering::SeqCst) {
				return Err(Error::other("hud.txt can't be written"));
			}
			Ok(())
		})
	}
}

impl StopService for FakeService {
	fn pause_working(&self) -> BoxFuture<'_, Result<()>> {
		self.call("pauseWorking")
	}

	fn stop(&self) -> BoxFuture<'_, Result<()>> {
		self.call("stop")
	}

	fn cut_paused_turns(&self) -> BoxFuture<'_, Result<()>> {
		self.call("cutPausedTurns")
	}

	fn end_thread_leftovers(&self) -> BoxFuture<'_, Result<()>> {
		self.call("endThreadLeftovers")
	}

	fn remove_ended_hosts(&self) -> BoxFuture<'_, Result<()>> {
		self.call("removeEndedHosts")
	}

	fn end_hunk_daemon(&self) -> BoxFuture<'_, Result<()>> {
		self.call("endHunkDaemon")
	}
}

struct World {
	herdr: Arc<FakeHerdr>,
	popups: Arc<Popups>,
	log: Log,
	service: Arc<FakeService>,
	sent: Arc<Mutex<Vec<ServerMessage>>>,
}

impl World {
	fn new() -> Self {
		let log: Log = Arc::default();
		Self {
			herdr: Arc::new(FakeHerdr::new()),
			popups: Arc::new(Popups::new()),
			service: Arc::new(FakeService { log: Arc::clone(&log), fail_stop: AtomicBool::new(false) }),
			log,
			sent: Arc::default(),
		}
	}

	fn parts(&self, extra_stops: Vec<StopStep>) -> StopParts {
		let sent = Arc::clone(&self.sent);
		let log = Arc::clone(&self.log);
		let server_log = Arc::clone(&self.log);
		let relay = step(&log, "relay").run;
		StopParts {
			popups: Arc::clone(&self.popups),
			broadcast: Arc::new(move |message| sent.lock().unwrap_or_else(PoisonError::into_inner).push(message)),
			herdr: self.herdr.clone(),
			session: "jocasta".to_owned(),
			service: self.service.clone(),
			extra_stops,
			whiteboard: None,
			relay,
			server: step(&server_log, "server").run,
			bounds: StopBounds::default(),
		}
	}
}

fn names(steps: &[StopStep]) -> Vec<&str> {
	steps.iter().map(|step| step.name.as_str()).collect()
}

#[test]
fn the_sequence_stops_everything_in_its_order_with_the_extra_stops_after_the_hunk_daemon() {
	let world = World::new();
	let extra = step(&world.log, "the test's stop");
	let steps = stop_steps(world.parts(vec![extra]));
	assert_eq!(
		names(&steps),
		[
			"his questions",
			"goodbye",
			"noting the threads at work",
			"the service",
			"the turns at work",
			"herdr session jocasta",
			"the threads' leftovers",
			"omp's host entries",
			"hunk's daemon",
			"the test's stop",
			"the collab relay",
			"the server",
			"jocasta-service.json",
		]
	);
	let by = |name: &str| steps.iter().find(|step| step.name == name).unwrap();
	assert_eq!(by("noting the threads at work").ms, Some(15_000));
	assert_eq!(by("the service").ms, Some(30_000));
	assert_eq!(by("herdr session jocasta").ms, Some(15_000));
	assert_eq!(by("the server").when, StopWhen::Finally);
	assert_eq!(by("the collab relay").when, StopWhen::Always);
	assert_eq!(by("hunk's daemon").when, StopWhen::All);
}

#[test]
fn the_bounds_can_be_changed_one_by_one() {
	let world = World::new();
	let mut parts = world.parts(Vec::new());
	parts.bounds = StopBounds { service: Some(200), herdr: None, whiteboard: Some(10) };
	let steps = stop_steps(parts);
	let by = |name: &str| steps.iter().find(|step| step.name == name).unwrap().ms;
	assert_eq!((by("the service"), by("noting the threads at work")), (Some(200), Some(15_000)));
}

#[tokio::test]
async fn stopping_everything_stops_the_service_then_herdrs_server_then_the_rest() {
	let _j = temp_jocasta();
	let world = World::new();
	world.popups.open(&crate::frontier::server::popups::allow_question("Allow?", "x", None), false);
	write_service_file(&ServiceFile { pid: std::process::id(), port: 1, token: "t".to_owned() }, None).unwrap();
	let steps = stop_steps(world.parts(vec![step(&world.log, "the test's stop")]));
	run_stop_steps(&steps, true, &mut |name, error| panic!("{name}: {error}")).await;
	assert_eq!(
		seen(&world.log),
		[
			"pauseWorking",
			"stop",
			"cutPausedTurns",
			"endThreadLeftovers",
			"removeEndedHosts",
			"endHunkDaemon",
			"the test's stop",
			"relay",
			"server",
		]
	);
	// The server stops once.
	assert_eq!(world.herdr.of("stopServer").len(), 1);
	assert_eq!(world.sent.lock().unwrap().as_slice(), [ServerMessage::Bye]);
	assert!(world.popups.list().is_empty());
	assert!(!service_file_path(None).exists());
}

#[tokio::test]
async fn a_failed_start_stops_the_service_the_relay_and_the_server_only() {
	let _j = temp_jocasta();
	let world = World::new();
	let steps = stop_steps(world.parts(Vec::new()));
	run_stop_steps(&steps, false, &mut |name, error| panic!("{name}: {error}")).await;
	assert_eq!(seen(&world.log), ["stop", "relay", "server"]);
	assert!(world.herdr.of("stopServer").is_empty());
}

#[tokio::test]
async fn a_step_that_fails_never_keeps_the_stop_from_finishing() {
	let _j = temp_jocasta();
	let world = World::new();
	world.service.fail_stop.store(true, Ordering::SeqCst);
	let steps = stop_steps(world.parts(Vec::new()));
	let failed: Log = Arc::default();
	let report = Arc::clone(&failed);
	run_stop_steps(&steps, true, &mut move |name, _| note(&report, name)).await;
	assert_eq!(seen(&failed), ["the service"]);
	assert_eq!(world.herdr.of("stopServer").len(), 1);
	assert_eq!(seen(&world.log).last().map(String::as_str), Some("server"));
}

#[tokio::test]
async fn a_step_past_its_bound_never_keeps_the_stop_from_finishing() {
	let _j = temp_jocasta();
	let world = World::new();
	let never = StopStep::new("never answers", hangs()).bounded(200);
	let stopper = stopper({
		let log = Arc::clone(&world.log);
		move |extras| {
			let mut steps = extras;
			steps.push(step(&log, "after"));
			steps
		}
	});
	_ = stopper.add(never);
	stopper.stop(true).await;
	assert_eq!(seen(&world.log), ["after"]);
}

#[tokio::test]
async fn the_stopper_runs_once_whoever_asks_first_and_stopped_resolves_when_it_ended() {
	let ran: Log = Arc::default();
	let stopper = stopper({
		let ran = Arc::clone(&ran);
		move |extras| {
			let mut steps = vec![step(&ran, "service"), step(&ran, "only all").when(StopWhen::All)];
			steps.extend(extras);
			steps
		}
	});
	assert!(!stopper.stopping());
	_ = stopper.add(step(&ran, "registered"));
	let (a, b) = tokio::join!(stopper.stop(true), stopper.stop(false));
	let _ = (a, b);
	assert!(stopper.stopping());
	// The first caller's `all` stands, and a second stop runs nothing more.
	assert_eq!(seen(&ran), ["service", "only all", "registered"]);
	stopper.stop(true).await;
	stopper.stopped().await;
	assert_eq!(seen(&ran).len(), 3);
}

#[tokio::test]
async fn an_extra_stop_that_unregistered_does_not_run_and_the_stopper_reads_them_when_the_stop_runs() {
	let ran: Log = Arc::default();
	let stopper = stopper(|extras| extras);
	let gone = stopper.add(step(&ran, "gone"));
	_ = stopper.add(step(&ran, "kept"));
	gone();
	stopper.stop(false).await;
	assert_eq!(seen(&ran), ["kept"]);
}
