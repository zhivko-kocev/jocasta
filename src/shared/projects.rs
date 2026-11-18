//! The catalog's projects (0103-0112, 0582): what the projects file and the scan register in `catalog.db`, and
//! `jocasta project add` and `remove`.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use futures_util::StreamExt;
use globset::{GlobBuilder, GlobMatcher};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};

use crate::shared::db::now_iso;
use crate::shared::errors::{Error, Result};
use crate::shared::git::{GitOptions, git_async};
use crate::shared::identity::{default_project_name, identity_key, project_id_for};
use crate::shared::log::{LogLevel, log};
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::projects_file;
use crate::shared::projects_file::{ListKey, ProjectsFile, add_to_list, read_projects_file, remove_from_list};

/// What a path is to its project: its main checkout, a linked worktree, or another clone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRole {
	Main,
	Worktree,
	Clone,
}

impl PathRole {
	/// The word the catalog stores.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Main => "main",
			Self::Worktree => "worktree",
			Self::Clone => "clone",
		}
	}

	fn parse(text: &str) -> Option<Self> {
		match text {
			"main" => Some(Self::Main),
			"worktree" => Some(Self::Worktree),
			"clone" => Some(Self::Clone),
			_ => None,
		}
	}
}

/// One of a project's folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPath {
	pub path: PathBuf,
	pub role: PathRole,
}

/// One project of the catalog (0110, 0112).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
	pub id: String,
	pub identity: String,
	pub name: String,
	pub main_path: PathBuf,
	pub git: bool,
	pub paths: Vec<ProjectPath>,
}

/// `project add`'s result: the project, and the linked worktree he named when it was its main checkout that got added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedProject {
	pub project: Project,
	pub main_checkout_of: Option<PathBuf>,
}

struct PathRow {
	path: String,
	project_id: String,
	role: PathRole,
	git_config_mtime: Option<i64>,
}

impl PathRow {
	/// A row read with the columns `project_paths.*` lists, in the table's order.
	fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
		let role: String = row.get(2)?;
		Ok(Self {
			path: row.get(0)?,
			project_id: row.get(1)?,
			role: PathRole::parse(&role).unwrap_or(PathRole::Clone),
			git_config_mtime: row.get(3)?,
		})
	}
}

const PATH_COLUMNS: &str = "path, project_id, role, git_config_mtime, added_at";

/// How many paths have their identity resolved at once (each is one or two git calls).
const GIT_PARALLEL: usize = 4;

/// `path.resolve(base, rel)` without the file system: `rel` against `base`, `.` and `..` folded away.
fn resolve_from(base: &Path, rel: &Path) -> PathBuf {
	let joined = base.join(rel);
	let mut out = PathBuf::new();
	for part in joined.components() {
		match part {
			Component::CurDir => {}
			Component::ParentDir => {
				if matches!(out.components().next_back(), Some(Component::Normal(_))) {
					out.pop();
				}
			}
			other => out.push(other.as_os_str()),
		}
	}
	out
}

/// The home dir and filesystem or drive roots are never projects (0106).
fn is_never_project(path: &Path) -> bool {
	let real = real_path(path);
	let home = std::env::home_dir().unwrap_or_default();
	real.parent().is_none() || path_key(&real) == path_key(&real_path(&home))
}

/// A checkout's git layout, read without git (0104, 0108, SHR-3).
struct GitLayout {
	/// The common git dir: `<main>/.git` for a checkout and its linked worktrees.
	common: PathBuf,
	/// A linked worktree: its `.git` is a file whose git dir holds `commondir` (a submodule's has none).
	linked: bool,
	/// mtime of the common dir's `config` (0108), or `None` when it can't be read.
	mtime: Option<i64>,
}

static GITDIR_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?m)^gitdir:\s*(.+)$").expect("the gitdir line pattern"));

