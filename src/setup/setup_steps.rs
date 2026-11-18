//! What one migration would do now, and the helpers that build a step: new files, owned files, MCP entries, keys
//! merged into his omp files and shipped files he may make his own (0466, 0479, 0584, 0594).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde_json::{Map, Value};

use super::{Apply, SetupIo};
use crate::setup::config_file::{edit_config, parse_config, parse_json_object, render_config};
use crate::setup::diff::line_diff;
use crate::setup::markers::{Marker, Undo, fingerprint};
use crate::setup::omp_settings::{deep_equals, merge_omp_settings, omp_yaml};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{as_object, write_atomically};

/// One change setup proposes: its diff lines (`+ `, `- `, `  `) and how to make it.
pub(super) struct SetupChange {
	pub title: String,
	pub path: String,
	pub diff: Vec<String>,
	/// Keys he set to another value than Jocasta ships, kept as they are (omp's settings, 0466).
	pub kept: Vec<String>,
	/// Nothing is written: the change only shows what of his stays (0479).
	pub keeps_only: bool,
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
	/// Kept in the marker (the omp settings applied).
	pub shipped: Option<Value>,
}

impl Step {
	pub(super) fn new(path: &Path, fingerprint: impl Into<String>, undo: Vec<Undo>) -> Self {
		Self { path: shown(path), fingerprint: fingerprint.into(), change: None, undo, shipped: None }
	}
}

/// A change that only shows what stays: nothing is written.
pub(super) fn keeps_only(title: String, path: &Path, diff: Vec<String>, kept: Vec<String>) -> SetupChange {
	SetupChange {
		title,
		path: shown(path),
		diff,
		kept,
		keeps_only: true,
		apply: Box::new(|_| Box::pin(async { Ok(()) })),
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

/// Writes `text` to `path` only while the file still is `has` (`None`: still absent), the text its diff was made
/// from: a change made after the diff (omp saving a setting, `/mcp add`) is never overwritten (SET-4, 0584).
pub(super) fn write_if_unchanged(path: &Path, has: Option<&str>, text: &str) -> Result<()> {
	if read_text(path)?.as_deref() != has {
		return Err(Error::other(format!("{} changed since the diff; run jocasta setup again", path.display())));
	}
	write_atomically(path, text, None)
}

/// An apply step that writes `text` to `path` while it still is `has`.
pub(super) fn writes(path: &Path, has: Option<String>, text: String) -> Apply {
	let path = path.to_path_buf();
	Box::new(move |_| Box::pin(async move { write_if_unchanged(&path, has.as_deref(), &text) }))
}

/// An apply step with no questions to ask.
pub(super) fn apply_sync(run: impl FnOnce() -> Result<()> + Send + 'static) -> Apply {
	Box::new(move |_: &dyn SetupIo| Box::pin(async move { run() }))
}

/// A new file: every line added.
pub(super) fn new_file(title: &str, path: &Path, text: &str) -> SetupChange {
	SetupChange {
		title: title.to_owned(),
		path: shown(path),
		diff: text.strip_suffix('\n').unwrap_or(text).split('\n').map(|line| format!("+ {line}")).collect(),
		kept: Vec::new(),
		keeps_only: false,
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
		keeps_only: false,
		apply: writes(path, Some(has), wanted.to_owned()),
	}))
}

/// MCP servers merged into an omp `mcp.json`, his other servers and keys kept; one change per file, titled by the
/// servers it adds or changes; `None` when they're all there.
pub(super) fn mcp_entry(path: &Path, servers: &Map<String, Value>) -> Result<Option<SetupChange>> {
	let before = read_text(path)?;
	let mut config = match &before {
		Some(text) => parse_json_object(path, text)?,
		None => Map::new(),
	};
	let existing = config.get("mcpServers");
	if existing.is_some_and(|value| as_object(value).is_none()) {
		return Err(Error::user_with_code(
			format!("{}: mcpServers is not an object; fix it and run setup again", path.display()),
			1,
		));
	}
	let mut current = existing.and_then(as_object).cloned().unwrap_or_default();
	let changed: Vec<&String> = servers
		.keys()
		.filter(|name| current.get(*name).map(Value::to_string) != servers.get(*name).map(Value::to_string))
		.collect();
	if changed.is_empty() {
		return Ok(None);
	}
	let title = format!(
		"omp MCP server{} {}",
		if changed.len() > 1 { "s" } else { "" },
		changed.iter().map(|name| format!("\"{name}\"")).collect::<Vec<_>>().join(", ")
	);
	for (name, server) in servers {
		current.insert(name.clone(), server.clone());
	}
	config.insert("mcpServers".to_owned(), Value::Object(current));
	let after = json(&Value::Object(config));
	let Some(before) = before else { return Ok(Some(new_file(&title, path, &after))) };
	Ok(Some(SetupChange {
		title,
		path: shown(path),
		diff: line_diff(&before, &after),
		kept: Vec::new(),
		keeps_only: false,
		apply: writes(path, Some(before), after),
	}))
}

