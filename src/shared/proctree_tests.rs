//! `proctree`'s tests, and the real process trees they and later waves' tests start. The stand-in processes are this
//! test executable run again on a test function that only acts when its environment variable says so (the TypeScript's
//! `test/frontier/server/sleepers.ts` ran Bun scripts).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::*;
use crate::shared::poll::wait_for;
use crate::shared::testing::{TempDir, temp_dir};

pub(crate) const SLEEPER_VAR: &str = "JOCASTA_TEST_SLEEPER";
const SHIM_VAR: &str = "JOCASTA_TEST_SHIM";

/// Waits until `check` holds, at most 10 s: real processes start and end on the OS clock, not a fake one.
pub(crate) async fn settles(check: impl Fn() -> bool) -> bool {
	wait_for(|| std::future::ready(Ok(check())), 10_000, 50).await.unwrap_or(false)
}

/// Starts this test executable on `test` with `var` set to `value`: a stand-in process, with `extra` after the
/// test's own arguments (the test run reads it as a filter and ignores it). `detached` makes it outlive its parent on
/// Windows (and lead its own group on POSIX); a stand-in never starts hidden, whose console host (`conhost.exe`)
/// would join the trees the tests compare.
fn spawn_self(
	test: &str,
	(var, value): (&str, &str),
	cwd: Option<&Path>,
	detached: bool,
	extra: &[&str],
) -> std::io::Result<Child> {
	let mut command = Command::new(std::env::current_exe()?);
	command
		.args(["--exact", test, "--nocapture", "--test-threads=1"])
		.args(extra)
		.env(var, value)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	if let Some(cwd) = cwd {
		command.current_dir(cwd);
	}
	#[cfg(windows)]
	if detached {
		use std::os::windows::process::CommandExt;
		// DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP.
		command.creation_flags(0x0000_0208);
	}
	#[cfg(unix)]
	if detached {
		use std::os::unix::process::CommandExt;
		command.process_group(0);
	}
	crate::shared::env::apply_to(&mut command);
	command.spawn()
}

/// What a stand-in does for at most 120 s, so none outlives a crashed test for long.
fn sleep_on() {
	for _ in 0..1_200 {
		std::thread::sleep(Duration::from_millis(100));
	}
}

pub(crate) fn pid_files(dir: &Path, prefix: &str) -> Vec<u32> {
	let mut found: Vec<u32> = std::fs::read_dir(dir)
		.into_iter()
		.flatten()
		.flatten()
		.filter_map(|entry| entry.file_name().to_str()?.strip_prefix(prefix)?.parse().ok())
		.collect();
	found.sort_unstable();
	found
}

/// The stand-in of a sleeper tree: writes its pid to `<pids>/<pid>`, starts its child (one level deeper, detached from
/// depth 1 on, and on Windows also when `outlive`), and sleeps.
#[test]
fn sleeper_main() {
	let Ok(spec) = std::env::var(SLEEPER_VAR) else { return };
	let parts: Vec<&str> = spec.split('|').collect();
	let (pids, depth, outlive) = (parts[0], parts[1].parse::<u32>().unwrap(), parts[2] == "outlive");
	std::fs::write(Path::new(pids).join(std::process::id().to_string()), depth.to_string()).unwrap();
	if depth < 2 {
		let detached = depth == 1 || (outlive && cfg!(windows));
		// Only the root's child is omp's daemon, when the spec says so (a fourth part).
		let extra: &[&str] = if depth == 0 && parts.get(3) == Some(&"daemon") { &[DAEMON_ARG] } else { &[] };
		let _ = spawn_self(
			"shared::proctree::tests::sleeper_main",
			(SLEEPER_VAR, &format!("{pids}|{}|{}", depth + 1, parts[2])),
			None,
			detached,
			extra,
		);
	}
	sleep_on();
}

/// A real tree of three stand-ins, as a pane's omp with its workers: the root starts a child, which starts a
/// grandchild detached (its own group on POSIX, as a language server may). `outlive` makes the child outlive the
/// root, as omp's workers outlive omp. Dropping it kills whatever of it is left and waits until it is gone.
pub(crate) struct SleeperTree {
	pub root: u32,
	/// Every pid of the tree, the root included.
	pub all: Vec<u32>,
	rootless: Option<Child>,
}

