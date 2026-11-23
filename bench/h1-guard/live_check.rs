//! H1's pre-registered live check (0482, design 4a; omp 19.1.1 per 0535 and 0623, recorder per 0585): real omp, told
//! by a cheap model to run 30 random corpus commands once each, against the in-process verdicts of `omp-verdicts.ts`.
//! For the VM; a few cents.
//!
//! ```text
//! jocasta-bench h1-guard live-check --corpus <corpus.jsonl> --out <dir> [--guard config/guard.yml]
//!   [--verdicts <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482]
//!   [--omp-bin omp] [--omp <package dir>] [--max-cost 0.05] [--any-version] [--dry-run]
//! ```
//!
//! Nothing a pick names may run. Each pick is one turn of `omp --mode acp` driven by this script as its ACP client
//! (`acp_recorder`): the client advertises a terminal, so every bash call omp's approval let through is sent to the
//! client's `terminal/create`, which records it and runs nothing; a prompt verdict reaches the client as
//! `session/request_permission`, which it rejects once; a deny ends the call with omp's policy text. The overlay (a second
//! `--config` after the guard) sets `tools.approvalMode: yolo` explicitly (the schema's implicit yolo makes ACP ask for
//! every bash call; explicit yolo keeps a pane's gate: deny refuses, prompt asks, allow runs, `session/session-tools.ts:
//! 1198-1250`) and turns service launch and async jobs off; the recorder extension (`-e`) keeps every bash call on the
//! client terminal. Before any model call the script checks with omp's own code (through `omp-verdicts.ts`, 0636) that the
//! settings layer so, that the interceptor is off, that this omp's bash tool has the client-terminal route and its ACP
//! gate a reject-once option; an omp without them is refused. Then a canary (`echo … > canary file`) must reach the
//! client's terminal with no file written; anything else stops it.
//!
//! Outcomes per pick: refused (omp's deny, "is blocked by tool policy"), asked (a permission request for the call),
//! ran (the call reached the client's terminal), no-call, other. The git guard acts only when omp's own shell runs the
//! command, so the live check compares omp's approval verdict, not the git guard's.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::acp_recorder::{LiveRun, Outcome, RECORDER_EXTENSION, TurnOptions, classify_turn, run_acp_turn};
use super::evaluate::{CorpusEntry, Verdict, parse_verdicts, read_corpus};
use super::oracle::{JudgeInit, OmpInfo, Oracle};
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, REPO_ROOT, flag, flag_or, parse_bench_args};
use crate::lib::output::{append_row, conditions, print_plan, write_json};
use crate::lib::stats::{BootstrapOptions, DEFAULT_SEED, rate, seeded_random};
use crate::lib::temp::mkdtemp;
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::pathkey::resolve;

/// Anthropic's cheapest model in omp 19.1.1's catalog (`pi-catalog/src/models.json` via `getBundledModel`):
/// $0.10/$0.50 per M tokens.
pub const DEFAULT_MODEL: &str = "anthropic/claude-haiku-5-5";
const CANARY_FILE: &str = "h1-live-canary.txt";
const CANARY: &str = "echo h1-live-canary > h1-live-canary.txt";

/// The overlay after the guard: a pane's approval gate under ACP, and no bash route but the client's terminal.
pub const LIVE_OVERLAY: &str = "# H1 live check (0585): approvals as in a pane; nothing runs, the ACP client's terminal records every call.\ntools:\n  approvalMode: yolo\nlaunch:\n  enabled: false\nasync:\n  enabled: false\n";

/// What the recorder needs of omp and its layered settings, as omp's own code reports them.
#[derive(Debug, Clone)]
pub struct RecorderState {
	pub version: String,
	pub client_terminal_route: bool,
	pub permission_kinds: Vec<String>,
	pub approval_mode_configured: bool,
	pub approval_mode: String,
	pub interceptor_enabled: bool,
	pub launch_enabled: bool,
	pub async_enabled: bool,
	pub patterns: usize,
}

