//! What the service shows of its threads (0402, 0403, 0409, 0538, 0540, 0546): herdr's sidebar tokens, the HUD,
//! `today`'s thread rows, `thread_status` and `projects`, each thread's ledger and task file, and their overlaps
//! (0456).
//!
//! The part's state sits behind one lock that is never held across an `.await` and never while another part is called;
//! a thread is read or changed through its [`ThreadRef`]. The session index (`ctx.sessions`) is a tokio lock: a
//! synchronous reader (the day's hooks, `hud`, `write_task`) takes it with `try_lock` and treats a busy index as "nothing
//! new yet", an async one waits for it (see "The service's shape" in `docs/research/rust-port.md`).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared, join_all, try_join_all};
use serde_json::json;

use crate::frontier::protocol::{
	Hud, Leftover, ServerMessage, ThreadState, ToastLevel, ToastPriority, ToastReason, Today, TopThread,
};
use crate::frontier::server::bridge::PaneAgent;
use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::day::DayThread;
use crate::frontier::server::events::Toast;
use crate::frontier::server::herdr::{AgentState, HerdrResult, PaneMetadata, STOP_PING_MS, Tokens, WorkspaceInfo};
use crate::frontier::server::native::{PIN_TOKEN, PIN_VALUE};
use crate::frontier::server::pane_launch::thread_budget;
use crate::frontier::server::report::report_line;
use crate::frontier::server::sessions::SessionIndex;
use crate::frontier::server::spend::{SpendTracker, spend_settled, spend_today};
use crate::frontier::server::task_file::{save_task, task_file};
use crate::frontier::server::thread::{Thread, ThreadRef, subject, thread_state};
use crate::frontier::server::wording::{Overlap, money, overlap_said, review_said, short_model};
use crate::frontier::server::worktrees::{find_leftovers, git_out};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::ledger::{ThreadLedger, ledger_line, thread_ledger};
use crate::shared::projects::{Project, list_projects};

/// The costliest open thread of `today`'s UNDERWAY rows (0668); none while no row has a cost above zero.
fn top_thread(today: &Today) -> Option<TopThread> {
	today
		.underway
		.iter()
		.filter_map(|row| row.cost.filter(|cost| *cost > 0.0).map(|cost| (row, cost)))
		.max_by(|a, b| a.1.total_cmp(&b.1))
		.map(|(row, cost)| TopThread { title: row.title.clone(), cost })
}

/// The sidebar tokens expire unless reported again within this time, so a stopped service leaves none (UX-13).
const METADATA_TTL_MS: i64 = 15 * 60_000;
/// Unchanged tokens are reported again after this long, well inside their TTL.
const METADATA_RENEW_MS: i64 = 5 * 60_000;
/// How long `stop` waits for herdr to take the cleared tokens.
const CLEAR_WAIT_MS: u64 = 2_000;
/// How long a thread's changed files are kept before git is asked again (0456's overlaps).
const OVERLAP_KEPT_MS: i64 = 60_000;
/// A thread's ledger is read again at most this often, and only when one of its session files grew (SVC-8).
const LEDGER_KEPT_MS: i64 = 5_000;
/// The spend at which omp warns in a thread's pane, and the boss hears it once (0541, PRM-11).
const SPEND_WARNED_USD: f64 = 50.0;

/// A thread's changed files, as one read gives them; the error is its message, so every caller of a shared read can
/// have it.
type Files = Result<Vec<String>, String>;
/// One read of a thread's changed files, shared by every caller meanwhile.
pub type SharedFiles = Shared<BoxFuture<'static, Files>>;

/// Each thread's changed files being read for one answer, by pane, so one read serves every line of it (SVC-4).
#[derive(Default)]
pub struct FileReads(Mutex<HashMap<String, SharedFiles>>);

impl FileReads {
	fn map(&self) -> MutexGuard<'_, HashMap<String, SharedFiles>> {
		self.0.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

/// How many threads work and how many await him, counted apart (`busyThreads`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
	pub working: usize,
	pub awaiting: usize,
}

/// What a thread is read by when its files are: its pane, project and worktree.
#[derive(Clone)]
struct Target {
	pane_id: String,
	project: Project,
	path: PathBuf,
}

impl Target {
	fn of(thread: &ThreadRef) -> Self {
		thread.with(|t| Self { pane_id: t.pane_id.clone(), project: t.project.clone(), path: t.path.clone() })
	}
}

/// What `today`, the sidebar and the HUD read of a thread's session.
struct Brief {
	model: Option<String>,
	question: Option<String>,
	last_line: Option<String>,
	context: Option<u64>,
}

/// A thread's session read once, with its whole cost.
struct Look {
	brief: Brief,
	spend: Option<f64>,
}

/// A thread's ledger as last read (SVC-8, SRV-B-18): its current session, its files' sizes, when, and the ledger.
#[derive(Clone)]
struct LedgerKept {
	at: i64,
	path: PathBuf,
	sizes: String,
	ledger: Option<ThreadLedger>,
}

/// Sidebar tokens last reported, with when, so unchanged ones wait to renew.
struct Reported {
	json: String,
	at: i64,
}

/// The thread's facts the sidebar is built from.
struct Row {
	pane_id: String,
	project: String,
	session_path: Option<PathBuf>,
	plan_awaiting: bool,
	review_ready: bool,
	working: bool,
	awaiting: bool,
}

/// Counts of a project workspace's threads working and awaiting him, by workspace.
type Counts = Vec<(String, (u32, u32))>;