impl SleeperTree {
	/// Ends the root by `TerminateProcess` / SIGKILL through its own handle.
	pub(crate) fn kill_root(&mut self) {
		if let Some(child) = &mut self.rootless {
			let _ = child.kill();
		}
	}
}

impl Drop for SleeperTree {
	fn drop(&mut self) {
		for pid in &self.all {
			os::kill(*pid);
		}
		let _ = self.rootless.take();
		let until = std::time::Instant::now() + Duration::from_secs(10);
		while self.all.iter().any(|pid| pid_alive(*pid)) && std::time::Instant::now() < until {
			std::thread::sleep(Duration::from_millis(50));
		}
	}
}

/// A sleeper tree working in `folder` once all three run. On POSIX its root leads its own process group, as a herdr
/// pane's process does.
pub(crate) async fn sleeper_tree(folder: &Path, outlive: bool) -> SleeperTree {
	started_tree(folder, outlive, false).await
}

/// [`sleeper_tree`], with `daemon` making the root's child (and so the grandchild under it) omp's daemon by its
/// arguments.
async fn started_tree(folder: &Path, outlive: bool, daemon: bool) -> SleeperTree {
	let pids = folder.join("pids");
	std::fs::create_dir_all(&pids).unwrap();
	let spec = format!(
		"{}|0|{}{}",
		pids.display(),
		if outlive { "outlive" } else { "attached" },
		if daemon { "|daemon" } else { "" }
	);
	let root =
		spawn_self("shared::proctree::tests::sleeper_main", (SLEEPER_VAR, &spec), Some(folder), !cfg!(windows), &[])
			.expect("starting the sleeper tree");
	let mut tree = SleeperTree { root: root.id(), all: vec![root.id()], rootless: Some(root) };
	assert!(settles(|| pid_files(&pids, "").len() == 3).await, "the sleeper tree didn't start");
	tree.all = pid_files(&pids, "");
	tree
}

/// A sleeper tree and the folders it works in.
struct TreeIn {
	tree: SleeperTree,
	dir: TempDir,
	folder: PathBuf,
}

async fn tree_in(outlive: bool) -> TreeIn {
	let dir = temp_dir("proctree");
	let folder = dir.path().join("wt");
	std::fs::create_dir_all(&folder).unwrap();
	let tree = sleeper_tree(&folder, outlive).await;
	TreeIn { tree, dir, folder }
}

/// The stand-in of a shim tree (see [`shim_tree`]): the role in its spec decides what it does.
#[test]
fn shim_main() {
	let Ok(spec) = std::env::var(SHIM_VAR) else { return };
	let parts: Vec<&str> = spec.split('|').collect();
	let (role, pids, go, exit) = (parts[0], PathBuf::from(parts[1]), PathBuf::from(parts[2]), PathBuf::from(parts[3]));
	std::fs::write(pids.join(format!("{role}-{}", std::process::id())), "").unwrap();
	let start = |role: &str| {
		let spec = format!("{role}|{}|{}|{}", pids.display(), go.display(), exit.display());
		let _ = spawn_self("shared::proctree::tests::shim_main", (SHIM_VAR, &spec), None, cfg!(windows), &[]);
	};
	let wait = |what: &dyn Fn() -> bool| {
		while !what() {
			std::thread::sleep(Duration::from_millis(20));
		}
	};
	if role == "shim" {
		wait(&|| go.exists());
		start("omp");
	}
	if role == "omp" {
		wait(&|| exit.exists());
		start("worker");
		start("worker");
		wait(&|| pid_files(&pids, "worker-").len() >= 2);
		std::process::exit(0);
	}
	sleep_on();
}

/// A real tree like a pane's omp (0490): a shim whose child is the real omp, either outliving the other. omp starts
/// its workers only on `exit()`, then exits. Dropping it kills whatever of it is left.
struct ShimTree {
	shim: u32,
	shim_child: Option<Child>,
	pids: PathBuf,
	exit_file: PathBuf,
}

impl ShimTree {
	fn omp(&self) -> Option<u32> {
		pid_files(&self.pids, "omp-").first().copied()
	}

	fn workers(&self) -> Vec<u32> {
		pid_files(&self.pids, "worker-")
	}

	/// omp starts two workers, waits until they run, then exits, as omp does when its pane closes.
	fn exit(&self) {
		std::fs::write(&self.exit_file, "").unwrap();
	}

