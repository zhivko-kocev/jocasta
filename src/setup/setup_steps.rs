//! What one migration would do now, and the helpers that build a step: new files, owned files and MCP entries (0466,
//! 0484, 0584).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde_json::Value;

use super::Apply;
use crate::setup::diff::line_diff;
use crate::setup::markers::{Marker, Undo, fingerprint};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;

/// One change setup proposes: its diff lines (`+ `, `- `, `  `) and how to make it.
pub(super) struct SetupChange {
	pub title: String,
	pub path: String,
	pub diff: Vec<String>,
	/// What of his stays as it is, said under the diff.
	pub kept: Vec<String>,
	pub apply: Apply,
}

/// What one migration would do now.
pub(super) struct Step {
	/// The file it is about.
	pub path: String,
	/// What Jocasta ships for it on this machine: a new one makes the migration pending again.
	pub fingerprint: String,
	/// `None` when the target already is what Jocasta ships.
	pub change: Option<SetupChange>,
	pub undo: Vec<Undo>,
}

impl Step {
	pub(super) fn new(path: &Path, fingerprint: impl Into<String>, undo: Vec<Undo>) -> Self {
		Self { path: shown(path), fingerprint: fingerprint.into(), change: None, undo }
	}
}

/// A path as the diff and the markers name it.
pub(super) fn shown(path: &Path) -> String {
	path.display().to_string()
}

/// `value` as `JSON.stringify(value, null, 2)` plus a line break.
pub(super) fn json(value: &Value) -> String {
	format!("{}\n", serde_json::to_string_pretty(value).unwrap_or_default())
}

pub(super) fn read_text(path: &Path) -> Result<Option<String>> {
	match fs::read(path) {
		Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
		Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
		Err(error) => Err(error.into()),
	}
}

/// Fails while `path` is no longer `has` (`None`: still absent), the text its diff was made from: a change made after
/// the diff (omp saving a setting, `/mcp add`) is never overwritten (SET-4, 0584).
pub(super) fn unchanged(path: &Path, has: Option<&str>) -> Result<()> {
	if read_text(path)?.as_deref() != has {
		return Err(Error::other(format!("{} changed since the diff; run jocasta setup again", path.display())));
	}
	Ok(())
}

/// Writes `text` to `path` only while the file still is `has` ([`unchanged`]).
pub(super) fn write_if_unchanged(path: &Path, has: Option<&str>, text: &str) -> Result<()> {
	unchanged(path, has)?;
	write_atomically(path, text, None)
}

/// An apply step that writes `text` to `path` while it still is `has`.
pub(super) fn writes(path: &Path, has: Option<String>, text: String) -> Apply {
	let path = path.to_path_buf();
	Box::new(move |_| Box::pin(async move { write_if_unchanged(&path, has.as_deref(), &text) }))
}

/// A new file: every line added.
pub(super) fn new_file(title: &str, path: &Path, text: &str) -> SetupChange {
	SetupChange {
		title: title.to_owned(),
		path: shown(path),
		diff: text.strip_suffix('\n').unwrap_or(text).split('\n').map(|line| format!("+ {line}")).collect(),
		kept: Vec::new(),
		apply: writes(path, None, text.to_owned()),
	}
}

/// How `file` in folder `dir` is undone (0484): the whole folder only when setup makes it now, or its marker says setup
/// made it; else only the file, so a folder that was his stays.
pub(super) fn folder_undo(dir: &Path, file: &str, marker: Option<&Marker>) -> Undo {
	let dir_text = shown(dir);
	let made = !dir.exists()
		|| marker.is_some_and(|m| {
			m.undo.iter().any(|u| matches!(u, Undo::Dir { path, file: Some(f) } if *path == dir_text && f == file))
		});
	if made {
		Undo::Dir { path: dir_text, file: Some(file.to_owned()) }
	} else {
		Undo::File { path: shown(&dir.join(file)) }
	}
}

/// A file Jocasta owns, following `wanted`: new when missing, its diff when it differs, `None` when it matches.
pub(super) fn owned_file(title: &str, path: &Path, wanted: &str) -> Result<Option<SetupChange>> {
	let Some(has) = read_text(path)? else { return Ok(Some(new_file(title, path, wanted))) };
	if has == wanted {
		return Ok(None);
	}
	Ok(Some(SetupChange {
		title: title.to_owned(),
		path: shown(path),
		diff: line_diff(&has, wanted),
		kept: Vec::new(),
		apply: writes(path, Some(has), wanted.to_owned()),
	}))
}

/// A file Jocasta owns as a migration's step: `wanted` is its fingerprint and its text.
pub(super) fn owned_step(title: &str, path: &Path, wanted: &str, undo: Vec<Undo>) -> Result<Step> {
	let mut step = Step::new(path, fingerprint(wanted), undo);
	step.change = owned_file(title, path, wanted)?;
	Ok(step)
}
