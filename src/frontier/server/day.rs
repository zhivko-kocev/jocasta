//! His day (phase 8): the pollers (8.1, 0301), the event queue with quiet hours (8.2, 0298), delivery (8.3, 0299),
//! interjections and their reminder (8.7, 0303), the `today` view (8.4, 0300), the inbox and worklog (8.5, 0302), and
//! the dashboard's third column (0429): news, weather, machine and services, today's activity. The Rust port of
//! `day.ts` (0625).
//!
//! `Day` is one type whose scheduler, alert, reminder and worklog methods share its database, clock, hooks and
//! timers; splitting would pass that state around for no gain (0610). Its methods sit in three files: this one (the
//! state, the timers, delivery and the worklog), `day_poll.rs` (the pollers, the machine sample, quota, leftovers and
//! the weekly review) and `day_view.rs` (the dashboard, `today`, the inbox and the worklog on request).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Local, SecondsFormat, Timelike, Utc};
use futures_util::future::BoxFuture;
use rusqlite::Connection;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::frontier::protocol::{
	Activity, ActivityEvent, Leftover, Machine, NewsList, QuotaSpan, QuotaWindow, ServerMessage, ServiceHealth,
	ThreadState, ToastLevel, ToastPriority, ToastReason, Weather,
};
use crate::frontier::server::worklog::{GitPlaces, WorklogSession, worklog_path, write_worklog};
use crate::shared::credentials::calendar_secret;
use crate::shared::day::{day_start, local_day};
use crate::shared::errors::{Error, Result};
use crate::shared::events::inbox::Inbox;
use crate::shared::events::queue::{EventPriority, EventQueue, EventRow, NewEvent};
use crate::shared::events::quiet::{QuietHours, is_quiet, read_quiet_hours};
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::pollers::calendar::{Fetched, Meeting};
use crate::shared::pollers::cli::{CliCommand, RunCli, RunFuture, SystemCli};
use crate::shared::pollers::machine::{CpuTimes, cpu_times, sample_machine};
use crate::shared::pollers::settings::{AzureDevOpsSettings, CalendarSettings, GithubSettings, NewsSettings};
use crate::shared::pollers::settings::{PollerSettings, read_poller};
use crate::shared::pollers::weather::Place;
use crate::shared::process::AbortSignal;
use crate::shared::projects::Project;
use crate::shared::schema::open_me_db;
use crate::shared::settings::{Settings, SettingsError, gone_settings, read_settings};

#[path = "day_poll.rs"]
mod poll;
#[path = "day_view.rs"]
mod view;

pub use view::parse_when;

const MINUTE: i64 = 60_000;
/// The one reminder comes 30 minutes after an event was raised (0303).
const REMINDER_AFTER_MS: i64 = 30 * MINUTE;
/// A meeting is urgent this close to its start (0298).
const MEETING_SOON_MS: i64 = 5 * MINUTE;
/// Disk under this fraction free is urgent (0303).
const DISK_LOW: f64 = 0.1;
/// Without quiet hours, the worklog is written at this hour (0302: "at day's end").
const WORKLOG_HOUR: u32 = 23;
/// The weather refreshes every 30 minutes (0429); Open-Meteo's data moves every 15.
const WEATHER_MINUTES: f64 = 30.0;
/// CPU, RAM, disk and the services are sampled this often (0429; every 10 s, 0443).
const MACHINE_MS: u64 = 10_000;
/// Today's worklog so far is recomputed this often: it reads every project's sessions and git log.
const WORKLOG_SO_FAR_MS: i64 = 10 * MINUTE;
/// The activity lists' length (0429).
const ACTIVITY_ROWS: usize = 10;
/// The subscription windows are read this often (0456).
const QUOTA_MINUTES: f64 = 5.0;
/// Leftover worktrees are looked for this often (0456): they change only when a thread is closed.
const LEFTOVER_MINUTES: f64 = 10.0;
/// The weekly review (0470) runs from this weekday (0 = Sunday) and hour on, once per project and week.
const WEEKLY_REVIEW_DAY: u32 = 1;
const WEEKLY_REVIEW_HOUR: u32 = 10;

/// The shared catalog database: the service and the day both use it.
pub type SharedCatalog = Arc<Mutex<Connection>>;

/// The clock, in ms since the epoch.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// A fetch of the calendar's feed (`None`: it couldn't be fetched), replacing the real HTTP one.
pub type CalendarFetch = Arc<dyn Fn(String) -> BoxFuture<'static, Option<Fetched>> + Send + Sync>;

