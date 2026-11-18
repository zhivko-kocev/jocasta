//! What the runner does to a trial's repo around the harness (`runner.ts`): the repo made from the fixture, the setup,
//! the end-state checks, the hidden grading on a clean clone and the line counts. Each shell command runs until a
//! deadline and is then ended with its whole process tree (EVL-5).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::evals::harness::copy_tree;
use crate::evals::runner::{Check, EvalCase, to_fixed};
use crate::lib::git::git;
use crate::lib::output::iso_now;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::process::{RunOptions, run_program};

/// The grading limit (0574): the end-state checks, and the hidden grading's setup and checks together, each get this
/// long; a command still running then is ended with its process tree and its check grades 0.
pub const GRADING_LIMIT_MS: u64 = 60 * 60_000;

/// The setup before the harness has its own limit (0574, amended): a setup still running then is ended with its process
/// tree and the trial is an infra error (not the agent's doing), which the task bench retries.
pub const SETUP_LIMIT_MS: u64 = 60 * 60_000;

/// The grade a hidden grading cut by [`GRADING_LIMIT_MS`] gets (0574).
pub const HIDDEN_TIMEOUT: &str = "hidden: timeout";
/// The grade of a grading setup that the trial's change broke: it passes on the fixture commit (0574).
const HIDDEN_SETUP_FAILED: &str = "hidden: setup failed";

pub struct CommandResult {
	pub status: Option<i32>,
	pub stdout: String,
	pub stderr: String,
	/// Ended by its deadline, with its process tree.
	pub timed_out: bool,
}

/// With `log_file`, appends a command's folder, exit code, seconds, stdout and stderr there (0482).
fn log_command(
	log_file: Option<&Path>,
	command: &str,
	cwd: &Path,
	started: Instant,
	result: &CommandResult,
) -> Result<()> {
	let Some(log_file) = log_file else { return Ok(()) };
	if let Some(parent) = log_file.parent() {
		fs::create_dir_all(parent)?;
	}
	let seconds = to_fixed(started.elapsed().as_secs_f64(), 1);
	let status = result.status.map_or_else(|| "null".to_owned(), |status| status.to_string());
	let exit = if result.timed_out { format!("{status} (killed at its time limit)") } else { status };
	let entry = format!(
		"=== {}\n$ {command}\ncwd: {}\nexit: {exit}\nseconds: {seconds}\n--- stdout\n{}\n--- stderr\n{}\n\n",
		iso_now(),
		cwd.display(),
		result.stdout,
		result.stderr
	);
	fs::OpenOptions::new().create(true).append(true).open(log_file)?.write_all(entry.as_bytes())?;
	Ok(())
}

/// `command` as the platform's shell runs it (`spawn(command, { shell: true })`).
fn shell_argv(command: &str) -> Vec<String> {
	if cfg!(windows) {
		let shell = jocasta::shared::env::var("COMSPEC").unwrap_or_else(|| "cmd.exe".to_owned());
		vec![shell, "/d".into(), "/s".into(), "/c".into(), format!("\"{command}\"")]
	} else {
		vec!["/bin/sh".into(), "-c".into(), command.to_owned()]
	}
}

/// Runs a shell command until `deadline`, then ends it with its whole process tree (EVL-5: a shell's own kill leaves
/// the test runner under it holding the pipes, and a blocked event loop never fires a test framework's timeout); with
/// `log_file`, logs it there.
pub async fn run_command(
	command: &str,
	cwd: &Path,
	env: Option<&HashMap<String, String>>,
	log_file: Option<&Path>,
	deadline: Instant,
) -> Result<CommandResult> {
	let started = Instant::now();
	let options = RunOptions {
		cwd: Some(cwd.to_path_buf()),
		env: env.cloned(),
		timeout_ms: Some(deadline.saturating_duration_since(started).as_millis() as i64),
		windows_verbatim_arguments: true,
		..RunOptions::default()
	};
	let result = match run_program(&shell_argv(command), options).await {
		Ok(run) => CommandResult { status: run.code, stdout: run.stdout, stderr: run.stderr, timed_out: run.timed_out },
		Err(error) => {
			CommandResult { status: None, stdout: String::new(), stderr: error.to_string(), timed_out: false }
		}
	};
	log_command(log_file, command, cwd, started, &result)?;
	Ok(result)
}

