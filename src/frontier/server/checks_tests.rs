//! `checks.rs`'s tests, and the stand-in check this test executable plays (the TypeScript ran `bun -e` programs): a
//! shim script in a test's own folder sets `JOCASTA_TEST_CHECK` to what to do and runs this executable again on
//! [`fake_check`].

use std::path::Path;
use std::time::{Duration, SystemTime};

use super::*;
use crate::shared::testing::{TempDir, temp_dir, temp_jocasta};

const FAKE: &str = "JOCASTA_TEST_CHECK";
const FAKE_TEST: &str = "frontier::server::checks::tests::fake_check";

/// What a shim says to do, `|`-separated: `run|<sleep ms>|<exit code>|<text>` prints the text after the sleep;
/// `lines|<exit code>|<line>~<line>…` prints lines; `orphan|<file>` starts a sleeper that outlives it, writes the
/// sleeper's pid in `<file>` and exits; `trace|<file>|<name>|<sleep ms>` appends `start <name>`, sleeps and
/// appends `end <name>`. Does nothing in a normal test run.
#[test]
fn fake_check() {
	let Some(action) = env::var(FAKE) else { return };
	let parts: Vec<&str> = action.split('|').collect();
	let pause = |ms: &str| std::thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0)));
	match parts[0] {
		"run" => {
			pause(parts[1]);
			println!("{}", parts[3]);
			std::process::exit(parts[2].parse().unwrap_or(1));
		}
		"orphan" => {
			let sleeper = std::process::Command::new(std::env::current_exe().expect("the test executable"))
				.args(["--exact", FAKE_TEST, "-q", "--nocapture"])
				.env(FAKE, "run|60000|0|sleeper")
				.stdin(Stdio::null())
				.stdout(Stdio::null())
				.stderr(Stdio::null())
				.spawn()
				.expect("starting the sleeper");
			std::fs::write(parts[1], sleeper.id().to_string()).expect("writing the pid");
			std::process::exit(0);
		}
		"lines" => {
			for line in parts[2].split('~') {
				println!("{line}");
			}
			std::process::exit(parts[1].parse().unwrap_or(1));
		}
		"trace" => {
			let append = |line: String| {
				let mut file = std::fs::OpenOptions::new().create(true).append(true).open(parts[1]).expect("the trace");
				writeln!(file, "{line}").expect("tracing");
			};
			append(format!("start {}", parts[2]));
			pause(parts[3]);
			append(format!("end {}", parts[2]));
			std::process::exit(0);
		}
		_ => {}
	}
}

/// A command line (quoted for cmd.exe and sh alike) that runs [`fake_check`] with `action`, through a shim `name` in
/// `dir`.
fn fake(dir: &Path, name: &str, action: &str) -> String {
	let exe = std::env::current_exe().expect("the test executable");
	let exe = exe.display();
	if cfg!(windows) {
		let shim = dir.join(format!("{name}.cmd"));
		let script = format!(
			"@echo off\r\nset \"{FAKE}={action}\"\r\n\"{exe}\" --exact {FAKE_TEST} -q --nocapture\r\nexit /b %ERRORLEVEL%\r\n"
		);
		std::fs::write(&shim, script).expect("writing the shim");
		format!("\"{}\"", shim.display())
	} else {
		let shim = dir.join(name);
		let script = format!("#!/bin/sh\n{FAKE}='{action}' exec \"{exe}\" --exact {FAKE_TEST} -q --nocapture\n");
		std::fs::write(&shim, script).expect("writing the shim");
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("making the shim run");
		}
		format!("\"{}\"", shim.display())
	}
}

/// A shim that runs `first` and then `second` (shims' commands, as [`fake`] makes them) in one shell.
fn after(dir: &Path, name: &str, first: &str, second: &str) -> String {
	if cfg!(windows) {
		let shim = dir.join(format!("{name}.cmd"));
		std::fs::write(&shim, format!("@echo off\r\ncall {first}\r\ncall {second}\r\n")).expect("writing the shim");
		format!("\"{}\"", shim.display())
	} else {
		let shim = dir.join(name);
		std::fs::write(&shim, format!("#!/bin/sh\n{first}\n{second}\n")).expect("writing the shim");
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("making the shim run");
		}
		format!("\"{}\"", shim.display())
	}
}

