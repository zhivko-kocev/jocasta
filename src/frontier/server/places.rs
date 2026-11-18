//! Where folders sit (SVC-15): git places and the watched project holding a folder, each project's trunk, and the
//! project list kept through a malformed `projects.yaml` (CO-1). Kept 10 minutes, then looked up again; a catalog sync
//! forgets them at once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::frontier::server::day::SharedCatalog;
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::git::git;
use crate::shared::pathkey::{path_key, real_path, resolve, same_dir};
use crate::shared::projects::{Project, list_projects, sync_catalog};

/// How long git places and projects found for a folder are kept before being looked up again.
const PLACES_KEPT_MS: i64 = 10 * 60_000;

/// A checkout's place in git: its top-level, its main checkout and branch; `None` outside git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
	pub top: PathBuf,
	pub main: PathBuf,
	pub branch: Option<String>,
}

/// What [`Places`] tells its owner. A hook runs with the places locked: it must not call back into them.
pub struct PlacesHooks {
	/// A problem with `projects.yaml`, said once until it changes.
	pub problem: Box<dyn Fn(&str) + Send + Sync>,
	/// A sync ran: the panes are looked at again for threads (CR-12).
	pub synced: Box<dyn Fn() + Send + Sync>,
}

/// The parent of `path`; `None` at a root.
fn dirname(path: &Path) -> Option<&Path> {
	path.parent().filter(|parent| !parent.as_os_str().is_empty())
}

fn catalog_of(catalog: &SharedCatalog) -> MutexGuard<'_, rusqlite::Connection> {
	catalog.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What [`Places`] keeps behind its lock.
struct Inner {
	catalog: SharedCatalog,
	hooks: PlacesHooks,
	checkouts: HashMap<String, Option<Checkout>>,
	projects_by_dir: HashMap<String, Option<Project>>,
	/// Each watched project by its catalog paths' keys, for `project_at`.
	project_paths: Option<HashMap<String, Project>>,
	/// Each project's trunk, kept with the places (10 minutes, PERF-1).
	trunks: HashMap<String, Option<String>>,
	/// The last good project list, kept while `projects.yaml` is malformed (CO-1), and the problem said about it.
	last_projects: Vec<Project>,
	projects_problem: Option<String>,
	/// Worktrees of threads removed here, by path key, so their summaries still find their project (CO-2).
	removed_worktrees: HashMap<String, Project>,
	/// When the places were last emptied: they are kept 10 minutes.
	places_at: i64,
}

impl Inner {
	fn new(catalog: SharedCatalog, hooks: PlacesHooks) -> Self {
		Self {
			catalog,
			hooks,
			checkouts: HashMap::new(),
			projects_by_dir: HashMap::new(),
			project_paths: None,
			trunks: HashMap::new(),
			last_projects: Vec::new(),
			projects_problem: None,
			removed_worktrees: HashMap::new(),
			places_at: now_ms(),
		}
	}

	/// Where a directory sits in git, cached (until `forget`): the worktree's top-level, its main checkout and branch.
	/// One git call; a repo without a commit yet (no HEAD to name) takes two more.
	pub fn checkout(&mut self, dir: &Path) -> Result<Option<Checkout>> {
		let key = path_key(dir);
		if let Some(found) = self.checkouts.get(&key) {
			return Ok(found.clone());
		}
		let found = if dir.exists() { Self::git_checkout(dir)? } else { None };
		self.checkouts.insert(key, found.clone());
		Ok(found)
	}

	fn git_checkout(dir: &Path) -> Result<Option<Checkout>> {
		let places = ["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"];
		let with_branch = [places.as_slice(), &["--abbrev-ref", "HEAD"]].concat();
		let (top, common, branch);
		if let Some(out) = git(dir, &with_branch)? {
			let mut lines = out.split('\n');
			top = lines.next().map(str::to_owned);
			common = lines.next().map(str::to_owned);
			branch = lines.next().map(str::to_owned);
		} else {
			let out = git(dir, &places)?;
			let mut lines = out.as_deref().map(|out| out.split('\n'));
			top = lines.as_mut().and_then(Iterator::next).map(str::to_owned);
			common = lines.as_mut().and_then(Iterator::next).map(str::to_owned);
			branch = match &top {
				Some(top) if !top.is_empty() => git(dir, &["symbolic-ref", "--short", "-q", "HEAD"])?,
				_ => None,
			};
		}
		let (Some(top), Some(common)) = (top.filter(|top| !top.is_empty()), common.filter(|common| !common.is_empty()))
		else {
			return Ok(None);
		};
		let common = PathBuf::from(common);
		let top = PathBuf::from(top);
		let main = if common.file_name().is_some_and(|name| name == ".git") {
			dirname(&common).map_or_else(|| common.clone(), Path::to_path_buf)
		} else {
			top.clone()
		};
		Ok(Some(Checkout { top, main, branch: branch.filter(|branch| !branch.is_empty() && branch != "HEAD") }))
	}

