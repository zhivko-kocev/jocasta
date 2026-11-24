//! The Jocasta service (0392, 0552): herdr runs the panes; this service keeps the boss pane (0404), starts, watches and
//! finishes threads (0388, 0412, 0426), serves the `jocasta` MCP tools, turns threads' changes and his day's events into
//! `[event]` lines and Windows notifications through herdr (0405, 0407, 0413, 0554), reports the sidebar's tokens and
//! the HUD (0402, 0403), and indexes omp's compaction summaries for `history_search` (0389, 0603). No Jocasta code runs
//! inside omp (0386). The Rust port of `service.ts` (0625).
//!
//! This file wires the parts (SVC-15) and holds the start, the stop and the API `server.rs`, `plugin.rs` and `run.rs`
//! use. The context and the parts it builds are in `context.rs`; the parts built after it (`Refresher`, `Tools`, `Keys`,
//! `PushGate`, the watcher) are held here, and so are the timers: the refresh tick, the host poll (gated by `hosts`)
//! and, through `Day`, the day.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared as SharedFuture};
use serde_json::{Map, Value};
use tokio::sync::{OnceCell, watch};
use tokio::task::JoinHandle;

use crate::frontier::brief::BriefLinks;
use crate::frontier::brief::routes::BriefActions;
use crate::frontier::protocol::{
	BriefLink, ClientMessage, Dashboard, Hud, Leftover, PrFacts, ServerMessage, ServiceHealth, ServiceName,
	ServiceState, ToastLevel, ToastReason, Today,
};
use crate::frontier::server::allow::Confirm;
use crate::frontier::server::asker::{Ask, Notifier};
use crate::frontier::server::boss::fresh_session;
use crate::frontier::server::bridge::{Bridge, BridgeService, PaneAgent, PaneEvent, Resolved, Role};
use crate::frontier::server::brief_actions::{self, brief_of_link};
use crate::frontier::server::checks::CheckRunner;
use crate::frontier::server::ci_fix;
use crate::frontier::server::close::CloseService;
use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::cost_cap;
use crate::frontier::server::day::{AlertDetails, Day, DayHooks, DayOptions, DayThread, ScheduleStart, SharedCatalog};
use crate::frontier::server::events::{Toast, clean};
use crate::frontier::server::feed::{FeedService, Reply};
use crate::frontier::server::herdr::{AgentState, Herdr, HerdrSnapshot, StopWatch, pane_text};
use crate::frontier::server::keys::{FinishHow, KeyFinishable, KeyOutcome, Keys, NewThreadForm};
use crate::frontier::server::phone::PhoneFacts;
use crate::frontier::server::popups::Counts;
use crate::frontier::server::pr_review;
use crate::frontier::server::ptt_hold::STT_TOGGLE_KEY;
use crate::frontier::server::push_gate::PushGate;
use crate::frontier::server::refresh::Refresher;
use crate::frontier::server::status::Busy;
use crate::frontier::server::stop_sequence::StopService;
use crate::frontier::server::thread_keys::KeyService;
use crate::frontier::server::tool_specs::{ToolSpec, bad_arguments, tools_for};
use crate::frontier::server::tools::{Caller, ToolReply, Tools};
use crate::frontier::server::watcher::{Watcher, WatcherOptions, start_watcher};
use crate::frontier::server::wording::Door;
use crate::frontier::server::worklog::WorklogSession;
use crate::frontier::server::{day_briefs, weekly_brief};
use crate::shared::day::{day_start, midnight_of};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::same_token;
use crate::shared::link::{open_in_browser, web_link};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::process::AbortSignal;
use crate::shared::projects::Project;
use crate::shared::push_hook::{PushAnswer, PushRequest};
use crate::shared::schedules::Schedule;
use crate::shared::settings::settings_problems;