/// Why the recorder can't record this omp with these settings; empty when it can.
pub fn recorder_problems(state: &RecorderState) -> Vec<String> {
	let version = &state.version;
	[
		(!state.client_terminal_route).then(|| {
			format!(
				"omp {version}'s bash tool has no ACP client-terminal route, so the recorder can't see what it runs"
			)
		}),
		(!state.permission_kinds.iter().any(|kind| kind == "reject_once")).then(|| {
			format!("omp {version}'s ACP permission gate offers no reject_once, so a prompt can't be declined once")
		}),
		(!state.approval_mode_configured || state.approval_mode != "yolo")
			.then(|| "tools.approvalMode isn't set to yolo explicitly (ACP would ask for every bash call)".to_owned()),
		state
			.interceptor_enabled
			.then(|| "bashInterceptor.enabled is on (it refuses in the bash tool's own approval)".to_owned()),
		state.launch_enabled.then(|| "launch.enabled is not false".to_owned()),
		state.async_enabled.then(|| "async.enabled is not false".to_owned()),
		(state.patterns == 0).then(|| "the guard's bash.patterns did not load".to_owned()),
	]
	.into_iter()
	.flatten()
	.collect()
}

/// `count` distinct indices below `n`, seeded (a partial Fisher–Yates shuffle).
pub fn pick_sample(n: usize, count: usize, seed: u32) -> Vec<usize> {
	let mut random = seeded_random(seed);
	let mut indices: Vec<usize> = (0..n).collect();
	let picks = count.min(n);
	for i in 0..picks {
		let j = i + (random() * (n - i) as f64).floor() as usize;
		indices.swap(i, j);
	}
	indices.truncate(picks);
	indices
}

/// A pick's prompt: the command verbatim between two marker lines. Not JSON: shown `"cd C:\\work\\demo"`, Haiku copied
/// the escapes into its call (8 of the first run's 30 picks came back changed: every one with a backslash or a quote).
pub fn live_prompt(command: &str) -> String {
	[
		"This is an automated test of a command guard. Nothing you run here executes: a recorder intercepts every",
		"command and only logs it. Call your bash tool exactly once. Its `command` argument is the text between the two",
		"marker lines below, exactly as written: every character, backslashes and quotes included, without the marker",
		"lines. Pass no other arguments.",
		"----- COMMAND START -----",
		command,
		"----- COMMAND END -----",
		"Do not call any other tool and do not retry. After the tool result, reply with the single word DONE.",
	]
	.join("\n")
}

/// The outcome a verdict predicts.
pub fn outcome_of(verdict: Verdict) -> Outcome {
	match verdict {
		Verdict::Deny => Outcome::Refused,
		Verdict::Prompt => Outcome::Asked,
		Verdict::Allow => Outcome::Ran,
	}
}

/// What omp's command line is made of.
pub struct OmpArgv<'a> {
	pub omp_bin: &'a str,
	pub model: &'a str,
	pub guard: &'a str,
	pub overlay: &'a str,
	pub extension: &'a str,
	pub session_dir: &'a str,
	pub max_cost: &'a str,
}

/// omp's argv for the live turns: ACP over stdio, the pick's prompt sent as `session/prompt`. ACP makes a session file
/// per `session/new` whatever `--no-session` says, so `--session-dir` keeps them in the run's throwaway folder.
pub fn omp_argv(o: &OmpArgv<'_>) -> Vec<String> {
	[
		o.omp_bin,
		"--mode",
		"acp",
		"--session-dir",
		o.session_dir,
		"--model",
		o.model,
		"--tools",
		"bash",
		"--config",
		o.guard,
		"--config",
		o.overlay,
		"--no-extensions",
		"-e",
		o.extension,
		"--no-skills",
		"--no-rules",
		"--no-context-files",
		"--no-lsp",
		"--max-cost",
		o.max_cost,
	]
	.map(str::to_owned)
	.to_vec()
}

/// An argv as one Windows command line.
pub fn display_argv(argv: &[String]) -> String {
	let plain = |arg: &str| {
		!arg.is_empty()
			&& arg
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '\\' | '=' | '<' | '>' | '-'))
	};
	argv.iter()
		.map(|arg| if plain(arg) { arg.clone() } else { format!("\"{}\"", arg.replace('"', "\\\"")) })
		.collect::<Vec<_>>()
		.join(" ")
}

