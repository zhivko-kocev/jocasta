//! Clean-room runs (0708): a command run on one commit in a `jdxcode/mise` container that sees only the committed tree.
//! The commit goes in as a read-only tar (`git archive`) unpacked inside; the project's two mise files go in read-only
//! as system config; tools and package caches live in the volume `jocasta-cache-<project id>`. No home folder, host
//! environment variable or Docker socket goes in. The project's check (`verify`, else `test`) is found inside the
//! container, so the host's mise never loads a stranger's config; its result is kept per tree like the other checks.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::frontier::server::checks::checks_dir;
use crate::frontier::server::mise::repo_mise_name;
use crate::setup::setup::mise_config_dir;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::git::{GitOptions, git_async};
use crate::shared::pathkey::same_path;
use crate::shared::paths::data_dir;
use crate::shared::process::{RunOptions, run_program};
use crate::shared::projects::Project;
use crate::shared::projects_file::read_projects_file;
use crate::shared::settings::read_clean_room;

#[cfg(test)]
#[path = "cleanroom_tests.rs"]
mod tests;

/// mise's official image.
pub const IMAGE: &str = "jdxcode/mise:latest";
/// A check's time limit, as the check runner's.
pub const CHECK_MS: i64 = 30 * 60_000;
/// How many of a run's last output lines its result keeps.
pub const TAIL_LINES: usize = 20;
/// `docker info` answers within this or Docker counts as not running.
const INFO_MS: i64 = 5_000;
/// Pulling the image.
const PULL_MS: i64 = 15 * 60_000;
/// `git archive` and `git rev-parse` of the commit.
const GIT_MS: i64 = 120_000;
/// `docker rm -f` of a container past its limit.
const REMOVE_MS: i64 = 30_000;
/// What `docker run` exits with when Docker itself failed to start the container.
const DOCKER_FAILED: i32 = 125;
/// The line the check script prints before its task, and when it finds none.
const TASK_MARK: &str = "jocasta-check: ";
const NO_TASK: &str = "none";
/// Why a project's check can't run anywhere: it has neither task.
pub const NO_TASK_SAID: &str = "the project has neither a verify nor a test task";
/// The project's check, found inside the container: `verify`, else `test`.
const CHECK_SCRIPT: &str = "for t in verify test; do if mise tasks info \"$t\" >/dev/null 2>&1; then echo \"jocasta-check: $t\"; exec mise run \"$t\"; fi; done; echo \"jocasta-check: none\"";

/// What one run left: its exit code (`None` past its time limit), its output (stdout and stderr; a stored result keeps
/// only its tail) and how long it took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ran {
	pub exit: Option<i32>,
	pub output: String,
	pub ms: i64,
}

/// The project's check in the clean room: the task it ran and what it left, or why it couldn't run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Check {
	Ran { task: String, ran: Ran },
	NotPossible(String),
}