/// The dock's workspace and the Agents view (0553), healed on every refresh with herdr's panes as read and the boss
/// pane's id.
pub type Heal = Arc<dyn Fn(HerdrSnapshot, Option<String>) -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// Resolves at herdr's next change the refresh reads (`ServerHost::herdr_changed`).
pub type Changed = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// What `JocastaService` is built with. Every field but `herdr` and `broadcast` is optional: `None` is the TypeScript's
/// absent option, and the default each field names is the service's, not this struct's. The service takes `day` out
/// (`Option::take`) to build the day, so the context's copy has none.
pub struct ServiceOptions {
	pub herdr: Arc<dyn Herdr>,
	pub broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>,
	/// The dock's workspace and the Agents view (0553): healed on every refresh. Absent in tests that don't need it.
	pub dock: Option<Heal>,
	/// herdr's changes (fork plan finding 8): each refreshes herdr's facts, and the full refresh comes every minute.
	/// Absent (a test's service, one beside a herdr of its own): everything every `refresh_ms`.
	pub herdr_changed: Option<Changed>,
	/// How a brief reaches his browser (0678): the boss's `show` and the thread brief's toast. Absent in a test's service
	/// that doesn't need it.
	pub briefs: Option<Arc<dyn BriefLinks>>,
	/// Phase 8's pollers and clock (test seam).
	pub day: Option<DayOptions>,
	/// How omp starts: `["omp"]` by default (test and smoke seam).
	pub omp: Option<Vec<String>>,
	/// The boss pane's argv; `omp --continue --config <config>/boss.yml --config <config>/guard.yml` by default (0424,
	/// 0450).
	pub boss_command: Option<Vec<String>>,
	/// A prompt file every project and thread pane appends with `--append-system-prompt`: the shipped
	/// `<config>/ponytail.md` when it exists (0421, 0427) when `None`; `Some(None)` appends nothing (test seam).
	pub pane_prompt: Option<Option<PathBuf>>,
	/// How gh starts for `thread_finish(…, "pr")`: `["gh"]` by default (test seam).
	pub gh: Option<Vec<String>>,
	/// How az starts for `thread_finish(…, "pr")` on Azure DevOps: `["az"]` by default (test seam).
	pub az: Option<Vec<String>>,
	/// How long one gh call may take before it is ended; `GH_TIMEOUT_MS`, a minute (test seam).
	pub gh_timeout_ms: Option<i64>,
	/// omp's sessions dir; `~/.omp/agent/sessions` by default.
	pub sessions_dir: Option<PathBuf>,
	/// How long `thread_start` waits for omp to come up in the new pane before sending the brief; `AGENT_WAIT_MS`, 20 s.
	pub agent_wait_ms: Option<i64>,
	/// How long a project pane's check request waits for its check (0617); `PANE_CHECK_WAIT_MS`, 20 s.
	pub pane_check_wait_ms: Option<i64>,
	/// How often everything is refreshed; 5 s, or a minute with `herdr_changed` (test seam).
	pub refresh_ms: Option<i64>,
	/// How long the end of a thread pane's omp is waited for before its leftovers end (0490); 10 s (test seam).
	pub leftover_wait_ms: Option<i64>,
	/// When leftovers are looked for again, for workers an exiting omp started (0490); 3 s later (test seam).
	pub leftover_again_ms: Option<i64>,
	/// The session watcher's scan interval; 30 s.
	pub watcher_interval_ms: Option<i64>,
	/// mise's argv (test seam); `Some(None)` runs none. Default: the `mise` on PATH (0473, 0477).
	pub mise: Option<Option<Vec<String>>>,
	/// How long the weekly big-picture review may run (0470); 30 minutes.
	pub weekly_review_ms: Option<i64>,
	/// How long the project's `scorecard` task may run before the weekly review goes on without it (0616); 10 minutes.
	pub scorecard_ms: Option<i64>,
	/// `Some(false)` (test seam): the minute tick never starts a weekly review (0470), so no test spawns omp; the boss's
	/// tool still can.
	pub weekly_reviews: Option<bool>,
	/// `Some(false)` (test seam): the refresh tick builds no morning brief and the due weekly review no weekly brief
	/// (0680); his `b` key still builds the morning brief.
	pub auto_briefs: Option<bool>,
	/// Asks him Allow or Deny (0484): a question through the service's question API (0519, 0552), with how it ended
	/// (0578: his answer, three wrong codes, the popup closed, another reader). The signal aborts once the answer isn't
	/// wanted any more (the wait ran out, the service is stopping) and the question goes. Without it every ask counts as
	/// Deny.
	pub confirm: Option<Confirm>,
	/// How long an ask waits for his answer; `CONFIRM_WAIT_MS` (test seam).
	pub confirm_wait_ms: Option<i64>,
	/// The boss pane's key (0484); a new random one per run by default (test seam).
	pub boss_key: Option<String>,
	/// The bridge (0675): one the service listens on by default; a recording one in tests (test seam).
	pub bridge: Option<Bridge>,
	/// How often the dock's heartbeat goes (0662); 10 s (test seam).
	pub alive_ms: Option<i64>,
	/// `Some(false)` (test seam): a task end without a `report_result` goes straight to its review, with no steer asking
	/// for the report first (0539).
	pub ask_report: Option<bool>,
	/// The check runner (0542, 0559): `<data>/checks` by default (test seam).
	pub checks: Option<Arc<CheckRunner>>,
	/// How long a plan-mode turn's end waits for the session to show its proposal; `PLAN_SETTLE_MS` (test seam).
	pub plan_settle_ms: Option<i64>,
	/// How many idle minutes a project pane's omp may run before it sleeps (0676); `service.paneIdleMinutes` (test seam).
	pub pane_idle_minutes: Option<f64>,
	/// The machine's free memory in bytes, `None` when unknown (0676); read from the machine by default (test seam).
	pub available_memory: Option<Arc<dyn Fn() -> Option<u64> + Send + Sync>>,
	/// Asks him a question through the service's question API (0686: the over-budget question): the server's own by
	/// default; without it nothing is asked (test seam).
	pub ask: Option<Ask>,
}

impl ServiceOptions {
	/// Options with herdr and the broadcast, and every default.
	pub fn new(herdr: Arc<dyn Herdr>, broadcast: Arc<dyn Fn(ServerMessage) + Send + Sync>) -> Self {
		Self {
			herdr,
			broadcast,
			dock: None,
			herdr_changed: None,
			briefs: None,
			day: None,
			omp: None,
			boss_command: None,
			pane_prompt: None,
			gh: None,
			az: None,
			gh_timeout_ms: None,
			sessions_dir: None,
			agent_wait_ms: None,
			pane_check_wait_ms: None,
			refresh_ms: None,
			leftover_wait_ms: None,
			leftover_again_ms: None,
			watcher_interval_ms: None,
			mise: None,
			weekly_review_ms: None,
			scorecard_ms: None,
			weekly_reviews: None,
			auto_briefs: None,
			confirm: None,
			confirm_wait_ms: None,
			boss_key: None,
			bridge: None,
			alive_ms: None,
			ask_report: None,
			checks: None,
			plan_settle_ms: None,
			pane_idle_minutes: None,
			available_memory: None,
			ask: None,
		}
	}

	/// Whether the minute tick may start a weekly review (`weeklyReviews !== false`).
	pub fn weekly_reviews_on(&self) -> bool {
		self.weekly_reviews != Some(false)
	}

	/// Whether the tick builds the morning brief and the weekly brief by itself (`autoBriefs !== false`).
	pub fn auto_briefs_on(&self) -> bool {
		self.auto_briefs != Some(false)
	}

	/// Whether a task end without a report is steered to hand one in first (`askReport !== false`).
	pub fn ask_report_on(&self) -> bool {
		self.ask_report != Some(false)
	}
}

/// How often everything is refreshed while nothing reports herdr's changes.
const REFRESH_MS: u64 = 5_000;
/// How often everything is refreshed while herdr's changes refresh its facts: what herdr has no event for (plans a
/// turn's end didn't show, catch-ups, hand commits, spend).
const FULL_REFRESH_MS: u64 = 60_000;
/// How long after herdr's change its facts are read: a burst (a workspace with its tab and pane) is one refresh.
const CHANGE_SETTLE_MS: u64 = 150;
/// How often the dock hears the service is alive (0662); it says the service isn't answering after 30 s without word.
const ALIVE_MS: u64 = 10_000;
/// How long `stop` waits for tool calls in flight before closing the catalog under them (SVC-13).
const CALLS_WAIT_MS: u64 = 25_000;

/// The start under way or done, as every caller of `start` and `stop` waits for it.
type Starting = SharedFuture<BoxFuture<'static, std::result::Result<(), String>>>;

/// What the day needs of the service: the parts reached through the context (`Link`), as the TypeScript's hooks did
/// through its closure.
struct ServiceHooks {
	link: Link,
}