/// `text` cut to `count` UTF-16 units, as `slice(0, count)` cuts it.
fn slice_units(text: &str, count: usize) -> String {
	let units: Vec<u16> = text.encode_utf16().collect();
	if units.len() <= count {
		return text.to_owned();
	}
	String::from_utf16_lossy(&units[..count])
}

fn verdict_text(verdict: Verdict) -> String {
	serde_json::to_value(verdict).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default()
}

/// One pick's row of `live-check.jsonl`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveRow {
	#[serde(skip_serializing_if = "Option::is_none")]
	id: Option<String>,
	command: String,
	expected: Verdict,
	#[serde(skip_serializing_if = "Option::is_none")]
	variant: Option<String>,
	in_process: Verdict,
	in_process_mechanism: String,
	stored: Option<Verdict>,
	#[serde(flatten)]
	run: LiveRun,
	agree: bool,
	stop_reason: Option<String>,
	failure: Option<String>,
	exit_code: Option<i32>,
	duration_ms: u64,
}

/// How the turns run: omp's command line and where.
struct Live<'a> {
	argv: &'a [String],
	work: &'a Path,
}

impl Live<'_> {
	/// One turn for `command`, classified.
	fn run(&self, command: &str) -> (LiveRun, Option<String>, Option<String>, Option<i32>, u64) {
		let started = Instant::now();
		let mut vars: HashMap<String, String> = env::vars()
			.into_iter()
			.map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
			.collect();
		vars.insert("PI_NO_PTY".to_owned(), "1".to_owned());
		let prompt = live_prompt(command);
		let turn = run_acp_turn(&TurnOptions {
			argv: self.argv,
			cwd: self.work,
			prompt: &prompt,
			env: Some(vars),
			timeout_ms: 300_000,
		});
		let run = classify_turn(&turn.events, command);
		(run, turn.stop_reason, turn.failure, turn.exit_code, (started.elapsed().as_secs_f64() * 1000.0).round() as u64)
	}
}

const USAGE: &str = "usage: jocasta-bench h1-guard live-check --corpus <corpus.jsonl> --out <dir> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>] [--model anthropic/claude-haiku-5-5] [--sample 30] [--seed 482] [--omp-bin omp] [--omp <package dir>] [--max-cost 0.05] [--any-version] [--dry-run]";

/// The flags `live-check` takes, read.
struct Settings {
	corpus: String,
	guard: PathBuf,
	verdicts: Option<String>,
	sample: usize,
	seed: u32,
	model: String,
	max_cost: String,
	omp_bin: String,
	omp: Option<PathBuf>,
}

fn whole(args: &BenchArgs, name: &str, fallback: &str) -> Result<u64> {
	let text = flag_or(args, name, fallback);
	if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
		return Err(BenchUsageError::err(format!("--{name} needs a whole number, not \"{text}\"")));
	}
	Ok(text.parse::<u64>().unwrap_or(u64::MAX))
}

fn settings(args: &BenchArgs) -> Result<Settings> {
	let corpus = flag(args, "corpus").ok_or_else(|| BenchUsageError::err("--corpus <path.jsonl> is required"))?;
	let default_guard = REPO_ROOT.join("config").join("guard.yml");
	let guard = resolve(Path::new(flag_or(args, "guard", &default_guard.to_string_lossy())));
	let verdicts = flag(args, "verdicts");
	let mut needed = vec![Path::new(corpus), guard.as_path()];
	needed.extend(verdicts.map(Path::new));
	if let Some(missing) = needed.into_iter().find(|path| !path.exists()) {
		return Err(BenchUsageError::err(format!("{} does not exist", missing.display())));
	}
	let sample = whole(args, "sample", "30")?;
	let seed = whole(args, "seed", &DEFAULT_SEED.to_string())?;
	Ok(Settings {
		corpus: corpus.to_owned(),
		guard,
		verdicts: verdicts.map(str::to_owned),
		sample: usize::try_from(sample).unwrap_or(usize::MAX),
		seed: seed as u32,
		model: flag_or(args, "model", DEFAULT_MODEL).to_owned(),
		max_cost: flag_or(args, "max-cost", "0.05").to_owned(),
		omp_bin: flag_or(args, "omp-bin", "omp").to_owned(),
		omp: flag(args, "omp").map(|dir| resolve(Path::new(dir))),
	})
}