#[derive(Default)]
struct State {
	/// herdr's workspaces as last read, for the project workspaces' tokens.
	workspaces: Vec<WorkspaceInfo>,
	/// `jocasta setup`'s pending changes, said once as a notification (0466).
	setup_notice: Option<String>,
	/// Sidebar tokens last reported, per pane and per project workspace, in the order first reported.
	reported: Vec<(String, Reported)>,
	/// The HUD's figures as last pushed to the page (FE-4): sent whenever they change, the quota's included.
	hud_sent: String,
	/// Each thread's changed files, by pane, when they were read and the read's number: read again after a minute
	/// (0456); a read that ends after a later one never replaces it.
	changed_files: HashMap<String, (i64, u64, Vec<String>)>,
	/// Each thread's changed files being read now, by pane, with the read's number, so one read serves every caller
	/// meanwhile.
	files_reading: HashMap<String, (u64, SharedFiles)>,
	/// The last read's number.
	read_seq: u64,
	/// Each thread's ledger as last read, by pane.
	ledgers: HashMap<String, LedgerKept>,
	/// Today's spend as last counted, for a look while the index is busy.
	last_spend: Option<f64>,
	/// A task that reads the sessions a list of today's files found is running.
	settling: bool,
}

/// `status.ts`'s `StatusViews`.
pub struct StatusViews {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	st: Mutex<State>,
	/// What `spend_today` keeps between calls; behind a tokio lock because the stepped read of new files holds it.
	spend: tokio::sync::Mutex<SpendTracker>,
	/// Held from a task file's snapshot to its write, so a snapshot taken earlier is never written over a later one.
	task_writes: Mutex<()>,
}

/// A thread's todo count and context use as its row says them, `4/9 · 62%` (0677); a part its omp hasn't reported is
/// left out.
fn progress_of(agent: Option<&PaneAgent>) -> Option<String> {
	let agent = agent?;
	let parts: Vec<String> = [
		agent.todo.map(|(done, total)| format!("{done}/{total}")),
		agent.usage_percent.map(|percent| format!("{}%", percent.round() as i64)),
	]
	.into_iter()
	.flatten()
	.collect();
	(!parts.is_empty()).then(|| parts.join(" · "))
}

/// A sleeping thread's row says so after its line (0676): `review ready · asleep`.
fn asleep_after(line: String, pane_id: &str, launched: &crate::frontier::server::boss::LaunchedPanes) -> String {
	match (launched.asleep(pane_id), line.is_empty()) {
		(false, _) => line,
		(true, true) => "asleep".to_owned(),
		(true, false) => format!("{line} · asleep"),
	}
}

/// A thread row's line in `today` (0409): its question or plan while it waits on him, its review while done, else the
/// last line its session said.
fn rail_line(t: &Thread, brief: Option<&Brief>) -> String {
	if t.shown == ThreadState::Blocked {
		if plan_awaiting(t) {
			return "plan ready".to_owned();
		}
		return brief.and_then(|b| b.question.clone()).unwrap_or_else(|| "awaiting you".to_owned());
	}
	if t.shown != ThreadState::Done {
		return brief.and_then(|b| b.last_line.clone()).unwrap_or_default();
	}
	if t.phase == crate::frontier::server::thread::Phase::Review {
		return "in review".to_owned();
	}
	match t.review.as_ref().and_then(|review| review.result.as_ref()) {
		None => "done".to_owned(),
		Some(result) => format!("done and reviewed: {}", review_said(result.notes, result.tests, result.blocking)),
	}
}

/// Whether the thread's plan waits for his approval in omp's Plan Review.
fn plan_awaiting(t: &Thread) -> bool {
	t.plan.as_ref().is_some_and(|plan| plan.awaiting.as_deref().is_some_and(|title| !title.is_empty()))
}

impl StatusViews {
	pub fn new(link: Link) -> Self {
		Self {
			link,
			st: Mutex::new(State::default()),
			spend: tokio::sync::Mutex::new(SpendTracker::new()),
			task_writes: Mutex::new(()),
		}
	}

	/// Held from a task file's snapshot to its write by every writer of a live thread's task file.
	pub fn task_writing(&self) -> MutexGuard<'_, ()> {
		self.task_writes.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn st(&self) -> MutexGuard<'_, State> {
		self.st.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// herdr's workspaces as last read.
	pub fn workspaces(&self) -> Vec<WorkspaceInfo> {
		self.st().workspaces.clone()
	}

	/// Remembers herdr's workspaces for the project workspaces' tokens.
	pub fn set_workspaces(&self, workspaces: Vec<WorkspaceInfo>) {
		self.st().workspaces = workspaces;
	}

	/// `jocasta setup`'s pending changes, said once as a notification on the next `report` (0466).
	pub fn set_setup_notice(&self, text: String) {
		self.st().setup_notice = Some(text);
	}

	/// Forgets what was read of a closed or finished thread.
	pub fn forget(&self, thread: &ThreadRef) {
		let pane_id = thread.with(|t| t.pane_id.clone());
		let mut st = self.st();
		st.changed_files.remove(&pane_id);
		st.ledgers.remove(&pane_id);
	}

	/// Runs `f` on the session index when nobody holds it; `None` for a busy index (it is being read in steps).
	fn try_sessions<R>(&self, f: impl FnOnce(&mut SessionIndex) -> R) -> Option<R> {
		let ctx = self.link.ctx();
		let mut sessions = ctx.sessions.try_lock().ok()?;
		Some(f(&mut sessions))
	}

	/// Runs `f` on the session index once it is free.
	async fn sessions<R>(&self, f: impl FnOnce(&mut SessionIndex) -> R) -> R {
		let ctx = self.link.ctx();
		let mut sessions = ctx.sessions.lock().await;
		f(&mut sessions)
	}

	/// A thread's spend in dollars over every omp session of its task, read as the files are now (0686); `None` before
	/// its first model call or when the session index can't say.
	pub async fn spend_of(&self, thread: &ThreadRef) -> Option<f64> {
		self.ledger_waiting(thread, true).await.map(|ledger| ledger.spend)
	}

