//! The per-project omp overlay (0399, 0400) every project and thread pane loads after the guard.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use futures_util::future::{BoxFuture, FutureExt, Shared};
use serde_json::Value;

use super::lessons::ensure_lessons_skill;

use crate::shared::errors::{Error, Result};
use crate::shared::fsx::temp_beside;
use crate::shared::paths::{lessons_dir, project_dir};

/// `<data>/projects/<pid>/omp.yml` (0399, 0400): the per-project omp overlay every project and thread pane loads.
pub fn overlay_file(project_id: &str) -> PathBuf {
	project_dir(project_id).join("omp.yml")
}

/// A string as JSON writes it (`JSON.stringify`).
fn quoted(value: &str) -> String {
	Value::String(value.to_owned()).to_string()
}

/// Where `/share` uploads go in Jocasta's panes (0536, 0561): a loopback port nothing listens on since the relay went
/// (0675), so a share is refused and never leaves this machine.
const SHARE_SINK_PORT: u16 = 47_499;

fn share_overlay() -> Vec<String> {
	vec!["share:".to_owned(), format!("  serverUrl: {}", quoted(&format!("http://127.0.0.1:{SHARE_SINK_PORT}/s")))]
}

/// The overlay's text: omp's memory off (0606); no completion probe and no job widgets, which only the boss keeps
/// (0601, 0602); a subagent starts no subagents of its own (0619); new sessions in plan mode; compaction at 200k tokens (0677),
/// a handoff first (0399, 0448, 0592); private notes from `AGENTS.local.md` (0449); the project's lessons as the skill
/// `lessons` (0459: omp lists `<dir>/<name>/SKILL.md`, so the project folder is the skill directory and `lessons/` its
/// one skill); no reviewer model, so omp's bundled reviewer runs on his `slow` role (0488); the privacy settings,
/// whatever his global config says (0478): no Auto QA
/// reports, no Claude Code plugins, web search on DuckDuckGo (an overlay's `modelRoles.web` leaves his other roles
/// alone; its `disabledProviders` replaces his list); `/share` kept on this machine (0536, 0561, 0675). Keys checked
/// against omp 19.2.0's source (`memory-backend/settings.ts`,
/// `task/settings.ts`, `tools/settings.ts`). Paths are JSON strings, valid YAML.
pub fn overlay_text(project_id: &str) -> String {
	let skills = lessons_dir(project_id).parent().map(Path::to_path_buf).unwrap_or_default();
	let mut lines: Vec<String> = [
		"memory:",
		"  backend: \"off\"",
		"task:",
		"  completionProbe: false",
		"  maxRecursionDepth: 1",
		"async:",
		"  widgets: false",
		"plan:",
		"  defaultOnStartup: true",
		"compaction:",
		"  methodOrder: [handoff, remote, soft, shake, snapcompact]",
		"  thresholdTokens: 200000",
		"  idleEnabled: true",
		"  idleThresholdTokens: 100000",
		"contextFiles:",
		"  extra: [AGENTS.local.md]",
		"skills:",
	]
	.iter()
	.map(|line| (*line).to_owned())
	.collect();
	lines.push(format!("  customDirectories: [{}]", quoted(&skills.display().to_string())));
	lines.extend(
		["dev:", "  autoqa: false", "disabledProviders: [claude-plugins]", "modelRoles:", "  web: web/duckduckgo"]
			.iter()
			.map(|line| (*line).to_owned()),
	);
	lines.extend(share_overlay());
	lines.push(String::new());
	lines.join("\n")
}

/// How often, and how far apart, a refused overlay rename is tried again (0534): 10 times, 100 ms apart.
const RENAME_TRIES: u32 = 10;
const RENAME_RETRY_MS: u64 = 100;

/// Errors Windows gives a rename over a file another process has open (omp reading the overlay as it starts, 0534).
fn rename_busy(error: &io::Error) -> bool {
	matches!(error.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy)
		|| (cfg!(windows) && error.raw_os_error().is_some_and(|code| matches!(code, 5 | 32 | 33)))
}

