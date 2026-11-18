//! The ledger's parser check (0538): the benchmark's ledger totals per session against tokscale's, bench-only.
//!
//! ```text
//! jocasta-bench tokscale-parity --out <task-benchmark runDir> [--dry-run]
//! ```
//!
//! For every trial of the run (`results.jsonl` names each trial's harness), the ledger reads its saved sessions
//! (`sessions/<task>-<arm>-<rep>/`) and sums uncached input, cache read, cache write and output per session; tokscale
//! reads the same sessions where it finds them: omp's saved folders through `TOKSCALE_EXTRA_DIRS=omp:<dir>,…` (omp ran
//! with `--session-dir` in the trial root), Claude Code's transcripts and opencode's database in their default places on
//! the VM, and `tokscale models --json --group-by client,session,model` gives its totals per session. Tokens must match
//! exactly; dollars aren't compared (tokscale prices with today's LiteLLM, the ledger with the pinned table). Writes
//! tokscale-parity.jsonl (one row per session, rewritten on each check) and tokscale-parity.json; exits 1 on any
//! mismatch and 2 when no saved session was compared.
//!
//! tokscale is approved for the benchmark only (0538) and isn't installed on his PC: pinned v4.21.0 (2026-11-02, MIT),
//! the Windows binary from https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0, on PATH in the VM. Without it the
//! script (and its test) say so and stop.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Value, json};

use crate::lib::args::{ArgSpec, BenchUsageError, parse_bench_args};
use crate::lib::ledger::{bench_pricing, read_trial_sessions};
use crate::lib::output::{conditions, print_plan, write_json, write_rows};
use crate::lib::prices::load_price_table;
use crate::lib::temp::mkdtemp;
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::ledger::{LedgerHarness, SessionRecord};
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};

pub const TOKSCALE_VERSION: &str = "4.21.0";
pub const TOKSCALE_RELEASE: &str = "https://github.com/junhoyeo/tokscale/releases/tag/v4.21.0";
/// What a skipped check says.
pub fn tokscale_missing() -> String {
	format!(
		"tokscale is not on PATH: the parity check is bench-only (0538); install tokscale {TOKSCALE_VERSION} from {TOKSCALE_RELEASE} in the VM"
	)
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTotals {
	pub input: f64,
	pub cache_read: f64,
	pub cache_write: f64,
	pub output: f64,
}

/// tokscale's totals for a session, with the client that read it.
#[derive(Debug, Clone, PartialEq)]
pub struct TokscaleSession {
	pub client: String,
	pub totals: SessionTotals,
}

/// The ledger's totals for a session, with the harness that wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerSession {
	pub harness: LedgerHarness,
	pub totals: SessionTotals,
}

/// JavaScript's `String(value)` for a client name.
fn client_name(value: Option<&Value>) -> String {
	match value {
		None | Some(Value::Null) => String::new(),
		Some(Value::String(text)) => text.clone(),
		Some(other) => other.to_string(),
	}
}

/// `models --json --group-by client,session,model` summed per session over its models, in first-seen order. opencode's
/// reasoning tokens join its output, as the ledger counts them; Claude Code's and omp's output already holds them.
pub fn tokscale_totals(json: &str) -> Result<Vec<(String, TokscaleSession)>> {
	let parsed: Value = serde_json::from_str(json)?;
	let mut totals: Vec<(String, TokscaleSession)> = Vec::new();
	let num = |entry: &Value, field: &str| entry.get(field).and_then(Value::as_f64).unwrap_or(0.0);
	for entry in parsed.get("entries").and_then(Value::as_array).into_iter().flatten() {
		let Some(session) = entry.get("sessionId").and_then(Value::as_str) else { continue };
		let client = client_name(entry.get("client"));
		let at = totals.iter().position(|(id, _)| id == session).unwrap_or_else(|| {
			totals.push((
				session.to_owned(),
				TokscaleSession { client: client.clone(), totals: SessionTotals::default() },
			));
			totals.len() - 1
		});
		let sum = &mut totals[at].1.totals;
		sum.input += num(entry, "input");
		sum.cache_read += num(entry, "cacheRead");
		sum.cache_write += num(entry, "cacheWrite");
		sum.output += num(entry, "output") + if client == "opencode" { num(entry, "reasoning") } else { 0.0 };
	}
	Ok(totals)
}

