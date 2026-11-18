//! The worklog (0302): `<data>/worklog/<day>.md`, one entry per project from the day's omp sessions and his commits.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::{DateTime, SecondsFormat};
use regex::Regex;
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;

use crate::shared::day::{DayRef, day_label, day_start, local_day};
use crate::shared::errors::Result;
use crate::shared::fsx::write_atomically;
use crate::shared::git::{GitOptions, git_async};
use crate::shared::paths::data_dir;
use crate::shared::projects::Project;

/// One omp session of a project's for the worklog: its label (title, else checkout) and its messages' times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorklogSession {
	pub label: String,
	pub times: Vec<i64>,
}

/// Gaps between a thread's messages up to this long count as time spent; longer ones are breaks.
const ACTIVE_GAP_MS: i64 = 15 * 60_000;
/// A worklog's `git log` (and `git config`) of one project (CO-12): off the event loop, bounded.
const GIT_TIMEOUT_MS: i64 = 60_000;
/// Projects whose git runs at once while a worklog is written (PERF-2).
const GIT_POOL: usize = 4;

fn worklog_dir() -> PathBuf {
	data_dir().join("worklog")
}

pub fn worklog_path(day: &str) -> PathBuf {
	worklog_dir().join(format!("{day}.md"))
}

/// Time spent: the sum of gaps between messages up to 15 minutes apart.
fn active_ms(times: &[i64]) -> i64 {
	times.windows(2).map(|pair| pair[1] - pair[0]).filter(|gap| *gap <= ACTIVE_GAP_MS).sum()
}

fn duration(ms: i64) -> String {
	let minutes = (ms as f64 / 60_000.0).round() as i64;
	if minutes < 60 {
		return format!("{minutes}m");
	}
	format!("{}h {:02}m", minutes.div_euclid(60), minutes.rem_euclid(60))
}

static GITHUB: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"github\.com[:/]([^/]+/[^/]+?)(?:\.git)?/?$").expect("a fixed pattern"));
static AZURE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"dev\.azure\.com/([^/]+)/([^/]+)/_git/([^/]+?)/?$").expect("a fixed pattern"));
static AZURE_USER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^https://[^@/]+@").expect("a fixed pattern"));
static SSH: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"ssh\.dev\.azure\.com:v3/([^/]+)/([^/]+)/([^/]+?)/?$").expect("a fixed pattern"));
static OPEN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"<!-- jocasta (\S+) (\S+) -->\n").expect("a fixed pattern"));

/// A commit's web page from the checkout's `origin`: GitHub or Azure DevOps; `None` for other hosts.
fn commit_url(origin: Option<&str>, sha: &str) -> Option<String> {
	let origin = origin.filter(|origin| !origin.is_empty())?;
	if let Some(github) = GITHUB.captures(origin) {
		return Some(format!("https://github.com/{}/commit/{sha}", &github[1]));
	}
	let bare = AZURE_USER.replace(origin, "https://");
	if let Some(azure) = AZURE.captures(&bare) {
		return Some(format!("https://dev.azure.com/{}/{}/_git/{}/commit/{sha}", &azure[1], &azure[2], &azure[3]));
	}
	let ssh = SSH.captures(origin)?;
	Some(format!("https://dev.azure.com/{}/{}/_git/{}/commit/{sha}", &ssh[1], &ssh[2], &ssh[3]))
}

/// One of his commits: its hash, subject and, for a known host, its web page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
	pub sha: String,
	pub subject: String,
	pub url: Option<String>,
}

/// What the worklog needs to know about a git project: his `user.email` there and its `origin`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitPlace {
	pub email: Option<String>,
	pub origin: Option<String>,
}

/// A git project's `user.email` and `origin` (two git calls off the event loop); empty for a project outside git.
async fn git_place(project: &Project) -> Result<GitPlace> {
	if !project.git {
		return Ok(GitPlace::default());
	}
	let value = |args: &'static [&'static str]| async move {
		let run = git_async(&project.main_path, args, GIT_TIMEOUT_MS, GitOptions::default()).await?;
		let text = run.stdout.trim();
		Ok::<_, crate::shared::errors::Error>((run.code == Some(0) && !text.is_empty()).then(|| text.to_owned()))
	};
	let (email, origin) =
		futures_util::future::try_join(value(&["config", "user.email"]), value(&["remote", "get-url", "origin"]))
			.await?;
	Ok(GitPlace { email, origin })
}

/// What [`GitPlaces`] keeps for one day: the day and each project's place, asked once.
#[derive(Default)]
struct PlacesOfDay {
	day: String,
	by_project: HashMap<String, Arc<OnceCell<GitPlace>>>,
}

/// Each project's git email and origin, kept for one local day (CO-12): both the worklog and its so-far pass ask.
#[derive(Default)]
pub struct GitPlaces {
	kept: Mutex<PlacesOfDay>,
}

impl GitPlaces {
	pub fn new() -> Self {
		Self::default()
	}

