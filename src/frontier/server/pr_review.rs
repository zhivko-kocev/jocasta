//! The pull-request review workspace (0680, 0681): his `R` on a review request of `today` starts a review thread by
//! code, with no turn of the boss. Jocasta reads the pull request (`gh` or `az`), fetches its head into the project's
//! repository, and starts a thread whose worktree is that head, whose brief is `config/pr-review.md` and the facts it
//! read, and whose phase is Review. The thread hands its findings in with `review_note`; when its turn ends the notes
//! are re-checked against the head and become the brief `pr-<project id>-<n>` (`ready`). Everything a stranger wrote
//! (title, description, existing comments, author) goes into the task file fenced as data (`fence_external`).

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{Brief, Fact, MapGroup, MapItem, MapLink, Proof, Section};
use crate::frontier::brief::store;
use crate::frontier::protocol::{ServerMessage, ToastLevel, ToastPriority, ToastReason};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::finish::GhRun;
use crate::frontier::server::journal::About;
use crate::frontier::server::review_notes::{ReviewNotes, collapse, git_out_raw, reads, review_file};
use crate::frontier::server::task_file::TaskRecord;
use crate::frontier::server::thread::{Phase, ReviewRunning, ThreadRef, ThreadReview, set_phase, show};
use crate::frontier::server::thread_brief::{DIFF_LINES, parse_diff};
use crate::frontier::server::thread_facts::facts_heading;
use crate::frontier::server::thread_start::StartOptions;
use crate::frontier::server::wording::{Door, cut, fence_external, quoted};
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::events::queue::EventRow;
use crate::shared::git::{GitOptions, git_async};
use crate::shared::identity::{decoded, normalize_origin};
use crate::shared::install::shipped_config_dir;
use crate::shared::projects::Project;

#[cfg(test)]
#[path = "pr_review_tests.rs"]
mod tests;

/// The review brief's template, shipped beside the binary.
const TEMPLATE: &str = "pr-review.md";

/// `git fetch` of a pull request's ref.
const FETCH_MS: i64 = 120_000;

/// The most lines the description and the existing comments take in the task file (0681).
const DESCRIPTION_LINES: usize = 80;
const COMMENT_LINES: usize = 120;

/// The most changed files the task file lists.
const FILES_LISTED: usize = 60;

/// Where a pull request lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
	Github,
	Azure,
}

impl Platform {
	/// The word the journal and the page use.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Github => "github",
			Self::Azure => "azure",
		}
	}
}

/// The pull request a review thread reviews (0680): where it is, its SHAs and what was read of it when the thread
/// started (so the brief needs no network).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrTarget {
	pub platform: Platform,
	pub url: String,
	pub number: u64,
	/// GitHub: `owner/repo`. Azure DevOps: the repository's name.
	pub repo: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub org_url: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub project: Option<String>,
	pub title: String,
	pub author: String,
	pub base_branch: String,
	/// The commit the review reads, and the one the merge base of the pull request is.
	pub head_sha: String,
	pub base_sha: String,
	pub ci: String,
	pub threads: usize,
}

/// A pull request's page link, taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
	pub platform: Platform,
	pub number: u64,
	pub repo: String,
	pub org_url: Option<String>,
	pub project: Option<String>,
}

