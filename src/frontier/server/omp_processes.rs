//! The omps of the thread panes Jocasta started (0490, 0547, 0549, 0675): remembered by pid and start time, ended with
//! what they left when their pane goes, told apart by the state their bridge pushes, and ended for `--restore` when
//! Jocasta stops everything.
//!
//! The omps being ended and the threads saved for `--restore` sit behind one lock that is never held across an `.await`
//! and never while a part is called.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::herdr::{AgentState, PaneInfo};
use crate::frontier::server::thread::{ThreadOmp, ThreadRef, settled, subject};
use crate::frontier::server::thread_lifecycle::StateSource;
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};
use crate::shared::proctree::{
	OmpProcesses, ProcessId, end_omp_leftovers, end_processes, merge_omp, omp_processes, process_id_of,
};

/// How long the end of a thread pane's omp is waited for before its leftovers end (0490).
const LEFTOVER_WAIT_MS: i64 = 10_000;
/// When leftovers are looked for again, for workers an exiting omp started (0490).
const LEFTOVER_AGAIN_MS: i64 = 3_000;

#[derive(Default)]
struct State {
	/// Old omps whose leftovers are being ended (0490): Stop everything ends them too until their last pass ran.
	ending: Vec<(u64, OmpProcesses)>,
	next_ending: u64,
	/// Threads Stop everything saves for `--restore` (0549): their omp ends without ending its turn first.
	paused: Vec<ThreadRef>,
}

pub struct OmpTracker {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	state: Mutex<State>,
}

impl OmpTracker {
	pub fn new(link: Link) -> Self {
		Self { link, state: Mutex::new(State::default()) }
	}

