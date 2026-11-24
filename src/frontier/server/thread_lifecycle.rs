//! A thread's states as herdr and omp report them (0405, 0407, 0547, CR-09): each pane's changes handled in order, a
//! run's end turned into a task end, a review or handoff step, a plan awaiting him (0494) or a turn; a pane closed,
//! relaunched (0521, 0549) or committed to by hand (0471).
//!
//! A pane's changes wait in a queue that one task drains (the TypeScript's promise chain per pane); the queues sit behind
//! one lock that is never held across an `.await` and never while a part is called.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::frontier::protocol::{ServerMessage, ThreadState, ToastLevel, ToastPriority, ToastReason};
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::events::Holds;
use crate::frontier::server::herdr::{AgentState, PaneInfo};
use crate::frontier::server::pane_launch::Respawn;
use crate::frontier::server::sessions::SessionStats;
use crate::frontier::server::thread::{
	Phase, PlanProposed, ThreadRef, is_idle, plan_of, set_phase, settled, show, subject,
};
use crate::frontier::server::wording::{LESSON_OFFER, PLAN_REVIEW_COMMAND, REPORT_STEER, quoted};
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};

/// How long a plan-mode turn's end waits for the session to show a proposed plan: omp writes the `propose` result
/// through a buffered writer, so herdr's idle event can come before it is in the file.
const PLAN_SETTLE_MS: i64 = 2_000;
/// How often a plan-mode turn's end reads the session again meanwhile.
const PLAN_POLL_MS: u64 = 100;
/// After a line goes into the boss pane, its idle pane state is herdr's lag, not the end of the turn it started.
const BOSS_PROMPT_GRACE_MS: i64 = 10_000;

/// Where a state change came from: herdr's own event, omp's host registry (both pushed: the refresh tick's reconcile
/// never overrides them), or a read of herdr's pane list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSource {
	Herdr,
	Host,
	Reconcile,
}

/// What a thread that is waiting on him waits on (`awaiting_him`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Awaiting {
	Question,
	Plan,
}

/// Whether a thread's plan still waits on him, as far as its own record says; `Read` needs the session's word.
enum Waiting {
	No,
	Yes,
	Read(std::path::PathBuf),
}

/// A title that counts (the TypeScript's truthiness): set and not empty.
fn named(title: &Option<String>) -> bool {
	title.as_deref().is_some_and(|title| !title.is_empty())
}

#[derive(Default)]
struct Queues {
	/// Each pane's states still to be handled, in the order herdr sent them; a pane is here from its first change until
	/// the last one was handled.
	by_pane: HashMap<String, VecDeque<AgentState>>,
	/// Panes whose plan was found while a state change of theirs was handled, with whether the boss hears it: looked at
	/// again once the last change is handled, since that change may have read the session before the plan was in it.
	plan_rechecks: HashMap<String, bool>,
}

pub struct ThreadLifecycle {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	queues: Mutex<Queues>,
	/// The tasks that drain the queues, for `settled`.
	chains: crate::frontier::server::context::Calls,
}

impl ThreadLifecycle {
	pub fn new(link: Link) -> Self {
		Self { link, queues: Mutex::new(Queues::default()), chains: Default::default() }
	}

	fn queues(&self) -> MutexGuard<'_, Queues> {
		self.queues.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Whether a state change of the pane is still being handled.
	pub fn busy(&self, pane_id: &str) -> bool {
		self.queues().by_pane.contains_key(pane_id)
	}

	/// While a state change of the pane is being handled, notes its plan to be looked at again when the last one ends
	/// (said if any finder asked for it); whether it was noted. One lock with `drain`'s end, so no note is left behind.
	fn plan_after_changes(&self, pane_id: &str, announce: bool) -> bool {
		let mut queues = self.queues();
		if !queues.by_pane.contains_key(pane_id) {
			return false;
		}
		*queues.plan_rechecks.entry(pane_id.to_owned()).or_default() |= announce;
		true
	}

	/// A plan noted by [`plan_after_changes`](Self::plan_after_changes), read from the session as the file is now, and
	/// in a reopened omp its Plan Review shown again, as the refresh tick does after finding one (0494).
	async fn recheck_plan(&self, pane_id: &str, announce: bool) -> Result<()> {
		let ctx = self.link.ctx();
		let Some(thread) = ctx.threads.get(pane_id) else { return Ok(()) };
		let Some(path) = thread.with(|t| t.session_path.clone()) else { return Ok(()) };
		let plan = ctx.sessions.lock().await.read(&path, true).plan_awaiting.clone();
		let Some(plan) = plan.filter(|plan| !plan.is_empty()) else { return Ok(()) };
		self.plan_found(&thread, &plan, announce).await?;
		if thread.with(|t| t.plan.as_ref().is_some_and(|plan| plan.reopened && named(&plan.awaiting))) {
			self.reopen_plan_review(&thread).await?;
		}
		Ok(())
	}

	/// Waits for every state change still being handled (`stop`).
	pub async fn settled(&self) {
		self.chains.settled().await;
	}

