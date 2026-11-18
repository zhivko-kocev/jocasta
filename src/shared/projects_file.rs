//! `~/.jocasta/agent/projects.yaml` (0109, 0112, 0262): read as data, and edited line by line so his comments, order
//! and keys it doesn't name (an old `scopes`) stay.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::pathkey::same_path;
use crate::shared::paths::projects_file;
use crate::shared::yaml::{PathSegment, SchemaProblem, parse_value, parse_yaml_file};

/// One watched root (0109).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedRoot {
	pub path: String,
	pub depth: u64,
	pub ignore: Vec<String>,
}

/// How a repo takes changes to its protected branches (0613): pushes straight onto them, or pull requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushWay {
	#[serde(rename = "direct")]
	Direct,
	#[serde(rename = "pull-requests")]
	PullRequests,
}

impl PushWay {
	/// The word the file writes.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Direct => "direct",
			Self::PullRequests => "pull-requests",
		}
	}
}

/// What a repo answered about pushes to trunk (0613), with its line in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushEntry {
	pub way: PushWay,
	pub line: usize,
}

/// `~/.jocasta/agent/projects.yaml` (0109, 0112, 0262). Keys it doesn't name (an old `scopes`) are kept untouched.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectsFile {
	pub roots: Vec<WatchedRoot>,
	pub add: Vec<String>,
	pub exclude: Vec<String>,
	/// Project id to its name, in the file's order.
	pub names: Vec<(String, String)>,
	/// What each repo (its main checkout's folder, as the file writes it) answered about pushes to trunk (0613), with
	/// its line, in the file's order.
	pub push: Vec<(String, PushEntry)>,
}

struct Parsed {
	roots: Vec<WatchedRoot>,
	add: Vec<String>,
	exclude: Vec<String>,
	names: Vec<(String, String)>,
	push: Vec<(String, PushWay)>,
}

fn invalid(path: Vec<PathSegment>, message: &str) -> SchemaProblem {
	SchemaProblem::Invalid { path, message: message.to_owned() }
}

fn key_path(parts: &[&str]) -> Vec<PathSegment> {
	parts.iter().map(|part| PathSegment::from(*part)).collect()
}

/// A list key may be left empty (`add:`), which YAML reads as null.
fn path_list(value: &Value, key: &str) -> std::result::Result<Vec<String>, SchemaProblem> {
	let items = match value {
		Value::Null => return Ok(Vec::new()),
		Value::Array(items) => items,
		_ => return Err(invalid(key_path(&[key]), "must be array")),
	};
	items
		.iter()
		.enumerate()
		.map(|(index, item)| {
			item.as_str()
				.map(str::to_owned)
				.ok_or_else(|| invalid(vec![PathSegment::from(key), PathSegment::from(index)], "must be string"))
		})
		.collect()
}

fn is_integer(value: &Value) -> bool {
	value.as_f64().is_some_and(|number| number.fract() == 0.0)
}

fn root_of(index: usize, value: &Value) -> std::result::Result<WatchedRoot, SchemaProblem> {
	let at = |extra: &str| {
		let mut path = vec![PathSegment::from("roots"), PathSegment::from(index)];
		if !extra.is_empty() {
			path.push(PathSegment::from(extra));
		}
		path
	};
	let Some(map) = value.as_object() else { return Err(invalid(at(""), "must be object")) };
	let Some(path) = map.get("path") else { return Err(invalid(at(""), "must have required properties path")) };
	let Some(path) = path.as_str() else { return Err(invalid(at("path"), "must be string")) };
	let depth = match map.get("depth") {
		None => 1,
		Some(depth) if !is_integer(depth) => return Err(invalid(at("depth"), "must be integer")),
		Some(depth) if depth.as_f64().is_some_and(|number| number < 1.0) => {
			return Err(invalid(at("depth"), "must be >= 1"));
		}
		Some(depth) => depth.as_f64().map_or(1, |number| number as u64),
	};
	let ignore = match map.get("ignore") {
		None => Vec::new(),
		Some(Value::Null) => Vec::new(),
		Some(Value::Array(items)) => items
			.iter()
			.enumerate()
			.map(|(i, item)| {
				item.as_str().map(str::to_owned).ok_or_else(|| {
					let mut path = at("ignore");
					path.push(PathSegment::from(i));
					invalid(path, "must be string")
				})
			})
			.collect::<std::result::Result<_, _>>()?,
		Some(_) => return Err(invalid(at("ignore"), "must be array")),
	};
	Ok(WatchedRoot { path: path.to_owned(), depth, ignore })
}