impl Address {
	/// `https://github.com/<o>/<r>/pull/<n>`, or an Azure DevOps `<org>/<project>/_git/<repo>/pullrequest/<id>` page
	/// (`dev.azure.com` or `<org>.visualstudio.com`); `None` for anything else.
	pub fn parse(url: &str) -> Option<Self> {
		let parsed = url::Url::parse(url).ok()?;
		if parsed.scheme() != "https" {
			return None;
		}
		let host = parsed.host_str()?.to_lowercase();
		let raw: Vec<&str> = parsed.path_segments()?.filter(|segment| !segment.is_empty()).collect();
		let segments: Vec<String> = raw.iter().map(|segment| decoded(segment)).collect();
		if host == "github.com" {
			let [owner, repo, kind, number] = segments.as_slice() else { return None };
			if kind != "pull" {
				return None;
			}
			return Some(Self {
				platform: Platform::Github,
				number: number.parse().ok()?,
				repo: format!("{owner}/{repo}"),
				org_url: None,
				project: None,
			});
		}
		if host == "dev.azure.com" || host.ends_with(".visualstudio.com") {
			let at = segments.iter().position(|segment| segment == "_git")?;
			if at == 0 || segments.len() != at + 4 || segments[at + 2] != "pullrequest" {
				return None;
			}
			let org: String = raw[..at - 1].iter().map(|segment| format!("/{segment}")).collect();
			return Some(Self {
				platform: Platform::Azure,
				number: segments[at + 3].parse().ok()?,
				repo: segments[at + 1].clone(),
				org_url: Some(format!("https://{host}{org}")),
				project: Some(segments[at - 1].clone()),
			});
		}
		None
	}

	/// The normalized origin of the repository (0103): what a project's `identity` is.
	pub fn identity(&self) -> Option<String> {
		let origin = match (self.platform, &self.org_url, &self.project) {
			(Platform::Github, ..) => format!("https://github.com/{}", self.repo),
			(Platform::Azure, Some(org), Some(project)) => format!("{org}/{project}/_git/{}", self.repo),
			_ => return None,
		};
		normalize_origin(&origin).ok()
	}
}

/// The open event of `kind` whose page is `url`: a client can make the service act only on what `today` shows.
pub(super) fn open_event(c: &ServiceContext, kind: &str, url: &str) -> Result<EventRow> {
	let rows = c.day.queue(|queue| queue.open())?;
	rows.into_iter()
		.find(|row| row.kind == kind && row.url.as_deref() == Some(url))
		.ok_or_else(|| Error::user("That row isn't open any more."))
}

/// The git project whose `origin` is `identity`.
pub(super) fn project_of(c: &ServiceContext, identity: &str) -> Result<Project> {
	c.places.listed().into_iter().find(|project| project.git && project.identity == identity).ok_or_else(|| {
		Error::user(format!("None of your projects has the remote {identity}; add it with `jocasta project add`."))
	})
}

/// The last line of a CLI's output: what it said last is why it failed.
pub(super) fn last_line(text: &str) -> &str {
	text.trim().lines().next_back().unwrap_or_default()
}

/// A `gh` or `az` run's output as JSON; why not when it isn't installed, failed or said something else.
pub(super) fn json_of(run: Option<GhRun>, what: &str) -> Result<Value> {
	let Some(run) = run else { return Err(Error::user(format!("{what}: the CLI isn't installed."))) };
	match run.code {
		Some(0) => {}
		Some(_) => return Err(Error::user(format!("{what} failed: {}", last_line(&run.out)))),
		None => return Err(Error::user(format!("{what}: no answer in time."))),
	}
	serde_json::from_str(&run.out).map_err(|_| Error::user(format!("{what} answered something other than JSON.")))
}

/// `args` as the `&str` slice the CLI wrappers take.
pub(super) fn refs(args: &[String]) -> Vec<&str> {
	args.iter().map(String::as_str).collect()
}

/// A branch name that is safe as one git argument: no leading `-`, no white space or control characters.
pub(super) fn safe_ref(name: &str) -> Result<()> {
	if name.is_empty() || name.starts_with('-') || name.chars().any(|c| c.is_whitespace() || c.is_control()) {
		return Err(Error::user("The branch name isn't one Jocasta passes to git."));
	}
	Ok(())
}

/// One line of outside text: control characters dropped, white space collapsed.
pub(super) fn one_line(text: &str) -> String {
	collapse(&text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>())
}