	fn all(&self) -> Vec<u32> {
		[pid_files(&self.pids, "shim-"), pid_files(&self.pids, "omp-"), self.workers()].concat()
	}
}

impl Drop for ShimTree {
	fn drop(&mut self) {
		for pid in self.all() {
			os::kill(pid);
		}
		let _ = self.shim_child.take();
		let until = std::time::Instant::now() + Duration::from_secs(10);
		while self.all().iter().any(|pid| pid_alive(*pid)) && std::time::Instant::now() < until {
			std::thread::sleep(Duration::from_millis(50));
		}
	}
}

async fn shim_tree(folder: &Path) -> ShimTree {
	let pids = folder.join("pids");
	let (exit_file, go_file) = (folder.join("exit"), folder.join("go"));
	std::fs::create_dir_all(&pids).unwrap();
	std::fs::write(&go_file, "").unwrap();
	let spec = format!("shim|{}|{}|{}", pids.display(), go_file.display(), exit_file.display());
	let child = spawn_self("shared::proctree::tests::shim_main", (SHIM_VAR, &spec), Some(folder), !cfg!(windows), &[])
		.expect("starting the shim tree");
	let tree = ShimTree { shim: child.id(), shim_child: Some(child), pids: pids.clone(), exit_file };
	let up = settles(|| pid_files(&pids, "shim-").len() == 1 && pid_files(&pids, "omp-").len() == 1).await;
	assert!(up, "the shim tree didn't start");
	tree
}

fn parents(rows: &[(u32, u32)]) -> BTreeMap<u32, u32> {
	rows.iter().copied().collect()
}

fn none() -> HashSet<u32> {
	HashSet::new()
}

#[test]
fn descendants_walks_a_parent_map_to_every_level_ignoring_a_pid_that_is_its_own_parent_and_never_entering_skip() {
	let map = parents(&[(10, 1), (11, 10), (12, 11), (13, 11), (14, 13), (20, 1), (0, 0)]);
	assert_eq!(descendants(10, &map, &none()), vec![11, 12, 13, 14]);
	assert_eq!(descendants(10, &map, &HashSet::from([13])), vec![11, 12]);
	assert_eq!(descendants(20, &map, &none()), Vec::<u32>::new());
}

/// A snapshot from `(pid, parent, start)` rows.
fn snapshot(rows: &[(u32, u32, Option<f64>)]) -> BTreeMap<u32, ProcessEntry> {
	rows.iter().map(|(pid, parent, start)| (*pid, ProcessEntry { parent: *parent, start: *start })).collect()
}

fn id(pid: u32, start: f64) -> ProcessId {
	ProcessId { pid, start }
}

