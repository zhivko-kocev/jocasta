//! `jocasta diagnose` (0472, 0683): a read-only omp session, in this terminal, that explains what went wrong from Jocasta's own
//! evidence and changes nothing. Its folder `<data>/diagnose/` holds `state.md` (the service's state now), `recent.log`
//! (the newest lines of Jocasta's log, 0431) and `omp.yml`, the pane's settings: plan mode from the start and the
//! shipped `config/skills/` (the diagnose skill). omp runs with read, grep and glob only, the two files attached to
//! its first message, so the log and the state (project names, pane folders, repo paths) go to his model provider with
//! it (0484); the pane's first line says so.

use std::fs;
use std::path::{Path, PathBuf};

use futures_util::future::BoxFuture;

use crate::frontier::server::launch::ServiceState;
use crate::frontier::server::phone::{PhoneMode, mode_of, read_topics};
use crate::setup::setup::SetupCheck;
use crate::shared::errors::Result;
use crate::shared::log::{LOG_DAYS, LOG_FILE};
use crate::shared::service_file::ServiceFile;
use crate::shared::settings::{NtfySettings, read_ntfy};

/// Lines of Jocasta's log `recent.log` holds.
pub const RECENT_LOG_LINES: usize = 400;

/// omp's tools in the pane: reading and searching only (plan mode adds its own guarded `write` for the plan). No `find`:
/// it's off (0451, `find.enabled`), and omp refuses to start when asked for a tool it doesn't have.
const READ_ONLY_TOOLS: &str = "read,grep,glob";

/// The service's state now and the file naming it, as `jocasta-service.json` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
	pub state: ServiceState,
	pub file: Option<ServiceFile>,
}

/// Runs omp (argv, folder) to the end in this terminal; resolves its exit code.
pub type RunOmp = Box<dyn Fn(Vec<String>, PathBuf) -> BoxFuture<'static, Result<i32>> + Send + Sync>;

pub struct DiagnoseDeps {
	pub data_dir: PathBuf,
	/// The shipped `config/`, whose `skills/` holds the diagnose skill.
	pub assets_dir: PathBuf,
	/// How omp starts (`["omp"]`).
	pub omp: Vec<String>,
	/// `jocasta --version`'s line.
	pub versions: String,
	pub service: Box<dyn Fn() -> BoxFuture<'static, Result<ServiceInfo>> + Send + Sync>,
	/// Runs the diagnose session.
	pub run_omp: RunOmp,
	/// Setup's start check (`setup_check`): what waits, or why it couldn't check.
	pub setup_check: Box<dyn Fn() -> BoxFuture<'static, SetupCheck> + Send + Sync>,
	/// The time written into `state.md` (epoch ms); now when absent.
	pub now: Option<i64>,
}

/// What `recent_log` read: the text and the files it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentLog {
	pub text: String,
	pub files: Vec<String>,
}

/// The newest `lines` lines across Jocasta's log files, oldest first, and the files they came from (`lines` defaults
/// to [`RECENT_LOG_LINES`]).
pub fn recent_log(logs_dir: &Path, lines: Option<usize>) -> Result<RecentLog> {
	let lines = lines.unwrap_or(RECENT_LOG_LINES);
	let mut names: Vec<String> = fs::read_dir(logs_dir)
		.map(|entries| {
			entries
				.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
				.filter(|name| LOG_FILE.is_match(name))
				.collect()
		})
		.unwrap_or_default();
	names.sort();
	// Newest first while collecting; each file's lines go in front of the newer ones.
	let mut kept: Vec<Vec<String>> = Vec::new();
	let mut files: Vec<String> = Vec::new();
	let mut count = 0;
	for name in names.into_iter().rev() {
		if count >= lines {
			break;
		}
		let text = String::from_utf8_lossy(&fs::read(logs_dir.join(&name))?).into_owned();
		let file_lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(&text).split('\n').collect();
		let from = file_lines.len().saturating_sub(lines - count);
		let take: Vec<String> = file_lines[from..].iter().map(|line| (*line).to_owned()).collect();
		count += take.len();
		kept.insert(0, take);
		files.insert(0, name);
	}
	let text = if kept.is_empty() { String::new() } else { format!("{}\n", kept.concat().join("\n")) };
	Ok(RecentLog { text, files })
}

