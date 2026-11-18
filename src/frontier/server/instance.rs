//! One Jocasta service at a time (0362, 0549): `<data>/service.lock` names the service that holds it by pid and start
//! time; a lock an older plain service wrote also names the herdr server it started (0582). A start that finds it held
//! by a live service that answers leaves; one that is closing is waited for; one that doesn't answer within a bound is
//! hung: its process tree, all but that herdr server and what runs under it, is ended and the lock taken over (0534,
//! 0582), so a hung service never blocks every later start, as a hung service did (0530).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};

use serde_json::{Number, Value, json};

use crate::frontier::server::launch::ServiceState;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::as_object;
use crate::shared::log::{LogLevel, log};
use crate::shared::proctree::{ProcessId, process_id_of};

/// How long a live holder may go without answering before it counts as hung (0534).
const HUNG_MS: i64 = 15_000;
/// How long a holder that says it is closing is waited for before it counts as hung.
const CLOSING_MS: i64 = 60_000;

type Boxed<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub struct InstanceOptions {
	/// `<data>/service.lock`.
	pub lock_file: PathBuf,
	/// What the service `jocasta-service.json` names is doing (`service_state`).
	pub state: Box<dyn Fn() -> Boxed<ServiceState> + Send + Sync>,
	/// This process (test seam): the process as it runs now when `None`.
	pub self_id: Option<ProcessId>,
	/// Ends a hung holder with every process under it but `spare` and what runs under it (`end_processes`).
	pub kill: Box<dyn Fn(ProcessId, Vec<ProcessId>) -> Boxed<()> + Send + Sync>,
	pub hung_ms: Option<i64>,
	pub closing_ms: Option<i64>,
	/// Waits `ms` (test seam); tokio's sleep when `None`.
	pub sleep: Option<Box<dyn Fn(u64) -> Boxed<()> + Send + Sync>>,
}

impl InstanceOptions {
	/// The options with their defaults: `state` and `kill` as in the TypeScript, every other field unset.
	pub fn new<S, SF, K, KF>(lock_file: impl Into<PathBuf>, state: S, kill: K) -> Self
	where
		S: Fn() -> SF + Send + Sync + 'static,
		SF: Future<Output = ServiceState> + Send + 'static,
		K: Fn(ProcessId, Vec<ProcessId>) -> KF + Send + Sync + 'static,
		KF: Future<Output = ()> + Send + 'static,
	{
		Self {
			lock_file: lock_file.into(),
			state: Box::new(move || Box::pin(state())),
			self_id: None,
			kill: Box::new(move |holder, spare| Box::pin(kill(holder, spare))),
			hung_ms: None,
			closing_ms: None,
			sleep: None,
		}
	}
}

/// The lock this process holds.
#[derive(Debug)]
pub struct Instance {
	file: PathBuf,
	id: ProcessId,
}

impl Instance {
	/// Gives the lock up (the service has stopped); only while it still names this process.
	pub fn release(&self) -> Result<()> {
		if self.mine() {
			remove(&self.file)?;
		}
		Ok(())
	}

	fn mine(&self) -> bool {
		matches!(read_holder(&self.file), Read::Held(holder) if holder.id.pid == self.id.pid && holder.id.start == self.id.start)
	}
}

/// What the lock says: its holder and the herdr server that holder started.
struct Holder {
	id: ProcessId,
	herdr: Option<ProcessId>,
}

enum Read {
	/// No lock file.
	Absent,
	/// Being written, or torn.
	Unreadable,
	Held(Holder),
}

/// A process id as the lock file spells it: a start time that is a whole number is written as one, as JavaScript does.
fn id_json(id: ProcessId) -> Value {
	let start = if id.start.fract() == 0.0 && id.start.abs() < 9_007_199_254_740_992.0 {
		Number::from(id.start as i64)
	} else {
		Number::from_f64(id.start).unwrap_or_else(|| Number::from(0))
	};
	json!({ "pid": id.pid, "start": Value::Number(start) })
}

