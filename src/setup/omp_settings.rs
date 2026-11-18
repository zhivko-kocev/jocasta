//! The omp settings Jocasta ships (`config/omp-config.yml`) merged into his existing `config.yml` key by key (0466): a
//! key he doesn't have is added; a key still at the value Jocasta shipped last time follows the new one; a key he set
//! to another value is kept and shown. His other keys are untouched. Keys are leaves: a list is one value.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Number, Value};

use crate::frontier::protocol::js_number_text;
use crate::shared::errors::{Error, Result};

/// A key he set to another value than Jocasta ships, kept.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptSetting {
	pub key: String,
	pub yours: Value,
	pub ships: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsMerge {
	/// His settings with Jocasta's merged in.
	pub merged: Map<String, Value>,
	/// Whether `merged` differs from his.
	pub changed: bool,
	pub kept: Vec<KeptSetting>,
}

/// JavaScript's `Bun.deepEquals(a, b, true)` on parsed data: numbers compare by value (`1` and `1.0` are one number),
/// objects by their keys whatever the order.
pub fn deep_equals(a: &Value, b: &Value) -> bool {
	match (a, b) {
		(Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
		(Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| deep_equals(p, q)),
		(Value::Object(x), Value::Object(y)) => {
			x.len() == y.len() && x.iter().all(|(key, p)| y.get(key).is_some_and(|q| deep_equals(p, q)))
		}
		_ => a == b,
	}
}

struct Walk {
	changed: bool,
	kept: Vec<KeptSetting>,
}

impl Walk {
	fn walk(
		&mut self,
		target: &mut Map<String, Value>,
		ships: &Map<String, Value>,
		was: Option<&Map<String, Value>>,
		prefix: &str,
	) {
		for (name, value) in ships {
			let key = format!("{prefix}{name}");
			let previous = was.and_then(|map| map.get(name));
			let Some(yours) = target.get_mut(name) else {
				target.insert(name.clone(), value.clone());
				self.changed = true;
				continue;
			};
			if let (Some(value_map), Value::Object(yours_map)) = (value.as_object(), &mut *yours) {
				self.walk(yours_map, value_map, previous.and_then(Value::as_object), &format!("{key}."));
			} else if deep_equals(yours, value) {
				// Already Jocasta's value.
			} else if previous.is_some_and(|p| deep_equals(yours, p)) {
				*yours = value.clone();
				self.changed = true;
			} else {
				self.kept.push(KeptSetting { key, yours: yours.clone(), ships: value.clone() });
			}
		}
	}
}

/// Merges `shipped` into `his`; `before` is what Jocasta shipped when it last merged (`None`: never).
pub fn merge_omp_settings(
	his: &Map<String, Value>,
	shipped: &Map<String, Value>,
	before: Option<&Map<String, Value>>,
) -> SettingsMerge {
	let mut merged = his.clone();
	let mut walk = Walk { changed: false, kept: Vec::new() };
	walk.walk(&mut merged, shipped, before, "");
	SettingsMerge { merged, changed: walk.changed, kept: walk.kept }
}

/// The shipped `tools.restrictedFolders` line in `config/omp-config.yml`: a flow list on one line.
static FENCE_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?mR)^(\s*restrictedFolders: \[)([^\r\n]*)\]$").expect("a valid pattern"));

/// The shipped settings' text with Windows' own folders added to the fence (0594); `system_folders` is `None` off
/// Windows, where the text is the shipped file's.
pub fn omp_settings_text(shipped: &str, system_folders: Option<&[String]>) -> Result<String> {
	let Some(folders) = system_folders.filter(|folders| !folders.is_empty()) else { return Ok(shipped.to_owned()) };
	if !FENCE_LINE.is_match(shipped) {
		return Err(Error::other("config/omp-config.yml has no one-line tools.restrictedFolders list"));
	}
	let added = folders.iter().map(|folder| json_string(folder)).collect::<Vec<_>>().join(", ");
	Ok(FENCE_LINE
		.replace(shipped, |found: &regex::Captures| {
			let list = &found[2];
			let separator = if list.is_empty() { "" } else { ", " };
			format!("{}{list}{separator}{added}]", &found[1])
		})
		.into_owned())
}

