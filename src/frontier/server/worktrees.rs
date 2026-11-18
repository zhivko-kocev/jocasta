//! Thread worktrees on disk (0414, 0445, 0455, 0457): where a new one goes, its own variables and setup files, its
//! number, the last commit a review covered, removing one, and the leftovers no open thread holds (0456).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf, absolute};
use std::time::Duration;

use futures_util::future::{BoxFuture, join_all};
use serde_json::{Map, Value};

use crate::frontier::protocol::Leftover;
use crate::frontier::server::wording::fallback_slug;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, remove_tree_async, write_atomically};
use crate::shared::git::{GitOptions, git, git_async};
use crate::shared::pathkey::{path_key, real_path, same_dir};
use crate::shared::paths::data_dir;
use crate::shared::proctree::processes_in;
use crate::shared::projects::Project;

/// One git call the service runs off the event loop (CO-10, PERF-1).
pub const GIT_MS: i64 = 30_000;
/// One `git worktree list` for the leftovers (0456).
const WORKTREE_LIST_MS: i64 = 30_000;
/// How long removing a closed thread's checkout keeps trying while its panes' processes exit (F-02).
const REMOVE_WAIT_MS: i64 = 20_000;
/// One `git worktree remove` (CO-10).
const WORKTREE_REMOVE_MS: i64 = 60_000;
/// How often removing a checkout tries again while something holds it.
const REMOVE_RETRY_MS: u64 = 500;
/// At most this many processes are named when a folder is still in use.
const HOLDERS_NAMED: usize = 5;

/// git's trimmed stdout in `dir` without holding the event loop (CO-10), or `None` when it failed or ran too long.
pub async fn git_out(dir: &Path, args: &[impl AsRef<std::ffi::OsStr>]) -> Result<Option<String>> {
	let run = git_async(dir, args, GIT_MS, GitOptions::default()).await?;
	Ok((run.code == Some(0)).then(|| run.stdout.trim().to_owned()))
}

/// A new thread's branch and worktree folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPlace {
	pub branch: String,
	pub path: PathBuf,
}

/// A new thread's branch and worktree folder (0414, 0445, 0455): `feature/<slug>` in `<parent>/<project folder>.wt/<slug>`;
/// when either is taken, the first `-2`, `-3`… free for both, so the branch matches its folder and a new thread never
/// picks up an old thread's branch. A title with no slug (another script, 0581) takes `thread-<YYYYMMDD>-<n>`, the first
/// `n` free.
pub fn thread_place(main_path: &Path, slug: &str) -> Result<ThreadPlace> {
	let folder = main_path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
	let root = main_path.parent().unwrap_or(Path::new("")).join(format!("{folder}.wt"));
	let today = now_ms();
	for n in 1_usize.. {
		let name = if slug.is_empty() {
			fallback_slug(today, n)
		} else if n == 1 {
			slug.to_owned()
		} else {
			format!("{slug}-{n}")
		};
		let place = ThreadPlace { branch: format!("feature/{name}"), path: root.join(&name) };
		let verify = ["rev-parse", "--verify", "-q", &format!("refs/heads/{}", place.branch)];
		if !place.path.exists() && git(main_path, &verify)?.is_none_or(|found| found.is_empty()) {
			return Ok(place);
		}
	}
	unreachable!("a free name is found among the endless `-n` ones")
}

/// A thread pane's own variables (0457): `JOCASTA_THREAD` (its title slug), `COMPOSE_PROJECT_NAME` (`<project
/// folder>-<slug>`, Compose's allowed characters only) and `JOCASTA_PORT_OFFSET` (10 × the thread's number, the lowest
/// free positive integer among the project's open threads), for a project to read so parallel threads don't share
/// ports or Compose projects.
pub fn thread_env(project_folder: &str, slug: &str, number: u32) -> Vec<(String, String)> {
	let mut compose = String::new();
	let mut pending_dash = false;
	for c in format!("{project_folder}-{slug}").to_lowercase().chars() {
		if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
			if std::mem::take(&mut pending_dash) {
				compose.push('-');
			}
			compose.push(c);
		} else {
			pending_dash = true;
		}
	}
	if pending_dash {
		compose.push('-');
	}
	let compose = compose.trim_start_matches(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit())).to_owned();
	vec![
		("JOCASTA_THREAD".to_owned(), slug.to_owned()),
		("COMPOSE_PROJECT_NAME".to_owned(), compose),
		("JOCASTA_PORT_OFFSET".to_owned(), (10 * u64::from(number)).to_string()),
	]
}

