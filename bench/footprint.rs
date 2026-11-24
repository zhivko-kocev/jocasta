//! The footprint budget (0674, 0676): Jocasta's own weight, measured on the release build and held to a budget that
//! only goes down.
//!
//!   jocasta-bench footprint [--write-budget]
//!
//! It builds a throwaway sandbox (its own data folder, config folder and herdr session), registers four projects,
//! puts a stub `omp` first on PATH (this binary under the name `omp`, which idles and answers `usage` and `config`
//! with nothing), starts the release `jocasta` as its server (`JOCASTA_SERVICE=1 jocasta server`, no client window),
//! opens the four projects' workspaces so their panes start, waits 60 s idle and prints one JSON object:
//! `processes` and `idle_rss_mb` (Jocasta's own tree, without the agents), `idle_cpu_pct` (that tree's CPU over the
//! idle minute, in percent of one core), `startup_ms` (the server's start to `/health` answering), `binary_mb`, and
//! `bridge_ms` (null until the bridge exists, 0675). Processes named `omp` and their trees are listed under `omp` and
//! never budgeted. The client (bare `jocasta`'s window) is not measured.
//! It measures three times, each in a fresh sandbox. `--write-budget` writes each number's worst run to
//! `bench/footprint-budget.json`; without it each number's median more than 5% over its budget prints
//! `footprint: <key> <value> over budget <budget>` and exits 1 (a null budget is skipped).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::lib::proctree::{ProcessInfo, snapshot_processes, tree_of};
use crate::lib::sandbox::require_sandbox;
use crate::lib::temp::mkdtemp;
use crate::lib::timing::{WaitOptions, wait_for};
use jocasta::frontier::server::herdr::{HerdrOptions, connect_herdr};
use jocasta::frontier::server::launch::{ServiceState, ask_shutdown, service_state};
use jocasta::frontier::server::relay::{RELAY_PORT, relay_url};
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::proctree::{kill_tree, pid_alive};
use jocasta::shared::service_file::read_service_file;

const BUDGET_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/footprint-budget.json");
const RUNS: usize = 3;
const PROJECTS: usize = 4;
const IDLE: Duration = Duration::from_secs(60);
const SLACK: f64 = 1.05;
const MIB: f64 = 1024.0 * 1024.0;
/// Set on the inner run, which finds the sandbox already in its environment.
const INNER_ENV: &str = "JOCASTA_FOOTPRINT_SANDBOX";

/// The stub agent: this binary started as `omp`. `usage` and `config` answer nothing; anything else idles until killed.
pub fn stub_agent() -> Option<ExitCode> {
	let exe = std::env::current_exe().ok()?;
	if !exe.file_stem().is_some_and(|stem| stem.eq_ignore_ascii_case("omp")) {
		return None;
	}
	let args: Vec<String> = std::env::args().skip(1).collect();
	match args.first().map(String::as_str) {
		Some("--version") => println!("19.1.1"),
		Some("usage") => println!("{{}}"),
		// `config get <key> --json` answers `{"value": …}`; the service's relay check reads `collab.relayUrl` (0561).
		Some("config") => {
			let value = if args.iter().any(|arg| arg == "collab.relayUrl") {
				json!(relay_url(RELAY_PORT))
			} else {
				json!(null)
			};
			println!("{}", json!({ "value": value }));
		}
		_ => {
			println!("footprint stub agent: idle");
			loop {
				std::thread::sleep(Duration::from_secs(3600));
			}
		}
	}
	Some(ExitCode::SUCCESS)
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
	// The service's collab relay listens on one fixed port (0561): a Jocasta already running holds it.
	if std::net::TcpListener::bind(("127.0.0.1", RELAY_PORT)).is_err() {
		return Err(Error::other(format!("port {RELAY_PORT} (the collab relay) is taken; close Jocasta first")));
	}
	let mut runs = Vec::new();
	for run in 1..=RUNS {
		let measured = one_run(&jocasta, run)?;
		println!("{}", Value::Object(measured.clone()));
		runs.push(measured.into_iter().filter(|(key, _)| key != "omp" && key != "own").collect::<Map<_, _>>());
	}
	Ok(runs)
}

