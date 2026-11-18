//! The Jocasta service inside herdr's server (fork plan phase 4): one process. Bare `jocasta` launches herdr's server
//! with `JOCASTA_SERVICE=1`, and that server runs the service as its `ServerExtension`: it starts once the server
//! listens, hears when his last terminal leaves (0549's close: stop at once, or ask first while threads are at work),
//! and ends before the process does. A plain `jocasta server` (a test's, the benchmark's) runs none. The service takes
//! the single-instance lock as the plain service did (taking over a hung holder, 0534) and starts the push-to-talk hook
//! (0556). herdr's own signal handling ends the server; the service's stop follows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use herdr::embed::{ServerExtension, ServerHost};
use tokio::runtime::Runtime;

use crate::frontier::server::herdr::herdr_pipe;
use crate::frontier::server::instance::{Instance, InstanceOptions, take_instance};
use crate::frontier::server::launch::service_state;
use crate::frontier::server::native::Native;
use crate::frontier::server::ptt::start_push_to_talk;
use crate::frontier::server::server::{JocastaServer, ServerOptions, start_server};
use crate::shared::herdr_session::herdr_session;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::proctree::end_processes;
use crate::shared::service_file::read_service_file;

/// How long the process waits for the service to end after herdr's server stopped: every stop step is bounded, so
/// this only guards against a hang.
const STOP_WAIT: Duration = Duration::from_secs(60);

/// The service herdr's server runs: see the module docs.
#[derive(Default)]
pub struct EmbeddedService {
	running: Option<Running>,
}

struct Running {
	runtime: Runtime,
	/// The server once it listens; `None` while it starts or when it couldn't.
	server: Arc<OnceLock<JocastaServer>>,
	/// herdr's client shells: there from the start, so no shell that attaches early is missed.
	shell: Arc<Native>,
	/// The task that runs the service until it stops.
	task: tokio::task::JoinHandle<()>,
}

impl ServerExtension for EmbeddedService {
	fn started(&mut self, host: ServerHost) {
		let socket = herdr_pipe(&host.api_socket.to_string_lossy());
		let stop = Arc::clone(&host.stop);
		let session = match herdr_session() {
			Ok(session) => session,
			Err(error) => return failed(&error.to_string(), &stop),
		};
		let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
			Ok(runtime) => runtime,
			Err(error) => return failed(&error.to_string(), &stop),
		};
		let shell = Native::new(host, runtime.handle().clone());
		let server = Arc::new(OnceLock::new());
		let task = runtime.spawn(run(session, socket, stop, Arc::clone(&server), Arc::clone(&shell)));
		self.running = Some(Running { runtime, server, shell, task });
	}

	fn last_client_left(&mut self) {
		let Some(running) = &self.running else { return };
		if let Some(server) = running.server.get() {
			let server = server.clone();
			running.runtime.spawn(async move { server.client_closed().await });
		}
	}

	fn stopped(&mut self) {
		let Some(Running { runtime, server, task, .. }) = self.running.take() else { return };
		runtime.block_on(async move {
			// herdr's server is gone: Jocasta closes as one, unasked (a stop already under way is joined).
			if let Some(server) = server.get() {
				let server = server.clone();
				tokio::spawn(async move { server.shutdown(false).await });
			}
			if tokio::time::timeout(STOP_WAIT, task).await.is_err() {
				log(LogLevel::Warn, "service", "didn't stop within a minute of herdr's server; ending", None);
			}
		});
		runtime.shutdown_timeout(Duration::from_secs(1));
	}

	fn client_joined(&mut self, client: u64, pid: Option<u32>) {
		if let Some(running) = &self.running {
			running.shell.joined(client, pid);
		}
	}

	fn client_left(&mut self, client: u64) {
		if let Some(running) = &self.running {
			running.shell.left(client);
		}
	}

	fn client_message(&mut self, client: u64, kind: &str, data: &str) {
		if let Some(running) = &self.running {
			running.shell.message(client, kind, data);
		}
	}

	fn action(&mut self, action: &str, pane_id: Option<&str>) -> bool {
		self.running.as_ref().is_some_and(|running| running.shell.action(action, pane_id))
	}
}

