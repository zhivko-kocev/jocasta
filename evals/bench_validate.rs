//! `--validate` of the task benchmark (0482): each task's hidden tests without a model.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use super::runner::{EvalCase, grade_hidden, make_trial_repo, make_trial_root, trial_env};
use super::tasks::{BenchTask, PreparedTask, prepare_task, task_case};
use crate::lib::git::git;
use crate::lib::output::append_row;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;

/// One `validation.jsonl` row: the task's hidden tests without a model, on the base commit and with the gold patch.
#[derive(Debug, Serialize)]
pub struct ValidationRow {
	pub task: String,
	pub language: super::tasks::TaskLanguage,
	/// The hidden tests failed on the base commit (as they must); `None` when validation stopped before.
	pub null_failed: Option<bool>,
	/// Gold-patch gradings that passed, of [`GOLD_GRADES`].
	pub gold_passes: u32,
	pub seconds: ValidationSeconds,
	pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ValidationSeconds {
	pub prepare: f64,
	pub setup: f64,
	pub null: Option<f64>,
	pub gold: Vec<f64>,
}

pub const GOLD_GRADES: u32 = 3;

/// Seconds to a tenth, as the TypeScript's `since` rounded them.
fn since(start: Instant) -> f64 {
	((start.elapsed().as_secs_f64() * 1000.0 / 100.0) + 0.5).floor() / 10.0
}

/// Validates one task without a model (0482, the SWE-bench-Live check): the fixture with its setup, graded unchanged on
/// a clean clone (a *null* trial, which must fail), then `gold.patch` applied as the "agent" and graded on a fresh clean
/// clone [`GOLD_GRADES`] times (each must pass). Grading is the runner's own `grade_hidden`, its limit included. With
/// `log_dir`, each phase's commands and output go to `setup.log`, `null.log` and `gold-<n>.log` there; the trial root
/// goes in `work_dir`.
async fn validate_task(
	task: &BenchTask,
	cache_dir: &Path,
	log_dir: Option<&Path>,
	work_dir: Option<&Path>,
) -> Result<ValidationRow> {
	let mut row = ValidationRow {
		task: task.instance_id.clone(),
		language: task.language,
		null_failed: None,
		gold_passes: 0,
		seconds: ValidationSeconds { prepare: 0.0, setup: 0.0, null: None, gold: Vec::new() },
		error: None,
	};
	let mut start = Instant::now();
	let prepared: PreparedTask = match prepare_task(task, cache_dir) {
		Ok(prepared) => prepared,
		Err(error) => {
			row.error = Some(error.to_string());
			return Ok(row);
		}
	};
	row.seconds.prepare = since(start);
	let eval_case = task_case(task, &prepared);
	let log_file = |phase: &str| log_dir.map(|dir| dir.join(format!("{phase}.log")));
	if let Some(dir) = log_dir {
		remove_tree(dir)?;
	}
	let root = make_trial_root(work_dir)?;
	let outcome: Result<()> = async {
		let no_extra = HashMap::new();
		let env = trial_env(&eval_case, &root, &no_extra);
		let grade_case = EvalCase {
			path_prefix: eval_case.grade_path_prefix.clone().or_else(|| eval_case.path_prefix.clone()),
			..eval_case.clone()
		};
		let grade_env = trial_env(&grade_case, &root, &no_extra);
		start = Instant::now();
		let made = make_trial_repo(&eval_case, 0, &root, &env, log_file("setup").as_deref(), None).await?;
		row.seconds.setup = since(start);
		let passes = |grade_dir: &str, phase: &str| {
			let grade_path = root.join(grade_dir);
			let phase_log = log_file(phase);
			let (eval_case, made, grade_env) = (&eval_case, &made, &grade_env);
			async move {
				let grades = grade_hidden(
					eval_case,
					&made.repo,
					&made.fixture_commit,
					&grade_path,
					grade_env,
					phase_log.as_deref(),
					None,
				)
				.await?;
				Ok::<bool, Error>(grades.values().all(|score| score.as_f64() == Some(1.0)))
			}
		};
		start = Instant::now();
		row.null_failed = Some(!passes("g0", "null").await?);
		row.seconds.null = Some(since(start));
		git(&made.repo, &["apply", "--whitespace=nowarn", &prepared.gold_patch.to_string_lossy()], None)?;
		git(&made.repo, &["add", "-A"], None)?;
		for i in 1..=GOLD_GRADES {
			start = Instant::now();
			if passes(&format!("g{i}"), &format!("gold-{i}")).await? {
				row.gold_passes += 1;
			}
			row.seconds.gold.push(since(start));
		}
		Ok(())
	}
	.await;
	if let Err(error) = outcome {
		row.error = Some(error.to_string());
	}
	remove_tree(&root)?;
	Ok(row)
}

fn validation_passed(row: &ValidationRow) -> bool {
	row.error.is_none() && row.null_failed == Some(true) && row.gold_passes == GOLD_GRADES
}

/// `--validate`: every task in turn into `<out>/validation.jsonl` (rewritten), each phase's log in
/// `<out>/validation/<task>/`, then a summary; the failing ids.
pub async fn run_validation(
	tasks: &[BenchTask],
	cache_dir: &Path,
	out_dir: &Path,
	log: &dyn Fn(&str),
	work_dir: Option<&Path>,
) -> Result<Vec<String>> {
	fs::create_dir_all(out_dir)?;
	let file = out_dir.join("validation.jsonl");
	fs::write(&file, "")?;
	let mut failed: Vec<String> = Vec::new();
	for task in tasks {
		log(&format!("validating {}", task.instance_id));
		let log_dir = out_dir.join("validation").join(&task.instance_id);
		let row = validate_task(task, cache_dir, Some(&log_dir), work_dir).await?;
		append_row(out_dir, "validation", &row)?;
		if !validation_passed(&row) {
			failed.push(task.instance_id.clone());
		}
		let null = match row.null_failed {
			None => "not run",
			Some(true) => "failed",
			Some(false) => "PASSED",
		};
		let error = row
			.error
			.as_ref()
			.map_or_else(String::new, |error| format!(", error: {}", error.split('\n').next().unwrap_or("")));
		log(&format!(
			"  {}: null {null}, gold {}/{GOLD_GRADES}{error}",
			if validation_passed(&row) { "ok" } else { "FAILED" },
			row.gold_passes
		));
	}
	log(&format!("{}/{} tasks valid; results: {}", tasks.len() - failed.len(), tasks.len(), file.display()));
	if !failed.is_empty() {
		log(&format!("swap for a spare of the same language: {}", failed.join(", ")));
	}
	Ok(failed)
}