/// Test seams; the service's server runs with the defaults (`DayOptions::default()`).
#[derive(Default)]
pub struct DayOptions {
	/// Whether the pollers run (0301); tests turn them off and call `poll()` themselves. Default true.
	pub pollers: Option<bool>,
	/// `gh` and `az`; the day passes its own abort signal to each call.
	pub run: Option<Arc<dyn RunCli>>,
	/// The news endpoint, instead of brutalist.report's (the tests' in-process server).
	pub news_endpoint: Option<String>,
	/// The geocoding endpoint, instead of Open-Meteo's.
	pub geocoding_endpoint: Option<String>,
	/// The forecast endpoint, instead of Open-Meteo's.
	pub forecast_endpoint: Option<String>,
	/// The calendar's fetch, instead of the real one.
	pub calendar_fetch: Option<CalendarFetch>,
	pub now: Option<Clock>,
	/// The calendar's secret iCal URL (0430): Credential Manager, else the env var it was taken from.
	pub calendar_secret: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,
	/// The free fraction of the data dir's disk.
	pub disk_free: Option<Arc<dyn Fn() -> f64 + Send + Sync>>,
	/// `omp usage --json`'s output (0456).
	pub usage: Option<Arc<dyn Fn() -> RunFuture + Send + Sync>>,
}

/// How a delivered event reaches the boss pane (0299, 0405): urgent at once, normal when the boss is idle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertDetails {
	pub priority: ToastPriority,
	pub url: Option<String>,
	pub reminder: bool,
	/// The parts of the text that came from outside (SEC-2): PR titles and repos, check and pipeline names, meeting
	/// titles. The service marks them in the boss's line as information, never his instruction.
	pub outside: Vec<String>,
}

/// The event kinds whose line after " — " is someone else's words (SEC-2): a blocked thread's question is its omp's
/// (SE-2), but never Jocasta's own tails.
const OUTSIDE_KINDS: [&str; 5] = ["review", "pipeline", "meeting-soon", "blocked", "weekly-review"];
/// A blocked row's tails Jocasta wrote itself: never quoted.
const OWN_TAILS: [&str; 2] = ["awaiting you", "plan ready"];

/// A thread pane as `today` shows it (0409).
#[derive(Debug, Clone, PartialEq)]
pub struct DayThread {
	pub pane_id: String,
	pub title: String,
	pub project: Option<String>,
	/// One plain line: what it is doing, its question, or "review ready".
	pub line: String,
	/// The titles of the project's other open threads that change the same files (0456).
	pub overlaps: Option<Vec<String>>,
	pub state: ThreadState,
	/// When it last changed state, epoch ms.
	pub changed_at: i64,
	/// When its review was ready (0456), epoch ms: set while it is reviewed and not finished.
	pub reviewed_at: Option<i64>,
	/// Its session's cost (USD) and context (tokens), for `today` (0555).
	pub cost: Option<f64>,
	pub context: Option<u64>,
}

/// What the day service needs from the Jocasta service.
pub trait DayHooks: Send + Sync {
	fn send(&self, message: ServerMessage);
	/// An `[event]` line for the boss pane (0299, 0405).
	fn alert(&self, text: &str, details: AlertDetails);
	/// A toast for the page: `level` is a warning or an error.
	fn notify(&self, text: &str, level: ToastLevel);
	fn projects(&self) -> Vec<Project>;
	fn threads(&self) -> Vec<DayThread>;
	/// Every project's omp sessions (in its main checkout and its worktrees) that may hold messages of `day`, by
	/// project id: the sessions folder listed once (CO-12).
	fn sessions_by_project(&self, day: &str) -> HashMap<String, Vec<WorklogSession>>;
	/// [`DayHooks::sessions_by_project`] that waits for a busy session index instead of reading "nothing new yet": the
	/// worklog (0302) must not be written without the sessions only because a read of another file was in steps.
	fn sessions_by_project_waiting(&self, day: &str) -> BoxFuture<'_, HashMap<String, Vec<WorklogSession>>> {
		Box::pin(std::future::ready(self.sessions_by_project(day)))
	}
	/// Reads the sessions that may hold `day`'s messages in steps, letting the event loop run, so
	/// `sessions_by_project` for that day then only parses what was appended.
	fn prepare_sessions(&self, day: &str) -> BoxFuture<'_, Result<()>>;
	/// Whether whiteboard and herdr answer (0429), for the dashboard's services row.
	fn services(&self) -> BoxFuture<'_, Result<Vec<ServiceHealth>>>;
	/// The watched project a folder (a main checkout or a thread's worktree) belongs to, by name; `None` outside.
	fn project_name(&self, cwd: &Path) -> Option<String>;
	/// Thread worktrees closed in herdr and still on disk (0456).
	fn leftovers(&self) -> BoxFuture<'_, Result<Vec<Leftover>>>;
	/// The weekly big-picture review of one project (0470): what merged in the past 7 days against its rules and
	/// lessons, read-only. Resolves to the result's one line, or `None` when nothing merged that week.
	fn weekly_review(&self, project: &Project) -> BoxFuture<'_, Result<Option<String>>>;
}

