//! A program run the settings trials watch (fork plan section 4): its whole process tree's peak memory and the
//! machine's lowest available memory while it ran, under the memory floor. On Windows the program runs in a Job Object
//! of its own, so `PeakJobMemoryUsed` counts every process it starts, the Job Objects its own runner makes nested
//! inside; elsewhere the peak is not read. One run at a time: a run starts only with [`START_FLOOR`] available, and one
//! that keeps the machine under [`KILL_FLOOR`] for [`KILL_SAMPLES`] samples is ended with its tree.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use jocasta::shared::errors::{Error, Result};
use jocasta::shared::pollers::machine::available_memory_bytes;
use tokio::io::AsyncReadExt;

/// A run starts only with this much memory available (4 GB).
pub const START_FLOOR: u64 = 4 << 30;
/// Below this (1.5 GB) a sample counts toward ending the run.
pub const KILL_FLOOR: u64 = 3 << 29;
/// Consecutive samples under [`KILL_FLOOR`] that end the run.
pub const KILL_SAMPLES: u32 = 5;
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

/// How a watched run ended.
#[derive(Debug, Clone, Default)]
pub struct Watched {
	/// The exit code; `None` when it was ended (limit or memory floor) or ended by a signal.
	pub code: Option<i32>,
	pub wall_ms: u64,
	/// The process tree's peak committed memory (Windows); `None` elsewhere.
	pub peak_tree_bytes: Option<u64>,
	/// The machine's lowest available memory over the samples; `None` where the platform doesn't tell.
	pub min_available_bytes: Option<u64>,
	/// Ended by the memory floor: the row is `infra: memory`, not the arm's.
	pub memory_floor: bool,
	pub timed_out: bool,
	pub stdout: String,
	pub stderr: String,
}

