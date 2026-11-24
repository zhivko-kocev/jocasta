//! What the dock says of each open thread beyond its state (0668): lines changed against trunk, the last check's result
//! and the last write to its session file (stuck when it is old), plus the cache of PR facts for landed threads.
//!
//! `get` is a sync cheap read for `status.rs::day_thread`; the facts are refreshed on the service's refresh tick
//! (`refresh`), git and gh run off the event loop and nothing here blocks a read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::frontier::protocol::{PrCi, PrFacts, ThreadDiff};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::task_file::CheckExit;
use crate::frontier::server::worktrees::git_out;
use crate::shared::log::{LogLevel, log};

const MINUTE: i64 = 60_000;
/// A thread's diff is read again after this long at the soonest: it runs git in its worktree.
const DIFF_EVERY_MS: i64 = 20_000;
/// A PR still pending is asked again after this long (0668).
const PR_PENDING_MS: i64 = 2 * MINUTE;
/// A PR whose checks settled is asked again after this long; a merged or closed one never.
const PR_SETTLED_MS: i64 = 10 * MINUTE;
/// A failing `gh` is asked again after 2 minutes, doubling up to this.
const PR_BACKOFF_MAX_MS: i64 = 30 * MINUTE;
/// At most this many PRs are watched; the one asked about longest ago is dropped for a new one.
const PR_MAX: usize = 10;

/// What is known of one thread's worktree.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
	/// Lines added and removed against the merge base with trunk; none while nothing changed or git didn't say.
	pub diff: Option<ThreadDiff>,
	/// The last check: `Some(true)` passed, `Some(false)` failed, `None` none yet.
	pub check: Option<bool>,
	/// The session file's last write, epoch ms.
	pub last_write_ms: Option<i64>,
}

#[derive(Default)]
struct Entry {
	facts: Facts,
	/// When the diff was last read.
	diff_at: Option<i64>,
	/// The last check run in this worktree this process saw, and when.
	noted: Option<(bool, i64)>,
}

struct PrEntry {
	facts: Option<PrFacts>,
	/// Not asked before this time (epoch ms).
	next_at: i64,
	errors: u32,
	/// Merged or closed: never asked again.
	settled: bool,
	touched: u64,
}

#[derive(Default)]
struct State {
	threads: HashMap<PathBuf, Entry>,
	prs: HashMap<String, PrEntry>,
	clock: u64,
}

/// Each open thread's facts and the watched PRs; shared by the service's parts.
#[derive(Default)]
pub struct ThreadFacts {
	state: Mutex<State>,
	refreshing: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The lines of `git diff --shortstat`: ` 3 files changed, 84 insertions(+), 12 deletions(-)`; none when nothing changed.
pub fn parse_shortstat(text: &str) -> Option<ThreadDiff> {
	let count = |word: &str| -> u32 {
		text.split(',')
			.find(|part| part.contains(word))
			.and_then(|part| part.split_whitespace().next())
			.and_then(|n| n.parse().ok())
			.unwrap_or(0)
	};
	let (added, removed) = (count("insertion"), count("deletion"));
	(added > 0 || removed > 0).then_some(ThreadDiff { added, removed })
}

/// Lines changed in `worktree` against its merge base with `trunk`, uncommitted work included.
async fn diff_of(worktree: &Path, trunk: &str) -> Option<ThreadDiff> {
	let base =
		git_out(worktree, &["merge-base", trunk, "HEAD"]).await.ok().flatten().filter(|base| !base.is_empty())?;
	let stat = git_out(worktree, &["diff", "--shortstat", base.as_str()]).await.ok().flatten()?;
	parse_shortstat(&stat)
}

/// A review's checks as one result: passed when every one that ran exited 0; none without checks.
pub fn check_of(checks: &[CheckExit]) -> Option<bool> {
	(!checks.is_empty()).then(|| checks.iter().all(|check| check.exit == Some(0)))
}

/// A block of facts this long goes whole into the thread's `facts.md`, and the prompt carries its first lines (0685).
const FACTS_BLOCK_MAX: usize = 60;
/// How many lines of such a block the prompt carries.
const FACTS_PROMPT_LINES: usize = 40;
/// The most commits and changed files a review's facts list.
const FACTS_COMMITS: usize = 30;
const FACTS_FILES: usize = 60;

/// A check as the review's facts say it: its command and exit (`None`: not run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFact {
	pub command: String,
	pub exit: Option<i32>,
}

