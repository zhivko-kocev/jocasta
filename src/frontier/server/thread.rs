//! A thread as the service knows it (0412): a herdr worktree workspace grouped under its project, with its step in the
//! task cycle, its last review and its plan; and the registry of the open ones, by pane.
//!
//! A [`Thread`] is data. The service shares one as a [`ThreadRef`] (`Shared<Thread>`): every part that holds it sees
//! the others' changes, and [`ThreadRegistry::current`] compares two by identity, as the TypeScript's `===` does.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::frontier::protocol::ThreadState;
use crate::frontier::server::checks::CheckSpec;
use crate::frontier::server::context::Shared;
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::hunk::ReviewProof;
use crate::frontier::server::sessions::TestsResult;
use crate::frontier::server::task_file::{CheckExit, TaskRecord};
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
use crate::shared::proctree::{OmpProcesses, ProcessId};
use crate::shared::projects::Project;

/// After a task end (0394, 0400): the review turn runs, then the `/handoff` turn; neither run's end is a task end.
/// `Finish`: the thread pushes and opens its PR (0426).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
	Task,
	Review,
	Handoff,
	Finish,
}

impl Phase {
	/// The phase's name as the log and `thread.json` spell it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Task => "task",
			Self::Review => "review",
			Self::Handoff => "handoff",
			Self::Finish => "finish",
		}
	}

	/// The moves between phases (SVC-15): a task's review, its handoff and Finish's PR turn, each back to the task. A
	/// task with no review to run goes straight to its handoff; Finish takes a thread whose handoff is written (or held
	/// past its time) to a new review (main moved) or its PR turn.
	fn moves(self) -> &'static [Phase] {
		match self {
			Self::Task => &[Self::Review, Self::Handoff, Self::Finish],
			Self::Review => &[Self::Handoff, Self::Task],
			Self::Handoff => &[Self::Task, Self::Review, Self::Finish],
			Self::Finish => &[Self::Task],
		}
	}
}

/// The last review's outcome (0425, 0453).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewResult {
	/// How many notes it left in hunk; `None` when hunk couldn't count them.
	pub notes: Option<usize>,
	/// How many of them the reviewer marked P0 or P1 (0453).
	pub blocking: Option<usize>,
	/// Its checked test result (0425, 0453).
	pub tests: TestsResult,
	/// Its diff size and test edits (0453).
	pub proof: Option<ReviewProof>,
	/// Its checks with their exit codes: the project's check, then each `done_when` command (0540).
	pub checks: Vec<CheckExit>,
	/// When it became ready; cleared once the thread works again (0456 "Awaiting you").
	pub at: Option<i64>,
}

/// The review running now: HEAD, the time it was asked for, what it covers and its range, the checks the runner ran
/// and why the project's own couldn't run (0559).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRunning {
	pub head: Option<String>,
	pub at: i64,
	pub commits: String,
	pub range: String,
	pub checks: Option<Vec<CheckSpec>>,
	pub skipped: Option<String>,
}

/// A thread's review step (0406): the hunk pane beside it, the review running now and the last one's outcome.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThreadReview {
	/// The hunk pane beside it (0406), once a task ended.
	pub pane: Option<String>,
	pub running: Option<ReviewRunning>,
	/// The last review's outcome, once it is ready; none while a new one runs.
	pub result: Option<ReviewResult>,
}

/// The last plan omp proposed, with its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanProposed {
	pub title: String,
	pub file: Option<PathBuf>,
}

/// A thread's plan in omp's Plan Review (0405, 0471, 0494, 0540).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadPlan {
	/// The title of a plan waiting for his approval in omp's Plan Review (herdr reports the pane idle meanwhile).
	pub awaiting: Option<String>,
	/// The last plan omp proposed, with its file, until the session shows it approved (he acted on it in omp's Plan
	/// Review, and the session is out of plan mode): then the task file records it (0540). "Approve and execute" goes on
	/// in a new session, which has no plan of its own.
	pub proposed: Option<PlanProposed>,
	/// Its omp was reopened (found in herdr, or started again in a pane herdr restored): omp shows no Plan Review for a
	/// plan resumed from its session, so `/plan-review` is typed once to show it again (0494). Cleared once typed, or
	/// once the picker is seen already open.
	pub reopened: bool,
	/// How often he had sent its plan back when last looked at (0471).
	pub sent_back: Option<usize>,
}

/// Its pane's omp as read for the pane's terminal `terminal` (0490), in a pane Jocasta started: `shim` the process
/// herdr started for the pane, `processes` it and what ran under it. Read again when the terminal changes, and on each
/// tick until the shim has a child (the real omp). What is left of it ends when the pane goes in herdr or herdr
/// restores it as a shell, and when Jocasta stops everything.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadOmp {
	pub terminal: Option<String>,
	pub shim: Option<ProcessId>,
	pub processes: Option<OmpProcesses>,
}

