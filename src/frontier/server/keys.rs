//! His thread keys in herdr (0544, 0554, 0581): Finish, Stop, Review and New thread on the focused thread or the one
//! picked in `today`. Their own key and code are his confirmation; every other rule holds.
//!
//! `Keys` is built after the context and takes it (`Arc<ServiceContext>`); it isn't a context field. Each key's call is
//! counted in `ctx.calls` like a tool call (SVC-13), so `stop` waits for it.

use std::sync::Arc;

use crate::frontier::protocol::{ToastLevel, ToastReason};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::events::Toast;
use crate::frontier::server::finish::Finishable;
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::journal::About;
use crate::frontier::server::thread::{Phase, ThreadRef, subject};
use crate::frontier::server::thread_lifecycle::Awaiting;
use crate::frontier::server::wording::{Door, brief_title, overlap_said};
use crate::frontier::server::worktrees::git_out;
use crate::shared::errors::{Error, Result};

/// A key's outcome that is only half done (0581, C11): every such text says what landed, then ", but" what didn't.
const HALF_DONE: &str = ", but ";

/// The thread keys that name a thread of their own pane: each one's name in a refusal and the key `today` takes for it
/// on its picked thread (0581, UX-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
	Finish,
	Stop,
	Review,
}

impl KeyAction {
	fn name(self) -> &'static str {
		match self {
			Self::Finish => "Finish",
			Self::Stop => "Stop",
			Self::Review => "Review",
		}
	}
}

/// How his Finish key finishes a thread: the popup's Merge or PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishHow {
	Merge,
	Pr,
}

impl FinishHow {
	/// The word `finish` takes.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Merge => "merge",
			Self::Pr => "pr",
		}
	}
}

/// What Finish's key shows him before it asks merge or PR (`keyFinishable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFinishable {
	pub pane_id: String,
	pub name: String,
	pub detail: String,
	pub trunk: String,
}

/// The New thread form: his git projects, and the one picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThreadForm {
	pub projects: Vec<String>,
	pub project: String,
}

/// A key's answer to his terminal: its text, and whether it went wrong or only half through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyOutcome {
	pub text: String,
	pub is_error: bool,
}

pub struct Keys {
	ctx: Arc<ServiceContext>,
}

impl Keys {
	pub fn new(ctx: Arc<ServiceContext>) -> Self {
		Self { ctx }
	}

	/// Runs one key's work counted in `ctx.calls` (SVC-13), refused once Jocasta is closing, as the TypeScript's `track`:
	/// `stop`, `review`, `finish` (after its thread is found) and `start` refuse; `finishable` and `new_thread_form`
	/// only read, so they don't.
	async fn track<T>(&self, run: impl std::future::Future<Output = Result<T>>) -> Result<T> {
		if self.ctx.stopping() {
			return Err(Error::user("Jocasta is closing; nothing was changed."));
		}
		self.ctx.calls.run(run).await
	}

	/// The thread `pane_id` belongs to: its own pane, or another pane of its workspace (a shell beside
	/// it). Refused for any other pane, the boss's among them; in `today`'s own pane the refusal names `today`'s key for
	/// the action (0581, UX-16).
	async fn thread_of_pane(&self, pane_id: Option<&str>, action: KeyAction) -> Result<ThreadRef> {
		let c = &self.ctx;
		let pane_id = pane_id.filter(|id| !id.is_empty());
		if let Some(own) = pane_id.and_then(|id| c.threads.get(id)) {
			return Ok(own);
		}
		let pane = match pane_id {
			Some(id) => c.herdr.panes().await?.into_iter().find(|p| p.pane_id == id),
			None => None,
		};
		if let Some(pane) = &pane {
			let of_workspace = c.threads.all().into_iter().find(|t| t.with(|t| t.workspace_id == pane.workspace_id));
			if let Some(thread) = of_workspace {
				return Ok(thread);
			}
		}
		Err(Error::user(format!(
			"{}: the focused pane isn't a thread's; focus a thread or pick one in today.",
			action.name()
		)))
	}