impl DayHooks for ServiceHooks {
	fn send(&self, message: ServerMessage) {
		self.link.ctx().events.send(message);
	}

	fn alert(&self, text: &str, details: AlertDetails) {
		self.link.ctx().events.alert(text, &details);
	}

	fn notify(&self, text: &str, level: ToastLevel) {
		let toast = Toast::new(ToastReason::Event, level, text);
		self.link.ctx().events.send(toast.into());
	}

	fn projects(&self) -> Vec<Project> {
		self.link.ctx().places.listed()
	}

	fn threads(&self) -> Vec<DayThread> {
		let c = self.link.ctx();
		c.threads.all().iter().map(|thread| c.status.day_thread(thread)).collect()
	}

	/// A session last written before the day began holds none of its messages; the folder is listed once. A busy
	/// session index (a read of it is in steps) is "nothing new yet".
	fn sessions_by_project(&self, day: &str) -> HashMap<String, Vec<WorklogSession>> {
		let c = self.link.ctx();
		let mut by_project: HashMap<String, Vec<WorklogSession>> = HashMap::new();
		let Ok(mut sessions) = c.sessions.try_lock() else { return by_project };
		for session in sessions.list(day_start(day, 0)) {
			let Ok(Some(project)) = c.places.project_at(Path::new(&session.cwd)) else { continue };
			let label = session.title.clone().unwrap_or_else(|| basename(&session.cwd));
			let times = sessions.times_on(&session.file, day).to_vec();
			by_project.entry(project.id).or_default().push(WorklogSession { label, times });
		}
		by_project
	}

	/// The same, once the session index is free: the worklog waits for a read in steps rather than writing without
	/// the sessions.
	fn sessions_by_project_waiting(&self, day: &str) -> BoxFuture<'_, HashMap<String, Vec<WorklogSession>>> {
		let c = self.link.ctx();
		let day = day.to_owned();
		Box::pin(async move {
			let mut by_project: HashMap<String, Vec<WorklogSession>> = HashMap::new();
			let mut sessions = c.sessions.lock().await;
			for session in sessions.list(day_start(&day, 0)) {
				let Ok(Some(project)) = c.places.project_at(Path::new(&session.cwd)) else { continue };
				let label = session.title.clone().unwrap_or_else(|| basename(&session.cwd));
				let times = sessions.times_on(&session.file, &day).to_vec();
				by_project.entry(project.id).or_default().push(WorklogSession { label, times });
			}
			by_project
		})
	}

	fn prepare_sessions(&self, day: &str) -> BoxFuture<'_, Result<()>> {
		let c = self.link.ctx();
		let day = day.to_owned();
		Box::pin(async move {
			c.sessions.lock().await.prepare(&day, || c.stopping()).await;
			Ok(())
		})
	}

	fn services(&self) -> BoxFuture<'_, Result<Vec<ServiceHealth>>> {
		let c = self.link.ctx();
		Box::pin(async move {
			let herdr = match c.herdr.ping().await {
				Ok(_) => ServiceHealth { name: ServiceName::Herdr, state: ServiceState::Up, note: None },
				Err(_) => ServiceHealth {
					name: ServiceName::Herdr,
					state: ServiceState::Down,
					note: Some("not answering".to_owned()),
				},
			};
			Ok(vec![herdr])
		})
	}

	fn project_name(&self, cwd: &Path) -> Option<String> {
		self.link.ctx().places.project_at(cwd).ok().flatten().map(|project| project.name)
	}

	fn leftovers(&self) -> BoxFuture<'_, Result<Vec<Leftover>>> {
		let c = self.link.ctx();
		Box::pin(async move { c.status.leftovers().await })
	}

	fn weekly_review(&self, project: &Project) -> BoxFuture<'_, Result<Option<String>>> {
		let c = self.link.ctx();
		let project = project.clone();
		Box::pin(async move { if c.options.weekly_reviews_on() { c.weekly.review(&project).await } else { Ok(None) } })
	}

	/// A scheduled thread's start (0673): E's vet, same-title check and start, through his key's door.
	fn start_scheduled(&self, project: &str, schedule: &Schedule) -> BoxFuture<'static, Result<ScheduleStart>> {
		let c = self.link.ctx();
		let (project, schedule) = (project.to_owned(), schedule.clone());
		Box::pin(async move { c.starts.start_scheduled(&project, &schedule).await })
	}

	fn pr_facts(&self, url: &str) -> Option<PrFacts> {
		self.link.ctx().thread_facts.pr_facts(url)
	}

	fn queued(&self) -> Vec<String> {
		self.link.ctx().pending.rows()
	}

	fn briefs(&self, now: i64) -> Vec<BriefLink> {
		day_briefs::links(&self.link.ctx(), now)
	}

	fn weekly_brief(&self, monday: &str) -> BoxFuture<'_, ()> {
		let (c, monday) = (self.link.ctx(), monday.to_owned());
		Box::pin(async move { weekly_brief::build_weekly(&c, &monday).await })
	}
}

/// The last folder of `cwd`, as `node:path`'s `basename`.
fn basename(cwd: &str) -> String {
	Path::new(cwd).file_name().map_or_else(|| cwd.to_owned(), |name| name.to_string_lossy().into_owned())
}

/// The Jocasta service: the context and the parts built after it, the start and the stop, and the API of `server.rs`.
pub struct JocastaService {
	/// What the parts share (`context.rs`).
	ctx: Arc<ServiceContext>,
	refresher: Arc<Refresher>,
	tools: Tools,
	keys: Keys,
	pushes: PushGate,
	watcher: Mutex<Option<Watcher>>,
	unwatch: Mutex<Option<StopWatch>>,
	/// The refresh tick's and the host poll's tasks.
	timers: Mutex<Vec<JoinHandle<()>>>,
	/// The start under way or done (`stop` waits for it).
	starting: Mutex<Option<Starting>>,
	/// Resolves once the threads herdr already runs were found (the first `discover()`), or when the start failed or
	/// the stop began: a close waits for it before counting the threads at work (LC-9).
	found: watch::Sender<bool>,
	stopped: OnceCell<()>,
	#[cfg(test)]
	ticks: AtomicUsize,
}

fn locked<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	value.lock().unwrap_or_else(PoisonError::into_inner)
}

