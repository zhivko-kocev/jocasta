//! Processes (0486): whether one lives and when it started, ending a whole process tree, and finding the processes
//! whose working folder is inside a folder before Jocasta removes it. omp's workers (the memory embedding worker,
//! language servers, MCP servers) otherwise outlive their pane and keep its worktree locked. A pane he closed in herdr
//! has its leftovers found through its omp's remembered process id and start time (0490). One windows-sys binding
//! serves every Windows read.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::shared::errors::Result;
use crate::shared::pathkey::{is_inside, real_path};
use crate::shared::poll::wait_for;

/// A process whose working folder is inside the folder asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderProcess {
	pub pid: u32,
	/// Its executable's file name (`bun.exe`, `node`), or `?` when unreadable.
	pub name: String,
	pub cwd: PathBuf,
}

/// A process as one run of it: its pid and when it started ([`process_start`]). The start compares only with other
/// starts read on this machine; a pid the OS gave again later has a later start. The start is a double, as the
/// TypeScript's number is (a Windows FILETIME is rounded to one), so a start remembered by the TypeScript build
/// compares equal to one read here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProcessId {
	pub pid: u32,
	pub start: f64,
}

/// A process in one snapshot of the process list: its parent pid, and its start when it could be read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessEntry {
	pub parent: u32,
	pub start: Option<f64>,
}

/// How long ended processes may take to go before [`end_processes`] stops waiting.
const KILL_WAIT_MS: u64 = 5_000;

/// This process and its ancestors in `parents`: never in a tree, and never walked through.
fn own_line(parents: &BTreeMap<u32, u32>) -> HashSet<u32> {
	let mut own = HashSet::from([std::process::id()]);
	let mut up = parents.get(&std::process::id()).copied();
	while let Some(pid) = up {
		if pid == 0 || !own.insert(pid) {
			break;
		}
		up = parents.get(&pid).copied();
	}
	own
}

/// Whether process `pid` is alive (a check that signals nothing; a process of another user counts).
pub fn pid_alive(pid: u32) -> bool {
	pid > 0 && os::alive(pid)
}

/// SIGKILL (`TerminateProcess` on Windows) to `pid` alone, as Node's `process.kill(pid, "SIGKILL")` does: fails when no
/// such process is alive.
pub fn kill_process(pid: u32) -> Result<()> {
	if !pid_alive(pid) {
		return Err(crate::shared::errors::Error::other(format!("kill: no process {pid}")));
	}
	os::kill(pid);
	Ok(())
}

/// FILETIME's epoch (1601-01-01) in Unix epoch milliseconds.
const FILETIME_EPOCH_MS: i64 = 11_644_473_600_000;

/// When process `pid` started, in epoch ms (Windows, from [`process_start`]'s FILETIME); `None` elsewhere or when it
/// isn't running or can't be read. Compared with a file's time, it tells a reused pid from the process that wrote it
/// (LC-4).
pub fn process_started_at(pid: u32) -> Option<i64> {
	if !cfg!(windows) {
		return None;
	}
	process_start(pid).map(|start| (start / 10_000.0).floor() as i64 - FILETIME_EPOCH_MS)
}

/// When `pid` started, or `None` when it isn't running or can't be read. Windows: `GetProcessTimes`' creation time.
/// Linux: `/proc/<pid>/stat`'s starttime. macOS: `ps -o lstart=`.
pub fn process_start(pid: u32) -> Option<f64> {
	process_starts(&[pid]).get(&pid).copied().flatten()
}

/// [`process_start`] of each of `pids` (one `ps` on macOS): `Some(None)` for one that ended (gone, a zombie, or on
/// Windows ended while a handle keeps it listed), missing when it can't be read (another user's on Windows).
fn process_starts(pids: &[u32]) -> HashMap<u32, Option<f64>> {
	let valid: Vec<u32> = pids.iter().copied().filter(|pid| *pid > 0).collect();
	if valid.is_empty() {
		return HashMap::new();
	}
	os::starts(&valid)
}

