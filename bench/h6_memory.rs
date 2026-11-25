//! H6 (0482, 0552): RAM per pane and in total: working set and private bytes of each process tree.
//!
//!   jocasta-bench h6-memory --out <dir> [--runs 3] [--threads 0,1,4,8] [--tools omp,claude,opencode]
//!                           [--project <name>] [--idle 120] [--turn] [--cwd <repo>] [--dry-run]
//!
//! Jocasta (sandbox; `--threads` above 0 needs `--project`): for each thread count it starts that many threads as
//! `jocasta thread new` does (no Allow, 0544; each brief one short turn, so a model runs: VM only), waits `--idle`
//! seconds, then sums Jocasta's processes: the tree of the one `jocasta` process `jocasta-service.json` names (the
//! service, the shell and every pane with its omp and MCP processes, 0683) and each thread pane's tree on its own.
//! Count 0 is Jocasta and the boss alone.
//! Competitors: each tool in a pty of its own, `--idle` seconds after its prompt is ready, then with
//! `--turn` once more after one scripted prompt (a model call). A tool the VM runs unsigned (0506: opencode) gets no
//! prompt; h6-memory.json's `skipped` says why. Windows reads private bytes; POSIX only RSS.
//! Writes h6-memory.jsonl (one snapshot per row) and h6-memory.json (per condition: median and CI of each sum).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::evals::args::number_of;
use crate::lib::args::{
	ArgSpec, BenchArgs, BenchUsageError, FlagValue, flag, flag_or, list_flag, number_list_flag, parse_bench_args,
	run_script,
};
use crate::lib::jocasta::{SandboxJocasta, open_sandbox};
use crate::lib::launch::{
	BENCH_TOOLS, BenchTool, close_pty, launch, ready_patterns, require_trusted_cwd, tool_command,
};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::proctree::{Memory, snapshot_processes, tree_memory, tree_of};
use crate::lib::pty::Pty;
use crate::lib::stats::{BootstrapOptions, summarize};
use crate::lib::temp::mkdtemp;
use crate::lib::threads::{start_thread, wait_for_enter};
use jocasta::frontier::protocol::js_number_text;
use jocasta::frontier::server::surface::pane_text;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::pathkey::resolve;
use jocasta::shared::service_file::read_service_file;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneMemory {
	pub pane_id: String,
	pub working_set: u64,
	pub private_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H6Row {
	pub subject: String,
	pub condition: String,
	pub run: u64,
	#[serde(flatten)]
	pub memory: Memory,
	/// Per pane, for Jocasta's thread panes.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub per_pane: Option<Vec<PaneMemory>>,
}

const SHORT_BRIEF: &str = "Benchmark H6: reply with the single word ready and change nothing.";
const TURN_PROMPT: &str = "Reply with the single word ready.";

/// Appends the row to `h6-memory.jsonl` and keeps it for the summary.
fn record(rows: &mut Vec<H6Row>, out: &Path, row: H6Row) -> Result<()> {
	append_row(out, "h6-memory", &row)?;
	rows.push(row);
	Ok(())
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

fn is_set(args: &BenchArgs, name: &str) -> bool {
	args.flags.get(name) == Some(&FlagValue::Set)
}

/// The script's own flags.
struct H6Settings<'a> {
	thread_counts: Vec<u64>,
	tools: Vec<BenchTool>,
	idle_ms: f64,
	project: Option<&'a str>,
	turn: bool,
	/// The tools the VM runs unsigned, which get no prompt: their `skipped` entries.
	skipped: Vec<Value>,
	skip_jocasta: bool,
	ready: BTreeMap<BenchTool, String>,
	cwd: Option<&'a str>,
}

impl H6Settings<'_> {
	fn tool_names(&self) -> Vec<&'static str> {
		self.tools.iter().map(|tool| tool.as_str()).collect()
	}

	async fn idle(&self) {
		if self.idle_ms > 0.0 {
			tokio::time::sleep(Duration::from_millis(self.idle_ms as u64)).await;
		}
	}
}

fn h6_settings(args: &BenchArgs) -> Result<H6Settings<'_>> {
	let thread_counts = number_list_flag(args, "threads", &[0, 1, 4, 8])?;
	let default_tools: Vec<&str> = BENCH_TOOLS.iter().map(|tool| tool.as_str()).collect();
	let mut tools: Vec<BenchTool> = Vec::new();
	for name in list_flag(args, "tools", &default_tools) {
		match BENCH_TOOLS.into_iter().find(|tool| tool.as_str() == name) {
			Some(tool) => tools.push(tool),
			None => return Err(BenchUsageError::err(format!("unknown tool {name}"))),
		}
	}
	let turn = is_set(args, "turn");
	let skipped = if turn {
		tools
			.iter()
			.filter_map(|tool| {
				tool.unsigned()
					.map(|reason| json!({ "subject": tool.as_str(), "condition": "after one turn", "reason": reason }))
			})
			.collect()
	} else {
		Vec::new()
	};
	Ok(H6Settings {
		thread_counts,
		tools,
		idle_ms: number_of(flag_or(args, "idle", "120")) * 1000.0,
		project: flag(args, "project"),
		turn,
		skipped,
		skip_jocasta: is_set(args, "skip-jocasta"),
		ready: ready_patterns(flag(args, "ready"))?,
		cwd: flag(args, "cwd"),
	})
}

