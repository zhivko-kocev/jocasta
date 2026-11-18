//! Running a program to its end with its output captured (CO-10, LC-11): one runner, so every run hides its console
//! window (the service has none of its own, 0552), ignores stdin unless given, keeps a bounded copy of its output, and
//! on its time limit or an abort ends the program's whole tree, never only the direct child: a `.cmd` shim's cmd.exe or
//! Bun's `omp.exe` shim would leave the real program running and holding the pipes. The program starts in a Job Object
//! (a process group elsewhere) through process-wrap and the tree ends through it, orphaned grandchildren too (0641);
//! the job never kills on close, so Jocasta's exit ends nothing.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use process_wrap::{std as wrap_std, tokio as wrap_tokio};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

use crate::shared::env;
use crate::shared::errors::{Error, Result};

/// The cancel an `AbortSignal` is: the sender sets `Some(reason)` to abort; the run then fails with that reason.
pub type AbortSignal = watch::Receiver<Option<String>>;

/// `CREATE_NO_WINDOW`: a run opens no console window.
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How much of each of stdout and stderr a run keeps by default; the rest is read and dropped.
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// After its tree was ended, how long a run still waits for its pipes before it settles with what it has.
const PIPE_GRACE_MS: u64 = 1_000;

/// Where `command` is on PATH, or `None`. Windows spells the variable `Path`, and an environment copied into a map
/// (as a Worker's `process.env` was) is case-sensitive, so the variable is found whatever its case. `env` is the
/// environment searched; the process's own when not given. On Windows each `PATHEXT` extension is tried too.
pub fn on_path(command: &str, env: Option<&HashMap<String, String>>) -> Option<PathBuf> {
	let own: HashMap<String, String>;
	let vars = match env {
		Some(vars) => vars,
		None => {
			own = env::vars()
				.into_iter()
				.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
				.collect();
			&own
		}
	};
	let lookup =
		|wanted: &str| vars.iter().find(|(name, _)| name.eq_ignore_ascii_case(wanted)).map(|(_, v)| v.as_str());
	let path = lookup("PATH").unwrap_or("");
	let extensions = extensions(lookup("PATHEXT"));
	if command.contains(['/', '\\']) {
		return find_in(Path::new(""), command, &extensions);
	}
	let separator = if cfg!(windows) { ';' } else { ':' };
	path.split(separator)
		.map(|dir| dir.trim_matches('"'))
		.filter(|dir| !dir.is_empty())
		.find_map(|dir| find_in(Path::new(dir), command, &extensions))
}

/// The extensions a program name may be completed with: Windows' `PATHEXT`, none elsewhere.
fn extensions(pathext: Option<&str>) -> Vec<String> {
	if !cfg!(windows) {
		return Vec::new();
	}
	pathext.unwrap_or(".COM;.EXE;.BAT;.CMD").split(';').filter(|ext| !ext.is_empty()).map(str::to_owned).collect()
}

fn find_in(dir: &Path, command: &str, extensions: &[String]) -> Option<PathBuf> {
	let exact = dir.join(command);
	std::iter::once(exact.clone())
		.chain(extensions.iter().map(|ext| dir.join(format!("{command}{ext}"))))
		.find(|candidate| is_program(candidate))
}