fn spec(name: &str, command: &str) -> CheckSpec {
	CheckSpec { name: name.to_owned(), command: command.to_owned(), mise: false }
}

/// A runner with no tool versions in its key (mise isn't needed here).
fn runner(dir: &Path) -> CheckRunner {
	CheckRunner::new(CheckRunnerOptions {
		dir: dir.to_path_buf(),
		versions: Some(Arc::new(|_, _| async { String::new() }.boxed())),
		poll_ms: Some(20),
		..CheckRunnerOptions::default()
	})
}

fn git(dir: &Path, args: &[&str]) -> String {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
	String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A git repo with one commit, in a folder of the test's own.
fn make_repo(parent: &TempDir) -> PathBuf {
	let dir = parent.path().join("repo");
	std::fs::create_dir_all(&dir).unwrap();
	git(&dir, &["init", "-q", "-b", "main"]);
	std::fs::write(dir.join("README.md"), "demo\n").unwrap();
	git(&dir, &["add", "."]);
	git(&dir, &["commit", "-q", "-m", "init repo"]);
	dir
}

/// Sets a file's or folder's modified time.
fn touch(path: &Path, time: SystemTime) {
	let mut options = std::fs::OpenOptions::new();
	#[cfg(windows)]
	{
		use std::os::windows::fs::OpenOptionsExt;
		// FILE_FLAG_BACKUP_SEMANTICS: a folder can be opened.
		options.write(true).custom_flags(0x0200_0000);
	}
	#[cfg(not(windows))]
	options.read(true);
	options.open(path).expect("opening to touch").set_modified(time).expect("setting the time");
}

fn data_of(j: &crate::shared::testing::TempJocasta) -> PathBuf {
	j.root().join("data")
}

const MISE_LOG: &str = concat!(
	"[lint] $ echo lint ok\n",
	"[test] $ bun test\n",
	"[lint] lint ok\n",
	"[test] test\\auth.test.ts:\n",
	"[test] 4 |   expect(backoff(2)).toBe(5);\n",
	"[test]       at <anonymous> (C:\\work\\repo\\test\\auth.test.ts:4:22)\n",
	"[test] (fail) retries back off [0.24ms]\n",
	"[test] (pass) keeps going [0.01ms]\n",
	"[test]       at <anonymous> (C:\\work\\repo\\test\\auth.test.ts:8:22)\n",
	"[test] (fail) gives up after 5 [0.06ms]\n",
	"ERROR 1 task(s) failed:\n",
	"[test] exited with status 1\n",
	"[test] ERROR task failed",
);

fn failure(file: &str, line: u32, test: &str) -> TestFailure {
	TestFailure { file: file.to_owned(), line: Some(line), test: test.to_owned() }
}

#[test]
fn mises_log_names_its_tasks_and_the_failed_ones_buns_test_output_names_failing_files_and_lines() {
	assert_eq!(
		mise_tasks(MISE_LOG),
		[MiseTask { name: "lint".to_owned(), failed: false }, MiseTask { name: "test".to_owned(), failed: true }]
	);
	assert_eq!(
		bun_failures(MISE_LOG, Path::new("C:\\work\\repo")),
		[failure("test/auth.test.ts", 4, "retries back off"), failure("test/auth.test.ts", 8, "gives up after 5")]
	);
}

#[test]
fn the_gate_reply_is_0546s_form_one_line_per_gate_failures_log_and_the_narrowest_rerun_silent_on_a_pass() {
	let mise = CheckSpec { name: "check".to_owned(), command: "mise run -c check".to_owned(), mise: true };
	let log = Path::new("C:/data/checks/3f9a1c2b4d5e/check.log");
	let cwd = Path::new("C:\\work\\repo");
	let done_when = spec("done_when", "bun run e2e:login");
	let run = CheckRun {
		tree: "3f9a1c2b4d5e6f708192a3b4c5d6e7f8091a2b3c".to_owned(),
		passed: false,
		results: vec![
			CheckResult {
				spec: mise.clone(),
				exit: Some(1),
				log: log.to_path_buf(),
				ms: 1,
				how: How::Ran,
				gates: gates_of(&mise, Some(1), MISE_LOG, log, cwd),
			},
			CheckResult {
				spec: done_when.clone(),
				exit: Some(0),
				log: PathBuf::from("x"),
				ms: 1,
				how: How::Hit,
				gates: gates_of(&done_when, Some(0), "", Path::new("x"), cwd),
			},
		],
	};
	let task_log = log.parent().unwrap().join("test.log");
	assert_eq!(
		gate_reply(&run),
		[
			"checks: FAIL (tree 3f9a1c)".to_owned(),
			"  PASS  lint       mise run lint".to_owned(),
			"  FAIL  test       mise run test".to_owned(),
			"        2 failed: test/auth.test.ts:4 \"retries back off\", test/auth.test.ts:8 \"gives up after 5\""
				.to_owned(),
			format!("        log: {}", task_log.display()),
			"        rerun: bun test test/auth.test.ts".to_owned(),
			"  PASS  done_when  bun run e2e:login".to_owned(),
		]
		.join("\n")
	);
	// A failure the runner names no files for reruns the check itself.
	let plain = gates_of(&spec("done_when", "make e2e"), Some(2), "boom", Path::new("l"), cwd);
	assert_eq!(
		plain,
		[Gate {
			name: "done_when".to_owned(),
			command: "make e2e".to_owned(),
			pass: false,
			failures: vec![],
			log: Some(PathBuf::from("l")),
			rerun: Some("make e2e".to_owned()),
		}]
	);
	assert_eq!(gate_reply(&CheckRun { passed: true, ..run }), "");
}

/// Waits on real processes: git for each tree key, a repo and two shim checks through the shell.
#[tokio::test]
async fn one_run_per_tree_for_two_concurrent_callers_a_fail_is_stored_and_replays_a_stale_lock_is_taken_over() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let dir = data_of(&j).join("checks");
	let slow = spec("test", &fake(work.path(), "slow", "run|400|3|ran"));
	let env = process_env();
	let a = runner(&dir);
	let b = runner(&dir);
	let (first, second) = tokio::join!(
		a.run(&repo, std::slice::from_ref(&slow), &env, None),
		b.run(&repo, std::slice::from_ref(&slow), &env, None)
	);
	let (first, second) = (first.unwrap().unwrap(), second.unwrap().unwrap());
	let mut hows = [format!("{:?}", first.results[0].how), format!("{:?}", second.results[0].how)];
	hows.sort();
	assert_eq!(hows, ["Joined", "Ran"]);
	assert_eq!(a.runs() + b.runs(), 1);
	assert!(!first.passed);
	assert_eq!(first.results[0].exit, Some(3));
	assert!(std::fs::read_to_string(&first.results[0].log).unwrap().contains("ran"));

	// The same tree again: the stored FAIL answers, nothing runs.
	let c = runner(&dir);
	let again = c.run(&repo, std::slice::from_ref(&slow), &env, None).await.unwrap().unwrap();
	assert_eq!(again.results[0].how, How::Hit);
	assert_eq!(again.results[0].exit, Some(3));
	assert_eq!(c.runs(), 0);
	assert_eq!(read_check_hits(&dir), 2);

	// An edit is a new tree; a lock left by a process that is gone is taken over.
	std::fs::write(repo.join("new.txt"), "x\n").unwrap();
	let quick = spec("test", &fake(work.path(), "quick", "run|0|0|ok"));
	let tree = c.tree_key(&repo).await.unwrap().unwrap();
	let folder = dir.join(&tree[..12]);
	std::fs::create_dir_all(&folder).unwrap();
	std::fs::write(folder.join("test.lock"), "999999").unwrap();
	let fresh = c.run(&repo, &[quick], &env, None).await.unwrap().unwrap();
	assert_eq!(fresh.results[0].how, How::Ran);
	assert!(fresh.passed);
	assert!(!folder.join("test.lock").exists());
	// The real index was never touched: the new file is still untracked.
	assert!(git(&repo, &["status", "--porcelain"]).contains("?? new.txt"));
	for runner in [a, b, c] {
		runner.stop().await;
	}
}

