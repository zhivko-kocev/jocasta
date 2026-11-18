//! H5 (0482, 0576): startup time to a usable prompt, Jocasta against omp, Claude Code and OpenCode.
//!
//!   jocasta-bench h5-startup --out <dir> [--runs 20] [--tools jocasta,omp,claude,opencode] [--cwd <fixture repo>]
//!                            [--pty-session bench-pty] [--ready omp=<regex>,…] [--timeout 60] [--dry-run]
//!
//! Each tool starts `--runs` times "cold" (the first start after its processes were gone for 30 s; an OS-cold start
//! needs a reboot per run and is done by hand) and `--runs` times warm (straight after the previous run).
//!   jocasta   (sandbox: JOCASTA_DATA_DIR, JOCASTA_HERDR_SESSION, JOCASTA_CODING_AGENT_DIR) starts `jocasta` and times
//!             the boss pane's screen matching omp's ready pattern (`readyMs`, the same end point as omp alone, 0576),
//!             herdr reporting the boss's omp idle (`bossIdleMs`), and the new service answering `/health`
//!             (`serviceMs`). Between runs it closes Jocasta the product's way (Stop everything, `/api/shutdown`) and
//!             waits until `jocasta-service.json` is gone and the started surface has exited (killed after 60 s as a fallback).
//!   omp, claude, opencode start in a tab of the bench herdr session (Bun's PTY is POSIX-only) in `--cwd`, timed until
//!             the visible screen matches the tool's ready pattern (herdr `pane.wait_for_output`), then closed.
//! Writes h5-startup.jsonl (one row per start) and h5-startup.json (per tool and mode: median, p95, CI). Rows before
//! 0576 named `serviceMs` `companionMs` and had no `bossIdleMs`; `row_service_ms` reads both.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::evals::args::number_of;
use crate::h3_recovery::start_detached;
use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag, flag_or, list_flag, parse_bench_args, run_script};
use crate::lib::jocasta::{SandboxJocasta, open_sandbox};
use crate::lib::launch::{
	BENCH_TOOLS, BenchTool, LaunchOptions, bench_herdr, close_pane, launch_in_pane, ready_patterns,
	require_trusted_cwd, tool_command, wait_ready,
};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::sandbox::BENCH_PTY_SESSION;
use crate::lib::stats::{BootstrapOptions, summarize};
use crate::lib::temp::mkdtemp;
use crate::lib::timing::{WaitOptions, wait_for};
use jocasta::frontier::protocol::js_number_text;
use jocasta::frontier::server::boss::{BOSS_WORKSPACE, is_today_pane};
use jocasta::frontier::server::herdr::{HerdrOptions, PaneInfo, WorkspaceInfo, stop_herdr_session};
use jocasta::frontier::server::launch::{ServiceState, ask_shutdown};
use jocasta::shared::errors::Result;
use jocasta::shared::pathkey::resolve;
use jocasta::shared::proctree::{kill_tree, pid_alive};
use jocasta::shared::service_file::read_service_file;

const TOOLS: [&str; 4] = ["jocasta", "omp", "claude", "opencode"];

/// How long Jocasta's own close gets before H5 ends what is left (BEN-9).
const CLOSE_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H5Row {
	pub tool: String,
	pub mode: &'static str,
	pub run: usize,
	/// To the usable prompt: the tool's ready pattern on its screen; for jocasta, on the boss pane.
	pub ready_ms: Option<f64>,
	/// jocasta only: to herdr reporting the boss pane's omp idle.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub boss_idle_ms: Option<Option<f64>>,
	/// jocasta only: to the new service answering `/health`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub service_ms: Option<Option<f64>>,
	/// What rows before 0576 named `service_ms`; read from an old row, never written.
	#[serde(skip)]
	pub companion_ms: Option<f64>,
}

/// A row's service time, under its name since 0576 or the name rows had before (`companionMs`).
pub fn row_service_ms(row: &H5Row) -> Option<f64> {
	row.service_ms.flatten().or(row.companion_ms)
}

/// Cold first, then warm, for each run index: the order of starts for one tool.
pub fn schedule(runs: usize) -> Vec<(&'static str, usize)> {
	(0..runs).flat_map(|run| [("cold", run), ("warm", run)]).collect()
}