/// The file's shape (0109): the mapping first, so a list or a scalar is told it must be a mapping. A root's other keys
/// and the file's other keys (an old `scopes`) are allowed and left alone. The messages are the schema checker's.
fn validate(value: &Value) -> std::result::Result<Parsed, SchemaProblem> {
	let mut parsed = Parsed { roots: vec![], add: vec![], exclude: vec![], names: vec![], push: vec![] };
	let map = match value {
		Value::Null => return Ok(parsed),
		Value::Object(map) => map,
		_ => return Err(invalid(vec![], "must be object")),
	};
	if let Some(version) = map.get("version")
		&& version.as_f64() != Some(1.0)
	{
		return Err(invalid(key_path(&["version"]), "must be equal to constant"));
	}
	match map.get("roots") {
		None | Some(Value::Null) => {}
		Some(Value::Array(roots)) => {
			for (index, root) in roots.iter().enumerate() {
				parsed.roots.push(root_of(index, root)?);
			}
		}
		Some(_) => return Err(invalid(key_path(&["roots"]), "must be array")),
	}
	if let Some(add) = map.get("add") {
		parsed.add = path_list(add, "add")?;
	}
	if let Some(exclude) = map.get("exclude") {
		parsed.exclude = path_list(exclude, "exclude")?;
	}
	match map.get("names") {
		None | Some(Value::Null) => {}
		Some(Value::Object(names)) => {
			for (id, name) in names {
				let Some(name) = name.as_str() else { return Err(invalid(key_path(&["names", id]), "must be string")) };
				parsed.names.push((id.clone(), name.to_owned()));
			}
		}
		Some(_) => return Err(invalid(key_path(&["names"]), "must be object")),
	}
	match map.get("push") {
		None | Some(Value::Null) => {}
		Some(Value::Object(push)) => {
			for (repo, way) in push {
				let way = match way.as_str() {
					Some("direct") => PushWay::Direct,
					Some("pull-requests") => PushWay::PullRequests,
					_ => return Err(invalid(key_path(&["push", repo]), "must be equal to constant")),
				};
				parsed.push.push((repo.clone(), way));
			}
		}
		Some(_) => return Err(invalid(key_path(&["push"]), "must be object")),
	}
	Ok(parsed)
}

/// Reads the projects file (`~/.jocasta/agent/projects.yaml` by default); a missing file is an empty one. A malformed
/// file stops the run with one line naming the file and line, `<file>:<line>: <reason>` (principle 2), exit 1 (0140,
/// 0582).
pub fn read_projects_file(file: Option<&Path>) -> Result<ProjectsFile> {
	let file = file.map_or_else(projects_file, Path::to_path_buf);
	let text = match fs::read_to_string(&file) {
		Ok(text) => text,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(ProjectsFile::default()),
		Err(error) => return Err(error.into()),
	};
	let document = parse_yaml_file(&text, &file.to_string_lossy(), validate)
		.map_err(|error| Error::user_with_code(error.to_string(), 1))?;
	let parsed = &document.value;
	let push = parsed
		.push
		.iter()
		.map(|(repo, way)| {
			let line = document.line_of(&[PathSegment::from("push"), PathSegment::Key(repo.clone())]);
			(repo.clone(), PushEntry { way: *way, line })
		})
		.collect();
	Ok(ProjectsFile {
		roots: parsed.roots.clone(),
		add: parsed.add.clone(),
		exclude: parsed.exclude.clone(),
		names: parsed.names.clone(),
		push,
	})
}

static PLAIN_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_./~$][^#]*$").expect("a valid pattern"));
static FLOW_LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\[.*\])\s*(#.*)?$").expect("a valid pattern"));
static FLOW_MAP: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^push:\s*(\{.*\})\s*(#.*)?$").expect("a valid pattern"));
static OLD_WAY: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r":\s+(direct|pull-requests)(\s+#.*)?$").expect("a valid pattern"));

/// Paths as the file writes them: forward slashes, like the 0109 preview.
fn yaml_scalar(path: &str) -> String {
	let value = path.replace('\\', "/");
	let plain = PLAIN_PATH.is_match(&value) && !value.contains(": ") && !value.ends_with(':');
	if plain { value } else { Value::String(value).to_string() }
}

