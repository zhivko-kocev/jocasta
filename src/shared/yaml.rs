//! Jocasta's YAML files: parsing with `saphyr` (the TypeScript's `Bun.YAML`), validation by the caller's schema, and
//! the line a problem sits on.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use saphyr::{LoadableYamlNode, Scalar, Yaml};
use serde_json::{Map, Number, Value};

/// A problem in one of Jocasta's YAML files, named by file and 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlFileError {
	pub file: String,
	pub line: usize,
	pub problem: String,
}

impl fmt::Display for YamlFileError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}:{}: {}", self.file, self.line, self.problem)
	}
}

impl std::error::Error for YamlFileError {}

/// One segment of a path into a parsed document: a key or a list index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
	Key(String),
	Index(usize),
}

impl From<&str> for PathSegment {
	fn from(key: &str) -> Self {
		Self::Key(key.to_owned())
	}
}

impl From<usize> for PathSegment {
	fn from(index: usize) -> Self {
		Self::Index(index)
	}
}

/// Path segments into a parsed document: keys and list indexes.
pub type YamlPath = Vec<PathSegment>;

/// What a schema found wrong with a parsed document. The TypeScript reads this from typebox's first error; the schema
/// is the caller's here, so it reports the same two shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaProblem {
	/// A key the schema doesn't allow (`additionalProperties`), found under `path`.
	UnknownKey { path: YamlPath, key: String },
	/// Any other mismatch at `path`; `message` follows the path in the problem line (`must be string`).
	Invalid { path: YamlPath, message: String },
}

/// A parsed, validated document.
#[derive(Debug, Clone)]
pub struct YamlDocument<T> {
	pub value: T,
	lines: Vec<String>,
}

impl<T> YamlDocument<T> {
	/// The 1-based line that best matches `path` in the source (block-style YAML; flow style falls back to its parent).
	pub fn line_of(&self, path: &[PathSegment]) -> usize {
		locate(&self.lines, path)
	}
}

/// Parses `source` as YAML and validates it with `validate`, which returns the typed value or the first problem. The
/// parser reports no position here either, so a syntax error is placed on the line after the longest prefix of the file
/// that still parses, and a schema error on the line its path reaches in the source.
pub fn parse_yaml_file<T>(
	source: &str,
	file: &str,
	validate: impl FnOnce(&Value) -> Result<T, SchemaProblem>,
) -> Result<YamlDocument<T>, YamlFileError> {
	let lines = split_lines(source);
	let value = parse_value(source).map_err(|problem| YamlFileError {
		file: file.to_owned(),
		line: syntax_error_line(&lines),
		problem,
	})?;
	match validate(&value) {
		Ok(value) => Ok(YamlDocument { value, lines }),
		Err(SchemaProblem::UnknownKey { mut path, key }) => {
			let problem = format!("unknown key `{key}` in {}", describe(&path));
			path.push(PathSegment::Key(key));
			Err(YamlFileError { file: file.to_owned(), line: locate(&lines, &path), problem })
		}
		Err(SchemaProblem::Invalid { path, message }) => {
			let problem = format!("{} {message}", describe(&path));
			Err(YamlFileError { file: file.to_owned(), line: locate(&lines, &path), problem })
		}
	}
}

/// The YAML text as a JSON value, the way `Bun.YAML.parse` hands it over: the first document, `null` for none. A
/// failure is the parser's own description.
pub fn parse_value(source: &str) -> Result<Value, String> {
	let documents = Yaml::load_from_str(source).map_err(|error| error.info().to_owned())?;
	Ok(documents.first().map_or(Value::Null, to_json))
}

fn to_json(node: &Yaml<'_>) -> Value {
	match node {
		Yaml::Value(scalar) => scalar_json(scalar),
		Yaml::Representation(text, ..) => Value::String(text.to_string()),
		Yaml::Sequence(items) => Value::Array(items.iter().map(to_json).collect()),
		Yaml::Mapping(entries) => {
			let mut map = Map::new();
			for (key, value) in entries {
				map.insert(key_text(key), to_json(value));
			}
			Value::Object(map)
		}
		Yaml::Tagged(_, inner) => to_json(inner),
		Yaml::Alias(_) | Yaml::BadValue => Value::Null,
	}
}

fn scalar_json(scalar: &Scalar<'_>) -> Value {
	match scalar {
		Scalar::Null => Value::Null,
		Scalar::Boolean(flag) => Value::Bool(*flag),
		Scalar::Integer(number) => Value::from(*number),
		Scalar::FloatingPoint(number) => Number::from_f64(number.0).map_or(Value::Null, Value::Number),
		Scalar::String(text) => Value::String(text.to_string()),
	}
}

