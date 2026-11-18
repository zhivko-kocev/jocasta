//! Tests of `spend.rs`, ported from the spend case of `test/frontier/server/sessions.test.ts`.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::json;

use super::*;
use crate::frontier::server::sessions::{Clock, system_clock};
use crate::shared::testing::temp_dir;

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 1e-9, "{actual} is not {expected}");
}

fn assistant(text: &str, timestamp: i64, cost: f64) -> String {
	let message = json!({
		"role": "assistant", "timestamp": timestamp, "content": [{"type": "text", "text": text}],
		"usage": {"cost": {"total": cost}},
	});
	json!({"type": "message", "message": message}).to_string()
}

fn header(cwd: &str) -> String {
	json!({"type": "session", "version": 3, "id": "s", "cwd": cwd}).to_string()
}

fn title() -> String {
	json!({"type": "title", "v": 1, "title": "demo", "pad": ""}).to_string()
}

fn write(file: &Path, lines: &[String]) -> PathBuf {
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	fs::write(file, format!("{}\n", lines.join("\n"))).unwrap();
	file.to_path_buf()
}

fn append(file: &Path, line: &str) {
	fs::OpenOptions::new().append(true).open(file).unwrap().write_all(format!("{line}\n").as_bytes()).unwrap();
}

#[tokio::test]
async fn today_s_spend_lists_the_sessions_folder_at_most_every_30_s_reads_today_s_known_files_on_every_call_and_reads_a_new_one_in_steps_off_the_call_co_15_perf_3()
 {
	let now = (system_clock())();
	let dir = temp_dir("spend");
	let sessions = dir.path().join("sessions");
	let file = write(
		&sessions.join("--C--demo--").join("spend.jsonl"),
		&[title(), header("C:\\demo"), assistant("A", now, 1.0)],
	);
	let clock = Arc::new(AtomicI64::new(now));
	let shared = Arc::clone(&clock);
	let as_clock: Clock = Arc::new(move || shared.load(Ordering::SeqCst));
	let mut index = SessionIndex::new(Some(sessions.clone()), Some(as_clock));
	let mut tracker = SpendTracker::new();
	index.read(&file, false);
	close(spend_today(&mut tracker, &mut index, clock.load(Ordering::SeqCst)), 1.0);
	// A known file is read on every call: its new call counts at once.
	append(&file, &assistant("B", now + 1, 2.0));
	clock.fetch_add(2_000, Ordering::SeqCst);
	close(spend_today(&mut tracker, &mut index, clock.load(Ordering::SeqCst)), 3.0);
	// A new session shows only after the next list, 30 s after the last; it is read off the call, then followed.
	let other = write(
		&sessions.join("--C--other--").join("new.jsonl"),
		&[title(), header("C:\\other"), assistant("C", now + 2, 4.0)],
	);
	clock.fetch_add(2_000, Ordering::SeqCst);
	close(spend_today(&mut tracker, &mut index, clock.load(Ordering::SeqCst)), 3.0);
	assert!(!index.follows(&other));
	clock.fetch_add(30_000, Ordering::SeqCst);
	spend_today(&mut tracker, &mut index, clock.load(Ordering::SeqCst));
	assert!(!index.follows(&other), "the call itself reads nothing new");
	spend_settled(&mut tracker, &mut index).await;
	assert!(index.follows(&other));
	append(&other, &assistant("D", now + 3, 8.0));
	clock.fetch_add(2_000, Ordering::SeqCst);
	close(spend_today(&mut tracker, &mut index, clock.load(Ordering::SeqCst)), 15.0);
}