/// Fetches of one repository share `FETCH_HEAD`: one at a time.
pub(super) static FETCHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `git fetch origin <refspec>` in `repo`, then the commit it fetched.
pub(super) async fn fetch_commit(repo: &Path, refspec: &str) -> Result<String> {
	let _one = FETCHING.lock().await;
	let run = git_async(repo, &["fetch", "origin", "--", refspec], FETCH_MS, GitOptions::default()).await?;
	if run.code != Some(0) {
		return Err(Error::user(format!("git fetch origin {refspec} failed: {}", last_line(&run.stderr))));
	}
	git_out(repo, &["rev-parse", "FETCH_HEAD"])
		.await?
		.filter(|sha| !sha.is_empty())
		.ok_or_else(|| Error::user(format!("git fetch origin {refspec} left no commit.")))
}

/// What the pull request's own page says: read from `gh` or `az`.
struct Read {
	title: String,
	body: String,
	author: String,
	base_branch: String,
	ci: String,
	/// One line per existing comment.
	comments: Vec<String>,
	threads: usize,
}

/// GitHub's check rollup as counts: `3 passed, 1 failed, 0 pending`, or `no checks`.
fn github_ci(rollup: &Value) -> String {
	let items = rollup.as_array().map(Vec::as_slice).unwrap_or_default();
	if items.is_empty() {
		return "no checks".to_owned();
	}
	let (mut passed, mut failed, mut pending) = (0, 0, 0);
	for item in items {
		let word = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("").to_uppercase();
		let (conclusion, state) = (word("conclusion"), word("state"));
		match (conclusion.as_str(), state.as_str()) {
			("SUCCESS" | "NEUTRAL" | "SKIPPED", _) | (_, "SUCCESS") => passed += 1,
			("FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE", _)
			| (_, "FAILURE" | "ERROR") => failed += 1,
			_ => pending += 1,
		}
	}
	format!("{passed} passed, {failed} failed, {pending} pending")
}

async fn read_github(c: &ServiceContext, address: &Address, url: &str) -> Result<Read> {
	let data = crate::shared::paths::data_dir();
	let view = ["pr", "view", url, "--json", "title,body,author,baseRefName,statusCheckRollup"];
	let view = json_of(c.finisher.gh(&view, &data).await, "gh pr view")?;
	let route = format!("repos/{}/pulls/{}/comments", address.repo, address.number);
	let pages = json_of(c.finisher.gh(&["api", &route, "--paginate", "--slurp"], &data).await, "gh api comments")?;
	let all: Vec<&Value> =
		pages.as_array().into_iter().flatten().flat_map(|page| page.as_array().into_iter().flatten()).collect();
	let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
	let comments = all
		.iter()
		.map(|comment| {
			let who = comment.get("user").map(|user| text(user, "login")).unwrap_or_default();
			let at = match comment.get("line").and_then(Value::as_u64) {
				Some(line) => format!("{}:{line}", text(comment, "path")),
				None => text(comment, "path"),
			};
			format!("{who} on {at}: {}", text(comment, "body"))
		})
		.collect();
	let threads = all.iter().filter(|comment| comment.get("in_reply_to_id").is_none_or(Value::is_null)).count();
	Ok(Read {
		title: text(&view, "title"),
		body: text(&view, "body"),
		author: view.get("author").map(|author| text(author, "login")).unwrap_or_default(),
		base_branch: text(&view, "baseRefName"),
		ci: github_ci(view.get("statusCheckRollup").unwrap_or(&Value::Null)),
		comments,
		threads,
	})
}