/// A mapping key as JavaScript names a property: a scalar's text.
fn key_text(key: &Yaml<'_>) -> String {
	match to_json(key) {
		Value::String(text) => text,
		Value::Null => "null".to_owned(),
		other => other.to_string(),
	}
}

fn split_lines(source: &str) -> Vec<String> {
	let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
	let last = lines.len() - 1;
	for line in lines.iter_mut().take(last) {
		if line.ends_with('\r') {
			line.pop();
		}
	}
	lines
}

/// `turns[1].tool_calls[0]`, or `the file` for the root.
pub fn describe(path: &[PathSegment]) -> String {
	if path.is_empty() {
		return "the file".to_owned();
	}
	let mut text = String::new();
	for (i, part) in path.iter().enumerate() {
		match part {
			PathSegment::Index(index) => text.push_str(&format!("[{index}]")),
			PathSegment::Key(key) if i == 0 => text.push_str(key),
			PathSegment::Key(key) => {
				text.push('.');
				text.push_str(key);
			}
		}
	}
	text
}

fn syntax_error_line(lines: &[String]) -> usize {
	for n in (0..lines.len()).rev() {
		if parse_value(&lines[..n].join("\n")).is_ok() {
			return n + 1;
		}
	}
	1
}

struct Block {
	/// Index of the line where the block's node starts (-1 for the root).
	at: isize,
	/// First line after the block.
	end: isize,
	/// Column of the node's own key or dash; children sit to the right of it (-1 for the root).
	column: isize,
}

/// Walks block-style YAML by indentation to the line of `path`; stops at the deepest segment it can find.
fn locate(lines: &[String], path: &[PathSegment]) -> usize {
	// The root starts before line 0, so a root-level list's first item is found too.
	let mut block = Block { at: -1, end: lines.len() as isize, column: -1 };
	let mut found = lines.iter().position(|line| !is_blank(line)).unwrap_or(0) as isize;
	for segment in path {
		let next = match segment {
			PathSegment::Index(index) => find_item(lines, &block, *index),
			PathSegment::Key(key) => find_key(lines, &block, key),
		};
		let Some(next) = next else { break };
		found = next.at;
		block = next;
	}
	(found + 1) as usize
}

fn is_blank(line: &str) -> bool {
	let trimmed = line.trim();
	trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---"
}

static DASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*-(\s|$)").expect("a fixed pattern"));

/// The lines inside a node that starts at `at` with its key or dash at `column`. A key's list may sit at the key's own
/// column (`turns:\n- text: a`).
fn block_end(lines: &[String], at: isize, column: isize, limit: isize, is_key: bool) -> isize {
	let mut end = at + 1;
	while end < limit {
		let line = &lines[end as usize];
		if is_blank(line) || indent_of(line) > column {
			end += 1;
			continue;
		}
		if is_key && indent_of(line) == column && DASH.is_match(line) {
			end += 1;
			continue;
		}
		break;
	}
	end
}

fn indent_of(line: &str) -> isize {
	line.chars().take_while(|c| c.is_whitespace()).count() as isize
}

