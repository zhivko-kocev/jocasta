//! The refresh (0402, 0403, 0404, 0405, 0521, CR-09): the boss pane, closed and new threads, panes to start omp in
//! again, missed states, plans, catch-up reviews, hand commits, spend, the sidebar and the HUD. herdr's facts are read
//! again as herdr reports a change (fork plan finding 8); everything on the tick. Each is its own step (SVC-3): one that
//! throws is said once per reason and the rest still run.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared as SharedFuture, join_all};

use crate::frontier::protocol::ThreadState;
use crate::frontier::server::boss::{DefaultClosed, close_herdr_default};
use crate::frontier::server::context::{Finishing, Link, ServiceContext, Shared};
use crate::frontier::server::herdr::{AgentState, HerdrSnapshot, PaneInfo};
use crate::frontier::server::pane_launch::{OmpBackoff, Respawn};
use crate::frontier::server::task_file::find_task;
use crate::frontier::server::thread::{Phase, ReviewResult, Thread, ThreadPlan, ThreadRef, ThreadReview, is_idle};
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::pathkey::same_dir;

/// Thread panes relaunched at once after herdr restored them (PERF-5): herdr drops connections under bursts (0517).
const RELAUNCH_POOL: usize = 3;

/// The refresh running now, as every caller of [`Refresher::refresh`] waits for it.
pub type Running = SharedFuture<BoxFuture<'static, ()>>;

/// What a refresh reads: herdr's facts (after herdr reported a change), or everything (the tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
	/// The boss pane, herdr's default workspace, closed and new threads, their panes and omps, missed states, the dock,
	/// the sidebar and the HUD.
	Facts,
	/// Those, then plans, catch-up reviews, hand commits and spend.
	Full,
}

/// The refresh under way: its number (so its end clears only itself), what it reads, and its wait.
struct Underway {
	number: u64,
	scope: Scope,
	running: Running,
}

/// What one tick works from: when herdr's list was read, the snapshot (read again after the tick changed the boss pane
/// or closed herdr's default workspace), its panes by id, and the threads known before the list was read (SVC-2).
struct Tick {
	read_at: i64,
	snapshot: HerdrSnapshot,
	by_id: HashMap<String, PaneInfo>,
	known: Vec<(String, ThreadRef)>,
}

/// Whether an optional text is set and not empty, as a string is truthy.
fn named(text: &Option<String>) -> bool {
	text.as_deref().is_some_and(|text| !text.is_empty())
}

/// The tick's state and the running tick; [`Refresher`] shares it with the tasks it spawns.
struct Inner {
	ctx: Arc<ServiceContext>,
	refreshing: Mutex<Option<Underway>>,
	/// The next refresh's number.
	numbers: AtomicU64,
	/// What each refresh step last warned of (SVC-3), so a failing step is said once per reason.
	step_warned: Mutex<HashMap<String, String>>,
	/// The panes `discover` last looked at, so an unchanged pane set isn't looked at again (CR-12).
	discovered_key: Mutex<String>,
	/// herdr's session was new at start (0495): its default workspace is looked at until it is closed or kept.
	herdr_default: AtomicBool,
	backoff: OmpBackoff,
}

pub struct Refresher {
	inner: Arc<Inner>,
}

impl Refresher {
	/// A refresher over `ctx`; built after the context, so it takes it (see "The service's shape").
	pub fn new(ctx: Arc<ServiceContext>) -> Self {
		let backoff = OmpBackoff::new(Link::to(&ctx));
		Self {
			inner: Arc::new(Inner {
				ctx,
				refreshing: Mutex::new(None),
				numbers: AtomicU64::new(0),
				step_warned: Mutex::new(HashMap::new()),
				discovered_key: Mutex::new(String::new()),
				herdr_default: AtomicBool::new(false),
				backoff,
			}),
		}
	}

	/// herdr's session was new at start (0495): its default workspace is looked at until it is closed or kept.
	pub fn set_herdr_default(&self, on: bool) {
		self.inner.herdr_default.store(on, Ordering::SeqCst);
	}