impl JocastaService {
	/// A service over `catalog`, with every part built and wired (SVC-15); nothing runs until `start`. The options'
	/// `day` is taken out to build the day.
	pub fn new(catalog: SharedCatalog, mut options: ServiceOptions) -> Result<Arc<Self>> {
		let link = Link::new();
		let hooks = Arc::new(ServiceHooks { link: link.clone() });
		let day = Day::new(catalog.clone(), hooks, options.day.take().unwrap_or_default())?;
		let ctx = ServiceContext::new(&link, catalog, options, day);
		Ok(Arc::new(Self {
			refresher: Arc::new(Refresher::new(Arc::clone(&ctx))),
			tools: Tools::new(Arc::clone(&ctx)),
			keys: Keys::new(Arc::clone(&ctx)),
			pushes: PushGate::new(Arc::clone(&ctx)),
			ctx,
			watcher: Mutex::new(None),
			unwatch: Mutex::new(None),
			timers: Mutex::new(Vec::new()),
			starting: Mutex::new(None),
			found: watch::channel(false).0,
			stopped: OnceCell::new(),
			#[cfg(test)]
			ticks: AtomicUsize::new(0),
		}))
	}

	/// What the parts share.
	pub fn context(&self) -> &Arc<ServiceContext> {
		&self.ctx
	}

	pub fn day(&self) -> &Day {
		&self.ctx.day
	}

	/// Resolves once the threads herdr already runs were found (the first `discover()`), or when the start failed or the
	/// stop began: a close waits for it before counting the threads at work (LC-9).
	pub async fn discovered(&self) {
		let mut found = self.found.subscribe();
		let _ = found.wait_for(|found| *found).await;
	}