/// A setup before the harness ended at [`SETUP_LIMIT_MS`]: an infra error (`classify_infra` reads `: setup \``).
pub enum SetupFailure {
	Killed(String),
	Failed(Error),
}

impl From<SetupFailure> for Error {
	fn from(failure: SetupFailure) -> Self {
		match failure {
			SetupFailure::Killed(message) => Error::other(message),
			SetupFailure::Failed(error) => error,
		}
	}
}

impl From<Error> for SetupFailure {
	fn from(error: Error) -> Self {
		Self::Failed(error)
	}
}

impl From<std::io::Error> for SetupFailure {
	fn from(error: std::io::Error) -> Self {
		Self::Failed(error.into())
	}
}

/// A setup failure as an infra error names it (`classify_infra` reads `: setup \``).
fn setup_failed(eval_case: &EvalCase, setup: &str, stderr: &str) -> String {
	format!("{}: setup `{setup}` failed: {stderr}", eval_case.id)
}

async fn run_setup(
	eval_case: &EvalCase,
	cwd: &Path,
	env: &HashMap<String, String>,
	log_file: Option<&Path>,
	deadline: Instant,
) -> std::result::Result<(), SetupFailure> {
	let Some(setup) = &eval_case.setup else { return Ok(()) };
	let run = run_command(setup, cwd, Some(env), log_file, deadline).await?;
	if run.timed_out {
		return Err(SetupFailure::Killed(format!("{}: setup `{setup}` killed at its time limit", eval_case.id)));
	}
	if run.status != Some(0) {
		return Err(SetupFailure::Failed(Error::other(setup_failed(eval_case, setup, &run.stderr))));
	}
	Ok(())
}

pub struct TrialRepo {
	pub repo: PathBuf,
	pub fixture_commit: String,
}

/// [`make_trial_repo`] with a killed setup told apart from a failed one.
// The TypeScript's positional parameters, which `evals/bench.rs` calls as they are.
#[allow(clippy::too_many_arguments)]
pub async fn make_trial_repo_checked(
	eval_case: &EvalCase,
	rep: u64,
	root: &Path,
	env: &HashMap<String, String>,
	log_file: Option<&Path>,
	limit_ms: Option<u64>,
) -> std::result::Result<TrialRepo, SetupFailure> {
	let repo = root.join("repo");
	fs::create_dir(&repo)?;
	if let Some(fixture) = &eval_case.fixture {
		copy_tree(fixture, &repo)?;
	}
	for args in [
		vec!["init", "-q", "-b", "main"],
		vec!["config", "user.name", "Jocasta Evals"],
		vec!["config", "user.email", "evals@example.org"],
		vec!["config", "core.autocrlf", "false"],
		vec!["add", "-A", "--force"],
		vec!["commit", "-q", "--allow-empty", "-m", "fixture"],
	] {
		git(&repo, &args, None)?;
	}
	if let Some(tag) = &eval_case.fixture_tag {
		git(&repo, &["tag", tag], None)?;
	}
	let deadline = Instant::now() + Duration::from_millis(limit_ms.unwrap_or(SETUP_LIMIT_MS));
	run_setup(eval_case, &repo, env, log_file, deadline).await?;
	// A folder that never exists, so no hook runs, wherever setup put them (husky moves `core.hooksPath` itself).
	git(&repo, &["config", "core.hooksPath", &root.join("no-hooks").display().to_string()], None)?;
	git(&repo, &["add", "-A"], None)?;
	git(&repo, &["commit", "-q", "--amend", "--allow-empty", "--no-edit"], None)?;
	// The amend made a new commit; grading's clean clone takes the tag along with the branch.
	if let Some(tag) = &eval_case.fixture_tag {
		git(&repo, &["tag", "-f", tag], None)?;
	}
	let fixture_commit = git(&repo, &["rev-parse", "HEAD"], None)?.trim().to_owned();
	// Not the case id, which for a benchmark task ends in its fix's PR number: git output shows the branch (0508).
	git(&repo, &["checkout", "-q", "-b", &format!("eval/trial-{rep}")], None)?;
	Ok(TrialRepo { repo, fixture_commit })
}