/// Waits until the machine has [`START_FLOOR`] available, saying so once when it has to wait.
pub async fn wait_for_start_floor() {
	let mut said = false;
	while available_memory_bytes().is_some_and(|available| available < START_FLOOR) {
		if !said {
			eprintln!("waiting for 4 GB of available memory before the next run");
			said = true;
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
}

/// The file whose presence pauses a run between trials: `<run>/PAUSE`.
pub const PAUSE_FILE: &str = "PAUSE";

/// Waits while `<dir>/PAUSE` exists, saying once how to go on. A trial already running finishes first, so a pause loses
/// nothing; deleting the file lets the next trial start.
pub async fn wait_while_paused(dir: &Path) {
	let file = dir.join(PAUSE_FILE);
	let mut said = false;
	while file.exists() {
		if !said {
			eprintln!("paused: delete {} to go on", file.display());
			said = true;
		}
		tokio::time::sleep(Duration::from_secs(10)).await;
	}
	if said {
		eprintln!("going on");
	}
}

/// Runs `argv` in `cwd` with exactly `env`, watched, until it exits or `limit` passes.
pub async fn run_watched(
	argv: &[String],
	cwd: &Path,
	env: &HashMap<String, String>,
	limit: Duration,
) -> Result<Watched> {
	let (program, args) = argv.split_first().ok_or_else(|| Error::other("no program to run"))?;
	let mut command = tokio::process::Command::new(program);
	command
		.args(args)
		.current_dir(cwd)
		.env_clear()
		.envs(env)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.kill_on_drop(true);
	let started = Instant::now();
	let mut child = command.spawn()?;
	let job = tree::Tree::of(&child)?;
	let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
	let pipes = tokio::spawn(async move {
		let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
		let read_out = async {
			if let Some(pipe) = out.as_mut() {
				let _ = pipe.read_to_end(&mut stdout).await;
			}
		};
		let read_err = async {
			if let Some(pipe) = err.as_mut() {
				let _ = pipe.read_to_end(&mut stderr).await;
			}
		};
		tokio::join!(read_out, read_err);
		(String::from_utf8_lossy(&stdout).into_owned(), String::from_utf8_lossy(&stderr).into_owned())
	});
	let mut watched = Watched { min_available_bytes: available_memory_bytes(), ..Watched::default() };
	let mut under = 0;
	let deadline = tokio::time::sleep(limit);
	tokio::pin!(deadline);
	let mut samples = tokio::time::interval(SAMPLE_EVERY);
	loop {
		tokio::select! {
			status = child.wait() => {
				watched.code = status?.code();
				break;
			}
			() = &mut deadline => {
				watched.timed_out = true;
				job.end();
				let _ = child.kill().await;
				break;
			}
			_ = samples.tick() => {
				let Some(available) = available_memory_bytes() else { continue };
				watched.min_available_bytes = Some(watched.min_available_bytes.map_or(available, |low| low.min(available)));
				under = if available < KILL_FLOOR { under + 1 } else { 0 };
				if under >= KILL_SAMPLES {
					watched.memory_floor = true;
					job.end();
					let _ = child.kill().await;
					break;
				}
			}
		}
	}
	watched.wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
	watched.peak_tree_bytes = job.peak();
	// A grandchild still holding the pipes is ended with the job; elsewhere the pipes close with the child.
	job.end();
	let (stdout, stderr) = tokio::time::timeout(Duration::from_secs(10), pipes)
		.await
		.ok()
		.and_then(|joined| joined.ok())
		.unwrap_or_default();
	watched.stdout = stdout;
	watched.stderr = stderr;
	Ok(watched)
}

#[cfg(windows)]
mod tree {
	use std::ffi::c_void;

	use jocasta::shared::errors::Result;
	use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
	use windows_sys::Win32::System::JobObjects::{
		AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
		JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, QueryInformationJobObject,
		SetInformationJobObject, TerminateJobObject,
	};

	/// The Job Object holding a run's process tree; closing it ends whatever is left.
	pub struct Tree(HANDLE);

	// SAFETY: a Job Object handle may be used from any thread.
	unsafe impl Send for Tree {}
	unsafe impl Sync for Tree {}

	impl Tree {
		/// A new Job Object with `child` in it. Processes `child` starts afterwards join it, their own Job Objects nested.
		pub fn of(child: &tokio::process::Child) -> Result<Self> {
			let process =
				child.raw_handle().ok_or_else(|| jocasta::shared::errors::Error::other("the run had no process"))?;
			// SAFETY: null attributes and name make an unnamed job with default security.
			let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
			if job.is_null() {
				return Err(std::io::Error::last_os_error().into());
			}
			let tree = Self(job);
			// SAFETY: an all-zero JOBOBJECT_EXTENDED_LIMIT_INFORMATION is valid; only the kill-on-close flag is set.
			let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
			limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
			let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
			// SAFETY: `limits` is a valid, sized structure of the class named.
			let set = unsafe {
				SetInformationJobObject(
					job,
					JobObjectExtendedLimitInformation,
					(&raw const limits).cast::<c_void>(),
					size,
				)
			};
			// SAFETY: both handles are open.
			if set == 0 || unsafe { AssignProcessToJobObject(job, process.cast()) } == 0 {
				return Err(std::io::Error::last_os_error().into());
			}
			Ok(tree)
		}

		/// The tree's peak committed memory so far.
		pub fn peak(&self) -> Option<u64> {
			// SAFETY: an all-zero structure is a valid out buffer of the class queried.
			let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
			let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
			// SAFETY: `info` is a valid, sized out buffer; the returned-length pointer may be null.
			let ok = unsafe {
				QueryInformationJobObject(
					self.0,
					JobObjectExtendedLimitInformation,
					(&raw mut info).cast::<c_void>(),
					size,
					std::ptr::null_mut(),
				)
			};
			(ok != 0).then_some(info.PeakJobMemoryUsed as u64)
		}

		/// Ends every process in the tree.
		pub fn end(&self) {
			// SAFETY: the handle is open; ending an empty job is harmless.
			unsafe { TerminateJobObject(self.0, 1) };
		}
	}

	impl Drop for Tree {
		fn drop(&mut self) {
			// SAFETY: the handle is open and closed once; kill-on-close ends what is left.
			unsafe { CloseHandle(self.0) };
		}
	}
}

#[cfg(not(windows))]
mod tree {
	use jocasta::shared::errors::Result;

	/// Elsewhere the run is the child alone: no peak is read, and the child is killed by the caller.
	pub struct Tree;

	impl Tree {
		/// The run without a Job Object.
		pub fn of(_child: &tokio::process::Child) -> Result<Self> {
			Ok(Self)
		}

		/// No peak is read here.
		pub fn peak(&self) -> Option<u64> {
			None
		}

		/// Nothing to end: the caller kills the child.
		pub fn end(&self) {}
	}
}