type Write = Shared<BoxFuture<'static, std::result::Result<PathBuf, String>>>;

/// Each project's overlay write under way (0534), shared by every start that needs it meanwhile.
static OVERLAY_WRITES: LazyLock<Mutex<HashMap<String, Write>>> = LazyLock::new(Mutex::default);

fn writes() -> std::sync::MutexGuard<'static, HashMap<String, Write>> {
	OVERLAY_WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The project's overlay, written when it differs, and its lessons skill (0459); resolves the overlay's path. One write
/// per project at a time: a start that comes while one runs waits for that one (0534). An overlay that already holds
/// the text isn't written again, so parallel starts in one project never replace the file their omps are reading; a
/// rename Windows refuses (EPERM, EBUSY, EACCES: an omp has it open) is tried again for about a second. The waits are
/// async, not `write_atomically`'s: the service's event loop goes on while an omp holds the file.
pub async fn ensure_overlay(project_id: &str) -> Result<PathBuf> {
	let write = {
		let mut running = writes();
		match running.get(project_id) {
			Some(write) => write.clone(),
			None => {
				let id = project_id.to_owned();
				let write: Write = async move {
					let done = write_overlay(&id).await.map_err(|error| error.to_string());
					writes().remove(&id);
					done
				}
				.boxed()
				.shared();
				running.insert(project_id.to_owned(), write.clone());
				write
			}
		}
	};
	write.await.map_err(Error::other)
}

async fn write_overlay(project_id: &str) -> Result<PathBuf> {
	let overlay = overlay_file(project_id);
	ensure_lessons_skill(project_id, None)?;
	let text = overlay_text(project_id);
	if fs::read_to_string(&overlay).is_ok_and(|existing| existing == text) {
		return Ok(overlay);
	}
	if let Some(dir) = overlay.parent() {
		fs::create_dir_all(dir)?;
	}
	let temp = temp_beside(&overlay);
	let written = swap_in(&temp, &overlay, &text).await;
	if written.is_err() {
		let _ = fs::remove_file(&temp);
	}
	written?;
	Ok(overlay)
}

async fn swap_in(temp: &Path, overlay: &Path, text: &str) -> Result<()> {
	fs::write(temp, text)?;
	let mut attempt = 1;
	loop {
		match fs::rename(temp, overlay) {
			Ok(()) => return Ok(()),
			Err(error) if attempt < RENAME_TRIES && rename_busy(&error) => {
				attempt += 1;
				tokio::time::sleep(Duration::from_millis(RENAME_RETRY_MS)).await;
			}
			Err(error) => return Err(error.into()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn the_overlay_text_names_the_projects_skill_folder_and_keeps_share_on_this_machine() {
		let _j = temp_jocasta();
		let text = overlay_text("acme");
		assert!(text.starts_with("memory:\n  backend: \"off\"\n"));
		assert!(
			text.contains(&format!("  customDirectories: [{}]", quoted(&project_dir("acme").display().to_string())))
		);
		assert!(text.contains("share:\n  serverUrl: \"http://127.0.0.1:47499/s\""));
		assert!(!text.contains("collab"));
		assert!(text.ends_with("\n"));
	}

	#[tokio::test]
	async fn ensure_overlay_writes_once_and_parallel_starts_share_one_write() {
		let _j = temp_jocasta();
		let (a, b) = tokio::join!(ensure_overlay("acme"), ensure_overlay("acme"));
		let file = overlay_file("acme");
		assert_eq!((a.unwrap(), b.unwrap()), (file.clone(), file.clone()));
		assert_eq!(fs::read_to_string(&file).unwrap(), overlay_text("acme"));
		assert!(lessons_dir("acme").join("SKILL.md").exists());
		let modified = fs::metadata(&file).unwrap().modified().unwrap();
		ensure_overlay("acme").await.unwrap();
		assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
		assert!(writes().is_empty());
	}
}