	/// Stop all (0686): `abort` to every connected pane, the boss's included. Phases and stuck states stay. An omp that
	/// can't abort (0684) isn't counted. The toast says how many turns ended; the journal gets `stop_all`.
	pub async fn stop_all(&self) -> Result<String> {
		self.track(async {
			let c = &self.ctx;
			let panes = c.bridge.panes();
			let mut stopped = 0_usize;
			for pane in &panes {
				match c.bridge.abort(pane) {
					Ok(true) => stopped += 1,
					Ok(false) => {}
					Err(error) => c.warn(error),
				}
			}
			c.journal.append("stop_all", About::project(""), serde_json::json!({ "panes": panes.len() }));
			let text = format!("stopped {stopped} {}", if stopped == 1 { "turn" } else { "turns" });
			c.events.send(Toast::new(ToastReason::Event, ToastLevel::Info, text.clone()).into());
			Ok(text)
		})
		.await
	}

	/// Stop (0544): one Escape into the thread's omp, while it works or asks him.
	pub async fn stop(&self, pane_id: Option<&str>) -> Result<String> {
		self.track(self.stop_now(pane_id)).await
	}

	async fn stop_now(&self, pane_id: Option<&str>) -> Result<String> {
		let thread = self.thread_of_pane(pane_id, KeyAction::Stop).await?;
		let (name, state, pane) = thread.with(|t| (subject(t), t.state, t.pane_id.clone()));
		if state != AgentState::Working && state != AgentState::Blocked {
			return Err(Error::user(format!("Stop: {name} isn't working; nothing to stop.")));
		}
		self.ctx.lifecycle.abort_turn(&pane).await?;
		Ok(format!("Stopped {name}'s run."))
	}

	/// Finish's checks before it asks merge or PR (0544): the thread's pane, its name, its trunk and what Finish would
	/// take.
	pub async fn finishable(&self, pane_id: Option<&str>) -> Result<KeyFinishable> {
		let thread = self.thread_of_pane(pane_id, KeyAction::Finish).await?;
		let found = self.ctx.finisher.finishable(&thread, Door::Key).await?;
		let (pane_id, branch) = thread.with(|t| (t.pane_id.clone(), t.branch.clone()));
		let detail = finishable_detail(&branch, &found);
		Ok(KeyFinishable { pane_id, name: found.name, detail, trunk: found.trunk })
	}

	/// Finish with his choice (0426, 0544, 0568): Merge or PR typed with its code confirms, every other rule holds. A
	/// merge that landed but left something behind (its workspace, folder or branch) is half done: an error (0581, C11).
	pub async fn finish(&self, pane_id: &str, how: FinishHow) -> Result<KeyOutcome> {
		let c = &self.ctx;
		let Some(thread) = c.threads.get(pane_id) else {
			return Err(Error::user("Finish: the thread closed meanwhile."));
		};
		let text = self.track(c.finisher.finish(&thread, how.as_str(), Door::Key)).await?;
		Ok(finish_outcome(text))
	}

	/// Review (0394, 0406, 0554): the review step on demand, as at a task's end: the thread is asked to test and review
	/// its commits on `<trunk>...<branch>`. Not while it works, awaits him or
	/// is in another step; nothing to review without commits ahead of trunk.
	pub async fn review(&self, pane_id: Option<&str>) -> Result<String> {
		self.track(self.review_now(pane_id)).await
	}

	async fn review_now(&self, pane_id: Option<&str>) -> Result<String> {
		let c = &self.ctx;
		let thread = self.thread_of_pane(pane_id, KeyAction::Review).await?;
		let name = thread.with(|t| subject(t));
		let waits = c.lifecycle.awaiting_him(&thread).await?;
		let (state, phase, project, path, branch) =
			thread.with(|t| (t.state, t.phase, t.project.clone(), t.path.clone(), t.branch.clone()));
		if state == AgentState::Working || waits.is_some() {
			let why = match waits {
				Some(Awaiting::Plan) => "its plan waits on you",
				Some(Awaiting::Question) => "blocked",
				None => state.as_str(),
			};
			return Err(Error::user(format!("Review: {name} isn't idle ({why}); review it once it is.")));
		}
		if phase != Phase::Task {
			return Err(Error::user(format!("Review: {name} is in its {} step; wait for it.", phase.as_str())));
		}
		let (trunk, head) = tokio::join!(c.places.trunk(&project), git_out(&path, &["rev-parse", "HEAD"]));
		let trunk = trunk?.filter(|trunk| !trunk.is_empty());
		let head = head?.filter(|head| !head.is_empty());
		let (Some(trunk), Some(head), false) = (&trunk, &head, branch.is_empty()) else {
			let what = if trunk.is_some() { "no branch" } else { "no trunk branch in its project" };
			return Err(Error::user(format!("Review: {name} has {what}.")));
		};
		let ahead = git_out(&path, &["rev-list", "--count", &format!("{trunk}..HEAD")]).await?;
		if ahead.and_then(|count| count.parse::<u64>().ok()).unwrap_or(0) == 0 {
			return Err(Error::user(format!("Review: {name} has no commits ahead of {trunk}; nothing to review.")));
		}
		let found = c.review.commits_said(&thread, None, head).await?;
		c.review.task_ended(&thread, &found.said, found.subject.as_deref()).await?;
		Ok(format!("Reviewing {name}: the thread tests and reviews {}.", found.said))
	}