#[cfg(unix)]
fn is_program(path: &Path) -> bool {
	use std::os::unix::fs::PermissionsExt;
	std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_program(path: &Path) -> bool {
	std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

/// What a run may be given.
#[derive(Default)]
pub struct RunOptions {
	pub cwd: Option<PathBuf>,
	/// The whole environment (not merged with the process's).
	pub env: Option<HashMap<String, String>>,
	/// What stdin reads; it is ignored otherwise.
	pub stdin: Option<Vec<u8>>,
	/// Ends the program's tree after this long; the result says `timed_out` and has `code: None`.
	pub timeout_ms: Option<i64>,
	/// Ends the program's tree when it aborts; the run then fails with the signal's reason at once.
	pub signal: Option<AbortSignal>,
	/// How much of each of stdout and stderr is kept (bytes); 64 MiB by default.
	pub max_bytes: Option<usize>,
	/// cmd.exe's command line as given (a `.cmd` quoted by the caller): the arguments after the program are passed
	/// as written. Windows only.
	pub windows_verbatim_arguments: bool,
}

/// What a run left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
	/// The exit code; `None` when it was ended at its time limit or by a signal.
	pub code: Option<i32>,
	/// The signal that ended it, when one did (`SIGKILL`).
	pub signal: Option<String>,
	pub stdout: String,
	pub stderr: String,
	pub timed_out: bool,
}

/// A signal number as Bun names it.
#[cfg(unix)]
fn signal_name(number: i32) -> String {
	match number {
		1 => "SIGHUP".into(),
		2 => "SIGINT".into(),
		3 => "SIGQUIT".into(),
		6 => "SIGABRT".into(),
		9 => "SIGKILL".into(),
		11 => "SIGSEGV".into(),
		13 => "SIGPIPE".into(),
		14 => "SIGALRM".into(),
		15 => "SIGTERM".into(),
		other => format!("SIG{other}"),
	}
}

#[cfg(unix)]
fn signal_of(status: &std::process::ExitStatus) -> Option<String> {
	use std::os::unix::process::ExitStatusExt;
	status.signal().map(signal_name)
}

#[cfg(not(unix))]
fn signal_of(_status: &std::process::ExitStatus) -> Option<String> {
	None
}

/// A program that couldn't start, as the TypeScript's callers read it: Bun says `Executable not found in $PATH: "x"`
/// for a bare name and `ENOENT: no such file or directory, uv_spawn 'x'` for a path, Node's `spawnSync x ENOENT` for the
/// synchronous run. Any other failure keeps the operating system's own text.
fn spawn_failed(error: std::io::Error, program: &impl AsRef<OsStr>, sync: bool) -> Error {
	if error.kind() != std::io::ErrorKind::NotFound {
		return Error::Io(error);
	}
	let program = program.as_ref().to_string_lossy();
	let said = if sync {
		format!("spawnSync {program} ENOENT")
	} else if program.contains(['/', '\\']) {
		format!("ENOENT: no such file or directory, uv_spawn '{program}'")
	} else {
		format!("Executable not found in $PATH: \"{program}\"")
	};
	Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, said))
}

/// The command line a run starts: program, arguments, folder, environment (then the test overlay of `env::apply_to`).
fn command_for(argv: &[impl AsRef<OsStr>], options: &RunOptions) -> Result<std::process::Command> {
	let (program, args) = argv.split_first().ok_or_else(|| Error::other("no program to run"))?;
	let mut command = std::process::Command::new(program);
	#[cfg(windows)]
	if options.windows_verbatim_arguments {
		use std::os::windows::process::CommandExt;
		for arg in args {
			command.raw_arg(arg);
		}
	} else {
		command.args(args);
	}
	#[cfg(not(windows))]
	command.args(args);
	if let Some(cwd) = &options.cwd {
		command.current_dir(cwd);
	}
	if let Some(vars) = &options.env {
		command.env_clear().envs(vars);
	}
	env::apply_to(&mut command);
	Ok(command)
}

/// `command` as process-wrap starts it (0641): in its own Job Object on Windows, leading its own process group
/// elsewhere, so ending it ends the whole tree, orphans included. `creation_flags` are Windows's. No `KillOnDrop`: a
/// program the TypeScript build left running when Jocasta exited still runs.
pub(crate) fn wrap_async(command: tokio::process::Command, creation_flags: u32) -> wrap_tokio::CommandWrap {
	let mut wrapped = wrap_tokio::CommandWrap::from(command);
	#[cfg(windows)]
	{
		let mut flags = wrap_tokio::CreationFlags(Default::default());
		flags.0.0 = creation_flags;
		wrapped.wrap(flags).wrap(wrap_tokio::JobObject);
	}
	#[cfg(unix)]
	{
		let _ = creation_flags;
		wrapped.wrap(wrap_tokio::ProcessGroup::leader());
	}
	wrapped
}

/// [`wrap_async`] for a synchronous start.
pub(crate) fn wrap_sync(command: std::process::Command, creation_flags: u32) -> wrap_std::CommandWrap {
	let mut wrapped = wrap_std::CommandWrap::from(command);
	#[cfg(windows)]
	{
		let mut flags = wrap_std::CreationFlags(Default::default());
		flags.0.0 = creation_flags;
		wrapped.wrap(flags).wrap(wrap_std::JobObject);
	}
	#[cfg(unix)]
	{
		let _ = creation_flags;
		wrapped.wrap(wrap_std::ProcessGroup::leader());
	}
	wrapped
}

