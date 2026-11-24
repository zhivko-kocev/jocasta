//! The review step after a task (0394, 0406, 0425, 0453, 0540, 0542): hunk beside the thread on its diff, the task's
//! checks through the runner, the review turn, its outcome said once, then `/handoff` (0400); the check on idle (0513)
//! and the `check` tool. The Rust port of `review-flow.ts` (0625).
//!
//! The part's own state, each project pane's check state (0617), sits behind one lock that is never held across an
//! `.await` and never while another part is called; the checks on idle and the pane checks that outlast their wait are
//! counted in `idle_checks`, which `stop` waits for ([`ReviewFlow::settled`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use regex::Regex;
use tokio::sync::Notify;

use crate::frontier::protocol::{ServerMessage, ThreadState, ToastLevel, ToastPriority, ToastReason};
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::checks::{CheckRun, CheckSpec, gate_reply, short_tree};
use crate::frontier::server::context::{Calls, Link, ServiceContext};
use crate::frontier::server::herdr::{Direction, SplitOptions};
use crate::frontier::server::hunk::{
	ReviewContext, ReviewProof, hunk_diff_args, list_comments, proof_said, review_prompt, review_proof,
};
use crate::frontier::server::mise::{Mise, MiseCheck, MiseCheckTask, mise_check, mise_env, skipped_said, trust_copies};
use crate::frontier::server::report::report_line;
use crate::frontier::server::sessions::{SessionStats, TestsResult, check_exits, checked_tests};
use crate::frontier::server::task_file::{CheckExit, TaskReview};
use crate::frontier::server::thread::{
	Phase, ReviewResult, ReviewRunning, Thread, ThreadRef, ThreadReview, is_idle, set_phase, show, subject,
};
use crate::frontier::server::wording::{HANDOFF_FOCUS, idle_check_text, quoted, review_said, tests_said};
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
/// How long a pane's turn-end check request waits for its check (0617), under the extension's own limit.
const PANE_CHECK_WAIT_MS: i64 = 20_000;
use crate::shared::process::{AbortSignal, aborted as abort_reason};

/// A project pane's check, started and shared: whoever awaits it gets its text; it runs on whether anyone waits.
type Checking = Shared<BoxFuture<'static, String>>;

/// A project pane's check the request stopped waiting for: when the request ends (an answer without it, its role gone,
/// or its future dropped with a disconnected client) while armed, the check goes on in its own task and its failure
/// reaches today and is owed to the pane's next request on the same tree (0617).
struct LateCheck {
	c: std::sync::Arc<ServiceContext>,
	name: String,
	tree: String,
	pane_id: String,
	checking: Checking,
	/// What the check said, when it ended before the request stopped waiting.
	done: Option<String>,
	armed: bool,
}

impl Drop for LateCheck {
	fn drop(&mut self) {
		// Dropped as the runtime ends (the service stopping), there is nothing to go on in: the TypeScript's tail
		// never outlived its process either.
		if !self.armed || tokio::runtime::Handle::try_current().is_err() {
			return;
		}
		let c = self.c.clone();
		let name = std::mem::take(&mut self.name);
		let tree = std::mem::take(&mut self.tree);
		let pane_id = std::mem::take(&mut self.pane_id);
		let (done, checking) = (self.done.take(), self.checking.clone());
		self.c.review.idle_checks.spawn(async move {
			let text = match done {
				Some(text) => text,
				None => checking.await,
			};
			if text.is_empty() || c.stopping() {
				return;
			}
			let short = short_tree(&tree);
			log(LogLevel::Info, "checks", &format!("{name}: checks failed on tree {short} after the wait"), None);
			c.day.record_event(&format!(
				"{name} — checks failed on tree {short} after its turn ended; its agent gets them at its next turn end"
			));
			let mut state = c.review.st();
			let pane = state.pane_checks.entry(pane_id).or_default();
			if pane.tree.as_deref() == Some(tree.as_str()) {
				pane.owed = Some((tree, text));
			}
		});
	}
}

/// What a finished review turn found, before it is said (`ReviewFlow::ready`).
struct ReviewFound {
	notes: Option<usize>,
	blocking: Option<usize>,
	tests: TestsResult,
	proof: Option<ReviewProof>,
	checks: Vec<CheckExit>,
	done_when: Vec<String>,
}

/// Where checks run: a thread's worktree, or a project pane's folder (0617).
#[derive(Debug, Clone)]
struct Place {
	project: crate::shared::projects::Project,
	path: PathBuf,
	title: String,
}

impl Place {
	fn of(thread: &Thread) -> Self {
		Self { project: thread.project.clone(), path: thread.path.clone(), title: thread.title.clone() }
	}
}

/// What a project pane's checks remember (0617).
#[derive(Default)]
struct PaneCheckState {
	/// The tree last checked for the pane: the same tree is never checked or answered twice.
	tree: Option<String>,
	/// A failure that came after the wait, owed to the pane's next request on that tree: the tree and the text.
	owed: Option<(String, String)>,
	/// The pane's last check: the next one runs after it, as runs in one folder must not overlap (`checks.rs`).
	running: Option<Checking>,
}

#[derive(Default)]
struct State {
	/// Each project pane's check state, by pane id (0617).
	pane_checks: HashMap<String, PaneCheckState>,
}

/// How a task's commits are said in its review line (`ReviewFlow::commits_said`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitsSaid {
	pub said: String,
	/// The subject of its last commit, when there is one.
	pub subject: Option<String>,
}