/// The pollers, by the names `jocasta.yaml` and the failure lines give them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PollerName {
	Github,
	AzureDevOps,
	Calendar,
	News,
	Weather,
}

impl PollerName {
	/// `github`, `azureDevOps`, `calendar`, `news`, `weather`.
	pub fn name(self) -> &'static str {
		match self {
			Self::Github => "github",
			Self::AzureDevOps => "azureDevOps",
			Self::Calendar => "calendar",
			Self::News => "news",
			Self::Weather => "weather",
		}
	}
}

/// What failed in a round or a settings read: a settings problem (the service may have said it at start, 0584, C7) or
/// any other fault.
enum Fault {
	Settings(SettingsError),
	Other(Error),
}

impl Fault {
	fn message(&self) -> String {
		match self {
			Self::Settings(error) => error.message.clone(),
			Self::Other(error) => error.to_string(),
		}
	}

	/// Whether it is a settings problem the service already said at start (0584, C7).
	fn said(&self) -> bool {
		matches!(self, Self::Settings(error) if error.said)
	}
}

impl<E: Into<Error>> From<E> for Fault {
	fn from(error: E) -> Self {
		Self::Other(error.into())
	}
}

/// Flips its flag when dropped, so whoever waits on the paired receiver goes on even if the task was cancelled.
struct Finish(watch::Sender<bool>);

impl Drop for Finish {
	fn drop(&mut self) {
		self.0.send_replace(true);
	}
}

/// A piece of work under way, finished once its [`Finish`] drops.
type Done = watch::Receiver<bool>;

async fn finished(mut done: Done) {
	// A sender dropped without flipping the flag can't happen (`Finish` flips it); either way the work is over.
	let _ = done.wait_for(|over| *over).await;
}

/// `gh`/`az` for the pollers, ended when the day stops.
struct Halted {
	run: Arc<dyn RunCli>,
	signal: AbortSignal,
}

impl RunCli for Halted {
	fn run(
		&self,
		command: CliCommand,
		args: Vec<String>,
		ok_codes: Vec<i32>,
		_signal: Option<AbortSignal>,
	) -> RunFuture {
		self.run.run(command, args, ok_codes, Some(self.signal.clone()))
	}
}

/// Everything that changes while the day runs.
struct State {
	meetings: Vec<Meeting>,
	timers: Vec<JoinHandle<()>>,
	/// The poller rounds under way, awaited by `stop()` before the databases close (F7).
	rounds: HashMap<PollerName, Done>,
	last_error: HashMap<String, String>,
	today_sent: String,
	worklog_day: String,
	/// The local day of the last minute tick: a new one checks yesterday's worklog was written (SRV-B-5).
	tick_day: String,
	/// The worklog being written (yesterday's at start, today's at day's end): its sessions are read in steps first.
	worklog_writing: Option<Done>,
	// The dashboard's third column (0429): kept in memory, never indexed, spoken or sent to the boss.
	news: Vec<NewsList>,
	news_error: Option<String>,
	weather: Option<Weather>,
	weather_error: Option<String>,
	/// The resolved `dashboard.weather`, so the place is looked up once per setting.
	place: Option<(String, Place)>,
	machine: Option<Machine>,
	cpu_before: Option<CpuTimes>,
	sampling: bool,
	services: Vec<ServiceHealth>,
	activity: Activity,
	/// The service's thread lines delivered today, for today's activity.
	service_events: Vec<ActivityEvent>,
	worklog_so_far_at: i64,
	/// Whether today's sessions were read in steps, so the worklog so far may read them (CO-11).
	worklog_ready: bool,
	/// The worklog so far's pass running now (it runs git off the event loop, CO-12).
	worklog_so_far: Option<Done>,
	dashboard_sent: String,
	/// The subscription windows (0456), as `omp usage` last reported them.
	quota_windows: Vec<QuotaWindow>,
	/// The windows past the warning whose toast went out; a window drops out once it is back under.
	quota_warned: Vec<QuotaSpan>,
	quota_reading: bool,
	leftover_list: Vec<Leftover>,
	leftovers_reading: Option<Done>,
	/// The weekly reviews (0470) run or checked this week, by `<week>|<project id>`; running ones included.
	weekly_done: HashSet<String>,
}

struct Inner {
	catalog: SharedCatalog,
	/// `me.db`; `None` once the day stopped.
	me: Mutex<Option<Connection>>,
	hooks: Arc<dyn DayHooks>,
	clock: Clock,
	pollers: bool,
	/// `gh`/`az`, ended when the day stops.
	run: Halted,
	/// Ends the pollers' fetches and `gh`/`az` runs when the day stops.
	halt: watch::Sender<Option<String>>,
	halt_signal: AbortSignal,
	news_endpoint: Option<String>,
	geocoding_endpoint: Option<String>,
	forecast_endpoint: Option<String>,
	calendar_fetch: Option<CalendarFetch>,
	calendar_secret: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,
	disk_free: Option<Arc<dyn Fn() -> f64 + Send + Sync>>,
	usage: Option<Arc<dyn Fn() -> RunFuture + Send + Sync>>,
	/// Each project's git email and origin for the worklogs, kept for one local day (CO-12).
	places: GitPlaces,
	stopped: AtomicBool,
	state: Mutex<State>,
}