/// Takes the single-instance lock: the lock this service holds, or `None` when another service holds it and answers.
async fn take_lock() -> crate::shared::errors::Result<Option<Instance>> {
	take_instance(InstanceOptions::new(
		data_dir().join("service.lock"),
		|| async { service_state(read_service_file().as_ref(), None).await },
		|holder, spare| async move {
			end_processes(&[holder], &spare).await;
		},
	))
	.await
}

/// Runs the service until it stops, publishing its server once it listens.
async fn run(
	session: String,
	socket: String,
	stop: Arc<dyn Fn() + Send + Sync>,
	published: Arc<OnceLock<JocastaServer>>,
	shell: Arc<Native>,
) {
	let instance = match take_lock().await {
		Ok(Some(instance)) => instance,
		Ok(None) => return failed("another Jocasta service is running; close it first", &stop),
		Err(error) => return failed(&error.to_string(), &stop),
	};
	let options = ServerOptions {
		herdr_session: Some(session),
		herdr_socket: Some(socket),
		shell: Some(Arc::clone(&shell)),
		..ServerOptions::default()
	};
	let server = match start_server(options).await {
		Ok(server) => server,
		Err(error) => {
			release(&instance);
			return failed(&error.to_string(), &stop);
		}
	};
	shell.attach(Arc::new(server.clone()));
	let _ = published.set(server.clone());
	// A start that fails after the server listened (LC-10) stops what it started, herdr's server among its steps (the
	// panes' processes listed first, 0486), so the service says it is closing before herdr's client ends; herdr's own
	// stop follows here, once that stop ended. The failure is marked before its stop begins: the stop's end wakes this
	// task too, which then aborts the watcher. The service's own start logged why.
	let failed = server.clone();
	let start_failed = Arc::new(AtomicBool::new(false));
	let marked = Arc::clone(&start_failed);
	let watching = tokio::spawn(async move {
		if failed.ready().await.is_err() {
			marked.store(true, Ordering::SeqCst);
			failed.stop().await;
		}
	});
	// Right Ctrl toggles omp's dictation in the boss pane (0556).
	let service = Arc::clone(&server.service);
	let ptt = start_push_to_talk(move || {
		let service = Arc::clone(&service);
		async move { service.toggle_dictation().await }
	});
	server.stopped().await;
	watching.abort();
	if start_failed.load(Ordering::SeqCst) {
		stop();
	}
	if let Some(ptt) = ptt {
		ptt.stop().await;
	}
	release(&instance);
}

/// A service that couldn't start says why and stops its herdr server: a work surface without Jocasta would mislead
/// him, and bare `jocasta` says why once its client ends (0582).
fn failed(why: &str, stop: &Arc<dyn Fn() + Send + Sync>) {
	log(LogLevel::Error, "service", &format!("didn't start: {why}"), None);
	stop();
}

fn release(instance: &Instance) {
	if let Err(error) = instance.release() {
		log(LogLevel::Warn, "service", &format!("couldn't give the lock up: {error}"), None);
	}
}

/// The env var bare `jocasta` sets for the herdr server it launches: that server runs the service.
pub const SERVICE_ENV: &str = "JOCASTA_SERVICE";

/// Whether `jocasta <herdr words>` runs the service: a server run (its words are `server` alone, after
/// `--session <name>` if any) that bare `jocasta` launched (`service_env` is `JOCASTA_SERVICE`'s value).
pub fn serves(args: &[String], service_env: Option<&str>) -> bool {
	let mut words = args.iter().map(String::as_str);
	let mut rest = Vec::new();
	while let Some(word) = words.next() {
		if word == "--session" {
			words.next();
		} else if !word.starts_with("--session=") {
			rest.push(word);
		}
	}
	service_env == Some("1") && rest == ["server"]
}

/// Registers the service with herdr's server for this process.
pub fn register() {
	herdr::embed::set_server_extension(Box::new(EmbeddedService::default()));
}

#[cfg(test)]
#[path = "embedded_tests.rs"]
mod tests;