/// JavaScript's `String(value)`.
fn js_string(value: &Value) -> String {
	match value {
		Value::Null => "null".to_owned(),
		Value::Bool(flag) => flag.to_string(),
		Value::Number(number) => crate::setup::omp_settings::js_number(number),
		Value::String(text) => text.clone(),
		Value::Array(items) => items
			.iter()
			.map(|item| if item.is_null() { String::new() } else { js_string(item) })
			.collect::<Vec<_>>()
			.join(","),
		Value::Object(_) => "[object Object]".to_owned(),
	}
}

struct ListBlock {
	/// Index after the block's last line.
	end: usize,
	indent: String,
	/// Item line indexes with their values.
	items: Vec<(usize, String)>,
}

fn parse_item(line: &str) -> Result<Option<String>> {
	let parsed = parse_value(&format!("x:\n{line}")).map_err(Error::other)?;
	Ok(parsed
		.get("x")
		.and_then(Value::as_array)
		.and_then(|items| items.first())
		.and_then(Value::as_str)
		.map(str::to_owned))
}

/// Whether `line` is the top-level `key:` line.
fn is_key_line(line: &str, key: &str) -> bool {
	line.strip_prefix(key)
		.and_then(|rest| rest.strip_prefix(':'))
		.is_some_and(|rest| rest.chars().next().is_none_or(char::is_whitespace))
}

/// Whether `line` is blank or a comment inside a block.
fn skipped(line: &str) -> bool {
	line.trim().is_empty() || (line.trim_start().starts_with('#') && line.starts_with(char::is_whitespace))
}

/// The text after `rest`'s comment marker group, as ` <comment>` or nothing.
fn comment_of(found: Option<regex::Match<'_>>) -> String {
	found.map_or_else(String::new, |comment| format!(" {}", comment.as_str()))
}

/// Finds a top-level `key:` block-sequence; a flow sequence (`key: []`) is rewritten as a block first.
fn find_block(lines: &mut Vec<String>, key: &str) -> Result<Option<ListBlock>> {
	let Some(key_line) = lines.iter().position(|line| is_key_line(line, key)) else { return Ok(None) };
	let after = lines[key_line][key.len() + 1..].to_owned();
	if let Some(flow) = FLOW_LIST.captures(&after) {
		let parsed = parse_value(&format!("x: {}", &flow[1])).map_err(Error::other)?;
		let values = parsed.get("x").and_then(Value::as_array).cloned().unwrap_or_default();
		let mut replacement = vec![format!("{key}:{}", comment_of(flow.get(2)))];
		replacement.extend(values.iter().map(|value| format!("  - {}", yaml_scalar(&js_string(value)))));
		lines.splice(key_line..=key_line, replacement);
	}
	let mut items = Vec::new();
	let mut indent = "  ".to_owned();
	let mut end = key_line + 1;
	for (i, line) in lines.iter().enumerate().skip(key_line + 1) {
		if skipped(line) {
			continue;
		}
		let dash = line.find("- ").filter(|at| line[..*at].chars().all(char::is_whitespace));
		if dash.is_none() && !line.starts_with(char::is_whitespace) {
			break;
		}
		if let Some(at) = dash {
			if items.is_empty() {
				indent = line[..at].to_owned();
			}
			if let Some(value) = parse_item(&line[at..])? {
				items.push((i, value));
			}
		}
		end = i + 1;
	}
	Ok(Some(ListBlock { end, indent, items }))
}

fn edit(file: &Path, change: impl FnOnce(&mut Vec<String>) -> Result<bool>) -> Result<bool> {
	let text = match fs::read_to_string(file) {
		Ok(text) => text,
		Err(error) if error.kind() == io::ErrorKind::NotFound => "version: 1\n".to_owned(),
		Err(error) => return Err(error.into()),
	};
	let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
	let mut lines: Vec<String> =
		text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned()).collect();
	if lines.last().is_some_and(String::is_empty) {
		lines.pop();
	}
	if !change(&mut lines)? {
		return Ok(false);
	}
	write_atomically(file, format!("{}{eol}", lines.join(eol)), None)?;
	Ok(true)
}

fn target(file: Option<&Path>) -> PathBuf {
	file.map_or_else(projects_file, Path::to_path_buf)
}

/// Which list of the file a path goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKey {
	Add,
	Exclude,
}

