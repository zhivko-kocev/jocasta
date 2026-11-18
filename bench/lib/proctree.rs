//! Process trees for H3, H6 and K11: one snapshot of every process (pid, parent, name, command line, working set and
//! private bytes), a tree under a root, and the tree's memory, which the product's process code doesn't read. Windows
//! reads `Win32_Process` through PowerShell's CIM; POSIX reads `ps` (RSS only; private bytes are unknown there). Ending
//! a tree is the product's `kill_tree` (`shared::proctree`), and liveness its `pid_alive` (EVL-14).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::process::{RunOptions, run_program_sync};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
	pub pid: u32,
	pub ppid: u32,
	pub name: String,
	pub command_line: String,
	/// Bytes.
	pub working_set: u64,
	/// Bytes; `None` where the OS doesn't report them.
	pub private_bytes: Option<u64>,
}

fn powershell() -> PathBuf {
	let root = env::var("SystemRoot").unwrap_or_else(|| "C:\\Windows".to_owned());
	Path::new(&root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe")
}

/// JavaScript's `Number(value)` over a JSON value: NaN when it isn't a number.
fn number(value: Option<&Value>) -> f64 {
	match value {
		Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
		Some(Value::String(text)) => {
			if text.trim().is_empty() {
				0.0
			} else {
				text.trim().parse().unwrap_or(f64::NAN)
			}
		}
		Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
		Some(Value::Null) => 0.0,
		_ => f64::NAN,
	}
}

/// `String(value ?? "")`.
fn text(value: Option<&Value>) -> String {
	match value {
		None | Some(Value::Null) => String::new(),
		Some(Value::String(text)) => text.clone(),
		Some(other) => other.to_string(),
	}
}

/// Parses `Get-CimInstance Win32_Process | Select … | ConvertTo-Json` (an object for one process, else an array).
pub fn parse_windows_processes(json: &str) -> Result<Vec<ProcessInfo>> {
	if json.trim().is_empty() {
		return Ok(Vec::new());
	}
	let parsed: Value = serde_json::from_str(json)?;
	let rows = match parsed {
		Value::Array(rows) => rows,
		other => vec![other],
	};
	Ok(rows
		.iter()
		.map(|row| ProcessInfo {
			pid: number(row.get("ProcessId")) as u32,
			ppid: number(row.get("ParentProcessId")) as u32,
			name: text(row.get("Name")),
			command_line: text(row.get("CommandLine")),
			working_set: match row.get("WorkingSetSize") {
				None | Some(Value::Null) => 0,
				other => number(other) as u64,
			},
			// Win32_Process.PrivatePageCount is the private bytes.
			private_bytes: match row.get("PrivatePageCount") {
				None | Some(Value::Null) => None,
				other => Some(number(other) as u64),
			},
		})
		.collect())
}

static PS_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s*(.*)$").expect("a fixed pattern"));

/// Parses `ps -A -o pid=,ppid=,rss=,comm=,args=` (RSS in KiB).
pub fn parse_ps_processes(text: &str) -> Vec<ProcessInfo> {
	text.split('\n')
		.filter_map(|line| {
			let found = PS_LINE.captures(line)?;
			Some(ProcessInfo {
				pid: found[1].parse().ok()?,
				ppid: found[2].parse().ok()?,
				working_set: found[3].parse::<u64>().ok()? * 1024,
				name: found[4].to_owned(),
				command_line: found[5].to_owned(),
				private_bytes: None,
			})
		})
		.collect()
}

/// Every running process, once.
pub fn snapshot_processes() -> Result<Vec<ProcessInfo>> {
	if cfg!(windows) {
		let script = "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine,WorkingSetSize,PrivatePageCount | ConvertTo-Json -Compress";
		let argv = [
			powershell().to_string_lossy().into_owned(),
			"-NoProfile".into(),
			"-NonInteractive".into(),
			"-Command".into(),
			script.into(),
		];
		let options =
			RunOptions { timeout_ms: Some(60_000), max_bytes: Some(64 * 1024 * 1024), ..RunOptions::default() };
		let failed = |why: String| Error::other(format!("process snapshot failed: {why}"));
		let result = run_program_sync(&argv, options).map_err(|error| failed(error.to_string()))?;
		if result.code != Some(0) {
			return Err(failed(result.stderr));
		}
		return parse_windows_processes(&result.stdout);
	}
	let failed = |why: String| Error::other(format!("ps failed: {why}"));
	let result = run_program_sync(&["ps", "-A", "-o", "pid=,ppid=,rss=,comm=,args="], RunOptions::default())
		.map_err(|error| failed(error.to_string()))?;
	if result.code != Some(0) {
		return Err(failed(result.stderr));
	}
	Ok(parse_ps_processes(&result.stdout))
}

/// The root and all its descendants in `processes`, root first (empty when the root isn't there).
pub fn tree_of(root_pid: u32, processes: &[ProcessInfo]) -> Vec<ProcessInfo> {
	let mut by_parent: HashMap<u32, Vec<&ProcessInfo>> = HashMap::new();
	for info in processes.iter().filter(|info| info.pid != info.ppid) {
		by_parent.entry(info.ppid).or_default().push(info);
	}
	let Some(root) = processes.iter().find(|info| info.pid == root_pid) else { return Vec::new() };
	let mut tree = vec![root];
	let mut seen = HashSet::from([root_pid]);
	let mut at = 0;
	while at < tree.len() {
		for child in by_parent.get(&tree[at].pid).into_iter().flatten() {
			if seen.insert(child.pid) {
				tree.push(child);
			}
		}
		at += 1;
	}
	tree.into_iter().cloned().collect()
}

/// A set of processes' memory: their count, working set and private bytes in bytes (private `None` where unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
	pub processes: usize,
	pub working_set: u64,
	pub private_bytes: Option<u64>,
}