async fn read_azure(c: &ServiceContext, address: &Address) -> Result<Read> {
	let data = crate::shared::paths::data_dir();
	let (org, project) = (address.org_url.clone().unwrap_or_default(), address.project.clone().unwrap_or_default());
	let id = address.number.to_string();
	let show: Vec<String> =
		["repos", "pr", "show", "--id", &id, "--org", &org, "-o", "json"].map(str::to_owned).to_vec();
	let show = json_of(c.finisher.az(&refs(&show), &data).await, "az repos pr show")?;
	let threads: Vec<String> = [
		"devops",
		"invoke",
		"--org",
		&org,
		"--area",
		"git",
		"--resource",
		"pullRequestThreads",
		"--route-parameters",
		&format!("project={project}"),
		&format!("repositoryId={}", address.repo),
		&format!("pullRequestId={id}"),
		"--api-version",
		"7.1",
		"-o",
		"json",
	]
	.map(str::to_owned)
	.to_vec();
	let threads = json_of(c.finisher.az(&refs(&threads), &data).await, "az devops invoke pullRequestThreads")?;
	let policy: Vec<String> =
		["repos", "pr", "policy", "list", "--id", &id, "--org", &org, "-o", "json"].map(str::to_owned).to_vec();
	let ci = match json_of(c.finisher.az(&refs(&policy), &data).await, "az repos pr policy list") {
		Ok(policies) => azure_ci(&policies),
		Err(_) => "unknown".to_owned(),
	};
	let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
	let mut comments = Vec::new();
	let mut count = 0;
	for thread in threads.get("value").and_then(Value::as_array).into_iter().flatten() {
		let place = thread.get("threadContext").and_then(|context| {
			let line = context.get("rightFileStart")?.get("line")?.as_u64()?;
			Some(format!("{}:{line}", text(context, "filePath")))
		});
		let mut counted = false;
		for comment in thread.get("comments").and_then(Value::as_array).into_iter().flatten() {
			if text(comment, "commentType") == "system" || text(comment, "content").is_empty() {
				continue;
			}
			counted = true;
			let who = comment.get("author").map(|author| text(author, "displayName")).unwrap_or_default();
			let at = place.clone().unwrap_or_else(|| "the pull request".to_owned());
			comments.push(format!("{who} on {at}: {}", text(comment, "content")));
		}
		count += usize::from(counted);
	}
	let target = text(&show, "targetRefName");
	Ok(Read {
		title: text(&show, "title"),
		body: text(&show, "description"),
		author: show.get("createdBy").map(|author| text(author, "displayName")).unwrap_or_default(),
		base_branch: target.strip_prefix("refs/heads/").unwrap_or(&target).to_owned(),
		ci,
		comments,
		threads: count,
	})
}

/// Azure DevOps' branch policies as counts of their evaluations' status.
fn azure_ci(policies: &Value) -> String {
	let items = policies.as_array().map(Vec::as_slice).unwrap_or_default();
	if items.is_empty() {
		return "no policies".to_owned();
	}
	let count = |wanted: &[&str]| {
		items
			.iter()
			.filter(|item| {
				let status = item.get("status").and_then(Value::as_str).unwrap_or("").to_lowercase();
				wanted.contains(&status.as_str())
			})
			.count()
	};
	let (passed, failed) = (count(&["approved"]), count(&["rejected", "broken"]));
	format!("{passed} passed, {failed} failed, {} pending", items.len() - passed - failed)
}

/// The base and head commits of the pull request, fetched into `repo`: GitHub's `pull/<n>/head` and the merge base
/// with its target branch; Azure DevOps' `pull/<n>/merge` commit and its first parent. Nothing in the checkout moves.
async fn fetch_shas(repo: &Path, address: &Address, base_branch: &str) -> Result<(String, String)> {
	match address.platform {
		Platform::Github => {
			let head = fetch_commit(repo, &format!("pull/{}/head", address.number)).await?;
			safe_ref(base_branch)?;
			let tip = fetch_commit(repo, base_branch).await?;
			let base = git_out(repo, &["merge-base", &head, &tip])
				.await?
				.filter(|sha| !sha.is_empty())
				.ok_or_else(|| Error::user("The pull request has no merge base with its target branch."))?;
			Ok((base, head))
		}
		Platform::Azure => {
			let merge = fetch_commit(repo, &format!("pull/{}/merge", address.number)).await?;
			let base = git_out(repo, &["rev-parse", &format!("{merge}^1")])
				.await?
				.filter(|sha| !sha.is_empty())
				.ok_or_else(|| Error::user("The pull request's merge commit has no parent."))?;
			Ok((base, merge))
		}
	}
}