impl ListKey {
	fn name(self) -> &'static str {
		match self {
			Self::Add => "add",
			Self::Exclude => "exclude",
		}
	}
}

/// Adds `path` to the `key` list (`add` or `exclude`), keeping the rest of the file and its comments (0109).
pub fn add_to_list(key: ListKey, path: &str, file: Option<&Path>) -> Result<bool> {
	let key = key.name();
	edit(&target(file), |lines| {
		let Some(block) = find_block(lines, key)? else {
			lines.push(format!("{key}:"));
			lines.push(format!("  - {}", yaml_scalar(path)));
			return Ok(true);
		};
		if block.items.iter().any(|(_, value)| same_path(Path::new(value), Path::new(path))) {
			return Ok(false);
		}
		lines.insert(block.end, format!("{}- {}", block.indent, yaml_scalar(path)));
		Ok(true)
	})
}

/// Removes `path` from the `key` list, keeping the rest of the file and its comments.
pub fn remove_from_list(key: ListKey, path: &str, file: Option<&Path>) -> Result<bool> {
	let key = key.name();
	edit(&target(file), |lines| {
		let block = find_block(lines, key)?;
		let matches: Vec<usize> = block
			.map(|block| {
				block
					.items
					.into_iter()
					.filter(|(_, value)| same_path(Path::new(value), Path::new(path)))
					.map(|(line, _)| line)
					.collect()
			})
			.unwrap_or_default();
		for line in matches.iter().rev() {
			lines.remove(*line);
		}
		Ok(!matches.is_empty())
	})
}

/// Records `way` for `repo` in the `push:` map (0613), keeping the rest of the file, its comments and its order: an
/// entry for the same folder is changed in place, a new one goes at the map's end, and a file without the map gets it
/// at its end. False when the file already says so.
pub fn set_push_way(repo: &str, way: PushWay, file: Option<&Path>) -> Result<bool> {
	edit(&target(file), |lines| {
		let entry = format!("{}: {}", yaml_scalar(repo), way.as_str());
		let Some(key_line) = lines.iter().position(|line| is_key_line(line, "push")) else {
			lines.push("push:".to_owned());
			lines.push(format!("  {entry}"));
			return Ok(true);
		};
		// A flow map (`push: {}`) is rewritten as a block first, as `find_block` does for a list.
		if let Some(flow) = FLOW_MAP.captures(&lines[key_line].clone()) {
			let parsed = parse_value(&format!("x: {}", &flow[1])).map_err(Error::other)?;
			let rows: Map<String, Value> = parsed.get("x").and_then(Value::as_object).cloned().unwrap_or_default();
			let mut replacement = vec![format!("push:{}", comment_of(flow.get(2)))];
			replacement.extend(rows.iter().map(|(key, value)| format!("  {}: {}", yaml_scalar(key), js_string(value))));
			lines.splice(key_line..=key_line, replacement);
		}
		place_entry(lines, key_line, repo, &entry, way)
	})
}

/// Puts `entry` into the `push:` block at `key_line`: over the repo's own line, else after the block's last one.
fn place_entry(lines: &mut Vec<String>, key_line: usize, repo: &str, entry: &str, way: PushWay) -> Result<bool> {
	let mut end = key_line + 1;
	let mut indent = "  ".to_owned();
	let snapshot = lines.clone();
	for (i, line) in snapshot.iter().enumerate().skip(key_line + 1) {
		if skipped(line) {
			continue;
		}
		if !line.starts_with(char::is_whitespace) {
			break;
		}
		indent = line[..line.len() - line.trim_start().len()].to_owned();
		end = i + 1;
		let parsed = parse_value(line.trim()).map_err(Error::other)?;
		let Some(key) = parsed.as_object().and_then(|map| map.keys().next()) else { continue };
		if !same_path(Path::new(key), Path::new(repo)) {
			continue;
		}
		let old = OLD_WAY.captures(line);
		if old.as_ref().is_some_and(|found| found[1] == *way.as_str()) {
			return Ok(false);
		}
		lines[i] = format!(
			"{indent}{entry}{}",
			old.as_ref().and_then(|found| found.get(2)).map_or("", |comment| comment.as_str())
		);
		return Ok(true);
	}
	lines.insert(end, format!("{indent}{entry}"));
	Ok(true)
}

#[cfg(test)]
#[path = "projects_file_tests.rs"]
mod tests;
