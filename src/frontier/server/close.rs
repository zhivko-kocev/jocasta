//! Jocasta's close (the Rust port of `close.ts`): the question before it, and the terminal that shows it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};

use crate::frontier::protocol::PopupView;
use crate::frontier::server::popup_routes::Ask;
use crate::frontier::server::popups::{Counts, STOP, stop_question};
use crate::shared::errors::{Error, Result};
use crate::shared::install::self_command;
use crate::shared::log::{LogLevel, log};
use crate::shared::process::on_path;

/// How the close went: everything stopped, or he chose Cancel and everything runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
	Stopped,
	Kept,
}

/// How long a start that hangs is waited for before the threads are counted (LC-9).
const DISCOVERY_WAIT: Duration = Duration::from_secs(10);

/// The argv that opens a new terminal running `command` on Windows (0549): a Windows Terminal window when `wt` is
/// installed; otherwise `command` itself, which `open_jocasta_terminal` starts detached so Windows gives it a console of
/// its own (cmd's `start` would read an unquoted title as the program, SEC-NET-9).
pub fn terminal_argv(command: &[PathBuf], wt: Option<&Path>) -> Vec<PathBuf> {
	match wt {
		Some(wt) => [wt, Path::new("-w"), Path::new("new"), Path::new("--title"), Path::new("Jocasta")]
			.into_iter()
			.map(Path::to_path_buf)
			.chain(command.iter().cloned())
			.collect(),
		None => command.to_vec(),
	}
}

/// A new Windows Terminal window running `jocasta`, which attaches to herdr's session as it always does (0549); a new
/// console window where Windows Terminal isn't installed. Off Windows there is none: the question waits for a client.
fn open_jocasta_terminal() -> Result<()> {
	if !cfg!(windows) {
		log(LogLevel::Warn, "shutdown", "no terminal opens here; the close question waits for `jocasta`", None);
		return Ok(());
	}
	let argv = terminal_argv(&self_command(None), on_path("wt", None).as_deref());
	let Some((program, args)) = argv.split_first() else { return Ok(()) };
	let mut command = std::process::Command::new(program);
	command
		.args(args)
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null());
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// Not hidden: a detached console program gets a console window of its own.
		const DETACHED_PROCESS: u32 = 0x0000_0008;
		command.creation_flags(DETACHED_PROCESS);
	}
	crate::shared::env::apply_to(&mut command);
	command.spawn().map(drop).map_err(Error::from)
}

/// What the close needs of the service: the TypeScript's `Pick<JocastaService, "discovered" | "busyThreads">`.
pub trait CloseService: Send + Sync {
	/// Settles once the threads herdr already runs were found, or the start failed or ended early.
	fn discovered(&self) -> BoxFuture<'static, ()>;
	/// Threads working or awaiting him.
	fn busy_threads(&self) -> Counts;
}

/// What the close needs.
pub struct CloseParts {
	pub service: Arc<dyn CloseService>,
	/// Asks him a question.
	pub question: Ask,
	/// The stop under way, if any.
	pub stopping: Arc<dyn Fn() -> Option<BoxFuture<'static, ()>> + Send + Sync>,
	/// Stops everything (`all`).
	pub stop_all: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
	/// Opens a new terminal attached to herdr's session; `open_jocasta_terminal` by default.
	pub reattach: Option<Arc<dyn Fn() -> Result<()> + Send + Sync>>,
}

/// How a close was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseHow {
	pub ask: bool,
	pub reattach: bool,
}

type Asking = Shared<BoxFuture<'static, CloseOutcome>>;

struct Inner {
	parts: CloseParts,
	asking: Mutex<Option<Asking>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The close (0434, 0438, 0549): threads working or awaiting him ask first (`reattach` first opens a terminal on
/// herdr's session for it: his client is gone), and Cancel keeps everything; otherwise everything stops. One close at a
/// time: a second asks nothing more. The question his quit key opened raises no notification (0581); the one after his
/// terminal closed does.
#[derive(Clone)]
pub struct CloseFlow {
	inner: Arc<Inner>,
}

/// The close: see [`CloseFlow`].
pub fn close_flow(parts: CloseParts) -> CloseFlow {
	CloseFlow { inner: Arc::new(Inner { parts, asking: Mutex::new(None) }) }
}

impl CloseFlow {
	/// Closes Jocasta as `how` says; a close under way is joined, a stop under way is waited for.
	pub async fn close(&self, how: CloseHow) -> CloseOutcome {
		if let Some(stopping) = (self.inner.parts.stopping)() {
			stopping.await;
			return CloseOutcome::Stopped;
		}
		let asking = {
			let mut slot = locked(&self.inner.asking);
			match &*slot {
				Some(asking) => asking.clone(),
				None => {
					// The task clears the slot when it ends, which waits for this lock: the slot is set first.
					let inner = self.inner.clone();
					let task = tokio::spawn(async move {
						let outcome = inner.run(how).await;
						*locked(&inner.asking) = None;
						outcome
					});
					let asking: Asking = (Box::pin(async move { task.await.unwrap_or(CloseOutcome::Stopped) })
						as BoxFuture<'static, _>)
						.shared();
					*slot = Some(asking.clone());
					asking
				}
			}
		};
		asking.await
	}
}

impl Inner {
	async fn run(&self, CloseHow { ask, reattach }: CloseHow) -> CloseOutcome {
		let parts = &self.parts;
		let service = &parts.service;
		// Threads count once herdr's panes were found (LC-9); a start that hangs is waited for 10 s at most.
		if ask {
			let _ = tokio::time::timeout(DISCOVERY_WAIT, service.discovered()).await;
		}
		let counts = if ask { service.busy_threads() } else { Counts { working: 0, awaiting: 0 } };
		let asked = counts.working + counts.awaiting > 0;
		if asked {
			if reattach {
				let open = parts.reattach.clone();
				if let Err(error) = open.map_or_else(open_jocasta_terminal, |open| open()) {
					log(
						LogLevel::Warn,
						"shutdown",
						&format!("no terminal opened for the close question: {error}"),
						None,
					);
				}
			}
			let view: PopupView = stop_question(counts, None);
			let popup = (parts.question)(&view, None, !reattach);
			if popup.answer().await != STOP {
				let now = service.busy_threads();
				log(
					LogLevel::Info,
					"shutdown",
					&format!("kept running: Cancel chosen with {} working and {} awaiting", now.working, now.awaiting),
					None,
				);
				return CloseOutcome::Kept;
			}
		}
		let now = asked.then(|| service.busy_threads());
		log(
			LogLevel::Info,
			"shutdown",
			&now.map_or_else(
				|| "stopping everything".to_owned(),
				|now| {
					format!(
						"stopping everything: Stop chosen with {} working and {} awaiting",
						now.working, now.awaiting
					)
				},
			),
			None,
		);
		(parts.stop_all)().await;
		CloseOutcome::Stopped
	}
}

#[cfg(test)]
#[path = "close_tests.rs"]
mod tests;