	/// Whether every state change queued so far was handled: a pane's queue goes once its last one's handling ends
	/// (tests).
	#[cfg(test)]
	pub(crate) fn states_settled(&self) -> bool {
		self.queues().by_pane.is_empty()
	}

	/// A pane's state changed: handled after the pane's earlier changes (CR-09).
	pub fn state_changed(&self, pane_id: &str, state: AgentState, source: StateSource) {
		let ctx = self.link.ctx();
		if source != StateSource::Reconcile {
			let now = now_ms();
			if let Some(boss) = ctx.boss() {
				boss.with(|boss| {
					if boss.pane_id == pane_id {
						boss.event_at = Some(now);
					}
				});
			}
			if let Some(thread) = ctx.threads.get(pane_id) {
				thread.with(|thread| thread.event_at = Some(now));
			}
		}
		let first = {
			let mut queues = self.queues();
			match queues.by_pane.get_mut(pane_id) {
				Some(queue) => {
					queue.push_back(state);
					false
				}
				None => {
					queues.by_pane.insert(pane_id.to_owned(), VecDeque::from([state]));
					true
				}
			}
		};
		if first {
			let pane_id = pane_id.to_owned();
			self.chains.spawn(async move { ctx.lifecycle.drain(&ctx, &pane_id).await });
		}
	}

	/// Handles a pane's queued states one after another until none is left; an error is a warning, and the next goes on.
	async fn drain(&self, ctx: &Arc<ServiceContext>, pane_id: &str) {
		loop {
			let (next, recheck) = {
				let mut queues = self.queues();
				let next = queues.by_pane.get_mut(pane_id).and_then(VecDeque::pop_front);
				let mut recheck = None;
				if next.is_none() {
					queues.by_pane.remove(pane_id);
					recheck = queues.plan_rechecks.remove(pane_id);
				}
				(next, recheck)
			};
			let Some(state) = next else {
				if let Some(announce) = recheck
					&& let Err(error) = self.recheck_plan(pane_id, announce).await
				{
					ctx.warn(error);
				}
				return;
			};
			if let Err(error) = self.on_state(pane_id, state).await {
				ctx.warn(error);
			}
		}
	}

	/// herdr pushed a pane's agent state: a thread whose omp host answers keeps omp's own state (0547, 0561); herdr
	/// saying no agent runs there any more counts (omp's entry goes with its process), and so does every boss change.
	pub fn herdr_state(&self, pane_id: &str, state: AgentState) {
		if let Some(thread) = self.link.ctx().threads.get(pane_id) {
			let kept = thread.with(|thread| {
				if thread.host.is_none() {
					return false;
				}
				if state != AgentState::Unknown {
					return true;
				}
				thread.host = None;
				false
			});
			if kept {
				return;
			}
		}
		self.state_changed(pane_id, state, StateSource::Herdr);
	}

	/// herdr's pane list read at `read_at` against what its events said (CR-09): a pane whose state differs, with no
	/// event since that read, changes as if herdr had pushed it. The boss pane just given a line is left alone a moment:
	/// herdr shows it idle until omp starts on the line. A handoff that ended without herdr or omp's host ever reporting
	/// it as a turn (F-01) ends here, for a thread whose host answers too: its session file has the handoff's entry.
	pub async fn reconcile(&self, by_id: &HashMap<String, PaneInfo>, read_at: i64) -> Result<()> {
		let ctx = self.link.ctx();
		if let Some(boss) = ctx.boss() {
			let boss_id = boss.with(|boss| boss.pane_id.clone());
			if let Some(pane) = by_id.get(&boss_id) {
				let differs = boss.with(|boss| {
					settled(pane.agent_state) != settled(boss.state)
						&& boss.event_at.unwrap_or(0) < read_at
						&& boss.prompted_at.unwrap_or(0) < read_at - BOSS_PROMPT_GRACE_MS
				});
				if differs {
					self.state_changed(&boss_id, pane.agent_state, StateSource::Reconcile);
				}
			}
		}
		for thread in ctx.threads.all() {
			let (pane_id, event_at, host, state, phase) =
				thread.with(|t| (t.pane_id.clone(), t.event_at, t.host.is_some(), t.state, t.phase));
			let Some(pane) = by_id.get(&pane_id) else { continue };
			if event_at.unwrap_or(0) >= read_at {
				continue;
			}
			// omp's own state, while its host answers, isn't overridden by herdr's (0547).
			if !host && settled(pane.agent_state) != settled(state) {
				self.state_changed(&pane_id, pane.agent_state, StateSource::Reconcile);
			} else if phase == Phase::Handoff && settled(state) == AgentState::Idle && self.handed_off(&thread).await {
				self.next_task(&thread).await?;
			}
		}
		Ok(())
	}

	/// Whether the `/handoff` asked for at `thread.handoff_at` is in the thread's session (F-01).
	pub async fn handed_off(&self, thread: &ThreadRef) -> bool {
		let (path, asked) = thread.with(|t| (t.session_path.clone(), t.handoff_at));
		let Some(path) = path else { return false };
		let at = self.link.ctx().sessions.lock().await.read(&path, false).handoff_at;
		at.is_some_and(|at| at >= asked.unwrap_or(0))
	}