fn strings(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn omps_daemon_is_the_process_with_the_exact_argument_after_the_program_never_a_path_or_a_prompt_holding_it_0490() {
	assert!(is_daemon_argv(&strings(&["omp.exe", "__omp_worker_daemon"])));
	assert!(is_daemon_argv(&strings(&["bun", "src/cli.ts", "__omp_worker_daemon"])));
	assert!(!is_daemon_argv(&strings(&["omp.exe"])));
	assert!(!is_daemon_argv(&strings(&["__omp_worker_daemon"])));
	assert!(!is_daemon_argv(&strings(&["omp.exe", "-p", "what is __omp_worker_daemon"])));
	assert!(!is_daemon_argv(&strings(&["omp.exe", "D:/x/__omp_worker_daemon/y", "__omp_worker_daemon2"])));
	assert!(!is_daemon_argv(&[]));
}

#[cfg(windows)]
#[test]
fn a_windows_command_line_splits_into_the_arguments_the_c_runtime_reads() {
	use super::os::split_command_line;
	assert_eq!(
		split_command_line(r"C:\omp\omp.exe __omp_worker_daemon"),
		strings(&[r"C:\omp\omp.exe", "__omp_worker_daemon"])
	);
	assert_eq!(
		split_command_line(r#""C:\Program Files\omp\omp.exe"  -p "run __omp_worker_daemon" "" x\\y "a\"b" "c\\" d"#),
		strings(&[r"C:\Program Files\omp\omp.exe", "-p", "run __omp_worker_daemon", "", r"x\\y", r#"a"b"#, r"c\", "d"])
	);
	assert_eq!(split_command_line("  "), Vec::<String>::new());
}

/// One tree under pid 10: 11 is omp's daemon with 12 and 13 under it, 14 under 13; 15 a sibling of the daemon.
fn daemon_rows() -> BTreeMap<u32, u32> {
	parents(&[(10, 1), (11, 10), (12, 11), (13, 11), (14, 13), (15, 10)])
}

#[test]
fn spare_daemons_takes_the_daemon_and_everything_under_it_out_of_the_candidates_and_nothing_else() {
	let map = daemon_rows();
	let mut near: BTreeSet<u32> = BTreeSet::from([10, 11, 12, 13, 14, 15]);
	let daemons = spare_daemons(&mut near, &map, &none(), |pids| {
		assert_eq!(pids, [10, 11, 12, 13, 14, 15]);
		HashSet::from([11])
	});
	assert_eq!(daemons, HashSet::from([11]));
	assert_eq!(near, BTreeSet::from([10, 15]));
	// Without a daemon the candidates stay whole.
	let mut near: BTreeSet<u32> = BTreeSet::from([10, 11, 15]);
	assert!(spare_daemons(&mut near, &map, &none(), |_| HashSet::new()).is_empty());
	assert_eq!(near, BTreeSet::from([10, 11, 15]));
}

#[test]
fn a_walk_over_the_candidates_left_after_sparing_the_daemon_leaves_its_subtree_out_of_the_pane_leftovers_0490() {
	let map = daemon_rows();
	let mut near: BTreeSet<u32> = BTreeSet::from([10, 11, 12, 13, 14, 15]);
	spare_daemons(&mut near, &map, &none(), |_| HashSet::from([11]));
	let processes = snapshot(&near.iter().map(|pid| (*pid, map[pid], Some(f64::from(*pid)))).collect::<Vec<_>>());
	assert_eq!(sorted(leftovers(id(10, 10.0), &processes, &none())), vec![10, 15]);
	// A daemon that is itself a root: nothing of it is left to end.
	let mut near: BTreeSet<u32> = BTreeSet::from([11, 12, 13, 14]);
	spare_daemons(&mut near, &map, &none(), |_| HashSet::from([11]));
	assert!(near.is_empty());
}

#[tokio::test]
async fn end_processes_ends_a_pane_and_spares_omps_daemon_under_it_with_its_workers_read_from_the_command_line_0490() {
	let dir = temp_dir("proctree-daemon");
	let folder = dir.path().join("wt");
	std::fs::create_dir_all(&folder).unwrap();
	let tree = started_tree(&folder, true, true).await;
	let root = process_id_of(tree.root).expect("the root runs");
	let under: Vec<u32> = tree.all.iter().copied().filter(|pid| *pid != tree.root).collect();
	let ended = end_processes(&[root], &[]).await;
	assert!(ended.contains(&tree.root));
	assert!(under.iter().all(|pid| !ended.contains(pid)), "ended {ended:?} of the daemon's {under:?}");
	assert!(under.iter().all(|pid| pid_alive(*pid)));
	// The daemon as a root of its own: nothing is ended either.
	let daemon = under
		.iter()
		.copied()
		.find(|pid| std::fs::read_to_string(folder.join("pids").join(pid.to_string())).is_ok_and(|depth| depth == "1"))
		.expect("the daemon's pid file");
	assert!(kill_tree(daemon).await.is_empty());
	assert!(pid_alive(daemon));
}

#[test]
fn leftovers_of_a_root_that_ended_its_orphans_and_their_children_never_one_started_before_it() {
	let mut processes = snapshot(&[
		(11, 10, Some(105.0)), // the dead root's child (a worker)
		(12, 11, Some(110.0)), // its grandchild
		(13, 10, Some(90.0)),  // started before the root: an earlier process of that pid had it
		(14, 13, Some(120.0)), // under that one
		(20, 1, Some(50.0)),   // unrelated
	]);
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), vec![11, 12]);
	// A grandchild started before its own parent hangs under an earlier run of that parent's pid.
	processes.insert(15, ProcessEntry { parent: 11, start: Some(104.0) });
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), vec![11, 12]);
	// A process whose start can't be read is left out with what is under it; the walk never enters skip.
	processes.insert(11, ProcessEntry { parent: 10, start: None });
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), Vec::<u32>::new());
	processes.insert(11, ProcessEntry { parent: 10, start: Some(105.0) });
	assert_eq!(leftovers(id(10, 100.0), &processes, &HashSet::from([11])), Vec::<u32>::new());
	assert_eq!(leftovers(id(10, 100.0), &processes, &HashSet::from([10])), Vec::<u32>::new());
}