#[tokio::test]
async fn outside_git_there_is_no_key_and_nothing_runs() {
	let j = temp_jocasta();
	let plain = temp_dir("checks");
	let r = runner(&data_of(&j).join("checks"));
	let run = r.run(plain.path(), &[spec("x", "echo")], &process_env(), None).await.unwrap();
	assert_eq!(run, None);
	r.stop().await;
}

#[tokio::test]
async fn a_lock_whose_pid_now_names_another_process_a_reused_pid_is_taken_over_not_waited_on() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let dir = data_of(&j).join("checks");
	let r = runner(&dir);
	let folder = dir.join(&r.tree_key(&repo).await.unwrap().unwrap()[..12]);
	std::fs::create_dir_all(&folder).unwrap();
	// This test's own pid is alive, but it started at another time than the lock says.
	std::fs::write(folder.join("test.lock"), format!("{} 1", std::process::id())).unwrap();
	let ok = spec("test", &fake(work.path(), "ok", "run|0|0|ok"));
	let run = r.run(&repo, &[ok], &process_env(), None).await.unwrap().unwrap();
	assert_eq!(run.results[0].how, How::Ran);
	assert!(!folder.join("test.lock").exists());
	let stale: Vec<_> = std::fs::read_dir(&folder)
		.unwrap()
		.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
		.filter(|name| name.ends_with(".stale"))
		.collect();
	assert!(stale.is_empty(), "{stale:?}");
	r.stop().await;
}

