//! His omp config files as setup and uninstall read and write them (0466, 0479): YAML or JSON by the file's extension.
//! In a YAML file of flat top-level keys (`keybindings.yml`), only the lines of the keys that change are edited, so
//! his comments and formatting stay (SET-5, 0584); a file whose lines don't edit cleanly is never rewritten whole: the
//! change is refused.

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::setup::omp_settings::{deep_equals, omp_yaml};
use crate::shared::errors::{Error, Result};
use crate::shared::yaml::parse_value;

fn is_json(path: &Path) -> bool {
	path.to_string_lossy().to_lowercase().ends_with(".json")
}

/// `text` of `path` as a JSON object; anything else is refused with how to go on.
pub fn parse_json_object(path: &Path, text: &str) -> Result<Map<String, Value>> {
	let value: Value = serde_json::from_str(text).map_err(|error| {
		Error::user_with_code(format!("{} is not JSON ({error}); fix it and run setup again", path.display()), 1)
	})?;
	match value {
		Value::Object(object) => Ok(object),
		_ => Err(Error::user_with_code(
			format!("{} is not a JSON object; fix it and run setup again", path.display()),
			1,
		)),
	}
}

/// `text` of `path` as a YAML mapping (an empty file is an empty one); anything else is refused.
fn parse_yaml_object(path: &Path, text: &str) -> Result<Map<String, Value>> {
	let value = if text.trim().is_empty() {
		Value::Object(Map::new())
	} else {
		parse_value(text).map_err(|error| {
			Error::user_with_code(format!("{} is not YAML ({error}); fix it and run setup again", path.display()), 1)
		})?
	};
	match value {
		Value::Object(object) => Ok(object),
		_ => Err(Error::user_with_code(
			format!("{} is not a YAML mapping; fix it and run setup again", path.display()),
			1,
		)),
	}
}

/// One of his key files (`keybindings.yml`, `lsp.json`) as an object; an empty file is an empty one.
pub fn parse_config(path: &Path, text: &str) -> Result<Map<String, Value>> {
	if !is_json(path) {
		return parse_yaml_object(path, text);
	}
	if text.trim().is_empty() { Ok(Map::new()) } else { parse_json_object(path, text) }
}

/// `value` as the whole file: YAML as omp writes it, or JSON.
pub fn render_config(path: &Path, value: &Map<String, Value>) -> String {
	if is_json(path) {
		// `to_string_pretty` on a `Value` cannot fail.
		format!("{}\n", serde_json::to_string_pretty(value).unwrap_or_default())
	} else {
		omp_yaml(&Value::Object(value.clone()))
	}
}

/// A top-level key on its line: plain, or quoted.
static KEY_LINE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r#"^(?:"((?:[^"\\]|\\.)*)"|'((?:[^']|'')*)'|([^\s#'"\-?:][^#]*?))\s*:(?:\s|$)"#)
		.expect("a valid pattern")
});

/// The key a top-level line starts, or `None` (a comment, a blank, a value's line).
fn key_of(line: &str) -> Option<String> {
	let found = KEY_LINE.captures(line)?;
	if let Some(quoted) = found.get(1) {
		return Some(
			serde_json::from_str::<String>(&format!("\"{}\"", quoted.as_str()))
				.unwrap_or_else(|_| quoted.as_str().to_owned()),
		);
	}
	if let Some(single) = found.get(2) {
		return Some(single.as_str().replace("''", "'"));
	}
	found.get(3).map(|plain| plain.as_str().to_owned())
}