	/// A thread's spend past $50 (0541, PRM-11): omp warned in its pane, and the boss hears it once. A thread found in
	/// herdr already past it was warned before this run.
	pub async fn spend_passed(&self) {
		let c = self.link.ctx();
		if !c.warmed() {
			return;
		}
		for thread in c.threads.all() {
			let Some(spend) = self.ledger_waiting(&thread, false).await.map(|ledger| ledger.spend) else { continue };
			let say = thread.with(|t| {
				let said = *t.spend_said.get_or_insert(spend >= SPEND_WARNED_USD);
				let say = !said && spend >= SPEND_WARNED_USD;
				if say {
					t.spend_said = Some(true);
				}
				say
			});
			if !say {
				continue;
			}
			let line = format!("spend passed {} ({}); omp warned in its pane", money(SPEND_WARNED_USD), money(spend));
			c.events.thread(&thread, &line, ToastPriority::Normal, None).await;
		}
	}

	/// The facts a thread's sidebar tokens are built from.
	fn row_of(&self, thread: &ThreadRef, warmed: bool) -> Row {
		thread.with(|t| Row {
			pane_id: t.pane_id.clone(),
			project: t.project.name.clone(),
			session_path: t.session_path.clone().filter(|_| warmed),
			plan_awaiting: plan_awaiting(t),
			review_ready: t.review.as_ref().is_some_and(|review| review.result.is_some()),
			working: t.state == AgentState::Working,
			awaiting: t.shown == ThreadState::Blocked,
		})
	}

	/// Sidebar tokens per thread pane and project workspace (0402, UX-04, UX-27), each expiring unless renewed (UX-13),
	/// then the HUD (0403).
	pub async fn report(&self) -> Result<()> {
		let c = self.link.ctx();
		let mut workspace_of: HashMap<String, String> = HashMap::new();
		// A project's workspace is the first with its name (as `projectWorkspace` finds it): a thread titled like its
		// project has one of the same label, made later (SVC-14).
		for w in self.workspaces() {
			workspace_of.entry(w.label).or_insert(w.id);
		}
		let mut counts = Counts::new();
		let mut live = HashSet::new();
		for thread in c.threads.all() {
			let row = self.row_of(&thread, c.warmed());
			live.insert(format!("pane:{}", row.pane_id));
			self.report_thread(&c, &thread, &row).await;
			let Some(workspace_id) = workspace_of.get(&row.project) else { continue };
			let at = match counts.iter().position(|(id, _)| id == workspace_id) {
				Some(at) => at,
				None => {
					counts.push((workspace_id.clone(), (0, 0)));
					counts.len() - 1
				}
			};
			counts[at].1.0 += u32::from(row.working);
			counts[at].1.1 += u32::from(row.awaiting);
		}
		// The boss's row is pinned first in the Agents view by this token (0667); its pane is live for `forget_closed`.
		if let Some(boss) = c.boss() {
			let pane_id = boss.with(|boss| boss.pane_id.clone());
			live.insert(format!("pane:{pane_id}"));
			self.report_boss(&c, &pane_id).await;
		}
		self.forget_closed(&live, &mut counts);
		for (workspace_id, (working, awaiting)) in counts {
			self.report_workspace(&c, &workspace_id, working, awaiting).await;
		}
		self.say_notices(&c);
		// One `today` for the HUD and the page (CO-15).
		let today = c.day.today()?;
		self.send_hud(&c, &today);
		c.day.push_built_today(today)
	}

	/// One thread's tokens and state labels.
	async fn report_thread(&self, c: &ServiceContext, thread: &ThreadRef, row: &Row) {
		let look = match &row.session_path {
			Some(path) => Some(self.look_waiting(thread, path).await),
			None => None,
		};
		let brief = look.as_ref().map(|look| &look.brief);
		let model = brief.and_then(|b| b.model.as_deref()).filter(|m| !m.is_empty()).map(short_model);
		let cost = look.as_ref().and_then(|look| look.spend).map(money);
		// "review ready" once hunk's notes are counted (0394); an idle pane with its plan in review waits on him.
		let blocked = brief.and_then(|b| b.question.clone()).unwrap_or_else(|| "awaiting you".to_owned());
		let mut labels = vec![(AgentState::Blocked, blocked)];
		if row.plan_awaiting {
			labels.push((AgentState::Idle, "plan ready".to_owned()));
			labels.push((AgentState::Done, "plan ready".to_owned()));
		} else if row.review_ready {
			labels.push((AgentState::Done, "review ready".to_owned()));
		}
		let value = json!({
			"tokens": {"model": model, "cost": cost},
			"stateLabels": labels.iter().map(|(state, label)| (state.as_str().to_owned(), json!(label))).collect::<serde_json::Map<_, _>>(),
			"ttlMs": METADATA_TTL_MS,
		});
		let meta = PaneMetadata {
			tokens: Some(vec![("model".to_owned(), model), ("cost".to_owned(), cost)]),
			state_labels: Some(labels),
			ttl_ms: Some(METADATA_TTL_MS),
		};
		let send = || c.herdr.report_pane_metadata(&row.pane_id, &meta);
		self.report_once(&format!("pane:{}", row.pane_id), value.to_string(), send).await;
	}

	/// The boss pane's `pin` token (0667), renewed like every sidebar token.
	async fn report_boss(&self, c: &ServiceContext, pane_id: &str) {
		let value = json!({"tokens": {(PIN_TOKEN): PIN_VALUE}, "ttlMs": METADATA_TTL_MS});
		let meta = PaneMetadata {
			tokens: Some(vec![(PIN_TOKEN.to_owned(), Some(PIN_VALUE.to_owned()))]),
			ttl_ms: Some(METADATA_TTL_MS),
			..PaneMetadata::default()
		};
		let send = || c.herdr.report_pane_metadata(pane_id, &meta);
		self.report_once(&format!("pane:{pane_id}"), value.to_string(), send).await;
	}