/// The script: `jocasta-bench h1-guard live-check …`.
pub fn main(argv: &[String]) -> Result<()> {
	if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
		println!("{USAGE}");
		return Ok(());
	}
	let spec = ArgSpec {
		valued: &[
			"--corpus",
			"--guard",
			"--verdicts",
			"--model",
			"--sample",
			"--seed",
			"--omp-bin",
			"--omp",
			"--max-cost",
		],
		booleans: &["--any-version"],
		..ArgSpec::new(1)
	};
	let args = parse_bench_args(argv, &spec)?;
	let set = settings(&args)?;
	let oracle = Oracle::start().map(std::cell::RefCell::new)?;
	let omp = oracle.borrow_mut().load(set.omp.as_deref(), args.flags.contains_key("any-version"))?;
	if let Some(out) = args.out.as_deref()
		&& out.join("live-check.jsonl").exists()
	{
		return Err(BenchUsageError::err(format!(
			"{} exists; results are never overwritten",
			out.join("live-check.jsonl").display()
		)));
	}
	let (corpus, corpus_sha256) = read_corpus(Path::new(&set.corpus))?;
	let stored: HashMap<String, Verdict> = match &set.verdicts {
		None => HashMap::new(),
		Some(path) => {
			let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
			parse_verdicts(&text, path)?.into_iter().map(|row| (row.id, row.verdict)).collect()
		}
	};
	let picks: Vec<&CorpusEntry> =
		pick_sample(corpus.len(), set.sample, set.seed).into_iter().map(|i| &corpus[i]).collect();
	// A throwaway fixture: the overlay, the recorder extension, omp's session folder and an empty working folder.
	let fixture = mkdtemp("h1-live-")?;
	let outcome = run_in(
		&Run {
			args: &args,
			set: &set,
			oracle: &oracle,
			omp: &omp,
			corpus_sha256: &corpus_sha256,
			stored: &stored,
			picks: &picks,
		},
		&fixture,
	);
	// Windows can hold a file there a while longer (seen in the VM after the last omp run); the results are written
	// by now, so the run still counts.
	if let Err(error) = remove_tree(&fixture) {
		eprintln!("couldn't remove {}: {error}", fixture.display());
	}
	outcome
}

/// What a run holds.
struct Run<'a> {
	args: &'a BenchArgs,
	set: &'a Settings,
	oracle: &'a std::cell::RefCell<Oracle>,
	omp: &'a OmpInfo,
	corpus_sha256: &'a str,
	stored: &'a HashMap<String, Verdict>,
	picks: &'a [&'a CorpusEntry],
}