/// The first line of every facts block (0685).
pub(super) fn facts_heading(sha: &str) -> String {
	format!("Facts as of {sha} (computed by Jocasta; don't re-run git status, git log or git diff --stat for them):")
}

/// `HEAD` of `worktree` as 7 hex, or `unknown` when git doesn't say.
async fn head7(worktree: &Path) -> String {
	git_out(worktree, &["rev-parse", "--short=7", "HEAD"])
		.await
		.ok()
		.flatten()
		.filter(|sha| !sha.is_empty())
		.unwrap_or_else(|| "unknown".to_owned())
}

/// What a new thread's brief carries (0685): its branch, trunk, `HEAD` and which of the mise tasks `quick`, `verify` and
/// `test` the project has.
pub async fn start_facts(worktree: &Path, trunk: Option<&str>, branch: &str, tasks: &[String]) -> String {
	let sha = head7(worktree).await;
	let tasks = if tasks.is_empty() { "none of quick, verify, test".to_owned() } else { tasks.join(", ") };
	[
		facts_heading(&sha),
		format!("- branch: {branch}"),
		format!("- trunk: {}", trunk.filter(|trunk| !trunk.is_empty()).unwrap_or("none")),
		format!("- HEAD: {sha}"),
		format!("- mise tasks: {tasks}"),
	]
	.join("\n")
}

/// A block as the prompt carries it: whole when it is at most 60 lines; else its first 40 lines and `Full facts: <path>`,
/// the whole block written to `file` (when that can't be written the cut block says so).
fn carried(block: Vec<String>, file: Option<&Path>) -> String {
	if block.len() <= FACTS_BLOCK_MAX {
		return block.join("\n");
	}
	let written = file.and_then(|file| {
		crate::shared::fsx::write_atomically(file, format!("{}\n", block.join("\n")), None).ok().map(|()| file)
	});
	let mut shown: Vec<String> = block.into_iter().take(FACTS_PROMPT_LINES).collect();
	match written {
		Some(file) => shown.push(format!("Full facts: {}", file.display())),
		None => shown.push("Full facts: not written".to_owned()),
	}
	shown.join("\n")
}

/// What a review is given (0685): up to 30 commits of the range (`git log --format="%h %s"`), up to 60 changed files
/// (`git diff --numstat`, then `… <n> more files`) and each check's command and exit with the tree key. `range` is the
/// review's `<trunk>...<branch>`; the commits are the branch's own. A block over 60 lines is written to `file`.
pub async fn review_facts(
	worktree: &Path,
	range: &str,
	checks: &[CheckFact],
	tree: Option<&str>,
	file: Option<&Path>,
) -> String {
	let sha = head7(worktree).await;
	let commits_range = range.replace("...", "..");
	let log = git_out(worktree, &["log", "--format=%h %s", &commits_range]).await.ok().flatten().unwrap_or_default();
	let stat = git_out(worktree, &["diff", "--numstat", range]).await.ok().flatten().unwrap_or_default();
	let commits: Vec<&str> = log.lines().filter(|line| !line.is_empty()).collect();
	let files: Vec<&str> = stat.lines().filter(|line| !line.is_empty()).collect();
	let mut block = vec![facts_heading(&sha), format!("Commits ({}):", commits.len())];
	block.extend(commits.iter().take(FACTS_COMMITS).map(|line| format!("  {line}")));
	if commits.len() > FACTS_COMMITS {
		block.push(format!("  … {} more commits", commits.len() - FACTS_COMMITS));
	}
	block.push(format!("Changed files, added/removed ({}):", files.len()));
	block.extend(files.iter().take(FACTS_FILES).map(|line| format!("  {}", line.replace('\t', " "))));
	if files.len() > FACTS_FILES {
		block.push(format!("  … {} more files", files.len() - FACTS_FILES));
	}
	if !checks.is_empty() {
		let at = tree.map(|tree| format!(" on tree {}", tree.chars().take(6).collect::<String>())).unwrap_or_default();
		block.push(format!("Checks{at}:"));
		block.extend(checks.iter().map(|check| match check.exit {
			Some(code) => format!("  `{}` exit {code}", check.command),
			None => format!("  `{}` not run", check.command),
		}));
	}
	carried(block, file)
}