/// His day. A cheap handle: clones share one day.
#[derive(Clone)]
pub struct Day {
	inner: Arc<Inner>,
}

/// `Date.toISOString()` of `ms`.
pub(crate) fn iso(ms: i64) -> String {
	DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The local time of epoch-ms `ms`.
pub(crate) fn local(ms: i64) -> DateTime<Local> {
	DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&Local)
}

/// A reminder's `[event]` line (UX-05): `Reminder (16:07) — send the invoice`.
fn reminder_line(text: &str, due: i64) -> String {
	format!("Reminder ({}) — {text}", crate::shared::day::hhmm(due))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Day {
	/// A day on the shared `catalog` (the event queue's database), with the service's `hooks`.
	pub fn new(catalog: SharedCatalog, hooks: Arc<dyn DayHooks>, options: DayOptions) -> Result<Self> {
		let clock: Clock = options.now.unwrap_or_else(|| Arc::new(|| Utc::now().timestamp_millis()));
		let (halt, halt_signal) = watch::channel(None);
		let run = Halted { run: options.run.unwrap_or_else(|| Arc::new(SystemCli)), signal: halt_signal.clone() };
		let me = open_me_db()?;
		let now = clock();
		let state = State {
			meetings: Vec::new(),
			timers: Vec::new(),
			rounds: HashMap::new(),
			last_error: HashMap::new(),
			today_sent: String::new(),
			worklog_day: String::new(),
			tick_day: local_day(now),
			worklog_writing: None,
			news: Vec::new(),
			news_error: None,
			weather: None,
			weather_error: None,
			place: None,
			machine: None,
			cpu_before: None,
			sampling: false,
			services: Vec::new(),
			activity: Activity { events: Vec::new(), worklog: Vec::new(), summaries: Vec::new() },
			service_events: Vec::new(),
			worklog_so_far_at: 0,
			worklog_ready: false,
			worklog_so_far: None,
			dashboard_sent: String::new(),
			quota_windows: Vec::new(),
			quota_warned: Vec::new(),
			quota_reading: false,
			leftover_list: Vec::new(),
			leftovers_reading: None,
			weekly_done: HashSet::new(),
		};
		Ok(Self {
			inner: Arc::new(Inner {
				catalog,
				me: Mutex::new(Some(me)),
				hooks,
				clock,
				pollers: options.pollers != Some(false),
				run,
				halt,
				halt_signal,
				news_endpoint: options.news_endpoint,
				geocoding_endpoint: options.geocoding_endpoint,
				forecast_endpoint: options.forecast_endpoint,
				calendar_fetch: options.calendar_fetch,
				calendar_secret: options.calendar_secret,
				disk_free: options.disk_free,
				usage: options.usage,
				places: GitPlaces::new(),
				stopped: AtomicBool::new(false),
				state: Mutex::new(state),
			}),
		})
	}

	fn st(&self) -> MutexGuard<'_, State> {
		lock(&self.inner.state)
	}

	fn now_ms(&self) -> i64 {
		(self.inner.clock)()
	}

	fn stopped(&self) -> bool {
		self.inner.stopped.load(Ordering::SeqCst)
	}

	/// The event queue (0298) on the shared catalog, with the day's clock.
	pub fn queue<R>(&self, work: impl FnOnce(&EventQueue<'_>) -> R) -> R {
		let db = lock(&self.inner.catalog);
		work(&EventQueue::with_clock(&db, || self.now_ms()))
	}

	/// The inbox (0302) on `me.db`, with the day's clock; fails once the day stopped.
	pub fn inbox<R>(&self, work: impl FnOnce(&Inbox<'_>) -> Result<R>) -> Result<R> {
		let me = lock(&self.inner.me);
		let db = me.as_ref().ok_or_else(|| Error::other("closed database"))?;
		work(&Inbox::with_clock(db, || self.now_ms()))
	}

	/// Starts the pollers, the machine sample and the minute tick; writes yesterday's worklog when it is missing. Today's
	/// activity leaves the worklog so far for later (`refresh_activity(Some(true))` once today's sessions were read in
	/// steps, CO-11). Needs a running tokio runtime.
	pub fn start(&self) {
		self.catch_up_worklog();
		// Nothing thrown in a tick may escape the timer: that would end the service (CO-1).
		self.every(Duration::from_millis(MINUTE.unsigned_abs()), Job::Tick);
		self.refresh_activity(Some(false));
		// `frontier.autostart` and `frontier.dashboard.display` went with the app (0557): said once at each start.
		if let Ok(settings) = read_settings() {
			for gone in gone_settings(&settings) {
				self.inner.hooks.notify(&gone, ToastLevel::Warning);
			}
		}
		if !self.inner.pollers {
			return;
		}
		// The first CPU figure needs two readings; the first sample comes a moment after the baseline.
		self.st().cpu_before = Some(cpu_times());
		self.later(Duration::from_secs(2), Job::Sample);
		self.every(Duration::from_millis(MACHINE_MS), Job::Sample);
		self.later(Duration::ZERO, Job::Quota);
		self.every(minutes(QUOTA_MINUTES), Job::Quota);
		self.later(Duration::ZERO, Job::Leftovers);
		self.every(minutes(LEFTOVER_MINUTES), Job::Leftovers);
		// A jocasta.yaml that isn't YAML turns off all its settings, the pollers and weather with them; the service
		// said it at start (0584).
		let Ok(settings) = read_settings() else { return };
		// Each poller reads only its own keys: a bad value turns that poller alone off (0584).
		if let Some(github) = self.poller::<GithubSettings>(&settings) {
			self.poll_every(PollerName::Github, github.every_minutes);
		}
		if let Some(azure) = self.poller::<AzureDevOpsSettings>(&settings)
			&& !azure.orgs.is_empty()
		{
			self.poll_every(PollerName::AzureDevOps, azure.every_minutes);
		}
		if let Some(calendar) = self.poller::<CalendarSettings>(&settings)
			&& self.secret().is_some()
		{
			self.poll_every(PollerName::Calendar, calendar.every_minutes);
		}
		if let Some(news) = self.poller::<NewsSettings>(&settings)
			&& !news.feeds.is_empty()
		{
			self.poll_every(PollerName::News, news.every_minutes);
		}
		// Reads `dashboard.weather` each round: hidden until set, picked up within 30 minutes once he sets it.
		self.poll_every(PollerName::Weather, WEATHER_MINUTES);
	}

	fn poll_every(&self, name: PollerName, every_minutes: f64) {
		self.later(Duration::ZERO, Job::Poll(name));
		self.every(minutes(every_minutes), Job::Poll(name));
	}

	fn poller<P: PollerSettings>(&self, settings: &Settings) -> Option<P> {
		match read_poller::<P>(settings) {
			Ok(poller) => Some(poller),
			Err(error) => {
				self.report_once(P::NAME, &error.message, error.said);
				None
			}
		}
	}

	/// A timer that fires `job` every `period`, the first time one period from now; dropped with the day.
	fn every(&self, period: Duration, job: Job) {
		let weak = Arc::downgrade(&self.inner);
		let timer = tokio::spawn(async move {
			let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
			ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
			loop {
				ticker.tick().await;
				let Some(inner) = weak.upgrade() else { break };
				Day { inner }.run_job(job).await;
			}
		});
		self.st().timers.push(timer);
	}

	/// A timer that fires `job` once, `delay` from now.
	fn later(&self, delay: Duration, job: Job) {
		let weak = Arc::downgrade(&self.inner);
		let timer = tokio::spawn(async move {
			tokio::time::sleep(delay).await;
			if let Some(inner) = weak.upgrade() {
				Day { inner }.run_job(job).await;
			}
		});
		self.st().timers.push(timer);
	}

	async fn run_job(&self, job: Job) {
		match job {
			Job::Tick => self.guard("tick", || self.tick()),
			Job::Sample => self.sample().await,
			Job::Quota => self.read_quota().await,
			Job::Leftovers => self.refresh_leftovers().await,
			Job::Poll(name) => self.poll(name).await,
		}
	}

	/// Stops every timer and ends the poller rounds under way, awaiting them before its database closes (the service
	/// closes the catalog after this, F7); resolves once a worklog-so-far pass and a worklog being written let go of
	/// their project folders (git) and wrote nothing more.
	pub async fn stop(&self) {
		let (timers, rounds) = {
			let mut state = self.st();
			self.inner.stopped.store(true, Ordering::SeqCst);
			(std::mem::take(&mut state.timers), state.rounds.values().cloned().collect::<Vec<_>>())
		};
		for timer in timers {
			timer.abort();
		}
		self.inner.halt.send_replace(Some("the day stopped".to_owned()));
		for round in rounds {
			finished(round).await;
		}
		*lock(&self.inner.me) = None;
		let so_far = self.st().worklog_so_far.clone();
		if let Some(so_far) = so_far {
			finished(so_far).await;
		}
		self.worklog_writing().await;
	}

	/// Settles once the worklogs queued so far are written (yesterday's at start, today's at day's end).
	pub async fn worklog_writing(&self) {
		let writing = self.st().worklog_writing.clone();
		if let Some(writing) = writing {
			finished(writing).await;
		}
	}

	fn secret(&self) -> Option<String> {
		self.inner.calendar_secret.as_ref().map_or_else(calendar_secret, |secret| secret())
	}

	/// Quiet hours now (0298); a malformed setting is reported once and counts as none.
	pub fn quiet(&self) -> bool {
		is_quiet(self.quiet_hours().as_ref(), self.now_ms())
	}

	/// `service.quietHours`, read live; a malformed value is reported once and counts as none.
	fn quiet_hours(&self) -> Option<QuietHours> {
		let read = read_settings().map_err(Fault::from).and_then(|s| read_quiet_hours(&s).map_err(Fault::Settings));
		match read {
			Ok(hours) => {
				self.st().last_error.remove("quietHours");
				hours
			}
			Err(error) => {
				self.report_once("quietHours", &error.message(), error.said());
				None
			}
		}
	}

	/// A failure is shown as a toast and logged once until it changes or clears, not every round. A settings problem
	/// the service said at start (`said`) is only remembered, so it is said once (0584).
	fn report_once(&self, key: &str, message: &str, said: bool) {
		{
			let mut state = self.st();
			if state.last_error.get(key).is_some_and(|last| last == message) {
				return;
			}
			state.last_error.insert(key.to_owned(), message.to_owned());
		}
		if said {
			return;
		}
		log(LogLevel::Warn, "day", message, None);
		self.inner.hooks.notify(message, ToastLevel::Warning);
	}

	fn guard(&self, key: &str, work: impl FnOnce() -> Result<()>) {
		match work() {
			Ok(()) => {
				self.st().last_error.remove(key);
			}
			Err(error) => self.report_once(key, &format!("{key}: {error}"), false),
		}
	}

	/// A poller's round: what went away from the part it saw is resolved; what is new is delivered. Nothing once the day
	/// stopped.
	fn ingest(
		&self,
		source: crate::shared::events::queue::EventSource,
		kinds: &[&str],
		round: &crate::shared::events::queue::PollRound,
	) -> Result<()> {
		if self.stopped() {
			return Ok(());
		}
		let added = self.queue(|queue| queue.sync(source, kinds, &round.events, Some(&*round.observed)))?;
		for row in &added {
			self.deliver(row, false)?;
		}
		Ok(())
	}

	/// A thread's `[event]` line held in quiet hours (0298), kept as a queue row so a restart keeps it too; sent to the
	/// boss pane once they end (`deliver_held`).
	pub fn hold(&self, text: &str) -> Result<()> {
		let at = iso(self.now_ms());
		self.queue(|queue| {
			queue.add(&event(
				&format!("thread:held:{at}|{text}"),
				crate::shared::events::queue::EventSource::Thread,
				"held",
				EventPriority::Normal,
				text,
			))
		})?;
		Ok(())
	}

	/// Outside quiet hours: the thread lines held during them go to the boss pane as `[event]` lines, oldest first (0405;
	/// as 0470's weekly review waits for their end). Only the boss line: their toast showed when they happened, and
	/// today's activity listed them then.
	fn deliver_held(&self) -> Result<()> {
		for row in self.queue(|queue| queue.open())? {
			if row.kind != "held" {
				continue;
			}
			self.inner.hooks.alert(
				&row.text,
				AlertDetails { priority: ToastPriority::Normal, url: None, reminder: false, outside: Vec::new() },
			);
			self.queue(|queue| queue.resolve(&row.source_id))?;
		}
		Ok(())
	}

	/// Adds an event and delivers it when new.
	pub fn raise(&self, new: &NewEvent) -> Result<Option<EventRow>> {
		let row = self.queue(|queue| queue.add(new))?;
		if let Some(row) = &row {
			self.deliver(row, false)?;
		}
		Ok(row)
	}

	/// Delivery (0299, 0405, 0407): each delivered event is an `[event]` line for the boss pane and a notification
	/// (through herdr, 0554); low goes only to `today`. In quiet hours only urgent goes through; the rest waits for the
	/// morning `today` (0298). The one reminder of an event still open (0303) reads `Still open: <line>` (UX-05).
	fn deliver(&self, row: &EventRow, reminder: bool) -> Result<()> {
		if row.priority == EventPriority::Low || (row.priority == EventPriority::Normal && self.quiet()) {
			return Ok(());
		}
		let priority =
			if row.priority == EventPriority::Urgent { ToastPriority::Urgent } else { ToastPriority::Normal };
		let text = if reminder { format!("Still open: {}", row.text) } else { row.text.clone() };
		let dash = row.text.find(" — ");
		// Poller and calendar lines are `<kind> — <what they said>`, a blocked thread's `<project>/<title> — <its
		// question>`; reminders are his, disk is Jocasta's.
		let tail = dash.map_or("", |at| &row.text[at + " — ".len()..]);
		let outside = if OUTSIDE_KINDS.contains(&row.kind.as_str())
			&& dash.is_some()
			&& !(row.kind == "blocked" && OWN_TAILS.contains(&tail.trim()))
		{
			vec![tail.to_owned()]
		} else {
			Vec::new()
		};
		self.inner.hooks.alert(&text, AlertDetails { priority, url: row.url.clone(), reminder, outside });
		self.inner.hooks.send(ServerMessage::Toast {
			reason: ToastReason::Event,
			level: if priority == ToastPriority::Urgent { ToastLevel::Warning } else { ToastLevel::Info },
			text,
			priority: Some(priority),
			pane_id: row.thread_id.clone(),
			url: row.url.clone(),
		});
		self.queue(|queue| if reminder { queue.mark_reminded(row.id) } else { queue.mark_raised(row.id) })?;
		self.refresh_activity(None);
		self.push_dashboard();
		Ok(())
	}

	/// A thread pane waiting on him (0298: urgent). The service already delivered it; the queue keeps it for the one
	/// reminder (0303) until the pane moves on.
	pub fn thread_blocked(&self, pane_id: &str, text: &str, project_id: Option<&str>) -> Result<()> {
		let mut blocked = event(
			&format!("thread:blocked:{pane_id}"),
			crate::shared::events::queue::EventSource::Thread,
			"blocked",
			EventPriority::Urgent,
			text,
		);
		blocked.thread_id = Some(pane_id.to_owned());
		blocked.project_id = project_id.filter(|id| !id.is_empty()).map(str::to_owned);
		if let Some(row) = self.queue(|queue| queue.add(&blocked))? {
			self.queue(|queue| queue.mark_raised(row.id))?;
		}
		Ok(())
	}

	pub fn thread_unblocked(&self, pane_id: &str) -> Result<()> {
		self.queue(|queue| queue.resolve(&format!("thread:blocked:{pane_id}")))
	}

	/// Every minute: meetings starting in 5 minutes, due reminders, disk space, the one reminder of unresolved events
	/// (0303), the thread lines held in quiet hours once they end, the day's-end worklog, today's activity, and the open
	/// `today` view.
	pub fn tick(&self) -> Result<()> {
		// A tick on a new day (the machine may have slept through the day's end): yesterday's worklog, if missing.
		let day = local_day(self.now_ms());
		let new_day = {
			let mut state = self.st();
			(state.tick_day != day).then(|| state.tick_day.clone_from(&day))
		};
		if new_day.is_some() {
			self.catch_up_worklog();
		}
		let quiet = self.quiet();
		self.meetings_soon()?;
		for item in self.inbox(|inbox| inbox.due())? {
			let due = item.due_at.as_deref().map_or(0, view::parse_iso);
			let mut reminder = event(
				&format!("inbox:{}", item.id),
				crate::shared::events::queue::EventSource::Inbox,
				"reminder",
				EventPriority::Normal,
				&reminder_line(&item.text, due),
			);
			reminder.url = None;
			self.raise(&reminder)?;
		}
		self.guard("disk", || self.check_disk());
		for row in self.queue(|queue| queue.due_reminders(REMINDER_AFTER_MS))? {
			if row.priority == EventPriority::Normal && quiet {
				continue;
			}
			self.deliver(&row, true)?;
		}
		if !quiet {
			self.guard("held lines", || self.deliver_held());
		}
		self.end_of_day(quiet);
		self.guard("weekly review", || self.weekly_reviews(quiet));
		self.refresh_activity(None);
		self.push_today()?;
		self.push_dashboard();
		Ok(())
	}

	fn meetings_soon(&self) -> Result<()> {
		let now = self.now_ms();
		for row in self.queue(|queue| queue.open())? {
			if row.kind != "meeting-soon" {
				continue;
			}
			let last = row.source_id.rsplit('|').next().unwrap_or_default().trim();
			let start = if last.is_empty() { Some(0) } else { last.parse::<i64>().ok() };
			if !start.is_some_and(|start| start > now) {
				self.queue(|queue| queue.resolve(&row.source_id))?;
			}
		}
		let meetings = self.st().meetings.clone();
		for m in meetings {
			let until = m.start - now;
			if m.all_day || until <= 0 || until > MEETING_SOON_MS {
				continue;
			}
			let mut soon = event(
				&format!("calendar:soon:{}|{}", m.uid, m.start),
				crate::shared::events::queue::EventSource::Calendar,
				"meeting-soon",
				EventPriority::Urgent,
				&format!("Meeting in {} min — {}", (until + MINUTE - 1) / MINUTE, m.title),
			);
			soon.url = m.url;
			self.raise(&soon)?;
		}
		Ok(())
	}

	fn check_disk(&self) -> Result<()> {
		let free = match &self.inner.disk_free {
			Some(free) => free(),
			None => disk_free_of_data()?,
		};
		let source_id = "machine:disk";
		if free >= DISK_LOW {
			return self.queue(|queue| queue.resolve(source_id));
		}
		self.raise(&event(
			source_id,
			crate::shared::events::queue::EventSource::Machine,
			"disk",
			EventPriority::Urgent,
			&format!("Disk almost full — {}% free", (free * 100.0 + 0.5).floor()),
		))?;
		Ok(())
	}

	/// The worklog at day's end (0302): when quiet hours start, or at 23:00 without them; once per day.
	fn end_of_day(&self, quiet: bool) {
		let now = self.now_ms();
		let day = local_day(now);
		if self.st().worklog_day == day {
			return;
		}
		let hours = self.quiet_hours();
		let hour = local(now).hour();
		// Quiet hours that start after midnight belong to the next morning, so the day ends at 23:00 instead.
		let ended = match hours {
			Some(hours) if hours.from >= 12 * 60 => quiet && hour >= 12,
			_ => hour >= WORKLOG_HOUR,
		};
		if !ended {
			return;
		}
		self.st().worklog_day.clone_from(&day);
		self.queue_worklog(day);
	}

	/// Yesterday's worklog when it is missing (0302): at start, and at the first tick of a new day, which catches a day
	/// whose end the machine slept through (SRV-B-5). The calendar's day before, not 24 hours back: right after a
	/// spring-forward day those differ (CO-17).
	fn catch_up_worklog(&self) {
		let yesterday = local_day(day_start(&local_day(self.now_ms()), -1));
		if self.st().worklog_day == yesterday || worklog_path(&yesterday).exists() {
			return;
		}
		self.st().worklog_day.clone_from(&yesterday);
		self.queue_worklog(yesterday);
	}

	/// Writes `day`'s worklog after any one being written.
	fn queue_worklog(&self, day: String) {
		let (finish, done) = watch::channel(false);
		let previous = self.st().worklog_writing.replace(done);
		let this = self.clone();
		tokio::spawn(async move {
			let _finish = Finish(finish);
			if let Some(previous) = previous {
				finished(previous).await;
			}
			this.write_worklog_for(&day).await;
		});
	}

	/// Writes a day's worklog (0302) once its sessions were read in steps; a failure is said once.
	async fn write_worklog_for(&self, day: &str) {
		// The worklog then reads what it needs itself when this fails.
		let _ = self.inner.hooks.prepare_sessions(day).await;
		if self.stopped() {
			return;
		}
		let by_project = self.inner.hooks.sessions_by_project_waiting(day).await;
		let projects = self.inner.hooks.projects();
		let stopped = || self.stopped();
		let written = write_worklog(
			&projects,
			day,
			|project, _| by_project.get(&project.id).cloned().unwrap_or_default(),
			Some(&self.inner.places),
			Some(&stopped),
		)
		.await;
		match written {
			Ok(_) => {
				self.st().last_error.remove("worklog");
			}
			Err(error) => self.report_once("worklog", &format!("worklog: {error}"), false),
		}
	}

	/// One of the service's thread lines (done and reviewed, closed, a PR), delivered now: into today's activity.
	pub fn record_event(&self, text: &str) {
		let at = iso(self.now_ms());
		{
			let mut state = self.st();
			state.service_events.insert(0, ActivityEvent { at, text: text.to_owned() });
			state.service_events.truncate(ACTIVITY_ROWS);
		}
		self.refresh_activity(None);
		self.push_dashboard();
	}
}

/// The timers' jobs.
#[derive(Clone, Copy)]
enum Job {
	Tick,
	Sample,
	Quota,
	Leftovers,
	Poll(PollerName),
}

fn minutes(minutes: f64) -> Duration {
	Duration::from_secs_f64(minutes * 60.0)
}

/// A new event with only what every one has.
fn event(
	source_id: &str,
	source: crate::shared::events::queue::EventSource,
	kind: &str,
	priority: EventPriority,
	text: &str,
) -> NewEvent {
	NewEvent {
		source_id: source_id.to_owned(),
		source,
		kind: kind.to_owned(),
		priority,
		text: text.to_owned(),
		url: None,
		project_id: None,
		thread_id: None,
	}
}

/// The free fraction of the data dir's disk (whole percents, as the dashboard's machine row has it).
fn disk_free_of_data() -> Result<f64> {
	let zero = CpuTimes { busy: 0, total: 0 };
	let disk = sample_machine(zero, zero, &data_dir())?.disk;
	Ok(if disk.total > 0 { disk.free as f64 / disk.total as f64 } else { 1.0 })
}

#[cfg(test)]
#[path = "day_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "day_threads_tests.rs"]
mod threads_tests;

#[cfg(test)]
#[path = "day_stop_tests.rs"]
mod stop_tests;

#[cfg(test)]
#[path = "day_settings_tests.rs"]
mod settings_tests;
