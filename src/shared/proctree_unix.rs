//! `proctree`'s POSIX reads and kills: `ps`, `/proc` on Linux, `lsof` on macOS, and `kill` for signals (no libc crate).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::FolderProcess;
use crate::shared::env;

/// `program args` run to its end with stdout captured (and stderr dropped), as text; empty when it can't run.
fn output_of(program: &str, args: &[&str], c_locale: bool) -> String {
	let mut command = Command::new(program);
	command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
	if c_locale {
		command.env("LC_ALL", "C");
	}
	env::apply_to(&mut command);
	command.output().map(|out| String::from_utf8_lossy(&out.stdout).into_owned()).unwrap_or_default()
}

/// Whether `pid` is alive: `/proc/<pid>` on Linux, else `kill -0` (EPERM, "not permitted", means it exists but
/// belongs to someone else).
pub fn alive(pid: u32) -> bool {
	if cfg!(target_os = "linux") {
		return Path::new(&format!("/proc/{pid}")).exists();
	}
	let mut command = Command::new("kill");
	command.args(["-0", &pid.to_string()]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
	env::apply_to(&mut command);
	command
		.output()
		.is_ok_and(|out| out.status.success() || String::from_utf8_lossy(&out.stderr).contains("not permitted"))
}

/// The start of each of `pids`: Linux reads `/proc/<pid>/stat`, macOS asks `ps` once.
pub fn starts(pids: &[u32]) -> HashMap<u32, Option<f64>> {
	if cfg!(target_os = "linux") {
		return pids.iter().map(|pid| (*pid, linux_start(*pid))).collect();
	}
	let mut starts: HashMap<u32, Option<f64>> = pids.iter().map(|pid| (*pid, None)).collect();
	let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
	for line in output_of("ps", &["-o", "pid=,lstart=", "-p", &list], true).lines() {
		let mut parts = line.split_whitespace();
		let Some(pid) = parts.next().and_then(|pid| pid.parse::<u32>().ok()) else { continue };
		let when = parts.collect::<Vec<_>>().join(" ");
		match parse_lstart(&when) {
			Some(start) => starts.insert(pid, Some(start)),
			None => starts.remove(&pid),
		};
	}
	starts
}

/// `ps -o lstart=`'s text (`Mon Nov 10 12:34:56 2026`, local time) as epoch ms, as `Date.parse` reads it.
fn parse_lstart(text: &str) -> Option<f64> {
	use chrono::TimeZone;
	let naive = chrono::NaiveDateTime::parse_from_str(text, "%a %b %d %H:%M:%S %Y").ok()?;
	let local = chrono::Local.from_local_datetime(&naive).earliest()?;
	Some(local.timestamp_millis() as f64)
}

/// `/proc/<pid>/stat`'s starttime (clock ticks): `Some(None)` for a gone, zombie or dead process.
fn linux_start(pid: u32) -> Option<f64> {
	// After `comm` (field 2, which may hold spaces): state (field 3), then starttime (field 22, clock ticks).
	let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
	let rest = stat.get(stat.rfind(')')? + 2..)?;
	let fields: Vec<&str> = rest.split(' ').collect();
	if matches!(fields.first(), Some(&"Z" | &"X")) {
		return None;
	}
	fields.get(19)?.parse().ok()
}

pub fn parents(groups: Option<&mut HashMap<u32, u32>>) -> BTreeMap<u32, u32> {
	let mut parents = BTreeMap::new();
	let mut groups = groups;
	for line in output_of("ps", &["-A", "-o", "pid=,ppid=,pgid="], false).lines() {
		let mut numbers = line.split_whitespace().map(|field| field.parse::<u32>().ok());
		let Some(Some(pid)) = numbers.next() else { continue };
		let Some(Some(ppid)) = numbers.next() else { continue };
		if pid == 0 {
			continue;
		}
		parents.insert(pid, ppid);
		if let (Some(groups), Some(Some(pgid))) = (groups.as_deref_mut(), numbers.next()) {
			groups.insert(pid, pgid);
		}
	}
	parents
}

pub fn cwds() -> Vec<FolderProcess> {
	if cfg!(target_os = "linux") { linux_cwds() } else { lsof_cwds() }
}

fn linux_cwds() -> Vec<FolderProcess> {
	let mut found = Vec::new();
	let Ok(entries) = std::fs::read_dir("/proc") else { return found };
	for entry in entries.flatten() {
		let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()).filter(|pid| *pid > 0)
		else {
			continue;
		};
		// Another user's process, or gone.
		let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) else { continue };
		let name =
			std::fs::read_to_string(format!("/proc/{pid}/comm")).map(|name| name.trim().to_owned()).unwrap_or_default();
		found.push(FolderProcess { pid, name: if name.is_empty() { "?".into() } else { name }, cwd });
	}
	found
}

/// macOS (and other BSDs): lsof's field output, `p<pid>`, `c<command>`, `n<path>` per process.
fn lsof_cwds() -> Vec<FolderProcess> {
	let mut found = Vec::new();
	let mut current: Option<(u32, String)> = None;
	for line in output_of("lsof", &["-w", "-d", "cwd", "-F", "pcn"], false).lines() {
		let field = line.get(1..).unwrap_or("");
		if line.starts_with('p') {
			current = field.parse().ok().map(|pid| (pid, "?".to_owned()));
		} else if line.starts_with('c') {
			if let Some(current) = &mut current {
				current.1 = field.to_owned();
			}
		} else if line.starts_with('n')
			&& let Some((pid, name)) = &current
		{
			found.push(FolderProcess { pid: *pid, name: name.clone(), cwd: PathBuf::from(field) });
		}
	}
	found
}

fn signal(args: &[&str]) {
	let mut command = Command::new("kill");
	command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
	env::apply_to(&mut command);
	let _ = command.status();
}

/// SIGKILL to `pid` (gone already is fine).
pub fn kill(pid: u32) {
	signal(&["-KILL", &pid.to_string()]);
}

/// SIGKILL to the whole process group led by `pid`.
pub fn kill_group(pid: u32) {
	signal(&["-KILL", "--", &format!("-{pid}")]);
}