	/// Starts: syncs projects from the watched roots, ensures the boss pane (0404), finds the threads herdr already
	/// runs, watches every Jocasta pane, starts his day and the session watcher, reads today's sessions without holding
	/// the event loop (the HUD's spend waits for it), then the refresh tick. The start runs on a task of its own from the
	/// call on; a second call gets the first's outcome. Needs a tokio runtime.
	pub fn start(self: &Arc<Self>) -> BoxFuture<'static, Result<()>> {
		let starting = locked(&self.starting)
			.get_or_insert_with(|| {
				let service = Arc::clone(self);
				// A start that fails or ends early still lets a waiting close count the threads (`discovered`).
				let run = tokio::spawn(async move {
					let started = service.start_up(Arc::downgrade(&service)).await;
					service.found.send_replace(true);
					started.map_err(|error| error.to_string())
				});
				let outcome: BoxFuture<'static, std::result::Result<(), String>> =
					async move { run.await.unwrap_or_else(|error| Err(error.to_string())) }.boxed();
				outcome.shared()
			})
			.clone();
		async move { starting.await.map_err(Error::other) }.boxed()
	}

	async fn start_up(&self, me: Weak<Self>) -> Result<()> {
		let c = &self.ctx;
		// The server answers /health before the catalog sync holds the event loop (PERF-7): bare `jocasta` polls it.
		tokio::task::yield_now().await;
		c.places.sync().await;
		// Each problem in jocasta.yaml is said once, at the start (0584, C7).
		for problem in settings_problems() {
			log(LogLevel::Warn, "settings", &problem, None);
			c.events.send(Toast::new(ToastReason::Event, ToastLevel::Warning, format!("settings: {problem}")).into());
		}
		// Only a session whose server this run started and that holds nothing of Jocasta's yet is new (0495).
		self.refresher.set_herdr_default(c.herdr.started_server() && fresh_session(&c.herdr.workspaces().await?));
		c.panes.ensure_boss(true).await?;
		if c.stopping() {
			return Ok(());
		}
		self.refresher.discover(&c.herdr.snapshot().await?).await?;
		self.found.send_replace(true);
		self.begin_watching();
		c.day.start();
		self.start_spend_backfill().await;
		self.start_setup_notice();
		self.start_watcher().await;
		self.refresh().await;
		self.start_timers(&me);
		self.warm_sessions().await;
		c.set_warmed();
		if c.stopping() {
			return Ok(());
		}
		c.day.refresh_activity(Some(true));
		self.refresh().await;
		Ok(())
	}

	/// Watches every Jocasta pane: herdr's pushed states reach the lifecycle. The callbacks hold the context weakly, so
	/// herdr (which the context holds) never keeps it alive.
	fn begin_watching(&self) {
		let (for_panes, for_events) = (Arc::downgrade(&self.ctx), Arc::downgrade(&self.ctx));
		let stop = self.ctx.herdr.watch(
			Box::new(move || {
				let Some(c) = for_panes.upgrade() else { return Vec::new() };
				let boss = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
				boss.into_iter().chain(c.threads.pane_ids()).collect()
			}),
			Box::new(move |event| {
				if let Some(c) = for_events.upgrade() {
					c.lifecycle.herdr_state(&event.pane_id, event.state);
				}
			}),
		);
		*locked(&self.unwatch) = Some(stop);
	}

	/// What setup says at the start, once it was looked at, for the HUD's notice.
	fn start_setup_notice(&self) {
		let c = Arc::downgrade(&self.ctx);
		tokio::spawn(async move {
			let text = crate::setup::setup::setup_toast().await;
			if let (Some(text), Some(c)) = (text, c.upgrade()) {
				c.status.set_setup_notice(text);
			}
		});
	}

	/// The session watcher, over omp's sessions folder and the catalog.
	async fn start_watcher(&self) {
		let c = &self.ctx;
		let for_projects = Arc::downgrade(c);
		let for_errors = Arc::downgrade(c);
		let watcher = start_watcher(WatcherOptions {
			sessions_dir: c.sessions.lock().await.dir.clone(),
			state_file: data_dir().join("watcher.json"),
			db: c.catalog.clone(),
			project_for: Arc::new(move |cwd| {
				let c = for_projects.upgrade()?;
				c.places.project_at(Path::new(cwd)).ok().flatten().map(|project| project.id)
			}),
			interval_ms: c.options.watcher_interval_ms.filter(|ms| *ms > 0).map(i64::unsigned_abs),
			now: None,
			on_error: Some(Arc::new(move |error| {
				if let Some(c) = for_errors.upgrade() {
					c.warn(error);
				}
			})),
		});
		*locked(&self.watcher) = Some(watcher);
	}

	/// The full refresh tick, and herdr's facts refreshed on its changes when herdr reports them (fork plan finding 8);
	/// and omp's own state for each thread pane (0547, 0561), between herdr's events and the refresh tick, never with
	/// the host registry off (0576's `herdr` condition).
	fn start_timers(&self, me: &Weak<Self>) {
		let changes = self.ctx.options.herdr_changed.clone();
		let full_ms = if changes.is_some() { FULL_REFRESH_MS } else { REFRESH_MS };
		let refresh_ms = self.ctx.options.refresh_ms.map_or(full_ms, |ms| ms.unsigned_abs().max(1));
		let mut timers = vec![every(refresh_ms, me.clone(), |service| async move { service.tick() }.boxed())];
		if let Some(changed) = changes {
			timers.push(on_changes(changed, me.clone()));
		}
		// The dock's heartbeat (0662), off a blocking thread: through the day's state and the catalog, the two the hang
		// of 2026-11-23 held, so a service stuck on either stops beating.
		let alive_ms = self.ctx.options.alive_ms.map_or(ALIVE_MS, |ms| ms.unsigned_abs().max(1));
		timers.push(every(alive_ms, me.clone(), |service| {
			async move {
				let (day, catalog) = (service.ctx.day.clone(), service.ctx.catalog.clone());
				let through = tokio::task::spawn_blocking(move || {
					day.answering();
					drop(locked(&catalog));
				});
				if through.await.is_ok() {
					service.ctx.events.send(ServerMessage::Alive);
				}
			}
			.boxed()
		}));
		locked(&self.timers).extend(timers);
	}

	/// The first read of a long session takes seconds: done in steps, so the page and herdr's events go on meanwhile.
	async fn warm_sessions(&self) {
		let c = &self.ctx;
		let mut files: Vec<PathBuf> = Vec::new();
		let own = c.boss().and_then(|boss| boss.with(|boss| boss.session_path.clone())).map(PathBuf::from);
		let threads =
			c.threads.all().into_iter().filter_map(|thread| thread.with(|thread| thread.session_path.clone()));
		let mut sessions = c.sessions.lock().await;
		files.extend(sessions.list(midnight_of(chrono::Utc::now().timestamp_millis())).into_iter().map(|s| s.file));
		files.extend(own.into_iter().chain(threads));
		sessions.warm(&files, || c.stopping()).await;
	}

	/// One tick of the timer: notifications herdr couldn't show are sent again (0584), and a refresh starts unless one
	/// runs.
	fn tick(&self) {
		#[cfg(test)]
		self.ticks.fetch_add(1, Ordering::SeqCst);
		self.ctx.events.resend_unshown();
		self.refresh_facts();
		drop(self.refresher.refresh());
	}

	/// Each open thread's diff, mtime and watched PRs, read off the tick (0668); `ThreadFacts` keeps one read at a time.
	fn refresh_facts(&self) {
		let c = Arc::clone(&self.ctx);
		self.ctx.calls.spawn(async move { c.thread_facts.refresh(&c).await });
	}

	/// The 7-day average starts warm (0668): once, the prior days' totals come from omp's session files of the last 8
	/// days, never over a recorded day.
	async fn start_spend_backfill(&self) {
		let c = Arc::clone(&self.ctx);
		let dir = c.sessions.lock().await.dir.clone();
		self.ctx.calls.spawn(async move {
			c.day.backfill_spend(&dir).await;
		});
	}

	/// How many timer ticks ran, and how many timers run (tests).
	#[cfg(test)]
	pub(crate) fn ticks(&self) -> usize {
		self.ticks.load(Ordering::SeqCst)
	}

	#[cfg(test)]
	pub(crate) fn timer_count(&self) -> usize {
		locked(&self.timers).len()
	}

	/// Stops the service; herdr's panes keep running (0392). The HUD says Jocasta is off and the sidebar's tokens are
	/// cleared (UX-13); tokens a herdr that doesn't answer keeps expire with their TTL. Safe to call twice: a second
	/// call waits for the first. The catalog closes when the last holder of it drops (the connection is shared).
	pub async fn stop(&self) {
		self.stopped.get_or_init(|| self.stop_once()).await;
	}

	async fn stop_once(&self) {
		let c = &self.ctx;
		c.set_stopping();
		self.found.send_replace(true);
		c.allow.withdraw_all();
		c.review.end_pane_waits();
		// A start still running stops at its next step; what it set up is torn down below.
		let starting = locked(&self.starting).clone();
		if let Some(starting) = starting {
			let _ = starting.await;
		}
		// Tool calls in flight finish first (a squash mid-commit removes its thread), for a while (SVC-13).
		c.calls.settled_within(CALLS_WAIT_MS).await;
		for timer in locked(&self.timers).drain(..) {
			timer.abort();
		}
		if let Some(unwatch) = locked(&self.unwatch).take() {
			unwatch();
		}
		if let Some(watcher) = locked(&self.watcher).take() {
			watcher.stop();
		}
		c.day.stop().await;
		if let Some(running) = self.refresher.running() {
			running.await;
		}
		c.lifecycle.settled().await;
		// A check still running ends with the service; nothing is stored for it.
		c.checks.stop().await;
		c.review.settled().await;
		c.status.clear().await;
		// Notifications still waiting on herdr, and their lines in notifications.jsonl, before herdr closes.
		c.events.settled().await;
		c.herdr.close();
	}

	/// One refresh tick (`refresh.rs`); a call while one runs gets that one. Notifications herdr couldn't show for want
	/// of a client are sent again (0584).
	pub async fn refresh(&self) {
		self.ctx.events.resend_unshown();
		self.refresh_facts();
		self.refresher.refresh().await;
	}

	/// Before Jocasta stops everything (0549): the threads at work are kept for `--restore` (`OmpTracker::pause`).
	pub async fn pause_working(&self) -> Result<()> {
		self.ctx.omps.pause().await
	}

	/// Stop everything (0549): the omps kept for `--restore` end mid-turn (`OmpTracker::cut_paused`).
	pub async fn cut_paused_turns(&self) {
		self.ctx.omps.cut_paused().await;
	}

	/// Once herdr's server is gone (0486, 0490): what the thread panes' omps left ends.
	pub async fn end_thread_leftovers(&self) -> Result<()> {
		self.ctx.omps.end_leftovers().await
	}

	/// One big-picture review of a project (0470), one at a time per project (SVC-12).
	pub async fn weekly_review(&self, project: &Project) -> Result<Option<String>> {
		self.ctx.weekly.review(project).await
	}

	/// Leftover worktrees (0456): thread checkouts no open thread holds.
	pub async fn leftovers(&self) -> Result<Vec<Leftover>> {
		self.ctx.status.leftovers().await
	}

	/// The HUD; `today` is the one the caller built, else `today`'s now.
	pub fn hud(&self, today: Option<&Today>) -> Result<Hud> {
		match today {
			Some(today) => Ok(self.ctx.status.hud_for(today)),
			None => self.ctx.status.hud(),
		}
	}

	/// What the phone check-in pushes about (0672): the rows awaiting him, what finished, and quiet hours.
	pub fn phone_facts(&self) -> PhoneFacts {
		let (needs, finished) = match self.ctx.day.today() {
			Ok(today) => (today.needs_you, today.landed),
			Err(_) => (Vec::new(), Vec::new()),
		};
		PhoneFacts { needs, finished, quiet: self.ctx.day.quiet() }
	}

	/// The threads closing Jocasta would cut off (0434, 0438): awaiting him, and working among the rest.
	pub fn busy_threads(&self) -> Result<Busy> {
		self.ctx.status.busy_threads()
	}

	/// The boss pane's state, herdr's `done` as idle.
	pub fn boss_state(&self) -> AgentState {
		self.ctx.events.boss_state()
	}

	/// Whether the service has handled every pane state herdr pushed or a refresh caught up (tests).
	#[cfg(test)]
	pub(crate) fn states_settled(&self) -> bool {
		self.ctx.lifecycle.states_settled()
	}

	/// A toast as a Windows notification through herdr (0407, 0554), logged in `notifications.jsonl` (H2). `show` is
	/// `options.show !== false`: `None` shows.
	pub async fn notify(&self, toast: &Toast, show: Option<bool>) {
		self.ctx.events.notify(toast, show != Some(false)).await;
	}

	/// Push-to-talk (0556): omp's dictation toggle (`app.stt.toggle`, bound by setup) sent to the boss pane through herdr,
	/// whether or not it has the focus.
	pub async fn toggle_dictation(&self) -> Result<()> {
		let Some(boss) = self.ctx.boss() else { return Err(Error::other("the boss pane isn't open yet")) };
		let pane_id = boss.with(|boss| boss.pane_id.clone());
		self.ctx.herdr.send_keys(&pane_id, &[STT_TOGGLE_KEY.to_owned()]).await?;
		Ok(())
	}

	/// Runs one tool, CLI or key call (SVC-13): refused once Jocasta is closing, and counted while it runs, so the stop
	/// waits for it before closing the catalog under it.
	async fn track<T>(&self, run: impl Future<Output = Result<T>>) -> Result<T> {
		self.closing()?;
		self.ctx.calls.run(run).await
	}

	/// The refusal of a call that comes once Jocasta is closing.
	fn closing(&self) -> Result<()> {
		if self.ctx.stopping() {
			return Err(Error::user("Jocasta is closing; nothing was changed."));
		}
		Ok(())
	}

	/// One tool call over the bridge (0675). A refusal is `is_error` with a one-line reason.
	pub async fn call_tool(&self, name: &str, args: &Map<String, Value>, caller: &Caller) -> ToolReply {
		reply_of(self.track(self.tools.tool(name, args, caller)).await)
	}

	/// One `jocasta thread|worktree` command (a bridge `cli` message, 0544): the same functions and rules as the boss's tools;
	/// `thread send`, `thread stop`, `thread finish` and `worktree remove` wait for his Allow. A refusal is `is_error`.
	pub async fn cli_call(&self, command: &str, args: &Map<String, Value>) -> ToolReply {
		reply_of(self.track(self.tools.cli(command, args)).await)
	}

	/// The pre-push hook's question (a bridge `push` message, 0613): whether the push may go ahead, which may wait for his Allow.
	/// Counted like a tool call (SVC-13); a refusal is the answer `{allow: false, reason}`.
	pub async fn push_verdict(&self, request: &PushRequest) -> PushAnswer {
		match self.track(self.pushes.verdict(request)).await {
			Ok(answer) => answer,
			Err(error) => PushAnswer { allow: false, reason: Some(error.to_string()) },
		}
	}

	/// A project pane's check at its turn's end (a bridge `stop` request, 0617): the text for its agent, or "" for nothing.
	/// `signal` ends when the pane stops asking. Counted like a tool call (SVC-13); a failure of its own answers nothing,
	/// as the extension would.
	pub async fn pane_check(&self, pane_key: &str, signal: Option<AbortSignal>) -> String {
		match self.track(self.ctx.review.pane_check(pane_key, signal)).await {
			Ok(text) => text,
			Err(error) => {
				self.ctx.warn(error);
				String::new()
			}
		}
	}

	/// A pane's omp pushed `event` over the bridge (0675): a thread's turn and dialog states reach the lifecycle as omp's
	/// own, as the host poll's did (0547, 0561); an omp that can't take prompts is said (0684).
	pub fn pane_event(&self, who: &Resolved, event: &PaneEvent, agent: &PaneAgent) {
		let c = &self.ctx;
		match event {
			PaneEvent::Connected { version, caps, .. } if !caps.iter().any(|cap| cap == "prompt") => {
				c.events.warning(&format!("omp {version} can't take prompts from Jocasta; check the pin"));
			}
			PaneEvent::Gone => c.omps.host_reports(&who.pane_id, None),
			PaneEvent::AgentStart | PaneEvent::AgentEnd { .. } | PaneEvent::Input { .. } => {
				let state = if agent.input_open {
					AgentState::Blocked
				} else if agent.working {
					AgentState::Working
				} else {
					AgentState::Idle
				};
				c.omps.host_reports(&who.pane_id, Some(state));
				if matches!(event, PaneEvent::AgentEnd { .. }) {
					// A thread at its cost cap is stopped and asked about after the turn that reached it (0686).
					let (c, pane_id) = (self.ctx.clone(), who.pane_id.clone());
					tokio::spawn(async move {
						if let Err(error) = cost_cap::after_turn(&c, &pane_id).await {
							c.warn(error);
						}
					});
				}
			}
			_ => {}
		}
	}

	// ---- his keys (0544, 0554), `keys.rs` ----

	pub async fn key_stop(&self, pane_id: Option<&str>) -> Result<String> {
		self.keys.stop(pane_id).await
	}

	pub async fn key_finishable(&self, pane_id: Option<&str>) -> Result<KeyFinishable> {
		self.keys.finishable(pane_id).await
	}

	pub async fn key_finish(&self, pane_id: &str, how: FinishHow) -> Result<KeyOutcome> {
		self.keys.finish(pane_id, how).await
	}

	pub async fn key_review(&self, pane_id: Option<&str>) -> Result<String> {
		self.keys.review(pane_id).await
	}

	pub async fn new_thread_form(&self, pane_id: Option<&str>) -> Result<NewThreadForm> {
		self.keys.new_thread_form(pane_id).await
	}

	pub async fn key_start(&self, project: &str, brief: &str) -> Result<KeyOutcome> {
		self.keys.start(project, brief).await
	}

	// ---- the state feed ----

	/// What a client gets on connect (0409): the HUD, `today` and the dashboard.
	pub fn snapshot(&self) -> Result<ServerMessage> {
		let today = self.ctx.day.today()?;
		let dashboard: Dashboard = self.ctx.day.dashboard();
		let hud = self.hud(Some(&today))?;
		clean(&ServerMessage::Snapshot { hud, today, dashboard }).map_err(|error| Error::other(error.to_string()))
	}

	/// The http(s) links `today` and the dashboard show now (0581, UX-7): the only ones `link.open` opens.
	fn shown_links(&self) -> Result<HashSet<String>> {
		fn walk(value: &Value, links: &mut HashSet<String>) {
			match value {
				Value::Array(items) => items.iter().for_each(|item| walk(item, links)),
				Value::Object(fields) => {
					for (key, inner) in fields {
						match (key.as_str(), inner) {
							("url", Value::String(url)) => links.extend(web_link(Some(url))),
							_ => walk(inner, links),
						}
					}
				}
				_ => {}
			}
		}
		let mut links = HashSet::new();
		walk(&serde_json::to_value(self.ctx.day.today()?)?, &mut links);
		walk(&serde_json::to_value(self.ctx.day.dashboard())?, &mut links);
		Ok(links)
	}

	/// One message from a client (SEC-NET-5: `pane.focus` and `link.open` only; the type of any other fails to read, so
	/// it never gets here); `reply` answers that client only. Refusals come back as `error`.
	pub async fn handle(&self, message: ClientMessage, reply: impl Fn(ServerMessage)) {
		let message = match message {
			ClientMessage::LeftoverRemove { folder } => {
				if let Some(answer) = self.remove_leftover(&folder).await {
					reply(answer);
				}
				return;
			}
			ClientMessage::PrReview { url } => {
				reply(self.started_by_code(pr_review::start(&self.ctx, &url)).await);
				return;
			}
			ClientMessage::CiFix { url } => {
				reply(self.started_by_code(ci_fix::start(&self.ctx, &url)).await);
				return;
			}
			ClientMessage::BriefMorning => {
				// His `b` (0680): today's morning brief, built from what the day holds now, then opened.
				let done = self.track(day_briefs::open_morning(&self.ctx)).await;
				reply(match done {
					Ok(text) => ServerMessage::Action { text, is_error: false },
					Err(error) => ServerMessage::Action { text: error.to_string(), is_error: true },
				});
				return;
			}
			other => other,
		};
		if let Err(error) = self.handle_one(message).await {
			reply(ServerMessage::Error { message: error.to_string() });
		}
	}

	/// His `R` on a review request or `F` on a red pipeline (0680): the review thread or the fix thread, started by code
	/// with no turn of the boss. Only a link an open event of the row's kind carries is acted on; the answer is an
	/// action, started or refused.
	async fn started_by_code(&self, start: impl Future<Output = Result<String>>) -> ServerMessage {
		match self.track(start).await {
			Ok(text) => ServerMessage::Action { text, is_error: false },
			Err(error) => ServerMessage::Action { text: error.to_string(), is_error: true },
		}
	}

	/// His `x` on a leftover in the dock (0670): only a folder `leftovers` lists is acted on (anything else is
	/// ignored, nothing said); his Allow comes first (`Door::Key`); the result comes back as an action, removed or not.
	async fn remove_leftover(&self, folder: &Path) -> Option<ServerMessage> {
		let listed = self.ctx.status.leftovers().await.ok()?;
		let leftover = listed.into_iter().find(|leftover| leftover.folder == folder)?;
		let path = leftover.folder.to_string_lossy().into_owned();
		let done = self.track(self.tools.remove_leftover(&path, Door::Key)).await;
		Some(match done {
			Ok(text) => ServerMessage::Action { text, is_error: false },
			Err(error) => ServerMessage::Action { text: error.to_string(), is_error: true },
		})
	}

	async fn handle_one(&self, message: ClientMessage) -> Result<()> {
		match message {
			// Answered in `handle`, which replies with an action.
			ClientMessage::LeftoverRemove { .. }
			| ClientMessage::PrReview { .. }
			| ClientMessage::CiFix { .. }
			| ClientMessage::BriefMorning => {}
			ClientMessage::PaneFocus { pane_id } => self.ctx.herdr.focus(&pane_id).await?,
			ClientMessage::LinkOpen { url } => {
				// A brief's link (the Done toast's) opens its page with a fresh launch code (0678).
				if let (Some(id), Some(briefs)) = (brief_of_link(&self.ctx, &url), self.ctx.options.briefs.as_ref()) {
					briefs.open(&id);
					return Ok(());
				}
				// Only a link the service itself shows now, and only http(s): a client can't open anything else (C9).
				let link =
					web_link(Some(&url)).filter(|link| self.shown_links().is_ok_and(|shown| shown.contains(link)));
				let Some(link) = link else { return Err(Error::user("That link isn't one today shows.")) };
				open_in_browser(&link)?;
			}
		}
		Ok(())
	}
}

