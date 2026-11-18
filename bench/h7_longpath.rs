//! H7 (0482): long-path robustness of each tool's default worktree location, with Git's default settings.
//!
//!   jocasta-bench h7-longpath --out <dir> [--root <dir>] [--lengths 100,110,…,240] [--runs 1]
//!                             [--home <dir>] [--location name=<template>] [--dry-run]
//!
//! For each L it builds a fixture repo `<root>/acme-app` whose deepest file has a relative path of exactly L characters,
//! then checks a branch out at each location with `git worktree add` and runs `git status` there. Git runs with no
//! global or system config (`GIT_CONFIG_GLOBAL` an empty file, `GIT_CONFIG_NOSYSTEM=1`), so `core.longpaths` is Git's
//! default. Locations (`<repo>` the main checkout, `<name>` its folder name, `<slug>` the branch):
//!   jocasta      <parent>/<name>.wt/<slug>              (`service.ts` worktree path, CHANGELOG 0457)
//!   herdr        <home>/.herdr/worktrees/<name>/<slug>  (herdr's default)
//!   claude       <repo>/.claude/worktrees/<slug>        (Claude Code `--worktree`; confirm in the VM)
//! `--location name=<template>` adds one (vibe-kanban's folder once confirmed there). `--home` defaults to a temp
//! folder, so nothing lands in his home. Writes h7-longpath.jsonl and h7-longpath.json (the largest passing L).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag, number_list_flag, parse_bench_args, run_script};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::temp::mkdtemp;
use jocasta::shared::env;
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::{normalize, remove_tree};
use jocasta::shared::pathkey::resolve;
use jocasta::shared::process::{RunOptions, RunResult, run_program_sync};

pub const DEFAULT_LOCATIONS: [(&str, &str); 3] = [
	("jocasta", "<parent>/<name>.wt/<slug>"),
	("herdr", "<home>/.herdr/worktrees/<name>/<slug>"),
	("claude", "<repo>/.claude/worktrees/<slug>"),
];

pub const DEFAULT_LENGTHS: [u64; 15] = [100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240];

/// A relative path (with `/`) of exactly `length` characters: folders of up to 20 characters, then `file.txt`.
pub fn deep_path(length: usize) -> Result<String> {
	if length < 10 {
		return Err(BenchUsageError::err("a path length below 10 can't hold a folder and the file"));
	}
	let file = "file.txt";
	let mut parts: Vec<String> = Vec::new();
	let mut left = length - file.len();
	let mut i = 0;
	while left > 0 {
		// Each folder adds its name and one "/". A last remainder of 1 can't hold a name and "/", so it borrows.
		let mut size = left.min(21);
		if left - size == 1 {
			size -= 1;
		}
		let mut name = format!("d{i}");
		while name.len() < size - 1 {
			name.push('x');
		}
		name.truncate(size - 1);
		parts.push(name);
		left -= size;
		i += 1;
	}
	parts.push(file.to_owned());
	Ok(parts.join("/"))
}

pub fn location_path(template: &str, repo: &str, home: &str, slug: &str) -> String {
	let repo_path = Path::new(repo);
	let parent = repo_path.parent().map_or_else(|| ".".to_owned(), |parent| parent.to_string_lossy().into_owned());
	let name = repo_path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
	template
		.replace("<repo>", repo)
		.replace("<parent>", &parent)
		.replace("<name>", &name)
		.replace("<home>", home)
		.replace("<slug>", slug)
}

/// One checkout's outcome: a row of h7-longpath.jsonl.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H7Row {
	pub location: String,
	pub length: u64,
	pub run: u64,
	pub ok: bool,
	pub absolute_length: usize,
	pub error: Option<String>,
}

/// The largest L whose every run passed, per location; `None` when none did.
pub fn largest_passing(rows: &[H7Row]) -> Vec<(String, Option<u64>)> {
	let mut locations: Vec<&str> = Vec::new();
	for row in rows {
		if !locations.contains(&row.location.as_str()) {
			locations.push(&row.location);
		}
	}
	locations
		.into_iter()
		.map(|location| {
			let of: Vec<&H7Row> = rows.iter().filter(|row| row.location == location).collect();
			let best = of
				.iter()
				.map(|row| row.length)
				.filter(|length| of.iter().filter(|row| row.length == *length).all(|row| row.ok))
				.max();
			(location.to_owned(), best)
		})
		.collect()
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(async { run(argv) }).await
}

fn git(cwd: &Path, env: &HashMap<String, String>, args: &[&str]) -> Result<RunResult> {
	let mut argv = vec!["git"];
	argv.extend(args);
	run_program_sync(
		&argv,
		RunOptions { cwd: Some(cwd.to_path_buf()), env: Some(env.clone()), ..RunOptions::default() },
	)
}