	/// Drops what was reported of a closed workspace or pane, and counts a workspace still reported with no thread as
	/// zero, so its tokens are cleared once.
	fn forget_closed(&self, live: &HashSet<String>, counts: &mut Counts) {
		let mut st = self.st();
		let State { workspaces, reported, .. } = &mut *st;
		reported.retain(|(key, _)| match key.strip_prefix("ws:") {
			// A closed workspace's tokens went with it: never reported again (CO-13).
			Some(id) if !workspaces.iter().any(|w| w.id == id) => false,
			Some(id) => {
				if !counts.iter().any(|(known, _)| known == id) {
					counts.push((id.to_owned(), (0, 0)));
				}
				true
			}
			None => !key.starts_with("pane:") || live.contains(key),
		});
	}

	/// A project workspace's `running` token: what works and what awaits him there.
	async fn report_workspace(&self, c: &ServiceContext, workspace_id: &str, working: u32, awaiting: u32) {
		let said = [
			if working > 0 { format!("{working} at work") } else { String::new() },
			if awaiting > 0 { format!("{awaiting} awaiting you") } else { String::new() },
		];
		let running = said.iter().filter(|part| !part.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
		let key = format!("ws:{workspace_id}");
		if running.is_empty() {
			// Cleared once, then forgotten: nothing left to renew (CO-13).
			if !self.st().reported.iter().any(|(known, _)| *known == key) {
				return;
			}
			let tokens: Tokens = vec![("running".to_owned(), None)];
			match c.herdr.report_workspace_metadata(workspace_id, &tokens, Some(METADATA_TTL_MS)).await {
				Ok(()) => self.st().reported.retain(|(known, _)| *known != key),
				Err(error) => c.warn(error),
			}
			return;
		}
		let value = json!({"running": running});
		let tokens: Tokens = vec![("running".to_owned(), Some(running))];
		let send = || c.herdr.report_workspace_metadata(workspace_id, &tokens, Some(METADATA_TTL_MS));
		self.report_once(&key, value.to_string(), send).await;
	}

	/// The setup and boss notices, each said once.
	fn say_notices(&self, c: &ServiceContext) {
		let setup = self.st().setup_notice.take();
		if let Some(text) = setup {
			c.events.send(Toast::new(ToastReason::Event, ToastLevel::Info, text).into());
		}
		if let Some(text) = c.panes.boss_notice() {
			c.events.send(Toast::new(ToastReason::Event, ToastLevel::Warning, text).into());
			c.panes.set_boss_notice(None);
		}
	}

	/// Sends a report when it changed, or when the last one is old enough to renew before herdr expires it.
	async fn report_once<F: Future<Output = HerdrResult<()>>>(
		&self,
		key: &str,
		json: String,
		send: impl FnOnce() -> F,
	) {
		let renew = {
			let st = self.st();
			st.reported
				.iter()
				.find(|(known, _)| known == key)
				.is_none_or(|(_, last)| last.json != json || now_ms() - last.at >= METADATA_RENEW_MS)
		};
		if !renew {
			return;
		}
		match send().await {
			Ok(()) => {
				let mut st = self.st();
				let reported = Reported { json, at: now_ms() };
				match st.reported.iter_mut().find(|(known, _)| known == key) {
					Some(slot) => slot.1 = reported,
					None => st.reported.push((key.to_owned(), reported)),
				}
			}
			Err(error) => self.link.ctx().warn(error),
		}
	}

	/// The service stops (UX-13): the sidebar's tokens are cleared when herdr answers; tokens a herdr that doesn't
	/// answer keeps expire with their TTL. The tab bar's figures go with the client shell that draws them.
	pub async fn clear(&self) {
		let herdr = self.link.ctx().herdr.clone();
		// Never through a stopped server: a request would start it again (0431), on the way out.
		let ping = tokio::time::timeout(Duration::from_millis(STOP_PING_MS.unsigned_abs()), herdr.ping()).await;
		if !matches!(ping, Ok(Ok(_))) {
			return;
		}
		let boss = self.link.ctx().boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		let keys: Vec<String> = self.st().reported.iter().map(|(key, _)| key.clone()).collect();
		let boss = &boss;
		let clears = keys.iter().map(|key| {
			let (kind, id) = key.split_once(':').unwrap_or((key, ""));
			let herdr = &herdr;
			async move {
				if kind == "pane" {
					let mut tokens = vec![("model".to_owned(), None), ("cost".to_owned(), None)];
					if boss.as_deref() == Some(id) {
						tokens.push((PIN_TOKEN.to_owned(), None));
					}
					let meta = PaneMetadata { tokens: Some(tokens), ..PaneMetadata::default() };
					let _ = herdr.report_pane_metadata(id, &meta).await;
				} else {
					let tokens: Tokens = vec![("running".to_owned(), None)];
					let _ = herdr.report_workspace_metadata(id, &tokens, None).await;
				}
			}
		});
		let _ = tokio::time::timeout(Duration::from_millis(CLEAR_WAIT_MS), join_all(clears)).await;
	}

	/// The HUD (0403): `today` is the one the caller built, else `today`'s now.
	pub fn hud(&self) -> Result<Hud> {
		Ok(self.hud_for(&self.link.ctx().day.today()?))
	}

	/// The HUD over a `today` the caller built (CO-15).
	pub fn hud_for(&self, today: &Today) -> Hud {
		let c = self.link.ctx();
		let threads = c.threads.all();
		let running = threads.iter().filter(|t| t.with(|t| t.state == AgentState::Working)).count();
		let blocked = threads.iter().filter(|t| t.with(|t| t.shown == ThreadState::Blocked)).count();
		let (accounts, capacity) = c.day.quota_accounts();
		let spend_today = self.spend_today(&c);
		// Today's total goes into the recorded days the 7-day average is the mean of (0668).
		if let Some(spent) = spend_today {
			c.day.record_spend(spent);
		}
		Hud {
			running: u32::try_from(running).unwrap_or(u32::MAX),
			// Waiting on him: herdr's blocked, and a plan waiting in omp's Plan Review (herdr says idle there).
			blocked: u32::try_from(blocked).unwrap_or(u32::MAX),
			// Every row of Awaiting you, the count `today` and the HUD say (0432, 0581).
			awaiting: u32::try_from(today.needs_you.len()).unwrap_or(u32::MAX),
			// Counted once today's sessions were read; until then the HUD says "…".
			spend_today,
			// The subscription windows from `omp usage` (0456), once it answered.
			quota: c.day.quota(),
			spend_avg_7d: c.day.spend_avg_7d(),
			top_thread: top_thread(today),
			accounts,
			capacity,
			checks: Some(c.checks.load()).filter(|load| load.running + load.waiting > 0),
			// today's header says quiet hours (0581).
			quiet: Some(c.day.quiet()),
		}
	}

	/// Today's spend once the sessions were read; a new file the listing found is read in steps off this call, and a
	/// busy index gives the figure last counted.
	fn spend_today(&self, c: &std::sync::Arc<ServiceContext>) -> Option<f64> {
		if !c.warmed() {
			return None;
		}
		let counted = {
			let mut sessions = c.sessions.try_lock().ok();
			let mut tracker = self.spend.try_lock().ok();
			match (sessions.as_deref_mut(), tracker.as_deref_mut()) {
				(Some(sessions), Some(tracker)) => {
					Some((spend_today(tracker, sessions, now_ms()), tracker.is_warming()))
				}
				_ => None,
			}
		};
		let Some((spent, warming)) = counted else { return self.st().last_spend };
		let settle = {
			let mut st = self.st();
			st.last_spend = Some(spent);
			let settle = warming && !st.settling && tokio::runtime::Handle::try_current().is_ok();
			st.settling |= settle;
			settle
		};
		if settle {
			let ctx = c.clone();
			c.calls.spawn(async move {
				{
					let mut tracker = ctx.status.spend.lock().await;
					let mut sessions = ctx.sessions.lock().await;
					spend_settled(&mut tracker, &mut sessions).await;
				}
				ctx.status.st().settling = false;
			});
		}
		Some(spent)
	}

	/// The HUD's figures (0403, 0581): Awaiting you's rows and today's spend, which the client shell's tab bar and the
	/// page say. They go out whenever any of them changed, the quota's included (FE-4).
	fn send_hud(&self, c: &ServiceContext, today: &Today) {
		let hud = self.hud_for(today);
		let json = serde_json::to_string(&hud).unwrap_or_default();
		{
			let mut st = self.st();
			if json == st.hud_sent {
				return;
			}
			st.hud_sent = json;
		}
		c.events.send(ServerMessage::Hud { hud });
	}

	/// The threads closing Jocasta would cut off (0434, 0438), counted apart: awaiting him, the threads among the rows of
	/// Awaiting you the HUD and `today` count (0432: a question, a plan in omp's Plan Review, reviewed work), and working
	/// (`at_work`) among the rest.
	pub fn busy_threads(&self) -> Result<Busy> {
		let c = self.link.ctx();
		let awaited = self.awaited_panes()?;
		let working = c.threads.all().iter().filter(|t| self.at_work(t, &awaited)).count();
		let awaiting = c.threads.pane_ids().iter().filter(|pane_id| awaited.contains(*pane_id)).count();
		Ok(Busy { working, awaiting })
	}

	/// The panes of Awaiting you's rows (0432).
	pub fn awaited_panes(&self) -> Result<HashSet<String>> {
		let today = self.link.ctx().day.today()?;
		Ok(today.needs_you.into_iter().filter_map(|row| row.pane_id).collect())
	}

	/// A thread at work, as the close question counts it and Stop everything saves it for `--restore` (0434, 0549): a
	/// turn runs (herdr's or omp's working: a task, its review or its handoff) or one was started and not yet reported,
	/// and it isn't awaiting him.
	pub fn at_work(&self, thread: &ThreadRef, awaited: &HashSet<String>) -> bool {
		thread.with(|t| {
			!awaited.contains(&t.pane_id) && (t.state == AgentState::Working || t.shown == ThreadState::Working)
		})
	}

	/// A thread's session read as it is now, and its whole cost (0582, SRV-B-18), as the sidebar says it.
	fn look_in(&self, sessions: &mut SessionIndex, thread: &ThreadRef, path: &Path) -> Look {
		let stats = sessions.read(path, false);
		let brief = Brief {
			model: stats.model.clone(),
			question: stats.question.clone(),
			last_line: stats.last_line.clone(),
			context: stats.context,
		};
		Look { brief, spend: self.ledger_in(sessions, thread, false).map(|ledger| ledger.spend) }
	}

	async fn look_waiting(&self, thread: &ThreadRef, path: &Path) -> Look {
		self.sessions(|sessions| self.look_in(sessions, thread, path)).await
	}

	/// A thread as `today` shows it (0409, 0555).
	pub fn day_thread(&self, thread: &ThreadRef) -> DayThread {
		let c = self.link.ctx();
		let path = thread.with(|t| t.session_path.clone()).filter(|_| c.warmed());
		let look = path.and_then(|path| self.try_sessions(|sessions| self.look_in(sessions, thread, &path)));
		// The overlaps as last read; a read over a minute old is started again, never waited for (PERF-1).
		let overlaps: Vec<String> = self.overlaps_known(thread).into_iter().map(|o| o.title).collect();
		// The diff, last check and last write as `ThreadFacts` last read them (0668); a cheap, synchronous read.
		let facts = c.thread_facts.get(&thread.with(|t| t.path.clone()));
		let agent = c.bridge.agent(&thread.with(|t| t.pane_id.clone()));
		thread.with(|t| DayThread {
			pane_id: t.pane_id.clone(),
			title: t.title.clone(),
			project: Some(t.project.name.clone()),
			line: asleep_after(rail_line(t, look.as_ref().map(|look| &look.brief)), &t.pane_id, &c.launched),
			overlaps: (!overlaps.is_empty()).then_some(overlaps),
			state: t.shown,
			changed_at: t.changed_at,
			reviewed_at: t.review.as_ref().and_then(|review| review.result.as_ref()).and_then(|result| result.at),
			cost: look.as_ref().and_then(|look| look.spend),
			context: look.as_ref().and_then(|look| look.brief.context),
			diff: facts.diff,
			check: facts.check,
			last_write_ms: facts.last_write_ms,
			halted: t.stuck.clone(),
			progress: progress_of(agent.as_ref()),
			budget: t.task.as_ref().and_then(thread_budget),
		})
	}

	/// The files a thread changed since it left trunk, committed or not (`git diff --name-only <merge-base>` in its
	/// worktree), read off the event loop (PERF-1): the read already running for the thread, else a new one.
	fn read_files(&self, target: &Target) -> SharedFiles {
		let mut st = self.st();
		if let Some((_, running)) = st.files_reading.get(&target.pane_id) {
			return running.clone();
		}
		self.start_read(&mut st, target)
	}

	/// A new read of a thread's files, which later callers join: a call's answer must see the files as they are when
	/// it asks, not a read the refresh tick started before a change.
	fn read_files_fresh(&self, target: &Target) -> SharedFiles {
		let mut st = self.st();
		self.start_read(&mut st, target)
	}

	fn start_read(&self, st: &mut State, target: &Target) -> SharedFiles {
		st.read_seq += 1;
		let seq = st.read_seq;
		let (link, owned) = (self.link.clone(), target.clone());
		let read: BoxFuture<'static, Files> = async move {
			let c = link.ctx();
			let files = c.status.read_files_now(&c, &owned, seq).await.map_err(|error| error.to_string());
			let mut st = c.status.st();
			if st.files_reading.get(&owned.pane_id).is_some_and(|(running, _)| *running == seq) {
				st.files_reading.remove(&owned.pane_id);
			}
			files
		}
		.boxed();
		let shared = read.shared();
		st.files_reading.insert(target.pane_id.clone(), (seq, shared.clone()));
		shared
	}