/// A tree's memory: working set and private bytes summed (private `None` when any member lacks it).
pub fn tree_memory(tree: &[ProcessInfo]) -> Memory {
	let mut private_bytes = Some(0);
	for info in tree {
		private_bytes = private_bytes.zip(info.private_bytes).map(|(sum, bytes)| sum + bytes);
	}
	Memory { processes: tree.len(), working_set: tree.iter().map(|info| info.working_set).sum(), private_bytes }
}

/// The pids of the bare `jocasta` surfaces attached to the sandbox: `<data>/surfaces/<pid>` (`attachedSurface`).
pub fn surface_pids(data_dir: &Path) -> Vec<u32> {
	let Ok(names) = fs::read_dir(data_dir.join("surfaces")) else { return Vec::new() };
	names
		.flatten()
		.filter_map(|entry| {
			let pid = number(Some(&Value::String(entry.file_name().to_string_lossy().into_owned())));
			(pid.fract() == 0.0 && pid > 0.0 && pid <= f64::from(u32::MAX)).then_some(pid as u32)
		})
		.collect()
}

/// The roots of Jocasta's process trees (0552): the service, herdr's server, and each attached surface.
#[derive(Debug, Clone, Default)]
pub struct JocastaRoots {
	pub service: Option<u32>,
	pub herdr_server: Option<u32>,
	pub surfaces: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Parts {
	pub service: Memory,
	pub herdr: Memory,
	pub surfaces: Memory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JocastaProcesses {
	pub all: Vec<ProcessInfo>,
	pub parts: Parts,
}

/// Jocasta's processes for H6 and K11 (0552's shape): the service's tree, herdr's server tree (every pane with its omp
/// and MCP processes), and each attached surface's tree (bare `jocasta` and its herdr client; the surface started the
/// service, so its tree's part leaves the service's out). Each process counts once in `all`; `parts` are disjoint.
pub fn jocasta_processes(roots: &JocastaRoots, processes: &[ProcessInfo]) -> JocastaProcesses {
	let service = roots.service.map_or_else(Vec::new, |root| tree_of(root, processes));
	let mut taken: HashSet<u32> = service.iter().map(|info| info.pid).collect();
	let herdr: Vec<ProcessInfo> = roots
		.herdr_server
		.map_or_else(Vec::new, |root| tree_of(root, processes))
		.into_iter()
		.filter(|info| !taken.contains(&info.pid))
		.collect();
	taken.extend(herdr.iter().map(|info| info.pid));
	let mut surfaces: Vec<ProcessInfo> = Vec::new();
	for root in &roots.surfaces {
		for info in tree_of(*root, processes) {
			if taken.insert(info.pid) {
				surfaces.push(info);
			}
		}
	}
	let parts = Parts { service: tree_memory(&service), herdr: tree_memory(&herdr), surfaces: tree_memory(&surfaces) };
	JocastaProcesses { all: [service, herdr, surfaces].concat(), parts }
}