/// `--location name=<template>` repeats; they are taken off the arguments first. Returns them and the arguments left.
/// A list of `(name, template)` in the order given.
type Locations = Vec<(String, String)>;

fn take_locations(argv: &[String]) -> Result<(Locations, Vec<String>)> {
	let mut extra: Vec<(String, String)> = Vec::new();
	let mut rest: Vec<String> = Vec::new();
	let mut i = 0;
	while i < argv.len() {
		if argv[i] != "--location" {
			rest.push(argv[i].clone());
			i += 1;
			continue;
		}
		i += 1;
		let spec = argv.get(i).map_or("", String::as_str);
		i += 1;
		let Some(at) = spec.find('=').filter(|at| *at >= 1) else {
			return Err(BenchUsageError::err(format!("--location takes name=<template>, not \"{spec}\"")));
		};
		upsert(&mut extra, &spec[..at], &spec[at + 1..]);
	}
	Ok((extra, rest))
}

/// The `--dry-run` plan.
fn print_h7_plan(
	args: &BenchArgs,
	lengths: &[u64],
	locations: &[(String, String)],
	root: Option<&str>,
	home: Option<&str>,
) {
	let counts: Vec<String> = lengths.iter().map(u64::to_string).collect();
	let mut lines = vec![format!(
		"fixture repos under {} with a deepest relative path of L = {}",
		root.unwrap_or("a temp folder"),
		counts.join(", ")
	)];
	lines.extend(locations.iter().map(|(name, template)| format!("{name}: git worktree add {template}")));
	lines.push(format!("home for <home>: {}; Git with no global or system config", home.unwrap_or("a temp folder")));
	lines.push(format!(
		"{} run(s) each; ok = worktree add and git status exit 0 and the deep file exists there",
		args.runs
	));
	print_plan("H7 long paths", &lines, args.out.as_deref());
}

/// The process's environment with no global or system Git config and a fixed identity.
fn git_environment() -> Result<HashMap<String, String>> {
	let empty_config = mkdtemp("h7-git-")?.join("empty.gitconfig");
	fs::write(&empty_config, "")?;
	let mut git_env: HashMap<String, String> = env::vars()
		.into_iter()
		.map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
		.collect();
	for (name, value) in [
		("GIT_CONFIG_GLOBAL", empty_config.to_string_lossy().into_owned()),
		("GIT_CONFIG_NOSYSTEM", "1".to_owned()),
		("GIT_AUTHOR_NAME", "Bench".to_owned()),
		("GIT_AUTHOR_EMAIL", "bench@example.org".to_owned()),
		("GIT_COMMITTER_NAME", "Bench".to_owned()),
		("GIT_COMMITTER_EMAIL", "bench@example.org".to_owned()),
	] {
		git_env.insert(name.to_owned(), value);
	}
	Ok(git_env)
}

/// `path.join(base, relative)`: `relative` with `/`, normalized.
fn joined(base: &str, relative: &str) -> PathBuf {
	normalize(&PathBuf::from(format!("{base}/{relative}")))
}

/// What one run's checks share: the Git environment, the locations, the home folder.
struct Bench<'a> {
	git_env: HashMap<String, String>,
	locations: &'a [(String, String)],
	home: String,
}