/// The checks of a place: its specs, and why the project's own check can't run (0559).
type ChecksFor = (Vec<CheckSpec>, Option<String>);

/// A word of mise's argv that needs no quoting in a command line.
static PLAIN_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_./:=-]+$").expect("a plain word"));

/// Whether the pane check's role stopped asking.
fn aborted(signal: &Option<AbortSignal>) -> bool {
	signal.as_ref().is_some_and(|signal| signal.borrow().is_some())
}

/// Which mise the service runs (`options.mise`: `None` the one on PATH, `Some(None)` none, or an argv prefix).
fn mise_of(c: &ServiceContext) -> Mise<'_> {
	match &c.options.mise {
		None => Mise::OnPath,
		Some(None) => Mise::Absent,
		Some(Some(argv)) => Mise::Argv(argv),
	}
}

/// A plan waits for his approval on the thread (a title that isn't empty).
fn plan_awaits(thread: &Thread) -> bool {
	thread.plan.as_ref().is_some_and(|plan| plan.awaiting.as_deref().is_some_and(|title| !title.is_empty()))
}

/// `done-when \`cmd\` exit 1` or `not run`: how a `done_when` command's exit reads.
fn exit_said(check: &CheckExit) -> String {
	match check.exit {
		None => format!("`{}` not run", check.command),
		Some(code) => format!("`{}` exit {code}", check.command),
	}
}

pub struct ReviewFlow {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	/// Checks on idle under way, and pane checks that outlast their wait: `stop` waits for them once their checks are
	/// ended.
	idle_checks: Calls,
	/// Ends every pane check's wait under way (`end_pane_waits`, when the service stops).
	pane_waits: Notify,
	state: Mutex<State>,
}

impl ReviewFlow {
	pub fn new(link: Link) -> Self {
		Self { link, idle_checks: Calls::default(), pane_waits: Notify::new(), state: Mutex::new(State::default()) }
	}