/// `config/pr-review.md` without the lines above its first `---`, its placeholders filled. The title is outside text:
/// it goes in as one quoted line, and the description is fenced below the template.
fn template_text(title: &str, url: &str, base: &str, head: &str) -> Result<String> {
	let file = shipped_config_dir(None).join(TEMPLATE);
	let text = std::fs::read_to_string(&file).map_err(|error| Error::other(format!("{}: {error}", file.display())))?;
	let body = text.split_once("\n---\n").map_or(text.as_str(), |(_, body)| body);
	Ok(body
		.trim_start()
		.replace("{{title}}", &quoted(title))
		.replace("{{url}}", url)
		.replace("{{base}}", base)
		.replace("{{head}}", head))
}

/// The brief of a review thread: the template, the facts code gathered, then the outside text fenced (0681).
fn brief_text(target: &PrTarget, read: &Read, numstat: &str) -> Result<String> {
	let short = |sha: &str| sha[..sha.len().min(7)].to_owned();
	let mut text = template_text(&target.title, &target.url, &short(&target.base_sha), &short(&target.head_sha))?;
	let files: Vec<&str> = numstat.lines().filter(|line| !line.trim().is_empty()).collect();
	text.push_str(&format!("\n\n{}\n", facts_heading(&short(&target.head_sha))));
	text.push_str(&format!("- Pull request: {}\n", target.url));
	text.push_str(&format!("- Author: {}\n", quoted(&target.author)));
	text.push_str(&format!(
		"- Base: {} on {}; head: {}\n",
		short(&target.base_sha),
		quoted(&target.base_branch),
		short(&target.head_sha)
	));
	text.push_str(&format!("- CI: {}\n", target.ci));
	text.push_str(&format!("- Existing comment threads: {}\n", target.threads));
	text.push_str(&format!("- Changed files ({}; added/removed):\n", files.len()));
	for line in files.iter().take(FILES_LISTED) {
		let mut parts = line.splitn(3, '\t');
		let (added, removed, path) =
			(parts.next().unwrap_or("0"), parts.next().unwrap_or("0"), parts.next().unwrap_or(""));
		text.push_str(&format!("  {path} +{added} -{removed}\n"));
	}
	if files.len() > FILES_LISTED {
		text.push_str(&format!("  … {} more files\n", files.len() - FILES_LISTED));
	}
	let source = if target.platform == Platform::Github { "github-pr" } else { "azure-pr" };
	let title_and_body = format!("{}\n\n{}", read.title, read.body);
	text.push_str(&format!(
		"\nThe title and description, written by its author:\n{}\n",
		fence_external(source, &title_and_body, DESCRIPTION_LINES)
	));
	if !read.comments.is_empty() {
		text.push_str(&format!(
			"\nIts existing comments:\n{}\n",
			fence_external("pr-comments", &read.comments.join("\n"), COMMENT_LINES)
		));
	}
	Ok(text)
}

/// His `R` on a review request (0680): the review thread for the pull request at `url`, started without a turn of the
/// boss. The text it answers is the start's own.
pub async fn start(c: &Arc<ServiceContext>, url: &str) -> Result<String> {
	let address = Address::parse(url).ok_or_else(|| Error::user("Review: that link isn't a pull request."))?;
	open_event(c, "review", url)?;
	let reviewing = c.threads.all().into_iter().any(|thread| {
		thread.with(|t| t.task.as_ref().and_then(|task| task.pr.as_ref()).is_some_and(|pr| pr.url == url))
	});
	if reviewing {
		return Err(Error::user("Review: a thread is reviewing that pull request already."));
	}
	let identity = address.identity().ok_or_else(|| Error::user("Review: that link isn't a pull request."))?;
	let project = project_of(c, &identity)?;
	let read = match address.platform {
		Platform::Github => read_github(c, &address, url).await?,
		Platform::Azure => read_azure(c, &address).await?,
	};
	let (base_sha, head_sha) = fetch_shas(&project.main_path, &address, &read.base_branch).await?;
	let range = format!("{base_sha}...{head_sha}");
	let numstat = git_out(&project.main_path, &["diff", "--numstat", &range]).await?.unwrap_or_default();
	let target = PrTarget {
		platform: address.platform,
		url: url.to_owned(),
		number: address.number,
		repo: address.repo.clone(),
		org_url: address.org_url.clone(),
		project: address.project.clone(),
		title: one_line(&read.title),
		author: one_line(&read.author),
		base_branch: one_line(&read.base_branch),
		head_sha: head_sha.clone(),
		base_sha,
		ci: read.ci.clone(),
		threads: read.threads,
	};
	let brief = brief_text(&target, &read, &numstat)?;
	let title = format!("Review: {}", cut(&target.title, 50));
	let options = StartOptions { start_at: Some(head_sha), pr: Some(target), ..StartOptions::new(Door::Key) };
	Ok(c.starts.start_with(&project.name, &title, &brief, options).await?.text)
}