fn write_budget(runs: &[Map<String, Value>]) -> Result<ExitCode> {
	let budget = combine(runs, |values| values.iter().copied().fold(f64::MIN, f64::max));
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
fn one_run(jocasta: &Path, run: usize) -> Result<Map<String, Value>> {
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
	let session = format!("footprint-{}-{run}", std::process::id());
	let sandbox_env = |command: &mut Command| {
		command
			.env("JOCASTA_DATA_DIR", &data)
			.env("JOCASTA_CODING_AGENT_DIR", &config)
			// His omp folder (sessions, the ledger's history) stays out of the numbers.
			.env("PI_CODING_AGENT_DIR", &omp_agent)
			.env("JOCASTA_HERDR_SESSION", &session)
			.env("HERDR_SESSION", &session)
			.env("PATH", &path);
	};
	for n in 1..=PROJECTS {
		let dir = projects.join(format!("acme-{n}"));
		fs::create_dir_all(&dir)?;
		fs::write(dir.join("README.md"), "# footprint fixture\n")?;
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
	// `JOCASTA_FOOTPRINT_KEEP=1` leaves the sandbox (its logs, herdr's log) for a look after a bad run.
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

/// Each measured number more than 5% over its budget, as the line that says so; a null or missing budget is skipped.
pub fn over_budget(measured: &Map<String, Value>, budget: &Map<String, Value>) -> Vec<String> {
	measured
		.iter()
		.filter_map(|(key, value)| {
			let value = value.as_f64()?;
			let limit = budget.get(key)?.as_f64()?;
			(value > limit * SLACK).then(|| format!("footprint: {key} {value} over budget {limit}"))
		})
		.collect()
}

/// The measured run, inside the sandbox's environment: start, open the projects, idle, read, stop.
async fn inner() -> Result<ExitCode> {
	// The guard the live benchmarks share: never his own data folder, herdr session or config folder.
	require_sandbox()?;
	let jocasta = PathBuf::from(std::env::var_os("JOCASTA_FOOTPRINT_EXE").unwrap_or_default());
	let session = std::env::var("HERDR_SESSION").unwrap_or_default();
	let started = Instant::now();
	let mut server = Command::new(&jocasta);
	server.arg("server").env("JOCASTA_SERVICE", "1").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		const CREATE_NO_WINDOW: u32 = 0x0800_0000;
		server.creation_flags(CREATE_NO_WINDOW);
	}
	let child = server.spawn()?;
	let up = wait_for(
		|| async {
			let file = read_service_file();
			file.is_some() && service_state(file.as_ref(), None).await == ServiceState::Up
		},
		WaitOptions { timeout_ms: 120_000, interval_ms: 25 },
	)
	.await;
	let startup_ms = started.elapsed().as_secs_f64() * 1000.0;
	let result = match up {
		Some(_) => measure(&session, startup_ms, &jocasta).await,
		None => Err(Error::other("the service didn't answer /health within 120 s")),
	};
	stop(child.id()).await;
	let row = result?;
	println!("{row}");
	Ok(ExitCode::SUCCESS)
}

async fn measure(session: &str, startup_ms: f64, jocasta: &Path) -> Result<Value> {
	let service = read_service_file().ok_or_else(|| Error::other("no service file"))?.pid;
	let herdr = connect_herdr(HerdrOptions { session: session.to_owned(), ..HerdrOptions::default() })
		.await
		.map_err(|error| Error::other(format!("herdr: {error}")))?;
	let root = PathBuf::from(std::env::var_os(INNER_ENV).unwrap_or_default());
	// A project pane comes with a thread today; here each project's workspace runs the stub agent as its command, by
	// its full path (herdr's panes on Windows don't take the service's PATH).
	let agent = vec![root.join("stub").join(format!("omp{}", std::env::consts::EXE_SUFFIX)).display().to_string()];
	for n in 1..=PROJECTS {
		let dir = root.join("projects").join(format!("acme-{n}"));
		herdr
			.ensure_workspace(&format!("acme-{n}"), &dir, Some(&agent), None)
			.await
			.map_err(|error| Error::other(format!("herdr workspace acme-{n}: {error}")))?;
	}
	herdr.close();
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
	let all = snapshot_processes()?;
	let (own, agents) = split(&tree_of(service, &all));
	let rss: u64 = own.iter().map(|info| info.working_set).sum();
	let cpu: f64 = own.iter().map(|info| after.get(info.pid) - before.get(info.pid)).sum();
	let binary = fs::metadata(jocasta)?.len();
	Ok(json!({
		"processes": own.len(),
		"idle_rss_mb": round2(rss as f64 / MIB),
		"idle_cpu_pct": round2(cpu.max(0.0) / wall * 100.0),
		"startup_ms": round2(startup_ms),
		"binary_mb": round2(binary as f64 / MIB),
		"bridge_ms": null,
		"omp": agents.iter().map(|info| json!({"name": info.name, "mb": round2(info.working_set as f64 / MIB)})).collect::<Vec<_>>(),
		"own": own.iter().map(|info| json!({"name": info.name, "mb": round2(info.working_set as f64 / MIB)})).collect::<Vec<_>>(),
	}))
}

fn round2(value: f64) -> f64 {
	(value * 100.0).round() / 100.0
}

/// A tree split into Jocasta's own processes and the agents (each process named `omp` with its whole subtree).
fn split(tree: &[ProcessInfo]) -> (Vec<ProcessInfo>, Vec<ProcessInfo>) {
	let mut agent: HashSet<u32> = HashSet::new();
	for info in tree {
		let named_omp = Path::new(&info.name).file_stem().is_some_and(|stem| stem.eq_ignore_ascii_case("omp"));
		if named_omp || agent.contains(&info.ppid) {
			agent.insert(info.pid);
		}
	}
	tree.iter().cloned().partition(|info| !agent.contains(&info.pid))
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

/// Stops the sandbox's Jocasta the product's way, then ends whatever is left of the server's tree.
async fn stop(server: u32) {
	if let Some(file) = read_service_file() {
		ask_shutdown(&file).await;
	}
	let closed =
		wait_for(|| async move { !pid_alive(server) }, WaitOptions { timeout_ms: 30_000, interval_ms: 250 }).await;
	if closed.is_none() {
		kill_tree(server).await;
	}
}

#[cfg(test)]
#[path = "footprint_tests.rs"]
mod tests;