/// What a `.git` file points at: the common dir, whether it is a linked worktree's, and whether it named a git dir
/// at all (an empty `gitdir:` leaves the layout without an mtime).
fn linked_layout(dir: &Path, dot_git: &Path) -> Option<(PathBuf, bool, bool)> {
	let text = fs::read_to_string(dot_git).ok()?;
	let Some(gitdir) = GITDIR_LINE.captures(&text).map(|found| found[1].trim().to_owned()).filter(|g| !g.is_empty())
	else {
		return Some((dot_git.to_path_buf(), false, false));
	};
	let git_dir =
		if Path::new(&gitdir).is_absolute() { PathBuf::from(&gitdir) } else { resolve_from(dir, Path::new(&gitdir)) };
	let commondir = git_dir.join("commondir");
	if !commondir.exists() {
		return Some((git_dir, false, true));
	}
	let named = fs::read_to_string(&commondir).ok()?;
	Some((resolve_from(&git_dir, Path::new(named.trim())), true, true))
}

/// `dir`'s git layout, or `None` when it has no `.git`.
fn git_layout(dir: &Path) -> Option<GitLayout> {
	let dot_git = dir.join(".git");
	let (common, linked, named) =
		if fs::metadata(&dot_git).ok()?.is_file() { linked_layout(dir, &dot_git)? } else { (dot_git, false, true) };
	let mtime = named
		.then(|| fs::metadata(common.join("config")).and_then(|meta| meta.modified()).ok())
		.flatten()
		.and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
		.and_then(|since| i64::try_from(since.as_millis()).ok());
	Some(GitLayout { common, linked, mtime })
}

fn is_git_dir(dir: &Path) -> bool {
	dir.join(".git").exists()
}

/// Git top-levels under a root, down to `depth` levels, skipping `ignore` globs and never-projects (0105, 0106).
fn scan_root(root_path: &str, depth: u64, ignore: &[String]) -> Result<Vec<PathBuf>> {
	let globs = ignore
		.iter()
		.map(|pattern| {
			GlobBuilder::new(pattern)
				.literal_separator(true)
				.build()
				.map(|glob| glob.compile_matcher())
				.map_err(|error| Error::other(format!("{pattern}: {error}")))
		})
		.collect::<Result<Vec<GlobMatcher>>>()?;
	let mut found = Vec::new();
	walk(&real_path(Path::new(root_path)), 1, depth, &globs, &mut found);
	Ok(found)
}

fn walk(dir: &Path, level: u64, depth: u64, globs: &[GlobMatcher], found: &mut Vec<PathBuf>) {
	// Unreadable folders (`System Volume Information`) are not projects.
	let Ok(read) = fs::read_dir(dir) else { return };
	// Name order on every OS (NTFS lists it; ext4 and APFS don't), so "the first one found" is the same path (0104).
	let mut entries: Vec<(String, bool)> = read
		.filter_map(|entry| entry.ok())
		.map(|entry| {
			(entry.file_name().to_string_lossy().into_owned(), entry.file_type().is_ok_and(|kind| kind.is_dir()))
		})
		.collect();
	entries.sort_by(|a, b| a.0.cmp(&b.0));
	for (name, is_dir) in entries {
		if !is_dir || globs.iter().any(|glob| glob.is_match(&name)) {
			continue;
		}
		let child = dir.join(&name);
		if is_git_dir(&child) {
			if !is_never_project(&child) {
				found.push(real_path(&child));
			}
		} else if level < depth {
			walk(&child, level + 1, depth, globs, found);
		}
	}
}

/// One path's catalog row as the sync decided it, before its write.
struct PathPlan {
	key: String,
	path: PathBuf,
	mtime: Option<i64>,
	identity: String,
	linked: bool,
}