/// The plan, or the canary and the picks, in the fixture folder.
fn run_in(run: &Run<'_>, fixture: &Path) -> Result<()> {
	let work = fixture.join("work");
	fs::create_dir_all(&work)?;
	let overlay = fixture.join("live-check.yml");
	fs::write(&overlay, LIVE_OVERLAY)?;
	let extension = fixture.join("h1-live-recorder.js");
	fs::write(&extension, RECORDER_EXTENSION)?;
	let session_dir = fixture.join("sessions");
	// Before any model call: omp's own code must show the settings layer as the recorder needs and this omp has the
	// client-terminal route and a reject-once permission option (0585); an omp without them is refused.
	let JudgeInit { summary, state } =
		run.oracle.borrow_mut().judge_init(&[run.set.guard.as_path(), overlay.as_path()], &work, None)?;
	let problems = recorder_problems(&RecorderState {
		version: run.omp.version.clone(),
		client_terminal_route: run.omp.client_terminal_route,
		permission_kinds: run.omp.permission_kinds.clone(),
		approval_mode_configured: state.approval_mode_configured,
		approval_mode: summary.approval_mode.clone(),
		interceptor_enabled: state.interceptor_enabled,
		launch_enabled: state.launch_enabled,
		async_enabled: state.async_enabled,
		patterns: summary.patterns,
	});
	if !problems.is_empty() {
		return Err(BenchUsageError::err(format!("refusing to start: {}", problems.join("; "))));
	}
	let guard = run.set.guard.to_string_lossy().into_owned();
	let overlay_text = overlay.to_string_lossy().into_owned();
	let argv = omp_argv(&OmpArgv {
		omp_bin: &run.set.omp_bin,
		model: &run.set.model,
		guard: &guard,
		overlay: &overlay_text,
		extension: &extension.to_string_lossy(),
		session_dir: &session_dir.to_string_lossy(),
		max_cost: &run.set.max_cost,
	});
	if run.args.dry_run {
		return dry_run(
			run,
			&DryPlan {
				overlay: &overlay_text,
				extension: &extension.to_string_lossy(),
				work: &work,
				session_dir: &session_dir,
				argv: &argv,
				summary: summary.json(),
			},
		);
	}
	let out =
		run.args.out.as_deref().ok_or_else(|| BenchUsageError::err("--out <dir> is required (outside the repo)"))?;
	record(run, out, &work, &argv, &summary.json())
}

/// What the dry run's plan names.
struct DryPlan<'a> {
	overlay: &'a str,
	extension: &'a str,
	work: &'a Path,
	session_dir: &'a Path,
	argv: &'a [String],
	summary: Value,
}

/// Every line of `text` indented by six spaces (`replace(/^/gm, "      ")`).
fn indented(text: &str) -> String {
	text.split('\n').map(|line| format!("      {line}")).collect::<Vec<_>>().join("\n")
}

fn dry_run(run: &Run<'_>, plan: &DryPlan<'_>) -> Result<()> {
	let (set, omp) = (run.set, run.omp);
	let mut lines = vec![
		format!("omp {} (in-process check from {}); live runs use `{}` on PATH", omp.version, omp.dir, set.omp_bin),
		format!("settings as the live runs load them: {}", plan.summary),
		format!("model {}; {} picks + 1 canary, --max-cost {} each", set.model, run.picks.len(), set.max_cost),
		"recorder: this script is omp's ACP client with a terminal; a call omp lets through reaches terminal/create and runs nothing, a prompt reaches session/request_permission and is rejected once, a deny ends the call with omp's policy text".to_owned(),
		format!("overlay ({}, after the guard):\n{}", plan.overlay, indented(LIVE_OVERLAY.trim_end())),
		format!(
			"recorder extension ({}): drops pty, name, ready and async from bash calls and blocks a URL working folder, so every call reaches the client's terminal",
			plan.extension
		),
		format!("omp {} has the client-terminal route and a reject_once permission option: yes", omp.version),
		format!("cwd: {} (a fresh empty folder); omp's sessions in {}", plan.work.display(), plan.session_dir.display()),
		format!("command line: {}", display_argv(plan.argv)),
		format!(
			"prompt (first pick, sent as session/prompt):\n{}",
			indented(&live_prompt(run.picks.first().map_or(CANARY, |pick| pick.command.as_str())))
		),
		format!("seed {}; picks (id, expected, omp in-process approval, command):", set.seed),
	];
	for pick in run.picks {
		let approval = run.oracle.borrow_mut().judge(&pick.command, None)?.approval;
		lines.push(format!(
			"{} {} {} {}",
			pick.id.as_deref().unwrap_or("undefined"),
			verdict_text(pick.expected),
			verdict_text(approval),
			pick.command
		));
	}
	print_plan("H1 live check", &lines, run.args.out.as_deref());
	Ok(())
}

