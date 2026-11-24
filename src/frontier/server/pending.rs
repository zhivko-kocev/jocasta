//! Threads that wait (0687): a `thread_start` with `after`, or one made while `threads.maxRunning` threads already run,
//! creates nothing. Its checked arguments are stored as a [`Pending`] in `<data>/projects/<pid>/pending.json`, and
//! [`PendingQueue::tick`] starts it through the normal start once every `after` is in trunk and there is room: no turn
//! of the boss, and its branch is cut from trunk as it is then.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;

use crate::frontier::protocol::{PrCi, ToastPriority};
use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::journal::About;
use crate::frontier::server::pr_review::PrTarget;
use crate::frontier::server::thread_start::StartOptions;
use crate::frontier::server::wording::Door;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::{data_dir, project_dir};
use crate::shared::settings::read_max_running;

/// The file in a project's folder.
const PENDING_FILE: &str = "pending.json";

/// One thing a pending start waits for: a thread (its task id) or another pending start (its id, until it starts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct After {
	pub thread: String,
	/// What it was called when this start was queued, for the lines that name it once it is gone.
	pub name: String,
	/// Its work is in trunk.
	pub met: bool,
	/// The PR it was finished as: it is met when that PR is read as merged.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pr: Option<String>,
}

/// The arguments of a start, as `thread_start` vetted them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingArgs {
	/// The project's name.
	pub project: String,
	pub title: String,
	pub brief: String,
	#[serde(default)]
	pub done_when: Vec<String>,
	#[serde(default)]
	pub heavy: bool,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub budget: Option<f64>,
	/// `boss`, `cli` or `key`: the door it came through.
	pub door: String,
	/// The commit the worktree starts at (0680), when it isn't trunk's tip.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub start_at: Option<String>,
	/// The pull request of a review thread that waits (0680).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pr: Option<PrTarget>,
}

/// A thread start that waits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
	pub id: String,
	pub project_id: String,
	pub title: String,
	pub args: PendingArgs,
	pub after: Vec<After>,
	pub queued_at_ms: i64,
	pub blocked: bool,
}

impl Pending {
	/// The names of what it still waits for.
	fn unmet(&self) -> Vec<&str> {
		self.after.iter().filter(|a| !a.met).map(|a| a.name.as_str()).collect()
	}

	/// `waiting: <title> · after <names>`, `queued: <title>` (waits for room only) or `blocked: <title>`.
	pub fn line(&self) -> String {
		if self.blocked {
			return format!("blocked: {}", self.title);
		}
		let unmet = self.unmet();
		if unmet.is_empty() {
			format!("queued: {}", self.title)
		} else {
			format!("waiting: {} · after {}", self.title, unmet.join(", "))
		}
	}
}

/// The door as it is stored.
pub fn door_name(door: Door) -> &'static str {
	match door {
		Door::Boss => "boss",
		Door::Cli => "cli",
		Door::Key => "key",
	}
}

fn door_of(name: &str) -> Door {
	match name {
		"cli" => Door::Cli,
		"key" => Door::Key,
		_ => Door::Boss,
	}
}

#[derive(Default)]
struct State {
	/// Every project's pending starts, oldest first.
	items: Vec<Pending>,
	loaded: bool,
}

/// The pending starts of every project, and the rule that starts them.
pub struct PendingQueue {
	link: Link,
	st: Mutex<State>,
	/// One tick at a time: a start takes seconds and a second tick must not start the same item.
	ticking: AsyncMutex<()>,
}

/// How many threads run: open, and not asleep (0687).
pub fn running_threads(c: &ServiceContext) -> usize {
	c.threads.entries().iter().filter(|(pane_id, _)| !c.launched.asleep(pane_id)).count()
}

fn read_project(project_id: &str) -> Vec<Pending> {
	let file = project_dir(project_id).join(PENDING_FILE);
	let Ok(text) = std::fs::read_to_string(&file) else { return Vec::new() };
	serde_json::from_str(&text).unwrap_or_else(|error| {
		log(LogLevel::Warn, "pending", &format!("{} isn't read: {error}", file.display()), None);
		Vec::new()
	})
}

