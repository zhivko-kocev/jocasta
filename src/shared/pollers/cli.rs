//! Running `gh` and `az` for the pollers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde::de::DeserializeOwned;

use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::process::{AbortSignal, RunOptions, on_path, run_program};

/// The two commands the pollers run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
	Gh,
	Az,
}

impl CliCommand {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Gh => "gh",
			Self::Az => "az",
		}
	}
}

/// What [`RunCli::run`] returns: stdout, or the failure.
pub type RunFuture = Pin<Box<dyn Future<Output = Result<String>> + Send>>;

/// Runs `gh` or `az` and returns stdout; a missing command, or an exit code other than 0 and `ok_codes`, fails with its
/// stderr's first line. `signal` ends the command (the service stopping). A closure of the same four parameters is a
/// `RunCli` too, as the tests' fakes are.
pub trait RunCli: Send + Sync {
	fn run(&self, command: CliCommand, args: Vec<String>, ok_codes: Vec<i32>, signal: Option<AbortSignal>)
	-> RunFuture;
}

impl<F> RunCli for F
where
	F: Fn(CliCommand, Vec<String>, Vec<i32>, Option<AbortSignal>) -> RunFuture + Send + Sync,
{
	fn run(
		&self,
		command: CliCommand,
		args: Vec<String>,
		ok_codes: Vec<i32>,
		signal: Option<AbortSignal>,
	) -> RunFuture {
		self(command, args, ok_codes, signal)
	}
}

/// `gh`/`az` from PATH, with no prompts, ended after a minute or when `signal` aborts: the real [`RunCli`].
pub struct SystemCli;

impl RunCli for SystemCli {
	fn run(
		&self,
		command: CliCommand,
		args: Vec<String>,
		ok_codes: Vec<i32>,
		signal: Option<AbortSignal>,
	) -> RunFuture {
		Box::pin(async move { run_cli(command, &args, &ok_codes, signal).await })
	}
}