fn json_string(text: &str) -> String {
	Value::String(text.to_owned()).to_string()
}

/// Settings as omp writes `config.yml` itself (`@oh-my-pi/pi-utils` `stringifyYamlConfig`, which is `Bun.YAML.stringify`
/// with two spaces): block style, a value quoted only where plain text would read as something else, and no final
/// line break. A mapping's keys come in a JavaScript object's order (array indexes first, ascending), a number as
/// JavaScript prints it. `omp_yaml_corpus.json` pins it against Bun's own output.
pub fn omp_yaml(value: &Value) -> String {
	let mut lines = Vec::new();
	match value {
		Value::Object(map) if !map.is_empty() => emit_map(map, 0, &mut lines),
		Value::Array(items) if !items.is_empty() => emit_seq(items, 0, &mut lines),
		other => lines.push(inline(other)),
	}
	let mut text = lines.join("\n");
	// `.replace(/: +$/gm, ":")`
	text = text.split('\n').map(strip_colon_spaces).collect::<Vec<_>>().join("\n");
	text
}

fn strip_colon_spaces(line: &str) -> &str {
	let trimmed = line.trim_end_matches(' ');
	if trimmed.len() < line.len() && trimmed.ends_with(':') { trimmed } else { line }
}

fn non_empty_container(value: &Value) -> bool {
	match value {
		Value::Object(map) => !map.is_empty(),
		Value::Array(items) => !items.is_empty(),
		_ => false,
	}
}

/// Whether a key is a JavaScript array index (`0` or digits without a leading zero, below 2^32 - 1).
fn is_array_index(key: &str) -> bool {
	let digits = !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit());
	digits && (key == "0" || !key.starts_with('0')) && key.parse::<u64>().is_ok_and(|index| index < u64::from(u32::MAX))
}

/// A mapping's entries in the order a JavaScript object has them: array-index keys first, ascending, then the rest as
/// they were added.
pub(crate) fn js_entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
	let (mut indexes, others): (Vec<_>, Vec<_>) = map.iter().partition(|(key, _)| is_array_index(key));
	indexes.sort_by_key(|(key, _)| key.parse::<u64>().unwrap_or(0));
	indexes.extend(others);
	indexes
}

fn emit_map(map: &Map<String, Value>, indent: usize, out: &mut Vec<String>) {
	let pad = " ".repeat(indent);
	for (key, value) in js_entries(map) {
		let key = scalar_text(key);
		match value {
			Value::Object(inner) if !inner.is_empty() => {
				out.push(format!("{pad}{key}:"));
				emit_map(inner, indent + 2, out);
			}
			Value::Array(items) if !items.is_empty() => {
				out.push(format!("{pad}{key}:"));
				emit_seq(items, indent + 2, out);
			}
			other => out.push(format!("{pad}{key}: {}", inline(other))),
		}
	}
}

fn emit_seq(items: &[Value], indent: usize, out: &mut Vec<String>) {
	let pad = " ".repeat(indent);
	for item in items {
		if non_empty_container(item) {
			let mut inner = Vec::new();
			match item {
				Value::Object(map) => emit_map(map, indent + 2, &mut inner),
				Value::Array(nested) => emit_seq(nested, indent + 2, &mut inner),
				_ => {}
			}
			let mut rest = inner.into_iter();
			if let Some(first) = rest.next() {
				out.push(format!("{pad}- {}", &first[indent + 2..]));
			}
			out.extend(rest);
		} else {
			out.push(format!("{pad}- {}", inline(item)));
		}
	}
}

/// A value on one line: a scalar, or an empty list or mapping.
fn inline(value: &Value) -> String {
	match value {
		Value::Null => "null".to_owned(),
		Value::Bool(flag) => flag.to_string(),
		Value::Number(number) => js_number(number),
		Value::String(text) => scalar_text(text),
		Value::Array(_) => "[]".to_owned(),
		Value::Object(_) => "{}".to_owned(),
	}
}

/// `Number`'s text as `Bun.YAML.stringify` writes it: [`js_number_text`], except that a negative zero is `-0`.
pub(crate) fn js_number(number: &Number) -> String {
	let value = number.as_f64().unwrap_or(0.0);
	if value == 0.0 && value.is_sign_negative() { "-0".to_owned() } else { js_number_text(value) }
}