/// A review thread that was just made: its phase is Review, with the pull request's range and an empty notes file.
pub async fn begin(c: &Arc<ServiceContext>, thread: &ThreadRef) -> Result<()> {
	let t = thread.get();
	let (Some(task), Some(pr)) = (t.task.as_ref(), t.task.as_ref().and_then(|task| task.pr.as_ref())) else {
		return Ok(());
	};
	ReviewNotes { head: pr.head_sha.clone(), ..ReviewNotes::default() }.write(&review_file(&t.project.id, &task.id))?;
	let running = ReviewRunning {
		head: Some(pr.head_sha.clone()),
		at: now_ms(),
		commits: format!("pull request #{}", pr.number),
		range: format!("{}...{}", pr.base_sha, pr.head_sha),
		checks: None,
		skipped: None,
	};
	thread.with(|t| {
		set_phase(t, Phase::Review, "its pull request review started");
		t.review = Some(ThreadReview { running: Some(running), result: None });
	});
	c.status.write_task(thread);
	Ok(())
}

/// The brief's id for a review thread: `pr-<project id>-<n>`.
pub fn brief_id(task: &TaskRecord) -> Option<String> {
	task.pr.as_ref().map(|pr| format!("pr-{}-{}", task.project_id, pr.number))
}

/// The review thread's turn ended: its notes are re-checked at the head, stored, and the brief is built and opened by
/// the toast. The thread then waits for him; its pane sleeps once idle (0676).
pub async fn ready(c: &Arc<ServiceContext>, thread: &ThreadRef) -> Result<()> {
	let t = thread.get();
	let (Some(task), Some(pr)) = (t.task.clone(), t.task.as_ref().and_then(|task| task.pr.clone())) else {
		return Ok(());
	};
	let file = review_file(&t.project.id, &task.id);
	let mut notes = ReviewNotes::read(&file);
	recheck(&t.path, &pr.head_sha, &mut notes).await;
	notes.write(&file)?;
	let counts = notes.counts();
	c.journal.thread("review_ready", &t, json!({ "notes": counts.notes, "blocking": counts.blocking }));
	thread.with(|t| {
		t.review = Some(ThreadReview::default());
		show(t, crate::frontier::protocol::ThreadState::Done);
		set_phase(t, Phase::Task, "its pull request review is ready");
		t.sleep_when_idle = true;
	});
	c.status.write_task(thread);
	let brief = match build(&task, &t.project.name, &notes, &t.path).await {
		Ok(brief) => store::save(&brief).map(|()| brief.id).map_err(|error| error.to_string()),
		Err(error) => Err(error.to_string()),
	};
	let url = match (&brief, c.options.briefs.as_ref()) {
		(Ok(id), Some(briefs)) => Some(briefs.url(id)),
		(Err(error), _) => {
			c.warn(Error::other(format!("the brief of {} wasn't built: {error}", task.title)));
			None
		}
		_ => None,
	};
	let plural = if counts.notes == 1 { "" } else { "s" };
	let said = format!("review ready: {} note{plural}, {} blocking", counts.notes, counts.blocking);
	c.events.thread(thread, &said, ToastPriority::Normal, None).await;
	let pane_id = t.pane_id.clone();
	c.events.send(ServerMessage::Toast {
		reason: ToastReason::Done,
		level: ToastLevel::Info,
		text: format!("{} — {said}", task.title),
		pane_id: Some(pane_id),
		priority: None,
		url,
	});
	c.status.report().await?;
	Ok(())
}

