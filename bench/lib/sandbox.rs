//! The live benchmarks (H2–H6, H8) act on a running Jocasta: they kill its processes, start threads and plant facts.
//! They run only against a sandbox (the 0441 walkthrough's, or 0481's VM) with its own data folder, herdr session and
//! config folder (0502, 0576): `JOCASTA_DATA_DIR` set and not the OS default, `JOCASTA_HERDR_SESSION` naming a session
//! that isn't his own (`jocasta`, or `jarvis` before the cutover), and `JOCASTA_CODING_AGENT_DIR` set and not
//! `~/.jocasta/agent`: the config folder picks the projects and herdr's config, so a sandbox on it would act on his.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};

use crate::lib::args::BenchUsageError;
use jocasta::frontier::server::launch::{OnOff, service_health};
use jocasta::shared::bridge_wire::{CliPeer, MAX_LINE, Stream, VERSION, bridge_address, connect, line_of};
use jocasta::shared::env;
use jocasta::shared::errors::{Context, Error, Result};
use jocasta::shared::layer::{HOSTS_ENV, LAYER_ENV};
use jocasta::shared::line_rpc::LineReader;
use jocasta::shared::pathkey::resolve;
use jocasta::shared::paths::default_data_dir;
use jocasta::shared::process::{RunOptions, run_program_sync};
use jocasta::shared::service_file::read_service_file;

const HIS_SESSIONS: [&str; 2] = ["jocasta", "jarvis"];

/// The bench herdr session the competitors' panes run in (H5, H6, the task benchmark's plan-review turns).
pub const BENCH_PTY_SESSION: &str = "bench-pty";

/// 0481's VM: a throwaway Windows 11 machine, the only one the scripts that run commands or uninstall touch.
pub const VM_MACHINE: &str = "JOCASTA-CLEAN";

/// This machine's name, as Node's `os.hostname()` gave it: what the VM checks compare with [`VM_MACHINE`].
pub fn machine_name() -> String {
	if cfg!(windows) {
		return env::var_nonempty("COMPUTERNAME").unwrap_or_default();
	}
	run_program_sync(&["hostname"], RunOptions { timeout_ms: Some(30_000), ..RunOptions::default() })
		.map(|run| run.stdout.trim().to_owned())
		.ok()
		.filter(|name| !name.is_empty())
		.or_else(|| env::var_nonempty("HOSTNAME"))
		.unwrap_or_default()
}

/// Refuses unless this machine is the VM (`what` names what would run).
pub fn require_vm(machine: &str, what: &str) -> Result<()> {
	if machine.to_uppercase() != VM_MACHINE {
		return Err(BenchUsageError::err(format!("refusing to {what} on {machine}: only on the VM {VM_MACHINE}")));
	}
	Ok(())
}

/// Whether `session` is one of his own herdr sessions.
fn is_his_session(session: &str) -> bool {
	HIS_SESSIONS.contains(&session.to_lowercase().as_str())
}

/// Refuses his own herdr session for a bench flag or variable (`what` names it in the refusal).
pub fn refuse_his_session(session: &str, what: &str) -> Result<()> {
	if is_his_session(session) {
		return Err(BenchUsageError::err(format!("{what} can't be {session}")));
	}
	Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
	pub data_dir: String,
	pub herdr_session: String,
	pub config_dir: String,
}

fn home_dir() -> PathBuf {
	std::env::home_dir().unwrap_or_default()
}

/// A path as compared: resolved, a leading `~` as the home folder, case-insensitive on Windows (`windows`).
fn path_key(path: &str, windows: bool) -> String {
	let expanded = if path == "~" {
		home_dir()
	} else if path.starts_with("~/") || path.starts_with("~\\") {
		home_dir().join(&path[2..])
	} else {
		PathBuf::from(path)
	};
	let resolved = resolve(&expanded).to_string_lossy().into_owned();
	if windows { resolved.to_lowercase() } else { resolved }
}