	pub async fn of(&self, project: &Project, day: &str) -> Result<GitPlace> {
		let place = {
			let mut kept = self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
			if kept.day != day {
				kept.day = day.to_owned();
				kept.by_project.clear();
			}
			Arc::clone(kept.by_project.entry(project.id.clone()).or_default())
		};
		place.get_or_try_init(|| git_place(project)).await.cloned()
	}
}

/// `git log` of his commits on `day`, on any branch.
fn log_args(day: &str, email: &str) -> Vec<String> {
	let iso =
		|ms: i64| DateTime::from_timestamp_millis(ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Millis, true);
	vec![
		"log".to_owned(),
		"--all".to_owned(),
		format!("--since={}", iso(day_start(day, 0))),
		format!("--until={}", iso(day_start(day, 1))),
		format!("--author={email}"),
		"--format=%H%x09%s".to_owned(),
	]
}

fn parse_commits(out: &str, origin: Option<&str>) -> Vec<Commit> {
	out.split('\n')
		.filter(|line| !line.is_empty())
		.map(|line| {
			let mut fields = line.split('\t');
			let sha = fields.next().unwrap_or_default().to_owned();
			let subject = fields.collect::<Vec<_>>().join("\t");
			let url = commit_url(origin, &sha);
			Commit { sha, subject, url }
		})
		.collect()
}

/// His commits on `day` in a git project, on any branch, without holding the event loop (CO-12, PERF-2): author email
/// from the checkout's `user.email` (`place`, from the caller's per-day cache); a `git log` that fails or runs past a
/// minute counts as none.
pub async fn commits_async(project: &Project, day: &str, place: &GitPlace) -> Result<Vec<Commit>> {
	let Some(email) = place.email.as_deref().filter(|_| project.git) else {
		return Ok(Vec::new());
	};
	let run = git_async(&project.main_path, &log_args(day, email), GIT_TIMEOUT_MS, GitOptions::default()).await?;
	Ok(if run.code == Some(0) { parse_commits(&run.stdout, place.origin.as_deref()) } else { Vec::new() })
}

/// One thread's time in a project's day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadTime {
	pub label: String,
	pub ms: i64,
}

/// A project's day (0302): time per thread from session activity, and his commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDay {
	pub threads: Vec<ThreadTime>,
	pub commits: Vec<Commit>,
	pub total_ms: i64,
}

/// A day's work in one project, or `None` when nothing happened in it; `mine` are its commits on that day.
pub fn project_day(day: &str, sessions: &[WorklogSession], mine: &[Commit]) -> Option<ProjectDay> {
	let threads: Vec<ThreadTime> = sessions
		.iter()
		.filter_map(|session| {
			let mut times: Vec<i64> = session.times.iter().copied().filter(|at| local_day(*at) == day).collect();
			times.sort_unstable();
			(!times.is_empty()).then(|| ThreadTime { label: session.label.clone(), ms: active_ms(&times) })
		})
		.collect();
	if threads.is_empty() && mine.is_empty() {
		return None;
	}
	let total_ms = threads.iter().map(|thread| thread.ms).sum();
	Some(ProjectDay { threads, commits: mine.to_vec(), total_ms })
}

/// A project's day as one line for the dashboard (0429): `acme-api · 1h 20m · 3 commits`.
pub fn project_day_line(name: &str, day: &ProjectDay) -> String {
	let n = day.commits.len();
	let mut parts = vec![name.to_owned(), duration(day.total_ms)];
	if n > 0 {
		parts.push(format!("{n} commit{}", if n == 1 { "" } else { "s" }));
	}
	parts.join(" · ")
}

/// A project's generated entry for `day`, or `None` when nothing happened in it.
fn project_entry(project: &Project, day: &str, sessions: &[WorklogSession], mine: &[Commit]) -> Option<String> {
	let found = project_day(day, sessions, mine)?;
	let mut lines = vec![format!("## {} · {}", project.name, duration(found.total_ms)), String::new()];
	if !found.threads.is_empty() {
		lines.push("Threads".to_owned());
		for thread in &found.threads {
			lines.push(format!("- {} · {}", thread.label, duration(thread.ms)));
		}
	}
	if !mine.is_empty() {
		if !found.threads.is_empty() {
			lines.push(String::new());
		}
		lines.push("Commits".to_owned());
		for commit in mine {
			let short: String = commit.sha.chars().take(7).collect();
			let link = commit.url.as_ref().map_or(short.clone(), |url| format!("[{short}]({url})"));
			lines.push(format!("- {link} {}", commit.subject));
		}
	}
	Some(lines.join("\n"))
}

fn hash(text: &str) -> String {
	let digest = Sha256::digest(text.as_bytes());
	let mut hex = String::with_capacity(12);
	for byte in &digest[..6] {
		let _ = write!(hex, "{byte:02x}");
	}
	hex
}

fn block(project_id: &str, body: &str) -> String {
	format!("<!-- jocasta {project_id} {} -->\n{body}\n<!-- /jocasta {project_id} -->", hash(body))
}

