//! The one executor of `{run, expect}` steps (0706, 0707, 0708): `review_try`'s steps, a finding's proof and the
//! clean-room check all run here, in the clean room or in a folder on his machine, and are judged the same way: a step
//! passes when its output contains `expect`, or, for `expect: "exit:N"`, when it exits N.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use serde_json::Value;

use crate::frontier::brief::model::RanProof;
use crate::frontier::server::cleanroom::{self, Ran, tail};
use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, run_program};
use crate::shared::projects::Project;

#[cfg(test)]
#[path = "steps_tests.rs"]
mod tests;

/// One step: the command to run and what its output must hold (or `exit:N`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
	pub run: String,
	pub expect: String,
}

/// Where steps run: the clean room on a commit, or a folder on his machine with an environment.
pub enum Place<'a> {
	CleanRoom { project: &'a Project, commit: &'a str },
	Here { path: &'a Path, env: &'a HashMap<String, String> },
}

/// One step's time limit, a check's.
pub const STEP_MS: i64 = cleanroom::CHECK_MS;

/// Whether `ran` gives `expect`: its output holds it, or for `exit:N` it exited N.
pub fn gives(ran: &Ran, expect: &str) -> bool {
	match expect.trim().strip_prefix("exit:") {
		Some(code) => code.trim().parse::<i32>().is_ok_and(|code| ran.exit == Some(code)),
		None => ran.output.contains(expect),
	}
}

/// How a run ended, for a refusal: `exit 3: <tail>`, or that it didn't end in time.
pub fn said(ran: &Ran) -> String {
	let ended = ran.exit.map_or_else(|| "it didn't end in time".to_owned(), |code| format!("exit {code}"));
	let tail = tail(&ran.output);
	if tail.is_empty() { format!("{ended}: no output") } else { format!("{ended}: {tail}") }
}

/// The proof a card keeps of a step that ran.
pub fn proof(step: &Step, ran: &Ran) -> RanProof {
	RanProof { run: step.run.clone(), expect: step.expect.clone(), exit: ran.exit, tail: tail(&ran.output) }
}

/// The argv that runs one shell command line here: cmd.exe on Windows (`/d /s /c "<line>"`, verbatim), else `sh -c`.
fn shell_argv(command: &str) -> Vec<String> {
	if cfg!(windows) {
		let comspec = crate::shared::env::var_nonempty("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned());
		vec![comspec, "/d".to_owned(), "/s".to_owned(), "/c".to_owned(), format!("\"{command}\"")]
	} else {
		vec!["sh".to_owned(), "-c".to_owned(), command.to_owned()]
	}
}

/// Runs `command` at `place` within [`STEP_MS`]. An error is a run that couldn't start.
pub async fn run(place: &Place<'_>, command: &str) -> Result<Ran> {
	match place {
		Place::CleanRoom { project, commit } => cleanroom::run(project, commit, command, STEP_MS).await,
		Place::Here { path, env } => {
			let started = Instant::now();
			let options = RunOptions {
				cwd: Some(path.to_path_buf()),
				env: Some((*env).clone()),
				timeout_ms: Some(STEP_MS),
				windows_verbatim_arguments: cfg!(windows),
				end_with_jocasta: true,
				..RunOptions::default()
			};
			let run = run_program(&shell_argv(command), options).await?;
			let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
			let output =
				if run.stderr.trim().is_empty() { run.stdout } else { format!("{}\n{}", run.stdout, run.stderr) };
			Ok(Ran { exit: if run.timed_out { None } else { run.code }, output, ms })
		}
	}
}

/// One `{run, expect}` object as given to `tool`; `what` names it in the refusal (`steps[2]`, `proof`).
pub fn step_of(value: &Value, tool: &str, what: &str) -> Result<Step> {
	let text = |key: &str| {
		value
			.get(key)
			.and_then(Value::as_str)
			.map(str::trim)
			.filter(|text| !text.is_empty())
			.map(str::to_owned)
			.ok_or_else(|| {
				Error::user(format!(
					"{tool}: {what} needs `run` (a command) and `expect` (what its output must hold, or exit:N)."
				))
			})
	};
	if let Some(extra) =
		value.as_object().and_then(|map| map.keys().find(|key| !matches!(key.as_str(), "run" | "expect")))
	{
		return Err(Error::user(format!("{tool}: {what} has an unknown key `{extra}`; a step is {{run, expect}}.")));
	}
	let step = Step { run: text("run")?, expect: text("expect")? };
	if let Some(code) = step.expect.strip_prefix("exit:")
		&& code.trim().parse::<i32>().is_err()
	{
		return Err(Error::user(format!("{tool}: {what}'s expect `{}` isn't exit:<number>.", step.expect)));
	}
	Ok(step)
}

/// `steps` as given to `tool`: a non-empty list of `{run, expect}`.
pub fn steps_of(value: Option<&Value>, tool: &str) -> Result<Vec<Step>> {
	let items = value
		.and_then(Value::as_array)
		.filter(|items| !items.is_empty())
		.ok_or_else(|| Error::user(format!("{tool}: steps is a list of {{run, expect}}, at least one.")))?;
	items.iter().enumerate().map(|(i, item)| step_of(item, tool, &format!("step {}", i + 1))).collect()
}