fn process_id(value: &Value) -> Option<ProcessId> {
	let object = as_object(value)?;
	let pid = u32::try_from(object.get("pid")?.as_u64()?).ok()?;
	Some(ProcessId { pid, start: object.get("start")?.as_f64()? })
}

fn read_holder(file: &Path) -> Read {
	let Ok(text) = fs::read_to_string(file) else { return Read::Absent };
	if let Ok(value) = serde_json::from_str::<Value>(&text)
		&& let Some(id) = process_id(&value)
	{
		return Read::Held(Holder { id, herdr: value.get("herdr").and_then(process_id) });
	}
	Read::Unreadable
}

/// `rmSync(file, { force: true })`.
fn remove(file: &Path) -> Result<()> {
	match fs::remove_file(file) {
		Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
		_ => Ok(()),
	}
}

fn private_options() -> OpenOptions {
	let mut options = OpenOptions::new();
	options.write(true);
	#[cfg(unix)]
	std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
	options
}

/// The lock, created only when there is none (`wx`); false when another holds it.
fn create(file: &Path, id: ProcessId) -> Result<bool> {
	if let Some(parent) = file.parent() {
		fs::create_dir_all(parent)?;
	}
	let mut created = match private_options().create_new(true).open(file) {
		Ok(created) => created,
		Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
		Err(error) => return Err(error.into()),
	};
	created.write_all(format!("{}\n", id_json(id)).as_bytes())?;
	Ok(true)
}

/// Takes the single-instance lock (0362, 0534): resolves the held lock, or `None` when another service holds it and
/// answers (this start then leaves). A dead holder's lock (crash; Windows reuses pids, so pid and start time both
/// count) is taken at once; a closing one is waited for up to `closing_ms`; a live holder that doesn't answer for
/// `hung_ms` is ended with its whole tree but the herdr server it recorded (0582), and its lock taken.
pub async fn take_instance(options: InstanceOptions) -> Result<Option<Instance>> {
	let id = match options.self_id {
		Some(id) => id,
		None => {
			process_id_of(std::process::id()).ok_or_else(|| Error::other("this process's start time can't be read"))?
		}
	};
	let file = options.lock_file.clone();
	let began = Instant::now();
	let now = || i64::try_from(began.elapsed().as_millis()).unwrap_or(i64::MAX);
	let mut silent_since: Option<i64> = None;
	let mut closing_since: Option<i64> = None;
	loop {
		if create(&file, id)? {
			return Ok(Some(Instance { file, id }));
		}
		let holder = match read_holder(&file) {
			Read::Absent => continue,
			Read::Unreadable => None,
			Read::Held(holder) => Some(holder),
		};
		if let Some(holder) = &holder
			&& process_id_of(holder.id.pid).map(|run| run.start) != Some(holder.id.start)
		{
			remove(&file)?;
			continue;
		}
		let state = if holder.is_some() { (options.state)().await } else { ServiceState::Busy };
		if state == ServiceState::Up {
			return Ok(None);
		}
		let at = now();
		if state == ServiceState::Closing {
			silent_since = None;
			closing_since.get_or_insert(at);
		} else {
			closing_since = None;
			silent_since.get_or_insert(at);
		}
		let hung = closing_since.is_some_and(|since| at - since >= options.closing_ms.unwrap_or(CLOSING_MS))
			|| silent_since.is_some_and(|since| at - since >= options.hung_ms.unwrap_or(HUNG_MS));
		if hung && let Some(holder) = &holder {
			log(
				LogLevel::Warn,
				"instance",
				&format!(
					"the service {} holds the lock but doesn't answer; ending it (herdr's session keeps running) and starting",
					holder.id.pid
				),
				None,
			);
			(options.kill)(holder.id, holder.herdr.into_iter().collect()).await;
			remove(&file)?;
			silent_since = None;
			closing_since = None;
			continue;
		}
		if hung {
			remove(&file)?;
		}
		match &options.sleep {
			Some(sleep) => sleep(250).await,
			None => tokio::time::sleep(Duration::from_millis(250)).await,
		}
	}
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;