/// The ledger's totals per session id (a Claude Code subagent's transcript carries its parent's id, as in tokscale).
pub fn ledger_totals(records: &[SessionRecord]) -> Vec<(String, LedgerSession)> {
	let mut totals: Vec<(String, LedgerSession)> = Vec::new();
	for record in records {
		let at = totals.iter().position(|(id, _)| *id == record.session).unwrap_or_else(|| {
			totals.push((
				record.session.clone(),
				LedgerSession { harness: record.harness, totals: SessionTotals::default() },
			));
			totals.len() - 1
		});
		let sum = &mut totals[at].1.totals;
		for row in &record.requests {
			sum.input += row.input as f64;
			sum.cache_read += row.cache_read as f64;
			sum.cache_write += row.cache_write as f64;
			sum.output += row.output as f64;
		}
	}
	totals
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParityRow {
	pub trial: String,
	pub session: String,
	pub harness: LedgerHarness,
	pub ledger: SessionTotals,
	/// tokscale's totals for the session; `None` when it doesn't list it.
	pub tokscale: Option<SessionTotals>,
	/// The fields that differ, or `missing in tokscale`.
	pub differences: Vec<String>,
}

/// One row per ledger session: tokscale's session whose id is the ledger's or contains it (omp's file names carry the
/// session id after a timestamp), and the token fields that differ.
pub fn compare_totals(
	trial: &str,
	ledger: &[(String, LedgerSession)],
	tokscale: &[(String, SessionTotals)],
) -> Vec<ParityRow> {
	ledger
		.iter()
		.map(|(session, ours)| {
			let theirs = tokscale
				.iter()
				.find(|(id, _)| id == session)
				.or_else(|| tokscale.iter().find(|(id, _)| id.contains(session.as_str())))
				.map(|(_, totals)| *totals);
			let differences = match &theirs {
				None => vec!["missing in tokscale".to_owned()],
				Some(theirs) => [
					("input", theirs.input != ours.totals.input),
					("cacheRead", theirs.cache_read != ours.totals.cache_read),
					("cacheWrite", theirs.cache_write != ours.totals.cache_write),
					("output", theirs.output != ours.totals.output),
				]
				.into_iter()
				.filter(|(_, differs)| *differs)
				.map(|(field, _)| field.to_owned())
				.collect(),
			};
			ParityRow {
				trial: trial.to_owned(),
				session: session.clone(),
				harness: ours.harness,
				ledger: ours.totals,
				tokscale: theirs,
				differences,
			}
		})
		.collect()
}

/// Windows caps an environment block's variable at 32,767 characters; folders go to tokscale in runs under this.
const EXTRA_DIRS_LIMIT: usize = 24_000;

/// `tokscale models --json --group-by client,session,model` with `extra_dirs` added, in a throwaway config folder.
pub fn run_tokscale(extra_dirs: &[String]) -> Result<String> {
	let tokscale = on_path("tokscale", None).ok_or_else(|| BenchUsageError::err(tokscale_missing()))?;
	let config = mkdtemp("tokscale-")?;
	let mut vars: HashMap<String, String> = env::vars()
		.into_iter()
		.map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
		.collect();
	vars.insert("TOKSCALE_CONFIG_DIR".to_owned(), config.to_string_lossy().into_owned());
	vars.insert("TOKSCALE_EXTRA_DIRS".to_owned(), extra_dirs.join(","));
	let program = tokscale.to_string_lossy().into_owned();
	let options = RunOptions {
		env: Some(vars),
		max_bytes: Some(256 * 1024 * 1024),
		timeout_ms: Some(600_000),
		..RunOptions::default()
	};
	let result =
		run_program_sync(&[program.as_str(), "models", "--json", "--group-by", "client,session,model"], options);
	remove_tree(&config)?;
	let result = result?;
	if result.code != Some(0) {
		return Err(Error::other(format!("tokscale models failed: {}", result.stderr)));
	}
	Ok(result.stdout)
}

/// tokscale's totals over every run of `omp:<dir>` folders that fits [`EXTRA_DIRS_LIMIT`]; one run without any.
pub fn tokscale_for(omp_dirs: &[String]) -> Result<Vec<(String, SessionTotals)>> {
	let mut batches: Vec<Vec<String>> = vec![Vec::new()];
	for dir in omp_dirs {
		let entry = format!("omp:{dir}");
		let batch = batches.last_mut().expect("one batch at least");
		if !batch.is_empty() && batch.join(",").len() + entry.len() + 1 > EXTRA_DIRS_LIMIT {
			batches.push(vec![entry]);
		} else {
			batch.push(entry);
		}
	}
	let mut totals: Vec<(String, SessionTotals)> = Vec::new();
	for batch in &batches {
		for (id, sum) in tokscale_totals(&run_tokscale(batch)?)? {
			match totals.iter_mut().find(|(known, _)| *known == id) {
				Some(slot) => slot.1 = sum.totals,
				None => totals.push((id, sum.totals)),
			}
		}
	}
	Ok(totals)
}

/// A trial's key in `results.jsonl`: `<task>-<arm>-<rep>`.
fn trial_key(row: &Value) -> String {
	let part = |field: &str| match row.get(field) {
		Some(Value::String(text)) => text.clone(),
		Some(Value::Null) | None => "undefined".to_owned(),
		Some(other) => other.to_string(),
	};
	format!("{}-{}-{}", part("task"), part("arm"), part("rep"))
}

/// Each trial's harness from the task benchmark's `results.jsonl`, in file order.
fn read_harnesses(file: &Path) -> Result<Vec<(String, LedgerHarness)>> {
	let text = String::from_utf8_lossy(&fs::read(file)?).into_owned();
	let mut harnesses: Vec<(String, LedgerHarness)> = Vec::new();
	for line in text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)) {
		if line.trim().is_empty() {
			continue;
		}
		let row: Value = serde_json::from_str(line)?;
		let harness: LedgerHarness = serde_json::from_value(row.get("harness").cloned().unwrap_or(Value::Null))
			.map_err(|_| {
				Error::other(format!("results.jsonl names a harness that isn't omp, claude or opencode: {line}"))
			})?;
		let key = trial_key(&row);
		match harnesses.iter_mut().find(|(known, _)| *known == key) {
			Some(slot) => slot.1 = harness,
			None => harnesses.push((key, harness)),
		}
	}
	Ok(harnesses)
}