/// The `--dry-run` plan.
fn print_h6_plan(args: &BenchArgs, settings: &H6Settings) {
	let idle_seconds = settings.idle_ms / 1000.0;
	let shown = if idle_seconds.is_nan() { "NaN".to_owned() } else { js_number_text(idle_seconds) };
	let counts: Vec<String> = settings.thread_counts.iter().map(u64::to_string).collect();
	let jocasta = if settings.skip_jocasta {
		"Jocasta skipped".to_owned()
	} else {
		format!(
			"Jocasta with {} threads in {} (jocasta thread new; model turns: VM only), {shown} s idle, its one process tree summed, each thread pane alone",
			counts.join(", "),
			settings.project.unwrap_or("<--project>")
		)
	};
	let unsigned: Vec<&str> = settings.skipped.iter().filter_map(|entry| entry["subject"].as_str()).collect();
	let no_prompt = if unsigned.is_empty() {
		String::new()
	} else {
		format!(" (no prompt for {}: unsigned, 0506)", unsigned.join(", "))
	};
	print_plan(
		"H6 memory",
		&[
			jocasta,
			format!(
				"{}: one pty each, {shown} s after ready{}{no_prompt}",
				settings.tool_names().join(", "),
				if settings.turn { ", again after one scripted prompt" } else { "" },
			),
			format!("{} runs per condition; working set and private bytes (Windows) per process tree", args.runs),
		],
		args.out.as_deref(),
	);
}

/// One snapshot of Jocasta's processes: the one tree (service, shell and every pane, 0683) and each thread pane's tree.
async fn jocasta_row(jocasta: &SandboxJocasta, count: u64, run: u64) -> Result<H6Row> {
	if read_service_file().is_none() {
		return Err(Error::other("no service: <data>/jocasta-service.json is missing"));
	}
	let roots: Vec<(String, u32)> = jocasta
		.bench_panes()
		.await?
		.into_iter()
		.filter_map(|pane| Some((pane.info.pane_id, pane.root.filter(|root| *root != 0)?)))
		.collect();
	let processes = snapshot_processes()?;
	let per_pane = roots
		.iter()
		.map(|(pane_id, root)| {
			let memory = tree_memory(&tree_of(*root, &processes));
			PaneMemory {
				pane_id: pane_id.clone(),
				working_set: memory.working_set,
				private_bytes: memory.private_bytes,
			}
		})
		.collect();
	Ok(H6Row {
		subject: "jocasta".to_owned(),
		condition: format!("{count} threads"),
		run,
		memory: tree_memory(&jocasta.process_tree(&processes)),
		per_pane: Some(per_pane),
	})
}

/// Jocasta with each thread count, `runs` times.
async fn measure_jocasta(settings: &H6Settings<'_>, runs: u64, out: &Path, rows: &mut Vec<H6Row>) -> Result<()> {
	let jocasta = open_sandbox()?;
	if settings.thread_counts.iter().any(|n| *n > 0) && settings.project.is_none() {
		return Err(BenchUsageError::err("thread counts above 0 need --project"));
	}
	let mut counts = settings.thread_counts.clone();
	counts.sort_unstable();
	for run in 0..runs {
		// Stopping a thread and removing its worktree ask him first (0484), so the bench never closes threads.
		if run > 0 {
			wait_for_enter(&format!("close run {}'s h6 threads in the sandbox, then press Enter", run - 1)).await;
		}
		let mut opened = 0;
		for &count in &counts {
			while opened < count {
				let project = settings.project.expect("checked above");
				start_thread(project, &format!("h6 run {run} thread {}", opened + 1), SHORT_BRIEF).await?;
				opened += 1;
			}
			settings.idle().await;
			record(rows, out, jocasta_row(&jocasta, count, run).await?)?;
		}
	}
	Ok(())
}

