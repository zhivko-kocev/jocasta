//! mise in project and thread panes (0473, 0477). Every pane of a project gets `MISE_ENV=jocasta-<project id>`, so mise
//! loads Jocasta's own config for that project, `~/.config/mise/config.jocasta-<project id>.override.toml` (global config,
//! trusted by path), in every worktree without a copy. With paranoid config trust on (`paranoid = ["config"]`, which
//! Jocasta's mise file sets, 0476) and every repo config in the folder trusted, `mise env --json` run there is merged
//! into the pane's env too (shims carry no `[env]`), and the review may run its `verify` (else `test`) task. The only
//! config Jocasta trusts itself is a thread worktree's unchanged copy of one the main checkout has trusted (0596): mise's
//! own worktree rule, which paranoid trust turns off. Without mise on PATH nothing changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::frontier::protocol::js_number_text;
use crate::shared::env;
use crate::shared::errors::Error;
use crate::shared::fsx::as_object;
use crate::shared::log::{LogLevel, log};
use crate::shared::pathkey::{is_inside, path_key, real_path};
use crate::shared::process::{RunOptions, on_path, run_program};

/// How long one mise call may take before it counts as failed.
const MISE_TIMEOUT_MS: i64 = 15_000;

/// An environment: name to value (a name left out is the TypeScript's `undefined`).
pub type Env = HashMap<String, String>;

/// Which mise a call runs: the TypeScript's `mise` parameter, whose default is the one on PATH, `null` none, or an
/// argv prefix.
#[derive(Debug, Clone, Copy)]
pub enum Mise<'a> {
	/// The mise on PATH (the default); none when there is none.
	OnPath,
	/// No mise: nothing changes.
	Absent,
	/// This argv prefix.
	Argv(&'a [String]),
}

impl Mise<'_> {
	/// mise's argv prefix: the one on PATH, or none.
	fn argv(self) -> Option<Vec<String>> {
		match self {
			Self::OnPath => on_path("mise", None).map(|found| vec![found.to_string_lossy().into_owned()]),
			Self::Absent => None,
			Self::Argv(argv) => Some(argv.to_vec()),
		}
	}
}

/// The process's own environment, the TypeScript's `process.env`.
fn process_env() -> Env {
	env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect()
}

/// What JavaScript's `String(error)` gives for an error: its name, then its message (`Error: …`, `UserError: …`).
fn js_string(error: &Error) -> String {
	let name = if error.is_user() { "UserError" } else { "Error" };
	format!("{name}: {error}")
}

/// One mise call in `cwd`; `None` when it fails or times out.
async fn run_mise(mise: &[String], args: &[&str], cwd: &Path, env: &Env) -> Option<String> {
	let argv: Vec<&str> = mise.iter().map(String::as_str).chain(args.iter().copied()).collect();
	let options = RunOptions {
		cwd: Some(cwd.to_path_buf()),
		env: Some(env.clone()),
		timeout_ms: Some(MISE_TIMEOUT_MS),
		..RunOptions::default()
	};
	let call = args.join(" ");
	match run_program(&argv, options).await {
		Ok(run) if run.code == Some(0) => return Some(run.stdout),
		Ok(run) => {
			let why = run.code.map_or_else(|| "timed out".to_owned(), |code| code.to_string());
			let first = run.stderr.trim().split('\n').next().unwrap_or_default();
			let said = format!("mise {call} in {} failed ({why}): {first}", cwd.display());
			log(LogLevel::Warn, "mise", &said, None);
		}
		Err(error) => {
			log(
				LogLevel::Warn,
				"mise",
				&format!("mise {call} in {} failed: {}", cwd.display(), js_string(&error)),
				None,
			);
		}
	}
	None
}

/// `MISE_ENV` with the project's `jocasta-<pid>` appended to whatever the pane already has (0477).
fn with_jocasta_env(project_id: &str, base: &Env) -> String {
	let own = format!("jocasta-{project_id}");
	let mut names: Vec<String> = base
		.get("MISE_ENV")
		.map_or("", String::as_str)
		.split(',')
		.map(|name| name.trim().to_owned())
		.filter(|name| !name.is_empty() && *name != own)
		.collect();
	names.push(own);
	names.join(",")
}