/// The processes left of `root`, a process that may have ended (0490), from one snapshot (`processes`): `root` itself
/// while it is still that same run, and every process under its pid started at or after it, each at or after its own
/// parent. When `root.pid` now belongs to another run, that run and whatever started at or after it are left out, so a
/// reused pid never counts. A process whose start is unknown is left out with what is under it; the walk never enters
/// `skip` (this process and its ancestors).
pub fn leftovers(root: ProcessId, processes: &BTreeMap<u32, ProcessEntry>, skip: &HashSet<u32>) -> Vec<u32> {
	if skip.contains(&root.pid) {
		return Vec::new();
	}
	let now = processes.get(&root.pid);
	let same = now.is_some_and(|entry| entry.start == Some(root.start));
	// The other run holding the pid now: its children can't be told from the root's but by start.
	let other = match now {
		Some(entry) if !same => match entry.start {
			Some(start) => start,
			None => return Vec::new(),
		},
		_ => f64::INFINITY,
	};
	let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
	for (pid, entry) in processes {
		if *pid != entry.parent {
			children.entry(entry.parent).or_default().push(*pid);
		}
	}
	let mut found = if same { vec![root.pid] } else { Vec::new() };
	let mut seen = HashSet::from([root.pid]);
	let mut queue = std::collections::VecDeque::from([(root.pid, root.start, other)]);
	while let Some((pid, from, before)) = queue.pop_front() {
		for child in children.get(&pid).into_iter().flatten() {
			let Some(start) = processes.get(child).and_then(|entry| entry.start) else { continue };
			if seen.contains(child) || skip.contains(child) {
				continue;
			}
			if start < from || start >= before {
				continue;
			}
			seen.insert(*child);
			found.push(*child);
			queue.push_back((*child, start, f64::INFINITY));
		}
	}
	found
}

/// One snapshot of the running processes at and under some roots' pids.
struct Snapshot {
	processes: BTreeMap<u32, ProcessEntry>,
	own: HashSet<u32>,
	parents: BTreeMap<u32, u32>,
	groups: HashMap<u32, u32>,
}

/// One snapshot of the running processes at and under `roots`' pids, each with its start (an ended one left out), this
/// process with its ancestors (never in a tree), every process's parent pid (a root's orphaned group member under the
/// root), and on POSIX each one's process group.
fn snapshot_under(roots: &[ProcessId]) -> Snapshot {
	let mut groups = HashMap::new();
	let mut parents = parent_pids(Some(&mut groups));
	let own = own_line(&parents);
	// POSIX hands a process whose parent ended to init or a subreaper, so its parent pid no longer leads back. One still
	// in a root's process group is that root's: the pane's root leads its own group, and a child stays in it unless it
	// starts one of its own (0486, 0490). Windows keeps the parent's pid and has no groups here.
	for root in roots {
		for (pid, group) in &groups {
			if *group != root.pid || *pid == root.pid || own.contains(pid) {
				continue;
			}
			let parent_group = groups.get(parents.get(pid).unwrap_or(&0));
			if parent_group != Some(&root.pid) {
				parents.insert(*pid, root.pid);
			}
		}
	}
	// The candidates by raw parent pid; `leftovers` then keeps only the edges whose starts agree.
	let mut near = BTreeSet::new();
	for root in roots {
		if own.contains(&root.pid) {
			continue;
		}
		if parents.contains_key(&root.pid) {
			near.insert(root.pid);
		}
		near.extend(descendants(root.pid, &parents, &own));
	}
	let wanted: Vec<u32> = near.iter().copied().collect();
	let starts = process_starts(&wanted);
	let mut processes = BTreeMap::new();
	for pid in near {
		match starts.get(&pid) {
			Some(None) => {}
			start => {
				processes.insert(pid, ProcessEntry { parent: parents[&pid], start: start.copied().flatten() });
			}
		}
	}
	Snapshot { processes, own, parents, groups }
}

/// [`leftovers`] of all of `roots` together, from one snapshot of the running processes.
pub fn leftovers_of(roots: &[ProcessId]) -> Vec<u32> {
	let Snapshot { processes, own, .. } = snapshot_under(roots);
	union_of(roots, &processes, &own)
}

/// Each root's [`leftovers`], every pid once.
fn union_of(roots: &[ProcessId], processes: &BTreeMap<u32, ProcessEntry>, skip: &HashSet<u32>) -> Vec<u32> {
	let mut seen = HashSet::new();
	roots.iter().flat_map(|root| leftovers(*root, processes, skip)).filter(|pid| seen.insert(*pid)).collect()
}

/// `pid` as the run it is now, or `None` when it isn't running or can't be read.
pub fn process_id_of(pid: u32) -> Option<ProcessId> {
	process_start(pid).map(|start| ProcessId { pid, start })
}