/// The sandbox the process's environment names; refuses anything else ([`require_sandbox_in`]).
pub fn require_sandbox() -> Result<Sandbox> {
	let given: HashMap<String, String> = ["JOCASTA_DATA_DIR", "JOCASTA_HERDR_SESSION", "JOCASTA_CODING_AGENT_DIR"]
		.iter()
		.filter_map(|name| Some(((*name).to_owned(), env::var_nonempty(name)?)))
		.collect();
	require_sandbox_in(&given, cfg!(windows), &default_data_dir())
}

/// The sandbox `env` names, for a host that compares paths as `windows` says; refuses a data folder that is
/// `his_data_dir`, one of his herdr sessions, and the config folder `~/.jocasta/agent`.
pub fn require_sandbox_in(env: &HashMap<String, String>, windows: bool, his_data_dir: &Path) -> Result<Sandbox> {
	let get = |name: &str| env.get(name).filter(|value| !value.is_empty());
	let data_dir = get("JOCASTA_DATA_DIR");
	let herdr_session = get("JOCASTA_HERDR_SESSION");
	let config_dir = get("JOCASTA_CODING_AGENT_DIR");
	let Some(data_dir) = data_dir else {
		return Err(BenchUsageError::err(
			"set JOCASTA_DATA_DIR to the sandbox's data folder; this benchmark acts on a running Jocasta",
		));
	};
	let his = his_data_dir.to_string_lossy();
	if path_key(data_dir, windows) == path_key(&his, windows) {
		return Err(BenchUsageError::err(format!(
			"set JOCASTA_DATA_DIR to the sandbox's data folder, not his own ({his})"
		)));
	}
	let Some(herdr_session) = herdr_session.filter(|session| !is_his_session(session)) else {
		return Err(BenchUsageError::err(
			"set JOCASTA_HERDR_SESSION to a sandbox herdr session (not jocasta or jarvis)",
		));
	};
	let his_config = home_dir().join(".jocasta").join("agent").to_string_lossy().into_owned();
	let Some(config_dir) = config_dir.filter(|dir| path_key(dir, windows) != path_key(&his_config, windows)) else {
		return Err(BenchUsageError::err(
			"set JOCASTA_CODING_AGENT_DIR to the sandbox's config folder; with ~/.jocasta/agent the sandbox uses his projects and herdr config",
		));
	};
	Ok(Sandbox { data_dir: data_dir.clone(), herdr_session: herdr_session.clone(), config_dir: config_dir.clone() })
}

/// What the service's bridge answers to a tool call or a `jocasta` command: a `result` line's text (or error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReply {
	pub text: String,
	pub is_error: Option<bool>,
}

impl ToolReply {
	fn of_result(result: &Value) -> Self {
		let ok = result.get("ok").and_then(Value::as_bool) != Some(false);
		let key = if ok { "text" } else { "error" };
		Self {
			text: result.get(key).and_then(Value::as_str).unwrap_or_default().to_owned(),
			is_error: (!ok).then_some(true),
		}
	}
}

/// The running service `<data>/jocasta-service.json` names (`JOCASTA_DATA_DIR` picks the sandbox's).
fn service() -> Result<(u16, String)> {
	let running = read_service_file()
		.ok_or_else(|| Error::other("no running service: <data>/jocasta-service.json is missing"))?;
	Ok((running.port, running.token))
}

/// One `jocasta thread|worktree` command against the running service, as the CLI makes it: a `cli` message on the
/// bridge, opened with the service's token from `<data>/jocasta-service.json` (0675).
pub(crate) async fn cli_request(command: &str, args: Map<String, Value>) -> Result<ToolReply> {
	let (port, token) = service()?;
	let mut peer = CliPeer::open(port, &token).await.context("connecting to the service's bridge")?;
	let result = peer.request(json!({ "t": "cli", "command": command, "args": args })).await?;
	Ok(ToolReply::of_result(&result))
}

/// A pane's omp as the service sees it over the bridge: a hello with the pane's key (0578), which the service keeps in
/// `<data>/panes.json`, then the pane's tool calls. The hello takes the real omp's place on the bridge while this
/// session lasts (a pane's omp that started again does), so a sandbox pane is only queried, never prompted, meanwhile.
pub struct PaneSession {
	read: LineReader<ReadHalf<Stream>>,
	write: WriteHalf<Stream>,
	next: u64,
}

