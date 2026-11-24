//! Panes that sleep (0676): a project's omp that has idled for `service.paneIdleMinutes`, or a thread's omp that has
//! reported and awaits his review, ends; its pane shows the sleeper (`jocasta pane-sleep`) and a key, a `thread_send`, a
//! review fix or a finish brings the same session back with `--restore`.
//!
//! The pane id changes with every respawn, so the sleeper's and the wake's registry updates run under one claim of the
//! pane (`busy`), which keeps the refresh tick from reading the gap as a crash (`omp_ended`) or a closed thread.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use futures_util::future::join_all;

use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::herdr::{HerdrSnapshot, PaneInfo};
use crate::frontier::server::journal::About;
use crate::frontier::server::pane_launch::Respawn;
use crate::frontier::server::thread::{Phase, ThreadRef, subject};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::pollers::machine::available_memory_bytes;
use crate::shared::proctree::{OmpProcesses, end_processes, omp_processes, process_id_of};
use crate::shared::settings::{read_check_reserve_gb, read_pane_idle_minutes};

/// How long the omp that was asked to shut down is waited for before its process tree is ended.
const SHUTDOWN_WAIT_MS: u64 = 10_000;
/// How long a second wake of a pane waits for the first one to finish.
const WAKE_WAIT_MS: u64 = 30_000;

/// A pane under way to sleep or wake: its ids are `busy` until this is dropped.
pub struct Claim<'a> {
	sleeps: &'a PaneSleeps,
	ids: Vec<String>,
}

impl Claim<'_> {
	/// The pane has a new id (every respawn gives one): it is claimed too.
	fn also(&mut self, pane_id: &str) {
		self.sleeps.busy_set().insert(pane_id.to_owned());
		self.ids.push(pane_id.to_owned());
	}
}

impl Drop for Claim<'_> {
	fn drop(&mut self) {
		let mut busy = self.sleeps.busy_set();
		for id in &self.ids {
			busy.remove(id);
		}
	}
}

/// A pane the sleep rule may put to sleep.
struct Candidate {
	pane_id: String,
	/// Since when its omp has been idle (epoch ms).
	idle_since: i64,
	/// A thread that has reported and awaits his review sleeps at once, not after the idle minutes.
	at_once: bool,
	focused: bool,
}

/// Puts panes to sleep and wakes them (0676); the panes under way are `busy`.
pub struct PaneSleeps {
	link: Link,
	busy: Mutex<HashSet<String>>,
}

impl PaneSleeps {
	/// The sleep rule's state for the context behind `link`.
	pub fn new(link: Link) -> Self {
		Self { link, busy: Mutex::new(HashSet::new()) }
	}

	fn busy_set(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
		self.busy.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Whether `pane_id` is being put to sleep or woken now.
	pub fn busy(&self, pane_id: &str) -> bool {
		let busy = self.busy_set();
		busy.contains(pane_id)
	}

	pub(crate) fn claim(&self, pane_id: &str) -> Option<Claim<'_>> {
		self.busy_set().insert(pane_id.to_owned()).then(|| Claim { sleeps: self, ids: vec![pane_id.to_owned()] })
	}

	/// What a pane's omp reports as idle enough to end: its turn ended with nothing running, no dialog of omp's open,
	/// and it said it can report running jobs (0684: without `jobs`, a pane never sleeps by itself). `Some(since)`.
	fn idle_since(&self, c: &ServiceContext, pane_id: &str) -> Option<i64> {
		let agent = c.bridge.agent(pane_id)?;
		let idle = agent.idle_reported && !agent.working && !agent.input_open && agent.jobs == Some(0);
		(idle && c.bridge.can(pane_id, "jobs")).then_some(agent.active_at)
	}