/// A stream read to its end, keeping its first `max` bytes in `kept`.
async fn capture(mut stream: impl AsyncRead + Unpin, max: usize, kept: Arc<Mutex<Vec<u8>>>) {
	let mut chunk = vec![0u8; 16 * 1024];
	loop {
		let Ok(read) = stream.read(&mut chunk).await else { return };
		if read == 0 {
			return;
		}
		let mut bytes = kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		let room = max.saturating_sub(bytes.len());
		bytes.extend_from_slice(&chunk[..read.min(room)]);
	}
}

/// [`capture`] of a pipe the child may not have.
async fn capture_opt(stream: Option<impl AsyncRead + Unpin>, max: usize, kept: Arc<Mutex<Vec<u8>>>) {
	if let Some(stream) = stream {
		capture(stream, max, kept).await;
	}
}

fn text_of(kept: &Mutex<Vec<u8>>) -> String {
	String::from_utf8_lossy(&kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())).into_owned()
}

/// Settles when the signal aborts, with its reason; never when there is no signal.
pub(crate) async fn aborted(signal: &mut Option<AbortSignal>) -> String {
	let Some(signal) = signal else { return std::future::pending().await };
	loop {
		if let Some(reason) = signal.borrow_and_update().clone() {
			return reason;
		}
		if signal.changed().await.is_err() {
			return std::future::pending().await;
		}
	}
}

/// Settles after `ms`; never when there is no limit.
pub(crate) async fn limit(ms: Option<i64>) {
	match ms {
		Some(ms) => tokio::time::sleep(Duration::from_millis(ms.max(0).unsigned_abs())).await,
		None => std::future::pending().await,
	}
}

enum First {
	Done(std::io::Result<std::process::ExitStatus>),
	Aborted(String),
	TimedOut,
}

/// Runs `argv` to its end and resolves its exit code and output. A program that can't be started (not found) is an
/// `Error::Io`, so each caller keeps its own text for it. An aborted signal fails the run with its reason.
pub async fn run_program(argv: &[impl AsRef<OsStr>], mut options: RunOptions) -> Result<RunResult> {
	if let Some(reason) = options.signal.as_ref().and_then(|signal| signal.borrow().clone()) {
		return Err(Error::other(reason));
	}
	let max = options.max_bytes.unwrap_or(MAX_OUTPUT_BYTES);
	let mut command = tokio::process::Command::from(command_for(argv, &options)?);
	command
		.stdin(if options.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child =
		wrap_async(command, CREATE_NO_WINDOW).spawn().map_err(|error| spawn_failed(error, &argv[0], false))?;
	if let (Some(input), Some(mut stdin)) = (options.stdin.take(), child.stdin().take()) {
		tokio::spawn(async move {
			let _ = stdin.write_all(&input).await;
			let _ = stdin.shutdown().await;
		});
	}
	let (out, err) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
	let (stdout, stderr) = (child.stdout().take(), child.stderr().take());
	let mut pipes = tokio::spawn({
		let (out, err) = (out.clone(), err.clone());
		async move {
			tokio::join!(capture_opt(stdout, max, out), capture_opt(stderr, max, err));
		}
	});
	// The direct child's exit, not the job's: a program that left a background process running has still ended.
	let first = tokio::select! {
		(status, _) = async { tokio::join!(child.inner_mut().wait(), async { (&mut pipes).await }) } => First::Done(status),
		reason = aborted(&mut options.signal) => First::Aborted(reason),
		() = limit(options.timeout_ms) => First::TimedOut,
	};
	let result = match first {
		First::Done(status) => {
			let status = status?;
			RunResult {
				code: status.code(),
				signal: signal_of(&status),
				stdout: text_of(&out),
				stderr: text_of(&err),
				timed_out: false,
			}
		}
		First::Aborted(reason) => {
			let _ = child.start_kill();
			pipes.abort();
			return Err(Error::other(reason));
		}
		First::TimedOut => {
			// A grandchild that escaped the walk may still hold the pipes, so the run settles once the program is gone
			// and its pipes had a moment, with the output read so far.
			let _ = child.start_kill();
			// The signal that ended it, as the child reports one: SIGKILL on POSIX, none on Windows.
			let status = child.inner_mut().wait().await.ok();
			let signal = status.and_then(|status| signal_of(&status));
			let grace = tokio::time::sleep(Duration::from_millis(PIPE_GRACE_MS));
			tokio::select! { _ = &mut pipes => (), () = grace => () }
			pipes.abort();
			RunResult { code: None, signal, stdout: text_of(&out), stderr: text_of(&err), timed_out: true }
		}
	};
	pipes.abort();
	Ok(result)
}

/// `run_program` for the callers that must run synchronously (git on the service's sync paths). The synchronous run
/// ends the program's whole tree at `timeout_ms`, through its job or group as `run_program` does; a program that couldn't start is an `Error::Io`, and so is output
/// past `max_bytes` (the child is ended), as Node's `maxBuffer` is. The run has no abort signal.
pub fn run_program_sync(argv: &[impl AsRef<OsStr>], options: RunOptions) -> Result<RunResult> {
	let max = options.max_bytes.unwrap_or(MAX_OUTPUT_BYTES);
	let mut command = command_for(argv, &options)?;
	command
		.stdin(if options.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child =
		wrap_sync(command, CREATE_NO_WINDOW).spawn().map_err(|error| spawn_failed(error, &argv[0], true))?;
	if let (Some(input), Some(mut stdin)) = (options.stdin, child.stdin().take()) {
		std::thread::spawn(move || {
			let _ = stdin.write_all(&input);
		});
	}
	let overflow = Arc::new(Mutex::new(false));
	let out = spawn_reader(child.stdout().take(), max, overflow.clone());
	let err = spawn_reader(child.stderr().take(), max, overflow.clone());
	let deadline = options.timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms.max(0).unsigned_abs()));
	let mut timed_out = false;
	let status = loop {
		if let Some(status) = child.try_wait()? {
			break status;
		}
		let over = *overflow.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		if over || deadline.is_some_and(|at| Instant::now() >= at) {
			timed_out = !over;
			let _ = child.start_kill();
			let status = child.wait()?;
			if over {
				return Err(Error::Io(std::io::Error::other("spawnSync ENOBUFS")));
			}
			break status;
		}
		std::thread::sleep(Duration::from_millis(2));
	};
	let (stdout, stderr) = (out.finish(), err.finish());
	Ok(RunResult {
		code: if timed_out { None } else { status.code() },
		// Node ends a run at its limit with SIGTERM, and says so on every platform.
		signal: if timed_out { Some("SIGTERM".to_owned()) } else { signal_of(&status) },
		stdout: String::from_utf8_lossy(&stdout).into_owned(),
		stderr: String::from_utf8_lossy(&stderr).into_owned(),
		timed_out,
	})
}

