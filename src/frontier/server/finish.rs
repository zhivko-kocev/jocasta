//! `thread_finish` (0426, 0454, 0544): a thread's branch squash-merged into trunk, or pushed and opened as a PR by the
//! thread, then its worktree, workspace and branch removed; every refusal leaves everything in place.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::frontier::protocol::ToastPriority;
use crate::frontier::server::allow::Confirmation;
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::context::{Finishing, Link};
use crate::frontier::server::day::Landing;
use crate::frontier::server::herdr::AgentState;
use crate::frontier::server::status::FileReads;
use crate::frontier::server::task_file::{CloseFacts, CloseHow, close_task};
use crate::frontier::server::thread::{Phase, Thread, ThreadRef, set_phase, subject};
use crate::frontier::server::thread_lifecycle::Awaiting;
use crate::frontier::server::wording::{Door, Overlap, asker, overlap_said, person, pr_request};
use crate::frontier::server::worktrees::{GIT_MS, git_out, remove_checkout};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::git::{GitOptions, git_async};
use crate::shared::process::{RunOptions, run_program};

/// `thread_finish` waits this long for a `/handoff` that hasn't written its entry yet (CO-2).
const HANDOFF_HOLD_MS: i64 = 5 * 60_000;
/// `git merge --squash` in `thread_finish` (CO-10).
const MERGE_TIMEOUT_MS: i64 = 60_000;
/// The squash commit, with the repo's hooks and signing (CO-10).
const COMMIT_TIMEOUT_MS: i64 = 600_000;
/// How long one gh call may take before it is ended (CO-8).
const GH_TIMEOUT_MS: i64 = 60_000;

/// What Finish says about a thread before it asks (`finishable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finishable {
	pub name: String,
	pub trunk: String,
	pub ahead: u64,
	pub failed: Option<String>,
	pub overlaps: Vec<Overlap>,
}

/// One way to finish (0426): what must hold first, what his Allow asks, what stays when it doesn't happen, and doing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
	Merge,
	Pr,
}

impl Mode {
	/// What stays when it doesn't happen.
	fn nothing(self) -> &'static str {
		match self {
			Self::Merge => "nothing was merged",
			Self::Pr => "no PR was opened",
		}
	}

	fn question(self, f: &Finishable) -> String {
		match self {
			Self::Merge => format!("Merge {} into {}?", f.name, f.trunk),
			Self::Pr => format!("Open a PR for {}?", f.name),
		}
	}

	/// What the Allow window says it does; `who` asks, `commits` it takes.
	fn detail(self, thread: &Thread, f: &Finishable, who: &str, commits: &str) -> String {
		let (branch, project) = (&thread.branch, &thread.project.name);
		match self {
			Self::Merge => format!(
				"{who} asks to squash-merge branch {branch} ({commits}) into {} in {project}'s main checkout, then remove the thread's worktree and workspace and delete the branch.",
				f.trunk
			),
			Self::Pr => format!(
				"{who} asks to have the thread push branch {branch} ({commits}) and open a pull request against {} with gh; its worktree and workspace go once the PR is open.",
				f.trunk
			),
		}
	}
}

/// What one gh call left: its exit code (`None` when it didn't answer in time) and its output, trimmed.
struct GhRun {
	code: Option<i32>,
	out: String,
}

/// Whether git printed something: the TypeScript's truthiness of `gitOut`'s result.
fn said(out: &Option<String>) -> bool {
	out.as_deref().is_some_and(|text| !text.is_empty())
}

