//! Scheduled threads (0673): a project's `schedules.yml` (a brief, weekdays and a local time each) and the state that
//! keeps a slot from running twice. The slot math is pure; the service's tick (`day.rs`) decides what is due.
//!
//! Slot rules: a slot is `days` x `time` in local time. A time the clock skips (a spring-forward gap) moves to the
//! first valid minute after it; a repeated time (fall back) is its first. All times are epoch ms.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::shared::errors::Result;
use crate::shared::fsx::write_atomically;
use crate::shared::paths::project_dir;
use crate::shared::yaml::{PathSegment, SchemaProblem, YamlPath, parse_yaml_file};

/// The file name in a project's data folder.
pub const SCHEDULES_FILE: &str = "schedules.yml";
/// The state file's name beside it.
pub const STATE_FILE: &str = "schedules.state.json";

const DAYS: [(&str, Weekday); 7] = [
	("mon", Weekday::Mon),
	("tue", Weekday::Tue),
	("wed", Weekday::Wed),
	("thu", Weekday::Thu),
	("fri", Weekday::Fri),
	("sat", Weekday::Sat),
	("sun", Weekday::Sun),
];

/// One schedule of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
	/// The first 12 hex of sha256 over `brief|days|time`: editing a schedule makes it a new one.
	pub id: String,
	pub brief: String,
	pub done_when: Vec<String>,
	pub heavy: bool,
	pub days: Vec<Weekday>,
	/// Local hour and minute.
	pub time: (u32, u32),
}

/// The first valid local moment at or after `date` `time` (a gap moves on a minute at a time), epoch ms.
fn slot_ms(date: NaiveDate, time: (u32, u32)) -> Option<i64> {
	let at = NaiveTime::from_hms_opt(time.0, time.1, 0)?;
	let mut naive: NaiveDateTime = date.and_time(at);
	for _ in 0..=180 {
		if let Some(found) = Local.from_local_datetime(&naive).earliest() {
			return Some(found.timestamp_millis());
		}
		naive += Duration::minutes(1);
	}
	None
}

impl Schedule {
	/// Every slot on the local days around `now_ms`, from a week and a day back to a week and a day ahead, ascending.
	fn slots(&self, now_ms: i64) -> Vec<i64> {
		let Some(now) = chrono::DateTime::from_timestamp_millis(now_ms) else { return Vec::new() };
		let today = now.with_timezone(&Local).date_naive();
		let mut slots: Vec<i64> = (-8..=8)
			.filter_map(|offset| today.checked_add_signed(Duration::days(offset)))
			.filter(|date| self.days.contains(&date.weekday()))
			.filter_map(|date| slot_ms(date, self.time))
			.collect();
		slots.sort_unstable();
		slots
	}

	/// The latest slot at or before `now_ms`; `None` for a schedule with no days.
	pub fn latest_slot(&self, now_ms: i64) -> Option<i64> {
		self.slots(now_ms).into_iter().rfind(|slot| *slot <= now_ms)
	}

	/// The first slot after `now_ms`.
	pub fn next_run(&self, now_ms: i64) -> Option<i64> {
		self.slots(now_ms).into_iter().find(|slot| *slot > now_ms)
	}
}

fn id_of(brief: &str, days: &[Weekday], time: (u32, u32)) -> String {
	let names: Vec<&str> = days.iter().filter_map(|day| DAYS.iter().find(|(_, d)| d == day).map(|(n, _)| *n)).collect();
	let text = format!("{brief}|{}|{:02}:{:02}", names.join(","), time.0, time.1);
	let digest = Sha256::digest(text.as_bytes());
	let mut hex = String::with_capacity(12);
	for byte in &digest[..6] {
		let _ = write!(hex, "{byte:02x}");
	}
	hex
}

fn invalid(path: &YamlPath, key: &str, message: &str) -> SchemaProblem {
	let mut path = path.clone();
	path.push(PathSegment::Key(key.to_owned()));
	SchemaProblem::Invalid { path, message: message.to_owned() }
}

fn parse_time(text: &str) -> Option<(u32, u32)> {
	let (hours, minutes) = text.split_once(':')?;
	if hours.len() != 2 || minutes.len() != 2 {
		return None;
	}
	let (hours, minutes) = (hours.parse::<u32>().ok()?, minutes.parse::<u32>().ok()?);
	(hours < 24 && minutes < 60).then_some((hours, minutes))
}