/// The scan `Bun.YAML.stringify` makes of a plain string to see whether it reads as a number (or ends as a document
/// marker): one state per row, one column per kind of character (see [`number_class`]). Learned from Bun 1.4.3's own
/// output over every string of up to five characters from `0 1 8 + - . e x a o _` and 30,000 longer ones, so it keeps
/// Bun's oddities (`1e` and `0-0` are quoted, `0--` is not, a string ending in `...` or `---` is).
const NUMBER_SCAN: [[u8; 11]; 35] = [
	[1, 2, 2, 3, 4, 5, 6, 6, 6, 6, 6],
	[2, 2, 2, 2, 7, 5, 8, 9, 6, 10, 6],
	[2, 2, 2, 2, 7, 5, 8, 6, 6, 6, 6],
	[1, 2, 2, 2, 7, 5, 8, 6, 6, 6, 6],
	[4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4],
	[11, 11, 11, 11, 12, 13, 14, 6, 6, 6, 6],
	[6, 6, 6, 6, 15, 16, 6, 6, 6, 6, 6],
	[17, 17, 17, 17, 18, 19, 20, 6, 6, 6, 6],
	[8, 8, 8, 8, 21, 22, 6, 6, 6, 6, 6],
	[23, 23, 23, 6, 24, 16, 23, 6, 23, 6, 6],
	[25, 25, 25, 25, 26, 16, 25, 6, 6, 6, 6],
	[11, 11, 11, 11, 12, 16, 14, 6, 6, 6, 6],
	[27, 27, 27, 27, 18, 16, 28, 6, 6, 6, 6],
	[6, 6, 6, 6, 15, 29, 6, 6, 6, 6, 6],
	[14, 14, 14, 14, 30, 16, 6, 6, 6, 6, 6],
	[6, 6, 6, 6, 18, 16, 6, 6, 6, 6, 6],
	[6, 6, 6, 6, 15, 13, 6, 6, 6, 6, 6],
	[17, 17, 17, 17, 15, 19, 20, 6, 6, 6, 6],
	[6, 6, 6, 6, 31, 16, 6, 6, 6, 6, 6],
	[27, 27, 27, 27, 15, 13, 28, 6, 6, 6, 6],
	[20, 20, 20, 20, 15, 32, 6, 6, 6, 6, 6],
	[20, 20, 20, 20, 18, 32, 6, 6, 6, 6, 6],
	[14, 14, 14, 14, 30, 13, 6, 6, 6, 6, 6],
	[23, 23, 23, 6, 24, 16, 23, 6, 23, 6, 6],
	[33, 33, 33, 6, 18, 16, 33, 6, 33, 6, 6],
	[25, 25, 25, 25, 26, 16, 25, 6, 6, 6, 6],
	[34, 34, 34, 34, 18, 16, 34, 6, 6, 6, 6],
	[27, 27, 27, 27, 15, 16, 28, 6, 6, 6, 6],
	[28, 28, 28, 28, 15, 16, 6, 6, 6, 6, 6],
	[6, 6, 6, 6, 15, 29, 6, 6, 6, 6, 6],
	[28, 28, 28, 28, 18, 16, 6, 6, 6, 6, 6],
	[6, 6, 6, 6, 31, 16, 6, 6, 6, 6, 6],
	[28, 28, 28, 28, 15, 13, 6, 6, 6, 6, 6],
	[33, 33, 33, 6, 15, 16, 33, 6, 33, 6, 6],
	[34, 34, 34, 34, 15, 16, 34, 6, 6, 6, 6],
];

/// The states of [`NUMBER_SCAN`] after which the string is quoted (the scan starts in state 0).
const NUMBER_SCAN_QUOTES: [u8; 26] =
	[1, 2, 4, 5, 7, 8, 11, 12, 14, 17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34];