impl BriefActions for JocastaService {
	fn act(&self, id: String, action: Value) -> BoxFuture<'static, Result<Value>> {
		let (ctx, tools) = (Arc::clone(&self.ctx), self.tools.clone());
		Box::pin(async move { brief_actions::act(&ctx, &tools, &id, &action).await })
	}
}

/// A tool's or a CLI call's answer to the caller: the text cleaned of terminal escapes, or the refusal.
fn reply_of(result: Result<String>) -> ToolReply {
	match result {
		Ok(text) => ToolReply { text: pane_text(&text), is_error: None },
		Err(error) => ToolReply { text: error.to_string(), is_error: Some(true) },
	}
}

/// A task that runs `work` every `period_ms` (the first run after one period) until the service is gone.
fn every(
	period_ms: u64,
	me: Weak<JocastaService>,
	work: impl Fn(Arc<JocastaService>) -> BoxFuture<'static, ()> + Send + 'static,
) -> JoinHandle<()> {
	tokio::spawn(async move {
		let period = Duration::from_millis(period_ms);
		let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
		timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
		loop {
			timer.tick().await;
			let Some(service) = me.upgrade() else { break };
			work(service).await;
		}
	})
}

/// A task that reads herdr's facts again `CHANGE_SETTLE_MS` after each change herdr reports, until the service is
/// gone. Notifications herdr couldn't show are sent again too (0584), as the tick does.
fn on_changes(changed: Changed, me: Weak<JocastaService>) -> JoinHandle<()> {
	tokio::spawn(async move {
		loop {
			changed().await;
			tokio::time::sleep(Duration::from_millis(CHANGE_SETTLE_MS)).await;
			let Some(service) = me.upgrade() else { break };
			service.ctx.events.resend_unshown();
			drop(service.refresher.facts());
		}
	})
}