	/// New thread's form (0554, 0581): his git projects with one always picked: the project of `pane_id` (a thread
	/// picked in `today`, or the focused pane), else the one of the pane he last focused, else the last one a thread
	/// started in, else the first.
	pub async fn new_thread_form(&self, pane_id: Option<&str>) -> Result<NewThreadForm> {
		let c = &self.ctx;
		let panes = c.herdr.panes().await?;
		c.starts.note_focus(&panes)?;
		let projects: Vec<String> = c.places.listed().into_iter().filter(|p| p.git).map(|p| p.name).collect();
		let Some(first) = projects.first().cloned() else {
			return Err(Error::user("New thread: no git project to start one in; add one with `jocasta project add`."));
		};
		let pane_id = pane_id.filter(|id| !id.is_empty());
		let pane = pane_id.and_then(|id| panes.iter().find(|p| p.pane_id == id));
		let mut of_pane = pane_id.and_then(|id| c.threads.get(id)).map(|t| t.with(|t| t.project.clone()));
		if of_pane.is_none() {
			of_pane = pane.and_then(|pane| {
				c.threads
					.all()
					.into_iter()
					.find(|t| t.with(|t| t.workspace_id == pane.workspace_id))
					.map(|t| t.with(|t| t.project.clone()))
			});
		}
		if of_pane.is_none() {
			of_pane = match pane_id {
				Some(id) => c.starts.pane_project(id)?,
				None => None,
			};
		}
		if of_pane.is_none()
			&& let Some(cwd) = pane.and_then(|pane| pane.cwd.as_deref())
		{
			of_pane = c.places.project_at(cwd)?;
		}
		let project = [of_pane, c.starts.focused_project(), c.starts.last_started_in()]
			.into_iter()
			.flatten()
			.map(|p| p.name)
			.find(|name| projects.contains(name));
		Ok(NewThreadForm { projects, project: project.unwrap_or(first) })
	}

	/// New thread from his form (0544, 0554): its title from the brief; it opens in plan mode and waits for him. A
	/// start whose brief wasn't sent is half done: an error (0581, C11).
	pub async fn start(&self, project: &str, brief: &str) -> Result<KeyOutcome> {
		let c = &self.ctx;
		let started = self.track(c.starts.start(project, &brief_title(brief), brief, Door::Key)).await?;
		Ok(KeyOutcome { text: started.text, is_error: !started.done })
	}
}

/// What Finish would take, as the key's popup shows it: its commits, a review that wasn't a pass, threads on the same files.
fn finishable_detail(branch: &str, found: &Finishable) -> String {
	let mut lines = vec![format!(
		"{} commit{} on {branch}, onto {}.",
		found.ahead,
		if found.ahead == 1 { "" } else { "s" },
		found.trunk
	)];
	if let Some(failed) = found.failed.as_deref().filter(|failed| !failed.is_empty()) {
		lines.push(format!("Its last review wasn't a pass ({failed})."));
	}
	if !found.overlaps.is_empty() {
		lines.push(format!("Open threads touch the same files: {}.", overlap_said(&found.overlaps)));
	}
	lines.join(
		"
",
	)
}

/// A finish text as the key answers: one that says what landed, then ", but" what didn't, is half done (0581, C11).
fn finish_outcome(text: String) -> KeyOutcome {
	KeyOutcome { is_error: text.contains(HALF_DONE), text }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
