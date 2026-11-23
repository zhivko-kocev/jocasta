//! What the service's parts share (SVC-15): the catalog, the options, herdr, the day, the session index and each part,
//! built once by `JocastaService`. A part reaches another only through this, so the parts import each other's types
//! alone and the wiring stays in one place (`service.rs`).
//!
//! The rules the Rust shape follows are in "The service's shape" in `docs/research/rust-port.md`. In short: the context
//! is an `Arc<ServiceContext>`; a part holds a [`Link`] to it and reaches a sibling as `self.link.ctx().<part>`; what the
//! TypeScript mutates sits in an atomic or in a [`Shared`] (a lock reachable only through a closure, so no guard can
//! cross an `.await`).

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Duration;

use crate::frontier::server::allow::AllowGate;
use crate::frontier::server::boss::LaunchedPanes;
use crate::frontier::server::checks::{Admission, CheckRunner, CheckRunnerOptions, checks_dir};
use crate::frontier::server::day::{Day, SharedCatalog};
use crate::frontier::server::events::{BossPane, Events};
use crate::frontier::server::finish::Finisher;
use crate::frontier::server::herdr::Herdr;
use crate::frontier::server::omp_processes::OmpTracker;
use crate::frontier::server::pane_launch::PaneLauncher;
use crate::frontier::server::places::{Places, PlacesHooks};
use crate::frontier::server::review_flow::ReviewFlow;
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::sessions::SessionIndex;
use crate::frontier::server::status::StatusViews;
use crate::frontier::server::thread::ThreadRegistry;
use crate::frontier::server::thread_facts::ThreadFacts;
use crate::frontier::server::thread_lifecycle::ThreadLifecycle;
use crate::frontier::server::thread_start::ThreadStarts;
use crate::frontier::server::weekly_review::WeeklyReviews;
use crate::frontier::server::worktrees::{ReviewedCommits, ThreadNumbers};
use crate::shared::layer::{hosts_on, layer_on};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;

/// A value several parts and tasks share and change. The lock is reachable only through [`Shared::with`], whose closure
/// is synchronous, so a guard can't be held across an `.await` (the service's rule, SVC-15). Clones share the value;
/// [`Shared::same`] compares identity, as the TypeScript's `===` on an object does.
pub struct Shared<T>(Arc<Mutex<T>>);

impl<T> Clone for Shared<T> {
	fn clone(&self) -> Self {
		Self(Arc::clone(&self.0))
	}
}

impl<T> Shared<T> {
	pub fn new(value: T) -> Self {
		Self(Arc::new(Mutex::new(value)))
	}

	/// Runs `f` on the value with the lock held. `f` must not call into another part (it would hold this lock while it
	/// runs): read or change the value and return.
	pub fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
		f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
	}

	/// A copy of the value as it is now.
	pub fn get(&self) -> T
	where
		T: Clone,
	{
		self.with(|value| value.clone())
	}

	/// Whether `other` is this very value, not an equal one.
	pub fn same(&self, other: &Self) -> bool {
		Arc::ptr_eq(&self.0, &other.0)
	}
}

/// The boss pane, shared by the parts that read or change its state (`Events` above all).
pub type BossRef = Shared<BossPane>;

/// Work in flight that `stop` waits for: tool, CLI and key calls (SVC-13), background git reads, and `Events`'
/// notifications and `[event]` lines sent without waiting (TypeScript `calls: Set<Promise>` and `inFlight`). Work is
/// counted from the moment it starts until it ends, finished or dropped (an aborted task counts down too).
pub struct Calls {
	running: tokio::sync::watch::Sender<usize>,
}

/// One unit of work in flight in [`Calls`]; ends when dropped.
pub struct InFlight(tokio::sync::watch::Sender<usize>);

impl Drop for InFlight {
	fn drop(&mut self) {
		self.0.send_modify(|running| *running -= 1);
	}
}

impl Default for Calls {
	fn default() -> Self {
		Self { running: tokio::sync::watch::channel(0).0 }
	}
}

impl Calls {
	/// Counts one unit of work until the returned guard drops.
	pub fn enter(&self) -> InFlight {
		self.running.send_modify(|running| *running += 1);
		InFlight(self.running.clone())
	}

	/// Awaits `work`, counted while it runs.
	pub async fn run<F: Future>(&self, work: F) -> F::Output {
		let _in_flight = self.enter();
		work.await
	}