/// What the sync does about one candidate path, before any write: `None` leaves it as it was.
async fn plan_path(known: &HashMap<String, (PathRow, String)>, key: &str, path: &Path) -> Option<PathPlan> {
	let layout = git_layout(path);
	let mtime = layout.as_ref().and_then(|found| found.mtime);
	let row = known.get(key);
	let path_keyed = row.is_some_and(|(_, identity)| identity.starts_with("path:"));
	if row.is_some_and(|(row, _)| row.git_config_mtime == mtime) && !(layout.is_some() && path_keyed) {
		return None;
	}
	let identity = if layout.is_some() {
		match identity_key(path).await {
			Ok(identity) => identity,
			Err(error) => {
				// A git that hangs or can't run leaves this path as it was until the next sync.
				log(LogLevel::Warn, "projects", &format!("{}: {error}", path.display()), None);
				return None;
			}
		}
	} else {
		format!("path:{key}")
	};
	if row.is_some_and(|(row, known_identity)| row.git_config_mtime == mtime && *known_identity == identity) {
		return None;
	}
	Some(PathPlan {
		key: key.to_owned(),
		path: path.to_path_buf(),
		mtime,
		identity,
		linked: layout.is_some_and(|found| found.linked),
	})
}

/// The paths the file and the scan want, by key, in the order they were found.
fn candidates_of(file: &ProjectsFile, excluded: &HashSet<String>) -> Result<Vec<(String, PathBuf)>> {
	let mut candidates: Vec<(String, PathBuf)> = Vec::new();
	let mut consider = |path: PathBuf| {
		let key = path_key(&path);
		if excluded.contains(&key) || is_never_project(&path) {
			return;
		}
		match candidates.iter_mut().find(|(known, _)| *known == key) {
			Some(slot) => slot.1 = path,
			None => candidates.push((key, path)),
		}
	};
	for root in &file.roots {
		for path in scan_root(&root.path, root.depth, &root.ignore)? {
			consider(path);
		}
	}
	for added in &file.add {
		let path = real_path(Path::new(added));
		if path.exists() {
			consider(path);
		}
	}
	Ok(candidates)
}