/// A thread pane's omp as Jocasta remembers it (0490), each process by pid and start: `omp` the process herdr started
/// for the pane (on Windows Bun's `omp.exe` shim) and its children (the real omp), `tree` those and everything that ran
/// under them when read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmpProcesses {
	pub omp: Vec<ProcessId>,
	pub tree: Vec<ProcessId>,
}

/// What runs now at and under each of `roots` (its [`leftovers`]), from one snapshot.
pub fn omp_processes(roots: &[ProcessId]) -> Vec<OmpProcesses> {
	let Snapshot { processes, own, .. } = snapshot_under(roots);
	roots
		.iter()
		.map(|root| {
			let tree: Vec<ProcessId> = leftovers(*root, &processes, &own)
				.into_iter()
				.map(|pid| ProcessId { pid, start: processes.get(&pid).and_then(|entry| entry.start).unwrap_or(0.0) })
				.collect();
			let omp = tree
				.iter()
				.filter(|id| id.pid == root.pid || processes.get(&id.pid).is_some_and(|entry| entry.parent == root.pid))
				.copied()
				.collect();
			OmpProcesses { omp, tree }
		})
		.collect()
}

/// Both remembered reads of one pane's omp in one, each process once.
pub fn merge_omp(a: &OmpProcesses, b: &OmpProcesses) -> OmpProcesses {
	let merge = |x: &[ProcessId], y: &[ProcessId]| {
		let mut all = x.to_vec();
		for id in y {
			if !x.iter().any(|seen| seen.pid == id.pid && seen.start == id.start) {
				all.push(*id);
			}
		}
		all
	};
	OmpProcesses { omp: merge(&a.omp, &b.omp), tree: merge(&a.tree, &b.tree) }
}

/// Whether any of `ids` still runs as that same run.
fn any_running(ids: &[ProcessId]) -> bool {
	let pids: Vec<u32> = ids.iter().map(|id| id.pid).collect();
	let starts = process_starts(&pids);
	ids.iter().any(|id| starts.get(&id.pid) == Some(&Some(id.start)))
}

/// Ends what remembered omps left (0490): waits up to `wait_ms` for each `omp` process to end (an omp on its way out
/// starts its memory workers about a second before it goes), then ends [`leftovers`] of every `tree` process at once,
/// the ones still running included. Resolves the pids found.
pub async fn end_omp_leftovers(sets: &[OmpProcesses], wait_ms: i64) -> Result<Vec<u32>> {
	let omp: Vec<ProcessId> = sets.iter().flat_map(|set| set.omp.iter().copied()).collect();
	if wait_ms > 0 {
		wait_for(|| std::future::ready(Ok(!any_running(&omp))), wait_ms, 100).await?;
	}
	let tree: Vec<ProcessId> = sets.iter().flat_map(|set| set.tree.iter().copied()).collect();
	Ok(end_processes(&tree, &[]).await)
}

