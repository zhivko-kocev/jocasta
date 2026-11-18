//! The service's answer to the pre-push hook of every pane (0613): a push that updates a protected branch is decided per
//! repo by the `push:` map of `projects.yaml` (0109), which his first Allow or Deny on that repo writes. `direct` passes
//! unasked, `pull-requests` refuses unasked, and a repo with no entry asks him through the Allow box (0484, 0558).
//!
//! `PushGate` is built after the context and takes it (`Arc<ServiceContext>`); it isn't a context field. The map of open
//! questions sits behind a std lock that is never held across an `.await`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use regex::Regex;
use serde_json::{Map, Value};
use tokio::sync::watch;

use crate::frontier::server::allow::{AskError, Confirmation};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::thread::subject;
use crate::frontier::server::wording::{Door, cut, quoted};
use crate::frontier::server::worktrees::git_out;
use crate::shared::errors::{Error, Result};
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::projects_file;
use crate::shared::projects_file::{PushWay, read_projects_file, set_push_way};
use crate::shared::push_hook::{NO_COMMIT, PushAnswer, PushRequest, PushedRef, is_protected};

/// How many commit subjects his box names before "and N more".
const SUBJECTS_NAMED: usize = 5;

/// The credentials of an address: `https://user:token@host` as `(scheme://)(user:token@)`.
static CREDENTIALS: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?i)^([a-z][a-z0-9+.-]*://)[^/@]*@").expect("a fixed pattern"));

/// `request` as the hook sends it (`POST /api/push`), or `None` when the body isn't one.
pub fn parse_push_request(body: &Map<String, Value>) -> Option<PushRequest> {
	let dir = body.get("dir")?.as_str()?;
	let remote = body.get("remote")?.as_str()?;
	let mut refs = Vec::new();
	for r#ref in body.get("refs")?.as_array()? {
		let text = |key: &str| r#ref.get(key).and_then(Value::as_str).map(str::to_owned);
		refs.push(PushedRef { r#ref: text("ref")?, local: text("local")?, remote: text("remote")? });
	}
	let pane_key = body.get("paneKey").and_then(Value::as_str).map(str::to_owned);
	Some(PushRequest { dir: dir.into(), remote: remote.to_owned(), refs, pane_key })
}

fn refuse(reason: impl Into<String>) -> PushAnswer {
	PushAnswer { allow: false, reason: Some(reason.into()) }
}

/// The address as his box shows it: without the user name and password an `https://user:token@host` carries.
fn without_credentials(address: &str) -> String {
	CREDENTIALS.replace(address, "${1}").into_owned()
}

/// The branch a ref names.
fn branch_name(r#ref: &PushedRef) -> String {
	r#ref.r#ref.replacen("refs/heads/", "", 1)
}

/// The updated branches' names, `main` or `main, develop`.
fn branches(updated: &[&PushedRef]) -> String {
	updated.iter().map(|r| branch_name(r)).collect::<Vec<_>>().join(", ")
}

/// A question open for one repo: the end of it is a `true` on the channel, after its entry left the map.
type Asking = HashMap<String, watch::Receiver<bool>>;

/// Holds the open question of one repo; dropping it (answered, failed or cancelled) removes it from the map and wakes
/// whoever waits for it.
struct Open<'a> {
	asking: &'a Mutex<Asking>,
	key: String,
	done: watch::Sender<bool>,
}

impl Drop for Open<'_> {
	fn drop(&mut self) {
		locked(self.asking).remove(&self.key);
		self.done.send_replace(true);
	}
}

fn locked(asking: &Mutex<Asking>) -> MutexGuard<'_, Asking> {
	asking.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct PushGate {
	ctx: Arc<ServiceContext>,
	/// The questions open per repo, so a second push to the same repo waits for the answer instead of asking again.
	asking: Mutex<Asking>,
}

impl PushGate {
	pub fn new(ctx: Arc<ServiceContext>) -> Self {
		Self { ctx, asking: Mutex::new(HashMap::new()) }
	}