	/// The watched project holding `dir` (cached until `forget`): the project whose catalog path is its git top-level
	/// or, for a herdr worktree, its main checkout; outside git, the nearest registered non-git ancestor (0105). Only
	/// `checkout`'s one git call. A folder gone with a thread `thread_finish` removed still names that thread's project
	/// (CO-2), so its summary is still indexed under it.
	pub fn project_at(&mut self, dir: &Path) -> Result<Option<Project>> {
		let key = path_key(dir);
		let cached = self.projects_by_dir.get(&key).cloned();
		if let Some(Some(project)) = &cached {
			return Ok(Some(project.clone()));
		}
		if !self.removed_worktrees.is_empty()
			&& !dir.exists()
			&& let Some(removed) = self.removed_project_above(dir)
		{
			self.projects_by_dir.insert(key, Some(removed.clone()));
			return Ok(Some(removed));
		}
		if let Some(known) = cached {
			return Ok(known);
		}
		if self.project_paths.is_none() {
			let paths = self
				.listed()
				.into_iter()
				.flat_map(|project| {
					let keys: Vec<String> = project.paths.iter().map(|path| path_key(&real_path(&path.path))).collect();
					keys.into_iter().map(move |key| (key, project.clone()))
				})
				.collect();
			self.project_paths = Some(paths);
		}
		let place = self.checkout(dir)?;
		let empty = HashMap::new();
		let paths = self.project_paths.as_ref().unwrap_or(&empty);
		let at = |path: &Path| paths.get(&path_key(&real_path(path)));
		let project = match place {
			Some(place) => at(&place.top).or_else(|| at(&place.main)).cloned(),
			None => {
				let mut found = None;
				let real = real_path(dir);
				let mut current = Some(real.as_path());
				while let Some(here) = current {
					if let Some(hit) = paths.get(&path_key(here)).filter(|hit| !hit.git) {
						found = Some(hit.clone());
					}
					if found.is_some() {
						break;
					}
					current = dirname(here);
				}
				found
			}
		};
		self.projects_by_dir.insert(key, project.clone());
		Ok(project)
	}

	/// The project of a removed thread's worktree that holds `dir`, walking up from it.
	fn removed_project_above(&self, dir: &Path) -> Option<Project> {
		let start = resolve(dir);
		let mut current = Some(start.as_path());
		while let Some(here) = current {
			if let Some(removed) = self.removed_worktrees.get(&path_key(here)) {
				return Some(removed.clone());
			}
			current = dirname(here);
		}
		None
	}

	/// The watched project whose main checkout is `dir` itself, the folder of its project pane (0617); `None` for any
	/// other folder, a thread's worktree included.
	pub fn project_pane_at(&mut self, dir: &Path) -> Result<Option<Project>> {
		Ok(self.project_at(dir)?.filter(|project| same_dir(&project.main_path, dir)))
	}

	/// Records a removed thread's worktree, by both spellings of its folder, as its project's (CO-2).
	pub fn removed(&mut self, path: &Path, project: &Project) {
		self.removed_worktrees.insert(path_key(path), project.clone());
		self.removed_worktrees.insert(path_key(&real_path(path)), project.clone());
	}

	/// The watched projects. A malformed `projects.yaml` (half-written, or mid-edit) keeps the last good list and is
	/// said once until it changes (CO-1): a timer that failed here would end the service.
	pub fn listed(&mut self) -> Vec<Project> {
		let read = list_projects(&catalog_of(&self.catalog), None);
		match read {
			Ok(projects) => {
				self.last_projects = projects;
				self.projects_problem = None;
			}
			Err(error) => self.said(format!("projects.yaml: {error}; showing the last good list.")),
		}
		self.last_projects.clone()
	}

	fn said(&mut self, problem: String) {
		if self.projects_problem.as_deref() == Some(problem.as_str()) {
			return;
		}
		(self.hooks.problem)(&problem);
		self.projects_problem = Some(problem);
	}

	/// A project by id or name (any case); an unknown one is a refusal.
	pub fn find(&self, name_or_id: &str) -> Result<Project> {
		let wanted = name_or_id.trim().to_lowercase();
		list_projects(&catalog_of(&self.catalog), None)?
			.into_iter()
			.find(|project| project.id == name_or_id || project.name.to_lowercase() == wanted)
			.ok_or_else(|| Error::user(format!("no project {name_or_id}.")))
	}

	/// Forgets cached git places, trunks and projects: the watched roots or a checkout's branch may have changed.
	pub fn forget(&mut self) {
		self.checkouts.clear();
		self.projects_by_dir.clear();
		self.trunks.clear();
		self.project_paths = None;
		self.places_at = now_ms();
	}

	/// Forgets the places once they are older than they are kept, as of `now`.
	pub fn expire(&mut self, now: i64) {
		if now - self.places_at > PLACES_KEPT_MS {
			self.forget();
		}
	}
}