	async fn read_files_now(&self, c: &ServiceContext, target: &Target, seq: u64) -> Result<Vec<String>> {
		let trunk = c.places.trunk(&target.project).await?;
		let base = match trunk {
			Some(trunk) => git_out(&target.path, &["merge-base", trunk.as_str(), "HEAD"]).await?,
			None => None,
		};
		let diff = match base.filter(|base| !base.is_empty()) {
			Some(base) => git_out(&target.path, &["diff", "--name-only", base.as_str()]).await?,
			None => None,
		};
		let files: Vec<String> =
			diff.unwrap_or_default().split('\n').filter(|file| !file.is_empty()).map(str::to_owned).collect();
		let mut st = self.st();
		if st.changed_files.get(&target.pane_id).is_none_or(|(_, kept, _)| *kept < seq) {
			st.changed_files.insert(target.pane_id.clone(), (now_ms(), seq, files.clone()));
		}
		Ok(files)
	}

	/// The overlaps `today` shows (0456): from each thread's files as last read; a thread's read over a minute old (or
	/// none yet) starts again in the background and shows on a later look.
	fn overlaps_known(&self, thread: &ThreadRef) -> Vec<Overlap> {
		let c = self.link.ctx();
		let known = |t: &ThreadRef| -> Vec<String> {
			let target = Target::of(t);
			let cached = self.st().changed_files.get(&target.pane_id).cloned();
			let stale = cached.as_ref().is_none_or(|(at, _, _)| *at < now_ms() - OVERLAP_KEPT_MS);
			if stale && !c.stopping() && tokio::runtime::Handle::try_current().is_ok() {
				let read = self.read_files(&target);
				// The stop waits for it, like a call: its git runs in the thread's folder.
				let ctx = c.clone();
				c.calls.spawn(async move {
					if let Err(error) = read.await {
						ctx.warn(error);
					}
				});
			}
			cached.map(|(_, _, files)| files).unwrap_or_default()
		};
		self.overlaps_of(thread, &known)
	}