/// Every note's proof read again against the head: the quote must still be where the note says.
async fn recheck(path: &Path, head: &str, notes: &mut ReviewNotes) {
	let mut files: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
	for card in &mut notes.notes {
		// A works-first card names no file: its proof is the command it ran (0706).
		if card.file.is_empty() {
			continue;
		}
		if !files.contains_key(&card.file) {
			let text = git_out_raw(path, head, &card.file).await.ok().flatten();
			files.insert(card.file.clone(), text);
		}
		let ok = files.get(&card.file).and_then(Option::as_ref).is_some_and(|text| reads(text, card.line, &card.quote));
		card.proof = Proof { sha: head.to_owned(), ok };
	}
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
	Fact { label: label.to_owned(), value: value.into(), url: None }
}

/// The page's top: the pull request, and where its review stands.
fn facts_section(task: &TaskRecord, project: &str, notes: &ReviewNotes) -> Option<Section> {
	let pr = task.pr.as_ref()?;
	let short = |sha: &str| sha[..sha.len().min(7)].to_owned();
	let counts = notes.counts();
	let edited = notes.notes.iter().filter(|card| card.edited.is_some()).count();
	let published = notes.notes.iter().filter(|card| card.published.is_some()).count();
	let mut rows = vec![
		fact("Project", project),
		Fact {
			label: "Pull request".to_owned(),
			value: format!("#{} {}", pr.number, pr.title),
			url: Some(pr.url.clone()),
		},
		fact("Author", pr.author.clone()),
		fact("Base", format!("{} on {}", short(&pr.base_sha), pr.base_branch)),
		fact("Head", short(&pr.head_sha)),
		fact("CI", pr.ci.clone()),
		fact("Existing comment threads", pr.threads.to_string()),
	];
	rows.push(fact(
		"Notes",
		format!("{} ({} blocking), {edited} edited, {published} published", counts.notes, counts.blocking),
	));
	if let Some(submitted) = &notes.submitted {
		rows.push(Fact {
			label: "Submitted".to_owned(),
			value: verdict_said(&submitted.verdict).to_owned(),
			url: Some(submitted.url.clone()),
		});
	}
	Some(Section::Facts { heading: None, rows })
}

/// A submitted verdict as the page says it.
pub fn verdict_said(verdict: &str) -> &'static str {
	match verdict {
		"approve" => "Approve",
		"request_changes" => "Request changes",
		_ => "Comment",
	}
}

/// The changed files grouped by top folder, with a link from each file to the files its cards mention.
fn map_section(numstat: &[(usize, usize, String)], notes: &ReviewNotes) -> Section {
	let mut groups: std::collections::BTreeMap<String, Vec<MapItem>> = std::collections::BTreeMap::new();
	for (added, removed, path) in numstat {
		let group = path.split_once('/').map_or("(root)", |(top, _)| top).to_owned();
		groups.entry(group).or_default().push(MapItem {
			id: path.clone(),
			label: path.rsplit('/').next().unwrap_or(path).to_owned(),
			added: *added,
			removed: *removed,
		});
	}
	let mut links: Vec<MapLink> = Vec::new();
	for card in &notes.notes {
		for (_, _, other) in numstat {
			let name = other.rsplit('/').next().unwrap_or(other);
			let mentioned = card.text.contains(other.as_str()) || (name.len() >= 4 && card.text.contains(name));
			let seen = links.iter().any(|link| link.a == card.file && link.b == *other);
			if *other != card.file && mentioned && !seen {
				links.push(MapLink { a: card.file.clone(), b: other.clone() });
			}
		}
	}
	Section::Map {
		heading: Some("Changed files".to_owned()),
		groups: groups.into_iter().map(|(label, items)| MapGroup { label, items }).collect(),
		links,
	}
}

