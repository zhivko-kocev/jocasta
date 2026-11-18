//! `jocasta service` (the Rust port of `run.ts`; 0549, 0552): the Jocasta service as a plain process, started detached by
//! bare `jocasta`. It takes the single-instance lock (taking over a hung holder, 0534, sparing the herdr server it
//! started, 0582), starts the server and the push-to-talk hook, and exits once Jocasta has closed.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::frontier::server::instance::{Instance, InstanceOptions, take_instance};
use crate::frontier::server::launch::service_state;
use crate::frontier::server::ptt::start_push_to_talk;
use crate::frontier::server::server::{JocastaServer, ServerOptions, start_server};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::proctree::end_processes;
use crate::shared::service_file::read_service_file;

/// Resolves when a signal that asks the service to end arrives (logoff, a kill that lets it, Ctrl-C): SIGINT, SIGTERM
/// and SIGHUP, and on Windows their console counterparts. Each arrival resolves it once.
async fn asked_to_end() {
	#[cfg(windows)]
	{
		use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_logoff, ctrl_shutdown};
		let (Ok(mut c), Ok(mut b), Ok(mut close), Ok(mut logoff), Ok(mut shutdown)) =
			(ctrl_c(), ctrl_break(), ctrl_close(), ctrl_logoff(), ctrl_shutdown())
		else {
			return std::future::pending().await;
		};
		tokio::select! {
			_ = c.recv() => {},
			_ = b.recv() => {},
			_ = close.recv() => {},
			_ = logoff.recv() => {},
			_ = shutdown.recv() => {},
		}
	}
	#[cfg(unix)]
	{
		use tokio::signal::unix::{SignalKind, signal};
		let kinds = [SignalKind::interrupt(), SignalKind::terminate(), SignalKind::hangup()];
		let mut streams: Vec<_> = kinds.into_iter().filter_map(|kind| signal(kind).ok()).collect();
		if streams.is_empty() {
			return std::future::pending().await;
		}
		let waits = streams.iter_mut().map(|stream| Box::pin(stream.recv())).collect::<Vec<_>>();
		futures_util::future::select_all(waits).await;
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

/// Closes Jocasta as one on every end signal, unasked, until the service has ended.
fn close_on_signals(server: &JocastaServer) -> tokio::task::JoinHandle<()> {
	let server = server.clone();
	tokio::spawn(async move {
		loop {
			asked_to_end().await;
			let server = server.clone();
			tokio::spawn(async move { server.shutdown(false).await });
		}
	})
}

/// Runs the service until Jocasta closes; resolves the exit code: 0 when it closed (or another service runs), 1 when
/// it couldn't start or its start failed after the server listened. A lock that can't be taken is logged and exits 1
/// (the TypeScript's rejection).
pub async fn run_service() -> i32 {
	let instance = match take_lock().await {
		Ok(Some(instance)) => Arc::new(instance),
		Ok(None) => {
			log(LogLevel::Info, "service", "another Jocasta service is running; this one leaves", None);
			return 0;
		}
		Err(error) => {
			log(LogLevel::Error, "service", &format!("didn't start: {error}"), None);
			return 1;
		}
	};
	let noted = Arc::clone(&instance);
	let options = ServerOptions {
		// The herdr server it starts is noted beside the lock: a takeover of this service, hung, spares it (0582).
		on_herdr_server: Some(Arc::new(move |herdr| {
			if let Err(error) = noted.note_herdr(herdr) {
				log(LogLevel::Warn, "service", &format!("couldn't note herdr's server beside the lock: {error}"), None);
			}
		})),
		..ServerOptions::default()
	};
	let server = match start_server(options).await {
		Ok(server) => server,
		Err(error) => {
			log(LogLevel::Error, "service", &format!("didn't start: {error}"), None);
			release(&instance);
			return 1;
		}
	};
	let code = Arc::new(AtomicI32::new(0));
	// A start that fails after the server listened stops what it started (herdr's connection, whiteboard,
	// `jocasta-service.json`, LC-10).
	let (failed, failing) = (server.clone(), Arc::clone(&code));
	let watching = tokio::spawn(async move {
		if failed.ready().await.is_err() {
			failing.store(1, Ordering::SeqCst);
			failed.stop().await;
		}
	});
	// Right Ctrl toggles omp's dictation in the boss pane (0556).
	let service = Arc::clone(&server.service);
	let ptt = start_push_to_talk(move || {
		let service = Arc::clone(&service);
		async move { service.toggle_dictation().await }
	});
	// Asked to end by the system (logoff, a kill that lets it): Jocasta closes as one, unasked.
	let signals = close_on_signals(&server);
	server.stopped().await;
	signals.abort();
	watching.abort();
	if let Some(ptt) = ptt {
		ptt.stop().await;
	}
	release(&instance);
	code.load(Ordering::SeqCst)
}

fn release(instance: &Instance) {
	if let Err(error) = instance.release() {
		log(LogLevel::Warn, "service", &format!("couldn't give the lock up: {error}"), None);
	}
}
