//! Jocasta's omp snapshot (fork plan phase 6, migration 19): the files in `config/omp/`, an `mcp.json` made for this
//! install, each written over the one in omp's folder after the diff and his `y`. Every file it
//! replaces is kept first in `~/.jocasta/agent/setup/omp-backup/<UTC time>/`, and uninstall puts it back while the file
//! is still the snapshot's text. `secrets.yml` holds his own strings: it is written only when missing and never shown.
//! Logins, sessions, memory and history are files the snapshot doesn't have, so they are never touched.

use std::fs;
use std::path::{Path, PathBuf};

use super::SetupPaths;
use super::migrations::{PlanContext, asset};
use super::steps::{SetupChange, Step, read_text, shown, unchanged};
use crate::setup::diff::line_diff;
use crate::setup::markers::{Marker, Undo, fingerprint, markers_dir};
use crate::setup::omp_settings::omp_settings_text;
use crate::shared::errors::Result;
use crate::shared::fsx::write_atomically;

/// One file of the snapshot: where it goes in omp's folder (`/`-separated) and its text.
struct SnapshotFile {
	relative: String,
	text: String,
	/// His own once there (`secrets.yml`): written only when missing.
	only_when_missing: bool,
}

/// (0594). herdr's own omp hook is no longer written (0683).
fn snapshot_files(paths: &SetupPaths) -> Result<Vec<SnapshotFile>> {
	let shipped = |relative: &str| asset(paths, &["omp", relative]);
	let file =
		|relative: &str, text: String| SnapshotFile { relative: relative.to_owned(), text, only_when_missing: false };
	Ok(vec![
		file("config.yml", omp_settings_text(&shipped("config.yml")?, paths.system_folders.as_deref())?),
		file("keybindings.yml", shipped("keybindings.yml")?),
		file("lsp.json", shipped("lsp.json")?),
		file("WATCHDOG.md", shipped("WATCHDOG.md")?),
		file("skills/mise/SKILL.md", asset(paths, &["omp", "skills", "mise", "SKILL.md"])?),
		SnapshotFile { relative: "secrets.yml".to_owned(), text: shipped("secrets.yml")?, only_when_missing: true },
	])
}

fn under(dir: &Path, relative: &str) -> PathBuf {
	relative.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The restore an earlier snapshot recorded for `path`: its backup is the file from before Jocasta's first snapshot.
fn earlier<'a>(marker: Option<&'a Marker>, path: &str) -> Option<&'a Undo> {
	marker?.undo.iter().find(|undo| matches!(undo, Undo::Restore { path: restored, .. } if restored == path))
}

/// A file the snapshot writes now: its text there (`None`: missing), the snapshot's, and where the old one is kept.
struct Write {
	path: PathBuf,
	has: Option<String>,
	text: String,
	backup: Option<PathBuf>,
}

/// The snapshot's step: one change with every file that differs, or none when each already is the snapshot's.
pub(super) fn plan_snapshot(c: &mut PlanContext) -> Result<Option<Step>> {
	let files = snapshot_files(c.paths)?;
	let shipped: String = files.iter().map(|file| format!("{}\n{}", file.relative, file.text)).collect();
	let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
	let root = markers_dir(&c.paths.jocasta_config_dir).join("omp-backup");
	// A second run within the same second gets its own folder: an earlier backup is never overwritten.
	let backups = (1..)
		.map(|n| if n == 1 { root.join(&stamp) } else { root.join(format!("{stamp}-{n}")) })
		.find(|dir| !dir.exists())
		.unwrap_or_else(|| root.join(&stamp));
	let mut undo = Vec::new();
	let mut writes = Vec::new();
	let mut diff = Vec::new();
	for file in files {
		let path = under(&c.paths.omp_agent_dir, &file.relative);
		let shown_path = shown(&path);
		let before = earlier(c.marker.as_ref(), &shown_path);
		let has = read_text(&path)?;
		if file.only_when_missing && has.is_some() {
			undo.extend(before.cloned());
			continue;
		}
		if has.as_deref() == Some(file.text.as_str()) {
			// Already the snapshot's: an earlier snapshot's restore follows the text; an untouched equal file has none.
			if let Some(Undo::Restore { backup, .. }) = before {
				let backup = backup.clone();
				undo.push(Undo::Restore { path: shown_path, backup, fingerprint: fingerprint(&file.text) });
			}
			continue;
		}
		let backup = has.as_ref().map(|_| under(&backups, &file.relative));
		let first_backup = match before {
			Some(Undo::Restore { backup, .. }) => backup.clone(),
			_ => backup.as_deref().map(shown),
		};
		undo.push(Undo::Restore {
			path: shown_path.clone(),
			backup: first_backup,
			fingerprint: fingerprint(&file.text),
		});
		diff.push(format!("  {shown_path}{}", if has.is_some() { "" } else { " (new)" }));
		diff.extend(
			line_diff(has.as_deref().unwrap_or_default(), &file.text).into_iter().map(|line| format!("  {line}")),
		);
		writes.push(Write { path, has, text: file.text, backup });
	}
	let mut step = Step::new(&c.paths.omp_agent_dir, fingerprint(&shipped), undo);
	if writes.is_empty() {
		return Ok(Some(step));
	}
	step.change = Some(SetupChange {
		title: format!(
			"Jocasta's omp snapshot over your omp files, each replaced one kept first in {}",
			backups.display()
		),
		path: shown(&c.paths.omp_agent_dir),
		diff,
		kept: Vec::new(),
		apply: Box::new(move |_| Box::pin(async move { write_snapshot(&writes) })),
	});
	Ok(Some(step))
}

/// Writes the snapshot: nothing while any file changed since its diff was shown (SET-4), then each one it replaces is
/// copied to its backup, then the new text goes in.
fn write_snapshot(writes: &[Write]) -> Result<()> {
	for write in writes {
		unchanged(&write.path, write.has.as_deref())?;
	}
	for write in writes {
		if let Some(backup) = &write.backup {
			if let Some(dir) = backup.parent() {
				fs::create_dir_all(dir)?;
			}
			fs::copy(&write.path, backup)?;
		}
	}
	for write in writes {
		write_atomically(&write.path, &write.text, None)?;
	}
	Ok(())
}