	/// The project's other open threads that touch the same files as `thread`, with those files (0456): every thread's
	/// files read now, after the call began, all at once and once per call (SVC-4): `reads` holds the call's reads,
	/// shared by all its lines.
	pub async fn overlaps(&self, thread: &ThreadRef, reads: &FileReads) -> Result<Vec<Overlap>> {
		let c = self.link.ctx();
		let project_id = thread.with(|t| t.project.id.clone());
		let project = c.threads.of_project(&project_id);
		let read = |t: &ThreadRef| {
			let target = Target::of(t);
			let found =
				reads.map().entry(target.pane_id.clone()).or_insert_with(|| self.read_files_fresh(&target)).clone();
			async move { found.await.map_err(Error::other) }
		};
		let all = try_join_all(project.iter().map(|t| {
			let pane_id = t.with(|t| t.pane_id.clone());
			let found = read(t);
			async move { Ok::<_, Error>((pane_id, found.await?)) }
		}))
		.await?;
		let all: HashMap<String, Vec<String>> = all.into_iter().collect();
		let files = |t: &ThreadRef| all.get(&t.with(|t| t.pane_id.clone())).cloned().unwrap_or_default();
		Ok(self.overlaps_of(thread, &files))
	}

	/// The overlaps of `thread` with its project's other threads, by each thread's `files`.
	fn overlaps_of(&self, thread: &ThreadRef, files: &dyn Fn(&ThreadRef) -> Vec<String>) -> Vec<Overlap> {
		let mine: HashSet<String> = files(thread).into_iter().collect();
		if mine.is_empty() {
			return Vec::new();
		}
		let project_id = thread.with(|t| t.project.id.clone());
		let mut found = Vec::new();
		for other in self.link.ctx().threads.all() {
			let (title, same_project) = other.with(|t| (t.title.clone(), t.project.id == project_id));
			if other.same(thread) || !same_project {
				continue;
			}
			let shared: Vec<String> = files(&other).into_iter().filter(|file| mine.contains(file)).collect();
			if !shared.is_empty() {
				found.push(Overlap { title, files: shared });
			}
		}
		found
	}