fn mtime_ms(file: &Path) -> Option<i64> {
	let modified = std::fs::metadata(file).ok()?.modified().ok()?;
	i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

/// A PR's CI from `gh pr view --json statusCheckRollup`: failing when any check failed, pending while any hasn't
/// finished (or there are none yet), else passing.
fn rollup_ci(rollup: Option<&Value>) -> PrCi {
	let items = rollup.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
	let mut pending = items.is_empty();
	for item in items {
		let text = |key: &str| item.get(key).and_then(Value::as_str).filter(|text| !text.is_empty());
		// A check run has `conclusion` once complete; a status context has `state`.
		match text("conclusion").or_else(|| text("state")) {
			Some("FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE") => {
				return PrCi::Failing;
			}
			Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => {}
			_ => pending = true,
		}
	}
	if pending { PrCi::Pending } else { PrCi::Passing }
}

/// A PR's facts from `gh pr view <url> --json number,state,statusCheckRollup`.
pub fn pr_from_json(text: &str) -> Option<PrFacts> {
	let parsed: Value = serde_json::from_str(text).ok()?;
	let number = parsed.get("number")?.as_u64()?;
	let ci = match parsed.get("state")?.as_str()? {
		"MERGED" => PrCi::Merged,
		"CLOSED" => PrCi::Closed,
		_ => rollup_ci(parsed.get("statusCheckRollup")),
	};
	Some(PrFacts { number, ci })
}

/// How long a PR with this CI state waits before it is asked again; `None` for merged and closed (never).
fn pr_ttl(ci: PrCi) -> Option<i64> {
	match ci {
		PrCi::Pending => Some(PR_PENDING_MS),
		PrCi::Passing | PrCi::Failing => Some(PR_SETTLED_MS),
		PrCi::Merged | PrCi::Closed => None,
	}
}

/// The wait after the `errors`-th failure in a row: 2, 4, 8, 16, then 30 minutes.
fn pr_backoff(errors: u32) -> i64 {
	let exponent = errors.saturating_sub(1).min(8);
	(PR_PENDING_MS << exponent).min(PR_BACKOFF_MAX_MS)
}

/// Ends a refresh however it ends, so the next one can run.
struct Reading<'a>(&'a AtomicBool);

impl Drop for Reading<'_> {
	fn drop(&mut self) {
		self.0.store(false, Ordering::SeqCst);
	}
}

impl ThreadFacts {
	/// Nothing known yet: every thread's facts come with the next refresh.
	pub fn new() -> Self {
		Self::default()
	}

	/// What is known of `worktree`'s thread; nothing known yet reads as none.
	pub fn get(&self, worktree: &Path) -> Facts {
		lock(&self.state).threads.get(worktree).map(|entry| entry.facts.clone()).unwrap_or_default()
	}

	/// A check just ran in `worktree` (`ReviewFlow::run_checks`): its result shows at once.
	pub fn note_check(&self, worktree: &Path, passed: bool, at_ms: i64) {
		let mut state = lock(&self.state);
		let entry = state.threads.entry(worktree.to_path_buf()).or_default();
		entry.noted = Some((passed, at_ms));
		entry.facts.check = Some(passed);
	}

