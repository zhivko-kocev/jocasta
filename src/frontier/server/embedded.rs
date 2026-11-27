//! Bare `jocasta` (0683): one process holds the service and Jocasta's own shell. The main thread runs the shell (the
//! terminal window, `frontier/shell`); a tokio runtime beside it runs the service. They meet in-process: the service's
//! surface is [`OwnSurface`] over the shell's tabs, its shell half is [`Native`] over the UI thread's channel, and the
//! panes' agent state is what the bridge says (0675). Closing the window ends the process: the console input ends, the
//! shell returns, and the service stops everything at once (no question can be asked of a closed window). The service
//! takes the single-instance lock as the plain service did (taking over a hung holder, 0534) and starts the
//! push-to-talk hook (0556). A service that can't start logs why and the window closes; bare `jocasta` says it (0582).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use tokio::sync::Notify;

use crate::frontier::server::bridge::Bridge;
use crate::frontier::server::instance::{Instance, InstanceOptions, take_instance};
use crate::frontier::server::launch::service_state;
use crate::frontier::server::native::{Native, ShellHost, Target};
use crate::frontier::server::own_surface::OwnSurface;
use crate::frontier::server::ptt::start_push_to_talk;
use crate::frontier::server::server::{ServerOptions, start_server};
use crate::frontier::shell::{AgentInfo, Hub, JocastaShell, Parts, Ui, run};
use crate::frontier::tui::load_palette;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::proctree::end_processes;
use crate::shared::service_file::read_service_file;

/// How long the process waits for the service to end once the window closed: every stop step is bounded, so this only
/// guards against a hang. 50 s, inside the 60 s a closing terminal (foot and others) gives its process group before
/// SIGKILL (0702, #6).
const STOP_WAIT: Duration = Duration::from_secs(50);
/// How long the window's own loop is given to take the word to leave from a signal before the process ends anyway.
#[cfg(unix)]
const UI_QUIT_WAIT: Duration = Duration::from_secs(5);

/// Takes the single-instance lock: the lock this service holds, or `None` when another service holds it and answers.
async fn take_lock() -> Result<Option<Instance>> {
	take_instance(InstanceOptions::new(
		data_dir().join("service.lock"),
		|| async { service_state(read_service_file().as_ref(), None).await },
		|holder, spare| async move {
			end_processes(&[holder], &spare).await;
		},
	))
	.await
}

fn release(instance: &Instance) {
	if let Err(error) = instance.release() {
		log(LogLevel::Warn, "service", &format!("couldn't give the lock up: {error}"), None);
	}
}

/// A service that couldn't start says why in the log and closes the window.
fn failed(why: &str, ui: &Sender<Ui>) {
	log(LogLevel::Error, "service", &format!("didn't start: {why}"), None);
	let _ = ui.send(Ui::Quit);
}

/// Runs the service with `options` until it stops or `closed` is notified (the window closed), then stops everything
/// at once.
async fn serve(native: Arc<Native>, options: ServerOptions, ui: Sender<Ui>, closed: Arc<Notify>) {
	let instance = match take_lock().await {
		Ok(Some(instance)) => instance,
		Ok(None) => return failed("another Jocasta service is running; close it first", &ui),
		Err(error) => return failed(&error.to_string(), &ui),
	};
	let server = match start_server(options).await {
		Ok(server) => server,
		Err(error) => {
			release(&instance);
			return failed(&error.to_string(), &ui);
		}
	};
	native.attach(Arc::new(server.clone()));
	// A start that fails after the server listened (LC-10) stops what it started, then the window closes; the
	// service's own start logged why.
	let watched = server.clone();
	let starting = tokio::spawn(async move {
		if watched.ready().await.is_err() {
			watched.stop().await;
		}
	});
	// Right Ctrl toggles omp's dictation in the boss pane (0556).
	let dictation = Arc::clone(&server.service);
	let ptt = start_push_to_talk(move || {
		let service = Arc::clone(&dictation);
		async move { service.toggle_dictation().await }
	});
	tokio::select! {
		() = server.stopped() => {}
		() = closed.notified() => {
			server.shutdown(false).await;
			server.stopped().await;
		}
	}
	starting.abort();
	if let Some(ptt) = ptt {
		ptt.stop().await;
	}
	release(&instance);
	let _ = ui.send(Ui::Quit);
}