	/// `thread_status` and `jocasta thread status|list`: each thread's line with its report's outcome once it handed one
	/// in for its current task (0539), and under it its ledger line (0538, 0546) once its session has model calls; a
	/// named thread also shows its task file and the whole report. Each thread's files are read once for the whole
	/// answer, all at once (SVC-4).
	pub async fn thread_status(&self, wanted: Option<&str>, project: Option<&Project>) -> Result<String> {
		let c = self.link.ctx();
		let reads = FileReads::default();
		if let Some(wanted) = wanted.filter(|wanted| !wanted.is_empty()) {
			let thread = c.threads.find(wanted)?;
			let report = thread.with(|t| t.task.as_ref().and_then(|task| task.report.clone()));
			let task = thread.with(|t| t.task.as_ref().map(|task| (task.project_id.clone(), task.id.clone())));
			let spent = self.ledger_said(&thread).await;
			let mut lines = vec![self.thread_line(&thread, &reads).await?];
			lines.extend(spent);
			if let Some((project_id, id)) = task {
				lines.push(format!("task file: {}", task_file(&project_id, &id).display()));
			}
			let said = match report {
				Some(report) => {
					let earlier = if thread.with(|t| c.review.report_in(t)) { "" } else { " (an earlier task's)" };
					format!("{}{earlier}", report_line(&report.value))
				}
				None => "none yet".to_owned(),
			};
			lines.push(format!("report: {said}"));
			return Ok(lines.join("\n"));
		}
		let threads: Vec<ThreadRef> = c
			.threads
			.all()
			.into_iter()
			.filter(|t| project.is_none_or(|project| t.with(|t| t.project.id == project.id)))
			.collect();
		if threads.is_empty() {
			return Ok(
				project.map_or_else(|| "No threads.".to_owned(), |project| format!("No threads in {}.", project.name))
			);
		}
		let lines = try_join_all(threads.iter().map(|t| self.status_line(t, &reads))).await?;
		Ok(lines.join("\n"))
	}

	/// One thread's line in the list: its report's outcome and its ledger line under it.
	async fn status_line(&self, t: &ThreadRef, reads: &FileReads) -> Result<String> {
		let c = self.link.ctx();
		let outcome = t.with(|t| t.task.as_ref().and_then(|task| task.report.as_ref()).map(|r| r.value.outcome));
		let report = match outcome {
			Some(outcome) if t.with(|task| c.review.report_in(task)) => {
				let name = serde_json::to_value(outcome).ok().and_then(|v| v.as_str().map(str::to_owned));
				format!(" · reported {}", name.unwrap_or_default())
			}
			_ => String::new(),
		};
		let spent = self.ledger_said(t).await.map(|spent| format!("\n  {spent}")).unwrap_or_default();
		Ok(format!("{}{report}{spent}", self.thread_line(t, reads).await?))
	}

	/// A thread's ledger line (0538, 0546), read as its sessions are now; `None` before its first model call.
	async fn ledger_said(&self, thread: &ThreadRef) -> Option<String> {
		let ledger = self.ledger_waiting(thread, true).await?;
		(ledger.requests > 0).then(|| ledger_line(&ledger))
	}

	/// A thread's ledger (0538, 0546) over every omp session of it: the ones in its worktree's session folder that began
	/// since its task did ("Approve and execute" starts a new one), or its current one alone without a task file. Parsed
	/// again only when one of those files changed size (SVC-8). The refresh tick's callers look at most every few
	/// seconds; `fresh` (a tool's answer, the task file) and a new current session always look. `None` when the session
	/// index is busy being read.
	pub fn ledger_of(&self, thread: &ThreadRef, fresh: bool) -> Option<ThreadLedger> {
		self.try_sessions(|sessions| self.ledger_in(sessions, thread, fresh)).flatten()
	}

	async fn ledger_waiting(&self, thread: &ThreadRef, fresh: bool) -> Option<ThreadLedger> {
		self.sessions(|sessions| self.ledger_in(sessions, thread, fresh)).await
	}