	fn state(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn leftover_wait_ms(c: &ServiceContext) -> i64 {
		c.options.leftover_wait_ms.unwrap_or(LEFTOVER_WAIT_MS)
	}

	fn leftover_again_ms(c: &ServiceContext) -> i64 {
		c.options.leftover_again_ms.unwrap_or(LEFTOVER_AGAIN_MS)
	}

	/// Remembers the omp of a thread pane Jocasta started (0490): the process herdr started for it and what runs under
	/// it, each by pid and start time, read for the pane's terminal `terminal`.
	pub async fn remember(&self, thread: &ThreadRef, terminal: Option<&str>) {
		let c = self.link.ctx();
		let pane_id = thread.with(|thread| thread.pane_id.clone());
		if !c.launched.has(&pane_id) {
			return;
		}
		let pid = match c.herdr.pane_root(&pane_id, None).await {
			Ok(pid) => pid,
			Err(error) => {
				c.warn(error);
				return;
			}
		};
		if thread.with(|thread| thread.pane_id != pane_id) {
			return;
		}
		let shim = pid.and_then(process_id_of);
		let processes = shim.and_then(|shim| omp_processes(&[shim]).into_iter().next());
		thread.with(|thread| thread.omp = Some(ThreadOmp { terminal: terminal.map(str::to_owned), shim, processes }));
	}

	/// Threads whose remembered omp is still only the process herdr started (the real omp, its child, not up yet when
	/// read): what runs under it now joins what is remembered, from one snapshot for all of them (0490).
	pub fn complete(&self, by_id: &HashMap<String, PaneInfo>) {
		let c = self.link.ctx();
		let waiting: Vec<(ThreadRef, ProcessId)> = c
			.threads
			.all()
			.into_iter()
			.filter_map(|thread| {
				let (pane_id, omp) = thread.with(|thread| (thread.pane_id.clone(), thread.omp.clone()));
				let pane = by_id.get(&pane_id)?;
				let omp = omp?;
				let shim = omp.shim?;
				(omp.terminal == pane.terminal_id && omp.processes.as_ref().map_or(0, |known| known.omp.len()) < 2)
					.then_some((thread, shim))
			})
			.collect();
		if waiting.is_empty() {
			return;
		}
		let shims: Vec<ProcessId> = waiting.iter().map(|(_, shim)| *shim).collect();
		for ((thread, _), read) in waiting.iter().zip(omp_processes(&shims)) {
			thread.with(|thread| {
				let Some(omp) = thread.omp.as_mut() else { return };
				let merged = match &omp.processes {
					Some(known) => merge_omp(known, &read),
					None => read.clone(),
				};
				omp.processes = Some(merged);
				// The shim ended without a child: nothing more to learn.
				if !read.omp.iter().any(|id| Some(id.pid) == omp.shim.map(|shim| shim.pid)) {
					omp.shim = None;
				}
			});
		}
	}

	/// Ends what an old omp left (0490): waits up to `leftover_wait_ms` for it to end, ends its leftovers, then looks
	/// again `leftover_again_ms` later for the workers it started on its way out. Resolves after the first pass.
	///
	/// The second pass runs on a task of its own that `stop` doesn't wait for, as the TypeScript's unawaited promise
	/// didn't; Stop everything ends what it still holds (`end_leftovers`).
	pub async fn end(&self, processes: OmpProcesses) {
		let c = self.link.ctx();
		let id = {
			let mut state = self.state();
			state.next_ending += 1;
			let id = state.next_ending;
			state.ending.push((id, processes.clone()));
			id
		};
		if let Err(error) = end_omp_leftovers(std::slice::from_ref(&processes), Self::leftover_wait_ms(&c)).await {
			c.warn(error);
		}
		let again = Self::leftover_again_ms(&c);
		tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(u64::try_from(again).unwrap_or(0))).await;
			if let Err(error) = end_omp_leftovers(&[processes], 0).await {
				c.warn(error);
			}
			c.omps.state().ending.retain(|(held, _)| *held != id);
		});
	}

	/// Stop everything and Keep's later stop, once herdr's server is gone (0486, 0490): every thread pane's remembered
	/// omp, and old ones still being ended, end with what they left, looked for twice as for a closed pane.
	pub async fn end_leftovers(&self) -> Result<()> {
		let c = self.link.ctx();
		let mut all: Vec<OmpProcesses> =
			c.threads.all().iter().filter_map(|thread| thread.with(|thread| thread.omp.clone()?.processes)).collect();
		all.extend(self.state().ending.iter().map(|(_, processes)| processes.clone()));
		if all.is_empty() {
			return Ok(());
		}
		end_omp_leftovers(&all, Self::leftover_wait_ms(&c)).await?;
		tokio::time::sleep(Duration::from_millis(u64::try_from(Self::leftover_again_ms(&c)).unwrap_or(0))).await;
		end_omp_leftovers(&all, 0).await?;
		Ok(())
	}

	/// Before Jocasta stops everything (0549): each thread at work, as the close question counts it (`at_work`), keeps
	/// its session in `panes.json`, so the next start brings its omp back with `--restore` and its turn goes on; the
	/// others reopen idle with `--continue`.
	pub async fn pause(&self) -> Result<()> {
		let c = self.link.ctx();
		let awaited = c.status.awaited_panes()?;
		for thread in c.threads.all() {
			let (pane_id, session) = thread.with(|thread| (thread.pane_id.clone(), thread.session_path.clone()));
			let at_work = c.status.at_work(&thread, &awaited);
			let Some(session) = session.filter(|_| at_work) else { continue };
			c.launched.pause(&pane_id, &session.display().to_string())?;
			let mut state = self.state();
			if !state.paused.iter().any(|held| held.same(&thread)) {
				state.paused.push(thread);
			}
		}
		Ok(())
	}

	/// Stop everything, once the service stopped and before herdr's server does (0549): the omp of each thread saved for
	/// `--restore` ends at once with what runs under it (found by start-checked parent links, SRV-A-1), forced, so it
	/// records no end of its turn. omp's own teardown on a signal or a closing pane aborts the turn and records the exit
	/// (omp 19.1.1 `interactive-mode.ts:2710-2741`, `agent-session.ts:4218-4268`), after which `--restore` finds
	/// nothing unfinished (`exit-diagnostics.ts:178-184`); a process that just ends leaves the turn open, and `--restore`
	/// goes on with it (`agent-session.ts:14056-14090`). The other panes' omps end with herdr as before.
	pub async fn cut_paused(&self) {
		let paused = self.state().paused.clone();
		for thread in paused {
			let (roots, name) = thread.with(|thread| {
				let omp = thread.omp.as_ref();
				let roots: Vec<ProcessId> = match omp.and_then(|omp| omp.processes.as_ref()) {
					Some(processes) => processes.tree.clone(),
					None => omp.and_then(|omp| omp.shim).into_iter().collect(),
				};
				(roots, subject(thread))
			});
			let found = if roots.is_empty() { Vec::new() } else { end_processes(&roots, &[]).await };
			if found.is_empty() {
				let said = format!("{name}'s omp isn't known; it ends with herdr, and its turn may not go on");
				log(LogLevel::Warn, "shutdown", &said, None);
				continue;
			}
			log(LogLevel::Info, "shutdown", &format!("{name}: its omp ended mid-turn, for --restore"), None);
		}
	}

	/// A thread pane's omp pushed its state over the bridge (0675): a dialog waiting, else a turn running, else idle. It
	/// is handled as herdr's pushed state is, with omp's own as the source (0547); the thread's `host` holds it until the
	/// omp's connection ends, when herdr and the session file decide as before. `state` is `None` when the omp is gone.
	pub fn host_reports(&self, pane_id: &str, state: Option<AgentState>) {
		let c = self.link.ctx();
		if !c.hosts || c.stopping() {
			return;
		}
		let Some(thread) = c.threads.get(pane_id) else { return };
		let (was, now_state, name) = thread.with(|thread| {
			let was = thread.host;
			thread.host = state;
			(was, thread.state, subject(thread))
		});
		let Some(state) = state else {
			if was.is_some() {
				log(LogLevel::Info, "threads", &format!("{name}: omp's bridge is gone; herdr decides"), None);
			}
			return;
		};
		if Some(state) != was {
			log(LogLevel::Info, "threads", &format!("{name}: omp reports {}", state.as_str()), None);
		}
		if state != settled(now_state) {
			c.lifecycle.state_changed(pane_id, state, StateSource::Host);
		}
	}
}

#[cfg(test)]
#[path = "omp_processes_tests.rs"]
mod tests;