/// The last [`TAIL_LINES`] lines of `text`, trimmed.
pub fn tail(text: &str) -> String {
	let lines: Vec<&str> = text.trim().lines().collect();
	lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

/// Whether Docker answers: `docker info` within 5 s.
pub async fn available() -> bool {
	let options = RunOptions { timeout_ms: Some(INFO_MS), ..RunOptions::default() };
	run_program(&["docker", "info"], options).await.is_ok_and(|run| run.code == Some(0))
}

/// Why clean-room runs are off for `project`: `checks.cleanRoom: off`, or `cleanRoom: {<repo>: off}` in projects.yaml.
pub fn off_for(project: &Project) -> Option<String> {
	if !read_clean_room() {
		return Some("clean-room runs are off (checks.cleanRoom)".to_owned());
	}
	let off = read_projects_file(None).ok()?.clean_room_off;
	off.iter()
		.any(|repo| same_path(Path::new(repo), &project.main_path))
		.then(|| format!("clean-room runs are off for {} (projects.yaml cleanRoom)", project.name))
}

/// The project's mise files that exist, with where they go in the container.
async fn mise_files(project: &Project) -> Vec<(PathBuf, &'static str)> {
	let base: std::collections::HashMap<String, String> = crate::shared::env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect();
	let dir = mise_config_dir();
	let mut files = Vec::new();
	if let Some(name) = repo_mise_name(&project.main_path, &base).await {
		files.push((dir.join(format!("config.{name}.toml")), "/etc/mise/conf.d/10-project.toml"));
	}
	let own = dir.join(format!("config.jocasta-{}.override.toml", project.id));
	if own.is_file() {
		files.push((own, "/etc/mise/conf.d/20-jocasta.toml"));
	}
	files
}

/// A Docker volume name from a project id: anything but letters, digits, `_`, `.` and `-` becomes `-`.
fn volume_of(project_id: &str) -> String {
	let id: String = project_id
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') { c } else { '-' })
		.collect();
	format!("jocasta-cache-{id}")
}

/// `docker run`'s argv for `command` on the tar at `tar`.
fn docker_args(name: &str, tar: &Path, project_id: &str, files: &[(PathBuf, &str)], command: &str) -> Vec<String> {
	let mut argv: Vec<String> = ["docker", "run", "--rm", "--init", "--name", name].map(str::to_owned).to_vec();
	argv.push("--mount".to_owned());
	argv.push(format!("type=bind,source={},target=/src.tar,readonly", tar.display()));
	argv.push("-v".to_owned());
	argv.push(format!("{}:/cache", volume_of(project_id)));
	for (file, target) in files {
		argv.push("--mount".to_owned());
		argv.push(format!("type=bind,source={},target={target},readonly", file.display()));
	}
	for env in [
		"MISE_DATA_DIR=/cache/mise",
		"MISE_CACHE_DIR=/cache/mise-cache",
		"CARGO_HOME=/cache/cargo",
		"npm_config_cache=/cache/npm",
		"PIP_CACHE_DIR=/cache/pip",
		"GOMODCACHE=/cache/go/mod",
		"GOCACHE=/cache/go/build",
		"MISE_TRUSTED_CONFIG_PATHS=/work",
		"MISE_YES=1",
	] {
		argv.push("-e".to_owned());
		argv.push(env.to_owned());
	}
	argv.extend(["-w", "/work", IMAGE, "sh", "-lc"].map(str::to_owned));
	argv.push(format!(
		"exec 2>&1; mkdir -p /work && tar -xf /src.tar -C /work && cd /work && mise install && mise reshim && export PATH=\"/cache/mise/shims:$PATH\" && {command}"
	));
	argv
}

/// Runs `command` on `commit` of the project's repository in the clean room, within `limit_ms`. An error is a run that
/// couldn't start (git couldn't export the commit, Docker isn't there); [`unready`] says first why Docker can't run.
pub async fn run(project: &Project, commit: &str, command: &str, limit_ms: i64) -> Result<Ran> {
	run_with(project, &mise_files(project).await, commit, command, limit_ms).await
}

/// [`run`] with the mise files already found: the export in a temp folder, removed afterwards.
async fn run_with(
	project: &Project,
	files: &[(PathBuf, &str)],
	commit: &str,
	command: &str,
	limit_ms: i64,
) -> Result<Ran> {
	let dir = std::env::temp_dir().join(format!("jocasta-cleanroom-{}", uuid::Uuid::new_v4().simple()));
	std::fs::create_dir_all(&dir)?;
	let tar = dir.join("src.tar");
	let ran = async {
		let output = tar.to_string_lossy().into_owned();
		let args = ["archive", "--format=tar", "--output", output.as_str(), commit];
		let archived = git_async(&project.main_path, &args, GIT_MS, GitOptions::default()).await?;
		if archived.code != Some(0) {
			return Err(Error::other(format!("git archive {commit} failed: {}", archived.stderr.trim())));
		}
		let name = format!("jocasta-cr-{}", uuid::Uuid::new_v4().simple());
		let argv = docker_args(&name, &tar, &project.id, files, command);
		let started = Instant::now();
		let options = RunOptions { timeout_ms: Some(limit_ms), end_with_jocasta: true, ..RunOptions::default() };
		let run = run_program(&argv, options).await?;
		let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
		if run.timed_out {
			let remove = RunOptions { timeout_ms: Some(REMOVE_MS), ..RunOptions::default() };
			let _ = run_program(&["docker", "rm", "-f", name.as_str()], remove).await;
		}
		let output = if run.stderr.trim().is_empty() { run.stdout } else { format!("{}\n{}", run.stdout, run.stderr) };
		Ok(Ran { exit: if run.timed_out { None } else { run.code }, output, ms })
	}
	.await;
	let _ = std::fs::remove_dir_all(&dir);
	ran
}

/// Why a clean-room run can't start now: Docker isn't running, or the image can't be had; `None` when it can.
pub async fn unready() -> Option<String> {
	if !available().await {
		return Some("Docker isn't running".to_owned());
	}
	image_missing().await
}

/// Why the image isn't there to run: `None` when it is (pulled now when it wasn't).
async fn image_missing() -> Option<String> {
	let quick = RunOptions { timeout_ms: Some(INFO_MS * 2), ..RunOptions::default() };
	if run_program(&["docker", "image", "inspect", IMAGE], quick).await.is_ok_and(|run| run.code == Some(0)) {
		return None;
	}
	let pull = RunOptions { timeout_ms: Some(PULL_MS), ..RunOptions::default() };
	match run_program(&["docker", "pull", IMAGE], pull).await {
		Ok(run) if run.code == Some(0) => None,
		Ok(run) => Some(format!("the image {IMAGE} can't be pulled: {}", last_line(&run.stderr, &run.stdout))),
		Err(error) => Some(format!("the image {IMAGE} can't be pulled: {error}")),
	}
}

fn last_line<'a>(first: &'a str, second: &'a str) -> &'a str {
	let text = if first.trim().is_empty() { second } else { first };
	text.trim().lines().next_back().unwrap_or("no output")
}

/// What the check script's output says: the task it ran, or `None` when the project has neither.
fn task_of(output: &str) -> Option<String> {
	output.lines().find_map(|line| line.trim().strip_prefix(TASK_MARK)).map(str::to_owned)
}

/// The stored result's file: `<data>/checks/<tree 12>/cleanroom-<hash>.json`, the hash over the image and the mise
/// files' paths and contents.
async fn result_file(project: &Project, commit: &str, files: &[(PathBuf, &str)]) -> Option<PathBuf> {
	let spec = format!("{commit}^{{tree}}");
	let out = git_async(&project.main_path, &["rev-parse", spec.as_str()], GIT_MS, GitOptions::default()).await.ok()?;
	let tree = out.stdout.trim();
	if out.code != Some(0) || tree.len() < 12 {
		return None;
	}
	let mut hash = Sha256::new();
	hash.update(IMAGE.as_bytes());
	for (file, target) in files {
		hash.update(target.as_bytes());
		hash.update(std::fs::read(file).unwrap_or_default());
	}
	let digest: String = hash.finalize().iter().take(4).map(|byte| format!("{byte:02x}")).collect();
	Some(checks_dir(&data_dir()).join(&tree[..12]).join(format!("cleanroom-{digest}.json")))
}

/// The project's check on `commit` in the clean room (0708): a stored result for the same tree and mise files, else a
/// run; or why it isn't possible (off, Docker not running, the image not pullable, no `verify` or `test` task).
pub async fn check(project: &Project, commit: &str) -> Check {
	if let Some(why) = off_for(project) {
		return Check::NotPossible(why);
	}
	let files = mise_files(project).await;
	let stored = result_file(project, commit, &files).await;
	if let Some(found) = stored.as_ref().and_then(|file| std::fs::read_to_string(file).ok())
		&& let Ok(check) = serde_json::from_str::<Check>(&found)
	{
		return check;
	}
	if let Some(why) = unready().await {
		return Check::NotPossible(why);
	}
	let ran = match run_with(project, &files, commit, CHECK_SCRIPT, CHECK_MS).await {
		Ok(ran) => ran,
		Err(error) => return Check::NotPossible(error.to_string()),
	};
	if ran.exit == Some(DOCKER_FAILED) {
		return Check::NotPossible(format!("Docker couldn't start the container: {}", last_line(&ran.output, "")));
	}
	let check = match task_of(&ran.output) {
		Some(task) if task == NO_TASK => {
			return Check::NotPossible(NO_TASK_SAID.to_owned());
		}
		task => Check::Ran {
			task: task.unwrap_or_else(|| "mise install".to_owned()),
			ran: Ran { output: tail(&ran.output), ..ran },
		},
	};
	// A run ended at its time limit says nothing about the tree: it runs again next time.
	let timed_out = matches!(&check, Check::Ran { ran, .. } if ran.exit.is_none());
	if let (Some(file), Ok(text), false) = (stored, serde_json::to_string_pretty(&check), timed_out) {
		let _ = write_atomically(&file, text, None);
	}
	check
}