/// Jocasta's servers in an omp `mcp.json`; `owned` names every server uninstall removes there (0468).
pub(super) fn mcp_step(path: &Path, servers: &Map<String, Value>, owned: Vec<String>) -> Result<Step> {
	let mut step = Step::new(
		path,
		fingerprint(&Value::Object(servers.clone()).to_string()),
		vec![Undo::Mcp { path: shown(path), servers: owned }],
	);
	step.change = mcp_entry(path, servers)?;
	Ok(step)
}

/// A file Jocasta owns as a migration's step: `wanted` is its fingerprint and its text.
pub(super) fn owned_step(title: &str, path: &Path, wanted: &str, undo: Vec<Undo>) -> Result<Step> {
	let mut step = Step::new(path, fingerprint(wanted), undo);
	step.change = owned_file(title, path, wanted)?;
	Ok(step)
}

/// What Jocasta ships for one of his omp files, merged key by key (0466, 0479).
pub(super) struct MergedFile<'a> {
	/// The change's title.
	pub title: &'a str,
	/// A new file's title, when it differs.
	pub new_title: Option<&'a str>,
	/// The title when some of his own values are kept.
	pub kept_title: String,
	pub path: &'a Path,
	pub shipped: Map<String, Value>,
	pub marker: Option<&'a Marker>,
	/// The shipped file's text: the migration's fingerprint, and a new file is written as it (comments kept).
	pub shipped_text: Option<String>,
	/// A file of flat keys (`keybindings.yml`, `lsp.json`; 0479): the undo names the keys setup added, and only their
	/// lines are edited (0584). Without it (`config.yml`), the merge stays his settings and the file is written whole.
	pub keys: bool,
}

/// The keys of his file setup would add or follow, as its undo records them.
fn claimed_keys(file: &MergedFile, his: &Map<String, Value>, has: Option<&str>) -> Vec<Undo> {
	let path_text = shown(file.path);
	let claimed = file.marker.and_then(|m| {
		m.undo.iter().find_map(|undo| match undo {
			Undo::Keys { path, keys, created } if *path == path_text => Some((keys, *created)),
			_ => None,
		})
	});
	let mut keys = Map::new();
	for (key, value) in &file.shipped {
		let yours = his.get(key);
		let followed = yours.is_some_and(|yours| {
			claimed.is_some_and(|(keys, _)| keys.get(key).is_some_and(|before| deep_equals(yours, before)))
		});
		if yours.is_none() || followed {
			keys.insert(key.clone(), value.clone());
		}
	}
	if keys.is_empty() {
		return Vec::new();
	}
	vec![Undo::Keys { path: path_text, keys, created: claimed.map_or(has.is_none(), |(_, created)| created) }]
}