/// `<root>/repo`: the fixture copied in and committed as `fixture` on `main`, every file of it, those its own ignore
/// rules match too (upstream tracks them: cli/cli's `internal/gh`, pipx's test package lists), and tagged with
/// `fixture_tag`; the setup run in that checkout within `limit_ms` ([`SETUP_LIMIT_MS`] by default; past it, it is ended
/// with its tree and this fails as infra) (tools that read the version from git, such as setuptools-scm and hatch-vcs,
/// need a commit, and upstream's tag to report upstream's version), and what setup wrote folded into the same commit,
/// so it never counts as the trial's change; then checked out on the trial's own branch. Hooks that setup installs
/// (husky, yorkie, simple-git-hooks) stay off: an arm that commits meets no lint or commit-message check that an arm
/// that doesn't never meets. Git converts no line endings there (`core.autocrlf` off), so its files and its clean clone
/// keep the fixture's bytes on Windows too.
// The TypeScript's positional parameters, which `evals/bench.rs` calls as they are.
#[allow(clippy::too_many_arguments)]
pub async fn make_trial_repo(
	eval_case: &EvalCase,
	rep: u64,
	root: &Path,
	env: &HashMap<String, String>,
	log_file: Option<&Path>,
	limit_ms: Option<u64>,
) -> Result<TrialRepo> {
	Ok(make_trial_repo_checked(eval_case, rep, root, env, log_file, limit_ms).await?)
}

/// A clone of `repo` at `commit` (detached), its bytes as committed; the fixture tag comes along.
fn clone_at(repo: &Path, commit: &str, dir: &Path) -> Result<()> {
	git(
		repo,
		&[
			"clone",
			"-q",
			"--no-checkout",
			"-c",
			"core.autocrlf=false",
			&repo.display().to_string(),
			&dir.display().to_string(),
		],
		None,
	)?;
	git(dir, &["checkout", "-q", "--detach", commit], None)?;
	Ok(())
}

fn score(passed: bool) -> Value {
	Value::from(u8::from(passed))
}

/// The hidden checks (0482 E1) on a clean clone of the trial's result: its changes (staged by the caller) committed on
/// whatever HEAD the agent left (EVL-8: a branch it made or a detached HEAD included), that commit cloned to `grade_dir`
/// (no ignored or build files the agent left), the case's grading setup run there, `hidden/` copied in. With `log_file`,
/// the setup's and the checks' commands are logged there.
///
/// Grading has one limit (0574, `limit_ms`, [`GRADING_LIMIT_MS`] by default) for setup and checks together: what runs
/// past it is ended and the grades so far get [`HIDDEN_TIMEOUT`] 0. A grading setup that fails is run again on a clean
/// clone of the fixture commit, within the same limit (EVL-5; cut there, it grades [`HIDDEN_TIMEOUT`] 0 like a cut
/// setup): when it passes there the trial's change broke it, and the grade is `HIDDEN_SETUP_FAILED` 0; when it fails
/// there too, the setup itself is broken and this fails (infra, 0482).
// The TypeScript's positional parameters, which `evals/bench.rs` calls as they are.
#[allow(clippy::too_many_arguments)]
pub async fn grade_hidden(
	eval_case: &EvalCase,
	repo: &Path,
	fixture_commit: &str,
	grade_dir: &Path,
	env: &HashMap<String, String>,
	log_file: Option<&Path>,
	limit_ms: Option<u64>,
) -> Result<Map<String, Value>> {
	let deadline = Instant::now() + Duration::from_millis(limit_ms.unwrap_or(GRADING_LIMIT_MS));
	git(repo, &["commit", "-q", "--allow-empty", "--no-verify", "-m", "trial result"], None)?;
	let head = git(repo, &["rev-parse", "HEAD"], None)?.trim().to_owned();
	clone_at(repo, &head, grade_dir)?;
	let timed_out = || Map::from_iter([(HIDDEN_TIMEOUT.to_owned(), score(false))]);
	let setup = eval_case.grade_setup.as_ref().or(eval_case.setup.as_ref());
	if let Some(setup) = setup {
		let run = run_command(setup, grade_dir, Some(env), log_file, deadline).await?;
		if run.timed_out {
			return Ok(timed_out());
		}
		if run.status != Some(0) {
			let base = PathBuf::from(format!("{}-base", grade_dir.display()));
			clone_at(repo, fixture_commit, &base)?;
			let again = run_command(setup, &base, Some(env), log_file, deadline).await?;
			if again.timed_out {
				return Ok(timed_out());
			}
			if again.status != Some(0) {
				return Err(Error::other(setup_failed(eval_case, setup, &again.stderr)));
			}
			return Ok(Map::from_iter([(HIDDEN_SETUP_FAILED.to_owned(), score(false))]));
		}
	}
	if let Some(hidden) = &eval_case.hidden {
		copy_tree(hidden, grade_dir)?;
	}
	let mut grade = Map::new();
	for check in &eval_case.hidden_checks {
		let result = check_result(check, grade_dir, Some(env), log_file, deadline).await?;
		grade.insert(format!("hidden: {}", check_label(check)), score(result.passed));
		if result.timed_out {
			grade.insert(HIDDEN_TIMEOUT.to_owned(), score(false));
			break;
		}
	}
	Ok(grade)
}

