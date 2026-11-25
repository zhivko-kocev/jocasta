//! The stop sequence (0434, 0549, LC-5, LC-8, SRV-A-2, SRV-A-3): what a stop and a failed start's undo run, in order,
//! each step caught and each bound one bounded, so the run always ends. The Rust port of `stop-sequence.ts` (0625).

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::future::BoxFuture;
use tokio::sync::watch;

use crate::frontier::protocol::ServerMessage;
use crate::frontier::server::popups::Popups;
use crate::frontier::server::surface::Surface;
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
use crate::shared::service_file::remove_service_file;

/// `promise`, or an error ("no answer in N s", the TypeScript's `StepTimeout`) after `ms`. The timer never outlives the
/// future.
pub async fn within<T>(promise: impl Future<Output = T>, ms: u64) -> Result<T> {
	tokio::time::timeout(Duration::from_millis(ms), promise)
		.await
		.map_err(|_| Error::other(format!("no answer in {} s", ms as f64 / 1000.0)))
}

/// What a step runs: called once per stop, it starts the step's work.
pub type StopRun = Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// When a step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopWhen {
	/// Every stop, a failed start's too (the default).
	#[default]
	Always,
	/// Only when Jocasta closes as one (0434, 0549).
	All,
	/// Last, after every other step.
	Finally,
}

/// One step of a stop: what it stops, bounded by `ms` when it waits on herdr or another process.
#[derive(Clone)]
pub struct StopStep {
	pub name: String,
	pub run: StopRun,
	pub ms: Option<u64>,
	pub when: StopWhen,
}

impl StopStep {
	pub fn new(name: impl Into<String>, run: StopRun) -> Self {
		Self { name: name.into(), run, ms: None, when: StopWhen::Always }
	}