	/// The handoff is done (F-01): what runs from here is the next task, whose commits count from HEAD now.
	async fn next_task(&self, thread: &ThreadRef) -> Result<()> {
		thread.with(|t| set_phase(t, Phase::Task, "its handoff is written"));
		let path = thread.with(|t| t.path.clone());
		let head = git_out(&path, &["rev-parse", "HEAD"]).await?;
		thread.with(|t| t.head = head);
		Ok(())
	}

	/// A watched pane's agent state changed (in order per pane, `state_changed`).
	async fn on_state(&self, pane_id: &str, state: AgentState) -> Result<()> {
		let ctx = self.link.ctx();
		if ctx.stopping() {
			return Ok(());
		}
		if let Some(boss) = ctx.boss()
			&& boss.with(|boss| boss.pane_id == pane_id)
		{
			ctx.events.boss_changed(state).await;
			return Ok(());
		}
		let Some(thread) = ctx.threads.get(pane_id) else { return Ok(()) };
		let was = thread.with(|t| (t.state != state).then_some(t.state));
		let Some(was) = was else { return Ok(()) };
		let stats = self.read_new_state(&thread, pane_id, state).await?;
		// herdr never reports `/handoff` as a turn (F-01): once its entry is in the session, this run is the next task,
		// whose commits count from here.
		if state == AgentState::Working && thread.with(|t| t.phase == Phase::Handoff) && self.handed_off(&thread).await
		{
			self.next_task(&thread).await?;
		}
		if let Some(stats) = &stats {
			self.plan_sent_back(&thread, stats).await;
		}
		let unblocked = thread.with(|t| {
			let awaiting = t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting));
			was == AgentState::Blocked || (awaiting && !is_idle(state))
		});
		if unblocked {
			ctx.day.thread_unblocked(pane_id)?;
			thread.with(|t| {
				if let Some(plan) = &mut t.plan {
					plan.awaiting = None;
				}
			});
		}
		self.note_approval(&thread, stats.as_ref());
		self.after_change(&thread, was, state, stats.as_ref()).await?;
		ctx.status.write_task(&thread);
		ctx.status.report().await?;
		Ok(())
	}

	/// The thread takes its new state, with the session file omp's integration reported (the question it waits on, its
	/// last line), known before the new state is: a thread counted at work has its session, which Stop everything saves
	/// for `--restore` (0549). The session as it is now (a plan proposed since must count, B1).
	async fn read_new_state(
		&self,
		thread: &ThreadRef,
		pane_id: &str,
		state: AgentState,
	) -> Result<Option<SessionStats>> {
		let ctx = self.link.ctx();
		let reported = ctx.herdr.panes().await?.into_iter().find(|p| p.pane_id == pane_id).and_then(|p| p.session_path);
		if let Some(reported) = reported {
			let moved = ctx.sessions.lock().await.moved(&reported);
			thread.with(|t| t.session_path = Some(moved));
		}
		let path = thread.with(|t| {
			t.state = state;
			t.session_path.clone()
		});
		let Some(path) = path else { return Ok(None) };
		// Before the start's read of today's sessions is done, this one is read in steps too, not in one long block.
		if !ctx.warmed() {
			ctx.sessions.lock().await.warm(std::slice::from_ref(&path), || ctx.stopping()).await;
		}
		let stats = ctx.sessions.lock().await.read(&path, true).clone();
		Ok(Some(stats))
	}

	/// He sent its plan back from Plan Review (0471): the boss offers once to keep what he corrected.
	async fn plan_sent_back(&self, thread: &ThreadRef, stats: &SessionStats) {
		let sent_back = stats.plan_sent_back as usize;
		let offer = thread.with(|t| sent_back > t.plan.as_ref().and_then(|plan| plan.sent_back).unwrap_or(sent_back));
		if offer {
			let said = format!("he sent its plan back{LESSON_OFFER}");
			self.link.ctx().events.thread(thread, &said, ToastPriority::Normal, None).await;
		}
		thread.with(|t| plan_of(t).sent_back = Some(sent_back));
	}

	/// What the new state does: a thread at work, blocked, started again mid-step (0582) or at a run's end.
	async fn after_change(
		&self,
		thread: &ThreadRef,
		was: AgentState,
		state: AgentState,
		stats: Option<&SessionStats>,
	) -> Result<()> {
		if state == AgentState::Working {
			self.working(thread).await;
		} else if state == AgentState::Blocked {
			self.blocked(thread, stats).await?;
		} else if is_idle(state) {
			// Its omp was started again mid-step and is idle now (0582).
			let cut = thread.with(|t| t.cut.take());
			if let Some(cut) = cut {
				self.on_relaunch(cut, thread).await?;
			} else if was == AgentState::Working || was == AgentState::Blocked {
				// A question he dismissed ends the turn too: the thread no longer awaits him (CO-3).
				self.on_turn_end(thread, stats).await?;
			}
		}
		Ok(())
	}

	/// A run's end (working → idle) by the phase it ended in (0394, 0400, 0426): the review turn's posts "review ready"
	/// and asks for the handoff; the handoff turn's closes the task; the finish turn's checks the PR it opened; a task's
	/// is a task end, a plan or a turn (`task_run_ended`).
	async fn on_turn_end(&self, thread: &ThreadRef, stats: Option<&SessionStats>) -> Result<()> {
		let ctx = self.link.ctx();
		let (phase, path) = thread.with(|t| (t.phase, t.path.clone()));
		match phase {
			Phase::Review => ctx.review.ready(thread).await,
			Phase::Finish => ctx.finisher.pr_opened(thread).await,
			Phase::Handoff => {
				let head = git_out(&path, &["rev-parse", "HEAD"]).await?;
				thread.with(|t| set_phase(t, Phase::Task, "its handoff turn ended"));
				// `handoff()` noted HEAD after the review's commits. When it moved since, this run was the next task's (the
				// handoff wrote no entry, CO-4): its end is a task end, not absorbed.
				if head != thread.with(|t| t.head.clone()) {
					self.task_run_ended(thread, stats, head).await?;
				}
				Ok(())
			}
			Phase::Task => {
				let head = git_out(&path, &["rev-parse", "HEAD"]).await?;
				self.task_run_ended(thread, stats, head).await
			}
		}
	}

	/// A step its omp was cut in by a restart (0582, SVC-6), once the new omp is idle: a cut review runs again on the
	/// same commits, a cut `/handoff` runs again, and a cut PR turn goes back to its task (it may have pushed half-way;
	/// he decides).
	async fn on_relaunch(&self, cut: Phase, thread: &ThreadRef) -> Result<()> {
		let ctx = self.link.ctx();
		match cut {
			Phase::Review => {
				let commits = thread.with(|t| {
					set_phase(t, Phase::Task, "its review turn was cut by a restart");
					t.review.as_ref().and_then(|review| review.running.as_ref()).map(|running| running.commits.clone())
				});
				ctx.review.task_ended(thread, commits.as_deref().unwrap_or("new commits"), None).await
			}
			Phase::Handoff => ctx.review.handoff(thread).await,
			Phase::Finish => {
				thread.with(|t| set_phase(t, Phase::Task, "its PR turn was cut by a restart"));
				let said = "no PR: its omp started again mid-turn; everything stays, finish it again";
				ctx.events.thread(thread, said, ToastPriority::Normal, None).await;
				Ok(())
			}
			Phase::Task => Ok(()),
		}
	}

	/// A thread at work.
	async fn working(&self, thread: &ThreadRef) {
		// Reviewed and not finished, and at work again on a task: he sent the reviewed work back (0471). When the
		// boss's own `thread_send` started it, it carried his word, which may not be a rejection: the boss knows.
		let relayed = thread.with(|t| {
			let sent_at = t.sent_at.unwrap_or(0);
			let result = t.review.as_mut().and_then(|review| review.result.as_mut())?;
			let at = result.at.take()?;
			(t.phase == Phase::Task).then_some(sent_at >= at)
		});
		if let Some(relayed) = relayed {
			let said = if relayed {
				format!(
					"at work again on its reviewed work, after your thread_send{LESSON_OFFER} if he rejected that work"
				)
			} else {
				format!("he sent its reviewed work back{LESSON_OFFER}")
			};
			self.link.ctx().events.thread(thread, &said, ToastPriority::Normal, None).await;
		}
		// The review and handoff turns keep the finished task's place in `today` (0394, 0400).
		thread.with(|t| {
			if t.phase == Phase::Task {
				show(t, ThreadState::Working);
			} else if t.shown == ThreadState::Blocked {
				show(t, ThreadState::Done);
			}
		});
	}

	/// A thread waits on his answer to its question.
	async fn blocked(&self, thread: &ThreadRef, stats: Option<&SessionStats>) -> Result<()> {
		let ctx = self.link.ctx();
		let (pane_id, subject, project_id) = thread.with(|t| {
			show(t, ThreadState::Blocked);
			(t.pane_id.clone(), subject(t), t.project.id.clone())
		});
		let question = stats.and_then(|stats| stats.question.as_deref());
		let row = format!("{subject} — {}", question.unwrap_or("awaiting you"));
		ctx.day.thread_blocked(&pane_id, &row, Some(&project_id))?;
		let asked = question.filter(|question| !question.is_empty());
		let said =
			format!("awaiting you{}", asked.map(|question| format!(": {}", quoted(question))).unwrap_or_default());
		let (link, held) = (self.link.clone(), thread.clone());
		let holds: Holds = Arc::new(move || {
			let ctx = link.ctx();
			ctx.threads.get(&held.with(|t| t.pane_id.clone())).is_some_and(|open| open.same(&held))
				&& held.with(|t| t.state == AgentState::Blocked)
		});
		ctx.events.thread(thread, &said, ToastPriority::Urgent, Some(holds)).await;
		let text =
			format!("{subject} — awaiting you{}", asked.map(|question| format!(": {question}")).unwrap_or_default());
		ctx.events.send(ServerMessage::Toast {
			reason: ToastReason::Blocked,
			level: ToastLevel::Warning,
			text,
			pane_id: Some(pane_id),
			priority: None,
			url: None,
		});
		Ok(())
	}

	/// A task's run ended at `head` (0400, 0413): a task end when its branch has new commits, a plan waiting for his
	/// approval in its pane, else a turn.
	async fn task_run_ended(
		&self,
		thread: &ThreadRef,
		stats: Option<&SessionStats>,
		head: Option<String>,
	) -> Result<()> {
		let ctx = self.link.ctx();
		let known = thread.with(|t| t.head.clone());
		if let Some(head) = head.as_deref().filter(|head| !head.is_empty() && Some(*head) != known.as_deref()) {
			if self.steered(thread).await {
				return Ok(());
			}
			let from = thread.with(|t| {
				t.report_asked = false;
				let from = t.head.replace(head.to_owned());
				show(t, ThreadState::Done);
				from
			});
			let said = ctx.review.commits_said(thread, from.as_deref(), head).await?;
			return ctx.review.task_ended(thread, &said.said, said.subject.as_deref()).await;
		}
		let plan = match stats {
			Some(stats) if stats.plan_mode && stats.plan_awaiting.as_deref().is_none_or(str::is_empty) => {
				self.settled_plan(thread).await
			}
			Some(stats) => stats.plan_awaiting.clone().filter(|plan| !plan.is_empty()),
			None => None,
		};
		if let Some(plan) = plan {
			return self.plan_ready(thread, &plan, true).await;
		}
		let path = thread.with(|t| t.session_path.clone());
		let last_line = match path {
			Some(path) => ctx.sessions.lock().await.read(&path, false).last_line.clone(),
			None => stats.and_then(|stats| stats.last_line.clone()),
		};
		thread.with(|t| show(t, ThreadState::Idle));
		let said = match last_line.filter(|line| !line.is_empty()) {
			Some(line) => format!("finished its turn: {}", quoted(&line)),
			None => "finished its turn".to_owned(),
		};
		ctx.events.thread(thread, &said, ToastPriority::Normal, None).await;
		// The check on idle runs beside the pane's state chain: a long check never holds its next state change.
		ctx.review.start_idle_check(thread);
		Ok(())
	}

	/// A task's end without its report gets one steer asking for it; the next end goes to the review either way (0539).
	/// HEAD stays noted where the task began, so that end still counts the task's commits. With the layer off there is no
	/// report to ask for (0576). Whether the steer went in.
	async fn steered(&self, thread: &ThreadRef) -> bool {
		let ctx = self.link.ctx();
		if !ctx.layer
			|| !ctx.options.ask_report_on()
			|| thread.with(|t| t.report_asked)
			|| thread.with(|t| ctx.review.report_in(t))
		{
			return false;
		}
		let pane_id = thread.with(|t| {
			t.report_asked = true;
			t.pane_id.clone()
		});
		match ctx.bridge.prompt(&pane_id, ctx.bridge.seal(REPORT_STEER), Deliver::FollowUp).await {
			Ok(()) => true,
			Err(error) => {
				ctx.warn(error);
				false
			}
		}
	}

	/// A plan-mode turn ended with no proposal in the session yet: the file is read again for a while, as omp's buffered
	/// writer may not have put the `propose` result in it when herdr said idle (B1). The pane's next state change
	/// waits meanwhile (its queue). The plan's title, or `None` when none came or the service stops.
	async fn settled_plan(&self, thread: &ThreadRef) -> Option<String> {
		let ctx = self.link.ctx();
		let end = now_ms() + ctx.options.plan_settle_ms.unwrap_or(PLAN_SETTLE_MS);
		loop {
			let path = thread.with(|t| t.session_path.clone());
			let path = path.filter(|_| now_ms() < end && !ctx.stopping())?;
			tokio::time::sleep(Duration::from_millis(PLAN_POLL_MS)).await;
			let plan = ctx.sessions.lock().await.read(&path, true).plan_awaiting.clone();
			if let Some(plan) = plan.filter(|plan| !plan.is_empty()) {
				return Some(plan);
			}
		}
	}

	/// omp's Plan Review is open: herdr reports idle, but the thread waits on him like a blocked one (0405, 0407). Said
	/// as an urgent line, a toast and a sentence when `announce`; otherwise only counted (HUD, `today`, close question).
	async fn plan_ready(&self, thread: &ThreadRef, plan: &str, announce: bool) -> Result<()> {
		let ctx = self.link.ctx();
		let path = thread.with(|t| t.session_path.clone());
		let file = match path {
			Some(path) => ctx.sessions.lock().await.read(&path, false).plan_file.clone(),
			None => None,
		};
		let (pane_id, subject, project_id) = thread.with(|t| {
			let own = plan_of(t);
			own.awaiting = Some(plan.to_owned());
			own.proposed = Some(PlanProposed { title: plan.to_owned(), file: file.map(Into::into) });
			show(t, ThreadState::Blocked);
			(t.pane_id.clone(), subject(t), t.project.id.clone())
		});
		ctx.day.thread_blocked(&pane_id, &format!("{subject} — plan ready"), Some(&project_id))?;
		if !announce {
			return Ok(());
		}
		// Dropped from the boss's queue once the plan no longer waits (beta day P8).
		let (link, held) = (self.link.clone(), thread.clone());
		let holds: Holds = Arc::new(move || link.ctx().lifecycle.plan_waits(&held));
		ctx.events.thread(thread, "plan ready, awaiting you", ToastPriority::Urgent, Some(holds)).await;
		ctx.events.send(ServerMessage::Toast {
			reason: ToastReason::Blocked,
			level: ToastLevel::Warning,
			text: format!("{subject} — plan ready, awaiting you"),
			pane_id: Some(pane_id),
			priority: None,
			url: None,
		});
		Ok(())
	}

	/// The task file's approved plan (0540): the last plan proposed, once he acted on it in omp's Plan Review and the
	/// thread's session (the new one, after "Approve and execute") is out of plan mode with no plan waiting; refining
	/// keeps it in plan mode until the next proposal.
	pub fn note_approval(&self, thread: &ThreadRef, stats: Option<&SessionStats>) {
		let Some(stats) = stats else { return };
		if stats.plan_mode || stats.plan_awaiting.as_deref().is_some_and(|plan| !plan.is_empty()) {
			return;
		}
		thread.with(|t| {
			let Some(own) = &mut t.plan else { return };
			let Some(proposed) = own.proposed.as_ref() else { return };
			if named(&own.awaiting) || t.task.is_none() {
				return;
			}
			let plan = crate::frontier::server::task_file::TaskPlan {
				approved_at: now_ms(),
				title: proposed.title.clone(),
				file: proposed.file.as_ref().map(|file| file.display().to_string()).or_else(|| stats.plan_file.clone()),
			};
			own.proposed = None;
			if let Some(task) = &mut t.task {
				task.plan = Some(plan);
			}
		});
	}

	/// What the thread's own record says about its plan still waiting on him.
	fn plan_gate(&self, thread: &ThreadRef) -> Waiting {
		if !self.link.ctx().threads.current(thread) {
			return Waiting::No;
		}
		thread.with(|t| {
			if !t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting)) {
				return Waiting::No;
			}
			t.session_path.clone().map_or(Waiting::Yes, Waiting::Read)
		})
	}

	/// Whether the thread's plan still waits on him: it is open and its session shows the proposal unanswered. For a hold
	/// that can't wait for the index: while a read has it, the plan counts as waiting.
	fn plan_waits(&self, thread: &ThreadRef) -> bool {
		match self.plan_gate(thread) {
			Waiting::No => false,
			Waiting::Yes => true,
			Waiting::Read(path) => self
				.link
				.ctx()
				.sessions
				.try_lock()
				.map_or(true, |mut sessions| sessions.read(&path, true).plan_awaiting.is_some()),
		}
	}

	/// [`plan_waits`](Self::plan_waits) for a caller that can wait for the index.
	async fn plan_waits_now(&self, thread: &ThreadRef) -> bool {
		match self.plan_gate(thread) {
			Waiting::No => false,
			Waiting::Yes => true,
			Waiting::Read(path) => self.link.ctx().sessions.lock().await.read(&path, true).plan_awaiting.is_some(),
		}
	}

	/// Shows omp's Plan Review again in a reopened thread whose plan awaits him (0494): omp resumes the plan from its
	/// session but not the picker, so `/plan-review` is sent to its omp once idle, and the extension opens it
	/// (`ctx.openPlanReview()`, 0675). Opening one while a dialog is open would pick its default, "Approve and execute"
	/// (B1), so the omp's own report of an open dialog is read first: the Plan Review already open ends the wait; another
	/// dialog waits for a later look.
	pub async fn reopen_plan_review(&self, thread: &ThreadRef) -> Result<()> {
		let ctx = self.link.ctx();
		let (idle, pane_id, has_session) =
			thread.with(|t| (is_idle(t.state), t.pane_id.clone(), t.session_path.is_some()));
		if !idle || self.busy(&pane_id) || !has_session || !self.plan_waits_now(thread).await {
			return Ok(());
		}
		let Some(agent) = ctx.bridge.agent(&pane_id) else { return Ok(()) };
		let open = ctx.threads.get(&pane_id).is_some_and(|open| open.same(thread));
		if !open || !thread.with(|t| t.plan.as_ref().is_some_and(|plan| plan.reopened)) {
			return Ok(());
		}
		if agent.input_open {
			if agent.input_kind.as_deref() == Some("plan") {
				thread.with(|t| plan_of(t).reopened = false);
			}
			return Ok(());
		}
		thread.with(|t| plan_of(t).reopened = false);
		ctx.bridge.prompt(&pane_id, ctx.bridge.seal(PLAN_REVIEW_COMMAND), Deliver::Steer).await?;
		Ok(())
	}

	/// Ends the turn of the omp in `pane_id` (0675): its `abort` message; an omp that can't abort (0684) has its process
	/// tree ended instead and its pane starts it again with `--restore`, so the session goes on from where it was.
	pub async fn abort_turn(&self, pane_id: &str) -> Result<()> {
		let ctx = self.link.ctx();
		if ctx.bridge.abort(pane_id)? {
			return Ok(());
		}
		let Some(thread) = ctx.threads.get(pane_id) else {
			return Err(Error::user(format!("omp in {pane_id} can't be stopped from Jocasta; check the pin")));
		};
		let session =
			ctx.bridge.session(pane_id).map(PathBuf::from).or_else(|| thread.with(|t| t.session_path.clone()));
		if let Some(session) = session {
			ctx.launched.pause(pane_id, &session.display().to_string())?;
		}
		self.relaunch(&thread).await
	}

	/// Whether the thread waits on him now, before anything is typed into its pane (B1): a question (herdr's blocked) or
	/// a plan in omp's Plan Review, read from herdr's pane and the session as the file is now. A plan found here that the
	/// service missed counts from now on (`plan_found`).
	pub async fn awaiting_him(&self, thread: &ThreadRef) -> Result<Option<Awaiting>> {
		let ctx = self.link.ctx();
		let pane_id = thread.with(|t| t.pane_id.clone());
		let pane = ctx.herdr.panes().await?.into_iter().find(|p| p.pane_id == pane_id);
		if let Some(reported) = pane.as_ref().and_then(|pane| pane.session_path.clone()) {
			let moved = ctx.sessions.lock().await.moved(&reported);
			thread.with(|t| t.session_path = Some(moved));
		}
		let (blocked, plan, path) = thread.with(|t| {
			(
				t.state == AgentState::Blocked,
				t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting)),
				t.session_path.clone(),
			)
		});
		if blocked || pane.is_some_and(|pane| pane.agent_state == AgentState::Blocked) {
			return Ok(Some(Awaiting::Question));
		}
		if plan {
			return Ok(Some(Awaiting::Plan));
		}
		let found = match path {
			Some(path) => ctx.sessions.lock().await.read(&path, true).plan_awaiting.clone(),
			None => None,
		};
		let Some(found) = found.filter(|plan| !plan.is_empty()) else { return Ok(None) };
		self.plan_found(thread, &found, true).await?;
		Ok(Some(Awaiting::Plan))
	}

	/// An idle thread's plan waiting in omp's Plan Review that no state change reported (B1, B3): its idle event came
	/// before omp wrote the proposal, or omp came back with `--continue` after a restart. It counts as awaiting him from
	/// now on, and the boss hears it when `announce`. While a state change of the pane is being handled, the plan is
	/// looked at again once the last one ends: that change may have read the session before the plan was in it.
	pub async fn plan_found(&self, thread: &ThreadRef, plan: &str, announce: bool) -> Result<()> {
		let (awaiting, phase, pane_id, idle) = thread.with(|t| {
			(t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting)), t.phase, t.pane_id.clone(), is_idle(t.state))
		});
		if awaiting || phase != Phase::Task {
			return Ok(());
		}
		let noted = self.plan_after_changes(&pane_id, announce);
		if noted || !idle {
			return Ok(());
		}
		self.plan_ready(thread, plan, announce).await
	}

	/// A thread's pane or workspace was closed in herdr (0405): its worktree and branch stay on disk, a leftover (0456).
	/// What its omp left running ends (0490), found by the omp Jocasta remembered for the pane, never by folder. In quiet
	/// hours its line is held for the boss like any normal one (0298).
	pub async fn closed(&self, thread: &ThreadRef) -> Result<()> {
		let ctx = self.link.ctx();
		let (processes, shown, pane_id) =
			thread.with(|t| (t.omp.as_ref().and_then(|omp| omp.processes.clone()), t.shown, t.pane_id.clone()));
		if let Some(processes) = processes {
			let omps = Arc::clone(&ctx);
			ctx.calls.spawn(async move { omps.omps.end(processes).await });
		}
		if shown == ThreadState::Blocked {
			ctx.day.thread_unblocked(&pane_id)?;
		}
		ctx.status.forget(thread);
		let day = Arc::clone(&ctx);
		ctx.calls.spawn(async move { day.day.refresh_leftovers().await });
		ctx.events.thread(thread, "closed; its worktree and branch stay", ToastPriority::Normal, None).await;
		Ok(())
	}

	/// Commits that appeared on an idle thread's branch with no run of its own (0471): he committed by hand, so the boss
	/// offers once to save a lesson. Threads busy, in a review or handoff, waiting on a plan or with a state change still
	/// being handled are left alone; their HEAD is then taken as the thread's. Every HEAD is read at once, off the event
	/// loop (PERF-1).
	pub async fn hand_commits(&self, by_id: &HashMap<String, PaneInfo>) -> Result<()> {
		let quiet = |thread: &ThreadRef| {
			let (quiet, pane_id) = thread.with(|t| {
				let waiting = t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting));
				let head = t.head.as_deref().is_some_and(|head| !head.is_empty());
				(t.phase == Phase::Task && !waiting && !t.catch_up && head && is_idle(t.state), t.pane_id.clone())
			});
			quiet && by_id.get(&pane_id).is_some_and(|pane| is_idle(pane.agent_state)) && !self.busy(&pane_id)
		};
		let candidates: Vec<(ThreadRef, String, std::path::PathBuf)> = self
			.link
			.ctx()
			.threads
			.all()
			.into_iter()
			.filter(|thread| quiet(thread))
			.filter_map(|thread| thread.with(|t| t.head.clone().map(|was| (thread.clone(), was, t.path.clone()))))
			.collect();
		let heads = futures_util::future::join_all(
			candidates.iter().map(|(_, _, path)| async move { git_out(path, &["rev-parse", "HEAD"]).await }),
		)
		.await;
		for ((thread, was, path), head) in candidates.iter().zip(heads) {
			let Some(head) = head? else { continue };
			// Read again after the wait: a state change meanwhile takes the HEAD as its own.
			if head.is_empty() || head == *was || thread.with(|t| t.head.as_deref() != Some(was)) || !quiet(thread) {
				continue;
			}
			self.hand_commit(thread, path, was, head).await?;
		}
		Ok(())
	}

	/// One thread's HEAD moved to `head` on its own: the boss offers a lesson for the commits he made by hand.
	async fn hand_commit(&self, thread: &ThreadRef, path: &Path, was: &str, head: String) -> Result<()> {
		let range = format!("{was}..{head}");
		let counted = git_out(path, &["rev-list", "--count", range.as_str()]).await?;
		let count: usize = counted.and_then(|count| count.parse().ok()).unwrap_or(0);
		if thread.with(|t| t.head.as_deref() != Some(was)) {
			return Ok(());
		}
		thread.with(|t| t.head = Some(head));
		if count == 0 {
			return Ok(()); // reset or rebased away, not a commit
		}
		let said = format!(
			"he committed by hand on its branch ({count} commit{}){LESSON_OFFER}",
			if count == 1 { "" } else { "s" }
		);
		self.link.ctx().events.thread(thread, &said, ToastPriority::Normal, None).await;
		Ok(())
	}

	/// A thread whose pane herdr restored as a shell, or whose omp ended in it while it stayed open (0521): omp again, in
	/// the same tab (its hunk pane goes with the old one); one that was mid-turn when Jocasta stopped goes on with that
	/// turn (`--restore`, 0549). What the old omp left, the old omp too while it still runs, ends first (0490), so the
	/// new omp is the session's only writer. A review, `/handoff` or PR turn it was cut in is noted (`cut`, 0582): the
	/// new omp, once idle, runs it again or goes back to its task.
	pub async fn relaunch(&self, thread: &ThreadRef) -> Result<()> {
		let ctx = self.link.ctx();
		let (old, project, path, phase, pane_id) = thread.with(|t| {
			(
				t.omp.as_ref().and_then(|omp| omp.processes.clone()),
				t.project.clone(),
				t.path.clone(),
				t.phase,
				t.pane_id.clone(),
			)
		});
		if let Some(old) = old {
			ctx.omps.end(old).await;
		}
		let env = ctx.panes.thread_pane_env(&project, &path).await?;
		let restore = ctx.launched.paused(&pane_id);
		if restore.is_none() && phase != Phase::Task {
			thread.with(|t| t.cut = Some(t.phase));
		}
		let how = Respawn {
			cwd: &path,
			project_id: &project.id,
			env: Some(env),
			restore: restore.as_deref().map(Path::new),
			thread: Some(thread),
		};
		let new_pane = ctx.panes.respawn(&pane_id, how).await?;
		ctx.threads.drop_pane(&pane_id);
		let was_blocked = thread.with(|t| t.shown == ThreadState::Blocked);
		if was_blocked {
			ctx.day.thread_unblocked(&pane_id)?;
		}
		let (awaiting, subject) = thread.with(|t| {
			t.pane_id = new_pane.clone();
			t.state = AgentState::Unknown;
			t.host = None;
			(t.plan.as_ref().is_some_and(|plan| named(&plan.awaiting)), subject(t))
		});
		// An interrupted run that doesn't resume with `--continue` (R3) is no longer at work until herdr says it works
		// again, and a question it asked is gone with it; a restored one (0549) says so itself. A plan in omp's Plan
		// Review still awaits him (0494 shows it again); its row moves to the new pane. Its "unknown" → "idle" is no turn
		// end, so nothing else would undo these.
		if thread.with(|t| t.shown == ThreadState::Working) || (was_blocked && !awaiting) {
			thread.with(|t| show(t, ThreadState::Idle));
		} else if was_blocked {
			ctx.day.thread_blocked(&new_pane, &format!("{subject} — plan ready"), Some(&project.id))?;
		}
		thread.with(|t| {
			if let Some(review) = &mut t.review {
				review.pane = None;
			}
			t.omp = None;
			plan_of(t).reopened = true;
		});
		ctx.threads.add(thread.clone());
		ctx.omps.remember(thread, None).await;
		Ok(())
	}
}

#[cfg(test)]
#[path = "thread_lifecycle_tests.rs"]
mod tests;