	/// Whether herdr's default workspace is still looked at.
	pub fn herdr_default(&self) -> bool {
		self.inner.herdr_default.load(Ordering::SeqCst)
	}

	/// The panes are looked at again for threads on the next tick (a catalog sync may add projects).
	pub fn rediscover(&self) {
		self.inner.rediscover();
	}

	/// Everything, on the tick: the boss pane is there and runs omp (0404), threads closed in herdr are dropped with an
	/// `[event]` (0405), new ones found, panes herdr restored as shells or whose omp ended while they stayed open (0521)
	/// run omp again, as do idle panes started before panes had keys (0578), a state herdr's events missed is caught up
	/// (CR-09), found threads' unreviewed commits get their review (0394), each thread's ledger is read (0582, and its
	/// spend past $50 said once, 0541), and the sidebar tokens and the HUD are reported (0402, 0403). The refresh reads
	/// herdr's snapshot (again after it changed the boss pane or closed herdr's default workspace) and asks herdr for
	/// the panes' roots, screens and prompts as the steps need them. One refresh at a time: a call meanwhile gets the
	/// running one, or, while one reads only herdr's facts, a full one that runs after it. The refresh runs whether or
	/// not the caller waits.
	pub fn refresh(&self) -> Running {
		self.run(Scope::Full)
	}

	/// herdr's facts only, after herdr reported a change: [`Refresher::refresh`] without plans, catch-up reviews, hand
	/// commits and spend. A call while any refresh runs gets that one.
	pub fn facts(&self) -> Running {
		self.run(Scope::Facts)
	}

	fn run(&self, scope: Scope) -> Running {
		let mut slot = self.inner.slot();
		let after = match slot.as_ref() {
			Some(underway) if underway.scope == Scope::Full || scope == Scope::Facts => {
				return underway.running.clone();
			}
			Some(underway) => Some(underway.running.clone()),
			None => None,
		};
		let number = self.inner.numbers.fetch_add(1, Ordering::SeqCst);
		let inner = Arc::clone(&self.inner);
		let work = tokio::spawn({
			let inner = Arc::clone(&inner);
			async move {
				if let Some(after) = after {
					after.await;
				}
				inner.tick(scope).await;
			}
		});
		let done = tokio::spawn(async move {
			let _ = work.await;
			let mut slot = inner.slot();
			if slot.as_ref().is_some_and(|underway| underway.number == number) {
				*slot = None;
			}
		});
		let running: Running = async move {
			let _ = done.await;
		}
		.boxed()
		.shared();
		*slot = Some(Underway { number, scope, running: running.clone() });
		running
	}

	/// The refresh still under way (the last one asked, which ends after any it follows), for `stop` to wait for.
	pub fn running(&self) -> Option<Running> {
		self.inner.slot().as_ref().map(|underway| underway.running.clone())
	}

	/// Threads herdr runs that the service doesn't know yet: a pane in a linked worktree of a watched project that
	/// Jocasta started omp in or that runs omp (CR-01), never a plain shell.
	pub async fn discover(&self, snapshot: &HerdrSnapshot) -> Result<()> {
		self.inner.discover(snapshot).await
	}
}

impl Inner {
	fn slot(&self) -> MutexGuard<'_, Option<Underway>> {
		self.refreshing.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn key(&self) -> MutexGuard<'_, String> {
		self.discovered_key.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn rediscover(&self) {
		self.key().clear();
	}

	/// One step of the tick (SVC-3): its result, or `None` once a failure was said (once per step and reason).
	async fn step<T, E: Display>(&self, name: &str, run: impl Future<Output = std::result::Result<T, E>>) -> Option<T> {
		let outcome = run.await;
		let mut warned = self.step_warned.lock().unwrap_or_else(PoisonError::into_inner);
		match outcome {
			Ok(result) => {
				warned.remove(name);
				Some(result)
			}
			Err(error) => {
				let message = error.to_string();
				if warned.get(name) != Some(&message) {
					warned.insert(name.to_owned(), message.clone());
					drop(warned);
					self.ctx.warn(format!("refresh, {name}: {message}"));
				}
				None
			}
		}
	}

