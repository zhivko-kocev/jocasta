//! The footprint budget (0674, 0676): Jocasta's own weight, measured on the release build and held to a budget that
//! only goes down.
//!
//!   jocasta-bench footprint [--write-budget]
//!
//! It builds a throwaway sandbox (its own data folder and config folder), registers four projects,
//! puts a stub `omp` first on PATH (this binary under the name `omp`, which idles and answers `usage` and `config`
//! with nothing), starts the release `jocasta` in a pty as bare `jocasta` runs (the terminal itself, 0683),
//! starts a thread in each project so their panes start, waits 60 s idle and prints one JSON object:
//! `processes` and `idle_rss_mb` (Jocasta's own tree, without the agents), `idle_cpu_pct` (that tree's CPU over the
//! idle minute, in percent of one core), `startup_ms` (the server's start to `/health` answering, polled every 2 ms),
//! `binary_mb`, and `bridge_ms` (the median of 20 `ping` round trips over the bridge on one connection with the
//! service's token, 0675). Processes named `omp` and their trees are listed under `omp` and never budgeted; the ConPTY
//! hosts the OS runs for every terminal pane (`OpenConsole.exe`, `conhost.exe`) are left out (0689).
//! It measures three times, each in a fresh sandbox. `--write-budget` writes each number's worst run to
//! `bench/footprint-budget.json`; without it each number's median over its budget's limit prints
//! `footprint: <key> <value> over budget <budget>` and exits 1 (a null budget is skipped). The limit is 5% over the
//! budget, and for the noisy numbers (`startup_ms`, `idle_cpu_pct`, `bridge_ms`) the larger of that and an absolute
//! slack ([`LIMITS`]).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::lib::proctree::{ProcessInfo, snapshot_processes, tree_of};
use crate::lib::pty::Pty;
use crate::lib::sandbox::require_sandbox;
use crate::lib::temp::mkdtemp;
use crate::lib::threads::start_thread;
use crate::lib::timing::{WaitOptions, wait_for};
use jocasta::frontier::server::launch::{ServiceState, service_state};
use jocasta::shared::bridge_wire::CliPeer;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::proctree::kill_tree;
use jocasta::shared::service_file::read_service_file;

const BUDGET_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/footprint-budget.json");
const RUNS: usize = 3;
const PROJECTS: usize = 4;
const IDLE: Duration = Duration::from_secs(60);
/// Round trips `bridge_ms` takes the median of.
const PINGS: usize = 20;
/// How far a median may go over its budget before the check fails (0674): every number 5% (`RELATIVE`), and the three
/// noisy ones, whose few-millisecond or few-hundredths values 5% would fail on noise alone, the larger of that and an
/// absolute slack in the key's own unit. The other three have no slack. A key not in the table has none either.
const RELATIVE: f64 = 1.05;
const LIMITS: [(&str, f64); 6] = [
	("processes", 0.0),
	("idle_rss_mb", 0.0),
	("binary_mb", 0.0),
	("startup_ms", 25.0),
	("idle_cpu_pct", 1.0),
	("bridge_ms", 2.0),
];
const MIB: f64 = 1024.0 * 1024.0;
/// Set on the inner run, which finds the sandbox already in its environment.
const INNER_ENV: &str = "JOCASTA_FOOTPRINT_SANDBOX";

/// The stub agent: this binary started as `omp`. `usage` and `config` answer nothing; anything else joins the bridge as
/// a pane's omp does (a hello with the pane's key, an `ack` for every prompt) and idles until killed.
pub fn stub_agent() -> Option<ExitCode> {
	let exe = std::env::current_exe().ok()?;
	if !exe.file_stem().is_some_and(|stem| stem.eq_ignore_ascii_case("omp")) {
		return None;
	}
	let args: Vec<String> = std::env::args().skip(1).collect();
	match args.first().map(String::as_str) {
		Some("--version") => println!("19.2.0"),
		Some("usage") => println!("{{}}"),
		// `config get <key> --json` answers `{"value": …}`.
		Some("config") => println!("{}", json!({ "value": null })),
		_ => {
			println!("footprint stub agent: idle");
			// `main` runs on a runtime, so the bridge's own runtime lives on a thread of its own.
			std::thread::spawn(|| {
				if let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
					runtime.block_on(stub_bridge());
				}
			});
			loop {
				std::thread::sleep(Duration::from_secs(3600));
			}
		}
	}
	Some(ExitCode::SUCCESS)
}