/// Jocasta's settings merged into one of his omp files (0466, 0479): a missing key added, one still at Jocasta's
/// earlier value following the new one, his own value kept and shown; his other keys untouched. A merge that keeps his
/// values only writes nothing.
pub(super) fn merged_step(file: MergedFile) -> Result<Step> {
	let path = file.path;
	let has = read_text(path)?;
	let his = match &has {
		Some(text) => parse_config(path, text)?,
		None => Map::new(),
	};
	let text = file.shipped_text.clone().unwrap_or_else(|| Value::Object(file.shipped.clone()).to_string());
	let mut step = Step::new(path, fingerprint(&text), Vec::new());
	step.shipped = Some(Value::Object(file.shipped.clone()));
	if file.keys {
		step.undo = claimed_keys(&file, &his, has.as_deref());
	}
	let before = file.marker.and_then(|m| m.shipped.as_ref()).and_then(as_object);
	let merge = merge_omp_settings(&his, &file.shipped, before);
	let (merged, changed, kept) = (merge.merged, merge.changed, merge.kept);
	if !changed && kept.is_empty() {
		return Ok(step);
	}
	let Some(has) = has else {
		let new_text = file.shipped_text.clone().unwrap_or_else(|| render_config(path, &merged));
		step.change = Some(new_file(file.new_title.unwrap_or(file.title), path, &new_text));
		return Ok(step);
	};
	let set: Vec<&str> = merged
		.iter()
		.filter(|(key, value)| !his.get(*key).is_some_and(|had| deep_equals(had, value)))
		.map(|(key, _)| key.as_str())
		.collect();
	let after =
		if file.keys { edit_config(path, &has, &merged, &set, &[]) } else { Some(render_config(path, &merged)) };
	// A file whose lines can't be edited shows the key lines setup would set; applying it is refused (0584).
	let diff = match (&after, changed) {
		(_, false) => Vec::new(),
		(None, true) => {
			let one: Map<String, Value> = set.iter().map(|key| ((*key).to_owned(), merged[*key].clone())).collect();
			let yaml = omp_yaml(&Value::Object(one));
			yaml.trim_end().split('\n').map(|line| format!("+ {line}")).collect()
		}
		(Some(after), true) => line_diff(&has, after),
	};
	let refused = set.iter().map(|key| format!("{key}: {}", merged[*key])).collect::<Vec<_>>().join(", ");
	let path_buf = path.to_path_buf();
	step.change = Some(SetupChange {
		title: if kept.is_empty() { file.title.to_owned() } else { file.kept_title.clone() },
		path: shown(path),
		diff,
		kept: kept.iter().map(|k| format!("{}: yours {} kept (Jocasta ships {})", k.key, k.yours, k.ships)).collect(),
		keeps_only: !changed,
		apply: apply_sync(move || {
			if !changed {
				return Ok(());
			}
			// His file is never rewritten whole (SET-5, 0584): it stays byte for byte and the migration fails as SET-4's does.
			let Some(after) = after else {
				return Err(Error::other(format!(
					"{} can't be edited line by line, so setup left it as it is; set {refused} in it by hand, then run jocasta setup again",
					path_buf.display()
				)));
			};
			write_if_unchanged(&path_buf, Some(&has), &after)
		}),
	});
	Ok(step)
}

/// A shipped file he may make his own (0479, 0594): written when missing, following a newer shipped text while it is
/// still Jocasta's. His own stays and is no longer setup's to undo; `yours` is what setup shows of it (none: nothing).
pub(super) fn text_step(
	title: &str,
	path: &Path,
	wanted: &str,
	marker: Option<&Marker>,
	yours: Option<&dyn Fn(&str) -> SetupChange>,
) -> Result<Step> {
	let ours = fingerprint(wanted);
	let mut step = Step::new(path, ours.clone(), vec![Undo::Text { path: shown(path), fingerprint: ours }]);
	let Some(has) = read_text(path)? else {
		step.change = Some(new_file(title, path, wanted));
		return Ok(step);
	};
	if has == wanted {
		return Ok(step);
	}
	let path_text = shown(path);
	let before = marker.and_then(|m| {
		m.undo.iter().find_map(|u| match u {
			Undo::Text { path, fingerprint } if *path == path_text => Some(fingerprint.as_str()),
			_ => None,
		})
	});
	if before == Some(fingerprint(&has).as_str()) {
		step.change = Some(SetupChange {
			title: title.to_owned(),
			path: path_text,
			diff: line_diff(&has, wanted),
			kept: Vec::new(),
			keeps_only: false,
			apply: writes(path, Some(has), wanted.to_owned()),
		});
		return Ok(step);
	}
	step.undo = Vec::new();
	if let Some(yours) = yours {
		step.change = Some(yours(&has));
	}
	Ok(step)
}
