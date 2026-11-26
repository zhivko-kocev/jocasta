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

/// The settings' text with omp's sleep inhibitor turned off by a `power` block at the end, in the text's own line
/// endings (Linux, 0696: omp's idle inhibitor keeps swayidle's timeouts off while a turn runs).
pub fn with_sleep_prevention_off(mut text: String) -> String {
	let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
	if !text.is_empty() && !text.ends_with('\n') {
		text.push_str(newline);
	}
	let lines = [
		"# omp's idle inhibitor keeps swayidle's timeouts off while a turn runs: off on Linux (0696).",
		"power:",
		"  sleepPrevention: \"off\"",
	];
	for line in lines {
		text.push_str(line);
		text.push_str(newline);
	}
	text
}

/// The shipped settings' text with the OS's own folders added to the fence (Windows 0594, Linux 0701);
/// `system_folders` is `None` on macOS, where the text is the shipped file's.
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
	use crate::shared::install::shipped_config_dir;
	use crate::shared::yaml::parse_value;

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
	fn on_linux_the_snapshot_turns_omps_sleep_inhibitor_off_with_a_power_block_at_the_end_0696() {
		let text = with_sleep_prevention_off("tools:\n  restrictedFolders: []\nother: 1\n".to_owned());
		assert!(text.starts_with("tools:\n  restrictedFolders: []\nother: 1\n# "));
		assert!(text.ends_with("\npower:\n  sleepPrevention: \"off\"\n"));
		assert_eq!(parse_value(&text).unwrap()["power"]["sleepPrevention"], json!("off"));
		// The text's own line endings, and a last line without one gets one first.
		assert!(with_sleep_prevention_off("a: 1\r\n".to_owned()).ends_with("power:\r\n  sleepPrevention: \"off\"\r\n"));
		assert!(with_sleep_prevention_off("a: 1".to_owned()).starts_with("a: 1\n# "));
		// The shipped snapshot has no `power` key of its own, so the block is its only one.
		let real = std::fs::read_to_string(shipped_config_dir(None).join("omp").join("config.yml")).unwrap();
		assert!(parse_value(&real).unwrap().get("power").is_none());
		assert_eq!(
			parse_value(&with_sleep_prevention_off(real)).unwrap()["power"],
			json!({ "sleepPrevention": "off" })
		);
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

	/// 0681's pins: what keeps text from leaving the machine. Changing one of these is a decision, so it is a record.
	#[test]
	fn the_shipped_omp_config_keeps_the_four_privacy_pins() {
		let shipped = std::fs::read_to_string(shipped_config_dir(None).join("omp").join("config.yml")).unwrap();
		let config = parse_value(&shipped).unwrap();
		assert_eq!(config["secrets"]["enabled"], json!(true), "secrets.enabled");
		assert_eq!(config["telemetry"]["otlpExportEnabled"], json!(false), "telemetry.otlpExportEnabled");
		assert_eq!(config["spotlight"]["experiment"]["enabled"], json!(false), "spotlight.experiment.enabled");
		assert_eq!(config["collab"]["autoStart"], json!("off"), "collab.autoStart");
	}
}