#[test]
fn leftovers_of_a_root_still_running_are_the_root_and_its_tree() {
	let processes = snapshot(&[(10, 1, Some(100.0)), (11, 10, Some(100.0)), (12, 11, Some(130.0))]);
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), vec![10, 11, 12]);
}

#[test]
fn a_reused_pid_never_counts_the_process_that_has_it_now_and_what_it_started_are_left_out() {
	let mut processes = snapshot(&[
		(10, 1, Some(200.0)),  // the pid, given to another process after the root ended
		(11, 10, Some(150.0)), // the dead root's orphan
		(12, 11, Some(160.0)),
		(15, 10, Some(210.0)), // the new process's child
		(16, 15, Some(220.0)),
	]);
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), vec![11, 12]);
	// The new process's start unknown: its children can't be told from the root's, so nothing counts.
	processes.insert(10, ProcessEntry { parent: 1, start: None });
	assert_eq!(leftovers(id(10, 100.0), &processes, &none()), Vec::<u32>::new());
}

#[test]
fn a_live_process_that_names_a_long_dead_parent_whose_pid_is_now_in_the_tree_is_never_reached_srv_a_1() {
	// Windows keeps a dead parent's pid as the child's parent pid and hands the pid out again: pid 10 is now a pane's
	// root, and an unrelated browser (20, with its own child 21) started long before it names an earlier pid 10.
	let mut processes = snapshot(&[
		(10, 1, Some(1_000.0)),  // the pane's root
		(11, 10, Some(1_010.0)), // its real child
		(20, 10, Some(50.0)),    // the unrelated process: started before the root
		(21, 20, Some(60.0)),
	]);
	assert_eq!(leftovers(id(10, 1_000.0), &processes, &none()), vec![10, 11]);
	// The same from a root that has ended since: its orphans only.
	processes.remove(&10);
	assert_eq!(leftovers(id(10, 1_000.0), &processes, &none()), vec![11]);
	// From the child as a root (the walk taken again after the root ended): still not the browser.
	assert_eq!(leftovers(id(11, 1_010.0), &processes, &none()), vec![11]);
}

fn sorted(mut pids: Vec<u32>) -> Vec<u32> {
	pids.sort_unstable();
	pids
}

#[tokio::test]
async fn processes_in_finds_every_process_of_a_real_tree_working_in_a_folder_kill_tree_ends_them_all() {
	let TreeIn { tree, dir, folder } = tree_in(false).await;
	assert_eq!(tree.all.len(), 3);
	assert!(tree.all.contains(&tree.root));
	let in_use = processes_in(&folder);
	for pid in &tree.all {
		assert!(in_use.iter().any(|p| p.pid == *pid), "{pid} not found in {in_use:?}");
	}
	let exe = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().to_lowercase();
	let root = in_use.iter().find(|p| p.pid == tree.root).unwrap();
	// Linux's `comm` is cut to 15 characters.
	assert!(exe.starts_with(&root.name.to_lowercase()), "{} vs {exe}", root.name);
	// The folder's parent holds it too; a sibling whose name starts the same doesn't.
	assert!(processes_in(dir.path()).iter().any(|p| p.pid == tree.root));
	assert_eq!(processes_in(Path::new(&format!("{}2", folder.display()))), Vec::new());

	let ended = kill_tree(tree.root).await;
	assert_eq!(sorted(ended), sorted(tree.all.clone()));
	assert!(settles(|| tree.all.iter().all(|pid| !pid_alive(*pid))).await);
	assert!(settles(|| processes_in(&folder).is_empty()).await);
}

#[tokio::test]
async fn end_processes_ends_a_tree_whose_root_went_first_from_the_roots_pid_and_start() {
	let TreeIn { mut tree, folder, .. } = tree_in(true).await;
	let root = process_id_of(tree.root).expect("the root runs");
	assert_eq!(sorted(leftovers_of(&[root])), sorted(tree.all.clone()));
	tree.kill_root();
	assert!(settles(|| !pid_alive(tree.root)).await);
	let ended = end_processes(&[root], &[]).await;
	let orphans: Vec<u32> = tree.all.iter().copied().filter(|pid| *pid != tree.root).collect();
	assert_eq!(sorted(ended), sorted(orphans));
	assert!(settles(|| tree.all.iter().all(|pid| !pid_alive(*pid))).await);
	assert!(settles(|| processes_in(&folder).is_empty()).await);
}

