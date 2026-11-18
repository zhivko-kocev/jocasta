//! `jocasta diagnose` (0472): a read-only omp pane in workspace "Jocasta" that explains what went wrong from Jocasta's own
//! evidence and changes nothing. Its folder `<data>/diagnose/` holds `state.md` (the service's state now), `recent.log`
//! (the newest lines of Jocasta's log, 0431) and `omp.yml`, the pane's settings: plan mode from the start and the
//! shipped `config/skills/` (the diagnose skill). omp runs with read, grep and glob only, the two files attached to
//! its first message, so the log and the state (project names, pane folders, repo paths) go to his model provider with
//! it (0484); the pane's first line says so.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::frontier::server::herdr::{Herdr, HerdrSnapshot, PaneInfo, TabSpec};
use crate::frontier::server::launch::ServiceState;
use crate::setup::setup::SetupCheck;
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LOG_DAYS, LOG_FILE};
use crate::shared::service_file::ServiceFile;

/// The boss's workspace (0392, 0404), where the diagnose pane opens.
const WORKSPACE: &str = "Jocasta";

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

/// Starts (or reaches) Jocasta's herdr session.
pub type Connect = Box<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn Herdr>>> + Send + Sync>;

/// Makes the boss's workspace and pane when they are missing.
pub type EnsureBoss = Box<dyn Fn(Arc<dyn Herdr>) -> BoxFuture<'static, Result<PaneInfo>> + Send + Sync>;

pub struct DiagnoseDeps {
	pub data_dir: PathBuf,
	/// The shipped `config/`, whose `skills/` holds the diagnose skill.
	pub assets_dir: PathBuf,
	/// How omp starts (`["omp"]`).
	pub omp: Vec<String>,
	/// `jocasta --version`'s line.
	pub versions: String,
	pub session: String,
	pub service: Box<dyn Fn() -> BoxFuture<'static, Result<ServiceInfo>> + Send + Sync>,
	/// Jocasta's herdr session, started when it isn't running.
	pub connect: Connect,
	/// Makes workspace "Jocasta" with its boss pane when it is missing.
	pub ensure_boss: EnsureBoss,
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

fn panes_section(snapshot: &HerdrSnapshot) -> Vec<String> {
	let mut lines = Vec::new();
	for workspace in &snapshot.workspaces {
		let cwd = workspace.cwd.as_ref().map_or(String::new(), |cwd| format!(" in {}", cwd.display()));
		lines.push(format!("- {} ({}){cwd}", workspace.label, workspace.id));
		for pane in snapshot.panes.iter().filter(|pane| pane.workspace_id == workspace.id) {
			let agent = match pane.agent.as_deref().filter(|agent| !agent.is_empty()) {
				Some(agent) => format!("{agent} {}", pane.agent_state.as_str()),
				None => format!("no agent ({})", pane.agent_state.as_str()),
			};
			let cwd = pane.cwd.as_ref().map_or(String::new(), |cwd| format!(", {}", cwd.display()));
			let label =
				pane.label.as_deref().filter(|label| !label.is_empty()).map_or(String::new(), |l| format!(", \"{l}\""));
			lines.push(format!("  - pane {}: {agent}{cwd}{label}", pane.pane_id));
		}
	}
	if lines.is_empty() { vec!["- none".to_owned()] } else { lines }
}

/// Writes the pane's folder and opens the pane; resolves the exit code.
pub async fn run_diagnose(deps: &DiagnoseDeps, print: &(dyn Fn(&str) + Send + Sync)) -> Result<i32> {
	let logs_dir = deps.data_dir.join("logs");
	let service = (deps.service)().await?;
	let herdr = (deps.connect)().await.map_err(|error| {
		Error::user_with_code(
			format!(
				"herdr session `{}` doesn't answer and couldn't start ({error}); Jocasta's log is in {}",
				deps.session,
				logs_dir.display()
			),
			1,
		)
	})?;
	let result = open_pane(deps, &service, &herdr, print).await;
	herdr.close();
	result
}

async fn open_pane(
	deps: &DiagnoseDeps,
	service: &ServiceInfo,
	herdr: &Arc<dyn Herdr>,
	print: &(dyn Fn(&str) + Send + Sync),
) -> Result<i32> {
	let now = deps.now.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
	let at = chrono::DateTime::from_timestamp_millis(now).unwrap_or_default();
	let dir = deps.data_dir.join("diagnose");
	let logs_dir = deps.data_dir.join("logs");
	let version = herdr.ping().await?;
	let snapshot = herdr.snapshot().await?;
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
		vec![
			format!(
				"# Jocasta's state for `jocasta diagnose` ({})",
				at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
			),
			String::new(),
			format!("- Versions: {}", deps.versions),
			format!("- Service: {}", service_line(service)),
			format!(
				"- herdr session `{}`: herdr {} answers; {} workspaces, {} panes",
				deps.session,
				version.as_deref().unwrap_or("?"),
				snapshot.workspaces.len(),
				snapshot.panes.len()
			),
			format!("- Setup: {setup_line}"),
			format!(
				"- Logs: {}, one file per day, {LOG_DAYS} days kept; recent.log has the newest {RECENT_LOG_LINES} lines{from}",
				logs_dir.display()
			),
			String::new(),
			"## herdr's workspaces and panes".to_owned(),
		],
		panes_section(&snapshot),
		vec![String::new()],
	]
	.concat()
	.join("\n");
	fs::write(&state_path, state)?;
	fs::write(&log_path, &recent.text)?;
	fs::write(&overlay, diagnose_overlay(&deps.assets_dir.join("skills")))?;
	let workspace = match snapshot.workspaces.iter().find(|workspace| workspace.label == WORKSPACE) {
		Some(workspace) => workspace.id.clone(),
		None => (deps.ensure_boss)(herdr.clone()).await?.workspace_id,
	};
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
	let tab =
		herdr.create_tab(&workspace, &TabSpec { title: "diagnose".to_owned(), cwd: dir, command, env: None }).await?;
	herdr.focus(&tab.pane_id).await?;
	print(&format!(
		"Opened a read-only diagnose pane in workspace {WORKSPACE} (herdr session `{}`): run `jocasta` to see it.",
		deps.session
	));
	Ok(0)
}

#[cfg(test)]
#[path = "diagnose_tests.rs"]
mod tests;
