//! Quiet hours (0298).

use chrono::{DateTime, Local, Timelike};

use crate::shared::settings::{ServiceKey, Settings, SettingsError, service_setting};

/// Quiet hours as minutes after midnight, local time (0298).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietHours {
	pub from: u32,
	pub to: u32,
}

/// `service.quietHours` (0298; `frontier.quietHours` still read, 0584), like `"22:00-08:00"`, from read settings.
/// Absent, empty or null is none; a bad value fails with `jocasta.yaml`'s line from its schema (0584) and counts as
/// none.
pub fn read_quiet_hours(settings: &Settings) -> Result<Option<QuietHours>, SettingsError> {
	let Some(value) = service_setting(settings, ServiceKey::QuietHours)? else { return Ok(None) };
	let Some(text) = value.as_str().filter(|text| !text.is_empty()) else { return Ok(None) };
	let numbers: Vec<u32> = text.trim().split([':', '-']).map(|part| part.parse().unwrap_or(0)).collect();
	let [h1, m1, h2, m2] = numbers[..] else { return Ok(None) };
	Ok(Some(QuietHours { from: h1 * 60 + m1, to: h2 * 60 + m2 }))
}

/// Whether `at` (ms since the epoch) falls in the quiet hours, local time; a range past midnight wraps.
pub fn is_quiet(quiet: Option<&QuietHours>, at: i64) -> bool {
	let Some(quiet) = quiet.filter(|quiet| quiet.from != quiet.to) else { return false };
	let Some(time) = DateTime::from_timestamp_millis(at).map(|utc| utc.with_timezone(&Local)) else { return false };
	let minute = time.hour() * 60 + time.minute();
	if quiet.from < quiet.to {
		minute >= quiet.from && minute < quiet.to
	} else {
		minute >= quiet.from || minute < quiet.to
	}
}

#[cfg(test)]
mod tests {
	use chrono::TimeZone;
	use serde_json::{Map, Value, json};

	use super::*;

	fn settings(value: Value) -> Settings {
		Settings::from_map(value.as_object().cloned().unwrap_or_else(Map::new))
	}

	fn hours(quiet_hours: Value) -> Result<Option<QuietHours>, SettingsError> {
		read_quiet_hours(&settings(json!({ "service": { "quietHours": quiet_hours } })))
	}

	fn at(hour: u32, minute: u32) -> i64 {
		Local.with_ymd_and_hms(2026, 10, 8, hour, minute, 0).unwrap().timestamp_millis()
	}

	#[test]
	fn quiet_hours_parse_like_22_00_08_00_wrap_past_midnight_and_a_bad_value_names_the_key() {
		let quiet = hours(json!("22:00-08:00")).unwrap();
		assert!(is_quiet(quiet.as_ref(), at(23, 0)));
		assert!(is_quiet(quiet.as_ref(), at(7, 59)));
		assert!(!is_quiet(quiet.as_ref(), at(8, 0)));
		assert!(!is_quiet(quiet.as_ref(), at(21, 59)));
		assert!(is_quiet(hours(json!("13:00-14:00")).unwrap().as_ref(), at(13, 30)));
		assert!(!is_quiet(read_quiet_hours(&settings(json!({}))).unwrap().as_ref(), at(23, 0)));
		assert!(!is_quiet(hours(json!("")).unwrap().as_ref(), at(23, 0)));
		assert_eq!(
			hours(json!("late")).unwrap_err().message,
			"settings: service.quietHours must be like \"22:00-08:00\""
		);
		assert!(hours(json!("25:00-08:00")).unwrap_err().message.contains("service.quietHours"));
		// The old group's name, as his file spells it (0584).
		let old = read_quiet_hours(&settings(json!({ "frontier": { "quietHours": "late" } }))).unwrap_err();
		assert!(old.message.starts_with("settings: frontier.quietHours"));
	}

	#[test]
	fn equal_ends_are_never_quiet_and_no_hours_are_none() {
		let equal = QuietHours { from: 60, to: 60 };
		assert!(!is_quiet(Some(&equal), at(1, 0)));
		assert_eq!(hours(Value::Null).unwrap(), None);
		assert_eq!(hours(json!(" 22:00-08:00 ")).unwrap(), Some(QuietHours { from: 1320, to: 480 }));
	}
}