	/// The sleep rule, on the refresh tick (0676): project panes idle for `service.paneIdleMinutes` sleep; a thread
	/// that has reported sleeps at once; and while the machine's free memory is under `checks.memoryReserveGb` one more
	/// idle pane, the longest idle, sleeps per tick. The boss never sleeps, an omp at work is never ended, and neither is
	/// the pane he has focused (he may be reading it, or typing a prompt omp hasn't been sent): a thread that reported
	/// sleeps once he looks elsewhere.
	pub async fn tick(&self, snapshot: &HerdrSnapshot) -> Result<()> {
		let c = self.link.ctx();
		if c.stopping() {
			return Ok(());
		}
		// A question of Jocasta's waits for him (an Allow box; they carry no pane, so any one counts): nothing sleeps,
		// by the idle limit or by memory, until it is answered.
		if c.allow.open() > 0 {
			return Ok(());
		}
		let minutes = c.options.pane_idle_minutes.unwrap_or_else(read_pane_idle_minutes);
		let boss_id = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		let mut candidates = Vec::new();
		for pane in &snapshot.panes {
			if let Some(candidate) = self.candidate(&c, pane, boss_id.as_deref()).await? {
				candidates.push(candidate);
			}
		}
		let now = now_ms();
		let due: Vec<String> = candidates
			.iter()
			.filter(|candidate| {
				!candidate.focused
					&& minutes > 0.0
					&& (candidate.at_once || (now - candidate.idle_since) as f64 >= minutes * 60_000.0)
			})
			.map(|candidate| candidate.pane_id.clone())
			.collect();
		for outcome in join_all(due.iter().map(|pane_id| self.sleep(pane_id))).await {
			if let Err(error) = outcome {
				c.warn(format!("a pane didn't go to sleep: {error}"));
			}
		}
		if !self.short_of_memory(&c) {
			return Ok(());
		}
		let squeezed = candidates
			.iter()
			.filter(|candidate| !candidate.focused && !due.contains(&candidate.pane_id))
			.min_by_key(|candidate| candidate.idle_since);
		if let Some(candidate) = squeezed {
			self.sleep(&candidate.pane_id).await?;
		}
		Ok(())
	}

	/// Whether free memory is under the check queue's reserve (`checks.memoryReserveGb`, 0 = off).
	fn short_of_memory(&self, c: &ServiceContext) -> bool {
		let reserve = read_check_reserve_gb();
		if reserve <= 0.0 {
			return false;
		}
		let available = match &c.options.available_memory {
			Some(read) => read(),
			None => available_memory_bytes(),
		};
		available.is_some_and(|bytes| (bytes as f64) < reserve * 1024.0 * 1024.0 * 1024.0)
	}

	async fn candidate(&self, c: &ServiceContext, pane: &PaneInfo, boss_id: Option<&str>) -> Result<Option<Candidate>> {
		let pane_id = pane.pane_id.as_str();
		if Some(pane_id) == boss_id
			|| !c.launched.has(pane_id)
			|| c.launched.asleep(pane_id)
			|| self.busy(pane_id)
			|| c.finishing(&pane.workspace_id).is_some()
			|| c.lifecycle.busy(pane_id)
		{
			return Ok(None);
		}
		let Some(idle_since) = self.idle_since(c, pane_id) else { return Ok(None) };
		let at_once = match c.threads.get(pane_id) {
			Some(thread) => {
				let (flagged, phase, planning) = thread.with(|t| {
					(
						t.sleep_when_idle,
						t.phase,
						t.plan.as_ref().is_some_and(|p| p.awaiting.as_deref().is_some_and(|text| !text.is_empty())),
					)
				});
				// Its handoff is written (a thread in its Handoff phase counts once the session holds the entry).
				let handed = match phase {
					Phase::Task => true,
					Phase::Handoff => c.lifecycle.handed_off(&thread).await,
					Phase::Review | Phase::Finish => false,
				};
				if !flagged || planning || !handed {
					return Ok(None);
				}
				true
			}
			None => {
				let Some(cwd) = pane.cwd.as_deref().filter(|cwd| !cwd.as_os_str().is_empty()) else { return Ok(None) };
				if c.places.project_pane_at(cwd)?.is_none() {
					return Ok(None);
				}
				false
			}
		};
		Ok(Some(Candidate { pane_id: pane_id.to_owned(), idle_since, at_once, focused: pane.focused }))
	}