/// One tool's pty: idle, then with `--turn` after one prompt.
async fn measure_pane(settings: &H6Settings<'_>, tool: BenchTool, pty: &Pty, run: u64) -> Result<Vec<H6Row>> {
	if pty.wait_for(&settings.ready[&tool], 60_000).await.is_none() {
		return Err(Error::other(format!(
			"{} never showed its ready pattern /{}/",
			tool.as_str(),
			settings.ready[&tool]
		)));
	}
	let mut rows = Vec::new();
	for condition in ["idle", "after one turn"] {
		if condition == "after one turn" {
			if !settings.turn || tool.unsigned().is_some() {
				break;
			}
			// A bracketed paste then Enter into the competitor's TUI, as typed.
			pty.send(format!("\u{1b}[200~{}\u{1b}[201~", pane_text(TURN_PROMPT)).as_bytes());
			pty.send(b"\r");
		}
		settings.idle().await;
		let Some(root) = pty.pid() else {
			return Err(Error::other(format!("no process for {}'s pty", tool.as_str())));
		};
		rows.push(H6Row {
			subject: tool.as_str().to_owned(),
			condition: condition.to_owned(),
			run,
			memory: tree_memory(&tree_of(root, &snapshot_processes()?)),
			per_pane: None,
		});
	}
	Ok(rows)
}

/// Each competitor in a pty of its own, `runs` times.
async fn measure_tools(
	settings: &H6Settings<'_>,
	cwd: &Path,
	runs: u64,
	out: &Path,
	rows: &mut Vec<H6Row>,
) -> Result<()> {
	for &tool in &settings.tools {
		for run in 0..runs {
			let pty = launch(&tool_command(tool)?, cwd, &[])?;
			let measured = measure_pane(settings, tool, &pty, run).await;
			close_pty(&pty).await;
			// Rows of a pane that failed midway were recorded before; here the whole pane's rows or its error.
			for row in measured? {
				record(rows, out, row)?;
			}
		}
	}
	Ok(())
}

/// h6-memory.json's summary: per subject and condition, the medians of each sum.
fn h6_summary(rows: &[H6Row]) -> Vec<Value> {
	let mut groups: Vec<(&str, &str)> = Vec::new();
	for row in rows {
		let key = (row.subject.as_str(), row.condition.as_str());
		if !groups.contains(&key) {
			groups.push(key);
		}
	}
	let megabytes = |bytes: u64| bytes as f64 / 1_048_576.0;
	groups
		.iter()
		.map(|(subject, condition)| {
			let of: Vec<&H6Row> =
				rows.iter().filter(|row| row.subject == *subject && row.condition == *condition).collect();
			let working: Vec<f64> = of.iter().map(|row| megabytes(row.memory.working_set)).collect();
			let privates: Vec<Option<u64>> = of.iter().map(|row| row.memory.private_bytes).collect();
			let private_mb = if privates.iter().all(Option::is_some) {
				let values: Vec<f64> = privates.iter().flatten().map(|bytes| megabytes(*bytes)).collect();
				serde_json::to_value(summarize(&values, BootstrapOptions::default())).expect("a summary serializes")
			} else {
				Value::Null
			};
			json!({
				"subject": subject,
				"condition": condition,
				"workingSetMb": summarize(&working, BootstrapOptions::default()),
				"privateMb": private_mb,
			})
		})
		.collect()
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec {
		default_runs: 3,
		valued: &["--threads", "--tools", "--project", "--idle", "--cwd", "--ready"],
		booleans: &["--turn", "--skip-jocasta"],
		..ArgSpec::default()
	};
	let args = parse_bench_args(&argv, &spec)?;
	let settings = h6_settings(&args)?;
	if args.dry_run {
		print_h6_plan(&args, &settings);
		return Ok(());
	}
	require_trusted_cwd(&settings.tool_names(), settings.cwd)?;
	let out = args.out.clone().expect("a real run has --out");
	let mut measured = vec!["omp", "claude", "opencode"];
	if !settings.skip_jocasta {
		measured.push("jocasta");
	}
	let started = conditions(&measured);
	claim_rows(&out, &["h6-memory"])?;
	let mut rows: Vec<H6Row> = Vec::new();
	if !settings.skip_jocasta {
		measure_jocasta(&settings, args.runs, &out, &mut rows).await?;
	}
	let cwd: PathBuf = match settings.cwd {
		Some(cwd) => resolve(Path::new(cwd)),
		None => {
			let dir = mkdtemp("jocasta-h6-")?;
			fs::write(dir.join("README.md"), "# h6 fixture\n")?;
			dir
		}
	};
	if !settings.tools.is_empty() {
		measure_tools(&settings, &cwd, args.runs, &out, &mut rows).await?;
	}
	let file = write_json(
		&out,
		"h6-memory",
		&json!({
			"benchmark": "H6",
			"conditions": started.finished(),
			"idleSeconds": settings.idle_ms / 1000.0,
			"summary": h6_summary(&rows),
			"skipped": settings.skipped,
		}),
	)?;
	println!("wrote {}", file.display());
	Ok(())
}