/// Runs Jocasta: the service on a runtime of its own and the shell here, until the window closes or the service
/// stops. The caller took the secrets out of the environment before any thread started.
pub fn run_embedded() -> Result<()> {
	load_palette(&config_dir().join("palette.json"));
	// Asked once, before the shell or the service reads the console: the shell asks for the keys with it, and the
	// first-run checks say when the terminal can't (0703).
	let enhanced = ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(2)
		.enable_all()
		.build()
		.map_err(|error| Error::other(format!("the runtime didn't start: {error}")))?;
	let (tx, rx) = channel::<Ui>();
	let hub = Hub::new(tx.clone());
	let bridge = Bridge::new();
	let asked = bridge.clone();
	hub.set_lookup(Arc::new(move |pane_id| {
		let agent = asked.agent(pane_id);
		AgentInfo {
			connected: asked.connected(pane_id),
			working: agent.as_ref().is_some_and(|agent| agent.working),
			input_open: agent.as_ref().is_some_and(|agent| agent.input_open),
			session: asked.session(pane_id).map(Into::into),
		}
	}));
	let to_ui = tx.clone();
	let host = ShellHost::new(
		Arc::new(move |_target: Target, kind: &str, data: String| {
			let _ = to_ui.send(Ui::Service(kind.to_owned(), data));
		}),
		hub.changed(),
	);
	let native = Native::new(host, runtime.handle().clone());
	let options = ServerOptions {
		herdr: Some(OwnSurface::new(Arc::clone(&hub))),
		shell: Some(Arc::clone(&native)),
		bridge: Some(bridge),
		first_run: Some(enhanced),
		clean_room: true,
		ask_system: true,
		..ServerOptions::default()
	};
	let closed = Arc::new(Notify::new());
	// The service's end is awaited on a plain channel, not on the runtime's timer: a runtime whose workers are all held
	// (the start hang of 2026-11-27) runs no timer, and the wait would never end.
	let (ended, service_ended) = channel::<()>();
	let serving = serve(Arc::clone(&native), options, tx.clone(), Arc::clone(&closed));
	runtime.spawn(async move {
		serving.await;
		let _ = ended.send(());
	});
	let ui_returned = Arc::new(AtomicBool::new(false));
	#[cfg(unix)]
	quit_on_signals(tx.clone(), Arc::clone(&ui_returned));
	let parts = Parts { hub, shell: JocastaShell::new(now_ms), native, tx, rx, enhanced };
	crate::shared::log::set_stderr(false);
	let ran = run(parts);
	ui_returned.store(true, Ordering::SeqCst);
	crate::shared::log::set_stderr(true);
	// The window closed, or the service stopped under it: what is left stops now, and the process waits for it.
	closed.notify_one();
	if service_ended.recv_timeout(STOP_WAIT).is_err() {
		log(LogLevel::Warn, "service", "didn't stop within a minute; ending", None);
	}
	runtime.shutdown_timeout(Duration::from_secs(1));
	ran.map_err(|error| Error::other(format!("the terminal failed: {error}")))
}

/// A closed window hangs up its terminal on Linux and macOS, and the default for that signal ends the process on the
/// spot, the service file and every pane left behind; it and a plain terminate are the word to leave, as the console's
/// close event is on Windows. They are awaited on a thread and runtime of their own, so they are heard while the
/// service's runtime is held up (a hung start). A process that doesn't end after one ends anyway: at once when the
/// window's own loop didn't take the word within 5 s, else once the service's stop had its 50 s.
#[cfg(unix)]
fn quit_on_signals(quit: Sender<Ui>, ui_returned: Arc<AtomicBool>) {
	use tokio::signal::unix::{SignalKind, signal};
	std::thread::spawn(move || {
		let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
		runtime.block_on(async move {
			let (Ok(mut hangup), Ok(mut terminate)) = (signal(SignalKind::hangup()), signal(SignalKind::terminate()))
			else {
				return;
			};
			tokio::select! {
				_ = hangup.recv() => {}
				_ = terminate.recv() => {}
			}
			let _ = quit.send(Ui::Quit);
			tokio::time::sleep(UI_QUIT_WAIT).await;
			if ui_returned.load(Ordering::SeqCst) {
				tokio::time::sleep(STOP_WAIT + UI_QUIT_WAIT).await;
			}
			log(LogLevel::Warn, "service", "didn't end after the word to leave; ending now", None);
			std::process::exit(1);
		});
	});
}