/// The lines of git's output, none empty.
fn lines_of(out: Option<String>) -> Vec<String> {
	out.unwrap_or_default().split('\n').filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

fn commits_said(count: u64) -> String {
	format!("{count} commit{}", if count == 1 { "" } else { "s" })
}

/// Finish: `thread_finish` and what follows its PR turn.
pub struct Finisher {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
}

impl Finisher {
	pub fn new(link: Link) -> Self {
		Self { link }
	}

	/// `thread_finish(thread, how)`, called only on his word: `merge` squash-merges the branch into trunk in the main
	/// checkout, then removes the worktree and workspace and deletes the branch (when main moved since the review, it is
	/// merged into the branch, re-tested and re-reviewed first, 0454); `pr` has the thread push its branch and open a PR
	/// with gh, and its turn's end removes the worktree (`pr_opened`). The reply says when the last review wasn't a pass
	/// (0453) and names the open threads touching the same files (0456). Every refusal leaves everything in place. The
	/// boss's call and `jocasta thread finish` wait for his Allow (0484, 0544), then look again, as the rules hold for
	/// every door (0544, SVC-5); his Finish key's Merge or PR, typed with its code, is his confirmation (0544, 0568).
	pub async fn finish(&self, thread: &ThreadRef, how: &str, door: Door) -> Result<String> {
		let mode = match how {
			"merge" => Mode::Merge,
			"pr" => Mode::Pr,
			_ => {
				let command = if door == Door::Cli { "jocasta thread finish" } else { "thread_finish" };
				return Err(Error::user(format!("{command}: how is \"merge\" or \"pr\".")));
			}
		};
		let c = self.link.ctx();
		let mut found = self.finishable(thread, door).await?;
		self.ready(mode, thread, &found.trunk).await?;
		if door != Door::Key {
			self.allowed(mode, thread, &found, door).await?;
			// His Allow can take minutes: the thread may have closed, started a run or moved meanwhile.
			if !c.threads.current(thread) {
				return Err(Error::user(format!("{} closed meanwhile; {}.", found.name, mode.nothing())));
			}
			found = self.finishable(thread, door).await?;
			self.ready(mode, thread, &found.trunk).await?;
		}
		let reply = match mode {
			Mode::Merge => self.merge(thread, &found.trunk).await?,
			Mode::Pr => self.open_pr(thread, &found.trunk, door).await?,
		};
		let merging_again = thread.with(|t| t.merge_after_review);
		let mut notes = Vec::new();
		if let Some(failed) = found.failed.as_ref().filter(|_| !merging_again) {
			notes.push(format!("its last review wasn't a pass ({failed})"));
		}
		if !found.overlaps.is_empty() {
			notes.push(format!("open threads touch the same files: {}", overlap_said(&found.overlaps)));
		}
		Ok(if notes.is_empty() { reply } else { format!("{reply} Note: {}.", notes.join("; ")) })
	}

	/// What must hold first besides the thread itself; asked before his Allow and again after it. Only a merge needs
	/// anything: its main checkout.
	async fn ready(&self, mode: Mode, thread: &ThreadRef, trunk: &str) -> Result<()> {
		match mode {
			Mode::Merge => self.main_ready(thread, trunk).await,
			Mode::Pr => Ok(()),
		}
	}

	/// His Allow before the merge or the PR (0484, 0544): one question naming what it does, the review that wasn't a
	/// pass and the threads touching the same files.
	async fn allowed(&self, mode: Mode, thread: &ThreadRef, found: &Finishable, door: Door) -> Result<()> {
		let t = thread.get();
		let commits = commits_said(found.ahead);
		let mut detail = vec![mode.detail(&t, found, &asker(door, "thread finish"), &commits)];
		if let Some(failed) = &found.failed {
			detail.push(format!("Its last review wasn't a pass ({failed})."));
		}
		if !found.overlaps.is_empty() {
			detail.push(format!("Open threads touch the same files: {}.", overlap_said(&found.overlaps)));
		}
		let ask = Confirmation { message: mode.question(found), detail: detail.join("\n") };
		self.link.ctx().allow.ask(&ask, mode.nothing(), door).await?;
		Ok(())
	}

	/// Finish's refusals before any merge or PR (0426, 0544): not while it works or awaits him, mid-review, mid-PR or
	/// mid-handoff, without a trunk or branch, with its worktree on another branch than the thread's (SEC-LOC-7), with
	/// uncommitted changes, or with nothing ahead of trunk. Otherwise what Finish says about it: its trunk, its commits
	/// ahead, a review that wasn't a pass, threads touching the same files. git is asked off the event loop, at once
	/// (PERF-5); the texts are worded for the door (0581).
	pub async fn finishable(&self, thread: &ThreadRef, door: Door) -> Result<Finishable> {
		let c = self.link.ctx();
		let name = thread.with(|t| subject(t));
		let waits = c.lifecycle.awaiting_him(thread).await?;
		let t = thread.get();
		if t.state == AgentState::Working || waits.is_some() {
			let why = match waits {
				Some(Awaiting::Plan) => format!("its plan waits on {}", person(door).him),
				Some(Awaiting::Question) => "blocked".to_owned(),
				None => t.state.as_str().to_owned(),
			};
			return Err(Error::user(format!("{name} isn't idle ({why}); finish it once it is.")));
		}
		if t.phase == Phase::Review || t.phase == Phase::Finish {
			let turn = if t.phase == Phase::Review { "review" } else { "PR" };
			return Err(Error::user(format!("{name} is still in its {turn} turn; wait for it.")));
		}
		// Closing the workspace mid-`/handoff` would lose the task's summary (CO-2); a handoff that never comes stops
		// holding it after 5 minutes.
		if t.phase == Phase::Handoff
			&& !c.lifecycle.handed_off(thread).await
			&& now_ms() - t.handoff_at.unwrap_or(0) < HANDOFF_HOLD_MS
		{
			return Err(Error::user(format!("{name} is still writing its handoff; try again in a minute.")));
		}
		let trunk = match c.places.trunk(&t.project).await? {
			Some(trunk) if !t.branch.is_empty() => trunk,
			other => {
				let why = if other.is_some() { "the thread has no branch" } else { "the project has no trunk branch" };
				return Err(Error::user(format!("{name}: {why}.")));
			}
		};
		let main = t.project.main_path.as_path();
		let range = format!("{trunk}..{}", t.branch);
		let count_args = ["rev-list", "--count", &range];
		let reads = FileReads::default();
		let (on_branch, status, count, overlaps) = tokio::try_join!(
			git_out(&t.path, &["symbolic-ref", "--short", "-q", "HEAD"]),
			git_out(&t.path, &["status", "--porcelain"]),
			git_out(main, &count_args),
			c.status.overlaps(thread, &reads),
		)?;
		// The review covered the worktree's HEAD: a worktree switched to another branch isn't what Finish would merge.
		if on_branch.as_deref() != Some(t.branch.as_str()) {
			return Err(Error::user(format!(
				"{name}'s worktree is on {}, not {}; finish it on its branch.",
				on_branch.as_deref().unwrap_or("a detached HEAD"),
				t.branch
			)));
		}
		if said(&status) {
			return Err(Error::user(format!(
				"{name} has uncommitted changes in its worktree; have it commit or drop them first."
			)));
		}
		let ahead = count.as_deref().and_then(|count| count.parse::<u64>().ok()).unwrap_or(0);
		if ahead == 0 {
			return Err(Error::user(format!("{name} has no commits ahead of {trunk}; nothing to finish.")));
		}
		let failed = c.review.failed(&t);
		Ok(Finishable { name, trunk, ahead, failed, overlaps })
	}

	/// `thread_finish` merge (0426, 0454): when trunk gained commits the branch doesn't hold, trunk is merged into the
	/// branch in its worktree and the review step runs again; the squash follows only once that review passes
	/// (`after_review`). A conflict aborts the merge and stops. Otherwise the branch is squash-merged now.
	pub async fn merge(&self, thread: &ThreadRef, trunk: &str) -> Result<String> {
		let c = self.link.ctx();
		let t = thread.get();
		let name = subject(&t);
		self.main_ready(thread, trunk).await?;
		if git_out(&t.project.main_path, &["merge-base", "--is-ancestor", trunk, &t.branch]).await?.is_some() {
			return self.squash(thread, trunk).await;
		}
		if c.panes.hunk().is_none() {
			return Err(Error::user(format!(
				"{trunk} moved since {name}'s review and no new review can run (hunk isn't installed); nothing was merged."
			)));
		}
		let merged =
			git_async(&t.path, &["merge", "--no-edit", trunk], MERGE_TIMEOUT_MS, GitOptions::default()).await?;
		if merged.code != Some(0) {
			let conflicts = lines_of(git_out(&t.path, &["diff", "--name-only", "--diff-filter=U"]).await?);
			git_async(&t.path, &["merge", "--abort"], GIT_MS, GitOptions::default()).await?;
			let at = if conflicts.is_empty() { String::new() } else { format!(" in {}", conflicts.join(", ")) };
			return Err(Error::user(format!(
				"{trunk} moved since {name}'s review, and bringing it into {} conflicts{at}; that merge was aborted and everything stays.",
				t.branch
			)));
		}
		let head = git_out(&t.path, &["rev-parse", "HEAD"]).await?;
		thread.with(|t| {
			t.merge_after_review = true;
			t.head = head;
		});
		c.review.task_ended(thread, &format!("{trunk} merged in"), None).await?;
		Ok(format!(
			"{trunk} moved since {name}'s review: it was merged into {}, which is being tested and reviewed again; it merges into {trunk} only when that passes, and you hear either way.",
			t.branch
		))
	}

	/// Refuses when the main checkout has uncommitted changes or isn't on trunk (0426).
	pub async fn main_ready(&self, thread: &ThreadRef, trunk: &str) -> Result<()> {
		let (main, project) = thread.with(|t| (t.project.main_path.clone(), t.project.name.clone()));
		let (status, current) = tokio::try_join!(
			git_out(&main, &["status", "--porcelain", "--untracked-files=no"]),
			git_out(&main, &["symbolic-ref", "--short", "-q", "HEAD"]),
		)?;
		if said(&status) {
			return Err(Error::user(format!(
				"the main checkout of {project} has uncommitted changes; nothing was merged."
			)));
		}
		if current.as_deref() != Some(trunk) {
			return Err(Error::user(format!(
				"the main checkout of {project} is on {}, not {trunk}; nothing was merged.",
				current.as_deref().unwrap_or("a detached HEAD")
			)));
		}
		Ok(())
	}

	/// The merge once its new review passed (0454): squashed now, unless trunk moved again meanwhile; said as one event
	/// with what the review found, `reviewed`.
	pub async fn after_review(&self, thread: &ThreadRef, reviewed: &str) -> Result<()> {
		let c = self.link.ctx();
		let trunk = c.places.trunk(&thread.with(|t| t.project.clone())).await?;
		let passed = format!("its new review passed ({reviewed})");
		let said = match self.squash_if_still_there(thread, trunk).await {
			Ok(said) => said,
			Err(error) => {
				c.events.thread(thread, &format!("{passed}; not merged: {error}"), ToastPriority::Normal, None).await;
				return c.review.handoff(thread).await;
			}
		};
		let mut first = said.chars();
		let lowered = first.next().map(|c| c.to_lowercase().chain(first).collect::<String>()).unwrap_or_default();
		c.events.thread(thread, &format!("{passed}; {lowered}"), ToastPriority::Normal, None).await;
		Ok(())
	}

	/// `after_review`'s try: the squash, unless there is no trunk, the main checkout isn't ready or trunk moved again.
	async fn squash_if_still_there(&self, thread: &ThreadRef, trunk: Option<String>) -> Result<String> {
		let Some(trunk) = trunk else {
			return Err(Error::user("the project has no trunk branch; nothing was merged."));
		};
		self.main_ready(thread, &trunk).await?;
		let (main, branch) = thread.with(|t| (t.project.main_path.clone(), t.branch.clone()));
		if git_out(&main, &["merge-base", "--is-ancestor", &trunk, &branch]).await?.is_none() {
			return Err(Error::user(format!(
				"{trunk} moved again during its review; nothing was merged (finish it again)."
			)));
		}
		self.squash(thread, &trunk).await
	}

	/// Squash-merges the branch into trunk in the main checkout, then removes the thread (0426).
	async fn squash(&self, thread: &ThreadRef, trunk: &str) -> Result<String> {
		let c = self.link.ctx();
		let t = thread.get();
		let name = subject(&t);
		let main = t.project.main_path.as_path();
		let range = format!("{trunk}..{}", t.branch);
		let (count_args, log_args) = (["rev-list", "--count", &range], ["log", "--reverse", "--format=- %s", &range]);
		let (count, log) = tokio::try_join!(git_out(main, &count_args), git_out(main, &log_args))?;
		let ahead = count.as_deref().and_then(|count| count.parse::<u64>().ok()).unwrap_or(0);
		let subjects = log.unwrap_or_default();
		let squash =
			git_async(main, &["merge", "--squash", &t.branch], MERGE_TIMEOUT_MS, GitOptions::default()).await?;
		if squash.code != Some(0) {
			let conflicts = lines_of(git_out(main, &["diff", "--name-only", "--diff-filter=U"]).await?);
			git_async(main, &["reset", "--merge"], GIT_MS, GitOptions::default()).await?;
			let at =
				if conflicts.is_empty() { String::new() } else { format!(" (conflicts in {})", conflicts.join(", ")) };
			return Err(Error::user(format!(
				"{name} doesn't merge cleanly into {trunk}{at}; the merge was aborted and everything stays."
			)));
		}
		self.commit_squash(main, &t, &subjects, &name).await?;
		let logged = git_out(main, &["log", "-1", "--format=%H%n%h"]).await?.unwrap_or_default();
		let mut ids = logged.split('\n');
		let (full, commit) = (ids.next().unwrap_or_default(), ids.next().unwrap_or_default());
		let line = format!("merged into {trunk} as {commit}");
		c.day.landed(&Landing {
			pane_id: &t.pane_id,
			title: &t.title,
			project_id: &t.project.id,
			line: &line,
			url: None,
		})?;
		let merged = format!("Merged {name} into {trunk} as {commit} ({} squashed)", commits_said(ahead));
		let commit_id = (!full.is_empty()).then(|| full.to_owned());
		if let Some(removed) = self.remove_thread(thread, CloseHow::Merged, commit_id).await? {
			return Ok(format!("{merged}, but {removed}."));
		}
		if git_async(main, &["branch", "-D", &t.branch], GIT_MS, GitOptions::default()).await?.code != Some(0) {
			return Ok(format!(
				"{merged}; its worktree and workspace are removed, but branch {} couldn't be deleted.",
				t.branch
			));
		}
		Ok(format!("{merged}; its worktree, workspace and branch are removed."))
	}

	/// The squash commit, with the repo's hooks and signing run off the event loop, bounded (CO-10); a refusal resets
	/// the main checkout to how it was.
	async fn commit_squash(
		&self,
		main: &Path,
		t: &crate::frontier::server::thread::Thread,
		subjects: &str,
		name: &str,
	) -> Result<()> {
		let body = format!("Squash-merged from {}:\n{subjects}", t.branch);
		let committed =
			git_async(main, &["commit", "-q", "-m", &t.title, "-m", &body], COMMIT_TIMEOUT_MS, GitOptions::default())
				.await?;
		if committed.code == Some(0) {
			return Ok(());
		}
		git_async(main, &["reset", "--merge"], GIT_MS, GitOptions::default()).await?;
		let Some(code) = committed.code else {
			return Err(Error::user("the merge commit's hooks didn't finish in 10 minutes; nothing was merged."));
		};
		let stderr = committed.stderr.trim();
		let stdout = committed.stdout.trim();
		let why = if !stderr.is_empty() {
			stderr.to_owned()
		} else if !stdout.is_empty() {
			stdout.to_owned()
		} else {
			format!("exit {code}")
		};
		Err(Error::user(format!("{name}: the squash commit failed ({why}); nothing was merged.")))
	}

	/// Removes a finished thread's workspace and worktree, and records how it closed and its commit (0621) once its
	/// workspace is closed, so a folder that stays locked doesn't keep a merged or PR'd thread open; what stays and why
	/// when it couldn't. herdr's own `worktree.remove` stops the workspace's terminals and on Windows restarts
	/// them as shells in the worktree, which keep the folder locked (F-02). So the workspace is closed first, then git
	/// removes the checkout once the panes' processes are gone, trying again for a while. While it runs, the refresh tick
	/// neither finds the thread again nor reports its workspace closed (CR-11).
	async fn remove_thread(&self, thread: &ThreadRef, how: CloseHow, commit: Option<String>) -> Result<Option<String>> {
		let c = self.link.ctx();
		let t = thread.get();
		c.threads.drop_pane(&t.pane_id);
		c.set_finishing(&t.workspace_id, Finishing::Removing);
		// Its summaries are read after the folder is gone: they still belong to this project (CO-2).
		c.places.removed(&t.path, &t.project);
		if let Err(error) = c.herdr.close_workspace(&t.workspace_id).await {
			c.clear_finishing(&t.workspace_id);
			c.threads.add(thread.clone());
			return Ok(Some(format!(
				"herdr couldn't close its workspace ({error}); its worktree, workspace and branch stay"
			)));
		}
		// Kept until a refresh no longer sees the workspace: one read during the closing may still list its panes.
		c.set_finishing(&t.workspace_id, Finishing::Removed);
		c.day.thread_unblocked(&t.pane_id)?;
		let failures = {
			// A task file write that began before the close never lands after it.
			let _writing = c.status.task_writing();
			thread.with(|t| {
				let mut failures = Vec::new();
				if let Some(task) = t.task.as_mut() {
					close_task(task, CloseFacts { how, commit }, |error| failures.push(error), None);
				}
				failures
			})
		};
		for failure in failures {
			c.warn(failure);
		}
		if let Some(failed) = remove_checkout(&t.project.main_path, &t.path, None).await {
			return Ok(Some(format!(
				"its workspace is closed, but its worktree folder {} couldn't be removed ({failed}); the folder and branch stay",
				t.path.display()
			)));
		}
		c.reviewed.mark(&t.path, None)?;
		c.numbers.forget(&t.path)?;
		c.status.forget(thread);
		Ok(None)
	}

	/// Runs gh in `cwd` (never prompting, at most a minute, CO-8); `None` when gh isn't installed, `code: None` when it
	/// didn't answer in time.
	async fn gh(&self, args: &[&str], cwd: &Path) -> Option<GhRun> {
		let c = self.link.ctx();
		let mut env: HashMap<String, String> = crate::shared::env::vars()
			.into_iter()
			.map(|(name, value)| (name.to_string_lossy().into_owned(), value.to_string_lossy().into_owned()))
			.collect();
		env.insert("GH_PROMPT_DISABLED".to_owned(), "1".to_owned());
		let mut argv = c.options.gh.clone().unwrap_or_else(|| vec!["gh".to_owned()]);
		argv.extend(args.iter().map(|arg| (*arg).to_owned()));
		let options = RunOptions {
			cwd: Some(cwd.to_path_buf()),
			env: Some(env),
			timeout_ms: Some(c.options.gh_timeout_ms.unwrap_or(GH_TIMEOUT_MS)),
			..RunOptions::default()
		};
		// A program that isn't on PATH fails to start at once.
		let run = run_program(&argv, options).await.ok()?;
		Some(GhRun { code: run.code, out: format!("{}{}", run.stdout, run.stderr).trim().to_owned() })
	}

	/// `gh pr view <url> --json number,state,statusCheckRollup` (0668): its output when gh answered, else why not. A PR's
	/// url needs no repository, so it runs in the data folder.
	pub(super) async fn pr_view(&self, url: &str) -> std::result::Result<String, String> {
		let args = ["pr", "view", url, "--json", "number,state,statusCheckRollup"];
		match self.gh(&args, &crate::shared::paths::data_dir()).await {
			Some(run) if run.code == Some(0) => Ok(run.out),
			Some(run) if run.code.is_none() => Err("gh didn't answer in time".to_owned()),
			Some(run) => Err(run.out.split('\n').next().unwrap_or_default().to_owned()),
			None => Err("gh isn't installed".to_owned()),
		}
	}

	/// `thread_finish` pr (0426): the thread is asked to push its branch and open the PR with gh; its turn's end, then
	/// `pr_opened`, removes the worktree. Refuses when gh isn't installed or logged in, or the project has no remote.
	pub async fn open_pr(&self, thread: &ThreadRef, trunk: &str, door: Door) -> Result<String> {
		let c = self.link.ctx();
		let t = thread.get();
		let name = subject(&t);
		let Some(auth) = self.gh(&["auth", "status"], &t.path).await else {
			return Err(Error::user(
				"gh isn't installed: install GitHub CLI (https://cli.github.com), then gh auth login.",
			));
		};
		let Some(code) = auth.code else {
			return Err(Error::user("gh didn't answer in a minute; nothing was changed."));
		};
		if code != 0 {
			let said =
				format!("gh isn't logged in (gh auth status: {})", auth.out.split('\n').next().unwrap_or_default());
			// Worded for the door (0581): the boss tells him; his key and the CLI tell him what to do.
			let then = match door {
				Door::Boss => "; he runs gh auth login.",
				Door::Key => ": run gh auth login, then Finish again.",
				Door::Cli => ": run gh auth login, then jocasta thread finish again.",
			};
			return Err(Error::user(format!("{said}{then}")));
		}
		let remotes = lines_of(git_out(&t.path, &["remote"]).await?);
		let Some(remote) = remotes.iter().find(|remote| *remote == "origin").or_else(|| remotes.first()) else {
			return Err(Error::user(format!("{} has no git remote to push {} to.", t.project.name, t.branch)));
		};
		thread.with(|t| set_phase(t, Phase::Finish, "it is asked to open its PR"));
		if let Err(error) =
			c.bridge.prompt(&t.pane_id, c.bridge.seal(&pr_request(remote, &t.branch, trunk)), Deliver::FollowUp).await
		{
			thread.with(|t| set_phase(t, Phase::Task, "it didn't take the PR request"));
			return Err(Error::user(format!("{name} didn't take the request: {error}")));
		}
		Ok(format!(
			"Asked {name} to push {} and open a PR against {trunk}; once it's open, its worktree and workspace are removed and you hear it.",
			t.branch
		))
	}

	/// The PR turn ended (0426): an open PR on the pushed branch removes the worktree and workspace; else all stays.
	pub async fn pr_opened(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		thread.with(|t| set_phase(t, Phase::Task, "its PR turn ended"));
		let t = thread.get();
		let view = self.gh(&["pr", "view", &t.branch, "--json", "url,state,headRefOid"], &t.path).await;
		// Not JSON: reported below with gh's own first line.
		let pr = view
			.as_ref()
			.filter(|view| view.code == Some(0))
			.and_then(|view| serde_json::from_str::<Value>(&view.out).ok())
			.filter(|pr| !pr.is_null());
		let field = |name: &str| pr.as_ref().and_then(|pr| pr.get(name)).and_then(Value::as_str).map(str::to_owned);
		let reason = match &pr {
			None if view.as_ref().is_some_and(|view| view.code.is_none()) => {
				Some("gh didn't answer in a minute".to_owned())
			}
			None => {
				let first = view.as_ref().map(|view| format!(": {}", view.out.split('\n').next().unwrap_or_default()));
				Some(format!("gh finds no PR for {}{}", t.branch, first.unwrap_or_default()))
			}
			Some(_) if field("state").as_deref() != Some("OPEN") => Some(format!(
				"its PR is {}",
				field("state").map_or_else(|| "not open".to_owned(), |s| s.to_lowercase())
			)),
			Some(_) => {
				let head = git_out(&t.path, &["rev-parse", "HEAD"]).await?;
				(field("headRefOid") != head)
					.then(|| "its PR doesn't have the branch's last commit (not pushed?)".to_owned())
			}
		};
		if pr.is_none() || reason.is_some() {
			let line = format!("no PR: {}; everything stays", reason.unwrap_or_default());
			c.events.thread(thread, &line, ToastPriority::Normal, None).await;
			return Ok(());
		}
		let removed = self.remove_thread(thread, CloseHow::Pr, field("headRefOid")).await?;
		let url = field("url").unwrap_or_else(|| "undefined".to_owned());
		let pr = field("url");
		let landing = Landing {
			pane_id: &t.pane_id,
			title: &t.title,
			project_id: &t.project.id,
			line: "PR open",
			url: pr.as_deref(),
		};
		c.day.landed(&landing)?;
		let how = match removed {
			Some(removed) => format!(", but {removed}"),
			None => "; its worktree and workspace are removed, the branch stays".to_owned(),
		};
		c.events.thread(thread, &format!("PR open{how} · {url}"), ToastPriority::Normal, None).await;
		Ok(())
	}
}

#[cfg(test)]
#[path = "finish_tests.rs"]
mod tests;