/// `git diff --numstat` lines as `(added, removed, path)`.
fn numstat_rows(text: &str) -> Vec<(usize, usize, String)> {
	text.lines()
		.filter_map(|line| {
			let mut parts = line.splitn(3, '\t');
			let (added, removed, file) = (parts.next()?, parts.next()?, parts.next()?);
			Some((added.parse().unwrap_or(0), removed.parse().unwrap_or(0), file.to_owned()))
		})
		.collect()
}

/// The page's sections, in the order every review page has (0712): facts, works, system, cards, his verdict box, the
/// changed files and the diff.
fn assemble(
	task: &TaskRecord,
	project: &str,
	notes: &ReviewNotes,
	map: Option<Section>,
	diff: Option<Section>,
) -> Result<Brief> {
	let pr = task.pr.as_ref().ok_or_else(|| Error::other("the thread reviews no pull request"))?;
	let id = brief_id(task).ok_or_else(|| Error::other("the thread reviews no pull request"))?;
	let mut sections: Vec<Section> = facts_section(task, project, notes).into_iter().collect();
	if !notes.works.is_empty() {
		sections.push(Section::Works { heading: Some("Works".to_owned()), rows: notes.works.clone() });
	}
	sections.extend(notes.system.as_ref().map(|system| system.section()));
	// His own comments go here even before there is any note (0711).
	sections.push(Section::Cards { heading: Some("Review notes".to_owned()), cards: notes.notes.clone() });
	sections.push(Section::Verdict {
		heading: Some("Your review".to_owned()),
		summary: notes.summary.clone(),
		submitted: notes.submitted.clone(),
	});
	sections.extend(map);
	sections.extend(diff);
	let mut brief = Brief {
		id,
		kind: "pr".to_owned(),
		title: format!("Review: {}", pr.title),
		subtitle: format!("{project} · {} #{}", pr.repo, pr.number),
		generated_at_ms: now_ms(),
		sections,
	};
	lay_out(&mut brief).map_err(Error::other)?;
	Ok(brief)
}

/// The pull request's brief, built by code from the notes and git (`repo` holds the commits).
pub async fn build(task: &TaskRecord, project: &str, notes: &ReviewNotes, repo: &Path) -> Result<Brief> {
	let pr = task.pr.as_ref().ok_or_else(|| Error::other("the thread reviews no pull request"))?;
	let range = format!("{}...{}", pr.base_sha, pr.head_sha);
	let (stat_args, diff_args) =
		(["diff", "--numstat", range.as_str()], ["diff", "--no-color", "--no-ext-diff", range.as_str()]);
	let (numstat, diff) = tokio::join!(git_out(repo, &stat_args), git_out(repo, &diff_args));
	let rows = numstat_rows(&numstat?.unwrap_or_default());
	let diff = diff?.unwrap_or_default();
	let map = (!rows.is_empty()).then(|| map_section(&rows, notes));
	let diff = (!diff.is_empty())
		.then(|| Section::Diff { heading: Some("Diff".to_owned()), files: parse_diff(&diff, DIFF_LINES) });
	assemble(task, project, notes, map, diff)
}

/// The stored brief with its notes-derived sections (facts, works, system, cards, verdict) built again from `notes`:
/// what an action changed. The map and the diff stay as they were stored.
pub fn rebuilt(brief: &Brief, task: &TaskRecord, project: &str, notes: &ReviewNotes) -> Result<Brief> {
	let keep = |wanted: &str| brief.sections.iter().find(|section| section.type_name() == wanted).cloned();
	let mut next = assemble(task, project, notes, keep("map"), keep("diff"))?;
	next.generated_at_ms = brief.generated_at_ms;
	Ok(next)
}

/// Journal line for what he published: `comment` (one card) or `review` (a submitted review, 0712).
pub fn journal_published(c: &ServiceContext, task: &TaskRecord, platform: Platform, what: &str) {
	c.journal.append(
		"published",
		About { project: &task.project_id, thread: Some(&task.id) },
		json!({ "platform": platform.as_str(), "what": what }),
	);
}