fn find_key(lines: &[String], block: &Block, key: &str) -> Option<Block> {
	let escaped = regex::escape(key);
	let pattern = Regex::new(&format!(r#"^(\s*(?:-\s+)*)(?:{escaped}|"{escaped}"|'{escaped}')\s*:(\s|$)"#))
		.expect("an escaped key");
	for i in block.at..block.end {
		if i < 0 {
			continue;
		}
		let Some(found) = pattern.captures(&lines[i as usize]) else { continue };
		let column = found.get(1).map_or(0, |m| m.as_str().chars().count()) as isize;
		if column <= block.column {
			continue;
		}
		return Some(Block { at: i, end: block_end(lines, i, column, block.end, true), column });
	}
	None
}

fn find_item(lines: &[String], block: &Block, index: usize) -> Option<Block> {
	// Items of this list are the dashes at the smallest column right of the parent, after the parent's own line.
	let mut items: Vec<isize> = Vec::new();
	let mut column = isize::MAX;
	for i in (block.at + 1)..block.end {
		let line = &lines[i as usize];
		if !DASH.is_match(line) {
			continue;
		}
		let dash = indent_of(line);
		if dash < block.column {
			continue;
		}
		if dash < column {
			column = dash;
			items.clear();
		}
		if dash == column {
			items.push(i);
		}
	}
	let at = *items.get(index)?;
	Some(Block { at, end: block_end(lines, at, column, block.end, false), column })
}

#[cfg(test)]
mod tests {
	use super::*;

	fn key(name: &str) -> PathSegment {
		PathSegment::Key(name.to_owned())
	}

	fn no_schema(value: &Value) -> Result<Value, SchemaProblem> {
		Ok(value.clone())
	}

	#[test]
	fn describe_names_a_path_or_the_file() {
		assert_eq!(describe(&[]), "the file");
		let path = [key("turns"), PathSegment::Index(1), key("tool_calls"), PathSegment::Index(0)];
		assert_eq!(describe(&path), "turns[1].tool_calls[0]");
	}

	#[test]
	fn a_document_becomes_json_and_an_empty_one_null() {
		let doc = parse_yaml_file("a: 1\nb: [x, true, null]\nc:\n  d: 1.5\n", "f.yml", no_schema).unwrap();
		assert_eq!(doc.value, serde_json::json!({"a": 1, "b": ["x", true, null], "c": {"d": 1.5}}));
		assert_eq!(parse_yaml_file("", "f.yml", no_schema).unwrap().value, Value::Null);
	}

	#[test]
	fn a_syntax_error_sits_on_the_line_after_the_longest_prefix_that_parses() {
		let error = parse_yaml_file("a: 1\nb: [1, 2\n", "f.yml", no_schema).unwrap_err();
		assert_eq!((error.file.as_str(), error.line), ("f.yml", 2));
		assert!(error.to_string().starts_with("f.yml:2: "));
	}

	#[test]
	fn a_schema_error_sits_on_the_line_its_path_reaches() {
		let source = "name: x\nturns:\n  - text: a\n    tool_calls:\n      - id: 1\n        extra: 2\n  - text: b\n";
		let path = vec![key("turns"), PathSegment::Index(0), key("tool_calls"), PathSegment::Index(0)];
		let unknown = parse_yaml_file::<()>(source, "f.yml", |_| {
			Err(SchemaProblem::UnknownKey { path: path.clone(), key: "extra".to_owned() })
		})
		.unwrap_err();
		assert_eq!(unknown.to_string(), "f.yml:6: unknown key `extra` in turns[0].tool_calls[0]");
		let invalid = parse_yaml_file::<()>(source, "f.yml", |_| {
			Err(SchemaProblem::Invalid {
				path: vec![key("turns"), PathSegment::Index(1)],
				message: "must be string".to_owned(),
			})
		})
		.unwrap_err();
		assert_eq!(invalid.to_string(), "f.yml:7: turns[1] must be string");
		let root = parse_yaml_file::<()>(source, "f.yml", |_| {
			Err(SchemaProblem::Invalid { path: vec![], message: "must be array".to_owned() })
		})
		.unwrap_err();
		assert_eq!(root.to_string(), "f.yml:1: the file must be array");
	}

	#[test]
	fn lines_are_found_in_block_style_with_comments_crlf_and_dashes_at_the_key_column() {
		let source =
			"# head\r\n---\r\nturns:\r\n- text: a\r\n\r\n# between\r\n- text: b\r\n  \"quoted\": 1\r\nlast: 1\r\n";
		let doc = parse_yaml_file(source, "f.yml", no_schema).unwrap();
		assert_eq!(doc.line_of(&[]), 3);
		assert_eq!(doc.line_of(&[key("turns")]), 3);
		assert_eq!(doc.line_of(&[key("turns"), PathSegment::Index(1)]), 7);
		assert_eq!(doc.line_of(&[key("turns"), PathSegment::Index(1), key("text")]), 7);
		assert_eq!(doc.line_of(&[key("turns"), PathSegment::Index(1), key("quoted")]), 8);
		assert_eq!(doc.line_of(&[key("last")]), 9);
		// A segment it can't find stops at the deepest one it can.
		assert_eq!(doc.line_of(&[key("turns"), PathSegment::Index(5), key("text")]), 3);
		assert_eq!(doc.line_of(&[key("missing")]), 3);
	}

	#[test]
	fn a_root_level_list_finds_its_first_item() {
		let doc = parse_yaml_file("- a\n- b\n", "f.yml", no_schema).unwrap();
		assert_eq!((doc.line_of(&[PathSegment::Index(0)]), doc.line_of(&[PathSegment::Index(1)])), (1, 2));
	}
}