/// Copies a new worktree's setup files from the main checkout (0457): the files `.worktreeinclude` (Claude Code's
/// format, gitignore patterns) matches that are untracked and gitignored, such as `.env`; a file the worktree has
/// already stays. Returns the paths copied, relative to the checkout.
pub async fn copy_worktree_include(main: &Path, worktree: &Path) -> Result<Vec<String>> {
	let include = main.join(".worktreeinclude");
	if !include.exists() {
		return Ok(Vec::new());
	}
	let listed = git_out(
		main,
		&[
			"ls-files".to_owned(),
			"--others".to_owned(),
			"--ignored".to_owned(),
			format!("--exclude-from={}", include.display()),
			"-z".to_owned(),
		],
	)
	.await?;
	let matched: Vec<&str> = listed.as_deref().unwrap_or("").split('\0').filter(|file| !file.is_empty()).collect();
	if matched.is_empty() {
		return Ok(Vec::new());
	}
	let stdin = format!("{}\0", matched.join("\0")).into_bytes();
	let ignored = git_async(
		main,
		&["check-ignore", "--stdin", "-z"],
		GIT_MS,
		GitOptions { stdin: Some(stdin), ..GitOptions::default() },
	)
	.await?;
	let mut copied = Vec::new();
	for file in ignored.stdout.split('\0').filter(|file| !file.is_empty()) {
		let to = worktree.join(file);
		if to.exists() {
			continue;
		}
		if let Some(dir) = to.parent() {
			fs::create_dir_all(dir)?;
		}
		fs::copy(main.join(file), &to)?;
		copied.push(file.to_owned());
	}
	Ok(copied)
}

/// `<data>/thread-numbers.json`: each open thread worktree's number (0457), so a respawned pane keeps its offset: the
/// lowest positive integer no other open thread of the project holds, given when first asked.
pub struct ThreadNumbers {
	file: PathBuf,
}

fn write_json(file: &Path, object: &Map<String, Value>) -> Result<()> {
	let text = serde_json::to_string_pretty(object)?;
	write_atomically(file, format!("{text}\n"), None)
}

impl ThreadNumbers {
	/// The numbers in `file`, or `<data>/thread-numbers.json`.
	pub fn new(file: Option<PathBuf>) -> Self {
		Self { file: file.unwrap_or_else(|| data_dir().join("thread-numbers.json")) }
	}

	/// The number of the thread in worktree `path`; `open` are the worktrees of the project's open threads.
	pub fn of(&self, path: &Path, open: &[PathBuf]) -> Result<u32> {
		let key = path_key(path);
		let mut numbers = read_json_object(&self.file).unwrap_or_default();
		if let Some(known) = numbers.get(&key).and_then(Value::as_u64) {
			return Ok(u32::try_from(known).unwrap_or(u32::MAX));
		}
		let taken: HashSet<u64> =
			open.iter().filter_map(|other| numbers.get(&path_key(other)).and_then(Value::as_u64)).collect();
		let mut number = 1_u64;
		while taken.contains(&number) {
			number += 1;
		}
		numbers.insert(key, Value::from(number));
		write_json(&self.file, &numbers)?;
		Ok(u32::try_from(number).unwrap_or(u32::MAX))
	}

	/// Forgets a removed thread's number, so the next thread can take it.
	pub fn forget(&self, path: &Path) -> Result<()> {
		let Some(mut numbers) = read_json_object(&self.file) else {
			return Ok(());
		};
		numbers.shift_remove(&path_key(path));
		write_json(&self.file, &numbers)
	}
}

/// `<data>/reviewed.json`: the last reviewed commit per thread worktree (0394), so a restart knows what's unreviewed.
pub fn reviewed_file() -> PathBuf {
	data_dir().join("reviewed.json")
}

/// The last reviewed commit per thread worktree, in [`reviewed_file`].
pub struct ReviewedCommits {
	file: PathBuf,
}