impl PaneSession {
	/// A pane session for `pane_key` (the pane's key from `<data>/panes.json`): a hello on the bridge, ready to `call`.
	pub async fn open(pane_key: &str) -> Result<Self> {
		let (port, _) = service()?;
		let stream = connect(&bridge_address(port)).await.context("connecting to the service's bridge")?;
		let (read, write) = tokio::io::split(stream);
		let mut session = Self { read: LineReader::new(read, MAX_LINE), write, next: 0 };
		session.send(&json!({ "t": "hello", "v": VERSION, "key": pane_key, "harness": "jocasta-bench" })).await?;
		if session.message().await?.get("t").and_then(Value::as_str) != Some("welcome") {
			return Err(Error::other("the bridge didn't welcome the pane's key"));
		}
		Ok(session)
	}

	async fn send(&mut self, message: &Value) -> Result<()> {
		self.write.write_all(line_of(message).as_bytes()).await?;
		self.write.flush().await?;
		Ok(())
	}

	async fn message(&mut self) -> Result<Value> {
		loop {
			match self.read.next_line().await {
				Ok(Some(line)) => {
					if let Ok(message) = serde_json::from_str::<Value>(&line) {
						return Ok(message);
					}
				}
				Ok(None) => {
					return Err(Error::other("the bridge closed the connection: not a pane key of the service?"));
				}
				Err(error) => return Err(Error::other(error.to_string())),
			}
		}
	}

	/// One tool call of the pane (`call`), as the pane's omp makes it; resolves its `result`.
	pub async fn call(&mut self, tool: &str, args: Map<String, Value>) -> Result<ToolReply> {
		self.next += 1;
		let id = self.next.to_string();
		self.send(&json!({ "t": "call", "id": id, "tool": tool, "args": args })).await?;
		loop {
			let answer = self.message().await?;
			if answer.get("t").and_then(Value::as_str) == Some("result") && answer.get("id") == Some(&json!(id)) {
				return Ok(ToolReply::of_result(&answer));
			}
		}
	}
}

/// The two test-only switches a sandbox service reports in `/health` (0576), with the variable that turns each off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
	Layer,
	Hosts,
}

struct SwitchWords {
	env: &'static str,
	what: &'static str,
	off: &'static str,
	on: &'static str,
}

impl Switch {
	fn words(self) -> SwitchWords {
		match self {
			Self::Layer => SwitchWords { env: LAYER_ENV, what: "the layer", off: "--layer off", on: "--layer on" },
			Self::Hosts => SwitchWords {
				env: HOSTS_ENV,
				what: "omp's host registry",
				off: "--condition herdr",
				on: "--condition host",
			},
		}
	}
}

fn on_off_word(value: OnOff) -> &'static str {
	match value {
		OnOff::On => "on",
		OnOff::Off => "off",
	}
}

/// Refuses a run whose flag doesn't match the sandbox service's switch as `/health` reports it (0576): H8's `--layer`
/// against `layer`, H2's `--condition` against `hosts`. The flag can't switch the service; the operator starts it with
/// the variable.
pub async fn require_service_switch(key: Switch, wanted: OnOff) -> Result<()> {
	let health = service_health(read_service_file().as_ref(), None).await;
	if health.state != jocasta::frontier::server::launch::ServiceState::Up {
		return Err(BenchUsageError::err("no running sandbox service answers /health"));
	}
	let actual = match key {
		Switch::Layer => health.layer,
		Switch::Hosts => health.hosts,
	};
	let words = key.words();
	let Some(actual) = actual else {
		return Err(BenchUsageError::err(format!(
			"the sandbox service's /health doesn't say whether {} is on (a build before 0576)",
			words.what
		)));
	};
	if actual != wanted {
		let (wanted, actual) = (on_off_word(wanted), on_off_word(actual));
		let flag = if wanted == "off" { words.off } else { words.on };
		return Err(BenchUsageError::err(format!(
			"{flag} needs {} {wanted}, but the sandbox service's /health says {actual}; start the service with {}=off for {}, without it for {}",
			words.what, words.env, words.off, words.on
		)));
	}
	Ok(())
}