/// Closes the sandbox Jocasta as he does (Stop everything, 0549): asks the service to shut down, waits until
/// `jocasta-service.json` is gone and `surface` (the `jocasta` H5 started) has exited; past `CLOSE_MS` it ends what is
/// left and stops the session's herdr server.
async fn stop_jocasta(jocasta: &SandboxJocasta, surface: Option<u32>) {
	if let Some(running) = read_service_file()
		&& jocasta.health(Some(&running)).await == ServiceState::Up
	{
		ask_shutdown(&running).await;
	}
	let closed = wait_for(
		|| async move { read_service_file().is_none() && surface.is_none_or(|pid| !pid_alive(pid)) },
		WaitOptions { timeout_ms: CLOSE_MS, interval_ms: 250 },
	)
	.await;
	if closed.is_some() {
		return;
	}
	eprintln!("Jocasta didn't close within {} s; ending its processes", CLOSE_MS / 1000);
	if let Some(surface) = surface.filter(|pid| pid_alive(*pid)) {
		kill_tree(surface).await;
	}
	if let Some(left) = read_service_file()
		&& pid_alive(left.pid)
	{
		kill_tree(left.pid).await;
	}
	let _ =
		stop_herdr_session(&HerdrOptions { session: jocasta.sandbox.herdr_session.clone(), ..HerdrOptions::default() })
			.await;
}

/// The boss pane in herdr's lists: the pane in workspace "Jocasta" that isn't the today pane.
pub fn boss_pane<'a>(workspaces: &[WorkspaceInfo], panes: &'a [PaneInfo]) -> Option<&'a PaneInfo> {
	let home = workspaces.iter().find(|w| w.label == BOSS_WORKSPACE).map(|w| w.id.as_str())?;
	panes.iter().find(|pane| pane.workspace_id == home && !is_today_pane(pane))
}

/// How long each of H5's herdr reads may take while Jocasta starts.
const LIST_MS: i64 = 2_000;

async fn list_boss(jocasta: &SandboxJocasta) -> Option<PaneInfo> {
	let snapshot = jocasta.herdr().await.ok()?.snapshot().await.ok()?;
	boss_pane(&snapshot.workspaces, &snapshot.panes).cloned()
}

struct Started {
	surface: Option<u32>,
	ready_ms: Option<f64>,
	boss_idle_ms: Option<f64>,
	service_ms: Option<f64>,
}

async fn start_jocasta(jocasta: &SandboxJocasta, ready_pattern: &str, timeout_ms: f64) -> Result<Started> {
	let before = read_service_file().map(|file| file.pid);
	let started = Instant::now();
	let surface = start_detached("jocasta")?;
	let since = || started.elapsed().as_secs_f64() * 1000.0;
	let wait = |interval_ms: u64| WaitOptions { timeout_ms: timeout_ms as u64, interval_ms };
	let service = async {
		let up = wait_for(
			|| async {
				match read_service_file() {
					Some(running) if Some(running.pid) != before => {
						jocasta.health(Some(&running)).await == ServiceState::Up
					}
					_ => false,
				}
			},
			wait(50),
		)
		.await;
		up.map(|_| since())
	};
	let idle = async {
		let seen = wait_for(
			|| async { list_boss(jocasta).await.is_some_and(|pane| pane.agent_state.as_str() == "idle") },
			wait(100),
		)
		.await;
		seen.map(|_| since())
	};
	let ready = async {
		let pane_id: RefCell<Option<String>> = RefCell::new(None);
		wait_for(
			|| async {
				let found = list_boss(jocasta).await.map(|pane| pane.pane_id);
				let some = found.is_some();
				*pane_id.borrow_mut() = found;
				some
			},
			wait(100),
		)
		.await;
		let pane_id = pane_id.into_inner()?;
		let left = timeout_ms - since();
		if left <= 0.0 {
			return None;
		}
		wait_ready(&jocasta.sandbox.herdr_session, &pane_id, ready_pattern, left as i64).await.map(|_| since())
	};
	let (service_ms, boss_idle_ms, ready_ms) = tokio::join!(service, idle, ready);
	Ok(Started { surface: Some(surface), ready_ms, boss_idle_ms, service_ms })
}