impl ReviewedCommits {
	/// The commits in `file`, or in [`reviewed_file`].
	pub fn new(file: Option<PathBuf>) -> Self {
		Self { file: file.unwrap_or_else(reviewed_file) }
	}

	/// The last reviewed commit of the worktree at `path`, if any.
	pub fn of(&self, path: &Path) -> Option<String> {
		read_json_object(&self.file)?.get(&path_key(&real_path(path)))?.as_str().map(str::to_owned)
	}

	/// Records `commit` as the worktree's last reviewed one, or forgets the worktree (`None`).
	pub fn mark(&self, path: &Path, commit: Option<&str>) -> Result<()> {
		let mut all = read_json_object(&self.file).unwrap_or_default();
		let key = path_key(&real_path(path));
		match commit.filter(|commit| !commit.is_empty()) {
			Some(commit) => {
				all.insert(key, Value::from(commit));
			}
			None => {
				all.shift_remove(&key);
			}
		}
		write_json(&self.file, &all)
	}
}

/// A worktree git lists: its folder and branch.
struct Listed {
	path: PathBuf,
	branch: Option<String>,
}

/// The worktrees `git worktree list --porcelain -z` names in the repo at `main` (SEC-LOC-6: NUL-separated, so a path is
/// never C-quoted), each with its branch; `None` when git failed, which is never "not listed".
async fn worktree_list(main: &Path) -> Result<Option<Vec<Listed>>> {
	let run =
		git_async(main, &["worktree", "list", "--porcelain", "-z"], WORKTREE_LIST_MS, GitOptions::default()).await?;
	if run.code != Some(0) {
		return Ok(None);
	}
	let mut found: Vec<Listed> = Vec::new();
	for field in run.stdout.split('\0') {
		if let Some(path) = field.strip_prefix("worktree ") {
			found.push(Listed { path: PathBuf::from(path), branch: None });
		} else if let (Some(branch), Some(current)) = (field.strip_prefix("branch refs/heads/"), found.last_mut()) {
			current.branch = Some(branch.to_owned());
		}
	}
	Ok(Some(found))
}

/// Why a folder can't go yet: the processes still working in it (0486), or `None` when none does.
fn in_use(path: &Path) -> Option<String> {
	let holders = processes_in(path);
	if holders.is_empty() {
		return None;
	}
	let mut named: Vec<String> =
		holders.iter().take(HOLDERS_NAMED).map(|process| format!("{} (pid {})", process.name, process.pid)).collect();
	if holders.len() > HOLDERS_NAMED {
		named.push(format!("{} more", holders.len() - HOLDERS_NAMED));
	}
	Some(format!("processes still work in it: {}", named.join(", ")))
}

/// One try at removing the checkout: `Ok(true)` once folder and git's record are gone, `Ok(false)` when it is still
/// there, `Err` with why not.
async fn try_remove(main: &Path, path: &Path) -> Result<bool> {
	let Some(worktrees) = worktree_list(main).await? else {
		let folder = main.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
		return Err(Error::other(format!("git couldn't list {folder}'s worktrees, so the folder stays")));
	};
	let listed = worktrees.iter().any(|w| w.path.exists() && same_dir(&w.path, path));
	if let Some(holders) = path.exists().then(|| in_use(path)).flatten() {
		return Err(Error::other(holders));
	}
	if listed {
		let removed = git_async(
			main,
			&[Path::new("worktree"), Path::new("remove"), path],
			WORKTREE_REMOVE_MS,
			GitOptions::default(),
		)
		.await?;
		match removed.code {
			None => return Err(Error::other("git worktree remove didn't finish in a minute")),
			Some(0) => {}
			Some(code) => {
				let said = removed.stderr.trim();
				let why =
					if said.is_empty() { format!("git worktree remove failed (exit {code})") } else { said.to_owned() };
				return Err(Error::other(why));
			}
		}
	} else if path.join(".git").exists() {
		return Err(Error::other("it still has its .git file, though git doesn't list it; it stays"));
	} else if path.exists() {
		remove_tree_async(path).await?;
	}
	if path.exists() {
		return Ok(false);
	}
	git_async(main, &["worktree", "prune"], GIT_MS, GitOptions::default()).await?;
	Ok(true)
}

