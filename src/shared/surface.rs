//! Bare `jocasta`'s work surface (0387, 0434, 0549): starting the service, attaching herdr's client and closing.

use std::cell::Cell;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::{Pin, pin};
use std::task::Poll;
use std::time::{Duration, UNIX_EPOCH};

use regex::Regex;
use tokio::sync::{OnceCell, mpsc};

use crate::frontier::server::launch::{ServiceHealth, ServiceState};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::herdr_session::herdr_session;
use crate::shared::log::LOG_FILE;
use crate::shared::proctree::{pid_alive, process_started_at};
use crate::shared::service_file::ServiceFile;

/// `SurfaceDeps::watch_service_stop`'s check: whether the service has since stopped herdr itself.
pub type StopCheck = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = bool>>>>;

/// What bare `jocasta` needs from outside.
pub trait SurfaceDeps {
	/// What the service `<data>/jocasta-service.json` names is doing (`service_health`): its `/health` with the file's
	/// token and pid, not merely whether that pid is alive, since a stale file's pid may belong to another program
	/// (CR-15).
	fn service_health(&self) -> impl Future<Output = ServiceHealth>;
	/// Starts the service (`jocasta service`) as a plain process, detached from this terminal (0549, 0552). It takes the
	/// single-instance lock itself: one that finds another service answering leaves, and one that finds a hung one ends
	/// it and starts (0534). The future starts it when first polled, which `open_surface` does at once, and resolves
	/// the service's exit code when it ends.
	fn start_service(&self) -> impl Future<Output = i32>;
	/// The service's last ERROR line in Jocasta's log since `since` (epoch ms), its message only (`last_logged_error`).
	fn last_error(&self, since: i64) -> Option<String>;
	/// The herdr binary, or `None`.
	fn herdr(&self) -> Option<&Path>;
	/// `~/.jocasta/agent/herdr.toml`.
	fn herdr_config(&self) -> &Path;
	/// Runs herdr's client in this terminal; resolves its exit code, or why it couldn't start.
	fn attach(&self, argv: &[String], env: &HashMap<String, String>) -> impl Future<Output = Result<i32>>;
	fn print(&self, line: &str);
	/// Waits this long (ms) for a service it started to answer before attaching. The service answers at once and makes
	/// the boss pane while it starts, so the pane may appear shortly after attaching.
	fn wait_ms(&self) -> Option<i64> {
		None
	}
	fn sleep(&self, ms: u64) -> impl Future<Output = ()> {
		tokio::time::sleep(Duration::from_millis(ms))
	}
	/// Marks this `jocasta` as attached to the session (`attached_surface`); the function it returns is `leave`, which
	/// says whether it was the last.
	fn enter_surface(&self) -> Box<dyn Fn() -> bool>;
	/// Calls `handler` when this terminal closes (the Windows console close event, with a few seconds left); returns the
	/// function that stops listening.
	fn on_terminal_close(&self, handler: Box<dyn Fn() + Send + Sync>) -> Box<dyn FnOnce()>;
	/// Tells the running service his client closed (`POST /api/shutdown`, 0549): it stops everything, or asks first
	/// while threads are at work. True when it took it, false when no service answers.
	fn tell_service_closed(&self) -> impl Future<Output = bool>;
	/// No service: stops herdr session `jocasta`'s server (only that session), never starting it.
	fn stop_herdr(&self) -> impl Future<Output = Result<()>>;
	/// Ends this process once Jocasta was told to close after its terminal closed.
	fn exit(&self);
	/// Notes which service `jocasta-service.json` names as herdr's client starts; the returned check says whether that
	/// service has since stopped herdr itself (`service_stopped`, 0491).
	fn watch_service_stop(&self) -> StopCheck;
	/// Whether herdr's server for the session answers a ping; never starts it (0518).
	fn herdr_up(&self) -> impl Future<Output = bool>;
}

/// How long bare `jocasta` waits for the service to start a dead herdr server again before closing (0518).
const HERDR_BACK_MS: u64 = 30_000;
/// How long bare `jocasta` waits for a service it started: a hung one it replaces takes `HUNG_MS` first (0534).
const SERVICE_START_MS: i64 = 90_000;
/// A started service is asked every 50 ms for its first 3 s, then every 250 ms (PERF-7).
const FAST_POLL_MS: i64 = 3_000;