/// The pane's omp settings: plan mode from the start, and the shipped skills (0472).
pub fn diagnose_overlay(skills_dir: &Path) -> String {
	let skills = serde_json::to_string(&skills_dir.display().to_string()).unwrap_or_default();
	[
		"# `jocasta diagnose` (0472): a read-only pane. Written by Jocasta each time; changes are lost.",
		"plan:",
		"  defaultOnStartup: true",
		"skills:",
		"  customDirectories:",
		&format!("    - {skills}"),
		"",
	]
	.join("\n")
}

fn state_name(state: ServiceState) -> &'static str {
	match state {
		ServiceState::Up => "up",
		ServiceState::Busy => "busy",
		ServiceState::Closing => "closing",
		ServiceState::Down => "down",
	}
}

fn service_line(service: &ServiceInfo) -> String {
	let state = state_name(service.state);
	match &service.file {
		None => format!("{state} (no jocasta-service.json: it isn't running)"),
		Some(file) => format!("{state} (jocasta-service.json names pid {}, port {})", file.pid, file.port),
	}
}

/// The `phone` line (0672), `None` with the phone off: the server, the mode and the main topic his phone's ntfy app
/// subscribes to. Printed for him alone and never written into `state.md`, which goes to his model provider. The token
/// is never shown; the reply topic isn't needed on the phone.
pub fn phone_line(ntfy: &NtfySettings, data_dir: &Path) -> Option<String> {
	let server = ntfy.server.as_deref()?;
	let mode = match mode_of(ntfy) {
		PhoneMode::Own => "own server",
		PhoneMode::Public => "public server",
	};
	let topic = match read_topics(data_dir) {
		Some(topics) => format!("main topic {}", topics.main),
		None => "the topics are made when Jocasta starts".to_owned(),
	};
	Some(format!("phone: {server}, {mode}, {topic} (a secret: don't paste this output)"))
}

/// Writes the diagnose folder and runs omp in it, here in this terminal; resolves omp's exit code (0472, 0683).
pub async fn run_diagnose(deps: &DiagnoseDeps, print: &(dyn Fn(&str) + Send + Sync)) -> Result<i32> {
	let service = (deps.service)().await?;
	let now = deps.now.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
	let at = chrono::DateTime::from_timestamp_millis(now).unwrap_or_default();
	let dir = deps.data_dir.join("diagnose");
	let logs_dir = deps.data_dir.join("logs");
	// Setup's check runs before the log is read, and a failed one is said as such (SET-14).
	let setup = (deps.setup_check)().await;
	let recent = recent_log(&logs_dir, None)?;
	let state_path = dir.join("state.md");
	let log_path = dir.join("recent.log");
	let overlay = dir.join("omp.yml");
	fs::create_dir_all(&dir)?;
	let setup_line = match (&setup.failed, &setup.waiting) {
		(Some(failed), _) => format!("couldn't check ({failed})"),
		(None, Some(waiting)) => waiting.clone(),
		(None, None) => "nothing waiting".to_owned(),
	};
	let from = if recent.files.is_empty() {
		" (there are none)".to_owned()
	} else {
		format!(" (from {})", recent.files.join(", "))
	};
	let state = [
		format!(
			"# Jocasta's state for `jocasta diagnose` ({})",
			at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
		),
		String::new(),
		format!("- Versions: {}", deps.versions),
		format!("- Service: {}", service_line(&service)),
		format!("- Setup: {setup_line}"),
		format!(
			"- Logs: {}, one file per day, {LOG_DAYS} days kept; recent.log has the newest {RECENT_LOG_LINES} lines{from}",
			logs_dir.display()
		),
		String::new(),
	]
	.join("\n");
	fs::write(&state_path, state)?;
	fs::write(&log_path, &recent.text)?;
	fs::write(&overlay, diagnose_overlay(&deps.assets_dir.join("skills")))?;
	let mut command = deps.omp.clone();
	command.extend([
		"--config".to_owned(),
		overlay.display().to_string(),
		"--tools".to_owned(),
		READ_ONLY_TOOLS.to_owned(),
		format!("@{}", state_path.display()),
		format!("@{}", log_path.display()),
		"Jocasta's state and recent log, attached, go to the model provider with this message. Diagnose Jocasta with the diagnose skill from them. Change nothing.".to_owned(),
	]);
	if let Some(line) = phone_line(&read_ntfy(), &deps.data_dir) {
		print(&line);
	}
	(deps.run_omp)(command, dir).await
}

#[cfg(test)]
#[path = "diagnose_tests.rs"]
mod tests;
