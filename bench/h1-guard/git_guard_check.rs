//! H1 (0482, 0501, 0535, 0623, 0690): measures omp 19.2.0's git guard instead of trusting `git_guard`'s reimplementation.
//! For the VM (a throwaway Windows 11 machine named JOCASTA-CLEAN); it RUNS corpus commands, so it refuses anywhere
//! else.
//!
//! ```text
//! jocasta-bench h1-guard git-guard-check --corpus <corpus.jsonl> --out <dir> [--omp <package dir>] [--timeout 60]
//!   [--any-version] (--dry-run | --run)
//! ```
//!
//! Lines: every corpus line made only of git (any spelling the corpus uses: `git`, `git.exe`, the full path, `git -C …`,
//! also inside `pwsh -c`/`cmd /c`), `cd …` and `echo …`, split with omp's own tokenizer. For each line, twice, on a
//! fresh fixture at C:\work\demo (the corpus's path; recipe below): omp's own `executeBash` from
//! `exec/bash-executor.ts`, the call the bash tool makes (`tools/bash.ts:1630-1652,2552-2571`), in C:\work\demo (a main
//! checkout, where the guard applies its shared-checkout rules) with `gitGuard: true`, then `gitGuard: false` as the
//! control. The tool also passes `ovnfs`, which only fills the refusal's guidance text. git's config is isolated for
//! the fixture and the runs (`GIT_CONFIG_GLOBAL` to a temp file, `GIT_CONFIG_NOSYSTEM=1`, a temp HOME and USERPROFILE),
//! and omp's settings come from an empty agent folder, so nothing outside C:\work and the temp folder changes. Output
//! is the shell's combined stdout and stderr, as the bash tool gets it. omp's code runs in `omp-verdicts.ts` (Bun, 0636).
//!
//! It runs only with `--run`, on a machine named JOCASTA-CLEAN, with C:\work holding nothing but the fixture's two
//! folders. Writes `git-guard-check.jsonl` (per line: both runs' exit code and trimmed output, the guard's refusal
//! message, `git_guard`'s prediction, agreement) and `git-guard-check.json` (agreement per subcommand and per way of
//! writing, every disagreement listed). `omp-verdicts.ts --git-guard <git-guard-check.jsonl>` then uses the measured
//! refusals for these lines.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value, json};

use super::evaluate::{CorpusEntry, read_corpus};
use super::git_guard::{Tokenize, git_guard_refusal_of, git_only_argv, git_subcommand};
use super::oracle::{ExecCall, Executed, OmpInfo, Oracle};
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag, flag_or, parse_bench_args};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::sandbox::{VM_MACHINE, machine_name, require_vm};
use crate::lib::temp::mkdtemp;
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::process::{RunOptions, run_program_sync};

pub const WORK_ROOT: &str = "C:\\work";
pub const FIXTURE: &str = "C:\\work\\demo";
pub const ORIGIN: &str = "C:\\work\\demo-origin.git";
/// What C:\work may hold before a run: nothing, or the fixture's own two folders.
const FIXTURE_ENTRIES: [&str; 2] = ["demo", "demo-origin.git"];

/// One step of the fixture: a git call (in `cwd`, else the fixture) or a file written under the fixture.
pub enum Step {
	Git { args: &'static [&'static str], cwd: Option<&'static str> },
	Write { path: &'static str, text: &'static str },
}

const fn git(args: &'static [&'static str]) -> Step {
	Step::Git { args, cwd: None }
}

const fn write(path: &'static str, text: &'static str) -> Step {
	Step::Write { path, text }
}