/// Ends `roots` while each is still that same run, and every process under them by the start-checked walk of
/// [`leftovers`] (0490): a process counts only when it started at or after its parent, so a live process that names a
/// long-dead parent whose pid Windows gave again is never reached (SRV-A-1; `taskkill /T` walks raw parent pids, so it
/// isn't used). The walk is taken again until nothing is left or 5 s pass: children started meanwhile, and the orphans
/// of a process that ended first (each process found stays a root by its own pid and start). A root whose pid is still
/// listed while its start can't be read (seen under load as that process was ending) hides that root's orphans, so the
/// walk isn't done while one is. Each process found gets SIGKILL (TerminateProcess on Windows); on POSIX a found group
/// leader's whole group too, unless a spared process or one under it is in that group. `spare` (a process and what runs
/// under it) is never entered or signalled while it is still that same run: the herdr server a hung service started
/// (0582). Never this process or its ancestors. Resolves the pids found.
pub async fn end_processes(roots: &[ProcessId], spare: &[ProcessId]) -> Vec<u32> {
	let mut all: Vec<ProcessId> = Vec::new();
	let add = |all: &mut Vec<ProcessId>, id: ProcessId| {
		if !all.iter().any(|seen| seen.pid == id.pid && seen.start == id.start) {
			all.push(id);
		}
	};
	for id in roots.iter().filter(|id| id.pid > 0) {
		add(&mut all, *id);
	}
	if all.is_empty() {
		return Vec::new();
	}
	let spared: Vec<u32> = spare.iter().filter(|id| process_start(id.pid) == Some(id.start)).map(|id| id.pid).collect();
	let mut found: Vec<u32> = Vec::new();
	let end = Instant::now() + Duration::from_millis(KILL_WAIT_MS);
	loop {
		let Snapshot { processes, own, parents, groups } = snapshot_under(&all);
		let skip: HashSet<u32> = own.iter().chain(&spared).copied().collect();
		// The groups a spared process or one under it is in: their leaders are ended by pid alone, so a spared process
		// that shares a found leader's group (a child of it that never started its own) lives on.
		let kept: HashSet<u32> = spared
			.iter()
			.flat_map(|pid| std::iter::once(*pid).chain(descendants(*pid, &parents, &own)))
			.filter_map(|pid| groups.get(&pid).copied())
			.collect();
		let now = union_of(&all, &processes, &skip);
		let unread = all.iter().any(|root| processes.get(&root.pid).is_some_and(|holder| holder.start.is_none()));
		if (now.is_empty() && !unread) || Instant::now() >= end {
			return found;
		}
		for pid in now {
			if !found.contains(&pid) {
				found.push(pid);
			}
			let start = processes.get(&pid).and_then(|entry| entry.start).unwrap_or(0.0);
			add(&mut all, ProcessId { pid, start });
			if groups.get(&pid) == Some(&pid) && !kept.contains(&pid) {
				os::kill_group(pid);
			}
			os::kill(pid);
		}
		tokio::time::sleep(Duration::from_millis(50)).await;
	}
}

/// Ends `pid` as it runs now with every process under it ([`end_processes`]); resolves the pids found in its tree.
pub async fn kill_tree(pid: u32) -> Vec<u32> {
	match process_id_of(pid) {
		Some(id) => end_processes(&[id], &[]).await,
		None => Vec::new(),
	}
}

/// Each running process's parent pid, from one snapshot of the process list; on POSIX, `groups` gets each one's process
/// group from the same snapshot.
fn parent_pids(groups: Option<&mut HashMap<u32, u32>>) -> BTreeMap<u32, u32> {
	os::parents(groups)
}

/// Every process under `root` in `parents` (child → parent), nearest first; the walk never enters `skip`.
pub fn descendants(root: u32, parents: &BTreeMap<u32, u32>, skip: &HashSet<u32>) -> Vec<u32> {
	let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
	for (pid, ppid) in parents {
		if pid != ppid {
			children.entry(*ppid).or_default().push(*pid);
		}
	}
	let mut found = Vec::new();
	let mut seen = HashSet::from([root]);
	let mut queue = std::collections::VecDeque::from([root]);
	while let Some(next) = queue.pop_front() {
		for child in children.get(&next).into_iter().flatten() {
			if seen.contains(child) || skip.contains(child) {
				continue;
			}
			seen.insert(*child);
			found.push(*child);
			queue.push_back(*child);
		}
	}
	found
}

/// The processes whose working folder is `folder` or inside it, this process excepted. Windows: each process's current
/// directory is read from its PEB (`NtQueryInformationProcess` + `ReadProcessMemory`); a process this user can't open
/// (another user's, an elevated one) is skipped, and a folder recorded with an 8.3 short name (`C:\Users\ZHIVKO~1\…`,
/// what a short `%TEMP%` gives a child) is expanded to its long one before comparing. Linux: `/proc/<pid>/cwd`. macOS:
/// `lsof -d cwd`.
pub fn processes_in(folder: &Path) -> Vec<FolderProcess> {
	let mut roots = vec![folder.to_path_buf()];
	let real = real_path(folder);
	if real != folder {
		roots.push(real);
	}
	let inside = |cwd: &Path| {
		let at = if cfg!(windows) && cwd.to_string_lossy().contains('~') { real_path(cwd) } else { cwd.to_path_buf() };
		roots.iter().any(|root| is_inside(root, &at))
	};
	os::cwds().into_iter().filter(|p| p.pid != std::process::id() && inside(&p.cwd)).collect()
}

#[cfg(windows)]
#[path = "proctree_windows.rs"]
mod os;
#[cfg(not(windows))]
#[path = "proctree_unix.rs"]
mod os;

#[cfg(test)]
#[path = "proctree_tests.rs"]
pub(crate) mod tests;