	/// Where `pane_id` runs (its folder), the words its sleeper shows, and its thread when it is one's.
	fn place_of(&self, c: &ServiceContext, pane_id: &str) -> Result<(PathBuf, String, Option<ThreadRef>)> {
		if let Some(thread) = c.threads.get(pane_id) {
			let (path, name) = thread.with(|t| (t.path.clone(), subject(t)));
			return Ok((path, format!("{name} · asleep"), Some(thread)));
		}
		let cwd = c
			.launched
			.cwd_of(pane_id)
			.map(PathBuf::from)
			.ok_or_else(|| Error::other(format!("{pane_id} isn't Jocasta's")))?;
		let name = c.places.project_pane_at(&cwd)?.map_or_else(
			|| cwd.file_name().map_or_else(|| "project".to_owned(), |name| name.to_string_lossy().into_owned()),
			|project| project.name,
		);
		Ok((cwd, format!("{name} · asleep"), None))
	}

	/// Puts the omp in `pane_id` to sleep: its session is kept, its omp asked to end (its process tree ended when it
	/// can't be asked, 0684, or didn't go within 10 s), what it left ended, and the pane starts the sleeper.
	pub async fn sleep(&self, pane_id: &str) -> Result<()> {
		let c = self.link.ctx();
		let Some(mut claim) = self.claim(pane_id) else { return Ok(()) };
		if c.launched.asleep(pane_id) {
			return Ok(());
		}
		let (cwd, text, thread) = self.place_of(&c, pane_id)?;
		// A thread's session is the one herdr's integration reported (it follows a handoff); a project's, the one its
		// omp said last.
		let session = thread
			.as_ref()
			.and_then(|thread| thread.with(|t| t.session_path.clone()))
			.map(|path| path.display().to_string())
			.or_else(|| c.bridge.session(pane_id))
			.or_else(|| c.launched.paused(pane_id));
		let processes = self.processes_of(&c, pane_id, thread.as_ref()).await;
		self.end_omp(&c, pane_id, processes).await;
		let new_pane = c.panes.respawn_asleep(pane_id, &cwd, &text, session.as_deref()).await?;
		claim.also(&new_pane);
		self.journal(&c, "slept", &cwd, thread.as_ref());
		if let Some(thread) = thread {
			c.threads.drop_pane(pane_id);
			thread.with(|t| {
				t.pane_id = new_pane.clone();
				t.state = crate::frontier::server::herdr::AgentState::Unknown;
				t.host = None;
				t.omp = None;
			});
			c.threads.add(thread);
		}
		Ok(())
	}

	/// What runs under the omp in `pane_id`: a thread's as it was remembered, else read from the pane's process now.
	async fn processes_of(
		&self,
		c: &ServiceContext,
		pane_id: &str,
		thread: Option<&ThreadRef>,
	) -> Option<OmpProcesses> {
		if let Some(known) =
			thread.and_then(|thread| thread.with(|t| t.omp.as_ref().and_then(|omp| omp.processes.clone())))
		{
			return Some(known);
		}
		let root = c.herdr.pane_root(pane_id, None).await.ok().flatten().and_then(process_id_of)?;
		omp_processes(&[root]).into_iter().next()
	}

	/// Asks the omp to end and waits for its pipe to close; one that can't be asked, or doesn't go, has its tree ended.
	/// Its leftovers (the memory workers it started) end after it (0490).
	async fn end_omp(&self, c: &ServiceContext, pane_id: &str, processes: Option<OmpProcesses>) {
		let asked = c.bridge.shutdown(pane_id).unwrap_or(false);
		let gone = asked && c.bridge.gone(pane_id, SHUTDOWN_WAIT_MS).await;
		if !gone && let Some(processes) = &processes {
			end_processes(&processes.tree, &[]).await;
		}
		if let Some(processes) = processes {
			c.omps.end(processes).await;
		}
	}