/// `base` with the project's `MISE_ENV`.
fn jocasta_env(project_id: &str, base: &Env) -> Env {
	let mut env = base.clone();
	env.insert("MISE_ENV".to_owned(), with_jocasta_env(project_id, base));
	env
}

/// One config `mise trust --show` listed.
struct TrustEntry {
	path: String,
	trusted: bool,
}

/// The configs `mise trust --show` listed, each trusted or not.
fn trust_list(show: Option<&str>) -> Vec<TrustEntry> {
	static LINE: LazyLock<Regex> = LazyLock::new(|| {
		// JavaScript's `.` stops at `\r` too.
		Regex::new(r"(?m)^([^\n\r\u{2028}\u{2029}]+): (trusted|untrusted)\s*$").expect("the trust line pattern")
	});
	LINE.captures_iter(show.unwrap_or_default())
		.map(|m| TrustEntry { path: m[1].to_owned(), trusted: &m[2] == "trusted" })
		.collect()
}

/// Whether `mise settings get paranoid` printed paranoid config trust on: `true` as a word, or `"config"`.
fn is_paranoid(paranoid: Option<&str>) -> bool {
	static ON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\btrue\b|"config""#).expect("the paranoid pattern"));
	paranoid.is_some_and(|text| ON.is_match(text))
}

/// Why mise may not load the folder's config for Jocasta (0473), or `None` when it may: paranoid config trust must
/// be on, so omp's `CI=true` can't trust a repo by itself, and `mise trust --show` must report no untrusted config here
/// or above (global config, Jocasta's own included, isn't listed: it is trusted by path). The reason is "" when there is
/// no repo config to say anything about.
async fn distrust(mise: &[String], cwd: &Path, env: &Env) -> Option<String> {
	let (paranoid, show) = tokio::join!(
		run_mise(mise, &["settings", "get", "paranoid"], cwd, env),
		run_mise(mise, &["trust", "--show"], cwd, env),
	);
	let configs = trust_list(show.as_deref());
	if !is_paranoid(paranoid.as_deref()) {
		return Some(if configs.is_empty() { String::new() } else { "mise's paranoid config trust is off".to_owned() });
	}
	if show.is_none() {
		return Some("mise didn't say whether its config is trusted".to_owned());
	}
	configs.iter().find(|config| !config.trusted).map(|config| format!("{} isn't trusted in mise", config.path))
}