fn times(of: &[&H5Row], pick: impl Fn(&H5Row) -> Option<f64>) -> Value {
	let values: Vec<f64> = of.iter().filter_map(|row| pick(row)).collect();
	serde_json::to_value(summarize(&values, BootstrapOptions::default())).expect("a summary serializes")
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

/// The script's own flags.
struct H5Settings<'a> {
	tools: Vec<String>,
	ready: BTreeMap<BenchTool, String>,
	pty_session: &'a str,
	timeout_ms: f64,
	cwd: Option<&'a str>,
	runs: usize,
}

fn bench_tool(tool: &str) -> Option<BenchTool> {
	BENCH_TOOLS.into_iter().find(|candidate| candidate.as_str() == tool)
}

fn h5_settings(args: &BenchArgs) -> Result<H5Settings<'_>> {
	let tools = list_flag(args, "tools", &TOOLS);
	for tool in &tools {
		if !TOOLS.contains(&tool.as_str()) {
			return Err(BenchUsageError::err(format!("unknown tool {tool}")));
		}
	}
	Ok(H5Settings {
		tools,
		ready: ready_patterns(flag(args, "ready"))?,
		pty_session: flag_or(args, "pty-session", BENCH_PTY_SESSION),
		timeout_ms: number_of(flag_or(args, "timeout", "60")) * 1000.0,
		cwd: flag(args, "cwd"),
		runs: usize::try_from(args.runs).unwrap_or(usize::MAX),
	})
}

/// The `--dry-run` plan.
fn print_h5_plan(args: &BenchArgs, settings: &H5Settings) {
	let H5Settings { tools, ready, pty_session, timeout_ms, cwd, .. } = settings;
	let shown = if timeout_ms.is_nan() { "NaN".to_owned() } else { js_number_text(timeout_ms / 1000.0) };
	let mut lines = vec![format!(
		"{} cold and {} warm starts of: {}; limit {shown} s each",
		args.runs,
		args.runs,
		tools.join(", ")
	)];
	for tool in tools {
		lines.push(if tool == "jocasta" {
			format!(
				"jocasta: start `jocasta`, time the boss pane matching /{}/ (readyMs), the boss idle (bossIdleMs) and /health (serviceMs); close it with Stop everything between runs",
				ready[&BenchTool::Omp]
			)
		} else {
			let pattern = bench_tool(tool).map_or("", |known| ready[&known].as_str());
			format!("{tool}: a tab in herdr session {pty_session}, timed to the screen matching /{pattern}/")
		});
	}
	let folder = match cwd {
		Some(cwd) => cwd,
		None if tools.iter().any(|tool| tool == "claude") => {
			"none given: Claude Code needs --cwd, a folder trusted once"
		}
		None => "a fresh temp folder with a README",
	};
	lines.push(format!("working folder: {folder}"));
	print_plan("H5 startup", &lines, args.out.as_deref());
}

/// Jocasta's starts: cold and warm, each after Jocasta was closed the product's way; appends each row.
async fn time_jocasta(
	jocasta: &SandboxJocasta,
	settings: &H5Settings<'_>,
	out: &Path,
	rows: &mut Vec<H5Row>,
) -> Result<()> {
	let mut surface: Option<u32> = None;
	for (mode, run) in schedule(settings.runs) {
		stop_jocasta(jocasta, surface).await;
		if mode == "cold" {
			tokio::time::sleep(Duration::from_secs(30)).await;
		}
		let result = start_jocasta(jocasta, &settings.ready[&BenchTool::Omp], settings.timeout_ms).await?;
		surface = result.surface;
		let row = H5Row {
			tool: "jocasta".to_owned(),
			mode,
			run,
			ready_ms: result.ready_ms,
			boss_idle_ms: Some(result.boss_idle_ms),
			service_ms: Some(result.service_ms),
			companion_ms: None,
		};
		append_row(out, "h5-startup", &row)?;
		rows.push(row);
	}
	stop_jocasta(jocasta, surface).await;
	Ok(())
}