	/// Brings a sleeping thread's omp back with `--restore` (the session the pane kept), before anything is sent to it.
	/// A thread that is awake only stops being due for sleep. Returns once the new pane runs omp; what is sent to it
	/// waits in the bridge for its hello.
	pub async fn wake_thread(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		thread.with(|t| t.sleep_when_idle = false);
		let pane_id = thread.with(|t| t.pane_id.clone());
		if !c.launched.asleep(&pane_id) && !self.busy(&pane_id) {
			return Ok(());
		}
		let Some(_claim) = self.claim_waiting(&pane_id).await else { return Ok(()) };
		let pane_id = thread.with(|t| t.pane_id.clone());
		if !c.launched.asleep(&pane_id) {
			return Ok(());
		}
		c.lifecycle.relaunch(thread).await?;
		thread.with(|t| c.journal.thread("woke", t, serde_json::json!({})));
		Ok(())
	}

	/// Brings a sleeping project pane's omp back (`--restore <session>`, `--continue` when none was kept).
	pub async fn wake_project(&self, pane_id: &str) -> Result<()> {
		let c = self.link.ctx();
		let Some(_claim) = self.claim_waiting(pane_id).await else { return Ok(()) };
		if !c.launched.asleep(pane_id) {
			return Ok(());
		}
		let (cwd, _, _) = self.place_of(&c, pane_id)?;
		let project = c
			.places
			.project_pane_at(&cwd)?
			.ok_or_else(|| Error::user(format!("{} isn't a project folder", cwd.display())))?;
		let restore = c.launched.paused(pane_id).map(PathBuf::from);
		let how = Respawn { cwd: &cwd, project_id: &project.id, env: None, restore: restore.as_deref(), thread: None };
		c.panes.respawn(pane_id, how).await?;
		self.journal(&c, "woke", &cwd, None);
		Ok(())
	}

	/// The journal's `slept` or `woke` (0688) for the thread, or the project at `cwd`.
	fn journal(&self, c: &ServiceContext, kind: &str, cwd: &std::path::Path, thread: Option<&ThreadRef>) {
		if let Some(thread) = thread {
			thread.with(|t| c.journal.thread(kind, t, serde_json::json!({})));
		} else if let Ok(Some(project)) = c.places.project_pane_at(cwd) {
			c.journal.append(kind, About::project(&project.id), serde_json::json!({}));
		}
	}

	/// A key was pressed in a sleeper (its pane key, 0578): its omp starts again. A key for a pane that is awake, or
	/// not known, is nothing to do (the sleeper is gone with its pane).
	pub async fn wake_by_key(&self, key: &str) -> Result<String> {
		let c = self.link.ctx();
		let Some(pane_id) = c.launched.pane_of_key(Some(key)) else { return Ok("not asleep".to_owned()) };
		if !c.launched.asleep(&pane_id) {
			return Ok("not asleep".to_owned());
		}
		match c.threads.get(&pane_id) {
			Some(thread) => self.wake_thread(&thread).await?,
			None => self.wake_project(&pane_id).await?,
		}
		Ok("woke".to_owned())
	}

	/// A sleeper herdr restored as a plain shell (a restart): it starts the sleeper again, with the session it kept.
	pub async fn resleep(&self, pane_id: &str) -> Result<()> {
		let c = self.link.ctx();
		let Some(mut claim) = self.claim(pane_id) else { return Ok(()) };
		let (cwd, text, thread) = self.place_of(&c, pane_id)?;
		let session = c.launched.paused(pane_id);
		let new_pane = c.panes.respawn_asleep(pane_id, &cwd, &text, session.as_deref()).await?;
		claim.also(&new_pane);
		if let Some(thread) = thread {
			c.threads.drop_pane(pane_id);
			thread.with(|t| {
				t.pane_id = new_pane.clone();
				t.omp = None;
			});
			c.threads.add(thread);
		}
		Ok(())
	}

	/// The claim of `pane_id`, waiting for another wake or sleep of it to end first.
	async fn claim_waiting(&self, pane_id: &str) -> Option<Claim<'_>> {
		let mut waited = 0;
		loop {
			if let Some(claim) = self.claim(pane_id) {
				return Some(claim);
			}
			if waited >= WAKE_WAIT_MS {
				return None;
			}
			tokio::time::sleep(Duration::from_millis(50)).await;
			waited += 50;
		}
	}
}