/// Removes the linked worktree at `path` of the repo at `main` (`git worktree remove`, no force: a dirty checkout is
/// refused) once no process has the folder or one inside it as its working folder (0486), trying again every half
/// second for up to `wait_ms` (default 20 s) while a process works in it or something else holds it. A folder git
/// positively doesn't list, and that has no `.git` file of its own any more, is one git already let go of: it is
/// deleted and pruned. git failing to list its worktrees keeps the folder (SEC-LOC-6): it may hold uncommitted work.
/// Returns `None` once the folder and git's record are gone, else why not.
pub async fn remove_checkout(main: &Path, path: &Path, wait_ms: Option<i64>) -> Option<String> {
	let end = now_ms() + wait_ms.unwrap_or(REMOVE_WAIT_MS);
	let mut reason = String::new();
	loop {
		match try_remove(main, path).await {
			Ok(true) => return None,
			Ok(false) => {}
			Err(error) => {
				reason = error.to_string().split('\n').next().unwrap_or_default().to_owned();
				// A dirty checkout won't become clean by waiting, nor git's own refusal to list.
				let final_reason =
					["modified or untracked", "contains modified", "couldn't list", "still has its .git"];
				if final_reason.iter().any(|needle| reason.contains(needle)) {
					return Some(reason);
				}
			}
		}
		if now_ms() >= end {
			return Some(if reason.is_empty() { "something still holds it".to_owned() } else { reason });
		}
		tokio::time::sleep(Duration::from_millis(REMOVE_RETRY_MS)).await;
	}
}

/// Leftover worktrees (0456): checkouts in a project's `<project>.wt/` folder that no open thread holds (closed in
/// herdr, still on disk), with their branch, commits ahead of trunk and changed files, read off the event loop; trunk
/// only for a project that has one (PERF-1). `open` are the open threads' worktrees. Never removed here.
pub async fn find_leftovers(
	projects: &[Project],
	open: &[PathBuf],
	trunk_of: impl for<'p> Fn(&'p Project) -> BoxFuture<'p, Option<String>>,
) -> Result<Vec<Leftover>> {
	let held: HashSet<String> = open.iter().map(|path| path_key(&real_path(path))).collect();
	let mut found = Vec::new();
	for project in projects.iter().filter(|project| project.git) {
		// Real paths on both sides: git lists a worktree by its real path, and the project's may run through a link
		// (macOS's `/var` to `/private/var`).
		let main = real_path(&project.main_path);
		let folder = main.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
		let root = path_key(&main.parent().unwrap_or(Path::new("")).join(format!("{folder}.wt")));
		let worktrees = worktree_list(&project.main_path).await?.unwrap_or_default();
		let left: Vec<Listed> = worktrees
			.into_iter()
			.filter(|w| {
				w.path.exists()
					&& !held.contains(&path_key(&real_path(&w.path)))
					&& path_key(&real_path(w.path.parent().unwrap_or(Path::new("")))).starts_with(&root)
			})
			.collect();
		if left.is_empty() {
			continue;
		}
		let trunk = trunk_of(project).await;
		let read = join_all(left.iter().map(|w| leftover(project, w, trunk.as_deref()))).await;
		for one in read {
			found.push(one?);
		}
	}
	Ok(found)
}

/// One leftover worktree: its branch, commits ahead of `trunk` and changed files.
async fn leftover(project: &Project, w: &Listed, trunk: Option<&str>) -> Result<Leftover> {
	let ahead = async {
		match trunk {
			Some(trunk) => {
				git_out(&w.path, &["rev-list".to_owned(), "--count".to_owned(), format!("{trunk}..HEAD")]).await
			}
			None => Ok(None),
		}
	};
	let (ahead, status) = futures_util::future::try_join(ahead, git_out(&w.path, &["status", "--porcelain"])).await?;
	let dirty = status.unwrap_or_default().split('\n').filter(|line| !line.is_empty()).count();
	Ok(Leftover {
		project: project.name.clone(),
		folder: absolute(&w.path)?,
		branch: w.branch.clone(),
		ahead: ahead.and_then(|count| count.parse().ok()).unwrap_or(0),
		dirty: u32::try_from(dirty).unwrap_or(u32::MAX),
	})
}

#[cfg(test)]
#[path = "worktrees_tests.rs"]
mod tests;