/// Whether files `a` and `b` hold the same bytes; false when either can't be read as a file.
fn same_bytes(a: &Path, b: &Path) -> bool {
	matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// A path as `mise trust --show` prints it, its `~` for the home folder (Linux and macOS) expanded with the `HOME` mise
/// ran with, which is the one it shortens by.
fn shown_path(path: &str, home: &str) -> PathBuf {
	match path.strip_prefix('~') {
		Some(rest) if rest.is_empty() || rest.starts_with(['\\', '/']) => {
			Path::new(home).join(rest.trim_start_matches(['\\', '/']))
		}
		_ => PathBuf::from(path),
	}
}

/// `path` relative to `root` (`path` is inside it).
fn relative_to(root: &Path, path: &Path) -> PathBuf {
	match path.strip_prefix(root) {
		Ok(rest) => rest.to_path_buf(),
		Err(_) => PathBuf::from(path_key(path)[path_key(root).len()..].trim_start_matches('/')),
	}
}

/// Trusts each config mise lists as untrusted inside the thread worktree `cwd` whose counterpart in the main checkout
/// `main` mise lists as trusted and which has the same bytes (0596). Only under paranoid config trust: without it, mise's
/// own worktree rule applies. A changed copy stays untrusted, so trust still follows content. Returns what it trusted.
/// `base` is the env the pane would get (the process's by default).
pub async fn trust_copies(
	project_id: &str,
	main: &Path,
	cwd: &Path,
	base: Option<&Env>,
	mise: Mise<'_>,
) -> Vec<PathBuf> {
	let Some(mise) = mise.argv() else { return Vec::new() };
	let base = base.cloned().unwrap_or_else(process_env);
	let env = jocasta_env(project_id, &base);
	let (paranoid, here, there) = tokio::join!(
		run_mise(&mise, &["settings", "get", "paranoid"], cwd, &env),
		run_mise(&mise, &["trust", "--show"], cwd, &env),
		run_mise(&mise, &["trust", "--show"], main, &env),
	);
	if !is_paranoid(paranoid.as_deref()) {
		return Vec::new();
	}
	let home = base
		.get("HOME")
		.filter(|home| !home.is_empty())
		.cloned()
		.or_else(|| std::env::home_dir().map(|home| home.to_string_lossy().into_owned()))
		.unwrap_or_default();
	let trusted_in_main: HashSet<String> = trust_list(there.as_deref())
		.into_iter()
		.filter(|config| config.trusted)
		.map(|config| path_key(&real_path(&shown_path(&config.path, &home))))
		.collect();
	let root = real_path(cwd);
	let main_root = real_path(main);
	let mut trusted = Vec::new();
	for config in trust_list(here.as_deref()) {
		if config.trusted {
			continue;
		}
		let copy = real_path(&shown_path(&config.path, &home));
		if path_key(&copy) == path_key(&root) || !is_inside(&root, &copy) {
			continue;
		}
		let original = main_root.join(relative_to(&root, &copy));
		if !trusted_in_main.contains(&path_key(&original)) || !same_bytes(&copy, &original) {
			continue;
		}
		if run_mise(&mise, &["trust", &copy.to_string_lossy()], cwd, &env).await.is_none() {
			continue;
		}
		let said = format!(
			"trusted {}: an unchanged copy of {}, which mise trusts (0596)",
			copy.display(),
			original.display()
		);
		log(LogLevel::Info, "mise", &said, None);
		trusted.push(copy);
	}
	trusted
}

/// The env to merge into a project or thread pane started in `cwd` (0473, 0477): `MISE_ENV` with `jocasta-<pid>`
/// appended, plus `mise env --json` when the folder's config may be loaded. `base` is the env the pane would otherwise
/// get (its thread variables reach mise's `[env]` templates; the process's by default). Empty when mise isn't on PATH.
pub async fn mise_env(project_id: &str, cwd: &Path, base: Option<&Env>, mise: Mise<'_>) -> Env {
	let Some(mise) = mise.argv() else { return Env::new() };
	let base = base.cloned().unwrap_or_else(process_env);
	let env = jocasta_env(project_id, &base);
	let mise_env = env["MISE_ENV"].clone();
	if distrust(&mise, cwd, &env).await.is_some() {
		return Env::from([("MISE_ENV".to_owned(), mise_env)]);
	}
	let out = run_mise(&mise, &["env", "--json"], cwd, &env).await;
	let parsed: Option<Value> = out.and_then(|text| match serde_json::from_str(&text) {
		Ok(parsed) => Some(parsed),
		Err(_) => {
			log(LogLevel::Warn, "mise", &format!("mise env --json in {} printed no JSON", cwd.display()), None);
			None
		}
	});
	let mut merged = Env::new();
	if let Some(Value::Object(object)) = parsed {
		for (key, value) in object {
			if let Value::String(value) = value {
				merged.insert(key, value);
			}
		}
	}
	merged.insert("MISE_ENV".to_owned(), mise_env);
	merged
}

/// The task a project's own check runs (0453, 0473, 0477).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiseCheckTask {
	Verify,
	Test,
}

/// The project's own check (0453, 0473, 0477): its mise task, or why it didn't run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiseCheck {
	Task(MiseCheckTask),
	Skipped(String),
}

/// How a project check that couldn't run is said beside the checks that did (0559): never a silent pass.
pub fn skipped_said(why: &str) -> String {
	format!("the project's check didn't run: {why}")
}

/// What [`task_among`] found.
enum Found {
	Task(String),
	Skipped(String),
}