/// A line of Jocasta's log (log.ts): `<ISO time> <LEVEL> [<scope>] <message>`; `.` is JavaScript's.
fn log_line() -> Regex {
	Regex::new(r"^(\S+) ERROR \[[^\]]*\] ([^\n\r\u{2028}\u{2029}]*)$").expect("the log line pattern")
}

/// The message of the last ERROR line in Jocasta's log folder `dir` written at or after `since` (epoch ms), from the
/// newest two day files; `None` when there is none. The service's "didn't start: " lead is left out (SHR-5).
pub fn last_logged_error(dir: &Path, since: i64) -> Option<String> {
	let mut names: Vec<String> = fs::read_dir(dir)
		.ok()?
		.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
		.filter(|name| LOG_FILE.is_match(name))
		.collect();
	names.sort();
	let pattern = log_line();
	let mut last: Option<String> = None;
	for name in names.iter().skip(names.len().saturating_sub(2)) {
		let Ok(text) = fs::read_to_string(dir.join(name)) else { continue };
		for line in text.lines() {
			let Some(found) = pattern.captures(line) else { continue };
			let at = chrono::DateTime::parse_from_rfc3339(&found[1]).map(|at| at.timestamp_millis());
			if at.is_ok_and(|at| at >= since - 1_000) {
				last = Some(found[2].to_owned());
			}
		}
	}
	last.map(|message| message.strip_prefix("didn't start: ").map_or_else(|| message.clone(), str::to_owned))
}

/// `<dir>/<pid>`: one file per bare `jocasta` attached to herdr's session `jocasta` (0434). Leaving removes this one's and
/// answers whether no other live `jocasta` is still attached, so a client that exits while another `jocasta` holds the
/// session (a second terminal, a re-attach) never closes Jocasta. A file counts only while its process lives and
/// started before the file was written (LC-4: Windows reuses pids); the others are removed.
pub struct AttachedSurface {
	dir: std::path::PathBuf,
	pid: u32,
	alive: Box<dyn Fn(u32) -> bool>,
	started_at: Box<dyn Fn(u32) -> Option<i64>>,
}

/// `attachedSurface`: `pid`, `alive` and `started_at` default to this process's and the real ones.
pub fn attached_surface(
	dir: &Path,
	pid: Option<u32>,
	alive: Option<Box<dyn Fn(u32) -> bool>>,
	started_at: Option<Box<dyn Fn(u32) -> Option<i64>>>,
) -> Result<AttachedSurface> {
	let pid = pid.unwrap_or_else(std::process::id);
	fs::create_dir_all(dir)?;
	fs::write(dir.join(pid.to_string()), "")?;
	Ok(AttachedSurface {
		dir: dir.to_path_buf(),
		pid,
		alive: alive.unwrap_or_else(|| Box::new(pid_alive)),
		started_at: started_at.unwrap_or_else(|| Box::new(process_started_at)),
	})
}

impl AttachedSurface {
	/// Removes this one's file; true when no other live `jocasta` is still attached.
	pub fn leave(&self) -> bool {
		let _ = fs::remove_file(self.dir.join(self.pid.to_string()));
		let mut others = false;
		let Ok(entries) = fs::read_dir(&self.dir) else { return true };
		for name in entries.filter_map(|entry| entry.ok()?.file_name().into_string().ok()) {
			let Ok(other) = name.parse::<u32>() else { continue };
			if other == self.pid {
				continue;
			}
			let file = self.dir.join(&name);
			if (self.alive)(other) {
				let written = fs::metadata(&file)
					.and_then(|meta| meta.modified())
					.ok()
					.and_then(|at| at.duration_since(UNIX_EPOCH).ok())
					.map_or(0, |at| i64::try_from(at.as_millis()).unwrap_or(0));
				if (self.started_at)(other).is_none_or(|started| started <= written + 1_000) {
					others = true;
					continue;
				}
			}
			let _ = fs::remove_file(file);
		}
		!others
	}
}

/// Whether the service `before` named (as herdr's client started) stopped herdr's server itself (0491): it is closing
/// Jocasta (`/health`'s `closing`, set before its Stop everything stops herdr), or its close already ended by removing
/// `jocasta-service.json` (`now` no longer names it). A service that died leaves its file and answers nothing: not a stop.
pub async fn service_stopped(
	before: Option<&ServiceFile>,
	now: Option<&ServiceFile>,
	state: impl AsyncFn(&ServiceFile) -> ServiceState,
) -> bool {
	let Some(before) = before else { return false };
	let Some(now) = now else { return true };
	if now.pid != before.pid || now.token != before.token {
		return true;
	}
	state(now).await == ServiceState::Closing
}