/// The stub's side of the bridge: the hello a pane's omp sends, then an `ack` for every prompt, until the pipe closes.
/// Without `JOCASTA_BRIDGE` and the pane's key the stub just idles.
async fn stub_bridge() {
	use tokio::io::AsyncWriteExt;

	use jocasta::shared::bridge_wire::{BRIDGE_ENV, MAX_LINE, VERSION, connect, line_of};
	use jocasta::shared::line_rpc::LineReader;

	let (Ok(address), Ok(key)) = (std::env::var(BRIDGE_ENV), std::env::var("JOCASTA_PANE_KEY")) else { return };
	let Ok(stream) = connect(&address).await else { return };
	let (read, mut write) = tokio::io::split(stream);
	let hello = json!({
		"t": "hello", "v": VERSION, "key": key, "pid": std::process::id(), "session": null,
		"harness": "omp", "harness_version": "19.2.0", "caps": ["prompt"],
	});
	if write.write_all(line_of(&hello).as_bytes()).await.is_err() {
		return;
	}
	let mut lines = LineReader::new(read, MAX_LINE);
	while let Ok(Some(line)) = lines.next_line().await {
		let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
		if message["t"] == "prompt" {
			let ack = json!({ "t": "ack", "id": message["id"] });
			if write.write_all(line_of(&ack).as_bytes()).await.is_err() {
				return;
			}
		}
	}
}

/// `jocasta-bench footprint [--write-budget]`: in the sandbox's child, the measured run; otherwise the runs, then the
/// budget written or compared.
pub async fn main(args: Vec<String>) -> ExitCode {
	let write = args.iter().any(|arg| arg == "--write-budget");
	if let Some(unknown) = args.iter().find(|arg| *arg != "--write-budget") {
		eprintln!("usage: jocasta-bench footprint [--write-budget] (unknown {unknown})");
		return ExitCode::from(2);
	}
	let result = if std::env::var_os(INNER_ENV).is_some() {
		inner().await
	} else {
		measure_runs().and_then(|runs| if write { write_budget(&runs) } else { compare(&runs) })
	};
	match result {
		Ok(code) => code,
		Err(error) => {
			eprintln!("footprint: {error}");
			ExitCode::from(1)
		}
	}
}

/// The release `jocasta` beside this binary.
fn release_jocasta() -> Result<PathBuf> {
	let dir = std::env::current_exe()?.parent().map(Path::to_path_buf).unwrap_or_default();
	let exe = dir.join(format!("jocasta{}", std::env::consts::EXE_SUFFIX));
	if !exe.is_file() {
		return Err(Error::other(format!("{} is missing; run cargo build --release first", exe.display())));
	}
	// The shipped layout (0661): without `conpty\` beside it the pty falls back to Windows' conhost, not what ships.
	if cfg!(windows) && !dir.join("conpty").join("conpty.dll").is_file() {
		return Err(Error::other(format!(
			"{} has no conpty\\ beside it; run `mise run build` once, which lays it out there",
			exe.display()
		)));
	}
	Ok(exe)
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
	let status =
		Command::new("git").args(args).current_dir(dir).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
	if !status.success() {
		return Err(Error::other(format!("git {} failed in {}", args.join(" "), dir.display())));
	}
	Ok(())
}

/// Measures `RUNS` times, each in a fresh sandbox: `--write-budget` keeps each number's worst run as the budget, the
/// comparison takes each number's median, so one noisy sample neither sets nor breaks the budget.
fn measure_runs() -> Result<Vec<Map<String, Value>>> {
	let jocasta = release_jocasta()?;
	let mut runs = Vec::new();
	for _ in 1..=RUNS {
		let measured = one_run(&jocasta)?;
		println!("{}", Value::Object(measured.clone()));
		runs.push(measured.into_iter().filter(|(key, _)| key != "omp" && key != "own").collect::<Map<_, _>>());
	}
	Ok(runs)
}

fn write_budget(runs: &[Map<String, Value>]) -> Result<ExitCode> {
	let measured = combine(runs, |values| values.iter().copied().fold(f64::MIN, f64::max));
	// The budget only goes down (0674): each number keeps the lower of its old budget and its new worst run.
	let old: Map<String, Value> =
		fs::read_to_string(BUDGET_FILE).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
	let budget = ratchet(&measured, &old);
	// Tab-indented, as the repo's formatter (Biome) keeps JSON.
	let mut text = Vec::new();
	let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
	serde::Serialize::serialize(&budget, &mut serde_json::Serializer::with_formatter(&mut text, formatter))?;
	text.push(b'\n');
	fs::write(BUDGET_FILE, text)?;
	println!("wrote {BUDGET_FILE}");
	Ok(ExitCode::SUCCESS)
}