	/// Runs `work` on its own task, counted until it ends. Needs a tokio runtime.
	pub fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
		let in_flight = self.enter();
		tokio::spawn(async move {
			work.await;
			drop(in_flight);
		});
	}

	/// How many are in flight now.
	pub fn len(&self) -> usize {
		*self.running.borrow()
	}

	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Resolves once none is in flight, including work that started while it waited.
	pub async fn settled(&self) {
		let mut running = self.running.subscribe();
		// The sender lives in `self`, so the wait can only end because the count reached 0.
		let _ = running.wait_for(|running| *running == 0).await;
	}

	/// Whether none is in flight within `ms` milliseconds (`stop`'s bounded wait, SVC-13).
	pub async fn settled_within(&self, ms: u64) -> bool {
		tokio::time::timeout(Duration::from_millis(ms), self.settled()).await.is_ok()
	}
}

/// How far `thread_finish` has got with a thread's workspace (CR-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finishing {
	Removing,
	Removed,
}

/// A part's way back to the context. The context owns the parts and a part is reachable only through it, so a `Link`
/// can always be followed while a method of the part runs; it is weak so that the context and its parts don't keep each
/// other alive. Clones share the target.
#[derive(Clone, Default)]
pub struct Link(Arc<OnceLock<Weak<ServiceContext>>>);

impl Link {
	/// A link to a context not built yet: `ServiceContext::new` points it at the context. Parts, and the hooks of things
	/// the context holds (the day), take a clone while the context is being built.
	pub fn new() -> Self {
		Self::default()
	}

	/// A link to an already built context, for what is built after it (`Refresher`, `Tools`, …) and needs a part's own
	/// `new(link)`, as `OmpBackoff` does.
	pub fn to(ctx: &Arc<ServiceContext>) -> Self {
		let link = Self::default();
		let _ = link.0.set(Arc::downgrade(ctx));
		link
	}

	/// The context. Parts run only once the context is built, and only through it, which keeps it alive.
	pub fn ctx(&self) -> Arc<ServiceContext> {
		self.0.get().and_then(Weak::upgrade).expect("a part runs only inside its built context")
	}
}

/// What the parts share: see the module's note. The fields are the TypeScript's `ServiceContext`, one for one.
pub struct ServiceContext {
	pub catalog: SharedCatalog,
	pub options: ServiceOptions,
	pub herdr: Arc<dyn Herdr>,
	/// The session index. Its methods take `&mut self` and `warm` and `prepare` run in steps over awaits, so it is behind
	/// a tokio lock: an async caller takes it with `lock().await` and never calls a part while holding it; a synchronous
	/// one (a `DayHooks` method) uses `try_lock` and treats a busy index as "nothing new yet".
	pub sessions: tokio::sync::Mutex<SessionIndex>,
	pub checks: Arc<CheckRunner>,
	/// The panes Jocasta started omp in (`boss.ts` `LaunchedPanes`).
	pub launched: LaunchedPanes,
	pub threads: ThreadRegistry,
	/// The layer's test switch (0576): off, thread panes get no layer parts, no report steer, no check on idle.
	pub layer: bool,
	/// H2's `herdr` condition (0576): off, omp's host registry is never read; herdr's events and the tick decide.
	pub hosts: bool,
	/// Tool, CLI and key calls under way (SVC-13), and background git reads: `stop` waits for them.
	pub calls: Calls,
	/// Set when a catalog sync ran (`Places` hook): the refresh tick clears it and looks at the panes again (CR-12).
	pub rediscover: AtomicBool,
	finishing: Mutex<HashMap<String, Finishing>>,
	stopping: AtomicBool,
	warmed: AtomicBool,
	boss: Mutex<Option<BossRef>>,
	pub day: Day,
	pub places: Places,
	pub events: Events,
	pub allow: AllowGate,
	pub panes: PaneLauncher,
	pub omps: OmpTracker,
	pub numbers: ThreadNumbers,
	pub reviewed: ReviewedCommits,
	pub review: ReviewFlow,
	pub finisher: Finisher,
	pub lifecycle: ThreadLifecycle,
	pub status: StatusViews,
	pub starts: ThreadStarts,
	pub weekly: WeeklyReviews,
	/// Each open thread's diff, last check and last write, read on the refresh tick (0668).
	pub thread_facts: ThreadFacts,
}