	/// The snapshot a tick works from, or `None` when this tick ends (herdr didn't answer).
	async fn read(&self) -> Option<HerdrSnapshot> {
		let c = &self.ctx;
		let read = self.step("reading herdr's panes", c.herdr.snapshot()).await?;
		if let Err(error) = c.launched.seen(&read.panes) {
			self.step::<(), _>("reading herdr's panes", async { Err(error) }).await;
			return None;
		}
		let mut snapshot = self.step("the boss pane", self.boss_pane(&read)).await.unwrap_or(read);
		// A new session's default workspace closes while untouched, once Jocasta's exists (0495).
		if self.herdr_default.load(Ordering::SeqCst) {
			let before = snapshot.clone();
			snapshot =
				self.step("herdr's default workspace", self.herdr_default_closed(&before)).await.unwrap_or(before);
		}
		Some(snapshot)
	}

	async fn tick(&self, scope: Scope) {
		let c = &self.ctx;
		if c.stopping() {
			return;
		}
		let read_at = now_ms();
		c.places.expire(read_at);
		// The threads known before herdr's list is read: one registered while the tick runs isn't in that list (SVC-2).
		let known = c.threads.entries();
		let Some(snapshot) = self.read().await else { return };
		let by_id = snapshot.panes.iter().map(|p| (p.pane_id.clone(), p.clone())).collect();
		let tick = Tick { read_at, snapshot, by_id, known };
		self.steps(&tick, scope).await;
	}

	/// What follows the snapshot, each step on its own; plans, catch-ups, hand commits and spend on a full refresh only.
	async fn steps(&self, tick: &Tick, scope: Scope) {
		let c = &self.ctx;
		let Tick { read_at, snapshot, by_id, .. } = tick;
		c.status.set_workspaces(snapshot.workspaces.clone());
		self.step("the focused pane", async { c.starts.note_focus(&snapshot.panes) }).await;
		for (workspace_id, step) in c.finishing_ids() {
			if step == Finishing::Removed && !snapshot.workspaces.iter().any(|w| w.id == workspace_id) {
				c.clear_finishing(&workspace_id);
			}
		}
		if let Some(boss) = c.boss() {
			let boss_id = boss.with(|boss| boss.pane_id.clone());
			if let Some(session) = by_id.get(&boss_id).and_then(|pane| pane.session_path.as_deref()) {
				let moved = c.sessions.lock().await.moved(session);
				boss.with(|boss| boss.session_path = Some(moved.to_string_lossy().into_owned()));
			}
		}
		self.step("closed threads", self.closed_threads(tick)).await;
		self.step("finding threads", self.discover(snapshot)).await;
		self.step("thread panes", self.thread_panes(tick)).await;
		self.step("the thread panes' omp", self.thread_omps(tick)).await;
		self.step("project panes", self.project_panes(tick)).await;
		self.step("pane sleep", c.sleep.tick(snapshot)).await;
		// The dock's workspace and the Agents view (0553), healed like the boss pane (0518, 0549).
		if let Some(dock) = c.options.dock.clone() {
			let boss_id = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
			self.step("the dock", dock(snapshot.clone(), boss_id)).await;
		}
		self.step("missed states", c.lifecycle.reconcile(by_id, *read_at)).await;
		if scope == Scope::Full {
			// A plan omp proposed that no idle event saw (B1), or one an omp resumed after a restart still holds (B3,
			// counted unsaid, as `discover` does); before the catch-up, so no review prompt goes into it.
			self.step("plans", self.plans()).await;
			self.step("reopened plans", self.reopened_plans()).await;
			self.step("catch-up reviews", self.catch_ups()).await;
			self.step("approved plans", self.approved_plans()).await;
			self.step("hand commits", c.lifecycle.hand_commits(by_id)).await;
			self.step("spend", async {
				c.status.spend_passed().await;
				Ok::<_, Error>(())
			})
			.await;
		}
		self.step("sidebar and HUD", c.status.report()).await;
	}

