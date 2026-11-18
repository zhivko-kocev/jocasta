//! The omp snapshot's `config.yml` as this machine gets it (0594), and the JavaScript-shaped helpers other writers share:
//! a mapping's entries in a JavaScript object's order and a number as JavaScript prints it.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Number, Value};

use crate::frontier::protocol::js_number_text;
use crate::shared::errors::{Error, Result};

/// The shipped `tools.restrictedFolders` line in `config/omp/config.yml`: a flow list on one line.
static FENCE_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?mR)^(\s*restrictedFolders: \[)([^\r\n]*)\]$").expect("a valid pattern"));

/// The shipped settings' text with Windows' own folders added to the fence (0594); `system_folders` is `None` off
/// Windows, where the text is the shipped file's.
pub fn omp_settings_text(shipped: &str, system_folders: Option<&[String]>) -> Result<String> {
	let Some(folders) = system_folders.filter(|folders| !folders.is_empty()) else { return Ok(shipped.to_owned()) };
	if !FENCE_LINE.is_match(shipped) {
		return Err(Error::other("config/omp/config.yml has no one-line tools.restrictedFolders list"));
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

/// `Number`'s text as JavaScript writes it: [`js_number_text`], except that a negative zero is `-0`.
pub(crate) fn js_number(number: &Number) -> String {
	let value = number.as_f64().unwrap_or(0.0);
	if value == 0.0 && value.is_sign_negative() { "-0".to_owned() } else { js_number_text(value) }
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

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
	fn entries_come_in_a_javascript_objects_order_and_numbers_as_javascript_prints_them() {
		let map = json!({ "b": 1, "10": 2, "a": 3, "2": 4, "01": 5 });
		let keys: Vec<&str> = js_entries(map.as_object().unwrap()).iter().map(|(key, _)| key.as_str()).collect();
		assert_eq!(keys, ["2", "10", "b", "a", "01"]);
		assert_eq!(js_number(&Number::from_f64(-0.0).unwrap()), "-0");
		assert_eq!(js_number(&Number::from_f64(1.5).unwrap()), "1.5");
		assert_eq!(js_number(&Number::from(3)), "3");
	}
}