impl StopService for JocastaService {
	fn pause_working(&self) -> BoxFuture<'_, Result<()>> {
		Box::pin(JocastaService::pause_working(self))
	}

	fn stop(&self) -> BoxFuture<'_, Result<()>> {
		Box::pin(async move {
			JocastaService::stop(self).await;
			Ok(())
		})
	}

	fn cut_paused_turns(&self) -> BoxFuture<'_, Result<()>> {
		Box::pin(async move {
			JocastaService::cut_paused_turns(self).await;
			Ok(())
		})
	}

	fn end_thread_leftovers(&self) -> BoxFuture<'_, Result<()>> {
		Box::pin(JocastaService::end_thread_leftovers(self))
	}
}

impl KeyService for JocastaService {
	fn key_stop<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		Box::pin(JocastaService::key_stop(self, pane_id))
	}

	fn key_review<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		Box::pin(JocastaService::key_review(self, pane_id))
	}

	fn key_finishable<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<KeyFinishable>> {
		Box::pin(JocastaService::key_finishable(self, pane_id))
	}

	fn key_finish<'a>(&'a self, pane_id: &'a str, how: FinishHow) -> BoxFuture<'a, Result<KeyOutcome>> {
		Box::pin(JocastaService::key_finish(self, pane_id, how))
	}

	fn new_thread_form<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<NewThreadForm>> {
		Box::pin(JocastaService::new_thread_form(self, pane_id))
	}

	fn key_start<'a>(&'a self, project: &'a str, brief: &'a str) -> BoxFuture<'a, Result<KeyOutcome>> {
		Box::pin(JocastaService::key_start(self, project, brief))
	}

	fn key_stop_all<'a>(&'a self) -> BoxFuture<'a, Result<String>> {
		Box::pin(self.keys.stop_all())
	}
}

