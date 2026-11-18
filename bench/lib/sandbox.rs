//! The live benchmarks (H2–H6, H8) act on a running Jocasta: they kill its processes, start threads and plant facts.
//! They run only against a sandbox (the 0441 walkthrough's, or 0481's VM) with its own data folder, herdr session and
//! config folder (0502, 0576): `JOCASTA_DATA_DIR` set and not the OS default, `JOCASTA_HERDR_SESSION` naming a session
//! that isn't his own (`jocasta`, or `jarvis` before the cutover), and `JOCASTA_CODING_AGENT_DIR` set and not
//! `~/.jocasta/agent`: the config folder picks the projects and herdr's config, so a sandbox on it would act on his.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::lib::args::BenchUsageError;
use crate::lib::output::json_text;
use jocasta::frontier::server::launch::{OnOff, service_health};
use jocasta::shared::env;
use jocasta::shared::errors::{Context, Error, Result};
use jocasta::shared::layer::{HOSTS_ENV, LAYER_ENV};
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

/// What the service's MCP and CLI endpoints answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReply {
	pub text: String,
	#[serde(default)]
	pub is_error: Option<bool>,
}

/// The running service `<data>/jocasta-service.json` names (`JOCASTA_DATA_DIR` picks the sandbox's).
fn service() -> Result<(u16, String)> {
	let running = read_service_file()
		.ok_or_else(|| Error::other("no running service: <data>/jocasta-service.json is missing"))?;
	Ok((running.port, running.token))
}

/// A client for the loopback service: no proxy, no time limit (a start waits for omp to come up in its pane).
pub(crate) fn http_client() -> Result<reqwest::Client> {
	reqwest::Client::builder().no_proxy().build().context("building the HTTP client")
}

/// POSTs `body` (JSON) to the running service with its token as a bearer; the reply as a [`ToolReply`].
pub(crate) async fn post_to_service(path: &str, body: &Value) -> Result<ToolReply> {
	let (port, token) = service()?;
	let response = http_client()?
		.post(format!("http://127.0.0.1:{port}{path}"))
		.bearer_auth(token)
		.header("content-type", "application/json")
		.body(json_text(body, false))
		.send()
		.await?;
	Ok(response.json().await?)
}

/// `encodeURIComponent`.
fn encode_uri_component(text: &str) -> String {
	let mut out = String::new();
	for byte in text.bytes() {
		if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
			out.push(char::from(byte));
		} else {
			out.push_str(&format!("%{byte:02X}"));
		}
	}
	out
}

/// Who a tool call is made for: a pane's tools name the pane by its key (0578).
#[derive(Debug, Clone, Default)]
pub struct Caller {
	pub pane_key: Option<String>,
	pub boss_key: Option<String>,
}

/// One call to the running service's MCP endpoint, as `jocasta mcp` makes it (`POST /api/mcp/<tool>`, bearer token from
/// `<data>/jocasta-service.json`, body `{args, paneKey?, bossKey?}`): a pane's tools name the pane by its key (0578), which
/// the service keeps in `<data>/panes.json`.
pub async fn call_tool(tool: &str, args: Map<String, Value>, caller: &Caller) -> Result<ToolReply> {
	let mut body = Map::new();
	body.insert("args".to_owned(), Value::Object(args));
	if let Some(key) = &caller.pane_key {
		body.insert("paneKey".to_owned(), Value::String(key.clone()));
	}
	if let Some(key) = &caller.boss_key {
		body.insert("bossKey".to_owned(), Value::String(key.clone()));
	}
	post_to_service(&format!("/api/mcp/{}", encode_uri_component(tool)), &Value::Object(body)).await
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