/// The column of [`NUMBER_SCAN`] for a character: `0`, `1`-`7`, `8`-`9`, `+`, `-`, `.`, `e`/`E`, `x`/`X`, the hex
/// letters `a`-`d` and `f` in either case, `o`/`O`, and anything else.
fn number_class(c: char) -> usize {
	match c {
		'0' => 0,
		'1'..='7' => 1,
		'8' | '9' => 2,
		'+' => 3,
		'-' => 4,
		'.' => 5,
		'e' | 'E' => 6,
		'x' | 'X' => 7,
		'a'..='d' | 'f' | 'A'..='D' | 'F' => 8,
		'o' | 'O' => 9,
		_ => 10,
	}
}

/// Whether Bun's scan of `text` reads a number (or a trailing `...` or `---`).
fn number_like(text: &str) -> bool {
	let state = text.chars().fold(0_usize, |state, c| usize::from(NUMBER_SCAN[state][number_class(c)]));
	NUMBER_SCAN_QUOTES.contains(&(state as u8))
}

/// Words YAML reads as something else, in the three cases Bun checks (`true`, `True`, `TRUE`), and the signed and
/// unsigned `.inf` and `.nan` in theirs.
fn reserved_word(text: &str) -> bool {
	const WORDS: [&str; 7] = ["true", "false", "null", "yes", "no", "on", "off"];
	if matches!(text, "~" | "y" | "Y" | "n" | "N" | ".nan" | ".NaN" | ".NAN") {
		return true;
	}
	let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
	if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
		return true;
	}
	WORDS.iter().any(|word| {
		let title = format!("{}{}", word[..1].to_ascii_uppercase(), &word[1..]);
		text == *word || text == title || text == word.to_ascii_uppercase()
	})
}

/// Whether YAML would read `text` as something other than the string, or not as one plain scalar.
fn needs_quotes(text: &str) -> bool {
	let Some(first) = text.chars().next() else { return true };
	let last = text.chars().next_back().unwrap_or(first);
	if matches!(first, ' ' | '\t') || matches!(last, ' ' | '\t') || text.chars().any(is_special) {
		return true;
	}
	if "-?:,[]{}#&*!|>'\"%@`<\u{feff}".contains(first) || text.ends_with(':') || text.contains(": ") {
		return true;
	}
	if text.chars().any(|c| ",[]{}#'\"`".contains(c)) {
		return true;
	}
	reserved_word(text) || number_like(text)
}

/// A character Bun writes inside quotes: a control character below U+0020 or U+007F, NEL, the no-break space, a line
/// separator, or a byte order mark (Bun quotes only a leading one, and writes a later one plain, which our own parser
/// refuses: quoted here, a cosmetic difference that still round-trips). The C1 controls U+0080 to U+009F other than NEL
/// it writes as they are.
fn is_special(c: char) -> bool {
	c < ' ' || matches!(c, '\u{7f}' | '\u{85}' | '\u{a0}' | '\u{2028}' | '\u{2029}' | '\u{feff}')
}