/// Where folders sit: see the module's note. Every method takes `&self`: the state sits behind one lock that is never
/// held across an `.await`, so one `Places` is shared as part of the service's context.
pub struct Places {
	catalog: SharedCatalog,
	inner: Mutex<Inner>,
}

impl Places {
	pub fn new(catalog: SharedCatalog, hooks: PlacesHooks) -> Self {
		Self { inner: Mutex::new(Inner::new(catalog.clone(), hooks)), catalog }
	}

	fn state(&self) -> MutexGuard<'_, Inner> {
		self.inner.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Where a directory sits in git, cached (until `forget`): the worktree's top-level, its main checkout and branch.
	/// One git call; a repo without a commit yet (no HEAD to name) takes two more.
	pub fn checkout(&self, dir: &Path) -> Result<Option<Checkout>> {
		self.state().checkout(dir)
	}

	/// The watched project holding `dir` (cached until `forget`): the project whose catalog path is its git top-level
	/// or, for a herdr worktree, its main checkout; outside git, the nearest registered non-git ancestor (0105). Only
	/// `checkout`'s one git call. A folder gone with a thread `thread_finish` removed still names that thread's project
	/// (CO-2), so its summary is still indexed under it.
	pub fn project_at(&self, dir: &Path) -> Result<Option<Project>> {
		self.state().project_at(dir)
	}

	/// The watched project whose main checkout is `dir` itself, the folder of its project pane (0617); `None` for any
	/// other folder, a thread's worktree included.
	pub fn project_pane_at(&self, dir: &Path) -> Result<Option<Project>> {
		self.state().project_pane_at(dir)
	}

	/// Records a removed thread's worktree, by both spellings of its folder, as its project's (CO-2).
	pub fn removed(&self, path: &Path, project: &Project) {
		self.state().removed(path, project);
	}

	/// The watched projects. A malformed `projects.yaml` (half-written, or mid-edit) keeps the last good list and is
	/// said once until it changes (CO-1): a timer that failed here would end the service.
	pub fn listed(&self) -> Vec<Project> {
		self.state().listed()
	}

	/// A project by id or name (any case); an unknown one is a refusal.
	pub fn find(&self, name_or_id: &str) -> Result<Project> {
		self.state().find(name_or_id)
	}

	/// Forgets cached git places, trunks and projects: the watched roots or a checkout's branch may have changed.
	pub fn forget(&self) {
		self.state().forget();
	}

	/// Forgets the places once they are older than they are kept, as of `now`.
	pub fn expire(&self, now: i64) {
		self.state().expire(now);
	}

	/// Projects from the watched roots into the catalog (0105), and the cached places with them. A sync that fails (a
	/// malformed `projects.yaml`, C8) is logged and said once; the catalog goes on as it was. `sync_catalog` holds the
	/// connection across git's awaits and so isn't `Send`: it runs on a blocking thread, with the catalog locked.
	pub async fn sync(&self) {
		let catalog = self.catalog.clone();
		let handle = tokio::runtime::Handle::current();
		let done = tokio::task::spawn_blocking(move || {
			let db = catalog_of(&catalog);
			handle.block_on(sync_catalog(&db, None))
		})
		.await;
		let result = done.unwrap_or_else(|error| Err(Error::other(error.to_string())));
		let mut state = self.state();
		if let Err(error) = result {
			state.said(format!("projects.yaml: {error}; showing the last good list."));
		}
		state.forget();
		(state.hooks.synced)();
	}

	/// The trunk threads branch from and land on: the remote's default branch (`origin/HEAD`), else `main`, `master` or
	/// `trunk`, else the main checkout's branch; `None` when none of them exists. Asked of git off the event loop and
	/// kept with the places, 10 minutes (PERF-1).
	pub async fn trunk(&self, project: &Project) -> Result<Option<String>> {
		if let Some(known) = self.state().trunks.get(&project.id) {
			return Ok(known.clone());
		}
		let main = project.main_path.as_path();
		let mut found: Option<String> = None;
		let remote = git_out(main, &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"]).await?;
		if let Some(remote) = remote.filter(|remote| !remote.is_empty()) {
			found = Some(remote.strip_prefix("origin/").unwrap_or(&remote).to_owned());
		}
		for name in ["main", "master", "trunk"] {
			if found.is_some() {
				break;
			}
			let verify = ["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")];
			if git_out(main, &verify).await?.is_some_and(|out| !out.is_empty()) {
				found = Some(name.to_owned());
			}
		}
		if found.as_deref().is_none_or(str::is_empty) {
			found = git_out(main, &["symbolic-ref", "--short", "-q", "HEAD"]).await?;
		}
		let found = found.filter(|found| !found.is_empty());
		self.state().trunks.insert(project.id.clone(), found.clone());
		Ok(found)
	}
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