/// The argv that runs `exe`. On Windows `az` is `az.cmd` under `C:\Program Files`; a `.cmd` at a path with spaces and
/// an argument with spaces too gets mangled, so cmd.exe gets the line quoted here. The pollers' arguments never hold
/// `"` or `%` (checked by [`run_cli`]).
fn command_line(exe: &str, args: &[String]) -> (Vec<String>, bool) {
	let lower = exe.to_ascii_lowercase();
	if !cfg!(windows) || !(lower.ends_with(".cmd") || lower.ends_with(".bat")) {
		return (std::iter::once(exe.to_owned()).chain(args.iter().cloned()).collect(), false);
	}
	let quoted = std::iter::once(exe)
		.chain(args.iter().map(String::as_str))
		.map(|a| {
			if a.is_empty() || a.chars().any(|c| c.is_whitespace() || "&|<>^()!,;='".contains(c)) {
				format!("\"{a}\"")
			} else {
				a.to_owned()
			}
		})
		.collect::<Vec<_>>()
		.join(" ");
	let shell = env::var("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned());
	(vec![shell, "/d".into(), "/s".into(), "/c".into(), format!("\"{quoted}\"")], true)
}

/// Runs `gh`/`az` (see [`SystemCli`]).
pub async fn run_cli(
	command: CliCommand,
	args: &[String],
	ok_codes: &[i32],
	signal: Option<AbortSignal>,
) -> Result<String> {
	let name = command.as_str();
	if args.iter().any(|a| a.contains(['"', '%'])) {
		return Err(Error::other(format!("{name}: an argument holds \" or %")));
	}
	let Some(exe) = on_path(name, None) else {
		return Err(Error::other(format!("{name} is not on PATH")));
	};
	let (argv, verbatim) = command_line(&exe.to_string_lossy(), args);
	let mut vars: HashMap<String, String> = env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect();
	vars.insert("GH_PROMPT_DISABLED".into(), "1".into());
	vars.insert("AZURE_CORE_NO_COLOR".into(), "1".into());
	let run = run_program(
		&argv,
		RunOptions {
			env: Some(vars),
			timeout_ms: Some(60_000),
			windows_verbatim_arguments: verbatim,
			signal,
			..RunOptions::default()
		},
	)
	.await?;
	let failed = match run.code {
		None => true,
		Some(code) => code != 0 && !ok_codes.contains(&code),
	};
	if failed {
		let why = if run.timed_out {
			"ran past a minute".to_owned()
		} else {
			format!(
				"exit {}",
				run.code.map_or_else(|| run.signal.clone().unwrap_or_else(|| "null".into()), |c| c.to_string())
			)
		};
		let first = run.stderr.trim().split('\n').next().unwrap_or("");
		let first = if first.is_empty() { why.as_str() } else { first };
		return Err(Error::other(format!(
			"{name} {} failed: {first}",
			args.iter().take(2).map(String::as_str).collect::<Vec<_>>().join(" ")
		)));
	}
	Ok(run.stdout)
}

/// stdout parsed as JSON; empty output is `fallback`.
pub async fn run_json<T: DeserializeOwned>(
	run: &dyn RunCli,
	command: CliCommand,
	args: &[&str],
	fallback: T,
	ok_codes: &[i32],
) -> Result<T> {
	let args = args.iter().map(|a| (*a).to_owned()).collect();
	let out = run.run(command, args, ok_codes.to_vec(), None).await?;
	let out = out.trim();
	if out.is_empty() { Ok(fallback) } else { Ok(serde_json::from_str(out)?) }
}

#[cfg(test)]
pub(crate) mod tests {
	use std::path::Path;
	use std::sync::{Arc, Mutex};
	use std::time::Duration;

	use serde_json::{Value, json};
	use tokio::sync::watch;

	use super::*;
	use crate::shared::testing::{TempDir, TempJocasta, temp_dir, temp_jocasta};

	/// What a fake `gh`/`az` was asked: each call as its command and arguments.
	pub(crate) type Calls = Arc<Mutex<Vec<Vec<String>>>>;

	/// A fake `gh`/`az`: answers by the command's words before its first flag or number, and logs each call.
	pub(crate) fn fake_run(responses: Vec<(&'static str, Value)>) -> (impl RunCli, Calls) {
		let calls: Calls = Arc::default();
		let log = calls.clone();
		let run = move |command: CliCommand, args: Vec<String>, _: Vec<i32>, _: Option<AbortSignal>| -> RunFuture {
			log.lock()
				.unwrap()
				.push(std::iter::once(command.as_str().to_owned()).chain(args.iter().cloned()).collect());
			let mut words = vec![command.as_str().to_owned()];
			for a in &args {
				if a.starts_with('-') || a.starts_with(|c: char| c.is_ascii_digit()) {
					break;
				}
				words.push(a.clone());
			}
			let key = words.join(" ");
			let answer = match responses.iter().find(|(name, _)| *name == key) {
				Some((_, Value::String(text))) => Ok(text.clone()),
				Some((_, value)) => Ok(value.to_string()),
				None => Err(Error::other(format!("no fake response for {key}"))),
			};
			Box::pin(async move { answer })
		};
		(run, calls)
	}

	const FAKE: &str = "shared::pollers::cli::tests::fake_gh_main";

	/// The stand-in `gh` (this test executable run again, see the shim): prints its arguments as JSON; "fail", "hang" and
	/// "checks" act.
	#[test]
	fn fake_gh_main() {
		if crate::shared::env::var("JOCASTA_FAKE_GH").is_none() {
			return;
		}
		let args: Vec<String> = std::env::args().skip_while(|a| a != "--quiet").skip(1).collect();
		match args.first().map(String::as_str) {
			Some("fail") => {
				eprintln!("boom\nmore");
				std::process::exit(2);
			}
			Some("hang") => std::thread::sleep(Duration::from_secs(10)),
			_ => {}
		}
		println!("{}", json!(args));
		std::process::exit(i32::from(args.first().map(String::as_str) == Some("checks")));
	}

	/// A folder with spaces holding a `gh` shim that runs [`fake_gh_main`] with its arguments.
	fn fake_gh() -> TempDir {
		let dir = temp_dir("jocasta fake gh");
		let exe = std::env::current_exe().unwrap();
		let command = format!("\"{}\" --exact {FAKE} --nocapture --quiet", exe.display());
		if cfg!(windows) {
			std::fs::write(dir.path().join("gh.cmd"), format!("@{command} %*\r\n")).unwrap();
		} else {
			let shim = dir.path().join("gh");
			std::fs::write(&shim, format!("#!/bin/sh\nexec {command} \"$@\"\n")).unwrap();
			#[cfg(unix)]
			std::fs::set_permissions(&shim, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
		}
		dir
	}

	/// Puts `dir` first on PATH (and the rest after it unless `only`), spelled `Path` with every other spelling unset,
	/// as a copied environment on Windows has it.
	fn path_with(jocasta: &mut TempJocasta, dir: &Path, only: bool) {
		let real: Vec<String> = std::env::vars_os()
			.filter_map(|(k, _)| k.into_string().ok())
			.filter(|k| k.eq_ignore_ascii_case("PATH"))
			.collect();
		let rest = real.iter().find_map(|k| std::env::var(k).ok()).unwrap_or_default();
		for name in &real {
			jocasta.set_var(name, None);
		}
		let separator = if cfg!(windows) { ";" } else { ":" };
		let value = if only { dir.display().to_string() } else { format!("{}{separator}{rest}", dir.display()) };
		jocasta.set_var("Path", Some(&value));
		jocasta.set_var("JOCASTA_FAKE_GH", Some("1"));
	}

	/// The JSON line a stand-in printed (libtest prints its own header first).
	fn json_line(out: &str) -> Value {
		let line = out.lines().find(|line| line.starts_with('[')).unwrap_or_else(|| panic!("no JSON in {out:?}"));
		serde_json::from_str(line).unwrap()
	}

	fn strings(args: &[&str]) -> Vec<String> {
		args.iter().map(|a| (*a).to_owned()).collect()
	}

	async fn gh(args: &[&str], ok_codes: &[i32]) -> Result<String> {
		run_cli(CliCommand::Gh, &strings(args), ok_codes, None).await
	}

	#[tokio::test]
	async fn run_cli_runs_the_gh_on_path_in_a_dir_with_spaces_and_accepts_the_exit_codes_a_caller_allows() {
		let mut jocasta = temp_jocasta();
		let dir = fake_gh();
		path_with(&mut jocasta, dir.path(), false);
		assert_eq!(json_line(&gh(&["api", "a b"], &[]).await.unwrap()), json!(["api", "a b"]));
		assert_eq!(json_line(&gh(&["checks"], &[1]).await.unwrap()), json!(["checks"]));
		let error = gh(&["checks"], &[]).await.unwrap_err();
		assert!(error.to_string().contains("gh checks failed: exit 1"), "{error}");
		let error = gh(&["fail"], &[]).await.unwrap_err();
		assert!(error.to_string().contains("gh fail failed: boom"), "{error}");
		let error = gh(&["say \"hi\""], &[]).await.unwrap_err();
		assert!(error.to_string().contains("holds \" or %"), "{error}");
		path_with(&mut jocasta, dir.path(), true);
		let error = run_cli(CliCommand::Az, &strings(&["x"]), &[], None).await.unwrap_err();
		assert!(error.to_string().contains("az is not on PATH"), "{error}");
		// The stop's abort ends a command that would answer only after ten seconds, and its whole tree with it.
		path_with(&mut jocasta, dir.path(), false);
		let (stop, signal) = watch::channel(None);
		let hang = strings(&["hang"]);
		let hanging = run_cli(CliCommand::Gh, &hang, &[], Some(signal));
		let abort = async {
			tokio::time::sleep(Duration::from_millis(300)).await;
			stop.send(Some("stopped".to_owned())).unwrap();
		};
		let (run, ()) = tokio::join!(hanging, abort);
		assert!(run.is_err());
	}

	#[tokio::test]
	async fn run_cli_finds_gh_when_the_environment_spells_the_variable_path() {
		let mut jocasta = temp_jocasta();
		let dir = fake_gh();
		path_with(&mut jocasta, dir.path(), false);
		assert_eq!(json_line(&gh(&["api", "x"], &[]).await.unwrap()), json!(["api", "x"]));
	}
}