	/// Takes the repo's turn to decide, or the channel of the one who has it.
	fn claim(&self, key: &str) -> std::result::Result<Open<'_>, watch::Receiver<bool>> {
		let mut asking = locked(&self.asking);
		if let Some(open) = asking.get(key) {
			return Err(open.clone());
		}
		let (done, receiver) = watch::channel(false);
		asking.insert(key.to_owned(), receiver);
		Ok(Open { asking: &self.asking, key: key.to_owned(), done })
	}

	/// Whether the push `request` describes may go ahead. Only the updates of main, master, develop and the project's own
	/// trunk are decided; any other push passes. A refusal's reason is for the agent that pushed.
	pub async fn verdict(&self, request: &PushRequest) -> Result<PushAnswer> {
		let c = &self.ctx;
		let Some(checkout) = c.places.checkout(&request.dir)? else {
			return Ok(refuse(format!(
				"Jocasta found no git repository at {}, so this push is refused.",
				request.dir.display()
			)));
		};
		// Also from a worktree: the answer belongs to the repo's main checkout.
		let repo = real_path(&checkout.main);
		let project = c.places.project_at(&repo)?;
		let trunk = match &project {
			Some(project) => c.places.trunk(project).await?,
			None => None,
		};
		let updated: Vec<&PushedRef> =
			request.refs.iter().filter(|r| is_protected(&r.r#ref, trunk.as_deref())).collect();
		if updated.is_empty() {
			return Ok(PushAnswer { allow: true, reason: None });
		}
		let name = project.map_or_else(
			|| repo.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
			|project| project.name,
		);
		let key = path_key(&repo);
		let turn = loop {
			match self.claim(&key) {
				Ok(turn) => break turn,
				Err(mut open) => {
					// Ends when the holder is done, or gone: either way its entry has left the map.
					let _ = open.wait_for(|done| *done).await;
				}
			}
		};
		let decided = self.decide(request, &repo, &name, &updated).await;
		drop(turn);
		match decided {
			Err(Error::User { message, .. }) => Ok(refuse(message)),
			other => other,
		}
	}

	/// What `projects.yaml` says about the repo, else his answer to the question it asks.
	async fn decide(
		&self,
		request: &PushRequest,
		repo: &Path,
		name: &str,
		updated: &[&PushedRef],
	) -> Result<PushAnswer> {
		let file = projects_file();
		let wanted = path_key(repo);
		let known = read_projects_file(Some(&file))?
			.push
			.into_iter()
			.find(|(key, _)| path_key(&real_path(Path::new(key))) == wanted)
			.map(|(_, entry)| entry);
		match known {
			Some(entry) if entry.way == PushWay::Direct => Ok(PushAnswer { allow: true, reason: None }),
			Some(entry) => Ok(refuse(takes_pull_requests(name, updated, &file.display().to_string(), entry.line))),
			None => self.ask(request, repo, name, updated).await,
		}
	}

	/// Asks him once for `repo` (0613) and keeps his Allow or Deny; no answer, or any end that isn't his Deny, keeps
	/// nothing.
	async fn ask(&self, request: &PushRequest, repo: &Path, name: &str, updated: &[&PushedRef]) -> Result<PushAnswer> {
		let file = projects_file();
		let shown = file.display();
		let mut lines = vec![format!(
			"{} asks to push to {} of {name} ({}).",
			self.who(request.pane_key.as_deref()),
			branches(updated),
			without_credentials(&request.remote)
		)];
		lines.extend(self.commit_lines(&request.dir, updated).await?);
		lines.push(format!(
			"Allow: {name} takes pushes straight to its protected branches from now on, unasked. Deny: it takes pull requests instead. Either is kept in {shown}."
		));
		let confirmation =
			Confirmation { message: format!("Push to {} in {name}?", branches(updated)), detail: lines.join("\n") };
		let repo_text = repo.display().to_string();
		match self.ctx.allow.ask(&confirmation, "nothing was pushed", Door::Boss).await {
			Ok(()) => {}
			Err(AskError::Denied(denied)) if denied.chose => {
				set_push_way(&repo_text, PushWay::PullRequests, Some(&file))?;
				return Ok(refuse(format!(
					"{} {name} now takes pull requests, as written in {shown}.",
					denied.message
				)));
			}
			Err(error) => return Err(error.into()),
		}
		set_push_way(&repo_text, PushWay::Direct, Some(&file))?;
		Ok(PushAnswer { allow: true, reason: None })
	}

	/// The pane that pushed, from its key: a thread, the boss, or a project pane.
	fn who(&self, pane_key: Option<&str>) -> String {
		let c = &self.ctx;
		let Some(pane_id) = c.launched.pane_of_key(pane_key) else {
			return "A process without a Jocasta pane key".to_owned();
		};
		if let Some(thread) = c.threads.get(&pane_id) {
			return format!("The thread {}", thread.with(|t| subject(t)));
		}
		let boss = c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		if boss.as_deref() == Some(pane_id.as_str()) { "The boss" } else { "A project pane" }.to_owned()
	}

	/// The commits the push brings, as lines for his box: their count and the first subjects; deletes say so.
	async fn commit_lines(&self, dir: &Path, updated: &[&PushedRef]) -> Result<Vec<String>> {
		let mut lines = Vec::new();
		for r#ref in updated {
			let branch = branch_name(r#ref);
			if NO_COMMIT.is_match(&r#ref.local) {
				lines.push(format!("It deletes {branch}."));
				continue;
			}
			// A new branch brings what no remote-tracking branch has yet.
			let range = if NO_COMMIT.is_match(&r#ref.remote) {
				vec![r#ref.local.clone(), "--not".to_owned(), "--remotes".to_owned()]
			} else {
				vec![format!("{}..{}", r#ref.remote, r#ref.local)]
			};
			let mut counting = vec!["rev-list".to_owned(), "--count".to_owned()];
			counting.extend(range.clone());
			let count = git_out(dir, &counting).await?.and_then(|out| out.parse::<usize>().ok()).unwrap_or(0);
			let mut logging = vec!["log".to_owned(), format!("-{SUBJECTS_NAMED}"), "--format=%s".to_owned()];
			logging.extend(range);
			let log = git_out(dir, &logging).await?.unwrap_or_default();
			let named: Vec<String> = log.split('\n').filter(|s| !s.is_empty()).map(|s| quoted(&cut(s, 72))).collect();
			let more = if count > named.len() { format!(", and {} more", count - named.len()) } else { String::new() };
			lines.push(if count > 0 {
				format!("{count} commit{} to {branch}: {}{more}.", if count == 1 { "" } else { "s" }, named.join(", "))
			} else {
				format!("Nothing new reaches {branch}.")
			});
		}
		Ok(lines)
	}
}

/// What a repo that takes pull requests tells the agent that pushed to its protected branch.
fn takes_pull_requests(name: &str, updated: &[&PushedRef], file: &str, line: usize) -> String {
	format!(
		"{name} takes pull requests, so a push to {} is refused: push a feature branch and open a pull request instead. The user chose this for the repo in {file}, line {line}, and can change it there.",
		branches(updated)
	)
}

#[cfg(test)]
#[path = "push_gate_tests.rs"]
mod tests;