fn known_rows(db: &Connection) -> Result<Vec<(PathRow, String)>> {
	let mut query = db.prepare(
		"SELECT project_paths.path, project_paths.project_id, project_paths.role, project_paths.git_config_mtime, \
		 project_paths.added_at, projects.identity FROM project_paths JOIN projects ON projects.id = project_paths.project_id",
	)?;
	let rows = query.query_map([], |row| Ok((PathRow::read(row)?, row.get::<_, String>(5)?)))?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn path_rows(db: &Connection) -> Result<Vec<PathRow>> {
	let mut query = db.prepare(&format!("SELECT {PATH_COLUMNS} FROM project_paths"))?;
	let rows = query.query_map([], PathRow::read)?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Writes one plan: its project (made when new) and its path's row.
fn write_plan(db: &Connection, plan: &PathPlan, row: Option<&PathRow>, now: &str) -> Result<()> {
	let project_id = project_id_for(&plan.identity);
	db.execute(
		"INSERT OR IGNORE INTO projects (id, identity, created_at) VALUES (?, ?, ?)",
		params![project_id, plan.identity, now],
	)?;
	let plan_path = plan.path.to_string_lossy();
	let has_main = db
		.query_row(
			"SELECT 1 FROM project_paths WHERE project_id = ? AND role = 'main' AND path != ?",
			params![project_id, row.map_or(plan_path.as_ref(), |row| row.path.as_str())],
			|_| Ok(()),
		)
		.optional()?
		.is_some();
	let role = if plan.linked {
		PathRole::Worktree
	} else if has_main {
		PathRole::Clone
	} else {
		PathRole::Main
	};
	if let Some(row) = row {
		db.execute(
			"UPDATE project_paths SET project_id = ?, role = ?, git_config_mtime = ? WHERE path = ?",
			params![project_id, role.as_str(), plan.mtime, row.path],
		)?;
	} else {
		db.execute(
			"INSERT INTO project_paths (path, project_id, role, git_config_mtime, added_at) VALUES (?, ?, ?, ?, ?)",
			params![plan_path.as_ref(), project_id, role.as_str(), plan.mtime, now],
		)?;
	}
	Ok(())
}

/// A project without a main path promotes its oldest clone; one with neither is dropped.
fn settle_mains(db: &Connection) -> Result<()> {
	let ids: Vec<String> =
		db.prepare("SELECT id FROM projects")?.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
	for id in ids {
		let mut query = db.prepare(&format!(
			"SELECT {PATH_COLUMNS} FROM project_paths WHERE project_id = ? AND role != 'worktree' ORDER BY role = 'main' DESC, added_at"
		))?;
		let roots: Vec<PathRow> = query.query_map([&id], PathRow::read)?.collect::<rusqlite::Result<_>>()?;
		match roots.first() {
			None => {
				db.execute("DELETE FROM projects WHERE id = ?", [&id])?;
			}
			Some(first) if first.role != PathRole::Main => {
				db.execute("UPDATE project_paths SET role = 'main' WHERE path = ?", [&first.path])?;
			}
			Some(_) => {}
		}
	}
	Ok(())
}

/// Brings `catalog.db` in line with the projects file plus the scan (0108, 0109): every launch lists the roots, and runs
/// git only for new paths, a changed `.git/config`, or a git checkout still keyed by its path because it was first seen
/// before its first commit (SHR-2), so it becomes `root:<sha>` once it has one (0103). Git runs asynchronously before
/// the write; the write is one short transaction with no git in it (SHR-3). `file` defaults to the projects file.
///
/// The future holds `db` across git's awaits, so it isn't `Send`: run it where the connection lives.
pub async fn sync_catalog(db: &Connection, file: Option<&ProjectsFile>) -> Result<()> {
	let read;
	let file = match file {
		Some(file) => file,
		None => {
			read = read_projects_file(None)?;
			&read
		}
	};
	let excluded: HashSet<String> = file.exclude.iter().map(|p| path_key(&real_path(Path::new(p)))).collect();
	let candidates = candidates_of(file, &excluded)?;

	let known = known_rows(db)?;
	let missing: HashSet<String> = known
		.iter()
		.filter(|(row, _)| !Path::new(&row.path).exists())
		.map(|(row, _)| path_key(Path::new(&row.path)))
		.collect();
	let by_key: HashMap<String, (PathRow, String)> =
		known.into_iter().map(|(row, identity)| (path_key(Path::new(&row.path)), (row, identity))).collect();
	// Worktree paths that are gone are checked before the write, so the transaction does no I/O.
	let plans: Vec<Option<PathPlan>> = futures_util::stream::iter(candidates.iter())
		.map(|(key, path)| plan_path(&by_key, key, path))
		.buffered(GIT_PARALLEL)
		.collect()
		.await;

	let now = now_iso();
	let tx = db.unchecked_transaction()?;
	// Read again inside the write: another sync may have written since the plan was made.
	let rows = path_rows(&tx)?;
	let current: HashMap<String, usize> =
		rows.iter().enumerate().map(|(at, row)| (path_key(Path::new(&row.path)), at)).collect();
	for plan in plans.iter().flatten() {
		write_plan(&tx, plan, current.get(&plan.key).map(|&at| &rows[at]), &now)?;
	}
	// Paths no longer wanted: main and clone paths outside the file and scan; worktrees that are gone.
	let wanted: HashSet<&str> = candidates.iter().map(|(key, _)| key.as_str()).collect();
	for row in &rows {
		let key = path_key(Path::new(&row.path));
		let gone = if row.role == PathRole::Worktree {
			missing.contains(&key) || excluded.contains(&key)
		} else {
			!wanted.contains(key.as_str())
		};
		if gone {
			tx.execute("DELETE FROM project_paths WHERE path = ?", [&row.path])?;
		}
	}
	settle_mains(&tx)?;
	tx.commit()?;
	Ok(())
}

/// ICU-like order of one character for `localeCompare`: punctuation and symbols, then digits, then letters (case and
/// accents second); anything else by code point after them.
fn collation_primary(c: char) -> (u8, u32) {
	const PUNCTUATION: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
	if let Some(at) = PUNCTUATION.find(c) {
		return (0, at as u32);
	}
	if c.is_ascii_digit() {
		return (1, u32::from(c));
	}
	let lower = c.to_lowercase().next().unwrap_or(c);
	if lower.is_alphabetic() { (2, u32::from(lower)) } else { (3, u32::from(lower)) }
}

/// `a.localeCompare(b)` approximated: by [`collation_primary`] first, then lowercase before uppercase.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
	let primary = a.chars().map(collation_primary).cmp(b.chars().map(collation_primary));
	primary.then_with(|| {
		a.chars()
			.zip(b.chars())
			.map(|(x, y)| y.is_lowercase().cmp(&x.is_lowercase()))
			.find(|o| o.is_ne())
			.unwrap_or(Ordering::Equal)
	})
}

/// Every project in the catalog, by display name (0110, 0112). `file` defaults to the projects file.
pub fn list_projects(db: &Connection, file: Option<&ProjectsFile>) -> Result<Vec<Project>> {
	let read;
	let file = match file {
		Some(file) => file,
		None => {
			read = read_projects_file(None)?;
			&read
		}
	};
	let projects: Vec<(String, String)> = db
		.prepare("SELECT id, identity FROM projects")?
		.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
		.collect::<rusqlite::Result<_>>()?;
	let paths: Vec<PathRow> = {
		let mut query = db.prepare(&format!("SELECT {PATH_COLUMNS} FROM project_paths ORDER BY added_at"))?;
		query.query_map([], PathRow::read)?.collect::<rusqlite::Result<_>>()?
	};
	let mut listed: Vec<Project> = projects
		.into_iter()
		.map(|(id, identity)| {
			let own: Vec<&PathRow> = paths.iter().filter(|row| row.project_id == id).collect();
			let main_path = own
				.iter()
				.find(|row| row.role == PathRole::Main)
				.or_else(|| own.first())
				.map(|row| PathBuf::from(&row.path))
				.unwrap_or_default();
			let name = file
				.names
				.iter()
				.find(|(named, _)| *named == id)
				.map_or_else(|| default_project_name(&identity, &main_path), |(_, name)| name.clone());
			Project {
				git: !identity.starts_with("path:"),
				paths: own.iter().map(|row| ProjectPath { path: PathBuf::from(&row.path), role: row.role }).collect(),
				id,
				identity,
				name,
				main_path,
			}
		})
		.collect();
	listed.sort_by(|a, b| locale_compare(&a.name, &b.name).then_with(|| locale_compare(&a.id, &b.id)));
	Ok(listed)
}

/// `jocasta project add <path>` (0111): registers a folder, persisting it under `add` when the scan doesn't find it. A
/// linked worktree adds its main checkout instead (0582), named in `main_checkout_of`. The file is written only once
/// the path is known to register. `file` defaults to the projects file.
pub async fn add_project(db: &Connection, path: &Path, file: Option<&Path>) -> Result<AddedProject> {
	let file = file.map_or_else(projects_file, Path::to_path_buf);
	let real = real_path(path);
	if !fs::metadata(&real).is_ok_and(|meta| meta.is_dir()) {
		return Err(Error::user(format!("{} is not a folder.", real.display())));
	}
	if is_never_project(&real) {
		return Err(Error::user(format!("{} can't be a project: home and drive roots never are.", real.display())));
	}
	let top = git_async(&real, &["rev-parse", "--show-toplevel"], 30_000, GitOptions::default()).await?;
	let mut target = if top.code == Some(0) && !top.stdout.trim().is_empty() {
		real_path(Path::new(top.stdout.trim()))
	} else {
		real
	};
	let mut main_checkout_of = None;
	let layout = if is_git_dir(&target) { git_layout(&target) } else { None };
	if let Some(layout) = layout.filter(|layout| layout.linked) {
		// A worktree's main checkout holds its common dir as `.git`; a bare repository has no main checkout.
		let main = layout.common.parent().map(Path::to_path_buf).unwrap_or_default();
		if layout.common.file_name().is_none_or(|name| name != ".git") || !is_git_dir(&main) {
			return Err(Error::user(format!(
				"{} is a worktree of the bare repository {}; add a checkout of it instead.",
				target.display(),
				layout.common.display()
			)));
		}
		main_checkout_of = Some(target);
		target = real_path(&main);
	}
	if is_never_project(&target) {
		return Err(Error::user(format!("{} can't be a project: home and drive roots never are.", target.display())));
	}
	let contents = read_projects_file(Some(&file))?;
	let target_key = path_key(&target);
	let unexcluded = ProjectsFile {
		exclude: contents
			.exclude
			.iter()
			.filter(|p| path_key(&real_path(Path::new(p))) != target_key)
			.cloned()
			.collect(),
		..contents
	};
	let mut found = find_path(db, &target, &unexcluded).await?;
	let listed = found.is_none();
	if found.is_none() {
		let mut with_add = unexcluded.clone();
		with_add.add.push(target.to_string_lossy().into_owned());
		found = find_path(db, &target, &with_add).await?;
	}
	let Some(project) = found else {
		return Err(Error::user_with_code(
			format!("{} can't be a project; {} is left as it was.", target.display(), file.display()),
			1,
		));
	};
	remove_from_list(ListKey::Exclude, &target.to_string_lossy(), Some(&file))?;
	if listed {
		add_to_list(ListKey::Add, &target.to_string_lossy(), Some(&file))?;
	}
	Ok(AddedProject { project, main_checkout_of })
}

async fn find_path(db: &Connection, path: &Path, contents: &ProjectsFile) -> Result<Option<Project>> {
	sync_catalog(db, Some(contents)).await?;
	let key = path_key(path);
	Ok(list_projects(db, Some(contents))?.into_iter().find(|p| p.paths.iter().any(|row| path_key(&row.path) == key)))
}

/// `jocasta project remove <name|id>` (0111): drops it from the catalog and excludes its paths; files are never touched.
/// `file` defaults to the projects file.
pub async fn remove_project(db: &Connection, name_or_id: &str, file: Option<&Path>) -> Result<Project> {
	let file = file.map_or_else(projects_file, Path::to_path_buf);
	let contents = read_projects_file(Some(&file))?;
	sync_catalog(db, Some(&contents)).await?;
	let projects = list_projects(db, Some(&contents))?;
	let by_id = projects.iter().find(|p| p.id == name_or_id);
	let wanted = name_or_id.to_lowercase();
	let by_name: Vec<&Project> = projects.iter().filter(|p| p.name.to_lowercase() == wanted).collect();
	if by_id.is_none() && by_name.len() > 1 {
		let ids: Vec<&str> = by_name.iter().map(|p| p.id.as_str()).collect();
		return Err(Error::user(format!(
			"{} projects are named {name_or_id}: {}. Use the id.",
			by_name.len(),
			ids.join(", ")
		)));
	}
	let Some(project) = by_id.or_else(|| by_name.first().copied()) else {
		return Err(Error::user(format!("no project named {name_or_id}. See jocasta project list.")));
	};
	for p in &project.paths {
		if p.role == PathRole::Worktree {
			continue;
		}
		remove_from_list(ListKey::Add, &p.path.to_string_lossy(), Some(&file))?;
		add_to_list(ListKey::Exclude, &p.path.to_string_lossy(), Some(&file))?;
	}
	let project = project.clone();
	sync_catalog(db, Some(&read_projects_file(Some(&file))?)).await?;
	Ok(project)
}

#[cfg(test)]
#[path = "projects_tests.rs"]
mod tests;