/// What closed Jocasta: herdr's client exiting, or the terminal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
	Client,
	Terminal,
}

/// Closing Jocasta, once: whichever trigger comes first does it, and a later one waits for that to end.
struct Closer<'a, D> {
	deps: &'a D,
	session: &'a str,
	leave: Box<dyn Fn() -> bool>,
	started: Cell<bool>,
	done: OnceCell<()>,
}

impl<D: SurfaceDeps> Closer<'_, D> {
	async fn close(&self, trigger: Trigger) {
		self.started.set(true);
		self.done
			.get_or_init(|| async {
				if !(self.leave)() {
					return;
				}
				if self.deps.tell_service_closed().await {
					if trigger == Trigger::Client {
						self.deps
							.print("jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.");
					}
					return;
				}
				if let Err(error) = self.deps.stop_herdr().await {
					self.deps.print(&format!("jocasta: herdr session {} didn't stop: {error}", self.session));
				}
			})
			.await;
	}
}

/// Polls `future` once, keeping its result in `result`.
async fn poll_once(future: &mut Pin<&mut impl Future<Output = i32>>, result: &mut Option<i32>) {
	if result.is_none()
		&& let Poll::Ready(code) = std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
	{
		*result = Some(code);
	}
}

/// A service that is closing is waited for first (LC-3).
async fn wait_while_closing(deps: &impl SurfaceDeps, mut state: ServiceState) -> Result<ServiceState> {
	if state != ServiceState::Closing {
		return Ok(state);
	}
	deps.print("jocasta: Jocasta is closing; it starts again once it has.");
	let end = now_ms() + 60_000;
	while state == ServiceState::Closing {
		if now_ms() >= end {
			return Err(Error::user_with_code(
				"Jocasta is still closing after a minute; run jocasta again in a moment.",
				1,
			));
		}
		deps.sleep(250).await;
		state = deps.service_health().await.state;
	}
	Ok(state)
}

/// Starts the service and waits for it. `Some(1)` when it ended before answering: attaching would show a work surface
/// without Jocasta (0582).
async fn start_and_wait(deps: &impl SurfaceDeps) -> Option<i32> {
	// Down, or not answering (hung, 0534): a new service takes over or leaves on its own.
	let started = now_ms();
	let mut exited: Option<i32> = None;
	let starting = deps.start_service();
	let mut starting = pin!(starting);
	poll_once(&mut starting, &mut exited).await;
	let end = started + deps.wait_ms().unwrap_or(SERVICE_START_MS);
	let mut up = false;
	while !up && now_ms() < end {
		deps.sleep(if now_ms() - started < FAST_POLL_MS { 50 } else { 250 }).await;
		poll_once(&mut starting, &mut exited).await;
		up = deps.service_health().await.state == ServiceState::Up;
		// It ended before answering: unless another service answers now (it left for that one), it failed to start.
		if let (false, Some(code)) = (up, exited) {
			up = deps.service_health().await.state == ServiceState::Up;
			if up {
				break;
			}
			match deps.last_error(started) {
				Some(why) if !why.is_empty() => {
					deps.print(&format!("jocasta: the service stopped while starting: {why}"))
				}
				_ => deps.print(&format!("jocasta: the service stopped while starting (exit code {code})")),
			}
			deps.print("jocasta: run `jocasta diagnose` for the full log.");
			return Some(1);
		}
	}
	if !up {
		deps.print("jocasta: the service is still starting; the boss's workspace appears when it is up.");
	}
	None
}

/// After the client exited: whether herdr's server crashed under it and is back (0518, 0534). Its server running means
/// he quit the client, unless the service started a stopped server again since the client attached (`revivals` grew):
/// then the crash and the restart both came before this look. A service that doesn't answer, or is closing, starts
/// nothing; and herdr not back within 30 s is a close as before. The ping never starts herdr itself: the service does
/// (0431).
async fn herdr_back<D: SurfaceDeps>(closer: &Closer<'_, D>, revivals: Option<u64>) -> bool {
	let deps = closer.deps;
	if deps.herdr_up().await {
		let now = deps.service_health().await.herdr_revivals;
		return revivals.zip(now).is_some_and(|(before, now)| now > before);
	}
	let answers = async || matches!(deps.service_health().await.state, ServiceState::Up | ServiceState::Busy);
	if !answers().await {
		return false;
	}
	deps.print(&format!(
		"jocasta: herdr session {} stopped; waiting for the service to start it again.",
		closer.session
	));
	let mut waited = 0;
	while waited < HERDR_BACK_MS {
		deps.sleep(250).await;
		// The terminal closed meanwhile: Jocasta is closing, nothing to attach to.
		if closer.started.get() {
			return false;
		}
		if deps.herdr_up().await {
			return true;
		}
		if !answers().await {
			return false;
		}
		waited += 250;
	}
	deps.print(&format!("jocasta: herdr session {} isn't back after 30 s.", closer.session));
	false
}