#[tokio::test]
async fn a_real_tree_whose_root_ended_leftovers_of_finds_the_orphans_by_the_roots_pid_and_start_end_processes_ends_them()
 {
	let TreeIn { mut tree, folder, .. } = tree_in(true).await;
	let start = process_start(tree.root).expect("the root's start");
	let root = id(tree.root, start);
	assert_eq!(sorted(leftovers_of(&[root])), sorted(tree.all.clone()));
	// Remembered with an earlier start, the pid's process now is another run of it: none of the tree counts.
	assert_eq!(leftovers_of(&[id(tree.root, start / 2.0)]), Vec::<u32>::new());
	tree.kill_root();
	assert!(settles(|| !pid_alive(tree.root)).await);
	assert_eq!(process_start(tree.root), None);
	let orphans: Vec<u32> = tree.all.iter().copied().filter(|pid| *pid != tree.root).collect();
	assert_eq!(sorted(leftovers_of(&[root])), sorted(orphans));
	end_processes(&[root], &[]).await;
	assert!(settles(|| tree.all.iter().all(|pid| !pid_alive(*pid))).await);
	assert!(settles(|| processes_in(&folder).is_empty()).await);
	assert_eq!(leftovers_of(&[root]), Vec::<u32>::new());
}

#[tokio::test]
async fn an_omp_behind_a_shim_the_workers_it_starts_on_its_way_out_after_the_last_read_end_once_shim_and_omp_went_0490()
{
	let dir = temp_dir("proctree");
	let mut tree = shim_tree(dir.path()).await;
	let shim = process_id_of(tree.shim).expect("the shim runs");
	let reads = omp_processes(&[shim]);
	let read = reads.first().expect("one read");
	let omp = tree.omp().unwrap();
	assert_eq!(sorted(read.omp.iter().map(|id| id.pid).collect()), sorted(vec![tree.shim, omp]));
	// herdr closes the pane: the shim goes at once; omp starts its workers, then goes.
	tree.exit();
	if let Some(child) = &mut tree.shim_child {
		let _ = child.kill();
	}
	assert!(settles(|| !pid_alive(tree.shim)).await);
	let found = end_omp_leftovers(std::slice::from_ref(read), 10_000).await.unwrap();
	assert_eq!(tree.workers().len(), 2);
	assert_eq!(sorted(found), sorted(tree.workers()));
	assert!(tree.workers().iter().all(|pid| !pid_alive(*pid)));
	// The shim alone leads nowhere once the real omp is gone.
	assert_eq!(leftovers_of(&[shim]), Vec::<u32>::new());
}

#[cfg(windows)]
#[tokio::test]
async fn processes_in_finds_a_process_whose_working_folder_is_the_8_3_short_name_of_the_folder_asked_about() {
	// A short %TEMP% (`C:\Users\ZHIVKO~1\…`) gives children a short working folder; seen on 2026-10-24 when two omp
	// workers held a folder that processes_in, asked with the long name, didn't see.
	let dir = temp_dir("proctree");
	let folder = dir.path().join("A Long Folder Name");
	std::fs::create_dir(&folder).unwrap();
	let fso = format!("(New-Object -ComObject Scripting.FileSystemObject).GetFolder('{}').ShortPath", folder.display());
	let shown = Command::new("powershell").args(["-NoProfile", "-Command", &fso]).output().unwrap();
	let short = String::from_utf8_lossy(&shown.stdout).trim().to_owned();
	if short.is_empty() || short.eq_ignore_ascii_case(&folder.to_string_lossy()) {
		return; // This volume makes no 8.3 names.
	}
	assert!(short.contains('~'));
	let tree = sleeper_tree(Path::new(&short), false).await;
	// Under the full suite's load the new process's working folder can be unreadable for a moment: ask again until
	// it shows or 10 s pass.
	let found =
		wait_for(|| std::future::ready(Ok(processes_in(&folder).iter().any(|p| p.pid == tree.root))), 10_000, 200)
			.await
			.unwrap();
	assert!(found);
}