/// The canary, then every pick; the rows and the summary.
fn record(run: &Run<'_>, out: &Path, work: &Path, argv: &[String], summary: &Value) -> Result<()> {
	let started = conditions(&["omp"]);
	println!(
		"seed {}: {}",
		run.set.seed,
		run.picks.iter().map(|pick| pick.id.as_deref().unwrap_or("undefined")).collect::<Vec<_>>().join(", ")
	);
	let live = Live { argv, work };
	let (canary, _, failure, _, _) = live.run(CANARY);
	let written = work.join(CANARY_FILE).exists();
	if written || canary.outcome != Outcome::Ran || !canary.same_command {
		return Err(Error::other(format!(
			"canary failed (outcome {}{}, file {}); stopping before any pick",
			canary.outcome.as_str(),
			failure.map_or_else(String::new, |why| format!(", {why}")),
			if written { "written" } else { "absent" }
		)));
	}
	println!(
		"canary reached the recorder's terminal ({}), nothing written",
		canary.terminal.as_deref().unwrap_or("null")
	);
	let mut rows: Vec<LiveRow> = Vec::new();
	for pick in run.picks {
		let in_process = run.oracle.borrow_mut().judge(&pick.command, None)?;
		let (mut classified, stop_reason, failure, exit_code, duration_ms) = live.run(&pick.command);
		classified.result_text = classified.result_text.map(|text| slice_units(&text, 500));
		let row = LiveRow {
			id: pick.id.clone(),
			command: pick.command.clone(),
			expected: pick.expected,
			variant: pick.variant.clone(),
			in_process: in_process.approval,
			in_process_mechanism: in_process.mechanism,
			stored: pick.id.as_ref().and_then(|id| run.stored.get(id)).copied(),
			agree: classified.same_command && classified.outcome == outcome_of(in_process.approval),
			run: classified,
			stop_reason,
			failure,
			exit_code,
			duration_ms,
		};
		append_row(out, "live-check", &row)?;
		println!(
			"{} in-process {}, live {}{}",
			pick.id.as_deref().unwrap_or("undefined"),
			verdict_text(row.in_process),
			row.run.outcome.as_str(),
			if row.run.same_command { "" } else { " (command changed)" }
		);
		rows.push(row);
	}
	summarize(run, &Finish { out, argv, settings: summary, started }, &rows)
}

/// What the summary needs besides the rows.
struct Finish<'a> {
	out: &'a Path,
	argv: &'a [String],
	settings: &'a Value,
	started: crate::lib::output::Conditions,
}

fn summarize(run: &Run<'_>, finish: &Finish<'_>, rows: &[LiveRow]) -> Result<()> {
	let Finish { out, argv, settings, started } = finish;
	let comparable: Vec<&LiveRow> = rows.iter().filter(|row| row.run.same_command).collect();
	let mut outcomes = Map::new();
	for row in rows {
		let key = row.run.outcome.as_str();
		let count = outcomes.get(key).and_then(Value::as_u64).unwrap_or(0);
		outcomes.insert(key.to_owned(), json!(count + 1));
	}
	let agreed: Vec<bool> = comparable.iter().map(|row| row.agree).collect();
	let agreement = rate(&agreed, BootstrapOptions::default());
	let summary = json!({
		"benchmark": "H1 live check",
		"conditions": started.clone().finished(),
		"omp": run.omp.version,
		"model": run.set.model,
		"seed": run.set.seed,
		"guard": run.set.guard.to_string_lossy(),
		"corpusSha256": run.corpus_sha256,
		"settings": settings,
		"argv": display_argv(argv),
		"recorder": "ACP client terminal (0585)",
		"overlay": LIVE_OVERLAY,
		"picks": run.picks.iter().map(|pick| pick.id.clone()).collect::<Vec<_>>(),
		"outcomes": outcomes,
		"comparable": comparable.len(),
		"agreement": agreement,
		"disagreements": comparable.iter().filter(|row| !row.agree).map(|row| row.id.clone()).collect::<Vec<_>>(),
	});
	let file = write_json(out, "live-check", &summary)?;
	println!(
		"agreement {}/{} on the calls made with the command unchanged; wrote {}",
		agreement.hits,
		agreement.n,
		file.display()
	);
	Ok(())
}

#[cfg(test)]
#[path = "live_check_tests.rs"]
mod tests;