/// Herdr's client, attached again each time its server crashed and came back; the client's exit code, or 0 when the
/// service stopped herdr itself.
async fn run_client<D: SurfaceDeps>(
	closer: &Closer<'_, D>,
	herdr: &Path,
	env: &HashMap<String, String>,
) -> Result<i32> {
	let deps = closer.deps;
	let argv = [herdr.to_string_lossy().into_owned(), "--session".to_owned(), closer.session.to_owned()];
	loop {
		let service_stop = deps.watch_service_stop();
		let revivals = deps.service_health().await.herdr_revivals;
		let code = deps.attach(&argv, env).await?;
		// Checked before closing: the service's close is still under way, or has just removed its file. Its own stop
		// (Stop everything) already ended herdr: nothing to close or say (0491).
		if code != 0 && service_stop().await {
			return Ok(0);
		}
		// A crashed server isn't a close: once the service has started herdr again, its client is attached again in
		// this terminal, as the first time (0518, 0534).
		if herdr_back(closer, revivals).await {
			continue;
		}
		closer.close(Trigger::Client).await;
		return Ok(code);
	}
}

/// Closes Jocasta when the terminal does, then ends this process; never returns.
async fn on_terminal_closed<D: SurfaceDeps>(
	closer: &Closer<'_, D>,
	mut closed: mpsc::UnboundedReceiver<()>,
) -> std::convert::Infallible {
	if closed.recv().await.is_some() {
		closer.close(Trigger::Terminal).await;
		closer.deps.exit();
	}
	std::future::pending().await
}

/// Bare `jocasta` (0387, 0392, 0404, 0434, 0549, 0552): starts the service when none answers and waits for it (it makes
/// workspace "Jocasta" with its boss pane first), then attaches herdr's client to session `jocasta` in this terminal
/// with Jocasta's herdr config. A service that is closing is waited for first. When the client exits because herdr's
/// server crashed (it stopped and the service started it again, or starts it within 30 s: 0518, 0534), the client is
/// attached again. Otherwise, when the client exits (quit or detach) or the terminal closes, and no other `jocasta` is
/// attached, the service is told: it stops everything, or with threads at work opens a terminal on the session again
/// and asks first (0549). With no service, herdr session `jocasta` stops here. Resolves herdr's exit
/// code, or 0 when the client exited because the service stopped herdr itself (Stop everything; 0491).
pub async fn open_surface(deps: &impl SurfaceDeps) -> Result<i32> {
	let Some(herdr) = deps.herdr() else {
		return Err(Error::user_with_code("herdr isn't installed: install it from https://herdr.dev", 1));
	};
	let session = herdr_session()?;
	let state = wait_while_closing(deps, deps.service_health().await.state).await?;
	if state != ServiceState::Up
		&& let Some(code) = start_and_wait(deps).await
	{
		return Ok(code);
	}
	let mut env = HashMap::new();
	if deps.herdr_config().exists() {
		env.insert("HERDR_CONFIG_PATH".to_owned(), deps.herdr_config().to_string_lossy().into_owned());
	} else {
		deps.print(&format!(
			"jocasta: {} is missing; run `jocasta setup` for Jocasta's herdr look.",
			deps.herdr_config().display()
		));
	}
	let closer = Closer {
		deps,
		session: &session,
		leave: deps.enter_surface(),
		started: Cell::new(false),
		done: OnceCell::new(),
	};
	// The terminal closing ends herdr's client too; whichever comes first closes Jocasta, once.
	let (sender, closed) = mpsc::unbounded_channel();
	let unlisten = deps.on_terminal_close(Box::new(move || {
		let _ = sender.send(());
	}));
	let result = tokio::select! {
		result = run_client(&closer, herdr, &env) => result,
		never = on_terminal_closed(&closer, closed) => match never {},
	};
	unlisten();
	// A client that couldn't start closes nothing, but this `jocasta` is no longer attached.
	if !closer.started.get() {
		(closer.leave)();
	}
	result
}

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