	/// A step that does its work at once, with no waiting.
	pub fn sync(name: impl Into<String>, run: impl Fn() -> Result<()> + Send + Sync + 'static) -> Self {
		Self::new(name, Arc::new(move || -> BoxFuture<'static, Result<()>> { Box::pin(std::future::ready(run())) }))
	}

	pub fn bounded(mut self, ms: u64) -> Self {
		self.ms = Some(ms);
		self
	}

	pub fn when(mut self, when: StopWhen) -> Self {
		self.when = when;
		self
	}
}

/// What a failed step is reported to: its name and what went wrong.
pub type Failed<'a> = &'a mut (dyn FnMut(&str, &Error) + Send);

/// Runs one step; an error or a step outliving its bound goes to `failed`, and the stop goes on.
async fn run_step(step: &StopStep, failed: &mut (dyn FnMut(&str, &Error) + Send)) {
	let done = (step.run)();
	let outcome = match step.ms {
		None => done.await,
		Some(ms) => within(done, ms).await.and_then(|result| result),
	};
	if let Err(error) = outcome {
		failed(&step.name, &error);
	}
}

/// Runs `steps` in order: the `All` ones only with `all`, the `Finally` ones last. Every step is caught and each bound
/// one is bounded, so the run always ends.
pub async fn run_stop_steps(steps: &[StopStep], all: bool, failed: Failed<'_>) {
	for step in steps {
		if step.when == StopWhen::Finally || (step.when == StopWhen::All && !all) {
			continue;
		}
		run_step(step, failed).await;
	}
	for step in steps.iter().filter(|step| step.when == StopWhen::Finally) {
		run_step(step, failed).await;
	}
}

/// What a failed start undoes (SRV-A-2): each resource's undo as the start registered it, run newest first.
pub async fn undo_start(undo: &[StopStep]) {
	let reversed: Vec<StopStep> = undo.iter().rev().cloned().collect();
	run_stop_steps(&reversed, true, &mut |name, error| {
		log(LogLevel::Warn, "server", &format!("after the failed start, {name} didn't stop: {error}"), None);
	})
	.await;
}

/// Each stop step's bound in ms; `None` keeps the default: service 30 s, herdr 15 s.
#[derive(Debug, Clone, Copy, Default)]
pub struct StopBounds {
	pub service: Option<u64>,
	pub herdr: Option<u64>,
}

type Step = BoxFuture<'static, Result<()>>;

/// What the stop sequence stops of the service (`Pick<JocastaService, …>` in the TypeScript, so a test can stand in).
pub trait StopService: Send + Sync {
	fn pause_working(&self) -> BoxFuture<'_, Result<()>>;
	fn stop(&self) -> BoxFuture<'_, Result<()>>;
	fn cut_paused_turns(&self) -> BoxFuture<'_, Result<()>>;
	fn end_thread_leftovers(&self) -> BoxFuture<'_, Result<()>>;
}

/// What the stop sequence stops.
pub struct StopParts {
	pub popups: Arc<Popups>,
	pub broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	pub herdr: Arc<dyn Surface>,
	pub service: Arc<dyn StopService>,
	/// What else Jocasta started that stops with it (0549), in the order registered; read when the stop runs.
	pub extra_stops: Vec<StopStep>,
	/// Stops the bridge's listener (0675).
	pub bridge: StopRun,
	/// Stops the listening server (`server.stop(true)`); nothing when it never listened.
	pub server: StopRun,
	pub bounds: StopBounds,
}

/// A step that calls `service.$method()`.
macro_rules! call {
	($service:expr, $method:ident) => {{
		let service = Arc::clone($service);
		let run: StopRun = Arc::new(move || -> Step {
			let service = Arc::clone(&service);
			Box::pin(async move { service.$method().await })
		});
		run
	}};
}

/// The stop sequence: a failed start stops the service only; Jocasta closing as one (`all`, 0434, 0549) stops
/// everything, in this order. Each step that waits on herdr or another process is bounded (LC-5, LC-8, SRV-A-3): the
/// threads noted and herdr's stop by `herdr`, the service and the extra stops by their own.
pub fn stop_steps(parts: StopParts) -> Vec<StopStep> {
	let StopParts { popups, broadcast, herdr, service, extra_stops, bridge, server, bounds } = parts;
	let service_ms = bounds.service.unwrap_or(30_000);
	let herdr_ms = bounds.herdr.unwrap_or(15_000);
	let stop_server = Arc::clone(&herdr);
	let mut steps = vec![
		// His open questions go unanswered: Cancel, Deny (0519).
		StopStep::sync("his questions", move || {
			popups.close_all();
			Ok(())
		}),
		StopStep::sync("goodbye", move || {
			broadcast(ServerMessage::Bye);
			Ok(())
		}),
		// Threads at work come back with `--restore` on the next start (0549).
		StopStep::new("noting the threads at work", call!(&service, pause_working))
			.bounded(herdr_ms)
			.when(StopWhen::All),
		StopStep::new("the service", call!(&service, stop)).bounded(service_ms),
		// Their omps end before herdr closes their panes, so their turns stay open for `--restore` (0549).
		StopStep::new("the turns at work", call!(&service, cut_paused_turns)).when(StopWhen::All),
		StopStep::new(
			"the shell's panes",
			Arc::new(move || -> Step {
				let herdr = Arc::clone(&stop_server);
				Box::pin(async move { herdr.stop_server().await.map_err(Error::from) })
			}),
		)
		.bounded(herdr_ms)
		.when(StopWhen::All),
		// What the thread panes' omps left, an earlier herdr server's included, once herdr is gone (0486, 0490).
		StopStep::new("the threads' leftovers", call!(&service, end_thread_leftovers)).when(StopWhen::All),
	];
	steps.extend(extra_stops);
	// Also after a failed start: nothing else would close its port.
	steps.push(StopStep::new("the bridge", bridge));
	steps.push(StopStep::new("the server", server).when(StopWhen::Finally));
	steps.push(
		StopStep::sync("jocasta-service.json", || {
			remove_service_file(std::process::id());
			Ok(())
		})
		.when(StopWhen::Finally),
	);
	steps
}

/// Registered extra stops, each under the number [`Stopper::add`] gave it.
type Extras = Arc<Mutex<Vec<(usize, StopStep)>>>;

type StepsOf = Box<dyn Fn(Vec<StopStep>) -> Vec<StopStep> + Send + Sync>;

/// Runs the stop sequence once, whoever asks first; [`Stopper::stopped`] resolves when it ended.
pub struct Stopper {
	steps: Arc<StepsOf>,
	extras: Extras,
	next: AtomicUsize,
	started: Mutex<bool>,
	stopped: watch::Sender<bool>,
}

/// A `Stopper` over `steps` with the extra stops registered, read when the stop runs.
pub fn stopper(steps: impl Fn(Vec<StopStep>) -> Vec<StopStep> + Send + Sync + 'static) -> Stopper {
	Stopper {
		steps: Arc::new(Box::new(steps)),
		extras: Arc::new(Mutex::new(Vec::new())),
		next: AtomicUsize::new(0),
		started: Mutex::new(false),
		stopped: watch::channel(false).0,
	}
}

impl Stopper {
	/// Starts the stop if none is under way, and resolves when it ended. `all` is the first caller's. The stop runs on a
	/// task of its own, so it goes on if the caller is dropped. Needs a tokio runtime.
	pub async fn stop(&self, all: bool) {
		let first = !std::mem::replace(&mut *self.started.lock().unwrap_or_else(PoisonError::into_inner), true);
		if first {
			let steps = {
				let extras = self.extras.lock().unwrap_or_else(PoisonError::into_inner);
				(self.steps)(extras.iter().map(|(_, step)| step.clone()).collect())
			};
			let done = self.stopped.clone();
			tokio::spawn(async move {
				run_stop_steps(&steps, all, &mut |name, error| {
					log(LogLevel::Warn, "shutdown", &format!("{name} didn't finish: {error}"), None);
				})
				.await;
				done.send_replace(true);
			});
		}
		self.stopped().await;
	}

	/// Registers something else Jocasta started that stops with it (0549), in the order registered. Returns the
	/// function that unregisters it.
	pub fn add(&self, step: StopStep) -> impl FnOnce() + Send + 'static {
		let id = self.next.fetch_add(1, Ordering::SeqCst);
		self.extras.lock().unwrap_or_else(PoisonError::into_inner).push((id, step));
		let extras = Arc::clone(&self.extras);
		move || extras.lock().unwrap_or_else(PoisonError::into_inner).retain(|(have, _)| *have != id)
	}

	/// Whether a stop is under way or done.
	pub fn stopping(&self) -> bool {
		*self.started.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Resolves once the stop ended; every step is bounded, so it always does (and never, if none was asked for).
	pub async fn stopped(&self) {
		let mut ended = self.stopped.subscribe();
		// The sender lives in `self`, so the wait ends only because the stop did.
		let _ = ended.wait_for(|ended| *ended).await;
	}
}

#[cfg(test)]
#[path = "stop_sequence_tests.rs"]
mod tests;