/// A thread the service knows: a herdr worktree workspace grouped under its project (0412).
#[derive(Debug, Clone)]
pub struct Thread {
	pub pane_id: String,
	pub workspace_id: String,
	pub project: Project,
	pub title: String,
	pub branch: String,
	/// The worktree.
	pub path: PathBuf,
	/// herdr's state of the pane, or omp's own while its host answers (`host`); what `today` shows is `shown`.
	pub state: AgentState,
	/// omp's own state from its host registry entry (0547, 0561): working (a turn runs), blocked (a dialog of omp's
	/// waits) or idle; `None` while its omp has no entry that answers (unknown, never idle: herdr's events and the
	/// session file decide then, as before).
	pub host: Option<AgentState>,
	/// What `today` shows: working, waiting on him, done with a task, or idle.
	pub shown: ThreadState,
	pub changed_at: i64,
	/// HEAD at the last task end (or start): new commits since mean the task ended (0400).
	pub head: Option<String>,
	/// Its step in the task cycle; changed only through [`set_phase`].
	pub phase: Phase,
	/// The step its omp was cut in by a restart (0582, SVC-6), never `Task`: once the new omp is idle, a cut review or
	/// `/handoff` runs again, and a cut PR turn goes back to `Task`.
	pub cut: Option<Phase>,
	/// Whether the boss heard its spend pass $50 (0541, PRM-11); `None` until its ledger is first read.
	pub spend_said: Option<bool>,
	/// Its review step (0406, 0453).
	pub review: Option<ThreadReview>,
	/// When the boss last typed into its pane with `thread_send`: a run after review it started is his relayed word.
	pub sent_at: Option<i64>,
	/// `thread_finish` merged main into the branch: the review now running decides the merge (0454).
	pub merge_after_review: bool,
	/// Found in herdr rather than started here: once omp is idle, unreviewed commits get their review (0394).
	pub catch_up: bool,
	/// Its plan in omp's Plan Review (0405, 0494, 0540).
	pub plan: Option<ThreadPlan>,
	pub session_path: Option<PathBuf>,
	/// When `/handoff` was sent (0400): a handoff entry in the session at or after it means the handoff is done.
	pub handoff_at: Option<i64>,
	/// When herdr last pushed this pane's state (the refresh tick's reconcile never overrides a newer one).
	pub event_at: Option<i64>,
	pub omp: Option<ThreadOmp>,
	/// Its task file's record (0540): none for a thread found in herdr that Jocasta has no record of.
	pub task: Option<TaskRecord>,
	/// This task's end already steered it once to hand in its report (0539).
	pub report_asked: bool,
	/// Why it is stopped until he answers (0677, 0686): `stuck: <gate> failed 3×` or `over budget: $<spent> of
	/// $<budget>`; the row shows it. Cleared by his `thread_send`, a raised budget or a new task.
	pub stuck: Option<String>,
	/// The over-budget question is open (0686): it is asked once.
	pub budget_asked: bool,
	/// Its review is ready and its handoff is written: it sleeps once its omp is idle (0676). Cleared when it works or
	/// wakes again.
	pub sleep_when_idle: bool,
}

/// A thread shared between the parts: see the module's note.
pub type ThreadRef = Shared<Thread>;

impl Thread {
	/// A thread with what every one has and the rest unset: in the task phase, idle, its state unknown, no worktree path
	/// yet (`path`).
	pub fn new(
		pane_id: impl Into<String>,
		workspace_id: impl Into<String>,
		project: Project,
		title: impl Into<String>,
		branch: impl Into<String>,
	) -> Self {
		Self {
			pane_id: pane_id.into(),
			workspace_id: workspace_id.into(),
			project,
			title: title.into(),
			branch: branch.into(),
			path: PathBuf::new(),
			state: AgentState::Unknown,
			host: None,
			shown: ThreadState::Idle,
			changed_at: chrono::Utc::now().timestamp_millis(),
			head: None,
			phase: Phase::Task,
			cut: None,
			spend_said: None,
			review: None,
			sent_at: None,
			merge_after_review: false,
			catch_up: false,
			plan: None,
			session_path: None,
			handoff_at: None,
			event_at: None,
			omp: None,
			task: None,
			report_asked: false,
			stuck: None,
			budget_asked: false,
			sleep_when_idle: false,
		}
	}
}

/// An agent state at rest: herdr's `done` is idle, not yet looked at.
pub fn is_idle(state: AgentState) -> bool {
	matches!(state, AgentState::Idle | AgentState::Done)
}

/// A state with herdr's `done` read as idle, for comparing two readings.
pub fn settled(state: AgentState) -> AgentState {
	if state == AgentState::Done { AgentState::Idle } else { state }
}

/// A thread as events, toasts and replies name it (UX-21): `<project>/<title>`.
pub fn subject(thread: &Thread) -> String {
	format!("{}/{}", thread.project.name, thread.title)
}