	fn ledger_in(&self, sessions: &mut SessionIndex, thread: &ThreadRef, fresh: bool) -> Option<ThreadLedger> {
		let (pane_id, session_path, created_at) =
			thread.with(|t| (t.pane_id.clone(), t.session_path.clone(), t.task.as_ref().map(|task| task.created_at)));
		let session_path = session_path?;
		let kept = self.st().ledgers.get(&pane_id).cloned();
		let now = now_ms();
		if let Some(kept) = &kept
			&& !fresh
			&& kept.path == session_path
			&& now - kept.at < LEDGER_KEPT_MS
		{
			return kept.ledger.clone();
		}
		let current = sessions.moved(&session_path);
		let files = match created_at {
			Some(created_at) => sessions.sessions_since(&current, created_at),
			None => vec![current],
		};
		let sizes = files
			.iter()
			.map(|file| match std::fs::metadata(file) {
				Ok(stat) => format!("{}|{}", file.display(), stat.len()),
				Err(_) => format!("{}|gone", file.display()),
			})
			.collect::<Vec<_>>()
			.join("\n");
		if let Some(mut kept) = kept.filter(|kept| kept.sizes == sizes) {
			kept.at = now;
			kept.path = session_path;
			let ledger = kept.ledger.clone();
			self.st().ledgers.insert(pane_id, kept);
			return ledger;
		}
		let ledger = thread_ledger(&files);
		self.st().ledgers.insert(pane_id, LedgerKept { at: now, path: session_path, sizes, ledger: ledger.clone() });
		ledger
	}

	/// `projects`: each project with its threads' lines and its leftover worktrees (0456).
	pub async fn projects_text(&self) -> Result<String> {
		let c = self.link.ctx();
		c.places.sync().await;
		let projects = list_projects(&c.catalog.lock().unwrap_or_else(PoisonError::into_inner), None)?;
		if projects.is_empty() {
			return Ok("No projects: add folders to the watched roots in projects.yaml.".to_owned());
		}
		let leftovers = self.leftovers().await?;
		let reads = FileReads::default();
		let blocks = try_join_all(projects.iter().map(|p| self.project_block(p, &leftovers, &reads))).await?;
		Ok(blocks.join("\n"))
	}

	async fn project_block(&self, p: &Project, leftovers: &[Leftover], reads: &FileReads) -> Result<String> {
		let threads = self.link.ctx().threads.of_project(&p.id);
		let git = if p.git { "" } else { ", not git" };
		let mut lines = vec![format!("{} ({}){git}: {}", p.name, p.id, p.main_path.display())];
		for line in try_join_all(threads.iter().map(|t| self.thread_line(t, reads))).await? {
			lines.push(format!("  {line}"));
		}
		for l in leftovers.iter().filter(|l| l.project == p.name) {
			let changed = if l.dirty == 0 { "clean".to_owned() } else { format!("{} changed", l.dirty) };
			lines.push(format!(
				"  left on disk: {} [{}] {} ahead, {changed}",
				l.folder.display(),
				l.branch.as_deref().unwrap_or("detached"),
				l.ahead
			));
		}
		Ok(lines.join("\n"))
	}

	/// A thread's line in `thread_status` and `projects`; `reads` is the answer's one read of each thread's files.
	async fn thread_line(&self, t: &ThreadRef, reads: &FileReads) -> Result<String> {
		let day = self.day_thread(t);
		let (state, subject, branch, awaiting) =
			t.with(|t| (thread_state(t), subject(t), t.branch.clone(), plan_awaiting(t)));
		let line = if !day.line.is_empty() && !awaiting && day.line != state {
			format!(" — {}", day.line)
		} else {
			String::new()
		};
		let overlaps = overlap_said(&self.overlaps(t, reads).await?);
		let overlaps = if overlaps.is_empty() { String::new() } else { format!(" · overlaps {overlaps}") };
		Ok(format!("{subject} [{branch}] {state}{line}{overlaps}"))
	}

	/// Leftover worktrees (0456): checkouts in a project's `<project>.wt/` folder that no open thread holds, read off the
	/// event loop. Never removed here.
	pub async fn leftovers(&self) -> Result<Vec<Leftover>> {
		let c = self.link.ctx();
		let open: Vec<PathBuf> = c.threads.all().iter().map(|t| t.with(|t| t.path.clone())).collect();
		let listed = c.places.listed();
		let ctx = c.clone();
		find_leftovers(&listed, &open, move |project| {
			let ctx = ctx.clone();
			Box::pin(async move {
				ctx.places.trunk(project).await.unwrap_or_else(|error| {
					ctx.warn(error);
					None
				})
			})
		})
		.await
	}

	/// Brings the thread's task file up to date (0540): its session, state and spend (its ledger's, over all its
	/// sessions, 0538) with whatever else changed on the record; written only when the text changed. A failed write is
	/// logged, never the thread's problem.
	pub fn write_task(&self, thread: &ThreadRef) {
		let c = self.link.ctx();
		if !c.threads.current(thread) || thread.with(|t| t.task.is_none()) {
			return;
		}
		let ledger = if c.warmed() { self.ledger_of(thread, true) } else { None };
		let _writing = self.task_writing();
		// The finish drops the pane before it closes the record under this lock: a write that got here after the close
		// would write the closed record's state back.
		if !c.threads.current(thread) {
			return;
		}
		let saved = thread.with(|t| {
			let state = thread_state(t);
			let reviewed = t.review.as_ref().and_then(|review| review.result.as_ref()).is_some_and(|r| r.at.is_some());
			let session = t.session_path.as_ref().map(|path| path.to_string_lossy().into_owned());
			let label = subject(t);
			let task = t.task.as_mut()?;
			task.session = session.or_else(|| task.session.clone());
			task.spend = ledger.map(|ledger| ledger.spend).or(task.spend);
			task.state = if state == "done" && reviewed { "done and reviewed" } else { state }.to_owned();
			Some((task.clone(), label))
		});
		let Some((task, label)) = saved else { return };
		if let Err(error) = save_task(&task) {
			c.warn(Error::other(format!("{label}'s task file wasn't written: {error}")));
		}
	}
}

#[cfg(test)]
#[path = "status_tests.rs"]
pub(crate) mod tests;