/// A marker block in `text` from `from` on: where it starts and ends, its project id, the hash it records and its body.
/// What `/<!-- jocasta (\S+) (\S+) -->\n([\s\S]*?)\n<!-- \/jocasta \1 -->/` finds, the backreference done by hand.
fn next_block(text: &str, from: usize) -> Option<(usize, usize, &str, &str, &str)> {
	let mut at = from;
	while at <= text.len() {
		let found = OPEN.captures_at(text, at)?;
		let (whole, id, recorded) = (found.get(0)?, found.get(1)?.as_str(), found.get(2)?.as_str());
		let close = format!("\n<!-- /jocasta {id} -->");
		if let Some(offset) = text[whole.end()..].find(&close) {
			let body = &text[whole.end()..whole.end() + offset];
			return Some((whole.start(), whole.end() + offset + close.len(), id, recorded, body));
		}
		at = whole.start() + text[whole.start()..].chars().next().map_or(1, char::len_utf8);
	}
	None
}

/// Merges new entries into a worklog he may have edited (0302): each project's entry sits between markers that carry
/// the hash of what Jocasta wrote. A block whose text still matches its hash is replaced; a block he edited is kept
/// as he left it; projects without a block are appended. Text outside the blocks is his and stays.
pub fn merge_worklog(existing: Option<&str>, heading: &str, entries: &[(String, String)]) -> String {
	let Some(existing) = existing else {
		let mut parts = vec![heading.to_owned()];
		parts.extend(entries.iter().map(|(id, body)| block(id, body)));
		return format!("{}\n", parts.join("\n\n"));
	};
	let mut seen: Vec<&str> = Vec::new();
	let mut merged = String::with_capacity(existing.len());
	let mut copied = 0;
	while let Some((start, end, id, recorded, body)) = next_block(existing, copied) {
		merged.push_str(&existing[copied..start]);
		seen.push(id);
		match entries.iter().find(|(entry, _)| entry == id) {
			Some((_, next)) if hash(body) == recorded => merged.push_str(&block(id, next)),
			_ => merged.push_str(&existing[start..end]),
		}
		copied = end;
	}
	merged.push_str(&existing[copied..]);
	let added: Vec<String> =
		entries.iter().filter(|(id, _)| !seen.contains(&id.as_str())).map(|(id, body)| block(id, body)).collect();
	if added.is_empty() { merged } else { format!("{}\n\n{}\n", merged.trim_end(), added.join("\n\n")) }
}

/// What [`write_worklog`] wrote: the file and the names of the projects it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorklogWritten {
	pub path: PathBuf,
	pub projects: Vec<String>,
}

/// Writes `<data>/worklog/<day>.md` (0302): one entry per project from that day's omp sessions and his commits, with
/// commit links and time per project from session activity. Each project's git runs off the event loop, four
/// projects at a time (PERF-2); once `stopped` says so (the service stopping meanwhile) nothing is written. `places`
/// defaults to a fresh [`GitPlaces`], `stopped` to never. Returns the path and the projects it covers.
pub async fn write_worklog(
	projects: &[Project],
	day: &str,
	sessions_of: impl Fn(&Project, &str) -> Vec<WorklogSession>,
	places: Option<&GitPlaces>,
	stopped: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<WorklogWritten> {
	let fresh = GitPlaces::new();
	let places = places.unwrap_or(&fresh);
	let stopped = || stopped.is_some_and(|stopped| stopped());
	let found: Mutex<HashMap<&str, Vec<Commit>>> = Mutex::new(HashMap::new());
	let next = AtomicUsize::new(0);
	let worker = || async {
		while next.load(Ordering::SeqCst) < projects.len() && !stopped() {
			let project = &projects[next.fetch_add(1, Ordering::SeqCst)];
			let commits = commits_async(project, day, &places.of(project, day).await?).await?;
			found.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(&project.id, commits);
		}
		Ok::<(), crate::shared::errors::Error>(())
	};
	let pool = GIT_POOL.min(projects.len());
	for done in futures_util::future::join_all((0..pool).map(|_| worker())).await {
		done?;
	}
	let path = worklog_path(day);
	if stopped() {
		return Ok(WorklogWritten { path, projects: Vec::new() });
	}
	let found = found.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner());
	let mut entries: Vec<(String, String)> = Vec::new();
	for project in projects {
		let mine = found.get(project.id.as_str()).map_or(&[][..], Vec::as_slice);
		if let Some(entry) = project_entry(project, day, &sessions_of(project, day), mine) {
			entries.push((project.id.clone(), entry));
		}
	}
	let existing = std::fs::read_to_string(&path).ok();
	let heading = format!("# Worklog · {}", day_label(DayRef::Day(day), true));
	let text = merge_worklog(existing.as_deref(), &heading, &entries);
	if existing.as_deref() != Some(text.as_str()) {
		write_atomically(&path, &text, None)?;
	}
	let names = projects
		.iter()
		.filter(|project| entries.iter().any(|(id, _)| *id == project.id))
		.map(|project| project.name.clone())
		.collect();
	Ok(WorklogWritten { path, projects: names })
}

#[cfg(test)]
#[path = "worklog_tests.rs"]
mod tests;
