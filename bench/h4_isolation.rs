//! H4 (0482, 0502, 0576, 0603): history isolation with canary facts, in a sandbox Jocasta on his PC with two projects A
//! and B.
//!
//!   jocasta-bench h4-isolation --out <dir> --project-a <p-…> --pane-a <pane id> --pane-b <pane id>
//!                              [--canaries 50] [--runs 100] [--plant-timeout 30] [--skip-plant] [--dry-run]
//!
//! Plants `--canaries` unique facts in project A's history through the service (0576): H4 starts a thread `h4 plant` in
//! A as `jocasta thread new` does, whose brief tells it to call `report_result` once with exactly the canaries as
//! decisions; the service adds them to A's history index (0539, 0603). The thread opens in plan mode, so he approves its
//! plan. Once `jocasta thread list` shows its report, H4 issues `--runs` queries through the service's MCP endpoint
//! (`history_search`, as `jocasta mcp` calls it with the pane's key from `<data>/panes.json`) from B's pane, which
//! searches only its own project (0484), and checks every answered reply for every canary: canaries × answered queries
//! checks. The same queries from A's pane are the positive control: a bound is given only when it found canaries in
//! at least one reply, else the run is `inconclusive`.
//! Mnemopi: every other project's `<data>/projects/<id>/mnemopi.db` and its `-wal` are scanned for the canary strings
//! (one bank per project, `service.ts` `overlayText`).
//! Writes h4-isolation.json: leaks, answered checks, the positive control and the rule-of-three 95% upper bound when
//! there are none; the canaries themselves in h4-canaries.json.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::evals::args::number_of;
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, FlagValue, flag, flag_or, parse_bench_args, run_script};
use crate::lib::output::{conditions, print_plan, write_json};
use crate::lib::sandbox::{Caller, call_tool, require_sandbox};
use crate::lib::stats::{BootstrapOptions, rate, rule_of_three};
use crate::lib::threads::{list_threads, start_thread};
use crate::lib::timing::{WaitOptions, wait_for};
use jocasta::frontier::protocol::js_number_text;
use jocasta::frontier::server::boss::LaunchedPanes;
use jocasta::frontier::server::history::HISTORY_SEARCH;
use jocasta::shared::errors::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canary {
	pub token: String,
	pub topic: String,
	pub fact: String,
}

const TOPICS: [&str; 6] =
	["build server", "release train", "staging database", "deploy key", "feature flag", "test fixture"];