/// A thread's state as `thread_status` says it.
pub fn thread_state(t: &Thread) -> &'static str {
	// herdr's `done` is "idle, not yet looked at", not a finished task (as in `bossState`); a finished task is `shown`.
	if t.shown == ThreadState::Done {
		return "done";
	}
	if t.plan.as_ref().is_some_and(|plan| plan.awaiting.as_deref().is_some_and(|title| !title.is_empty())) {
		return "plan ready";
	}
	if t.state == AgentState::Blocked {
		return "awaiting you";
	}
	if t.state == AgentState::Done { "idle" } else { t.state.as_str() }
}

/// The thread's plan record, made when it has none yet.
pub fn plan_of(thread: &mut Thread) -> &mut ThreadPlan {
	thread.plan.get_or_insert_with(ThreadPlan::default)
}

/// Moves the thread to phase `to` (SVC-15): a move [`Phase::moves`] holds is made and logged with `why`; any other is
/// refused and logged as a warning. Staying in the phase it is in is no move. Whether the thread is in `to` now.
pub fn set_phase(thread: &mut Thread, to: Phase, why: &str) -> bool {
	let from = thread.phase;
	if from == to {
		return true;
	}
	let (from, to_name) = (from.as_str(), to.as_str());
	if !thread.phase.moves().contains(&to) {
		log(LogLevel::Warn, "phase", &format!("{}: {from} → {to_name} refused ({why})", subject(thread)), None);
		return false;
	}
	thread.phase = to;
	log(LogLevel::Info, "phase", &format!("{}: {from} → {to_name} ({why})", subject(thread)), None);
	true
}

/// Sets what `today` shows for the thread, and when it changed.
pub fn show(thread: &mut Thread, shown: ThreadState) {
	thread.shown = shown;
	thread.changed_at = chrono::Utc::now().timestamp_millis();
}

/// The open threads by pane id, in the order they were added. Every method takes `&self`: the list sits behind a lock
/// that is never held across an `.await` and never while a thread's own lock is taken for longer than a read.
#[derive(Default)]
pub struct ThreadRegistry {
	by_pane: Mutex<Vec<(String, ThreadRef)>>,
}

impl ThreadRegistry {
	pub fn new() -> Self {
		Self::default()
	}

	fn panes(&self) -> MutexGuard<'_, Vec<(String, ThreadRef)>> {
		self.by_pane.lock().unwrap_or_else(PoisonError::into_inner)
	}

	pub fn get(&self, pane_id: &str) -> Option<ThreadRef> {
		self.panes().iter().find(|(key, _)| key == pane_id).map(|(_, thread)| thread.clone())
	}

	pub fn has(&self, pane_id: &str) -> bool {
		self.panes().iter().any(|(key, _)| key == pane_id)
	}

	/// Registers `thread` under its pane; a thread already there is replaced in its place.
	pub fn add(&self, thread: ThreadRef) {
		let pane_id = thread.with(|t| t.pane_id.clone());
		let mut panes = self.panes();
		match panes.iter_mut().find(|(key, _)| *key == pane_id) {
			Some(slot) => slot.1 = thread,
			None => panes.push((pane_id, thread)),
		}
	}

	pub fn drop_pane(&self, pane_id: &str) {
		self.panes().retain(|(key, _)| key != pane_id);
	}

	/// Whether `thread` is the one open in its pane now: one closed, finished or relaunched meanwhile isn't.
	pub fn current(&self, thread: &ThreadRef) -> bool {
		let pane_id = thread.with(|t| t.pane_id.clone());
		self.get(&pane_id).is_some_and(|open| open.same(thread))
	}

	/// Whether `thread` is still open, in whatever pane it runs now.
	pub fn includes(&self, thread: &ThreadRef) -> bool {
		self.panes().iter().any(|(_, open)| open.same(thread))
	}

	/// The open threads as they are now, a copy. (The TypeScript's live `values()` is this too: iterate a copy, and
	/// check `current` where a thread dropped meanwhile matters.)
	pub fn all(&self) -> Vec<ThreadRef> {
		self.panes().iter().map(|(_, thread)| thread.clone()).collect()
	}

	/// The open threads with their pane ids, a copy.
	pub fn entries(&self) -> Vec<(String, ThreadRef)> {
		self.panes().clone()
	}

	pub fn pane_ids(&self) -> Vec<String> {
		self.panes().iter().map(|(key, _)| key.clone()).collect()
	}

	/// The open threads of a project.
	pub fn of_project(&self, project_id: &str) -> Vec<ThreadRef> {
		self.all().into_iter().filter(|t| t.with(|t| t.project.id == project_id)).collect()
	}

	/// A thread by pane id, title or branch; unknown ones are a refusal.
	pub fn find(&self, wanted: &str) -> Result<ThreadRef> {
		let key = wanted.trim().to_lowercase();
		let threads = self.all();
		self.get(wanted)
			.or_else(|| threads.iter().find(|t| t.with(|t| t.title.to_lowercase() == key)).cloned())
			.or_else(|| threads.iter().find(|t| t.with(|t| t.branch.to_lowercase() == key)).cloned())
			.ok_or_else(|| Error::user(format!("no thread {wanted}.")))
	}
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