	fn st(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// A thread found in herdr, now idle (0394): commits ahead of trunk that no review covered (the service was off
	/// when the task ended) get their review now, said as the commits since its last reviewed one (beta day P3).
	pub async fn catch_up(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		let t = thread.get();
		let (trunk, head) = tokio::try_join!(c.places.trunk(&t.project), git_out(&t.path, &["rev-parse", "HEAD"]))?;
		let (Some(trunk), Some(head)) = (trunk.filter(|trunk| !trunk.is_empty()), head.filter(|head| !head.is_empty()))
		else {
			return Ok(());
		};
		let count = git_out(&t.path, &["rev-list", "--count", &format!("{trunk}..HEAD")]).await?;
		let ahead: u64 = count.and_then(|count| count.parse().ok()).unwrap_or(0);
		let reviewed = c.reviewed.of(&t.path);
		if ahead == 0 || reviewed.as_deref() == Some(head.as_str()) {
			return Ok(());
		}
		let from = match reviewed.filter(|reviewed| !reviewed.is_empty()) {
			Some(reviewed) if git_out(&t.path, &["merge-base", "--is-ancestor", &reviewed, &head]).await?.is_some() => {
				Some(reviewed)
			}
			_ => None,
		};
		thread.with(|t| {
			t.head = Some(head.clone());
			show(t, ThreadState::Done);
		});
		let CommitsSaid { said, subject } = self.commits_said(thread, from.as_deref(), &head).await?;
		self.task_ended(thread, &said, subject.as_deref()).await
	}

	/// What a task's commits from `from` (exclusive; `None`: since trunk) to `to` say in its review line (N-2, beta
	/// day P3): the thread's own commits, a merge and trunk's commits it brought in left out (`2 new commits`), with the
	/// subject of its last one; `<trunk> merged in` when only trunk came in.
	pub async fn commits_said(&self, thread: &ThreadRef, from: Option<&str>, to: &str) -> Result<CommitsSaid> {
		let c = self.link.ctx();
		let t = thread.get();
		let trunk = c.places.trunk(&t.project).await?.filter(|trunk| !trunk.is_empty());
		let mut range = vec![from.map_or_else(|| to.to_owned(), |from| format!("{from}..{to}"))];
		range.extend(trunk.iter().map(|trunk| format!("^{trunk}")));
		let mut count_args = vec!["rev-list".to_owned(), "--count".to_owned(), "--no-merges".to_owned()];
		count_args.extend(range.iter().cloned());
		let count: u64 = git_out(&t.path, &count_args).await?.and_then(|count| count.parse().ok()).unwrap_or(0);
		if count > 0 {
			let mut log_args = ["log", "-1", "--no-merges", "--format=%s"].map(str::to_owned).to_vec();
			log_args.extend(range);
			let subject = git_out(&t.path, &log_args).await?.filter(|subject| !subject.is_empty());
			let plural = if count == 1 { "" } else { "s" };
			return Ok(CommitsSaid { said: format!("{count} new commit{plural}"), subject });
		}
		let said = match (&trunk, from) {
			(Some(trunk), Some(_)) => format!("{trunk} merged in"),
			_ => "new commits".to_owned(),
		};
		Ok(CommitsSaid { said, subject: None })
	}

	/// A thread finished a task (0394, 0406, 0425) with `commits`, the last of them `last`: hunk opens to the right of
	/// its pane at half width on `<trunk>...<branch>` (`--watch`, so a later task's commits show in the same pane), and
	/// the thread is asked to run the project's tests (its trusted mise `verify`/`test` task when it has one, 0473) and
	/// its `verify` skill, then omp's reviewer with the approved plan, the project's rules and lessons (0453, 0459,
	/// 0460), posting its findings into hunk and ending on its test command; the task is announced once, when its review
	/// is ready (UX-21). Without hunk, a trunk or a branch the review is skipped (and its commit counts as reviewed): the
	/// task is announced done and the handoff follows at once (0400).
	pub async fn task_ended(&self, thread: &ThreadRef, commits: &str, last: Option<&str>) -> Result<()> {
		let c = self.link.ctx();
		let t = thread.get();
		let hunk = c.panes.hunk();
		let (trunk, head) = tokio::try_join!(c.places.trunk(&t.project), git_out(&t.path, &["rev-parse", "HEAD"]))?;
		let trunk = trunk.filter(|trunk| !trunk.is_empty());
		let (Some(hunk), Some(trunk), false) = (&hunk, &trunk, t.branch.is_empty()) else {
			let why = if hunk.is_none() {
				"hunk isn't installed"
			} else if trunk.is_none() {
				"the project has no trunk branch"
			} else {
				"the thread has no branch"
			};
			c.warn(Error::other(format!("no review for {}: {why}", t.title)));
			c.reviewed.mark(&t.path, head.as_deref())?;
			self.announce_done(thread, commits).await;
			return self.handoff(thread).await;
		};
		let range = format!("{trunk}...{}", t.branch);
		let at = now_ms();
		// What this review covers, so a second task's line differs from the first's (N-2): `1 new commit «subject»`.
		let covered = match last.filter(|last| !last.is_empty()) {
			Some(last) => format!("{commits} {}", quoted(last)),
			None => commits.to_owned(),
		};
		thread.with(|t| {
			set_phase(t, Phase::Review, "its task ended");
			let running =
				ReviewRunning { head, at, commits: covered, range: range.clone(), checks: None, skipped: None };
			t.review = Some(ThreadReview {
				pane: t.review.as_ref().and_then(|review| review.pane.clone()),
				running: Some(running),
				result: None,
			});
		});
		if let Err(error) = self.begin_review(thread, hunk, &range, at).await {
			c.warn(&error);
			self.review_failed(thread, commits, &error).await?;
		}
		Ok(())
	}

	/// The review's start: the hunk pane beside the thread, the task's checks, then the review turn asked for.
	async fn begin_review(&self, thread: &ThreadRef, hunk: &[String], range: &str, at: i64) -> Result<()> {
		let c = self.link.ctx();
		let t = thread.get();
		let known = t.review.as_ref().and_then(|review| review.pane.clone());
		let open = match &known {
			Some(pane) => c.herdr.panes().await?.iter().any(|open| open.pane_id == *pane),
			None => false,
		};
		if !open {
			// Started from the home folder first, the session daemon never holds the worktree open (F-02).
			c.panes.ensure_hunk_daemon(hunk).await;
			let split = SplitOptions {
				command: hunk_diff_args(hunk, range),
				cwd: t.path.clone(),
				env: Some(c.panes.pane_env().clone()),
				label: Some("review".to_owned()),
				direction: Some(Direction::Right),
				ratio: Some(0.5),
			};
			let pane = c.herdr.split_beside(&t.pane_id, &split).await?;
			self.update_review(thread, at, |review| review.pane = Some(pane));
		}
		// After "Approve and execute" the session is a new one without the plan: the plan recorded at approval (0453, 0540).
		let from_session = match &t.session_path {
			Some(session) => c.sessions.lock().await.read(session, false).plan_file.clone(),
			None => None,
		};
		let plan = from_session.or_else(|| t.task.as_ref().and_then(|record| record.plan.as_ref()?.file.clone()));
		let rules = [t.path.join("AGENTS.md"), t.project.main_path.join("AGENTS.local.md")]
			.into_iter()
			.filter(|file| file.exists())
			.collect();
		// Jocasta's runner checks the task's tree first (0542, 0546): the turn gets the failing gates, nothing on a pass.
		let done_when = t.task.as_ref().map(|record| record.done_when.clone()).unwrap_or_default();
		let place = Place::of(&t);
		let (specs, skipped) = self.checks_for(&place, &done_when).await?;
		let run = self.run_checks(&place, &specs, None).await;
		self.update_review(thread, at, |review| {
			if let Some(running) = review.running.as_mut() {
				if run.is_some() {
					running.checks = Some(specs.clone());
				}
				if skipped.is_some() {
					running.skipped.clone_from(&skipped);
				}
			}
		});
		let record = t.task.as_ref();
		let context = ReviewContext {
			plan: plan.map(PathBuf::from),
			rules,
			gates: run.as_ref().map(gate_reply),
			checked: run.is_some() && specs.iter().any(|spec| spec.mise),
			brief: record.filter(|record| !record.brief.trim().is_empty()).map(|record| record.brief.clone()),
			report: Some(
				record
					.and_then(|record| record.report.as_ref())
					.filter(|_| self.report_in(&t))
					.map(|report| report_line(&report.value)),
			),
		};
		c.bridge.prompt(&t.pane_id, c.bridge.seal(&review_prompt(hunk, range, &context)), Deliver::FollowUp).await?;
		Ok(())
	}

	/// Changes the thread's review step while the review asked for at `at` still runs.
	fn update_review(&self, thread: &ThreadRef, at: i64, change: impl FnOnce(&mut ThreadReview)) {
		thread.with(|t| {
			if let Some(review) = t.review.as_mut()
				&& review.running.as_ref().is_some_and(|running| running.at == at)
			{
				change(review);
			}
		});
	}

	/// The review couldn't start: the thread goes back to its task, a merge waiting on it says it isn't made, the task
	/// is announced done and the handoff follows.
	async fn review_failed(&self, thread: &ThreadRef, commits: &str, error: &Error) -> Result<()> {
		let c = self.link.ctx();
		thread.with(|t| set_phase(t, Phase::Task, "its review couldn't start"));
		if thread.with(|t| std::mem::take(&mut t.merge_after_review)) {
			let what = format!("not merged: its new review couldn't start ({error})");
			c.events.thread(thread, &what, ToastPriority::Normal, None).await;
		}
		self.announce_done(thread, commits).await;
		self.handoff(thread).await
	}

	/// A task done with no review to follow: one line, one toast, one spoken sentence (0405, 0407).
	async fn announce_done(&self, thread: &ThreadRef, commits: &str) {
		let c = self.link.ctx();
		c.events.thread(thread, &format!("done: {commits}"), ToastPriority::Normal, None).await;
		let (text, pane_id) = thread.with(|t| (format!("{} — done: {commits}", subject(t)), t.pane_id.clone()));
		c.events.send(ServerMessage::Toast {
			reason: ToastReason::Done,
			level: ToastLevel::Info,
			text,
			pane_id: Some(pane_id),
			priority: None,
			url: None,
		});
	}

	/// The review turn ended: what it found is collected (`found`), the task is announced done and reviewed in one line,
	/// toast and sentence (0394, 0405, 0425, UX-21), the commit counts as reviewed, then `/handoff`. A review
	/// `thread_finish` asked for after main moved (0454) merges when it passed, else says why not.
	pub async fn ready(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		let found = self.found(thread).await?;
		let said = self.record_result(thread, &found)?;
		let counted = format!("done and reviewed: {said}");
		let (merging, failed) = thread.with(|t| {
			let merging = std::mem::take(&mut t.merge_after_review);
			(merging, if merging { self.failed(t) } else { None })
		});
		// A review `thread_finish` asked for after main moved (0454) is said once, with the merge's outcome (beta day P3).
		if !merging {
			c.events.thread(thread, &counted, ToastPriority::Normal, None).await;
		} else if let Some(failed) = &failed {
			let what = format!("{counted}; not merged: its new review after main moved didn't pass ({failed})");
			c.events.thread(thread, &what, ToastPriority::Normal, None).await;
		}
		let (title, pane_id) = thread.with(|t| (subject(t), t.pane_id.clone()));
		c.events.send(ServerMessage::Toast {
			reason: ToastReason::Done,
			level: ToastLevel::Info,
			text: format!("{title} — {}", counted.replace(['«', '»'], "")),
			pane_id: Some(pane_id),
			priority: None,
			url: None,
		});
		c.status.report().await?;
		if merging && failed.is_none() {
			return c.finisher.after_review(thread, &said).await;
		}
		self.handoff(thread).await
	}

	/// Stores the review's outcome on the thread and its task file, shows it done and counts its commit as reviewed;
	/// the line it says.
	fn record_result(&self, thread: &ThreadRef, found: &ReviewFound) -> Result<String> {
		let c = self.link.ctx();
		let ReviewFound { notes, blocking, tests, proof, checks, done_when } = found;
		let at = now_ms();
		let (running, reported, path) = thread.with(|t| {
			let reported = self.report_in(t);
			let running = t.review.as_ref().and_then(|review| review.running.clone());
			let result = ReviewResult {
				notes: *notes,
				blocking: *blocking,
				tests: *tests,
				proof: proof.clone(),
				checks: checks.clone(),
				at: Some(at),
			};
			t.review = Some(ThreadReview {
				pane: t.review.as_ref().and_then(|review| review.pane.clone()),
				running: None,
				result: Some(result),
			});
			show(t, ThreadState::Done);
			(running, reported, t.path.clone())
		});
		c.reviewed.mark(&path, running.as_ref().and_then(|running| running.head.as_deref()))?;
		// The project's check that couldn't run (0559): said beside the review's verdict, never a silent pass.
		let skipped = running.as_ref().and_then(|running| running.skipped.as_deref()).map(skipped_said);
		let done_when_said = (!done_when.is_empty()).then(|| {
			let own = &checks[checks.len().saturating_sub(done_when.len())..];
			format!("done-when {}", own.iter().map(exit_said).collect::<Vec<_>>().join(", "))
		});
		let verdict = review_said(*notes, *tests, *blocking);
		let said: Vec<String> = [
			running.map(|running| running.commits),
			proof.as_ref().map(proof_said),
			Some(verdict.clone()),
			done_when_said,
			skipped.clone(),
			(!reported).then(|| "no report".to_owned()),
		]
		.into_iter()
		.flatten()
		.filter(|part| !part.is_empty())
		.collect();
		let has_task = thread.with(|t| {
			let passed = self.failed(t).is_none();
			let Some(task) = t.task.as_mut() else { return false };
			let said_in_task: Vec<String> =
				[Some(verdict), skipped, (!reported).then(|| "no report".to_owned())].into_iter().flatten().collect();
			task.reviews.push(TaskReview {
				at,
				passed,
				checks: checks.clone(),
				said: said_in_task.join(", "),
				// What a restart gives the thread back as its last review (`Refresher::discover`).
				notes: *notes,
				blocking: *blocking,
				tests: Some(*tests),
				proof: proof.clone(),
			});
			true
		});
		if has_task {
			c.status.write_task(thread);
		}
		Ok(said.join(", "))
	}

	/// What the review turn found: hunk's notes since the review was asked for, counted with the blocking ones (a count
	/// hunk can't give is said, not hidden, F-03); the tests checked against the turn's bash results (0453); the diff's
	/// size and test edits from git; and the checks: the runner's on the reviewed tree (its stored result when the turn
	/// changed nothing), the project's check first, then each done_when (0453, 0540, 0542); without the project's check,
	/// the test command the turn named and its `TESTS:` line, as the session's bash runs show them.
	async fn found(&self, thread: &ThreadRef) -> Result<ReviewFound> {
		let c = self.link.ctx();
		let t = thread.get();
		let running = t.review.as_ref().and_then(|review| review.running.clone());
		let since = running.as_ref().map_or(0, |running| running.at);
		let (mut notes, mut blocking) = (None, None);
		let counted = match c.panes.hunk() {
			Some(hunk) => list_comments(&hunk, &t.path, since, c.panes.hunk_port()).await,
			None => Err(Error::other("hunk isn't installed")),
		};
		match counted {
			Ok(counts) => (notes, blocking) = (Some(counts.notes), Some(counts.blocking)),
			Err(error) => c.warn(Error::other(format!("{}'s review notes weren't counted: {error}", t.title))),
		}
		let stats = match &t.session_path {
			Some(session) => Some(c.sessions.lock().await.read(session, false).clone()),
			None => None,
		};
		let proof = match running.as_ref().map(|running| running.range.as_str()) {
			Some(range) => review_proof(&t.path, range).await?,
			None => None,
		};
		let done_when = t.task.as_ref().map(|record| record.done_when.clone()).unwrap_or_default();
		let run = match running.as_ref().and_then(|running| running.checks.as_ref()) {
			Some(specs) => self.run_checks(&Place::of(&t), specs, None).await,
			None => None,
		};
		let project = run.as_ref().and_then(|run| run.results.iter().find(|result| result.spec.mise));
		let tests = match (project, &stats) {
			(Some(project), _) if project.gates.iter().all(|gate| gate.pass) => TestsResult::Pass,
			(Some(_), _) => TestsResult::Fail,
			(None, Some(stats)) => checked_tests(stats, since),
			(None, None) => TestsResult::Unknown,
		};
		let named = stats
			.as_ref()
			.and_then(|stats| stats.tests.as_ref())
			.filter(|reported| reported.at >= since)
			.and_then(|reported| reported.command.clone());
		let checks = Self::checks_run(run.as_ref(), stats.as_ref(), named, &done_when, since);
		Ok(ReviewFound { notes, blocking, tests, proof, checks, done_when })
	}

	/// The checks a review reports with their exits: the runner's results (the project's check first, then each
	/// done_when), else what the session's bash runs show of the named test command and each done_when.
	fn checks_run(
		run: Option<&CheckRun>,
		stats: Option<&SessionStats>,
		named: Option<String>,
		done_when: &[String],
		since: i64,
	) -> Vec<CheckExit> {
		let exits = |stats: &SessionStats, commands: &[String]| -> Vec<CheckExit> {
			check_exits(stats, commands, since)
				.into_iter()
				.map(|(command, exit)| CheckExit { command, exit: exit.and_then(|exit| i32::try_from(exit).ok()) })
				.collect()
		};
		let commands: Vec<String> = named.iter().chain(done_when).cloned().collect();
		match (run, stats) {
			(Some(run), _) => {
				let has_project = run.results.iter().any(|result| result.spec.mise);
				let mut checks = match (has_project, &named, stats) {
					(false, Some(named), Some(stats)) => exits(stats, std::slice::from_ref(named)),
					_ => Vec::new(),
				};
				checks.extend(
					run.results
						.iter()
						.map(|result| CheckExit { command: result.spec.command.clone(), exit: result.exit }),
				);
				checks
			}
			(None, Some(stats)) => exits(stats, &commands),
			(None, None) => commands.into_iter().map(|command| CheckExit { command, exit: None }).collect(),
		}
	}

	/// Why the thread's last review wasn't a pass (0453, 0610): its tests didn't pass by their run, the reviewer marked
	/// notes blocking (P0/P1), the diff weakens the tests (a removed test, fewer assertions in a test file, an added
	/// skip/only; proofs stored before 0610 lack the first two), or a done_when command failed; `None` when it passed or
	/// no review result is known here.
	pub fn failed(&self, thread: &Thread) -> Option<String> {
		let result = thread.review.as_ref()?.result.as_ref()?;
		// A done_when command that didn't exit 0 in the review turn (0540): the task's own definition of done.
		let done_when: &[String] = thread.task.as_ref().map_or(&[], |record| record.done_when.as_slice());
		let unmet = result.checks.iter().filter(|check| done_when.contains(&check.command) && check.exit != Some(0));
		let weakened = |label: &str, files: Option<&Vec<String>>| {
			files.filter(|files| !files.is_empty()).map(|files| format!("{label} in {}", files.join(", ")))
		};
		let proof = result.proof.as_ref();
		let why: Vec<String> = [
			(!matches!(result.tests, TestsResult::Pass | TestsResult::None))
				.then(|| tests_said(result.tests).to_owned()),
			result.notes.is_none().then(|| "notes not counted".to_owned()),
			result.blocking.filter(|&count| count > 0).map(|count| {
				let plural = if count == 1 { "" } else { "s" };
				format!("{count} blocking note{plural}")
			}),
			weakened("removed tests", proof.and_then(|proof| proof.removed_tests.as_ref())),
			weakened("fewer assertions", proof.and_then(|proof| proof.fewer_assertions.as_ref())),
			weakened("added skip/only", proof.map(|proof| &proof.skipped)),
		]
		.into_iter()
		.flatten()
		.chain(unmet.map(|check| format!("done-when {}", exit_said(check))))
		.collect();
		(!why.is_empty()).then(|| why.join(", "))
	}

	/// Whether the thread handed in its report for the task now ending: one after its last review (0539).
	pub fn report_in(&self, thread: &Thread) -> bool {
		let Some(record) = thread.task.as_ref() else { return false };
		let last = record.reviews.last().map_or(0, |review| review.at);
		record.report.as_ref().is_some_and(|report| report.at > last)
	}

	/// The checks of `place`, a thread's worktree or a project pane's folder (0453, 0473, 0540, 0542, 0617): the project's
	/// own (its trusted mise `verify`/`test` task, run as `mise run -c <task>` so a failing task doesn't stop the others,
	/// 0559) and the `done_when` commands, with why the project's own can't run when mise may not load its config (0559).
	/// The one place checks are decided: the review, the check on idle, the project pane's check and the `check` tool all
	/// run these through the runner. The folder's unchanged copies of trusted mise configs are trusted first (0596), so a
	/// copy a merge brought in from trunk is trusted again here.
	async fn checks_for(&self, place: &Place, done_when: &[String]) -> Result<ChecksFor> {
		let c = self.link.ctx();
		let mise = mise_of(&c);
		let env = c.panes.check_env();
		let own = !matches!(mise, Mise::Absent);
		if own {
			trust_copies(&place.project.id, &place.project.main_path, &place.path, Some(&env), mise).await;
		}
		let project = if own { mise_check(&place.project.id, &place.path, Some(&env), mise).await } else { None };
		let words: Vec<String> = match mise {
			Mise::Argv(argv) => argv.to_vec(),
			_ => vec!["mise".to_owned()],
		};
		let argv: Vec<String> = words
			.into_iter()
			.map(|word| if PLAIN_WORD.is_match(&word) { word } else { format!("\"{word}\"") })
			.collect();
		let (task, skipped) = match project {
			Some(MiseCheck::Task(MiseCheckTask::Verify)) => (Some("verify"), None),
			Some(MiseCheck::Task(MiseCheckTask::Test)) => (Some("test"), None),
			Some(MiseCheck::Skipped(why)) => (None, Some(why)),
			None => (None, None),
		};
		let mut specs: Vec<CheckSpec> = task
			.map(|task| CheckSpec {
				name: task.to_owned(),
				command: format!("{} run -c {task}", argv.join(" ")),
				mise: true,
			})
			.into_iter()
			.collect();
		specs.extend(done_when.iter().enumerate().map(|(i, command)| CheckSpec {
			name: if i == 0 { "done_when".to_owned() } else { format!("done_when-{}", i + 1) },
			command: command.clone(),
			mise: false,
		}));
		Ok((specs, skipped))
	}

	/// Runs `specs` on `place`'s folder through the runner (0542, 0559), with the env its panes get (mise's included,
	/// Jocasta's own keys left out), on `tree` when the caller already keyed it (SRV-B-17); `None` without specs,
	/// outside git, or when the runner fails (logged).
	async fn run_checks(&self, place: &Place, specs: &[CheckSpec], tree: Option<&str>) -> Option<CheckRun> {
		if specs.is_empty() {
			return None;
		}
		let c = self.link.ctx();
		let mut env = c.panes.check_env();
		let own = match mise_of(&c) {
			Mise::Absent => HashMap::new(),
			mise => mise_env(&place.project.id, &place.path, Some(&env), mise).await,
		};
		env.extend(own);
		match c.checks.run(&place.path, specs, &env, tree).await {
			Ok(run) => {
				if let Some(run) = &run {
					c.thread_facts.note_check(&place.path, run.passed, c.day.now_ms());
				}
				run
			}
			Err(error) => {
				c.warn(Error::other(format!("{}'s checks didn't run: {error}", place.title)));
				None
			}
		}
	}

	/// Starts the check on idle beside the pane's state chain: a long check never holds its next state change. None with
	/// the layer off (0576).
	pub fn start_idle_check(&self, thread: &ThreadRef) {
		let c = self.link.ctx();
		if !c.layer {
			return;
		}
		let thread = thread.clone();
		self.idle_checks.spawn(async move {
			if let Err(error) = c.review.idle_check(&thread).await {
				c.warn(error);
			}
		});
	}

	/// Waits for the checks on idle still under way (`stop`, once the runner ended their checks).
	pub async fn settled(&self) {
		self.idle_checks.settled().await;
	}

	/// Ends the pane checks' waits, so their requests answer nothing at once (`stop`); their checks end with the runner.
	pub fn end_pane_waits(&self) {
		self.pane_waits.notify_waiters();
	}

	/// The check on idle (0513, v1 by 0542): a thread whose turn ended with edits not yet checked has its checks run on
	/// its tree; failing gates go into its pane under 0583's frame, a pass says nothing. A tree already checked, or one
	/// with no edits since the last commit, costs the tree key only: mise is asked for the checks after (SVC-10).
	async fn idle_check(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		let t = thread.get();
		if t.phase != Phase::Task || plan_awaits(&t) {
			return Ok(());
		}
		let Some(tree) = c.checks.tree_key(&t.path).await? else { return Ok(()) };
		if !self.unchecked(&t.path, &tree, t.idle_tree.as_deref()).await? {
			return Ok(());
		}
		let done_when = t.task.as_ref().map(|record| record.done_when.clone()).unwrap_or_default();
		let place = Place::of(&t);
		let (specs, _) = self.checks_for(&place, &done_when).await?;
		if specs.is_empty() {
			return Ok(());
		}
		thread.with(|t| t.idle_tree = Some(tree.clone()));
		let Some(run) = self.run_checks(&place, &specs, Some(&tree)).await else { return Ok(()) };
		if run.passed || c.stopping() || !c.threads.current(thread) {
			return Ok(());
		}
		// Only into a thread still idle on that tree: a turn since, or a plan or question waiting, leaves it alone.
		let now = thread.get();
		if now.phase != Phase::Task || plan_awaits(&now) || !is_idle(now.state) || c.lifecycle.busy(&now.pane_id) {
			return Ok(());
		}
		let message = format!("{}: checks failed on tree {}", subject(&now), short_tree(&run.tree));
		log(LogLevel::Info, "checks", &message, None);
		c.bridge.prompt(&now.pane_id, c.bridge.seal(&idle_check_text(&gate_reply(&run))), Deliver::FollowUp).await?;
		Ok(())
	}

	/// Whether `tree`, the tree of the folder `path` as it is now, has edits not yet checked (0513, 0617): it differs from
	/// `last`, the tree last checked there, and from the tree of HEAD (no edits since the last commit). A tree already
	/// checked, or one with no edits, costs the tree key only.
	async fn unchecked(&self, path: &Path, tree: &str, last: Option<&str>) -> Result<bool> {
		if last == Some(tree) {
			return Ok(false);
		}
		Ok(git_out(path, &["rev-parse", "HEAD^{tree}"]).await?.as_deref() != Some(tree))
	}

	/// A project pane's check when its turn ends (0617), asked by the bridge extension as a `stop` request (0675): the
	/// threads' rule on the pane's own folder. A tree with edits not yet checked has the project's check run (no
	/// `done_when`), after the pane's previous check; a failure within the wait, counted from the request, is the answer,
	/// in the threads' frame text, once per tree. A check still running when the wait ends, or when `signal` says the
	/// pane stopped asking, goes on: if it then fails, `today` says so and the failure is owed to the pane's next request
	/// on the same tree, once; a tree that changed meanwhile drops it. A pass says nothing; so does a pane that isn't a
	/// project pane's (a thread's, the boss's, an unknown key) and the layer off (0576). The tree is noted before mise is
	/// asked, so a project with no check costs the tree key alone until it changes. Empty: nothing to say.
	pub async fn pane_check(&self, pane_key: &str, signal: Option<AbortSignal>) -> Result<String> {
		let c = self.link.ctx();
		let ends = now_ms() + c.options.pane_check_wait_ms.unwrap_or(PANE_CHECK_WAIT_MS);
		let Some(pane_id) = c.launched.pane_of_key(Some(pane_key)) else { return Ok(String::new()) };
		let Some(path) = c.launched.cwd_of(&pane_id).map(PathBuf::from) else { return Ok(String::new()) };
		let project = if c.layer { c.places.project_pane_at(&path)? } else { None };
		let Some(project) = project else { return Ok(String::new()) };
		let Some(tree) = c.checks.tree_key(&path).await? else { return Ok(String::new()) };
		let owed = self.st().pane_checks.entry(pane_id.clone()).or_default().owed.take();
		if let Some((owed_tree, text)) = owed
			&& owed_tree == tree
		{
			return Ok(text);
		}
		let last = self.st().pane_checks.entry(pane_id.clone()).or_default().tree.replace(tree.clone());
		if !self.unchecked(&path, &tree, last.as_deref()).await? {
			return Ok(String::new());
		}
		let name = project.name.clone();
		let checking = self.start_pane_check(&pane_id, Place { title: name.clone(), project, path }, &tree);
		let mut late =
			LateCheck { c: c.clone(), name, tree, pane_id, checking: checking.clone(), done: None, armed: true };
		let done = self.within(checking, ends, signal.clone()).await;
		if let Some(text) = &done
			&& !aborted(&signal)
		{
			late.armed = false;
			return Ok(text.clone());
		}
		late.done = done;
		Ok(String::new())
	}

	/// Starts the pane's check of `tree` after the pane's previous one: the tree replaced while it waited is not run.
	fn start_pane_check(&self, pane_id: &str, place: Place, tree: &str) -> Checking {
		let c = self.link.ctx();
		let previous = self.st().pane_checks.get(pane_id).and_then(|state| state.running.clone());
		let (pane, key) = (pane_id.to_owned(), tree.to_owned());
		let task = tokio::spawn(async move {
			if let Some(previous) = previous {
				previous.await;
			}
			let current = c.review.st().pane_checks.get(&pane).and_then(|state| state.tree.clone());
			if current.as_deref() == Some(key.as_str()) {
				c.review.check_tree(&place, &key).await
			} else {
				String::new()
			}
		});
		let checking: Checking = async move { task.await.unwrap_or_default() }.boxed().shared();
		self.st().pane_checks.entry(pane_id.to_owned()).or_default().running = Some(checking.clone());
		checking
	}

	/// `checking`'s text if it ends before `ends` (a time), `signal` aborts or the service stops; else `None`.
	async fn within(&self, checking: Checking, ends: i64, signal: Option<AbortSignal>) -> Option<String> {
		let notified = self.pane_waits.notified();
		tokio::pin!(notified);
		notified.as_mut().enable();
		let wait = Duration::from_millis(u64::try_from((ends - now_ms()).max(0)).unwrap_or(0));
		let mut signal = signal;
		let left = abort_reason(&mut signal);
		// Biased, the stop and the role leaving first: stopping kills the running check, which then ends with a failure
		// in the same instant, and that failure is no answer to give.
		tokio::select! {
			biased;
			() = &mut notified => None,
			_ = left => None,
			text = checking => Some(text),
			() = tokio::time::sleep(wait) => None,
		}
	}

	/// `place`'s project check on `tree` through the runner: the frame text of its failing gates, "" on a pass or no check.
	async fn check_tree(&self, place: &Place, tree: &str) -> String {
		let c = self.link.ctx();
		let specs = match self.checks_for(place, &[]).await {
			Ok((specs, _)) => specs,
			Err(error) => {
				c.warn(error);
				return String::new();
			}
		};
		match self.run_checks(place, &specs, Some(tree)).await {
			Some(run) if !run.passed => idle_check_text(&gate_reply(&run)),
			_ => String::new(),
		}
	}

	/// The `check` thread tool (0542, 0546): the task's checks on the thread's tree, as gates; one line on a pass. A
	/// project check that couldn't run is said (0559).
	pub async fn check_tool(&self, pane_id: Option<&str>, unkeyed: &str) -> Result<String> {
		let c = self.link.ctx();
		let Some(thread) = pane_id.and_then(|pane| c.threads.get(pane)) else {
			return Err(Error::user(format!(
				"check: only a thread's pane runs its checks, and this pane isn't a thread's{unkeyed}."
			)));
		};
		let t = thread.get();
		let done_when = t.task.as_ref().map(|record| record.done_when.clone()).unwrap_or_default();
		let place = Place::of(&t);
		let (specs, skipped) = self.checks_for(&place, &done_when).await?;
		if specs.is_empty() {
			return Ok(match skipped {
				Some(why) => format!("No checks: {}, and the task has no done_when.", skipped_said(&why)),
				None => {
					"No checks: the project has no trusted mise verify or test task, and the task has no done_when."
						.to_owned()
				}
			});
		}
		let Some(run) = self.run_checks(&place, &specs, None).await else {
			return Err(Error::user("check: the checks couldn't run here (see Jocasta's log)."));
		};
		let reply = gate_reply(&run);
		let said = if reply.is_empty() { format!("checks: PASS (tree {})", short_tree(&run.tree)) } else { reply };
		Ok(match skipped {
			Some(why) => format!("{said}\nNote: {}.", skipped_said(&why)),
			None => said,
		})
	}

	/// Asks the thread for `/handoff` (0400), so the task leaves one summary for the watcher. HEAD is noted first:
	/// whatever the review committed is not the next task's work, and a later move means a new task ran (CO-4).
	pub async fn handoff(&self, thread: &ThreadRef) -> Result<()> {
		let c = self.link.ctx();
		thread.with(|t| {
			set_phase(t, Phase::Handoff, "it is asked for /handoff");
			t.handoff_at = Some(now_ms());
		});
		let (path, pane_id) = thread.with(|t| (t.path.clone(), t.pane_id.clone()));
		let head = git_out(&path, &["rev-parse", "HEAD"]).await?;
		thread.with(|t| t.head = head);
		if let Err(error) = self.send_handoff(&pane_id).await {
			thread.with(|t| set_phase(t, Phase::Task, "it didn't take /handoff"));
			c.warn(error);
		}
		Ok(())
	}

	/// `/handoff` to the omp in `pane_id` (0400): as `ctx.compact` in handoff mode with the focus as its guidance, so omp runs
	/// it itself (0675); an omp that can't compact (0684) gets `/handoff <focus>` as a prompt, which omp's own input
	/// doesn't run as a command, and the degrade is logged.
	async fn send_handoff(&self, pane_id: &str) -> Result<()> {
		let c = self.link.ctx();
		if c.bridge.compact(pane_id, c.bridge.seal(HANDOFF_FOCUS)).await? {
			return Ok(());
		}
		log(
			LogLevel::Warn,
			"threads",
			&format!("omp in {pane_id} can't compact for Jocasta; /handoff goes as a prompt"),
			None,
		);
		c.bridge.prompt(pane_id, c.bridge.seal(&format!("/handoff {HANDOFF_FOCUS}")), Deliver::FollowUp).await
	}
}

#[cfg(test)]
#[path = "review_flow_tests.rs"]
mod tests;