/// A competitor's starts in a tab of the bench herdr session; appends each row.
async fn time_competitor(
	tool: &str,
	settings: &H5Settings<'_>,
	cwd: &Path,
	out: &Path,
	rows: &mut Vec<H5Row>,
) -> Result<()> {
	let session = settings.pty_session;
	let known = bench_tool(tool).expect("a listed tool is known");
	let herdr = bench_herdr(session, None).await?;
	let workspace = herdr.ensure_workspace("bench", cwd, None, None).await?;
	// Resolved before any start is timed (`tool_command`: herdr can't start npm's extensionless shim).
	let command = tool_command(known)?;
	for (mode, run) in schedule(settings.runs) {
		if mode == "cold" {
			tokio::time::sleep(Duration::from_secs(30)).await;
		}
		let t0 = Instant::now();
		let pane = launch_in_pane(&*herdr, &workspace, &command, cwd, LaunchOptions::default()).await?;
		let waited = wait_ready(session, &pane.pane_id, &settings.ready[&known], settings.timeout_ms as i64).await;
		let row = H5Row {
			tool: tool.to_owned(),
			mode,
			run,
			ready_ms: waited.map(|_| t0.elapsed().as_secs_f64() * 1000.0),
			boss_idle_ms: None,
			service_ms: None,
			companion_ms: None,
		};
		append_row(out, "h5-startup", &row)?;
		rows.push(row);
		close_pane(&*herdr, session, &pane).await;
	}
	herdr.close();
	Ok(())
}

/// h5-startup.json's summary: per tool and mode, the medians of each time.
fn h5_summary(tools: &[String], rows: &[H5Row]) -> Map<String, Value> {
	let mut summary = Map::new();
	for tool in tools {
		let mut by_mode = Map::new();
		for mode in ["cold", "warm"] {
			let of: Vec<&H5Row> = rows.iter().filter(|row| row.tool == *tool && row.mode == mode).collect();
			let mut figures = Map::new();
			figures.insert("ready".to_owned(), times(&of, |row| row.ready_ms));
			figures.insert("failed".to_owned(), json!(of.iter().filter(|row| row.ready_ms.is_none()).count()));
			if tool == "jocasta" {
				figures.insert("bossIdle".to_owned(), times(&of, |row| row.boss_idle_ms.flatten()));
				figures.insert("service".to_owned(), times(&of, row_service_ms));
			}
			by_mode.insert(mode.to_owned(), Value::Object(figures));
		}
		summary.insert(tool.clone(), Value::Object(by_mode));
	}
	summary
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec {
		default_runs: 20,
		valued: &["--tools", "--cwd", "--pty-session", "--ready", "--timeout"],
		..ArgSpec::default()
	};
	let args = parse_bench_args(&argv, &spec)?;
	let settings = h5_settings(&args)?;
	if args.dry_run {
		print_h5_plan(&args, &settings);
		return Ok(());
	}
	require_trusted_cwd(&settings.tools, settings.cwd)?;
	let out = args.out.clone().expect("a real run has --out");
	let with_jocasta = settings.tools.iter().any(|tool| tool == "jocasta");
	let jocasta = if with_jocasta { Some(open_sandbox(Some(LIST_MS))?) } else { None };
	claim_rows(&out, &["h5-startup"])?;
	let mut measured = vec!["omp", "claude", "opencode", "herdr"];
	if with_jocasta {
		measured.push("jocasta");
	}
	let started = conditions(&measured);
	let cwd: PathBuf = match settings.cwd {
		Some(cwd) => resolve(Path::new(cwd)),
		None => {
			let dir = mkdtemp("jocasta-h5-")?;
			fs::write(dir.join("README.md"), "# h5 fixture\n")?;
			dir
		}
	};
	let mut rows: Vec<H5Row> = Vec::new();
	for tool in &settings.tools {
		match (tool == "jocasta", &jocasta) {
			(true, Some(jocasta)) => time_jocasta(jocasta, &settings, &out, &mut rows).await?,
			_ => time_competitor(tool, &settings, &cwd, &out, &mut rows).await?,
		}
	}
	let patterns: Map<String, Value> =
		BENCH_TOOLS.iter().map(|tool| (tool.as_str().to_owned(), json!(settings.ready[tool]))).collect();
	let file = write_json(
		&out,
		"h5-startup",
		&json!({
			"benchmark": "H5",
			"conditions": started.finished(),
			"readyPatterns": patterns,
			"summary": h5_summary(&settings.tools, &rows),
		}),
	)?;
	println!("wrote {}", file.display());
	Ok(())
}

#[cfg(test)]
#[path = "h5_startup_tests.rs"]
mod tests;