#[tokio::test]
async fn a_run_waiting_on_a_live_holders_lock_ends_when_the_runner_stops() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let dir = data_of(&j).join("checks");
	let r = runner(&dir);
	let tree = r.tree_key(&repo).await.unwrap().unwrap();
	let folder = dir.join(&tree[..12]);
	std::fs::create_dir_all(&folder).unwrap();
	// This process holds it, as itself: a live holder the run waits on.
	let pid = std::process::id();
	let held = process_start(pid).map_or_else(|| pid.to_string(), |start| format!("{pid} {start}"));
	std::fs::write(folder.join("test.lock"), held).unwrap();
	let ok = spec("test", &fake(work.path(), "ok", "run|0|0|ok"));
	let env = process_env();
	let specs = [ok];
	let run = r.run(&repo, &specs, &env, Some(&tree));
	// Lets the run reach its wait (the runner exposes no signal for it); a stop before that ends it the same way.
	let stop = async {
		tokio::time::sleep(Duration::from_millis(100)).await;
		r.stop().await;
	};
	let (run, ()) = tokio::join!(run, stop);
	assert!(run.unwrap_err().to_string().contains("the check runner has stopped"));
	assert_eq!(r.runs(), 0);
}

#[tokio::test]
async fn a_runs_checks_run_one_after_another() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let trace = j.root().join("trace.txt");
	let step = |name: &str| spec(name, &fake(work.path(), name, &format!("trace|{}|{name}|200", trace.display())));
	let r = runner(&data_of(&j).join("checks"));
	let run = r.run(&repo, &[step("one"), step("two")], &process_env(), None).await.unwrap().unwrap();
	assert!(run.passed);
	let said = std::fs::read_to_string(&trace).unwrap();
	assert_eq!(said.lines().collect::<Vec<_>>(), ["start one", "end one", "start two", "end two"]);
	r.stop().await;
}

#[tokio::test]
async fn a_stored_result_keeps_its_gates_reruns_quote_paths_with_spaces_task_names_make_safe_file_names() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let dir = data_of(&j).join("checks");
	let env = process_env();
	let failing = spec("done_when", &fake(work.path(), "failing", "lines|1|test/a b.test.ts:~(fail) adds"));
	let first = runner(&dir).run(&repo, std::slice::from_ref(&failing), &env, None).await.unwrap().unwrap();
	let tree = first.tree.clone();
	let gate = &first.results[0].gates[0];
	assert_eq!(gate.rerun.as_deref(), Some("bun test \"test/a b.test.ts\""));
	std::fs::remove_file(&first.results[0].log).unwrap();
	let again = runner(&dir).run(&repo, &[failing], &env, None).await.unwrap().unwrap();
	assert_eq!(again.results[0].how, How::Hit);
	assert_eq!(again.results[0].gates, first.results[0].gates);

	// A mise task named `ci/lint` writes `ci-lint.log`, and its result is stored.
	let command = fake(work.path(), "mise", "lines|1|[ci/lint] boom~[ci/lint] ERROR task failed");
	let mise = CheckSpec { name: "check".to_owned(), command, mise: true };
	let run = runner(&dir).run(&repo, &[mise], &env, Some(&tree)).await.unwrap().unwrap();
	let folder = dir.join(&tree[..12]);
	let gates: Vec<_> = run.results[0].gates.iter().map(|g| (g.name.clone(), g.pass, g.log.clone())).collect();
	assert_eq!(gates, [("ci/lint".to_owned(), false, Some(folder.join("ci-lint.log")))]);
	assert!(std::fs::read_to_string(folder.join("ci-lint.log")).unwrap().contains("boom"));
	assert!(folder.join("check.json").exists());
}

