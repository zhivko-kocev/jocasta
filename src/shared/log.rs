//! Jocasta's log (0431): warnings and errors appended to `<data>/logs/jocasta-YYYY-MM-DD.log` (local day), files older
//! than 7 days removed on the first write of a day. Each line is also written to stderr (a source run). Logging never
//! fails: a full disk must not break the caller.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use crate::shared::day::{day_start, local_day};
use crate::shared::paths::data_dir;

/// A line's severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
	Info,
	Warn,
	Error,
}

impl LogLevel {
	fn upper(self) -> &'static str {
		match self {
			Self::Info => "INFO",
			Self::Warn => "WARN",
			Self::Error => "ERROR",
		}
	}
}

/// Days of log files kept, today included.
pub const LOG_DAYS: usize = 7;

/// A log file's name, its local day captured: `jocasta-YYYY-MM-DD.log`.
pub static LOG_FILE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^jocasta-(\d{4}-\d{2}-\d{2})\.log$").expect("the log file pattern"));

static NEWLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\r?\n").expect("the newline pattern"));

/// `<data>/logs`.
pub fn logs_dir() -> PathBuf {
	data_dir().join("logs")
}

/// The folder and day pruned last, so pruning runs once per day and folder.
static PRUNED: Mutex<String> = Mutex::new(String::new());

/// Removes log files older than `LOG_DAYS` days before `today` (`YYYY-MM-DD`).
fn prune_logs(dir: &Path, today: &str) -> std::io::Result<()> {
	let oldest = local_day(day_start(today, -(LOG_DAYS as i64 - 1)));
	for entry in fs::read_dir(dir)? {
		let name = entry?.file_name();
		let Some(name) = name.to_str() else { continue };
		if LOG_FILE.captures(name).is_some_and(|found| found[1] < *oldest) {
			let _ = fs::remove_file(dir.join(name));
		}
	}
	Ok(())
}

fn append(line: &str, now: i64) -> std::io::Result<()> {
	let dir = logs_dir();
	fs::create_dir_all(&dir)?;
	let today = local_day(now);
	fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(dir.join(format!("jocasta-{today}.log")))?
		.write_all(line.as_bytes())?;
	let mut pruned = PRUNED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	let key = format!("{}|{today}", dir.display());
	if *pruned != key {
		*pruned = key;
		prune_logs(&dir, &today)?;
	}
	Ok(())
}

/// Appends one line to today's log file and stderr. `now` (epoch ms) defaults to the current time.
pub fn log(level: LogLevel, scope: &str, message: &str, now: Option<i64>) {
	let now = now.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
	let at = chrono::DateTime::from_timestamp_millis(now).unwrap_or_default();
	let message = NEWLINE.replace_all(message, "\n    ");
	let line =
		format!("{} {} [{scope}] {message}\n", at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true), level.upper());
	let _ = std::io::stderr().write_all(line.as_bytes());
	// The log is best effort; stderr has the line.
	let _ = append(&line, now);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn jocastas_log_one_file_per_local_day_in_data_logs_7_days_kept() {
		use chrono::TimeZone;
		let _temp = temp_jocasta();
		let day = chrono::Local.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap().timestamp_millis();
		fs::create_dir_all(logs_dir()).unwrap();
		for old in ["2026-10-01", "2026-10-07", "2026-10-08"] {
			fs::write(logs_dir().join(format!("jocasta-{old}.log")), "old\n").unwrap();
		}
		fs::write(logs_dir().join("other.txt"), "kept\n").unwrap();
		log(LogLevel::Warn, "herdr", "herdr 0.10.0 is running\nsecond line", Some(day));
		let text = fs::read_to_string(logs_dir().join("jocasta-2026-10-14.log")).unwrap();
		assert!(text.contains("WARN [herdr] herdr 0.10.0 is running\n    second line\n"), "{text}");
		assert!(text.starts_with("2026-10-14T"), "{text}");
		// Today and the 6 days before stay.
		assert_eq!(LOG_DAYS, 7);
		let mut names: Vec<_> =
			fs::read_dir(logs_dir()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
		names.sort();
		assert_eq!(names, ["jocasta-2026-10-08.log", "jocasta-2026-10-14.log", "other.txt"]);
	}
}