/// One trial: its folder name, harness and the ledger's totals per session.
struct Trial {
	name: String,
	harness: LedgerHarness,
	totals: Vec<(String, LedgerSession)>,
}

fn harness_summary(rows: &[ParityRow], harness: LedgerHarness) -> Value {
	let of = |row: &&ParityRow| row.harness == harness;
	json!({
		"sessions": rows.iter().filter(of).count(),
		"mismatched": rows.iter().filter(of).filter(|row| !row.differences.is_empty()).count(),
	})
}

/// The script: `jocasta-bench tokscale-parity …`; the exit code is 1 on any mismatch.
pub fn main(argv: &[String]) -> Result<ExitCode> {
	let args = parse_bench_args(argv, &ArgSpec::new(1))?;
	if args.dry_run {
		let found = on_path("tokscale", None);
		print_plan(
			"tokscale parity",
			&[
				"per trial of the run: the ledger's per-session tokens from sessions/<trial>/".to_owned(),
				format!(
					"tokscale {TOKSCALE_VERSION} models --json --group-by client,session,model, omp's saved folders as TOKSCALE_EXTRA_DIRS"
				),
				"compare uncached input, cache read, cache write and output exactly; any mismatch exits 1, nothing compared exits 2".to_owned(),
				found.map_or_else(tokscale_missing, |path| format!("tokscale at {}", path.display())),
			],
			args.out.as_deref(),
		);
		return Ok(ExitCode::SUCCESS);
	}
	let out = args.out.as_deref().ok_or_else(|| BenchUsageError::err("--out <dir> is required (outside the repo)"))?;
	let results_file = out.join("results.jsonl");
	if !results_file.exists() {
		return Err(BenchUsageError::err(format!("no task-benchmark results at {}", results_file.display())));
	}
	let harness_of = read_harnesses(&results_file)?;
	let sessions_root = out.join("sessions");
	let names: Vec<String> = match fs::read_dir(&sessions_root) {
		Ok(entries) => entries.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect(),
		Err(_) => Vec::new(),
	};
	let prices = load_price_table()?;
	let started = conditions(&["tokscale", "omp", "claude", "opencode"]);
	let mut trials: Vec<Trial> = Vec::new();
	for name in names {
		let Some((_, harness)) = harness_of.iter().find(|(key, _)| *key == name) else { continue };
		let records = read_trial_sessions(&sessions_root.join(&name), *harness, &bench_pricing(prices))?;
		trials.push(Trial { totals: ledger_totals(&records), harness: *harness, name });
	}
	// Nothing to compare is no pass (0576): a wrong run folder or renamed session folders exit 2.
	if trials.iter().all(|trial| trial.totals.is_empty()) {
		return Err(BenchUsageError::err(format!(
			"no saved sessions to compare in {} (results.jsonl names {} trials)",
			sessions_root.display(),
			harness_of.len()
		)));
	}
	if on_path("tokscale", None).is_none() {
		return Err(BenchUsageError::err(tokscale_missing()));
	}
	let omp_dirs: Vec<String> = trials
		.iter()
		.filter(|trial| trial.harness == LedgerHarness::Omp)
		.map(|trial| sessions_root.join(&trial.name).to_string_lossy().into_owned())
		.collect();
	let tokscale = tokscale_for(&omp_dirs)?;
	let rows: Vec<ParityRow> =
		trials.iter().flat_map(|trial| compare_totals(&trial.name, &trial.totals, &tokscale)).collect();
	// Derived from the run's own files: each check rewrites its rows.
	write_rows(out, "tokscale-parity", &rows)?;
	let mismatched = rows.iter().filter(|row| !row.differences.is_empty()).count();
	let file = write_json(
		out,
		"tokscale-parity",
		&json!({
			"check": "0538 tokscale parity",
			"tokscaleVersion": TOKSCALE_VERSION,
			"conditions": started.finished(),
			"sessions": rows.len(),
			"mismatched": mismatched,
			"byHarness": {
				"omp": harness_summary(&rows, LedgerHarness::Omp),
				"claude": harness_summary(&rows, LedgerHarness::Claude),
				"opencode": harness_summary(&rows, LedgerHarness::Opencode),
			},
		}),
	)?;
	println!("{}/{} sessions match tokscale; wrote {}", rows.len() - mismatched, rows.len(), file.display());
	Ok(if mismatched > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

#[cfg(test)]
#[path = "tokscale_parity_tests.rs"]
mod tests;