	/// The boss pane, ensured again when it is gone (after 0521's doubling wait when it keeps closing), its omp ended
	/// (0521) or it runs omp without this run's key (0484).
	async fn boss_pane(&self, read: &HerdrSnapshot) -> Result<HerdrSnapshot> {
		let c = &self.ctx;
		let boss_id = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		let current = read.panes.iter().find(|p| Some(&p.pane_id) == boss_id.as_ref());
		// An omp that ended in the boss pane starts again there, as at the start (0521).
		let boss_ended = current.is_some_and(|pane| self.backoff.omp_ended(pane));
		if let Some(pane) = current
			&& !boss_ended
			&& !c.launched.restored(pane)
			&& !c.panes.unkeyed_boss(pane)
		{
			return Ok(read.clone());
		}
		if current.is_none() && boss_id.is_some() && !self.backoff.boss_closed() {
			return Ok(read.clone());
		}
		c.panes.ensure_boss(boss_ended).await?;
		let again = c.herdr.snapshot().await?;
		c.launched.seen(&again.panes)?;
		Ok(again)
	}

	/// herdr's default workspace in a new session (0495): closed while untouched, once Jocasta's exists.
	async fn herdr_default_closed(&self, before: &HerdrSnapshot) -> Result<HerdrSnapshot> {
		let outcome = close_herdr_default(self.ctx.herdr.as_ref(), before, None).await?;
		if outcome != DefaultClosed::Wait {
			self.herdr_default.store(false, Ordering::SeqCst);
		}
		if outcome == DefaultClosed::Closed { Ok(self.ctx.herdr.snapshot().await?) } else { Ok(before.clone()) }
	}

	/// Threads whose pane went in herdr are dropped and said closed (0405); the others follow their session's moves.
	async fn closed_threads(&self, tick: &Tick) -> Result<()> {
		let c = &self.ctx;
		for (pane_id, thread) in &tick.known {
			// Finished, relaunched or dropped meanwhile: not this list's to judge.
			if !c.threads.get(pane_id).is_some_and(|current| current.same(thread)) {
				continue;
			}
			// Being put to sleep or woken: its pane id is about to change, which is no closed pane (0676).
			if c.sleep.busy(pane_id) {
				continue;
			}
			match tick.by_id.get(pane_id) {
				None => {
					c.threads.drop_pane(pane_id);
					let workspace_id = thread.with(|t| t.workspace_id.clone());
					if c.finishing(&workspace_id).is_none() {
						c.lifecycle.closed(thread).await?;
					}
				}
				Some(pane) => {
					if let Some(session) = pane.session_path.as_deref() {
						let moved = c.sessions.lock().await.moved(session);
						thread.with(|t| t.session_path = Some(moved));
					}
				}
			}
		}
		Ok(())
	}

	async fn discover(&self, snapshot: &HerdrSnapshot) -> Result<()> {
		let c = &self.ctx;
		if c.rediscover.swap(false, Ordering::SeqCst) {
			self.rediscover();
		}
		let panes = &snapshot.panes;
		let key = panes
			.iter()
			.map(|p| {
				let cwd = p.cwd.as_deref().map(|cwd| cwd.to_string_lossy().into_owned()).unwrap_or_default();
				format!("{}|{}|{cwd}", p.pane_id, p.agent.as_deref().unwrap_or(""))
			})
			.collect::<Vec<_>>()
			.join("\n");
		if key == *self.key() {
			return Ok(());
		}
		let mut taken: HashSet<String> = c.threads.all().iter().map(|t| t.with(|t| t.workspace_id.clone())).collect();
		let boss_id = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		for pane in panes {
			let Some(cwd) = pane.cwd.as_deref().filter(|cwd| !cwd.as_os_str().is_empty()) else { continue };
			if Some(&pane.pane_id) == boss_id.as_ref() || c.threads.has(&pane.pane_id) {
				continue;
			}
			if taken.contains(&pane.workspace_id) || c.finishing(&pane.workspace_id).is_some() {
				continue;
			}
			if pane.agent.as_deref() != Some("omp") && !c.launched.has(&pane.pane_id) {
				continue;
			}
			let Some(checkout) = c.places.checkout(cwd)? else { continue };
			if same_dir(&checkout.top, &checkout.main) {
				continue;
			}
			let Some(project) = c.places.project_at(cwd)? else { continue };
			if !same_dir(&project.main_path, &checkout.main) {
				continue;
			}
			taken.insert(pane.workspace_id.clone());
			self.found(snapshot, pane, project, &checkout).await?;
		}
		// Only once every pane was looked at: a throw part-way looks again on the next tick (SVC-11).
		*self.key() = key;
		Ok(())
	}