#[tokio::test]
async fn stored_results_untouched_for_14_days_are_removed_at_the_days_first_start_with_one_log_line() {
	let j = temp_jocasta();
	let dir = data_of(&j).join("checks");
	let old = SystemTime::now() - Duration::from_secs(15 * 24 * 60 * 60);
	let stale = dir.join("aaaaaaaaaaaa");
	std::fs::create_dir_all(&stale).unwrap();
	std::fs::write(stale.join("test.log"), "x".repeat(2048)).unwrap();
	std::fs::write(stale.join("test.json"), "").unwrap();
	for file in ["test.log", "test.json"] {
		touch(&stale.join(file), old);
	}
	touch(&stale, old);
	let fresh = dir.join("bbbbbbbbbbbb");
	std::fs::create_dir_all(&fresh).unwrap();
	std::fs::write(fresh.join("test.log"), "x").unwrap();
	std::fs::write(dir.join("hits.log"), "1\n").unwrap();
	touch(&dir.join("hits.log"), old);
	std::fs::create_dir_all(dir.join("tmp")).unwrap();
	std::fs::write(dir.join("tmp").join("index-old"), "").unwrap();
	touch(&dir.join("tmp").join("index-old"), SystemTime::now() - Duration::from_secs(2 * 60 * 60));
	std::fs::write(dir.join("tmp").join("index-new"), "").unwrap();

	let r = runner(&dir);
	r.pruning().await;
	r.stop().await;
	assert!(!stale.exists());
	assert!(fresh.exists());
	assert_eq!(read_check_hits(&dir), 1);
	let tmp: Vec<_> = std::fs::read_dir(dir.join("tmp")).unwrap().map(|e| e.unwrap().file_name()).collect();
	assert_eq!(tmp, ["index-new"]);
	let logs = data_of(&j).join("logs");
	let said_all = || {
		std::fs::read_dir(&logs)
			.unwrap()
			.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
			.collect::<String>()
	};
	assert!(said_all().contains("[checks] removed 1 stored results older than 14 days (2.0 KB)"));

	// Another start the same day prunes nothing more: a result gone stale since stays, and nothing is said.
	let later = dir.join("cccccccccccc");
	std::fs::create_dir_all(&later).unwrap();
	std::fs::write(later.join("test.log"), "x").unwrap();
	touch(&later.join("test.log"), old);
	touch(&later, old);
	let same_day = runner(&dir);
	same_day.pruning().await;
	same_day.stop().await;
	assert!(later.exists());
	assert_eq!(said_all().split("removed").count(), 2);

	// The next day's first start prunes again.
	std::fs::write(dir.join("pruned-on"), "2000-01-01\n").unwrap();
	let next_day = runner(&dir);
	next_day.pruning().await;
	next_day.stop().await;
	assert!(!later.exists());
}