/// The fixture: commits on `main`, the branches and tag the corpus names, `feature/x` (which its force-pushes name)
/// with its own commit, a bare `origin` with everything pushed, one stash entry, `src/app.ts` (the tracked file the
/// corpus names) modified and uncommitted, and an untracked file. Paths are relative to C:\work\demo unless `cwd` says
/// otherwise.
pub const FIXTURE_RECIPE: [Step; 25] = [
	Step::Git { args: &["init", "-b", "main", FIXTURE], cwd: Some(WORK_ROOT) },
	write("README.md", "# demo\n"),
	write("src/app.ts", "export const app = 1;\n"),
	git(&["add", "."]),
	git(&["commit", "-m", "initial"]),
	write("src/app.ts", "export const app = 2;\n"),
	git(&["commit", "-am", "second"]),
	git(&["tag", "v0.1.0"]),
	git(&["branch", "dev"]),
	git(&["branch", "backup"]),
	git(&["branch", "old-feature"]),
	// One branch with its own commit. Git can't hold `feature` beside `feature/x` (a ref can't also be a folder).
	git(&["switch", "-c", "feature/x"]),
	write("feature.txt", "feature work\n"),
	git(&["add", "feature.txt"]),
	git(&["commit", "-m", "feature work"]),
	git(&["switch", "main"]),
	Step::Git { args: &["init", "--bare", "-b", "main", ORIGIN], cwd: Some(WORK_ROOT) },
	git(&["remote", "add", "origin", ORIGIN]),
	git(&["push", "origin", "--all"]),
	git(&["push", "origin", "--tags"]),
	git(&["branch", "--set-upstream-to=origin/main", "main"]),
	write("src/app.ts", "export const app = 3; // stashed\n"),
	git(&["stash", "push", "-m", "fixture stash"]),
	write("src/app.ts", "export const app = 4; // uncommitted\n"),
	write("notes.txt", "untracked\n"),
];

const GITCONFIG: &str = "[user]\n\tname = H1 Check\n\temail = h1-check@example.invalid\n[core]\n\tautocrlf = false\n";

/// The env that isolates git's config for the fixture and the runs, in the order the TypeScript listed it.
pub fn isolated_git_env(temp: &Path) -> Map<String, Value> {
	let home = temp.join("home").to_string_lossy().into_owned();
	let mut vars = Map::new();
	vars.insert("GIT_CONFIG_GLOBAL".to_owned(), json!(temp.join("gitconfig").to_string_lossy()));
	vars.insert("GIT_CONFIG_NOSYSTEM".to_owned(), json!("1"));
	vars.insert("GIT_TERMINAL_PROMPT".to_owned(), json!("0"));
	vars.insert("HOME".to_owned(), json!(home));
	vars.insert("USERPROFILE".to_owned(), json!(home));
	vars
}

pub struct CheckLine {
	pub entry: CorpusEntry,
	pub subcommand: String,
	/// `git_guard`'s prediction: its refusal, or `None`.
	pub reimplemented: Option<String>,
}

/// The corpus lines the check runs, with their git subcommand and `git_guard`'s prediction.
pub fn check_lines(corpus: &[CorpusEntry], tokenize: Tokenize<'_>) -> Result<Vec<CheckLine>> {
	let mut lines = Vec::new();
	for entry in corpus {
		let Some(argv) = git_only_argv(&entry.command, tokenize)? else { continue };
		lines.push(CheckLine {
			entry: entry.clone(),
			subcommand: git_subcommand(&argv),
			reimplemented: git_guard_refusal_of(&tokenize(&entry.command)?),
		});
	}
	Ok(lines)
}

static BLOCKED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"[^\n]*(?-u:\b)is blocked(?-u:\b)[^\n]*").expect("a fixed pattern"));

/// The git guard's refusal message in a run's output, or `None`.
pub fn refusal_in(output: &str) -> Option<String> {
	BLOCKED.find(output).map(|found| found.as_str().trim().to_owned())
}

/// One run of a line through omp's `executeBash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
	pub exit_code: Option<i32>,
	pub timed_out: bool,
	/// Combined stdout and stderr, trimmed to 2,000 characters.
	pub output: String,
	/// The git guard's message in the output, or `None`.
	pub refusal: Option<String>,
}

/// JavaScript's `trimStart` white space: Unicode white space and the byte order mark.
fn js_space(c: char) -> bool {
	c.is_whitespace() || c == '\u{feff}'
}

static SHELL_STOPPED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^error: (cd:|command not found:)").expect("a fixed pattern"));