impl ServiceContext {
	/// Builds the context and every part, then points `link` at it. `day` was built with hooks that hold a clone of `link`
	/// (it can't be built after the context: the context holds it). The options' `day` is already taken out of `options`.
	pub fn new(link: &Link, catalog: SharedCatalog, options: ServiceOptions, day: Day) -> Arc<Self> {
		let checks = options.checks.clone().unwrap_or_else(|| {
			Arc::new(CheckRunner::new(CheckRunnerOptions {
				dir: checks_dir(&data_dir()),
				admission: Some(Admission::from_settings()),
				..Default::default()
			}))
		});
		let places_link = link.clone();
		let synced_link = link.clone();
		let places = Places::new(
			catalog.clone(),
			PlacesHooks {
				problem: Box::new(move |text| places_link.ctx().events.warning(text)),
				synced: Box::new(move || synced_link.ctx().rediscover.store(true, Ordering::SeqCst)),
			},
		);
		let ctx = Arc::new(Self {
			herdr: options.herdr.clone(),
			sessions: tokio::sync::Mutex::new(SessionIndex::new(options.sessions_dir.clone(), None)),
			checks,
			launched: LaunchedPanes::new(None),
			threads: ThreadRegistry::new(),
			layer: layer_on(None),
			hosts: hosts_on(None),
			calls: Calls::default(),
			rediscover: AtomicBool::new(false),
			finishing: Mutex::new(HashMap::new()),
			stopping: AtomicBool::new(false),
			warmed: AtomicBool::new(false),
			boss: Mutex::new(None),
			events: Events::new(link.clone(), options.broadcast.clone()),
			allow: AllowGate::new(options.confirm.clone(), options.confirm_wait_ms),
			places,
			numbers: ThreadNumbers::new(None),
			reviewed: ReviewedCommits::new(None),
			panes: PaneLauncher::new(link.clone()),
			omps: OmpTracker::new(link.clone()),
			status: StatusViews::new(link.clone()),
			review: ReviewFlow::new(link.clone()),
			finisher: Finisher::new(link.clone()),
			lifecycle: ThreadLifecycle::new(link.clone()),
			starts: ThreadStarts::new(link.clone()),
			weekly: WeeklyReviews::new(link.clone()),
			thread_facts: ThreadFacts::new(),
			catalog,
			options,
			day,
		});
		// A context is built once per link; a second `set` would mean two contexts shared one set of hooks.
		let _ = link.0.set(Arc::downgrade(&ctx));
		ctx
	}

	/// The boss pane, once ensured.
	pub fn boss(&self) -> Option<BossRef> {
		self.boss.lock().unwrap_or_else(PoisonError::into_inner).clone()
	}

	pub fn set_boss(&self, boss: Option<BossRef>) {
		*self.boss.lock().unwrap_or_else(PoisonError::into_inner) = boss;
	}

	/// The service is stopping: nothing new starts, nothing more is said.
	pub fn stopping(&self) -> bool {
		self.stopping.load(Ordering::SeqCst)
	}

	pub fn set_stopping(&self) {
		self.stopping.store(true, Ordering::SeqCst);
	}

	/// Today's sessions were read once (`SessionIndex::warm`): until then nothing reads a whole session on a tick.
	pub fn warmed(&self) -> bool {
		self.warmed.load(Ordering::SeqCst)
	}

	pub fn set_warmed(&self) {
		self.warmed.store(true, Ordering::SeqCst);
	}

	fn finishing_map(&self) -> MutexGuard<'_, HashMap<String, Finishing>> {
		self.finishing.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// How far `thread_finish` got with the thread workspace `workspace_id`: neither found again nor reported closed
	/// (CR-11).
	pub fn finishing(&self, workspace_id: &str) -> Option<Finishing> {
		self.finishing_map().get(workspace_id).copied()
	}

	/// Every workspace `thread_finish` has a step recorded for, in no order (the refresh tick forgets the removed ones).
	pub fn finishing_ids(&self) -> Vec<(String, Finishing)> {
		self.finishing_map().iter().map(|(id, how)| (id.clone(), *how)).collect()
	}

	pub fn set_finishing(&self, workspace_id: &str, how: Finishing) {
		self.finishing_map().insert(workspace_id.to_owned(), how);
	}

	pub fn clear_finishing(&self, workspace_id: &str) {
		self.finishing_map().remove(workspace_id);
	}

	/// Into Jocasta's log (0431), which a source run also prints; nothing once the service is stopping.
	pub fn warn(&self, error: impl std::fmt::Display) {
		if self.stopping() {
			return;
		}
		log(LogLevel::Warn, "service", &error.to_string(), None);
	}
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod testing;
#[cfg(test)]
pub(crate) use testing::{TestContext, test_context};