impl Notifier for JocastaService {
	fn notify(&self, toast: Toast, show: bool) -> BoxFuture<'_, ()> {
		Box::pin(async move { JocastaService::notify(self, &toast, Some(show)).await })
	}
}

/// What the panes' omps and the `jocasta` CLI ask of the service over the bridge (0675). A refusal is already the
/// reply's `is_error`.
impl BridgeService for JocastaService {
	fn resolve(&self, key: &str) -> Option<Resolved> {
		let c = &self.ctx;
		if same_token(Some(key), c.panes.boss_key()) {
			let pane_id = c.panes.boss_keyed()?;
			let caller = Caller { pane_key: c.launched.key_of(&pane_id), boss_key: Some(key.to_owned()) };
			return Some(Resolved { pane_id, role: Role::Boss, caller });
		}
		let pane_id = c.launched.pane_of_key(Some(key))?;
		let role = if c.threads.get(&pane_id).is_some() { Role::Thread } else { Role::Project };
		Some(Resolved { pane_id, role, caller: Caller { pane_key: Some(key.to_owned()), boss_key: None } })
	}

	fn tools(&self, role: Role) -> Vec<Value> {
		tools_for(role, self.ctx.layer).iter().map(ToolSpec::to_value).collect()
	}

	fn call(&self, who: Resolved, tool: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move {
			let listed = tools_for(who.role, self.ctx.layer);
			if let Some(spec) = listed.iter().find(|spec| spec.name == tool)
				&& let Some(problem) = bad_arguments(spec, &args)
			{
				return ToolReply { text: problem, is_error: Some(true) };
			}
			JocastaService::call_tool(self, &tool, &args, &who.caller).await
		})
	}

	fn cli(&self, command: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move { JocastaService::cli_call(self, &command, &args).await })
	}

	fn push(&self, request: PushRequest) -> BoxFuture<'_, PushAnswer> {
		Box::pin(async move { JocastaService::push_verdict(self, &request).await })
	}

	fn stop(&self, pane_key: String, signal: AbortSignal) -> BoxFuture<'_, String> {
		Box::pin(async move { JocastaService::pane_check(self, &pane_key, Some(signal)).await })
	}

	fn event(&self, who: &Resolved, event: &PaneEvent, agent: &PaneAgent) {
		JocastaService::pane_event(self, who, event, agent);
	}
}

impl FeedService for JocastaService {
	fn snapshot(&self) -> Result<ServerMessage> {
		JocastaService::snapshot(self)
	}

	fn handle(&self, message: ClientMessage, reply: Reply) -> BoxFuture<'_, ()> {
		Box::pin(async move { JocastaService::handle(self, message, |message| reply(message)).await })
	}
}

impl CloseService for JocastaService {
	fn discovered(&self) -> BoxFuture<'static, ()> {
		let mut found = self.found.subscribe();
		Box::pin(async move {
			let _ = found.wait_for(|found| *found).await;
		})
	}

	/// The TypeScript's throws here would reject the close; a failed count is logged and counts nothing instead.
	fn busy_threads(&self) -> Counts {
		match JocastaService::busy_threads(self) {
			Ok(Busy { working, awaiting }) => Counts { working, awaiting },
			Err(error) => {
				log(LogLevel::Warn, "shutdown", &format!("the busy threads couldn't be counted: {error}"), None);
				Counts { working: 0, awaiting: 0 }
			}
		}
	}
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
