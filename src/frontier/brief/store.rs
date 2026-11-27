//! Briefs on disk (0678): `<data>/briefs/<id>.json`, written atomically and kept 30 days after their last write (0715).
//! Ids are `thread-<tid>`, `pr-<project id>-<n>`, `morning-<yyyy-mm-dd>`, `weekly-<yyyy>-W<ww>` and `show-<n>`.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::frontier::brief::model::Brief;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::paths::data_dir;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// `<data>/briefs`.
pub fn brief_dir() -> PathBuf {
	data_dir().join("briefs")
}

/// An id is letters, digits, `-` and `_` only, so it can never name a path outside the folder.
pub fn valid_id(id: &str) -> bool {
	!id.is_empty() && id.len() <= 120 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn file_of(id: &str) -> Result<PathBuf> {
	if !valid_id(id) {
		return Err(Error::user(format!("`{id}` isn't a brief id.")));
	}
	Ok(brief_dir().join(format!("{id}.json")))
}

/// Stores `brief` under its id, replacing the one that was there; then the briefs older than [`KEEP`] go (0715).
pub fn save(brief: &Brief) -> Result<()> {
	let text = serde_json::to_string_pretty(brief).map_err(|error| Error::other(error.to_string()))?;
	write_atomically(&file_of(&brief.id)?, text, None)?;
	prune(SystemTime::now());
	Ok(())
}

/// How long a brief is kept after it was last written (0715).
pub const KEEP: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Deletes every brief last written more than [`KEEP`] before `now`, with its screenshots
/// (`<data>/briefs/assets/<id>/`). A file it can't read or remove stays; the next save tries again.
fn prune(now: SystemTime) {
	let Some(cutoff) = now.checked_sub(KEEP) else { return };
	let dir = brief_dir();
	for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		let Some(id) = name.strip_suffix(".json").filter(|id| valid_id(id)) else { continue };
		let old = entry.metadata().and_then(|meta| meta.modified()).is_ok_and(|at| at < cutoff);
		if old && std::fs::remove_file(entry.path()).is_ok() {
			let _ = std::fs::remove_dir_all(dir.join("assets").join(id));
		}
	}
}

/// Whether a brief with this id is stored.
pub fn exists(id: &str) -> bool {
	file_of(id).is_ok_and(|file| file.is_file())
}

/// The brief with this id, or `None` when none is stored (or the file isn't a brief).
pub fn load(id: &str) -> Option<Brief> {
	let text = std::fs::read_to_string(file_of(id).ok()?).ok()?;
	serde_json::from_str(&text).ok()
}

/// The next free `show-<n>` id: one above the highest stored.
pub fn next_show_id() -> String {
	let highest = std::fs::read_dir(brief_dir())
		.into_iter()
		.flatten()
		.flatten()
		.filter_map(|entry| {
			let name = entry.file_name().to_string_lossy().into_owned();
			name.strip_prefix("show-")?.strip_suffix(".json")?.parse::<u64>().ok()
		})
		.max()
		.unwrap_or(0);
	format!("show-{}", highest + 1)
}