/// `bytes` random bytes as lower-case hex (`randomBytes(n).toString("hex")`).
pub fn random_hex(bytes: usize) -> String {
	let mut buffer = vec![0u8; bytes];
	getrandom::fill(&mut buffer).expect("the system's random source");
	buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `count` canaries: a random token each, inside a plain sentence on one of a few topics.
pub fn make_canaries(count: usize, random: &mut dyn FnMut(usize) -> String) -> Vec<Canary> {
	(0..count)
		.map(|i| {
			let token = format!("CANARY-{}", random(6).to_uppercase());
			let topic = TOPICS[i % TOPICS.len()];
			Canary {
				fact: format!("In the acme project the {topic} codename is {token}."),
				token,
				topic: topic.to_owned(),
			}
		})
		.collect()
}

/// The queries, cycling: each topic's codename question, then each canary token asked for directly.
pub fn make_queries(canaries: &[Canary], count: usize) -> Vec<String> {
	let pool: Vec<String> = TOPICS
		.iter()
		.map(|topic| format!("What is the {topic} codename?"))
		.chain(canaries.iter().map(|canary| canary.token.clone()))
		.chain(canaries.iter().map(|canary| format!("codename of the {} in acme", canary.topic)))
		.collect();
	(0..count).map(|i| pool[i % pool.len()].clone()).collect()
}

/// Which canaries a reply carries.
pub fn leaked(reply: &str, canaries: &[Canary]) -> Vec<String> {
	canaries.iter().filter(|canary| reply.contains(&canary.token)).map(|canary| canary.token.clone()).collect()
}

/// The title of the thread that plants the canaries.
pub const PLANT_TITLE: &str = "h4 plant";

/// The `h4 plant` thread's brief: one `report_result` call whose decisions are exactly the canaries, which the service
/// adds to A's history as `- <topic> codename: <token>. Why: H4 canary` lines (0539, 0603).
pub fn plant_brief(canaries: &[Canary]) -> String {
	let decisions: Vec<Value> = canaries
		.iter()
		.map(
			|canary| json!({ "question": format!("{} codename", canary.topic), "choice": canary.token, "why": "H4 canary" }),
		)
		.collect();
	[
		"Benchmark H4: change no file and run no command. Call the report_result tool exactly once, with outcome done,",
		"summary \"H4 canaries\", no files, no tests, no questions, no follow-ups, and exactly these decisions, unchanged:",
		&Value::Array(decisions).to_string(),
		"Then stop.",
	]
	.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
	pub query: String,
	pub leaked: Vec<String>,
	pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositiveControl {
	pub hits: usize,
	pub answered: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H4Score {
	pub answered_queries: usize,
	pub errored_queries: usize,
	pub checks: usize,
	pub search_leaks: usize,
	pub search_leak_rate: Option<f64>,
	/// Answered queries from A's pane whose reply carried a canary, of A's answered queries.
	pub positive_control: PositiveControl,
	/// No canary was findable from A's pane, or nothing was answered: no bound.
	pub inconclusive: bool,
	pub upper_bound95: Option<f64>,
}

/// H4's figures (0576): only answered queries count as checks, and the rule-of-three bound holds only when the positive
/// control found a canary at all and B leaked none.
pub fn score_h4(from_b: &[QueryResult], from_a: &[QueryResult], canaries: usize) -> H4Score {
	let answered_b: Vec<&QueryResult> = from_b.iter().filter(|result| !result.is_error).collect();
	let answered_a: Vec<&QueryResult> = from_a.iter().filter(|result| !result.is_error).collect();
	let checks = canaries * answered_b.len();
	let leaks: usize = answered_b.iter().map(|result| result.leaked.len()).sum();
	let hits = answered_a.iter().filter(|result| !result.leaked.is_empty()).count();
	let inconclusive = hits == 0 || checks == 0;
	H4Score {
		answered_queries: answered_b.len(),
		errored_queries: from_b.len() - answered_b.len(),
		checks,
		search_leaks: leaks,
		search_leak_rate: (checks != 0).then(|| leaks as f64 / checks as f64),
		positive_control: PositiveControl { hits, answered: answered_a.len() },
		inconclusive,
		upper_bound95: (!inconclusive && leaks == 0).then(|| rule_of_three(checks)),
	}
}

/// `toLocaleString("en-US")` of a whole number.
fn grouped(value: usize) -> String {
	let digits = value.to_string();
	let mut out = String::new();
	for (i, digit) in digits.chars().enumerate() {
		if i > 0 && (digits.len() - i).is_multiple_of(3) {
			out.push(',');
		}
		out.push(digit);
	}
	out
}

/// The run's line, as H4 prints it.
pub fn h4_line(score: &H4Score) -> String {
	let control = if score.positive_control.hits == 0 {
		"positive control found no canary: inconclusive".to_owned()
	} else {
		format!(
			"positive control found canaries in {}/{} queries",
			score.positive_control.hits, score.positive_control.answered
		)
	};
	let bound = match score.upper_bound95 {
		None => String::new(),
		Some(_) => format!("; 95% upper bound 3/{}", grouped(score.checks)),
	};
	format!(
		"{} leaks in {} answered checks ({} queries errored); {control}{bound}",
		score.search_leaks,
		grouped(score.checks),
		score.errored_queries
	)
}

/// Canary tokens in a project's mnemopi bank: `mnemopi.db` and its write-ahead log (mnemopi runs WAL).
pub fn bank_leaks(project_dir: &Path, canaries: &[Canary]) -> Vec<String> {
	let text = ["mnemopi.db", "mnemopi.db-wal"]
		.iter()
		.map(|name| project_dir.join(name))
		.filter(|path| path.exists())
		.filter_map(|path| fs::read(path).ok())
		.map(|bytes| bytes.iter().map(|&byte| char::from(byte)).collect::<String>())
		.collect::<Vec<_>>()
		.join("\n");
	canaries.iter().filter(|canary| text.contains(&canary.token)).map(|canary| canary.token.clone()).collect()
}

/// `${value}` of a JavaScript number.
fn shown(value: f64) -> String {
	if value.is_nan() { "NaN".to_owned() } else { js_number_text(value) }
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

/// The script's own flags.
struct H4Flags<'a> {
	count: usize,
	plant_minutes: f64,
	project_a: Option<&'a str>,
	pane_a: Option<&'a str>,
	pane_b: Option<&'a str>,
	skip_plant: bool,
}

fn h4_flags(args: &BenchArgs) -> Result<H4Flags<'_>> {
	let count = number_of(flag_or(args, "canaries", "50"));
	if !count.is_finite() || count.fract() != 0.0 || count < 1.0 {
		return Err(BenchUsageError::err("--canaries needs a whole number"));
	}
	Ok(H4Flags {
		count: count as usize,
		plant_minutes: number_of(flag_or(args, "plant-timeout", "30")),
		project_a: flag(args, "project-a"),
		pane_a: flag(args, "pane-a"),
		pane_b: flag(args, "pane-b"),
		skip_plant: args.flags.get("skip-plant") == Some(&FlagValue::Set),
	})
}

/// The `--dry-run` plan.
fn print_h4_plan(args: &BenchArgs, flags: &H4Flags) {
	let count = flags.count;
	let first = if flags.skip_plant {
		"reuse the canaries in <out>/h4-canaries.json".to_owned()
	} else {
		format!(
			"start the thread \"{PLANT_TITLE}\" in {} (jocasta thread new; he approves its plan) to hand in report_result with {count} canaries as decisions, which the service adds to A's history; wait up to {} min for its report",
			flags.project_a.unwrap_or("<--project-a>"),
			shown(flags.plant_minutes)
		)
	};
	print_plan(
		"H4 memory isolation",
		&[
			first,
			format!(
				"{} {HISTORY_SEARCH} queries from B's pane {} through POST /api/mcp/{HISTORY_SEARCH} (the pane's key from <data>/panes.json)",
				args.runs,
				flags.pane_b.unwrap_or("<--pane-b>")
			),
			format!(
				"check every answered reply for every canary: up to {} checks; 0 leaks → 95% upper bound 3/<answered checks>",
				shown(count as f64 * args.runs as f64)
			),
			format!(
				"positive control: the same queries from A's pane {}; no canary found there → inconclusive, no bound",
				flags.pane_a.unwrap_or("<--pane-a>")
			),
			"scan every other project's mnemopi.db and mnemopi.db-wal for the canary strings".to_owned(),
		],
		args.out.as_deref(),
	);
}

/// The canaries of an earlier run (`--skip-plant`), or new ones planted in project A through the `h4 plant` thread.
async fn canaries_for(out: &Path, flags: &H4Flags<'_>, project_a: &str) -> Result<Vec<Canary>> {
	let canary_file = out.join("h4-canaries.json");
	if flags.skip_plant {
		if !canary_file.exists() {
			return Err(BenchUsageError::err(format!(
				"--skip-plant needs {} from an earlier run",
				canary_file.display()
			)));
		}
		let mut file: Value = serde_json::from_str(&fs::read_to_string(&canary_file)?)?;
		return Ok(serde_json::from_value(file["canaries"].take())?);
	}
	if canary_file.exists() {
		return Err(BenchUsageError::err(format!("{} exists; results are never overwritten", canary_file.display())));
	}
	let canaries = make_canaries(flags.count, &mut random_hex);
	write_json(out, "h4-canaries", &json!({ "projectA": project_a, "canaries": canaries }))?;
	let branch = start_thread(project_a, PLANT_TITLE, &plant_brief(&canaries)).await?;
	println!("started \"{PLANT_TITLE}\" on {branch}; approve its plan in its pane");
	let branch = &branch;
	let reported = wait_for(
		|| async move {
			list_threads(Some(project_a))
				.await
				.map(|lines| lines.iter().any(|line| line.branch == *branch && line.reported.is_some()))
				.unwrap_or(false)
		},
		WaitOptions { timeout_ms: (flags.plant_minutes * 60_000.0) as u64, interval_ms: 10_000 },
	)
	.await;
	if reported.is_none() {
		return Err(Error::other(format!(
			"\"{PLANT_TITLE}\" handed in no report within {} min; nothing was planted",
			shown(flags.plant_minutes)
		)));
	}
	println!("planted {} canaries in {project_a}", canaries.len());
	Ok(canaries)
}

/// Each query through the service's MCP endpoint as the pane `pane_key` makes it.
async fn ask(queries: &[String], canaries: &[Canary], pane_key: &str) -> Result<Vec<QueryResult>> {
	let mut results = Vec::new();
	for query in queries {
		let mut call = Map::new();
		call.insert("query".to_owned(), json!(query));
		call.insert("limit".to_owned(), json!(20));
		let caller = Caller { pane_key: Some(pane_key.to_owned()), boss_key: None };
		let reply = call_tool(HISTORY_SEARCH, call, &caller).await?;
		results.push(QueryResult {
			query: query.clone(),
			leaked: leaked(&reply.text, canaries),
			is_error: reply.is_error == Some(true),
		});
	}
	Ok(results)
}

/// Every other project's bank, scanned for the canaries.
fn scan_banks(projects_dir: &Path, project_a: &str, canaries: &[Canary]) -> Vec<Value> {
	let Ok(entries) = fs::read_dir(projects_dir) else { return Vec::new() };
	entries
		.flatten()
		.map(|entry| entry.file_name().to_string_lossy().into_owned())
		.filter(|id| id != project_a)
		.map(|id| json!({ "project": id, "leaked": bank_leaks(&projects_dir.join(&id), canaries) }))
		.collect()
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec {
		default_runs: 100,
		valued: &["--project-a", "--pane-a", "--pane-b", "--canaries", "--wait", "--plant-timeout"],
		booleans: &["--skip-plant"],
		..ArgSpec::default()
	};
	let args = parse_bench_args(&argv, &spec)?;
	let flags = h4_flags(&args)?;
	if args.dry_run {
		print_h4_plan(&args, &flags);
		return Ok(());
	}
	let out = args.out.clone().expect("a real run has --out");
	let sandbox = require_sandbox()?;
	let (Some(project_a), Some(pane_a), Some(pane_b)) = (flags.project_a, flags.pane_a, flags.pane_b) else {
		return Err(BenchUsageError::err("--project-a, --pane-a and --pane-b are required"));
	};
	let data_dir = Path::new(&sandbox.data_dir);
	let launched = LaunchedPanes::new(Some(&data_dir.join("panes.json")));
	let key_of = |pane_id: &str| -> Result<String> {
		launched.key_of(pane_id).filter(|key| !key.is_empty()).ok_or_else(|| {
			BenchUsageError::err(format!("pane {pane_id} has no key in <data>/panes.json: pass a pane Jocasta started"))
		})
	};
	let key_a = key_of(pane_a)?;
	let key_b = key_of(pane_b)?;
	let started = conditions(&["omp", "jocasta"]);
	let canaries = canaries_for(&out, &flags, project_a).await?;
	let queries = make_queries(&canaries, usize::try_from(args.runs).unwrap_or(usize::MAX));
	let from_b = ask(&queries, &canaries, &key_b).await?;
	let from_a = ask(&queries, &canaries, &key_a).await?;
	let score = score_h4(&from_b, &from_a, canaries.len());
	let banks = scan_banks(&data_dir.join("projects"), project_a, &canaries);
	let answered_a: Vec<bool> =
		from_a.iter().filter(|result| !result.is_error).map(|result| !result.leaked.is_empty()).collect();
	let mut summary = Map::new();
	summary.insert("benchmark".to_owned(), json!("H4"));
	summary.insert("conditions".to_owned(), serde_json::to_value(started.finished())?);
	summary.insert("canaries".to_owned(), json!(canaries.len()));
	summary.insert("queries".to_owned(), json!(queries.len()));
	let Value::Object(figures) = serde_json::to_value(&score)? else { unreachable!("a struct is an object") };
	summary.extend(figures);
	summary.insert(
		"positiveControlRate".to_owned(),
		serde_json::to_value(rate(&answered_a, BootstrapOptions::default()))?,
	);
	summary.insert("mnemopiBanks".to_owned(), Value::Array(banks));
	let leaked_queries: Vec<&QueryResult> = from_b.iter().filter(|result| !result.leaked.is_empty()).collect();
	summary.insert("leakedQueries".to_owned(), serde_json::to_value(leaked_queries)?);
	let file = write_json(&out, "h4-isolation", &Value::Object(summary))?;
	println!("{}; wrote {}", h4_line(&score), file.display());
	Ok(())
}

#[cfg(test)]
#[path = "h4_isolation_tests.rs"]
mod tests;