#[test]
fn sizes_are_said_in_b_kb_mb_and_gb_whole_numbers_from_10_up_one_decimal_below() {
	assert_eq!(size_said(512.0), "512 B");
	assert_eq!(size_said(4.2 * 1024.0), "4.2 KB");
	assert_eq!(size_said(380.0 * 1024.0 * 1024.0), "380 MB");
	assert_eq!(size_said(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB");
}

/// Bun's `proc.exited` resolves to 128 plus the signal for a child a signal ended (probed: SIGTERM 143, SIGKILL 137),
/// and the TypeScript stores that as the check's `exit`.
#[cfg(unix)]
#[test]
fn a_check_ended_by_a_signal_has_the_exit_128_plus_its_number() {
	use std::os::unix::process::ExitStatusExt;
	assert_eq!(exit_code(&std::process::ExitStatus::from_raw(9)), Some(137));
	assert_eq!(exit_code(&std::process::ExitStatus::from_raw(15)), Some(143));
	assert_eq!(exit_code(&std::process::ExitStatus::from_raw(3 << 8)), Some(3));
}

#[tokio::test]
async fn a_check_ended_at_its_limit_ends_a_grandchild_whose_parent_already_exited_0641() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let pid_file = work.path().join("sleeper.pid");
	let started = fake(work.path(), "orphaner", &format!("orphan|{}", pid_file.display()));
	let hang = fake(work.path(), "hang", "run|60000|0|never");
	let check = spec("test", &after(work.path(), "both", &started, &hang));
	let r = CheckRunner::new(CheckRunnerOptions {
		dir: data_of(&j).join("checks"),
		versions: Some(Arc::new(|_, _| async { String::new() }.boxed())),
		timeout_ms: Some(4_000),
		poll_ms: Some(20),
		admission: None,
	});
	let run = r.run(&repo, &[check], &process_env(), None).await.unwrap().unwrap();
	assert_eq!(run.results[0].exit, None);
	assert!(std::fs::read_to_string(&run.results[0].log).unwrap().contains("jocasta: killed (0 minutes)"));
	let sleeper: u32 = std::fs::read_to_string(&pid_file).expect("the sleeper's pid").trim().parse().unwrap();
	assert!(crate::shared::proctree::tests::settles(|| !pid_alive(sleeper)).await);
	r.stop().await;
}

const GIB: u64 = 1 << 30;

/// A queue with `free_gib` of free memory, `reserve_gib` kept back and `cores` cores; `seen` are the commands with a
/// record: (command, peak in GiB, cores).
fn queue(free_gib: u64, reserve_gib: u64, cores: usize, seen: &[(&str, u64, f64)]) -> Arc<Admission> {
	let costs = CheckCosts::in_memory();
	for (command, peak, used) in seen {
		costs.record(&CheckCosts::key(Path::new("project"), command), peak * GIB, *used, 0);
	}
	Arc::new(Admission::with(Arc::new(move || free_gib * GIB), cores, reserve_gib * GIB, costs))
}

type Waiter = tokio::task::JoinHandle<Option<Permit>>;

/// `command` asks to run; the ask is answered once the queue lets it.
fn ask(queue: &Arc<Admission>, command: &'static str) -> Waiter {
	let queue = Arc::clone(queue);
	tokio::spawn(async move { queue.admit(Path::new("project"), command).await })
}

/// Lets every ready task run (the tests' tasks only wait on the queue, never on the clock).
async fn settle() {
	for _ in 0..20 {
		tokio::task::yield_now().await;
	}
}

async fn permit(waiter: Waiter) -> Permit {
	waiter.await.unwrap().expect("admitted")
}

fn load(queue: &Admission) -> (u32, u32) {
	let load = queue.load();
	(load.running, load.waiting)
}

#[tokio::test]
async fn a_check_with_a_record_runs_beside_others_while_their_peaks_fit_in_free_memory_minus_the_reserve_0671() {
	// 10 GiB free, 2 kept: 8 GiB of room.
	let q = queue(10, 2, 16, &[("a", 3, 1.0), ("b", 4, 1.0), ("c", 2, 1.0)]);
	let a = permit(ask(&q, "a")).await;
	let _b = permit(ask(&q, "b")).await;
	assert_eq!(load(&q), (2, 0));
	// 3 + 4 + 2 = 9 GiB does not fit: c waits until a ends.
	let c = ask(&q, "c");
	settle().await;
	assert!(!c.is_finished());
	assert_eq!(load(&q), (2, 1));
	drop(a);
	let _c = permit(c).await;
	assert_eq!(load(&q), (2, 0));
}

#[tokio::test]
async fn the_cores_of_the_running_checks_and_the_new_one_must_fit_in_the_machines_cores_0671() {
	let q = queue(100, 0, 2, &[("a", 1, 1.5), ("b", 1, 1.0), ("c", 1, 0.5)]);
	let a = permit(ask(&q, "a")).await;
	let b = ask(&q, "b");
	settle().await;
	// 1.5 + 1.0 > 2 cores, though memory is plenty.
	assert!(!b.is_finished());
	drop(a);
	let _b = permit(b).await;
	// 1.0 + 0.5 fits.
	let _c = permit(ask(&q, "c")).await;
}

#[tokio::test]
async fn the_queue_is_first_come_first_served_a_small_check_never_passes_a_bigger_one_before_it_0671() {
	let q = queue(10, 2, 16, &[("a", 2, 1.0), ("b", 7, 1.0), ("c", 1, 1.0)]);
	let a = permit(ask(&q, "a")).await;
	let b = ask(&q, "b");
	settle().await;
	let c = ask(&q, "c");
	settle().await;
	// c would fit beside a (2 + 1 <= 8) but b, which does not (2 + 7 > 8), asked first.
	assert!(!b.is_finished() && !c.is_finished());
	assert_eq!(load(&q), (1, 2));
	drop(a);
	let _b = permit(b).await;
	let _c = permit(c).await;
	assert_eq!(load(&q), (2, 0));
}

#[tokio::test]
async fn a_check_with_no_record_runs_alone_once_and_its_measurement_becomes_its_record_0671() {
	let q = queue(100, 0, 16, &[("known", 1, 1.0)]);
	let known = permit(ask(&q, "known")).await;
	let fresh = ask(&q, "fresh");
	settle().await;
	// It waits for the running check to drain.
	assert!(!fresh.is_finished());
	drop(known);
	let fresh = permit(fresh).await;
	// Nothing starts while it runs, though memory and cores are plenty.
	let other = ask(&q, "known");
	settle().await;
	assert!(!other.is_finished());
	assert_eq!(load(&q), (1, 1));
	fresh.record(GIB, 1.0);
	drop(fresh);
	let _other = permit(other).await;
	// Now it has a record: it runs beside another.
	let _again = permit(ask(&q, "fresh")).await;
	assert_eq!(load(&q), (2, 0));
}

#[tokio::test]
async fn nothing_running_admits_the_next_check_whatever_it_costs_0671() {
	let q = queue(10, 2, 4, &[("huge", 100, 64.0)]);
	let huge = permit(ask(&q, "huge")).await;
	assert_eq!(load(&q), (1, 0));
	drop(huge);
	assert_eq!(load(&q), (0, 0));
}

#[tokio::test]
async fn closing_the_queue_releases_the_waiters_and_admits_no_one_after_0671() {
	let q = queue(10, 2, 4, &[("a", 1, 1.0)]);
	let _a = permit(ask(&q, "fresh")).await;
	let waiting = ask(&q, "a");
	settle().await;
	assert!(!waiting.is_finished());
	q.close();
	assert!(waiting.await.unwrap().is_none());
	assert!(q.admit(Path::new("project"), "a").await.is_none());
	assert_eq!(load(&q), (1, 0));
}

#[tokio::test]
async fn a_wait_that_is_dropped_leaves_the_queue_0671() {
	let q = queue(10, 2, 4, &[]);
	let _first = permit(ask(&q, "first")).await;
	let second = ask(&q, "second");
	settle().await;
	assert_eq!(load(&q), (1, 1));
	second.abort();
	let _ = second.await;
	assert_eq!(load(&q), (1, 0));
}

#[tokio::test]
async fn the_runner_records_what_a_check_cost_and_waiting_does_not_count_toward_its_timeout_0671() {
	let j = temp_jocasta();
	let work = temp_dir("checks");
	let repo = make_repo(&work);
	let dir = data_of(&j).join("checks");
	let costs_file = dir.join("costs.json");
	let r = CheckRunner::new(CheckRunnerOptions {
		dir: dir.clone(),
		versions: Some(Arc::new(|_, _| async { String::new() }.boxed())),
		// Each check runs about 2 s; the second waits that long for the first, then must still have its own 3.5 s.
		timeout_ms: Some(3_500),
		poll_ms: Some(20),
		admission: Some(Admission::with(Arc::new(|| u64::MAX), 64, 0, CheckCosts::load(costs_file.clone()))),
	});
	let one = [spec("one", &fake(work.path(), "one", "run|2000|0|one"))];
	let two = [spec("two", &fake(work.path(), "two", "run|2000|0|two"))];
	let env = process_env();
	let (first, second) = tokio::join!(r.run(&repo, &one, &env, None), r.run(&repo, &two, &env, None));
	// Neither has a record yet: one runs alone and the other waits, and neither was killed.
	for run in [first.unwrap().unwrap(), second.unwrap().unwrap()] {
		assert_eq!(run.results[0].exit, Some(0));
	}
	assert_eq!((r.load().running, r.load().waiting), (0, 0));
	// Each run's measurement became its record.
	let records = read_json_object(&costs_file).expect("the costs file");
	assert_eq!(records.len(), 2);
	for (_, cost) in records {
		assert!(cost["peak_rss_bytes"].as_u64().unwrap() > 0);
		assert_eq!(cost["runs"], 1);
	}
	r.stop().await;
}