impl PendingQueue {
	/// An empty queue; the stored starts of every project are read on first use.
	pub fn new(link: Link) -> Self {
		Self { link, st: Mutex::new(State::default()), ticking: AsyncMutex::new(()) }
	}

	/// The state, read from every project's file on first use.
	fn st(&self) -> MutexGuard<'_, State> {
		let mut st = self.st.lock().unwrap_or_else(PoisonError::into_inner);
		if !st.loaded {
			st.loaded = true;
			for entry in std::fs::read_dir(data_dir().join("projects")).into_iter().flatten().flatten() {
				st.items.extend(read_project(&entry.file_name().to_string_lossy()));
			}
			st.items.sort_by_key(|item| item.queued_at_ms);
		}
		st
	}

	/// Writes `project_id`'s items; its file goes when none are left.
	fn save(&self, st: &State, project_id: &str) -> Result<()> {
		let mine: Vec<&Pending> = st.items.iter().filter(|item| item.project_id == project_id).collect();
		let file = project_dir(project_id).join(PENDING_FILE);
		if mine.is_empty() {
			return match std::fs::remove_file(&file) {
				Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
				_ => Ok(()),
			};
		}
		let text = serde_json::to_string_pretty(&mine).map_err(|error| Error::other(error.to_string()))?;
		write_atomically(&file, text, None)
	}

	/// A save that can't fail the caller: logged.
	fn saved(&self, st: &State, project_id: &str) {
		if let Err(error) = self.save(st, project_id) {
			self.link.ctx().warn(format!("pending starts of {project_id} weren't saved: {error}"));
		}
	}

	/// The pending starts, oldest first; one project's when given.
	pub fn list(&self, project_id: Option<&str>) -> Vec<Pending> {
		self.st().items.iter().filter(|item| project_id.is_none_or(|id| item.project_id == id)).cloned().collect()
	}

	/// What `today` lists: each pending start's line.
	pub fn rows(&self) -> Vec<String> {
		self.st().items.iter().map(Pending::line).collect()
	}

	/// A pending start by its id or title.
	pub fn find(&self, wanted: &str) -> Option<Pending> {
		let key = wanted.trim().to_lowercase();
		let st = self.st();
		st.items
			.iter()
			.find(|item| item.id == wanted)
			.or_else(|| st.items.iter().find(|item| item.title.to_lowercase() == key))
			.cloned()
	}

	/// Whether `project_id` has a pending start of this title.
	pub fn has_title(&self, project_id: &str, title: &str) -> bool {
		self.st().items.iter().any(|item| item.project_id == project_id && item.title == title)
	}

	/// The `after` references as stored entries: an open thread (as `thread_send` finds it) or a pending start. One
	/// that is neither is the start's error; a reference must already exist, so no cycle can be written.
	pub fn resolve_after(&self, refs: &[String], prefix: &str) -> Result<Vec<After>> {
		let c = self.link.ctx();
		let mut found: Vec<After> = Vec::new();
		for wanted in refs {
			let entry = match c.threads.find(wanted) {
				Ok(thread) => {
					let (id, name) = thread.with(|t| (t.task.as_ref().map(|task| task.id.clone()), t.title.clone()));
					let Some(id) = id else {
						return Err(Error::user(format!("{prefix}: after: {wanted} has no task file to wait on.")));
					};
					After { thread: id, name, met: false, pr: None }
				}
				Err(error) => match self.find(wanted) {
					Some(pending) => After { thread: pending.id, name: pending.title, met: false, pr: None },
					None => return Err(Error::user(format!("{prefix}: after: {error}"))),
				},
			};
			if !found.iter().any(|have| have.thread == entry.thread) {
				found.push(entry);
			}
		}
		Ok(found)
	}

	/// Stores a start that waits, and answers its words: `Waiting: …` for one with `after`, else `Queued: …`.
	pub fn enqueue(&self, project_id: &str, args: PendingArgs, after: Vec<After>, running: usize) -> Result<String> {
		let c = self.link.ctx();
		let mut st = self.st();
		let id = loop {
			let mut bytes = [0_u8; 2];
			getrandom::fill(&mut bytes).expect("the OS has random bytes");
			let id = format!("q-{:02x}{:02x}", bytes[0], bytes[1]);
			if !st.items.iter().any(|item| item.id == id) {
				break id;
			}
		};
		let said = if after.is_empty() {
			format!("Queued: {} ({running} running); it starts when one finishes or sleeps.", args.title)
		} else {
			let names: Vec<&str> = after.iter().map(|a| a.name.as_str()).collect();
			format!("Waiting: {} starts after {}.", args.title, names.join(", "))
		};
		let ids: Vec<&str> = after.iter().map(|a| a.thread.as_str()).collect();
		c.journal.append("queued", About { project: project_id, thread: Some(&id) }, json!({ "after": ids }));
		st.items.push(Pending {
			id,
			project_id: project_id.to_owned(),
			title: args.title.clone(),
			args,
			after,
			queued_at_ms: now_ms(),
			blocked: false,
		});
		self.save(&st, project_id).inspect_err(|_| {
			st.items.pop();
		})?;
		Ok(said)
	}

	/// `thread_stop` on a pending start: it is removed, and what waited for it is blocked. `None` when there is none.
	pub async fn remove(&self, wanted: &str) -> Result<Option<Pending>> {
		let Some(found) = self.find(wanted) else { return Ok(None) };
		{
			let mut st = self.st();
			st.items.retain(|item| item.id != found.id);
			self.save(&st, &found.project_id)?;
		}
		self.block_waiting_on(&found.id, "stopped").await;
		self.tick_soon();
		Ok(Some(found))
	}

	/// The thread with task `task_id` is in trunk (a merge finished): what waited for it no longer does.
	pub fn thread_merged(&self, task_id: &str) {
		self.mark(task_id, |after| after.met = true);
	}

	/// The thread with task `task_id` was finished as a PR: what waited for it waits for the PR to merge. A PR with no
	/// address can't be watched, so what waits for it is blocked by the next tick.
	pub fn thread_pr_open(&self, task_id: &str, url: Option<&str>) {
		if let Some(url) = url.filter(|url| !url.is_empty()) {
			self.mark(task_id, |after| after.pr = Some(url.to_owned()));
		}
	}

	fn mark(&self, task_id: &str, change: impl Fn(&mut After)) {
		let mut st = self.st();
		let mut projects: Vec<String> = Vec::new();
		for item in &mut st.items {
			for after in item.after.iter_mut().filter(|after| after.thread == task_id) {
				change(after);
				if !projects.contains(&item.project_id) {
					projects.push(item.project_id.clone());
				}
			}
		}
		for project in projects {
			self.saved(&st, &project);
		}
	}

	/// What waits for `thread` (a task id, or a pending id) and isn't in trunk is blocked, and he hears it (once per
	/// start): `<title> waits on <parent>, which was <why>`. What waits for those is blocked in turn.
	pub async fn block_waiting_on(&self, thread: &str, why: &str) {
		let told = {
			let mut st = self.st();
			let mut told: Vec<(String, String)> = Vec::new();
			let mut parents = vec![(thread.to_owned(), None::<String>)];
			let mut projects: Vec<String> = Vec::new();
			while let Some((parent, name)) = parents.pop() {
				for item in &mut st.items {
					let Some(after) = item.after.iter().find(|a| a.thread == parent && !a.met && a.pr.is_none()) else {
						continue;
					};
					if item.blocked {
						continue;
					}
					let parent_name = name.clone().unwrap_or_else(|| after.name.clone());
					item.blocked = true;
					told.push((item.title.clone(), parent_name));
					parents.push((item.id.clone(), Some(item.title.clone())));
					if !projects.contains(&item.project_id) {
						projects.push(item.project_id.clone());
					}
				}
			}
			for project in projects {
				self.saved(&st, &project);
			}
			told
		};
		let c = self.link.ctx();
		for (title, parent) in told {
			let line = format!("[event] {title} waits on {parent}, which was {why}");
			c.events.tell(&line, ToastPriority::Urgent, None).await;
		}
	}

	/// A thread's PRs that merged meet what waits for them; one closed unmerged blocks it.
	async fn settle_prs(&self, c: &ServiceContext) {
		let urls: Vec<(String, String)> = self
			.st()
			.items
			.iter()
			.flat_map(|item| {
				item.after.iter().filter(|a| !a.met).filter_map(|a| a.pr.clone().map(|url| (a.thread.clone(), url)))
			})
			.collect();
		for (thread, url) in urls {
			match c.thread_facts.pr_facts(&url).map(|facts| facts.ci) {
				Some(PrCi::Merged) => self.thread_merged(&thread),
				Some(PrCi::Closed) => {
					self.mark(&thread, |after| after.pr = None);
					self.block_waiting_on(&thread, "closed unmerged").await;
				}
				_ => {}
			}
		}
	}

	/// Starts what may start (0687): while fewer than `threads.maxRunning` threads run, the oldest start that isn't
	/// blocked and whose `after` are all met goes through the normal start. A start that fails stays, blocked, and he
	/// hears why.
	pub async fn tick(&self) {
		let c = self.link.ctx();
		if c.stopping() {
			return;
		}
		let Ok(_turn) = self.ticking.try_lock() else { return };
		self.settle_prs(&c).await;
		while !c.stopping() {
			let Some(next) = self.next(&c) else { return };
			let options = StartOptions {
				door: door_of(&next.args.door),
				done_when: next.args.done_when.clone(),
				heavy: next.args.heavy,
				budget: next.args.budget,
				after: Vec::new(),
				from_queue: true,
				start_at: next.args.start_at.clone(),
				pr: next.args.pr.clone(),
			};
			let args = &next.args;
			match c.starts.start_with(&args.project, &args.title, &args.brief, options).await {
				Ok(started) => {
					self.started(&next, started.task_id.as_deref());
					c.journal.append(
						"started_from_queue",
						About { project: &next.project_id, thread: started.task_id.as_deref() },
						json!({ "waited_ms": now_ms() - next.queued_at_ms }),
					);
				}
				Err(error) => {
					self.failed(&next);
					let line = format!("[event] {} didn't start: {error}", next.title);
					c.events.tell(&line, ToastPriority::Urgent, None).await;
					self.block_waiting_on(&next.id, "blocked").await;
				}
			}
		}
	}

	/// The oldest start that may go now, when there is room for it.
	fn next(&self, c: &ServiceContext) -> Option<Pending> {
		let limit = read_max_running();
		if limit > 0 && (running_threads(c) + c.starts.starting()) as u64 >= limit {
			return None;
		}
		self.st().items.iter().find(|item| !item.blocked && item.after.iter().all(|a| a.met)).cloned()
	}

	/// `item` started as the thread with task `task_id`: it leaves the queue, and what waited for it waits for the
	/// thread.
	fn started(&self, item: &Pending, task_id: Option<&str>) {
		let mut st = self.st();
		st.items.retain(|have| have.id != item.id);
		let mut projects = vec![item.project_id.clone()];
		for have in &mut st.items {
			for after in have.after.iter_mut().filter(|after| after.thread == item.id) {
				if let Some(task_id) = task_id {
					after.thread = task_id.to_owned();
				}
				if !projects.contains(&have.project_id) {
					projects.push(have.project_id.clone());
				}
			}
		}
		for project in projects {
			self.saved(&st, &project);
		}
	}

	fn failed(&self, item: &Pending) {
		let mut st = self.st();
		if let Some(have) = st.items.iter_mut().find(|have| have.id == item.id) {
			have.blocked = true;
		}
		self.saved(&st, &item.project_id);
	}

	/// A tick on its own task, when anything waits: called where room may have appeared (the refresh tick, a finish, a
	/// sleep, a stop).
	pub fn tick_soon(&self) {
		if self.st().items.is_empty() {
			return;
		}
		let c = self.link.ctx();
		let ctx: Arc<ServiceContext> = Arc::clone(&c);
		c.calls.spawn(async move { ctx.pending.tick().await });
	}
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