fn one_schedule(item: &Value, at: usize) -> std::result::Result<Schedule, SchemaProblem> {
	let path: YamlPath = vec![PathSegment::Index(at)];
	let Some(map) = item.as_object() else {
		return Err(SchemaProblem::Invalid { path, message: "must be a mapping".to_owned() });
	};
	if let Some(key) = map.keys().find(|key| !["brief", "days", "time", "done_when", "heavy"].contains(&key.as_str())) {
		return Err(SchemaProblem::UnknownKey { path, key: key.clone() });
	}
	let brief = match map.get("brief").and_then(Value::as_str).map(str::trim) {
		Some(brief) if !brief.is_empty() => brief.to_owned(),
		_ => return Err(invalid(&path, "brief", "must be a non-empty string")),
	};
	let mut days = Vec::new();
	let listed = map.get("days").and_then(Value::as_array).filter(|list| !list.is_empty());
	for name in listed.ok_or_else(|| invalid(&path, "days", "must be a list of mon, tue, wed, thu, fri, sat, sun"))? {
		let day = name
			.as_str()
			.and_then(|name| DAYS.iter().find(|(short, _)| *short == name.trim().to_lowercase()))
			.map(|(_, day)| *day)
			.ok_or_else(|| invalid(&path, "days", "must be a list of mon, tue, wed, thu, fri, sat, sun"))?;
		if !days.contains(&day) {
			days.push(day);
		}
	}
	let time = map
		.get("time")
		.and_then(Value::as_str)
		.and_then(parse_time)
		.ok_or_else(|| invalid(&path, "time", "must be a local time as \"HH:MM\""))?;
	let done_when = match map.get("done_when") {
		None | Some(Value::Null) => Vec::new(),
		Some(value) => value
			.as_array()
			.and_then(|items| items.iter().map(|item| item.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>())
			.ok_or_else(|| invalid(&path, "done_when", "must be a list of commands"))?,
	};
	let heavy = match map.get("heavy") {
		None | Some(Value::Null) => false,
		Some(value) => value.as_bool().ok_or_else(|| invalid(&path, "heavy", "must be true or false"))?,
	};
	Ok(Schedule { id: id_of(&brief, &days, time), brief, done_when, heavy, days, time })
}

/// `schedules.yml`'s text as schedules; `file` names it in a problem (`file:line: reason`). An empty file has none.
pub fn parse_schedules(source: &str, file: &str) -> std::result::Result<Vec<Schedule>, String> {
	let parsed = parse_yaml_file(source, file, |value| match value {
		Value::Null => Ok(Vec::new()),
		Value::Array(items) => items.iter().enumerate().map(|(at, item)| one_schedule(item, at)).collect(),
		_ => Err(SchemaProblem::Invalid { path: Vec::new(), message: "must be a list of schedules".to_owned() }),
	});
	parsed.map(|document| document.value).map_err(|error| error.to_string())
}

/// A project's schedules; a missing file has none. A bad file is `Err("file:line: reason")` as a whole.
pub fn read_schedules(project_id: &str) -> std::result::Result<Vec<Schedule>, String> {
	let file = project_dir(project_id).join(SCHEDULES_FILE);
	match std::fs::read_to_string(&file) {
		Ok(source) => parse_schedules(&source, &file.display().to_string()),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
		Err(error) => Err(format!("{}:1: {error}", file.display())),
	}
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
	since: i64,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	last: Option<i64>,
}

/// When each schedule was first seen and last started: `{id: {since, last}}` in `schedules.state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScheduleState {
	entries: BTreeMap<String, Entry>,
}

impl ScheduleState {
	/// A project's state; a missing or corrupt file is the default (everything unseen).
	pub fn read(project_id: &str) -> Self {
		let entries = std::fs::read_to_string(project_dir(project_id).join(STATE_FILE))
			.ok()
			.and_then(|text| serde_json::from_str(&text).ok())
			.unwrap_or_default();
		Self { entries }
	}

	/// Writes the project's state file, making its folder first.
	pub fn write(&self, project_id: &str) -> Result<()> {
		let dir = project_dir(project_id);
		std::fs::create_dir_all(&dir)?;
		write_atomically(&dir.join(STATE_FILE), serde_json::to_vec_pretty(&self.entries)?, None)
	}

	/// `last`, else `since`; `None` for a schedule not seen yet.
	pub fn baseline(&self, id: &str) -> Option<i64> {
		self.entries.get(id).map(|entry| entry.last.unwrap_or(entry.since))
	}

	/// Marks a schedule first seen at `now_ms`; one already seen keeps its time.
	pub fn see(&mut self, id: &str, now_ms: i64) {
		self.entries.entry(id.to_owned()).or_insert(Entry { since: now_ms, last: None });
	}

	/// Marks a schedule started at `now_ms`.
	pub fn started(&mut self, id: &str, now_ms: i64) {
		self.entries.entry(id.to_owned()).or_insert(Entry { since: now_ms, last: None }).last = Some(now_ms);
	}
}

#[cfg(test)]
#[path = "schedules_tests.rs"]
mod tests;