impl Bench<'_> {
	/// The fixture repo with a deepest file of `deep`; the first Git failure of its set-up, if any.
	fn fixture(&self, repo: &Path, deep: &str) -> Result<Option<String>> {
		let repo_text = repo.to_string_lossy().into_owned();
		remove_tree(repo)?;
		for (_, template) in self.locations {
			let stale = location_path(template, &repo_text, &self.home, "x");
			if let Some(folder) = Path::new(&stale).parent() {
				remove_tree(folder)?;
			}
		}
		fs::create_dir_all(repo)?;
		if let Some(folder) = joined(&repo_text, deep).parent() {
			fs::create_dir_all(folder)?;
		}
		fs::write(joined(&repo_text, deep), "h7\n")?;
		let init = [
			git(repo, &self.git_env, &["init", "-q", "-b", "main"])?,
			git(repo, &self.git_env, &["add", "-A"])?,
			git(repo, &self.git_env, &["commit", "-q", "-m", "fixture"])?,
		];
		Ok(init.iter().find(|result| result.code != Some(0)).map(|result| format!("fixture: {}", result.stderr.trim())))
	}

	/// A worktree at `path` on a branch of its own, its status, and the deep file there: the first thing that failed.
	fn check_out(&self, repo: &Path, branch: &str, path: &str, deep: &str) -> Result<Option<String>> {
		if let Some(folder) = Path::new(path).parent() {
			fs::create_dir_all(folder)?;
		}
		// One branch per location: the folder is the slug, the branch names its location too.
		let add = git(repo, &self.git_env, &["worktree", "add", "-q", "-b", branch, path])?;
		if add.code != Some(0) {
			return Ok(Some(format!("worktree add: {}", add.stderr.trim())));
		}
		let status = git(Path::new(path), &self.git_env, &["status", "--porcelain"])?;
		if status.code != Some(0) {
			return Ok(Some(format!("status: {}", status.stderr.trim())));
		}
		Ok((!joined(path, deep).exists()).then(|| "the deep file is missing in the worktree".to_owned()))
	}

	/// One run at one length: a row per location.
	fn run_length(&self, root: &Path, (length, run): (u64, u64), out: &Path, rows: &mut Vec<H7Row>) -> Result<()> {
		let repo = normalize(&root.join("acme-app"));
		let repo_text = repo.to_string_lossy().into_owned();
		let deep = deep_path(usize::try_from(length).unwrap_or(usize::MAX))?;
		let fixture_error = self.fixture(&repo, &deep)?;
		for (location, template) in self.locations {
			let slug = format!("h7-{length}-{run}");
			let path = location_path(template, &repo_text, &self.home, &slug);
			let error = match &fixture_error {
				Some(error) => Some(error.clone()),
				None => self.check_out(&repo, &format!("{location}/{slug}"), &path, &deep)?,
			};
			let row = H7Row {
				location: location.clone(),
				length,
				run,
				ok: error.is_none(),
				absolute_length: joined(&path, &deep).to_string_lossy().chars().count(),
				error,
			};
			append_row(out, "h7-longpath", &row)?;
			rows.push(row);
		}
		Ok(())
	}
}

fn run(argv: Vec<String>) -> Result<()> {
	let (extra, rest) = take_locations(&argv)?;
	let spec = ArgSpec { default_runs: 1, valued: &["--root", "--lengths", "--home"], ..ArgSpec::default() };
	let args = parse_bench_args(&rest, &spec)?;
	let lengths = number_list_flag(&args, "lengths", &DEFAULT_LENGTHS)?;
	let mut locations: Vec<(String, String)> = Vec::new();
	for (name, template) in DEFAULT_LOCATIONS {
		upsert(&mut locations, name, template);
	}
	for (name, template) in &extra {
		upsert(&mut locations, name, template);
	}
	let (root_flag, home_flag) = (flag(&args, "root"), flag(&args, "home"));
	if args.dry_run {
		print_h7_plan(&args, &lengths, &locations, root_flag, home_flag);
		return Ok(());
	}
	let out = args.out.clone().expect("a real run has --out");
	claim_rows(&out, &["h7-longpath"])?;
	let started = conditions(&["git"]);
	let root: PathBuf = root_flag.map_or_else(|| mkdtemp("h7-"), |root| Ok(resolve(Path::new(root))))?;
	let home: PathBuf = home_flag.map_or_else(|| mkdtemp("h7-home-"), |home| Ok(resolve(Path::new(home))))?;
	let bench = Bench { git_env: git_environment()?, locations: &locations, home: home.to_string_lossy().into_owned() };
	let mut rows: Vec<H7Row> = Vec::new();
	for &length in &lengths {
		for run in 0..args.runs {
			bench.run_length(&root, (length, run), &out, &mut rows)?;
		}
	}
	let mut template_map = Map::new();
	for (name, template) in &locations {
		template_map.insert(name.clone(), json!(template));
	}
	let mut best = Map::new();
	for (location, length) in largest_passing(&rows) {
		best.insert(location, json!(length));
	}
	let file = write_json(
		&out,
		"h7-longpath",
		&json!({
			"benchmark": "H7",
			"conditions": started.finished(),
			"root": root.to_string_lossy(),
			"locations": Value::Object(template_map),
			"largestPassing": Value::Object(best),
		}),
	)?;
	println!("wrote {}", file.display());
	Ok(())
}

/// `{ ...record, [name]: value }`: a known name keeps its place.
fn upsert(list: &mut Vec<(String, String)>, name: &str, value: &str) {
	match list.iter_mut().find(|(known, _)| known == name) {
		Some(entry) => entry.1 = value.to_owned(),
		None => list.push((name.to_owned(), value.to_owned())),
	}
}

#[cfg(test)]
#[path = "h7_longpath_tests.rs"]
mod tests;