/// A thread reading one pipe to its end into a shared buffer.
struct Reader {
	kept: Arc<Mutex<Vec<u8>>>,
	thread: Option<std::thread::JoinHandle<()>>,
}

impl Reader {
	/// What was read; waits a moment for the pipe to end (a grandchild may hold it) and takes what it has.
	fn finish(mut self) -> Vec<u8> {
		let wait_until = Instant::now() + Duration::from_millis(PIPE_GRACE_MS);
		while let Some(thread) = &self.thread {
			if thread.is_finished() || Instant::now() >= wait_until {
				break;
			}
			std::thread::sleep(Duration::from_millis(2));
		}
		if self.thread.as_ref().is_some_and(std::thread::JoinHandle::is_finished) {
			let _ = self.thread.take().map(std::thread::JoinHandle::join);
		}
		std::mem::take(&mut *self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
	}
}

fn spawn_reader(stream: Option<impl Read + Send + 'static>, max: usize, overflow: Arc<Mutex<bool>>) -> Reader {
	let kept = Arc::new(Mutex::new(Vec::new()));
	let thread = stream.map(|mut stream| {
		let kept = kept.clone();
		std::thread::spawn(move || {
			let mut chunk = vec![0u8; 16 * 1024];
			while let Ok(read) = stream.read(&mut chunk) {
				if read == 0 {
					return;
				}
				let mut bytes = kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
				if bytes.len() + read > max {
					*overflow.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
					return;
				}
				bytes.extend_from_slice(&chunk[..read]);
			}
		})
	});
	Reader { kept, thread }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::proctree::{pid_alive, tests::settles};

	const SPAWNER_VAR: &str = "JOCASTA_TEST_PROCESS_CHILD";

	/// The stand-in programs: this test executable run again on this function (what `bun -e` was). `sleep` sleeps;
	/// `tree` starts a sleeper, prints its pid on a line of its own as `pid=<n>`, and sleeps.
	#[test]
	#[allow(clippy::zombie_processes)] // The sleeper is ended by the run that started this process.
	fn process_child() {
		let Ok(mode) = std::env::var(SPAWNER_VAR) else { return };
		if mode == "tree" || mode == "orphan" {
			// An orphan's grandchild keeps the pipes, so the run is still going when its parent has gone.
			let grandchild = std::process::Command::new(std::env::current_exe().unwrap())
				.args(["--exact", "shared::process::tests::process_child", "-q", "--nocapture"])
				.env(SPAWNER_VAR, "sleep")
				.stdin(Stdio::null())
				.stdout(if mode == "orphan" { Stdio::inherit() } else { Stdio::null() })
				.stderr(Stdio::null())
				.spawn()
				.unwrap();
			println!("pid={}", grandchild.id());
			std::io::stdout().flush().unwrap();
			if mode == "orphan" {
				return;
			}
		}
		for _ in 0..1_200 {
			std::thread::sleep(Duration::from_millis(100));
		}
	}

	fn this_exe() -> OsStringArgs {
		let exe = std::env::current_exe().unwrap();
		vec![
			exe.into_os_string(),
			"--exact".into(),
			"shared::process::tests::process_child".into(),
			"-q".into(),
			"--nocapture".into(),
		]
	}

	type OsStringArgs = Vec<std::ffi::OsString>;

	/// This test executable as the `sleep` stand-in, its environment set.
	fn with_mode(mode: &str) -> HashMap<String, String> {
		let mut vars: HashMap<String, String> =
			env::vars().into_iter().filter_map(|(n, v)| Some((n.into_string().ok()?, v.into_string().ok()?))).collect();
		vars.insert(SPAWNER_VAR.to_owned(), mode.to_owned());
		vars
	}

	/// A shell command: PowerShell on Windows, `sh` elsewhere.
	fn shell(script_windows: &str, script_unix: &str) -> Vec<String> {
		if cfg!(windows) {
			vec!["powershell".into(), "-NoProfile".into(), "-Command".into(), script_windows.into()]
		} else {
			vec!["sh".into(), "-c".into(), script_unix.into()]
		}
	}

	#[test]
	fn on_path_finds_a_program_whatever_the_path_variables_case() {
		let exe = std::env::current_exe().unwrap();
		let name = exe.file_name().unwrap().to_string_lossy().into_owned();
		let folder = exe.parent().unwrap().to_string_lossy().into_owned();
		let only = |key: &str| HashMap::from([(key.to_owned(), folder.clone())]);
		assert!(on_path(&name, Some(&only("Path"))).is_some());
		assert!(on_path(&name, Some(&only("PATH"))).is_some());
		assert!(on_path(&name, Some(&HashMap::new())).is_none());
		if cfg!(windows) {
			assert!(on_path(name.trim_end_matches(".exe"), Some(&only("Path"))).is_some());
		}
	}

	#[tokio::test]
	async fn run_program_exit_code_both_streams_stdin_when_given_output_past_the_cap_dropped() {
		let echo = shell(
			"[Console]::Out.Write([Console]::In.ReadToEnd()); [Console]::Error.Write('err' + \"`n\"); exit 3",
			"cat; echo err >&2; exit 3",
		);
		let run =
			run_program(&echo, RunOptions { stdin: Some(b"hello".to_vec()), ..RunOptions::default() }).await.unwrap();
		assert_eq!(
			run,
			RunResult { code: Some(3), signal: None, stdout: "hello".into(), stderr: "err\n".into(), timed_out: false }
		);
		let flood = shell("[Console]::Out.Write('x' * 100000)", "head -c 100000 /dev/zero | tr '\\0' x");
		let capped = run_program(&flood, RunOptions { max_bytes: Some(10), ..RunOptions::default() }).await.unwrap();
		assert_eq!(capped.stdout, "x".repeat(10));
		assert!(run_program(&["jocasta-no-such-program"], RunOptions::default()).await.is_err());
	}

	#[tokio::test]
	async fn a_program_that_cannot_start_says_so_in_buns_and_nodes_words() {
		let bare = run_program(&["jocasta-no-such-program"], RunOptions::default()).await.unwrap_err();
		assert_eq!(bare.to_string(), "Executable not found in $PATH: \"jocasta-no-such-program\"");
		let path = if cfg!(windows) { "D:/jocasta-none/x.exe" } else { "/jocasta-none/x" };
		let by_path = run_program(&[path], RunOptions::default()).await.unwrap_err();
		assert_eq!(by_path.to_string(), format!("ENOENT: no such file or directory, uv_spawn '{path}'"));
		let sync = run_program_sync(&["jocasta-no-such-program"], RunOptions::default()).unwrap_err();
		assert_eq!(sync.to_string(), "spawnSync jocasta-no-such-program ENOENT");
	}

	#[tokio::test]
	async fn run_program_ends_the_programs_whole_tree_at_its_limit_and_says_so() {
		let run = run_program(
			&this_exe(),
			RunOptions { env: Some(with_mode("tree")), timeout_ms: Some(3_000), ..RunOptions::default() },
		)
		.await
		.unwrap();
		assert!(run.timed_out);
		assert_eq!(run.code, None);
		// Bun's `signalCode` after the tree kill: SIGKILL on POSIX, none on Windows (TerminateProcess).
		assert_eq!(run.signal.as_deref(), if cfg!(unix) { Some("SIGKILL") } else { None });
		let grandchild: u32 = run
			.stdout
			.lines()
			.find_map(|line| line.strip_prefix("pid=")?.trim().parse().ok())
			.expect("the grandchild's pid");
		assert!(grandchild > 0);
		assert!(settles(|| !pid_alive(grandchild)).await);
	}

	/// The pid a stand-in printed as `pid=<n>`.
	fn printed_pid(stdout: &str) -> u32 {
		stdout.lines().find_map(|line| line.strip_prefix("pid=")?.trim().parse().ok()).expect("the grandchild's pid")
	}

	#[tokio::test]
	async fn run_program_ends_a_grandchild_whose_parent_already_exited_with_the_tree_0641() {
		let run = run_program(
			&this_exe(),
			RunOptions { env: Some(with_mode("orphan")), timeout_ms: Some(3_000), ..RunOptions::default() },
		)
		.await
		.unwrap();
		assert!(run.timed_out);
		let grandchild = printed_pid(&run.stdout);
		assert!(settles(|| !pid_alive(grandchild)).await);
	}

	#[test]
	fn run_program_sync_ends_the_programs_whole_tree_at_its_limit_through_its_job_0641() {
		let run = run_program_sync(
			&this_exe(),
			RunOptions { env: Some(with_mode("tree")), timeout_ms: Some(3_000), ..RunOptions::default() },
		)
		.unwrap();
		assert!(run.timed_out);
		let grandchild = printed_pid(&run.stdout);
		let start = Instant::now();
		while pid_alive(grandchild) && start.elapsed() < Duration::from_secs(10) {
			std::thread::sleep(Duration::from_millis(50));
		}
		assert!(!pid_alive(grandchild));
	}

	#[tokio::test]
	async fn run_program_fails_at_once_with_the_reason_when_its_signal_aborts() {
		let (abort, signal) = watch::channel(None);
		let argv = this_exe();
		let run = run_program(
			&argv,
			RunOptions { env: Some(with_mode("sleep")), signal: Some(signal), ..RunOptions::default() },
		);
		let aborting = async {
			tokio::time::sleep(Duration::from_millis(300)).await;
			abort.send_replace(Some("stopping".into()));
		};
		let (run, ()) = tokio::join!(run, aborting);
		assert_eq!(run.unwrap_err().to_string(), "stopping");
	}

	#[test]
	fn run_program_sync_the_same_result_and_a_time_limit_that_reads_as_code_none() {
		let said =
			run_program_sync(&shell("[Console]::Out.Write('ok' + \"`n\")", "echo ok"), RunOptions::default()).unwrap();
		assert_eq!(
			said,
			RunResult { code: Some(0), signal: None, stdout: "ok\n".into(), stderr: String::new(), timed_out: false }
		);
		let slow = run_program_sync(
			&this_exe(),
			RunOptions { env: Some(with_mode("sleep")), timeout_ms: Some(1_000), ..RunOptions::default() },
		)
		.unwrap();
		assert!(slow.timed_out);
		assert_eq!(slow.code, None);
		// Node's `spawnSync` ends it with SIGTERM on every platform.
		assert_eq!(slow.signal.as_deref(), Some("SIGTERM"));
	}
}