fn scalar_text(text: &str) -> String {
	if !needs_quotes(text) {
		return text.to_owned();
	}
	let mut quoted = String::from("\"");
	for c in text.chars() {
		match c {
			'"' => quoted.push_str("\\\""),
			'\\' => quoted.push_str("\\\\"),
			'\n' => quoted.push_str("\\n"),
			'\r' => quoted.push_str("\\r"),
			'\t' => quoted.push_str("\\t"),
			'\0' => quoted.push_str("\\0"),
			'\u{7}' => quoted.push_str("\\a"),
			'\u{8}' => quoted.push_str("\\b"),
			'\u{b}' => quoted.push_str("\\v"),
			'\u{c}' => quoted.push_str("\\f"),
			'\u{1b}' => quoted.push_str("\\e"),
			'\u{85}' => quoted.push_str("\\N"),
			'\u{a0}' => quoted.push_str("\\_"),
			'\u{2028}' => quoted.push_str("\\L"),
			'\u{2029}' => quoted.push_str("\\P"),
			c if c < ' ' || c == '\u{7f}' => quoted.push_str(&format!("\\x{:02x}", c as u32)),
			c => quoted.push(c),
		}
	}
	quoted.push('"');
	quoted
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

	fn object(value: Value) -> Map<String, Value> {
		value.as_object().cloned().expect("an object")
	}

	#[test]
	fn merge_omp_settings_adds_missing_keys_follows_old_values_keeps_his_and_treats_lists_as_one_value() {
		let merged = merge_omp_settings(
			&object(json!({ "a": 1, "b": { "c": 2, "d": ["x"] }, "his": true })),
			&object(json!({ "a": 1, "b": { "c": 3, "d": ["y"], "e": 4 }, "f": { "g": 5 } })),
			Some(&object(json!({ "b": { "c": 2, "d": ["z"] } }))),
		);
		assert_eq!(
			merged,
			SettingsMerge {
				merged: object(json!({ "a": 1, "b": { "c": 3, "d": ["x"], "e": 4 }, "his": true, "f": { "g": 5 } })),
				changed: true,
				kept: vec![KeptSetting { key: "b.d".to_owned(), yours: json!(["x"]), ships: json!(["y"]) }],
			}
		);
		assert_eq!(
			merge_omp_settings(&object(json!({ "a": 1 })), &object(json!({ "a": 1 })), None),
			SettingsMerge { merged: object(json!({ "a": 1 })), changed: false, kept: vec![] }
		);
	}

	#[test]
	fn the_shipped_fence_also_names_windows_own_folders() {
		let shipped = "tools:\n  restrictedFolders: [\"/etc\"]\nother: 1\n";
		assert_eq!(omp_settings_text(shipped, None).unwrap(), shipped);
		assert_eq!(omp_settings_text(shipped, Some(&[])).unwrap(), shipped);
		assert_eq!(
			omp_settings_text("tools:\n  restrictedFolders: []\n", Some(&["C:\\W".to_owned()])).unwrap(),
			"tools:\n  restrictedFolders: [\"C:\\\\W\"]\n"
		);
		assert_eq!(
			omp_settings_text(shipped, Some(&["C:\\W".to_owned(), "D:\\P".to_owned()])).unwrap(),
			"tools:\n  restrictedFolders: [\"/etc\", \"C:\\\\W\", \"D:\\\\P\"]\nother: 1\n"
		);
		assert_eq!(
			omp_settings_text("tools:\r\n  restrictedFolders: []\r\n", Some(&["C:\\W".to_owned()])).unwrap(),
			"tools:\r\n  restrictedFolders: [\"C:\\\\W\"]\r\n"
		);
		let refused = omp_settings_text("tools: {}\n", Some(&["C:\\W".to_owned()])).unwrap_err();
		assert!(refused.to_string().contains("no one-line tools.restrictedFolders list"));
	}

	#[test]
	fn omp_yaml_writes_block_style_quoting_only_what_plain_text_would_misread() {
		let value = json!({
			"a": 1,
			"b": { "c": "x y", "d": ["p", "q"], "e": [], "f": {} },
			"k.k": "22:00",
			"s": "",
			"t": "true",
			"n": null,
			"u": "a: b",
			"w": "#x",
			"fl": 1.5,
			"neg": -1,
			"l": [{ "a": 1, "b": 2 }, [1, 2]],
			"q": "it's",
			"nl": "a\nb",
			"y": "yes",
			"num": "123",
			"path": "C:\\W",
			"uni": "héllo",
			"bool": true,
		});
		assert_eq!(
			omp_yaml(&value),
			[
				"a: 1",
				"b:",
				"  c: x y",
				"  d:",
				"    - p",
				"    - q",
				"  e: []",
				"  f: {}",
				"k.k: 22:00",
				"s: \"\"",
				"t: \"true\"",
				"\"n\": null",
				"u: \"a: b\"",
				"w: \"#x\"",
				"fl: 1.5",
				"neg: -1",
				"l:",
				"  - a: 1",
				"    b: 2",
				"  - - 1",
				"    - 2",
				"q: \"it's\"",
				"nl: \"a\\nb\"",
				"\"y\": \"yes\"",
				"num: \"123\"",
				"path: C:\\W",
				"uni: héllo",
				"bool: true",
			]
			.join("\n")
		);
		assert_eq!(omp_yaml(&json!({})), "{}");
		assert_eq!(
			omp_yaml(&json!({ "app.live.toggle": [], "app.stt.toggle": "f9" })),
			"app.live.toggle: []\napp.stt.toggle: f9"
		);
	}
}

#[cfg(test)]
#[path = "omp_yaml_tests.rs"]
mod yaml_tests;