/// The first of `names` the folder's mise config defines as a task, when mise may load that config (0473): the task, or
/// why it can't be run when the folder has a repo config mise may not load for Jocasta (0559); `None` when mise is
/// absent or none is defined. Jocasta's own per-project config counts through `MISE_ENV`.
async fn task_among(
	project_id: &str,
	cwd: &Path,
	names: &[&str],
	base: &Env,
	mise: Option<&[String]>,
) -> Option<Found> {
	let mise = mise?;
	let env = jocasta_env(project_id, base);
	if let Some(why) = distrust(mise, cwd, &env).await {
		return if why.is_empty() { None } else { Some(Found::Skipped(why)) };
	}
	let out = run_mise(mise, &["tasks", "ls", "--json"], cwd, &env).await;
	let mut defined: HashSet<String> = HashSet::new();
	let tasks: Value = match out {
		None => Value::Array(Vec::new()),
		Some(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
			log(LogLevel::Warn, "mise", &format!("mise tasks ls --json in {} printed no JSON", cwd.display()), None);
			Value::Array(Vec::new())
		}),
	};
	if let Value::Array(tasks) = tasks {
		for task in &tasks {
			if let Some(name) = as_object(task).and_then(|task| task.get("name")).and_then(Value::as_str) {
				defined.insert(name.to_owned());
			}
		}
	}
	names.iter().find(|name| defined.contains(**name)).map(|name| Found::Task((*name).to_owned()))
}

/// The project's own check as a mise task (0453, 0473, 0477): `verify`, else `test`, when the folder's config may be
/// loaded and defines it (Jocasta's own per-project config counts through `MISE_ENV`); why it can't run when the folder
/// has a repo config mise may not load for Jocasta (0559); `None` otherwise.
pub async fn mise_check(project_id: &str, cwd: &Path, base: Option<&Env>, mise: Mise<'_>) -> Option<MiseCheck> {
	let base = base.cloned().unwrap_or_else(process_env);
	match task_among(project_id, cwd, &["verify", "test"], &base, mise.argv().as_deref()).await? {
		Found::Task(name) => {
			Some(MiseCheck::Task(if name == "verify" { MiseCheckTask::Verify } else { MiseCheckTask::Test }))
		}
		Found::Skipped(why) => Some(MiseCheck::Skipped(why)),
	}
}

/// What [`mise_run_task`] made of a task: its stdout, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiseTaskRun {
	Output(String),
	Failed(String),
	Skipped(String),
}

/// What [`mise_run_task`] may be given.
pub struct MiseRunOptions<'a> {
	pub timeout_ms: i64,
	/// The checks' env; the process's by default.
	pub env: Option<&'a Env>,
	pub mise: Mise<'a>,
}

/// Runs the folder's mise task `name` to its end (0616) under the same trust rules as [`mise_check`]: a trusted config's
/// task only, with `env` (the checks' env) and the project's `MISE_ENV`, ended after `timeout_ms`. `Failed` says how it
/// failed (exit code or time limit); `None` when mise is absent or the folder has no such task.
pub async fn mise_run_task(
	project_id: &str,
	cwd: &Path,
	name: &str,
	options: MiseRunOptions<'_>,
) -> Option<MiseTaskRun> {
	let MiseRunOptions { timeout_ms, env: base, mise } = options;
	let base = base.cloned().unwrap_or_else(process_env);
	let mise = mise.argv()?;
	let name_only = [name];
	let found = task_among(project_id, cwd, &name_only, &base, Some(&mise)).await?;
	if let Found::Skipped(why) = found {
		return Some(MiseTaskRun::Skipped(why));
	}
	let env = jocasta_env(project_id, &base);
	let argv: Vec<&str> = mise.iter().map(String::as_str).chain(["run", name]).collect();
	let run_options = RunOptions {
		cwd: Some(cwd.to_path_buf()),
		env: Some(env),
		timeout_ms: Some(timeout_ms),
		..RunOptions::default()
	};
	Some(match run_program(&argv, run_options).await {
		Ok(run) if run.code == Some(0) => MiseTaskRun::Output(run.stdout),
		Ok(run) if run.timed_out || run.signal.is_some() => {
			MiseTaskRun::Failed(format!("ran past its {} s limit", js_number_text(timeout_ms as f64 / 1000.0)))
		}
		Ok(run) => {
			let last = run.stderr.trim().split('\n').next_back().unwrap_or_default();
			let code = run.code.map_or_else(|| "null".to_owned(), |code| code.to_string());
			MiseTaskRun::Failed(format!("exited {code}: {last}"))
		}
		Err(error) => MiseTaskRun::Failed(format!("did not start: {}", js_string(&error))),
	})
}

#[cfg(test)]
#[path = "mise_tests.rs"]
pub(crate) mod tests;
