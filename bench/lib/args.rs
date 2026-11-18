//! The flags every benchmark script takes (0482): `--out <dir>` outside the repo (raw results live in
//! `D:/thesis/data/`), `--runs <n>` and `--dry-run`, plus the script's own `--name value` flags.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::LazyLock;

use clap::Args;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::pathkey::{is_inside, real_path, resolve};

/// The repo root, found at run time: the nearest ancestor of the running executable's folder that holds
/// `bench/prices`, as the TypeScript's `import.meta.dir` was the checkout the script sat in. A `jocasta-bench` built on
/// one machine and copied into another's checkout (the VM has no Rust toolchain) finds that checkout. When no ancestor
/// holds it (a `cargo run` or `cargo test` from a checkout whose target folder is elsewhere), the checkout the binary
/// was built from (`CARGO_MANIFEST_DIR`).
pub static REPO_ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
	let found = std::env::current_exe().ok().map(|exe| real_path(&exe)).and_then(|exe| {
		exe.ancestors().skip(1).find(|dir| dir.join("bench").join("prices").is_dir()).map(Path::to_path_buf)
	});
	found.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
});

/// The harness's one usage-error convention (DUP-18): a user error (exit 2), which [`run_script`] prints after
/// `bench: `; `evals` prints the same error after `jocasta: `.
pub struct BenchUsageError;

impl BenchUsageError {
	/// A refusal with exit code 2.
	pub fn err(message: impl Into<String>) -> Error {
		Error::user(message)
	}
}

/// A script's own flag value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagValue {
	/// A `--name value` flag's last value.
	Text(String),
	/// A listed boolean flag, given.
	Set,
}

#[derive(Debug, Clone)]
pub struct BenchArgs {
	/// Absolute; `None` only on a dry run without `--out`.
	pub out: Option<PathBuf>,
	pub runs: u64,
	pub dry_run: bool,
	/// The script's own flags, by name without the dashes.
	pub flags: HashMap<String, FlagValue>,
}

#[derive(Debug, Clone, Default)]
pub struct ArgSpec {
	pub default_runs: u64,
	/// The script's own flags that take a value.
	pub valued: &'static [&'static str],
	/// The script's own flags without a value.
	pub booleans: &'static [&'static str],
	/// Repo-relative or absolute paths that may lie inside the repo are fine; `--out` never may.
	pub repo_root: Option<PathBuf>,
}

impl ArgSpec {
	pub fn new(default_runs: u64) -> Self {
		Self { default_runs, ..Self::default() }
	}
}

pub fn parse_bench_args(argv: &[String], spec: &ArgSpec) -> Result<BenchArgs> {
	let is_valued = |flag: &str| matches!(flag, "--out" | "--runs") || spec.valued.contains(&flag);
	let is_boolean = |flag: &str| flag == "--dry-run" || spec.booleans.contains(&flag);
	let mut flags: HashMap<String, FlagValue> = HashMap::new();
	let mut at = 0;
	while at < argv.len() {
		let flag = argv[at].as_str();
		at += 1;
		if is_boolean(flag) {
			flags.insert(flag[2..].to_owned(), FlagValue::Set);
			continue;
		}
		if !is_valued(flag) {
			return Err(BenchUsageError::err(format!("unknown option: {flag}")));
		}
		let value = argv.get(at);
		at += 1;
		let Some(value) = value.filter(|value| !value.starts_with("--")) else {
			return Err(BenchUsageError::err(format!("{flag} needs a value")));
		};
		flags.insert(flag[2..].to_owned(), FlagValue::Text(value.clone()));
	}
	let dry_run = flags.remove("dry-run") == Some(FlagValue::Set);
	let mut runs = spec.default_runs;
	if let Some(FlagValue::Text(text)) = flags.remove("runs") {
		let whole = !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
		let count = if whole { text.parse::<u64>().unwrap_or(u64::MAX) } else { 0 };
		if count < 1 {
			return Err(BenchUsageError::err(format!("--runs needs a whole number of 1 or more, not \"{text}\"")));
		}
		runs = count;
	}
	let out = match flags.remove("out") {
		Some(FlagValue::Text(text)) => Some(resolve(Path::new(&text))),
		_ => None,
	};
	let root = spec.repo_root.clone().unwrap_or_else(|| REPO_ROOT.clone());
	if let Some(out) = out.as_deref().filter(|out| is_inside(&root, out)) {
		return Err(BenchUsageError::err(format!(
			"--out must be outside the repo (raw results live in D:/thesis/data/), not {}",
			out.display()
		)));
	}
	if out.is_none() && !dry_run {
		return Err(BenchUsageError::err("--out <dir> is required (outside the repo)"));
	}
	Ok(BenchArgs { out, runs, dry_run, flags })
}

/// A script's own valued flag.
pub fn flag<'a>(args: &'a BenchArgs, name: &str) -> Option<&'a str> {
	match args.flags.get(name) {
		Some(FlagValue::Text(value)) => Some(value),
		_ => None,
	}
}

/// A script's own valued flag, or `fallback`.
pub fn flag_or<'a>(args: &'a BenchArgs, name: &str, fallback: &'a str) -> &'a str {
	flag(args, name).unwrap_or(fallback)
}

/// A comma-separated list flag.
pub fn list_flag(args: &BenchArgs, name: &str, fallback: &[&str]) -> Vec<String> {
	match flag(args, name) {
		None => fallback.iter().map(|item| (*item).to_owned()).collect(),
		Some(value) => value.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_owned).collect(),
	}
}

/// A whole-number list flag such as `--threads 2,4,8`.
pub fn number_list_flag(args: &BenchArgs, name: &str, fallback: &[u64]) -> Result<Vec<u64>> {
	let fallback: Vec<String> = fallback.iter().map(u64::to_string).collect();
	let fallback: Vec<&str> = fallback.iter().map(String::as_str).collect();
	list_flag(args, name, &fallback)
		.into_iter()
		.map(|item| {
			let whole = !item.is_empty() && item.chars().all(|c| c.is_ascii_digit());
			match item.parse::<u64>() {
				Ok(number) if whole => Ok(number),
				_ => Err(BenchUsageError::err(format!("--{name} takes whole numbers, not \"{item}\""))),
			}
		})
		.collect()
}

/// A script's own arguments, handed on as typed: each script parses them with [`parse_bench_args`], whose flags and
/// errors are the TypeScript's.
#[derive(Args)]
pub struct ScriptArgs {
	#[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
	pub args: Vec<String>,
}

/// Runs a script's main and gives the process's exit code: a usage error (any user error, a [`BenchUsageError`] or a
/// refusal from the product's modules such as an unknown variant) prints one line after `bench: ` and exits with its
/// code (2); a dry run prints the plan and measures nothing. Any other error prints its message and exits 1 (the
/// TypeScript's uncaught error printed Bun's stack trace).
pub async fn run_script(main: impl Future<Output = Result<()>>) -> ExitCode {
	finish_script(main.await.map(|()| ExitCode::SUCCESS))
}

/// A synchronous script's end: its own exit code, or its error printed as [`run_script`] prints it.
pub fn finish_script(result: Result<ExitCode>) -> ExitCode {
	match result {
		Ok(code) => code,
		Err(Error::User { message, exit_code }) => {
			eprintln!("bench: {message}");
			ExitCode::from(exit_code)
		}
		Err(error) => {
			eprintln!("{error}");
			ExitCode::from(1)
		}
	}
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