/// omp's shell error when it stopped a line before git ran, else `None`: its `cd` reads an unquoted `C:\work\demo` with
/// `\` as an escape, and a missing program (`pwsh` where PowerShell 7 isn't installed) is "command not found".
pub fn shell_stopped(output: &str) -> Option<String> {
	let first = output.trim_start_matches(js_space).split('\n').next().unwrap_or("");
	let first = first.strip_suffix('\r').unwrap_or(first);
	SHELL_STOPPED.is_match(first).then(|| first.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckRow {
	pub id: String,
	pub command: String,
	pub variant: String,
	pub subcommand: String,
	/// Both runs completed on a fresh fixture.
	pub measured: bool,
	pub refusal: Option<String>,
	pub reimplemented: Option<String>,
	pub agree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disagreement {
	pub id: String,
	pub command: String,
	pub measured: Option<String>,
	pub reimplemented: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Agreement {
	pub key: String,
	pub n: usize,
	pub agree: usize,
	pub disagreements: Vec<Disagreement>,
}

/// Measured vs reimplemented refusal per `key_of`, over measured rows, in first-seen order.
pub fn agreement(rows: &[CheckRow], key_of: impl Fn(&CheckRow) -> String) -> Vec<Agreement> {
	let mut by_key: Vec<Agreement> = Vec::new();
	for row in rows.iter().filter(|row| row.measured) {
		let key = key_of(row);
		let at = by_key.iter().position(|entry| entry.key == key).unwrap_or_else(|| {
			by_key.push(Agreement { key, n: 0, agree: 0, disagreements: Vec::new() });
			by_key.len() - 1
		});
		let entry = &mut by_key[at];
		entry.n += 1;
		if row.agree {
			entry.agree += 1;
		} else {
			entry.disagreements.push(Disagreement {
				id: row.id.clone(),
				command: row.command.clone(),
				measured: row.refusal.clone(),
				reimplemented: row.reimplemented.clone(),
			});
		}
	}
	by_key
}

/// `JSON.stringify(text)`.
fn quoted(text: &str) -> String {
	Value::String(text.to_owned()).to_string()
}

fn describe_step(step: &Step) -> String {
	match step {
		Step::Git { args, cwd } => {
			let words: Vec<String> = args
				.iter()
				.map(|arg| if arg.chars().any(js_space) { quoted(arg) } else { (*arg).to_owned() })
				.collect();
			let place = cwd.map_or_else(String::new, |cwd| format!(" (in {cwd})"));
			format!("git {}{place}", words.join(" "))
		}
		Step::Write { path, text } => format!("write {path}: {}", quoted(text)),
	}
}

/// Refuses unless this is the VM and C:\work holds nothing but the fixture.
pub fn require_clean_machine(machine: &str, work_entries: Option<&[String]>) -> Result<()> {
	require_vm(machine, "run corpus commands")?;
	let strangers: Vec<&str> = work_entries
		.unwrap_or_default()
		.iter()
		.map(String::as_str)
		.filter(|name| !FIXTURE_ENTRIES.contains(name))
		.collect();
	if !strangers.is_empty() {
		return Err(BenchUsageError::err(format!(
			"refusing: {WORK_ROOT} holds more than the fixture ({})",
			strangers.join(", ")
		)));
	}
	Ok(())
}

const USAGE: &str = "usage: jocasta-bench h1-guard git-guard-check --corpus <corpus.jsonl> --out <dir> [--omp <package dir>] [--timeout 60] [--any-version] (--dry-run | --run)";

/// A fresh fixture, then `command` through omp's `executeBash`.
struct Runner<'a> {
	oracle: &'a RefCell<Oracle>,
	git_env: &'a Map<String, Value>,
	timeout_ms: u64,
}

impl Runner<'_> {
	fn reset_fixture(&self) -> Result<()> {
		for dir in [FIXTURE, ORIGIN] {
			remove_tree(Path::new(dir))?;
		}
		fs::create_dir_all(WORK_ROOT)?;
		let mut vars: HashMap<String, String> = env::vars()
			.into_iter()
			.map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
			.collect();
		vars.extend(self.git_env.iter().map(|(key, value)| (key.clone(), value.as_str().unwrap_or("").to_owned())));
		for step in &FIXTURE_RECIPE {
			match step {
				Step::Write { path, text } => {
					let file = Path::new(FIXTURE).join(path);
					if let Some(folder) = file.parent() {
						fs::create_dir_all(folder)?;
					}
					fs::write(file, text)?;
				}
				Step::Git { args, cwd } => {
					let mut argv = vec!["git"];
					argv.extend_from_slice(args);
					let options = RunOptions {
						cwd: Some(PathBuf::from(cwd.unwrap_or(FIXTURE))),
						env: Some(vars.clone()),
						..RunOptions::default()
					};
					let result = run_program_sync(&argv, options)?;
					if result.code != Some(0) {
						return Err(Error::other(format!(
							"fixture step \"{}\" failed: {}",
							describe_step(step),
							result.stderr.trim()
						)));
					}
				}
			}
		}
		Ok(())
	}

	fn run_once(&self, command: &str, git_guard: bool) -> Result<RunResult> {
		self.reset_fixture()?;
		let call = ExecCall { command, cwd: FIXTURE, timeout_ms: self.timeout_ms, git_guard, env: self.git_env };
		let Executed { exit_code, timed_out, output } = self.oracle.borrow_mut().exec(&call)?;
		Ok(RunResult { exit_code, timed_out, output: clipped(&output), refusal: refusal_in(&output) })
	}
}

/// `output` cut to 2,000 UTF-16 units with an ellipsis, as `slice(0, 2000)` cut it.
fn clipped(output: &str) -> String {
	let units: Vec<u16> = output.encode_utf16().collect();
	if units.len() <= 2000 {
		return output.to_owned();
	}
	format!("{}…", String::from_utf16_lossy(&units[..2000]))
}

/// One line, run twice: the guarded run and the control.
fn measure_line(runner: &Runner<'_>, line: &CheckLine) -> (CheckRow, Value) {
	let entry = &line.entry;
	let (mut guarded, mut control, mut error) = (None, None, None);
	match runner.run_once(&entry.command, true) {
		Ok(run) => {
			guarded = Some(run);
			match runner.run_once(&entry.command, false) {
				Ok(run) => control = Some(run),
				Err(caught) => error = Some(caught.to_string()),
			}
		}
		Err(caught) => error = Some(caught.to_string()),
	}
	// A line omp's shell stops before git (the control fails the same way) says nothing about the guard.
	let stopped = control.as_ref().and_then(|run| shell_stopped(&run.output));
	if let Some(stopped) = &stopped {
		error = Some(format!("omp's shell stopped before git: {stopped}"));
	}
	let measured = guarded.is_some() && control.is_some() && stopped.is_none();
	let refusal = guarded.as_ref().and_then(|run| run.refusal.clone());
	let row = CheckRow {
		id: entry.id.clone().unwrap_or_default(),
		command: entry.command.clone(),
		variant: entry.variant.clone().unwrap_or_default(),
		subcommand: line.subcommand.clone(),
		measured,
		refusal: refusal.clone(),
		reimplemented: line.reimplemented.clone(),
		agree: measured && refusal.is_some() == line.reimplemented.is_some(),
	};
	let control_also_blocked = control.as_ref().is_some_and(|run| run.refusal.is_some());
	let shown = if measured {
		(if refusal.is_none() { "ran" } else { "refused" }).to_owned()
	} else {
		format!("error: {}", error.as_deref().unwrap_or("null"))
	};
	println!(
		"{} {shown}{} {}",
		row.id,
		if control_also_blocked { " (control also blocked)" } else { "" },
		entry.command
	);
	let mut written = serde_json::to_value(&row).unwrap_or(Value::Null);
	if let Some(map) = written.as_object_mut() {
		map.insert("expected".to_owned(), json!(entry.expected));
		map.insert("guarded".to_owned(), json!(guarded));
		map.insert("control".to_owned(), json!(control));
		map.insert("error".to_owned(), json!(error));
	}
	(row, written)
}

/// The dry run's plan.
fn plan(args: &BenchArgs, omp: &OmpInfo, lines: &[CheckLine], temp: &Path, timeout_ms: u64) {
	let git_env = isolated_git_env(temp);
	let mut by_subcommand = Map::new();
	for line in lines {
		let count = by_subcommand.get(&line.subcommand).and_then(Value::as_u64).unwrap_or(0);
		by_subcommand.insert(line.subcommand.clone(), json!(count + 1));
	}
	let call = |guard: bool| {
		format!(
			"executeBash(<line>, {})",
			json!({ "cwd": FIXTURE, "timeout": timeout_ms, "gitGuard": guard, "env": &git_env })
		)
	};
	let mut plan = vec![
		format!(
			"omp {} from {}; refuses to run without --run, off {VM_MACHINE}, or with {WORK_ROOT} holding more than demo and demo-origin.git",
			omp.version, omp.dir
		),
		format!(
			"{} lines made only of git, cd and echo (by subcommand: {})",
			lines.len(),
			Value::Object(by_subcommand)
		),
		format!(
			"per line, twice (guard on, then the control), a fresh fixture: remove {FIXTURE} and {ORIGIN}, then, with git's config isolated ({}; {} sets user.name/email):",
			Value::Object(git_env.clone()),
			temp.join("gitconfig").display()
		),
	];
	plan.extend(FIXTURE_RECIPE.iter().enumerate().map(|(i, step)| format!("  {}. {}", i + 1, describe_step(step))));
	plan.push(format!(
		"omp: Settings.init({{ cwd: \"{}\", agentDir: \"{}\" }}), then from {}:",
		temp.join("cwd").display(),
		temp.join("agent").display(),
		Path::new(&omp.dir).join("src").join("exec").join("bash-executor.ts").display()
	));
	plan.push(format!("  {}", call(true)));
	plan.push(format!("  {}   (the control)", call(false)));
	plan.push("lines (id, variant, subcommand, git_guard's prediction, command):".to_owned());
	plan.extend(lines.iter().map(|line| {
		format!(
			"{} {} {} {} {}",
			line.entry.id.as_deref().unwrap_or("undefined"),
			line.entry.variant.as_deref().unwrap_or("undefined"),
			line.subcommand,
			line.reimplemented.as_ref().map_or("runs".to_owned(), |why| format!("refused ({why})")),
			line.entry.command
		)
	}));
	print_plan("H1 git guard check", &plan, args.out.as_deref());
}

/// The script: `jocasta-bench h1-guard git-guard-check …`.
pub fn main(argv: &[String]) -> Result<()> {
	if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
		println!("{USAGE}");
		return Ok(());
	}
	let spec = ArgSpec {
		valued: &["--corpus", "--omp", "--timeout"],
		booleans: &["--any-version", "--run"],
		..ArgSpec::new(1)
	};
	let args = parse_bench_args(argv, &spec)?;
	let corpus_path = flag(&args, "corpus").ok_or_else(|| BenchUsageError::err("--corpus <path.jsonl> is required"))?;
	if !Path::new(corpus_path).exists() {
		return Err(BenchUsageError::err(format!("{corpus_path} does not exist")));
	}
	let timeout_text = flag_or(&args, "timeout", "60");
	let seconds =
		if timeout_text.chars().all(|c| c.is_ascii_digit()) { timeout_text.parse::<u64>().unwrap_or(0) } else { 0 };
	if timeout_text.is_empty() || seconds < 1 {
		return Err(BenchUsageError::err(format!("--timeout needs whole seconds, not \"{timeout_text}\"")));
	}
	let timeout_ms = seconds * 1000;
	let oracle = RefCell::new(Oracle::start()?);
	let omp_dir = flag(&args, "omp").map(|dir| jocasta::shared::pathkey::resolve(Path::new(dir)));
	let omp = oracle.borrow_mut().load(omp_dir.as_deref(), args.flags.contains_key("any-version"))?;
	let (corpus, corpus_sha256) = read_corpus(Path::new(corpus_path))?;
	let tokenize = |command: &str| oracle.borrow_mut().tokenize(command);
	let lines = check_lines(&corpus, &tokenize)?;
	if args.dry_run {
		let temp = std::env::temp_dir().join("h1-git-guard-<random>");
		plan(&args, &omp, &lines, &temp, timeout_ms);
		return Ok(());
	}
	if !args.flags.contains_key("run") {
		return Err(BenchUsageError::err("this runs corpus commands: pass --run (in the VM) or --dry-run"));
	}
	if !cfg!(windows) {
		return Err(BenchUsageError::err("the fixture path is C:\\work\\demo: run on Windows"));
	}
	let entries: Option<Vec<String>> = fs::read_dir(WORK_ROOT)
		.ok()
		.map(|dir| dir.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect());
	require_clean_machine(&machine_name(), entries.as_deref())?;
	let out = args.out.as_deref().ok_or_else(|| BenchUsageError::err("--out <dir> is required (outside the repo)"))?;
	claim_rows(out, &["git-guard-check"])?;
	run_all(&oracle, &lines, timeout_ms, (&omp, &corpus_sha256), out)
}

/// The real run: the fixture, both runs of every line, the rows and the summary.
fn run_all(
	oracle: &RefCell<Oracle>,
	lines: &[CheckLine],
	timeout_ms: u64,
	(omp, corpus_sha256): (&OmpInfo, &str),
	out: &Path,
) -> Result<()> {
	let started = conditions(&["git"]);
	let temp = mkdtemp("h1-git-guard-")?;
	let git_env = isolated_git_env(&temp);
	for folder in ["home", "cwd", "agent"] {
		fs::create_dir_all(temp.join(folder))?;
	}
	fs::write(temp.join("gitconfig"), GITCONFIG)?;
	oracle.borrow_mut().exec_init(&temp, &git_env)?;
	let runner = Runner { oracle, git_env: &git_env, timeout_ms };
	let mut rows: Vec<CheckRow> = Vec::new();
	let measured_all = (|| -> Result<()> {
		for line in lines {
			let (row, written) = measure_line(&runner, line);
			rows.push(row);
			append_row(out, "git-guard-check", &written)?;
		}
		Ok(())
	})();
	for dir in [FIXTURE, ORIGIN] {
		remove_tree(Path::new(dir))?;
	}
	measured_all?;
	let by_subcommand = agreement(&rows, |row| row.subcommand.clone());
	let by_variant = agreement(&rows, |row| row.variant.clone());
	let measured_rows: Vec<&CheckRow> = rows.iter().filter(|row| row.measured).collect();
	let mut finished = started.finished();
	finished.versions.insert("omp".to_owned(), json!(omp.version));
	let agreeing = measured_rows.iter().filter(|row| row.agree).count();
	let summary = json!({
		"benchmark": "H1 git guard check",
		"conditions": finished,
		"omp": omp.version,
		"corpusSha256": corpus_sha256,
		"fixture": FIXTURE_RECIPE.iter().map(describe_step).collect::<Vec<_>>(),
		"timeoutMs": timeout_ms,
		"lines": rows.len(),
		"measured": measured_rows.len(),
		"agree": agreeing,
		"bySubcommand": by_subcommand,
		"byVariant": by_variant,
		"disagreements": by_subcommand.iter().flat_map(|entry| entry.disagreements.clone()).collect::<Vec<_>>(),
		"errors": rows.iter().filter(|row| !row.measured).map(|row| row.id.clone()).collect::<Vec<_>>(),
	});
	let file = write_json(out, "git-guard-check", &summary)?;
	println!("agreement {agreeing}/{}; wrote {}", measured_rows.len(), file.display());
	// Windows can hold a file there a while longer (seen in the VM with nothing working in the folder); the results
	// are written by now, so the run still counts.
	if let Err(error) = remove_tree(&temp) {
		eprintln!("couldn't remove {}: {error}", temp.display());
	}
	Ok(())
}

#[cfg(test)]
#[path = "git_guard_check_tests.rs"]
mod tests;