fn compare(runs: &[Map<String, Value>]) -> Result<ExitCode> {
	let numbers = combine(runs, |values| {
		let mut sorted = values.to_vec();
		sorted.sort_by(f64::total_cmp);
		sorted[sorted.len() / 2]
	});
	let budget: Map<String, Value> = serde_json::from_str(&fs::read_to_string(BUDGET_FILE)?)?;
	let over = over_budget(&numbers, &budget);
	for line in &over {
		println!("{line}");
	}
	Ok(if over.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

/// The new budget: per key the lower of `measured` and `old`; a key with no old number (missing or null) takes the
/// measured one.
pub fn ratchet(measured: &Map<String, Value>, old: &Map<String, Value>) -> Map<String, Value> {
	measured
		.iter()
		.map(|(key, value)| {
			let kept = match (value.as_f64(), old.get(key).and_then(Value::as_f64)) {
				(Some(new), Some(before)) => json!(new.min(before)),
				(None, Some(before)) => json!(before),
				_ => value.clone(),
			};
			(key.clone(), kept)
		})
		.collect()
}

/// Each key of the first run with `pick` over its values in every run; a key that isn't a number in every run is null.
pub fn combine(runs: &[Map<String, Value>], pick: impl Fn(&[f64]) -> f64) -> Map<String, Value> {
	let Some(first) = runs.first() else { return Map::new() };
	first
		.keys()
		.map(|key| {
			let values: Option<Vec<f64>> = runs.iter().map(|run| run.get(key).and_then(Value::as_f64)).collect();
			(key.clone(), values.map_or(Value::Null, |values| json!(pick(&values))))
		})
		.collect()
}

/// One measurement in a fresh sandbox, run in a child with the sandbox's environment so this process's own never
/// changes.
fn one_run(jocasta: &Path) -> Result<Map<String, Value>> {
	let root = mkdtemp("jocasta-footprint-")?;
	let (data, config, omp_agent, stub, projects) =
		(root.join("data"), root.join("config"), root.join("omp-agent"), root.join("stub"), root.join("projects"));
	for dir in [&data, &config, &omp_agent, &stub, &projects] {
		fs::create_dir_all(dir)?;
	}
	fs::copy(std::env::current_exe()?, stub.join(format!("omp{}", std::env::consts::EXE_SUFFIX)))?;
	let path = std::env::join_paths(
		std::iter::once(stub.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
	)
	.map_err(|error| Error::other(error.to_string()))?;
	let sandbox_env = |command: &mut Command| {
		command
			.env("JOCASTA_DATA_DIR", &data)
			.env("JOCASTA_CODING_AGENT_DIR", &config)
			// His omp folder (sessions, the ledger's history) stays out of the numbers.
			.env("PI_CODING_AGENT_DIR", &omp_agent)
			.env("PATH", &path);
	};
	for n in 1..=PROJECTS {
		let dir = projects.join(format!("acme-{n}"));
		fs::create_dir_all(&dir)?;
		// Distinct content: identical fixtures made in one second share a root commit, which is one project.
		fs::write(dir.join("README.md"), format!("# footprint fixture {n}\n"))?;
		git(&dir, &["init", "-q"])?;
		git(&dir, &["-c", "user.name=bench", "-c", "user.email=bench@example.org", "add", "."])?;
		git(&dir, &["-c", "user.name=bench", "-c", "user.email=bench@example.org", "commit", "-qm", "fixture"])?;
		let mut add = Command::new(jocasta);
		add.args(["project", "add"]).arg(&dir).stdout(Stdio::null());
		sandbox_env(&mut add);
		if !add.status()?.success() {
			return Err(Error::other(format!("jocasta project add {} failed", dir.display())));
		}
	}
	let mut inner = Command::new(std::env::current_exe()?);
	inner.arg("footprint").env(INNER_ENV, &root).env("JOCASTA_FOOTPRINT_EXE", jocasta).stderr(Stdio::inherit());
	sandbox_env(&mut inner);
	let output = inner.output()?;
	// `JOCASTA_FOOTPRINT_KEEP=1` leaves the sandbox (its logs) for a look after a bad run.
	if std::env::var_os("JOCASTA_FOOTPRINT_KEEP").is_some() {
		eprintln!("footprint: sandbox kept at {}", root.display());
	} else {
		let _ = fs::remove_dir_all(&root);
	}
	if !output.status.success() {
		return Err(Error::other("the measured run failed (its reason is above)"));
	}
	let text = String::from_utf8_lossy(&output.stdout);
	let line = text.lines().last().unwrap_or_default();
	serde_json::from_str(line).map_err(|error| Error::other(format!("no result line ({error}): {text}")))
}

/// Each measured number over its limit (5% over its budget; the noisy keys the larger of that and their absolute slack,
/// [`LIMITS`]), as the line that says so; a null or missing budget is skipped.
pub fn over_budget(measured: &Map<String, Value>, budget: &Map<String, Value>) -> Vec<String> {
	measured
		.iter()
		.filter_map(|(key, value)| {
			let value = value.as_f64()?;
			let limit = budget.get(key)?.as_f64()?;
			let slack = LIMITS.iter().find(|(name, _)| name == key).map_or(0.0, |(_, slack)| *slack);
			(value > (limit * RELATIVE).max(limit + slack))
				.then(|| format!("footprint: {key} {value} over budget {limit}"))
		})
		.collect()
}

/// The median of `PINGS` `ping` round trips over the bridge, on one connection with the service's token (0675).
async fn bridge_ms() -> Result<f64> {
	let file = read_service_file().ok_or_else(|| Error::other("no service file"))?;
	let mut peer = CliPeer::open(file.port, &file.token)
		.await
		.map_err(|error| Error::other(format!("the bridge refused the footprint's peer: {error}")))?;
	let mut times = Vec::with_capacity(PINGS);
	for _ in 0..PINGS {
		let started = Instant::now();
		let answer = peer
			.request(json!({ "t": "cli", "command": "ping", "args": {} }))
			.await
			.map_err(|error| Error::other(format!("bridge ping: {error}")))?;
		if answer.get("ok").and_then(Value::as_bool) != Some(true) {
			return Err(Error::other(format!("bridge ping answered {answer}")));
		}
		times.push(started.elapsed().as_secs_f64() * 1000.0);
	}
	times.sort_by(f64::total_cmp);
	Ok(times[times.len() / 2])
}

/// The measured run, inside the sandbox's environment: start, open the projects, idle, read, stop.
async fn inner() -> Result<ExitCode> {
	// The guard the live benchmarks share: never his own data folder or config folder.
	require_sandbox()?;
	let jocasta = PathBuf::from(std::env::var_os("JOCASTA_FOOTPRINT_EXE").unwrap_or_default());
	let started = Instant::now();
	// Bare `jocasta` is the terminal itself (0683): it runs in a pty, with the sandbox's environment this process has.
	let window = Pty::spawn(&[jocasta.display().to_string()], &std::env::temp_dir(), &[], &[])?;
	let up = wait_for(
		|| async {
			let file = read_service_file();
			file.is_some() && service_state(file.as_ref(), None).await == ServiceState::Up
		},
		WaitOptions { timeout_ms: 120_000, interval_ms: 2 },
	)
	.await;
	let startup_ms = started.elapsed().as_secs_f64() * 1000.0;
	let result = match up {
		Some(_) => measure(startup_ms, &jocasta).await,
		None => Err(Error::other("the service didn't answer /health within 120 s")),
	};
	stop(&window).await;
	let row = result?;
	println!("{row}");
	Ok(ExitCode::SUCCESS)
}

async fn measure(startup_ms: f64, jocasta: &Path) -> Result<Value> {
	let service = read_service_file().ok_or_else(|| Error::other("no service file"))?.pid;
	// Each project's thread runs the stub agent (it is first on PATH as `omp`), so the four project panes and the boss
	// each hold one, as a day's threads do.
	for n in 1..=PROJECTS {
		start_thread(&format!("acme-{n}"), &format!("footprint {n}"), "idle")
			.await
			.map_err(|error| Error::other(format!("thread in acme-{n}: {error}")))?;
	}
	// The boss and the four project panes each run the stub agent.
	let agents = wait_for(
		|| async move { snapshot_processes().is_ok_and(|all| split(&tree_of(service, &all)).1.len() > PROJECTS) },
		WaitOptions { timeout_ms: 90_000, interval_ms: 1_000 },
	)
	.await;
	if agents.is_none() {
		eprintln!("footprint: fewer than {} agents started; measuring what runs", PROJECTS + 1);
	}
	let before = cpu_seconds(service)?;
	let idle_from = Instant::now();
	tokio::time::sleep(IDLE).await;
	let after = cpu_seconds(service)?;
	let wall = idle_from.elapsed().as_secs_f64();
	let bridge = bridge_ms().await?;
	let all = snapshot_processes()?;
	let (own, agents) = split(&tree_of(service, &all));
	let rss: u64 = own.iter().map(|info| info.working_set).sum();
	let cpu_of = |info: &ProcessInfo| after.get(info.pid) - before.get(info.pid);
	let cpu: f64 = own.iter().map(cpu_of).sum();
	let binary = fs::metadata(jocasta)?.len();
	Ok(json!({
		"processes": own.len(),
		"idle_rss_mb": round2(rss as f64 / MIB),
		"idle_cpu_pct": round2(cpu.max(0.0) / wall * 100.0),
		"startup_ms": round2(startup_ms),
		"binary_mb": round2(binary as f64 / MIB),
		"bridge_ms": round2(bridge),
		"omp": agents.iter().map(|info| json!({"name": info.name, "mb": round2(info.working_set as f64 / MIB)})).collect::<Vec<_>>(),
		"own": own.iter().map(|info| json!({"name": info.name, "mb": round2(info.working_set as f64 / MIB), "cpu_pct": round2(cpu_of(info).max(0.0) / wall * 100.0)})).collect::<Vec<_>>(),
	}))
}

fn round2(value: f64) -> f64 {
	(value * 100.0).round() / 100.0
}

/// A tree split into Jocasta's own processes and the agents (each process named `omp` with its whole subtree); the
/// ConPTY hosts are in neither (0689).
fn split(tree: &[ProcessInfo]) -> (Vec<ProcessInfo>, Vec<ProcessInfo>) {
	let stem_is = |info: &ProcessInfo, names: &[&str]| {
		Path::new(&info.name)
			.file_stem()
			.and_then(|stem| stem.to_str())
			.is_some_and(|stem| names.iter().any(|name| stem.eq_ignore_ascii_case(name)))
	};
	let mut agent: HashSet<u32> = HashSet::new();
	for info in tree {
		if stem_is(info, &["omp"]) || agent.contains(&info.ppid) {
			agent.insert(info.pid);
		}
	}
	tree.iter()
		.filter(|info| !stem_is(info, &["OpenConsole", "conhost"]))
		.cloned()
		.partition(|info| !agent.contains(&info.pid))
}

/// CPU seconds per process of `root`'s tree.
struct CpuTimes(Vec<(u32, f64)>);

impl CpuTimes {
	fn get(&self, pid: u32) -> f64 {
		self.0.iter().find(|(seen, _)| *seen == pid).map_or(0.0, |(_, seconds)| *seconds)
	}
}

fn cpu_seconds(root: u32) -> Result<CpuTimes> {
	let all = snapshot_processes()?;
	let pids: HashSet<u32> = tree_of(root, &all).iter().map(|info| info.pid).collect();
	let mut times = Vec::new();
	if cfg!(windows) {
		let script = "Get-CimInstance Win32_Process | Select-Object ProcessId,KernelModeTime,UserModeTime | ConvertTo-Json -Compress";
		let output =
			Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script]).output()?;
		let rows: Value = serde_json::from_slice(&output.stdout)?;
		for row in rows.as_array().into_iter().flatten() {
			let pid = row.get("ProcessId").and_then(Value::as_u64).unwrap_or(0) as u32;
			if pids.contains(&pid) {
				let ticks = ["KernelModeTime", "UserModeTime"]
					.iter()
					.filter_map(|key| row.get(*key).and_then(Value::as_f64))
					.sum::<f64>();
				times.push((pid, ticks / 10_000_000.0));
			}
		}
	} else {
		let output = Command::new("ps").args(["-A", "-o", "pid=,time="]).output()?;
		for line in String::from_utf8_lossy(&output.stdout).lines() {
			let mut parts = line.split_whitespace();
			let (Some(pid), Some(time)) = (parts.next().and_then(|p| p.parse::<u32>().ok()), parts.next()) else {
				continue;
			};
			if pids.contains(&pid) {
				times.push((pid, ps_seconds(time)));
			}
		}
	}
	Ok(CpuTimes(times))
}

/// `ps`'s `[[dd-]hh:]mm:ss` as seconds.
fn ps_seconds(time: &str) -> f64 {
	let (days, rest) = time.split_once('-').map_or((0.0, time), |(d, r)| (d.parse().unwrap_or(0.0), r));
	let seconds = rest.split(':').fold(0.0, |sum, part| sum * 60.0 + part.parse::<f64>().unwrap_or(0.0));
	days * 86_400.0 + seconds
}

/// Closes the sandbox's Jocasta the way he does, by closing its window, then ends whatever is left of its tree.
async fn stop(window: &Pty) {
	window.close_window();
	let closed = window.wait_exit(30_000).await;
	if closed.is_none()
		&& let Some(root) = window.pid()
	{
		kill_tree(root).await;
	}
	window.end();
}

#[cfg(test)]
#[path = "footprint_tests.rs"]
mod tests;
