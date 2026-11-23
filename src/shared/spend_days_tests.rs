//! Tests of `spend_days.rs`.

use std::fs;
use std::path::Path;

use serde_json::json;

use super::*;
use crate::shared::day::midnight_of;
use crate::shared::testing::temp_dir;

/// Midday of the real today: session files are stamped with the real clock, so the backfill's window must be real.
fn noon() -> i64 {
	midnight_of(chrono::Utc::now().timestamp_millis()) + 12 * 3_600_000
}

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 1e-9, "{actual} is not {expected}");
}

fn day_before(back: i64) -> String {
	local_day(day_start(&local_day(noon()), -back))
}

fn at_noon(back: i64) -> i64 {
	day_start(&local_day(noon()), -back) + 12 * 3_600_000
}

fn session(dir: &Path, name: &str, calls: &[(i64, f64)]) {
	let folder = dir.join("--C--demo--");
	fs::create_dir_all(&folder).unwrap();
	let mut lines = vec![
		json!({"type": "title", "v": 1, "title": "demo", "pad": ""}).to_string(),
		json!({"type": "session", "version": 3, "id": name, "cwd": "C:\\demo"}).to_string(),
	];
	for (at, cost) in calls {
		let message = json!({
			"role": "assistant", "timestamp": at, "content": [{"type": "text", "text": "x"}],
			"usage": {"cost": {"total": cost}},
		});
		lines.push(json!({"type": "message", "message": message}).to_string());
	}
	fs::write(folder.join(format!("{name}.jsonl")), format!("{}\n", lines.join("\n"))).unwrap();
}

fn read_file(file: &Path) -> serde_json::Value {
	serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
}

#[test]
fn the_average_is_the_mean_of_the_recorded_prior_days_and_none_with_none() {
	let dir = temp_dir("spend-days");
	let store = SpendDays::at(dir.path().join("spend-days.json"));
	let today = local_day(noon());
	assert_eq!(store.average(&today), None);
	store.record(&today, 5.0, noon()).unwrap();
	assert_eq!(store.average(&today), None, "today is live, not part of the average");
	for (back, spent) in [(1, 2.0), (2, 4.0)] {
		assert!(store.fill(&day_before(back), spent));
	}
	close(store.average(&today).unwrap(), 3.0);
}

#[test]
fn the_file_keeps_14_days_and_is_written_at_most_once_a_minute_and_at_rollover() {
	let dir = temp_dir("spend-days");
	let file = dir.path().join("spend-days.json");
	let store = SpendDays::at(file.clone());
	for back in 1..=20 {
		store.fill(&day_before(back), 1.0);
	}
	let today = local_day(noon());
	let at = noon();
	store.record(&today, 1.0, at).unwrap();
	let written = read_file(&file);
	assert_eq!(written.as_object().unwrap().len(), 14);
	assert!(written.get(&today).is_some());
	// Within the minute a growing total stays in memory.
	store.record(&today, 2.0, at + 30_000).unwrap();
	close(read_file(&file)[&today].as_f64().unwrap(), 1.0);
	store.record(&today, 3.0, at + 61_000).unwrap();
	close(read_file(&file)[&today].as_f64().unwrap(), 3.0);
	// A new day writes at once, the old day's last total with it.
	store.record(&today, 4.0, at + 62_000).unwrap();
	let tomorrow = local_day(day_start(&today, 1));
	store.record(&tomorrow, 0.5, at + 63_000).unwrap();
	let rolled = read_file(&file);
	close(rolled[&today].as_f64().unwrap(), 4.0);
	close(rolled[&tomorrow].as_f64().unwrap(), 0.5);
}

#[test]
fn a_new_store_reads_what_was_written_and_a_torn_file_reads_as_none() {
	let dir = temp_dir("spend-days");
	let file = dir.path().join("spend-days.json");
	fs::write(&file, "{ not json").unwrap();
	assert_eq!(SpendDays::at(file.clone()).average("2999-01-01"), None);
	fs::write(&file, r#"{"2026-01-01": 2.0, "2026-01-02": "x"}"#).unwrap();
	close(SpendDays::at(file).average("2026-01-03").unwrap(), 2.0);
}

#[tokio::test]
async fn the_backfill_sums_earlier_days_from_sessions_ignores_old_files_and_never_overwrites_a_recorded_day() {
	let dir = temp_dir("spend-days");
	let sessions = dir.path().join("sessions");
	session(&sessions, "a", &[(at_noon(1), 1.5), (at_noon(1) + 1, 2.0), (at_noon(3), 4.0)]);
	session(&sessions, "b", &[(at_noon(6), 8.0), (at_noon(0), 100.0)]);
	session(&sessions, "old", &[(at_noon(20), 16.0), (at_noon(2), 32.0)]);
	let old = sessions.join("--C--demo--").join("old.jsonl");
	fs::File::options().write(true).open(&old).unwrap().set_modified(std::time::SystemTime::UNIX_EPOCH).unwrap();
	let store = SpendDays::at(dir.path().join("spend-days.json"));
	assert!(store.fill(&day_before(3), 99.0));
	backfill(&sessions, noon(), &store).await;
	close(store.get(&day_before(1)).unwrap(), 3.5);
	close(store.get(&day_before(3)).unwrap(), 99.0);
	close(store.get(&day_before(6)).unwrap(), 8.0);
	assert_eq!(store.get(&day_before(20)), None);
	assert_eq!(store.get(&day_before(2)), None, "a file older than 8 days is ignored, whatever it holds");
	assert_eq!(store.get(&local_day(noon())), None, "today is live");
	assert!(dir.path().join("spend-days.json").exists());
}