	/// A thread found in `pane`: read from its session (as at the start), its task file and HEAD, then registered.
	async fn found(
		&self,
		snapshot: &HerdrSnapshot,
		pane: &PaneInfo,
		project: crate::shared::projects::Project,
		checkout: &crate::frontier::server::places::Checkout,
	) -> Result<()> {
		let c = &self.ctx;
		let label = snapshot.workspaces.iter().rfind(|w| w.id == pane.workspace_id).map(|w| w.label.clone());
		// A pane found idle with its plan in omp's Plan Review still waits on him. Read in steps, as at the start.
		let at_rest = is_idle(pane.agent_state);
		let session_path = match pane.session_path.as_deref() {
			Some(session) => Some(c.sessions.lock().await.moved(session)),
			None => None,
		};
		let mut stats = None;
		if let (true, Some(path)) = (at_rest, session_path.as_deref()) {
			c.sessions.lock().await.warm(std::slice::from_ref(&path.to_path_buf()), || c.stopping()).await;
			stats = Some(c.sessions.lock().await.read(path, false).clone());
		}
		let plan = stats.as_ref().and_then(|stats| stats.plan_awaiting.clone()).filter(|plan| !plan.is_empty());
		let task = find_task(&project.id, &checkout.top);
		let head = git_out(&checkout.top, &["rev-parse", "HEAD"]).await?;
		// Found meanwhile (a start, the next tick): that one stays.
		if c.threads.has(&pane.pane_id) {
			return Ok(());
		}
		// Done and reviewed before a restart (its task file says so): its last review still awaits him (0432, 0549).
		let result =
			task.as_ref().filter(|task| task.state == "done and reviewed").and_then(|task| task.reviews.last()).map(
				|last| ReviewResult {
					notes: last.notes,
					blocking: last.blocking,
					tests: last.tests.unwrap_or(crate::frontier::server::sessions::TestsResult::Unknown),
					proof: last.proof.clone(),
					checks: last.checks.clone(),
					at: Some(last.at),
				},
			);
		let title = label
			.or_else(|| checkout.branch.clone())
			.unwrap_or_else(|| checkout.top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
		let mut thread =
			Thread::new(&pane.pane_id, &pane.workspace_id, project, title, checkout.branch.clone().unwrap_or_default());
		thread.path = checkout.top.clone();
		thread.state = pane.agent_state;
		thread.shown = if pane.agent_state == AgentState::Working {
			ThreadState::Working
		} else if pane.agent_state == AgentState::Blocked || plan.is_some() {
			ThreadState::Blocked
		} else if result.is_some() {
			ThreadState::Done
		} else {
			ThreadState::Idle
		};
		thread.head = head;
		thread.phase = Phase::Task;
		thread.catch_up = true;
		thread.plan = Some(ThreadPlan {
			reopened: true,
			awaiting: plan,
			sent_back: stats.as_ref().map(|stats| stats.plan_sent_back as usize),
			..ThreadPlan::default()
		});
		if result.is_some() {
			thread.review = Some(ThreadReview { running: None, result });
		}
		thread.session_path = session_path;
		thread.task = task;
		c.threads.add(Shared::new(thread));
		Ok(())
	}

	/// A pane Jocasta started before panes had keys (0578), idle with omp in it: started again with its key, resuming
	/// its session, so its thread tools answer it. Never mid-turn, mid-step or with a plan or question waiting.
	fn unkeyed(&self, pane: &PaneInfo, thread: Option<&ThreadRef>) -> bool {
		let c = &self.ctx;
		if !c.launched.has(&pane.pane_id) || c.launched.key_of(&pane.pane_id).is_some() {
			return false;
		}
		if pane.agent.as_deref() != Some("omp") || !is_idle(pane.agent_state) || c.lifecycle.busy(&pane.pane_id) {
			return false;
		}
		thread.is_none_or(|thread| {
			thread.with(|t| {
				t.phase == Phase::Task && !t.plan.as_ref().is_some_and(|p| named(&p.awaiting)) && is_idle(t.state)
			})
		})
	}

	/// A thread pane herdr restored as a shell, one whose omp ended while it stayed open (0521), or an idle one started
	/// before panes had keys (0578): a few at once (PERF-5), each on its own (SVC-3).
	async fn thread_panes(&self, tick: &Tick) -> Result<()> {
		let c = &self.ctx;
		let mut resleep: Vec<(String, PathBuf)> = Vec::new();
		let due: Vec<(ThreadRef, PathBuf)> = c
			.threads
			.all()
			.into_iter()
			.filter_map(|thread| {
				let (pane_id, path) = thread.with(|t| (t.pane_id.clone(), t.path.clone()));
				let pane = tick.by_id.get(&pane_id)?;
				if !self.backoff.due(Some(&path)) || c.sleep.busy(&pane_id) {
					return None;
				}
				// A sleeping thread stays asleep (0676): herdr restoring its pane as a shell starts the sleeper again.
				if c.launched.asleep(&pane_id) {
					if c.launched.restored(pane) {
						resleep.push((pane_id, path));
					}
					return None;
				}
				let relaunch =
					c.launched.restored(pane) || self.backoff.omp_ended(pane) || self.unkeyed(pane, Some(&thread));
				relaunch.then_some((thread, path))
			})
			.collect();
		for (pane_id, path) in &resleep {
			self.backoff.respawning(path, c.sleep.resleep(pane_id)).await;
		}
		for chunk in due.chunks(RELAUNCH_POOL) {
			join_all(chunk.iter().map(|(thread, path)| self.backoff.respawning(path, c.lifecycle.relaunch(thread))))
				.await;
		}
		Ok(())
	}

	/// Each thread pane's omp, read once per terminal (0490): a new pane, or one herdr started a process in again.
	async fn thread_omps(&self, tick: &Tick) -> Result<()> {
		let c = &self.ctx;
		let wanted: Vec<(ThreadRef, Option<String>)> = c
			.threads
			.all()
			.into_iter()
			.filter_map(|thread| {
				let (pane_id, known) =
					thread.with(|t| (t.pane_id.clone(), t.omp.as_ref().map(|omp| omp.terminal.clone())));
				let pane = tick.by_id.get(&pane_id)?;
				if known.is_some_and(|terminal| terminal == pane.terminal_id) {
					return None;
				}
				Some((thread, pane.terminal_id.clone()))
			})
			.collect();
		join_all(wanted.iter().map(|(thread, terminal)| c.omps.remember(thread, terminal.as_deref()))).await;
		c.omps.complete(&tick.by_id);
		Ok(())
	}

	/// A project pane (the project's own omp in its main checkout) herdr restored as a shell, one whose omp ended while
	/// it stayed open (0521), or an idle one started before panes had keys (0578); never a finishing thread's.
	async fn project_panes(&self, tick: &Tick) -> Result<()> {
		let c = &self.ctx;
		let labels: HashMap<&str, &str> =
			tick.snapshot.workspaces.iter().map(|w| (w.id.as_str(), w.label.as_str())).collect();
		let boss_id = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		for pane in &tick.snapshot.panes {
			let Some(cwd) = pane.cwd.as_deref().filter(|cwd| !cwd.as_os_str().is_empty()) else { continue };
			if Some(&pane.pane_id) == boss_id.as_ref() || c.threads.has(&pane.pane_id) {
				continue;
			}
			if c.finishing(&pane.workspace_id).is_some() || !self.backoff.due(Some(cwd)) {
				continue;
			}
			let unkeyed = self.unkeyed(pane, None);
			if !c.launched.restored(pane) && !c.launched.ended(pane) && !unkeyed {
				continue;
			}
			let Some(project) = c.places.project_at(cwd)? else { continue };
			if !same_dir(&project.main_path, cwd) {
				continue;
			}
			if labels.get(pane.workspace_id.as_str()) != Some(&project.name.as_str()) {
				continue;
			}
			if c.launched.restored(pane) && c.launched.asleep(&pane.pane_id) {
				// A sleeper herdr restored as a shell is the sleeper again (0676), not an omp.
				self.backoff.respawning(cwd, c.sleep.resleep(&pane.pane_id)).await;
				continue;
			}
			if c.launched.restored(pane) || unkeyed || self.backoff.omp_ended(pane) {
				let how = Respawn { cwd, project_id: &project.id, env: None, restore: None, thread: None };
				self.backoff.respawning(cwd, c.panes.respawn(&pane.pane_id, how)).await;
			}
		}
		Ok(())
	}

	/// Plans awaiting him in idle threads that no idle event saw (B1, B3).
	async fn plans(&self) -> Result<()> {
		let c = &self.ctx;
		for thread in c.threads.all() {
			let (awaiting, path, idle, catch_up) = thread.with(|t| {
				(
					t.plan.as_ref().is_some_and(|p| named(&p.awaiting)),
					t.session_path.clone(),
					is_idle(t.state),
					t.catch_up,
				)
			});
			let Some(path) = path.filter(|_| !awaiting && idle) else { continue };
			let plan = c.sessions.lock().await.read(&path, false).plan_awaiting.clone().filter(|plan| !plan.is_empty());
			if let Some(plan) = plan {
				c.lifecycle.plan_found(&thread, &plan, !catch_up).await?;
			}
		}
		Ok(())
	}

	/// A plan awaiting him in a reopened omp gets omp's Plan Review back (0494); a reopened omp idle with no plan
	/// awaiting in its session, read as the file is now, has none to show.
	async fn reopened_plans(&self) -> Result<()> {
		let c = &self.ctx;
		for thread in c.threads.all() {
			let (reopened, awaiting, idle, path) = thread.with(|t| {
				let plan = t.plan.as_ref();
				(
					plan.is_some_and(|p| p.reopened),
					plan.is_some_and(|p| named(&p.awaiting)),
					is_idle(t.state),
					t.session_path.clone(),
				)
			});
			if !reopened {
				continue;
			}
			if awaiting {
				c.lifecycle.reopen_plan_review(&thread).await?;
			} else if let Some(path) = path.filter(|_| idle) {
				let shown = c.sessions.lock().await.read(&path, true).plan_awaiting.clone();
				if !named(&shown) {
					thread.with(|t| {
						if let Some(plan) = &mut t.plan {
							plan.reopened = false;
						}
					});
				}
			}
		}
		Ok(())
	}

	/// Found threads, idle now, get the review their unreviewed commits never had (0394).
	async fn catch_ups(&self) -> Result<()> {
		for thread in self.ctx.threads.all() {
			let due = thread.with(|t| {
				let waiting = t.plan.as_ref().is_some_and(|p| named(&p.awaiting));
				let due = t.catch_up && t.phase == Phase::Task && !waiting && is_idle(t.state);
				if due {
					t.catch_up = false;
				}
				due
			});
			if due {
				self.ctx.review.catch_up(&thread).await?;
			}
		}
		Ok(())
	}

	/// A plan approved since the last state change (a new session's turn may bring none): the task file says so.
	async fn approved_plans(&self) -> Result<()> {
		let c = &self.ctx;
		for thread in c.threads.all() {
			let (proposed, awaiting, path, before) = thread.with(|t| {
				let plan = t.plan.as_ref();
				(
					plan.is_some_and(|p| p.proposed.is_some()),
					plan.is_some_and(|p| named(&p.awaiting)),
					t.session_path.clone(),
					t.task.as_ref().and_then(|task| task.plan.clone()),
				)
			});
			let Some(path) = path.filter(|_| proposed && !awaiting) else { continue };
			let stats = c.sessions.lock().await.read(&path, false).clone();
			c.lifecycle.note_approval(&thread, Some(&stats));
			if thread.with(|t| t.task.as_ref().and_then(|task| task.plan.clone())) != before {
				c.status.write_task(&thread);
			}
		}
		Ok(())
	}
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