/// Lines added and removed in `checkout` since the fixture commit, committed, staged, unstaged and untracked alike;
/// with `log_file`, the per-file `git diff --numstat` they sum goes there (0508).
pub fn line_counts(checkout: &Path, fixture_commit: &str, log_file: Option<&Path>) -> Result<(u64, u64)> {
	git(checkout, &["add", "-A"], None)?;
	let numstat = git(checkout, &["diff", "--cached", "--numstat", fixture_commit], None)?;
	if let Some(log_file) = log_file {
		if let Some(parent) = log_file.parent() {
			fs::create_dir_all(parent)?;
		}
		fs::write(log_file, &numstat)?;
	}
	let (mut added, mut removed) = (0, 0);
	for line in numstat.split('\n') {
		// A binary file's counts are "-", which count 0.
		let mut parts = line.split('\t');
		added += parts.next().and_then(|count| count.parse::<u64>().ok()).unwrap_or(0);
		removed += parts.next().and_then(|count| count.parse::<u64>().ok()).unwrap_or(0);
	}
	Ok((added, removed))
}

pub fn check_label(check: &Check) -> String {
	if let Some(command) = &check.command {
		return format!("command: {command}");
	}
	if let Some(clean) = check.git_clean {
		return format!("git_clean: {clean}");
	}
	let file = check.file.as_deref().unwrap_or_default();
	if check.absent == Some(true) {
		return format!("file: {file} absent");
	}
	if let Some(contains) = &check.contains {
		return format!("file: {file} contains {}", serde_json::to_string(contains).unwrap_or_default());
	}
	format!("file: {file}")
}

/// Grades one end-state check against the trial repo after the run (0164); a command runs with `env` (default: this
/// process's) within [`GRADING_LIMIT_MS`] and, with `log_file`, is logged there.
#[cfg(test)]
pub async fn grade_check(
	check: &Check,
	repo: &Path,
	env: Option<&HashMap<String, String>>,
	log_file: Option<&Path>,
) -> Result<bool> {
	let deadline = Instant::now() + Duration::from_millis(GRADING_LIMIT_MS);
	Ok(check_result(check, repo, env, log_file, deadline).await?.passed)
}

pub struct CheckResult {
	pub passed: bool,
	pub timed_out: bool,
}

/// [`grade_check`] until `deadline`: a command still running then is ended with its tree and fails.
pub async fn check_result(
	check: &Check,
	repo: &Path,
	env: Option<&HashMap<String, String>>,
	log_file: Option<&Path>,
	deadline: Instant,
) -> Result<CheckResult> {
	if let Some(command) = &check.command {
		let run = run_command(command, repo, env, log_file, deadline).await?;
		return Ok(CheckResult { passed: run.status == Some(0) && !run.timed_out, timed_out: run.timed_out });
	}
	if let Some(expected) = check.git_clean {
		let clean = git(repo, &["status", "--porcelain"], None)?.trim().is_empty();
		return Ok(CheckResult { passed: clean == expected, timed_out: false });
	}
	let path = repo.join(check.file.as_deref().unwrap_or_default());
	let passed = if check.absent == Some(true) {
		!path.exists()
	} else if !path.exists() {
		false
	} else {
		match &check.contains {
			None => true,
			Some(text) => String::from_utf8_lossy(&fs::read(&path)?).contains(text.as_str()),
		}
	};
	Ok(CheckResult { passed, timed_out: false })
}