/// Whether `line` belongs to the key above it: indented, or a list item at the key's own indent.
fn continues(line: &str) -> bool {
	line.starts_with([' ', '\t'])
		|| line.strip_prefix('-').is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// `text` split at `\r?\n`.
fn split_lines(text: &str) -> Vec<&str> {
	let mut lines: Vec<&str> = text.split('\n').collect();
	let last = lines.len() - 1;
	for line in lines.iter_mut().take(last) {
		if let Some(stripped) = line.strip_suffix('\r') {
			*line = stripped;
		}
	}
	lines
}

/// `key`'s lines as omp writes them, from `expected`.
fn rendered(expected: &Map<String, Value>, key: &str) -> Vec<String> {
	let mut one = Map::new();
	if let Some(value) = expected.get(key) {
		one.insert(key.to_owned(), value.clone());
	}
	let text = omp_yaml(&Value::Object(one));
	text.strip_suffix('\n').unwrap_or(&text).split('\n').map(str::to_owned).collect()
}

/// `text` with the top-level keys in `set` written as `expected` has them (a key's lines replaced, a new key appended)
/// and the keys in `remove` dropped with their lines; every other line stays as it was. `expected` is what the file
/// must read as afterwards: a JSON file (no comments to keep) is written whole from it; YAML whose edited lines don't
/// read as it gives `None`, and the caller refuses the change rather than rewrite his file (0584).
pub fn edit_config(
	path: &Path,
	text: &str,
	expected: &Map<String, Value>,
	set: &[&str],
	remove: &[&str],
) -> Option<String> {
	if is_json(path) {
		return Some(render_config(path, expected));
	}
	let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
	let lines = split_lines(text);
	let mut out: Vec<String> = Vec::new();
	let mut placed: HashSet<String> = HashSet::new();
	let mut i = 0;
	while i < lines.len() {
		let key = key_of(lines[i]);
		let mut end = i + 1;
		if key.is_some() {
			while end < lines.len() && continues(lines[end]) {
				end += 1;
			}
		}
		match key.as_deref() {
			Some(name) if remove.contains(&name) => {}
			Some(name) if set.contains(&name) && !placed.contains(name) => {
				out.extend(rendered(expected, name));
				placed.insert(name.to_owned());
			}
			_ => out.extend(lines[i..end].iter().map(|line| (*line).to_owned())),
		}
		i = end;
	}
	let added: Vec<String> =
		set.iter().filter(|key| !placed.contains(**key)).flat_map(|key| rendered(expected, key)).collect();
	if !added.is_empty() {
		// Before the file's last line break; a file without one gets it.
		if out.last().is_some_and(String::is_empty) {
			let at = out.len() - 1;
			out.splice(at..at, added);
		} else {
			out.extend(added);
			out.push(String::new());
		}
	}
	let edited = out.join(eol);
	// A file left with only his comments reads as an empty mapping.
	let reads = if edited.trim().is_empty() {
		Value::Object(Map::new())
	} else {
		match parse_value(&edited) {
			Ok(Value::Null) => Value::Object(Map::new()),
			Ok(value) => value,
			Err(_) => return None,
		}
	};
	deep_equals(&reads, &Value::Object(expected.clone())).then_some(edited)
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

	fn object(value: Value) -> Map<String, Value> {
		value.as_object().cloned().expect("an object")
	}

	#[test]
	fn a_key_line_is_replaced_in_place_when_its_value_changes_and_the_lines_around_it_stay() {
		let keybindings = Path::new("keybindings.yml");
		assert_eq!(
			edit_config(
				keybindings,
				"# mine\napp.live.toggle: [] # off\napp.stt.toggle: f9\n# end\n",
				&object(json!({ "app.live.toggle": [], "app.stt.toggle": "f8" })),
				&["app.stt.toggle"],
				&[],
			)
			.as_deref(),
			Some("# mine\napp.live.toggle: [] # off\napp.stt.toggle: f8\n# end\n")
		);
	}

	#[test]
	fn a_crlf_file_keeps_its_line_ends_and_lines_that_cannot_give_the_keys_are_never_rewritten_whole() {
		let keybindings = Path::new("keybindings.yml");
		assert_eq!(
			edit_config(keybindings, "a: 1\r\n", &object(json!({ "a": 1, "b": 2 })), &["b"], &[]).as_deref(),
			Some("a: 1\r\nb: 2\r\n")
		);
		assert_eq!(edit_config(keybindings, "{a: 1}\n", &object(json!({ "a": 1, "b": 2 })), &["b"], &[]), None);
	}

	#[test]
	fn his_comments_alone_left_read_as_an_empty_file() {
		let keybindings = Path::new("keybindings.yml");
		assert_eq!(
			edit_config(keybindings, "# mine\nb: 2\n", &object(json!({})), &[], &["b"]).as_deref(),
			Some("# mine\n")
		);
	}

	#[test]
	fn a_new_key_is_appended_keeping_his_comments_and_formatting() {
		let his = "# my bindings, tuned over months\napp.model.cycle: [ctrl+k] # cycle models\n\napp.live.toggle:\n  - ctrl+l\n";
		let expected = object(json!({
			"app.model.cycle": ["ctrl+k"],
			"app.live.toggle": ["ctrl+l"],
			"app.stt.toggle": "f9",
		}));
		assert_eq!(
			edit_config(Path::new("keybindings.yml"), his, &expected, &["app.stt.toggle"], &[]),
			Some(format!("{his}app.stt.toggle: f9\n"))
		);
	}

	#[test]
	fn a_json_file_is_written_whole_and_configs_parse_by_extension() {
		let lsp = Path::new("lsp.json");
		assert_eq!(
			edit_config(lsp, "{}", &object(json!({ "a": 1 })), &["a"], &[]).as_deref(),
			Some("{\n  \"a\": 1\n}\n")
		);
		assert_eq!(parse_config(lsp, "  ").unwrap(), Map::new());
		assert_eq!(parse_config(Path::new("keybindings.yml"), "").unwrap(), Map::new());
		assert_eq!(parse_config(Path::new("keybindings.yml"), "a: 1\n").unwrap(), object(json!({ "a": 1 })));
		let not_object = parse_config(lsp, "[1]").unwrap_err();
		assert_eq!(not_object.to_string(), "lsp.json is not a JSON object; fix it and run setup again");
		assert!(matches!(not_object, Error::User { exit_code: 1, .. }));
		let list = parse_config(Path::new("k.yml"), "- a\n").unwrap_err();
		assert_eq!(list.to_string(), "k.yml is not a YAML mapping; fix it and run setup again");
		assert!(parse_config(lsp, "{").unwrap_err().to_string().starts_with("lsp.json is not JSON ("));
	}
}