	/// A PR's facts from the cache; the first ask puts it on the watch list (it answers `None` until fetched).
	pub fn pr_facts(&self, url: &str) -> Option<PrFacts> {
		let mut state = lock(&self.state);
		state.clock += 1;
		let touched = state.clock;
		if let Some(entry) = state.prs.get_mut(url) {
			entry.touched = touched;
			return entry.facts.clone();
		}
		if state.prs.len() >= PR_MAX
			&& let Some(oldest) = state.prs.iter().min_by_key(|(_, entry)| entry.touched).map(|(url, _)| url.clone())
		{
			state.prs.remove(&oldest);
		}
		state.prs.insert(url.to_owned(), PrEntry { facts: None, next_at: 0, errors: 0, settled: false, touched });
		None
	}

	/// The watched PRs to ask about now.
	fn prs_due(&self, now: i64) -> Vec<String> {
		let state = lock(&self.state);
		state
			.prs
			.iter()
			.filter(|(_, entry)| !entry.settled && entry.next_at <= now)
			.map(|(url, _)| url.clone())
			.collect()
	}

	/// What one ask about `url` came to: the facts with their next time, or a failure backing off (the last good facts
	/// stay). Whether this failure is the first of a run, to be logged once.
	fn pr_answered(&self, url: &str, answer: Option<PrFacts>, now: i64) -> bool {
		let mut state = lock(&self.state);
		let Some(entry) = state.prs.get_mut(url) else { return false };
		match answer {
			Some(facts) => {
				entry.errors = 0;
				entry.settled = pr_ttl(facts.ci).is_none();
				entry.next_at = now + pr_ttl(facts.ci).unwrap_or(0);
				entry.facts = Some(facts);
				false
			}
			None => {
				entry.errors += 1;
				entry.next_at = now + pr_backoff(entry.errors);
				entry.errors == 1
			}
		}
	}

	/// Reads each open thread's diff and last write, and the watched PRs; forgets the threads that are gone. One
	/// refresh at a time; git and gh run off the event loop.
	pub async fn refresh(&self, c: &ServiceContext) {
		if self.refreshing.swap(true, Ordering::SeqCst) {
			return;
		}
		let reading = Reading(&self.refreshing);
		let now = c.day.now_ms();
		let mut open = Vec::new();
		for thread in c.threads.all() {
			let (path, project, session, checks) = thread.with(|t| {
				let checks =
					t.review.as_ref().and_then(|review| review.result.as_ref()).map(|result| result.checks.clone());
				(t.path.clone(), t.project.clone(), t.session_path.clone(), checks)
			});
			let last_write_ms = session.as_deref().and_then(mtime_ms);
			let due = lock(&self.state)
				.threads
				.get(&path)
				.is_none_or(|entry| entry.diff_at.is_none_or(|at| now - at >= DIFF_EVERY_MS || now < at));
			let diff = if due {
				match c.places.trunk(&project).await {
					Ok(Some(trunk)) if !trunk.is_empty() => Some(diff_of(&path, &trunk).await),
					_ => None,
				}
			} else {
				None
			};
			{
				let mut state = lock(&self.state);
				let entry = state.threads.entry(path.clone()).or_default();
				if let Some(diff) = diff {
					entry.facts.diff = diff;
					entry.diff_at = Some(now);
				}
				entry.facts.last_write_ms = last_write_ms;
				// A check this process saw is newer than the review's stored one; after a restart the stored one stands.
				entry.facts.check =
					entry.noted.map(|(passed, _)| passed).or_else(|| checks.as_deref().and_then(check_of));
			}
			open.push(path);
		}
		lock(&self.state).threads.retain(|path, _| open.contains(path));
		for url in self.prs_due(now) {
			let answer = match c.finisher.pr_view(&url).await {
				Ok(text) => pr_from_json(&text),
				Err(_) => None,
			};
			if self.pr_answered(&url, answer, now) {
				log(LogLevel::Warn, "day", &format!("gh couldn't tell the state of {url}; trying again later"), None);
			}
		}
		drop(reading);
	}
}

#[cfg(test)]
#[path = "thread_facts_tests.rs"]
mod tests;