#[tokio::test]
async fn a_process_that_is_gone_or_this_process_is_never_in_a_tree_and_ends_nothing() {
	let mut gone = if cfg!(windows) {
		Command::new("cmd").args(["/C", "exit", "0"]).stdout(Stdio::null()).spawn().unwrap()
	} else {
		Command::new("sh").args(["-c", "true"]).stdout(Stdio::null()).spawn().unwrap()
	};
	gone.wait().unwrap();
	assert_eq!(kill_tree(gone.id()).await, Vec::<u32>::new());
	let ppid = parent_pids(None).get(&std::process::id()).copied();
	let own: Vec<ProcessId> =
		[Some(std::process::id()), ppid].into_iter().flatten().filter_map(process_id_of).collect();
	assert_eq!(leftovers_of(&own), Vec::<u32>::new());
	assert_eq!(end_processes(&own, &[]).await, Vec::<u32>::new());
	assert_eq!(kill_tree(std::process::id()).await, Vec::<u32>::new());
	assert!(pid_alive(std::process::id()));
}

#[tokio::test]
async fn end_processes_spares_a_process_and_what_runs_under_it_while_it_is_that_same_run_0582() {
	let TreeIn { tree, .. } = tree_in(true).await;
	let root = process_id_of(tree.root).expect("the root runs");
	let child = tree.all.iter().copied().find(|pid| *pid != tree.root).unwrap();
	let spared = process_id_of(child).expect("the child runs");
	let ended = end_processes(&[root], &[spared]).await;
	assert!(ended.contains(&tree.root));
	assert!(!ended.contains(&child));
	assert!(pid_alive(child));
}

#[test]
fn pid_alive_and_process_started_at_this_process_a_pid_that_is_none() {
	// test/shared/platform.test.ts: "pidAlive and sameToken" (the pidAlive part).
	assert!(pid_alive(std::process::id()));
	assert!(!pid_alive(0));
}

#[test]
fn process_started_at_is_this_processs_start_time_none_for_none_lc_4() {
	if !cfg!(windows) {
		return;
	}
	let now = chrono::Utc::now().timestamp_millis();
	let started = process_started_at(std::process::id()).expect("this process's start");
	assert!(started <= now);
	assert!(started > now - 3_600_000);
	assert_eq!(process_started_at(0), None);
}

#[test]
fn this_process_has_resident_memory_and_its_tree_is_at_least_that_big_0671() {
	let own = process_usage(std::process::id()).expect("this process's usage");
	assert!(own.rss_bytes > 0);
	assert!(tree_usage(std::process::id()).rss_bytes >= own.rss_bytes);
	assert_eq!(tree_usage(u32::MAX), TreeUsage::default());
}

#[test]
fn the_linux_proc_files_read_as_resident_bytes_and_cpu_ms_0671() {
	let status = "Name:\tcargo\nVmPeak:\t  900 kB\nVmRSS:\t  2048 kB\nThreads:\t4\n";
	assert_eq!(parse_status_rss(status), Some(2048 * 1024));
	assert_eq!(parse_status_rss("Name:\tkthreadd\n"), None);
	// utime 250 and stime 50 ticks; a `comm` with spaces and parentheses doesn't shift the fields.
	let stat = "42 (a (weird) name) S 1 42 42 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 1 0 12345 1000 200";
	assert_eq!(parse_stat_cpu_ms(stat), Some(3000));
	assert_eq!(parse_stat_cpu_ms("42 (short) S 1"), None);
}

#[test]
fn ps_time_reads_with_and_without_hours_and_days_0671() {
	assert_eq!(parse_ps_time("0:00.05"), Some(50));
	assert_eq!(parse_ps_time("12:34.50"), Some(754_500));
	assert_eq!(parse_ps_time("1:02:03.00"), Some(3_723_000));
	assert_eq!(parse_ps_time("2-03:04:05"), Some(2 * 86_400_000 + 11_045_000));
	assert_eq!(parse_ps_time("soon"), None);
}

#[test]
fn ps_lines_read_as_usages_and_a_bad_line_is_left_out_0671() {
	let text = "  101  20480   0:01.50\nnot a line\n  102   1024 1:00:00.00\n";
	assert_eq!(
		parse_ps_usage(text),
		vec![
			TreeUsage { rss_bytes: 20480 * 1024, cpu_ms: 1500 },
			TreeUsage { rss_bytes: 1024 * 1024, cpu_ms: 3_600_000 },
		]
	);
}
