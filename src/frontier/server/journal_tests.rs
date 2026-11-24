use serde_json::json;

use super::*;
use crate::shared::testing::temp_jocasta;

fn lines() -> Vec<serde_json::Value> {
	std::fs::read_to_string(data_dir().join(JOURNAL_FILE))
		.unwrap_or_default()
		.lines()
		.map(|line| serde_json::from_str(line).expect("a JSON line"))
		.collect()
}

#[test]
fn each_event_is_one_json_line_with_time_kind_and_who() {
	let _jocasta = temp_jocasta();
	let journal = Journal::default();
	journal.append("stuck", About { project: "p1", thread: Some("t-0001") }, json!({"gate": "quick"}));
	journal.append("slept", About::project("p1"), json!(null));
	let written = lines();
	assert_eq!(written.len(), 2);
	assert_eq!(written[0]["kind"], "stuck");
	assert_eq!(written[0]["project"], "p1");
	assert_eq!(written[0]["thread"], "t-0001");
	assert_eq!(written[0]["gate"], "quick");
	assert!(written[0]["at"].as_i64().unwrap() > 0);
	assert_eq!(written[1]["kind"], "slept");
	assert!(written[1].get("thread").is_none());
}

#[test]
fn a_write_that_fails_is_logged_and_never_returned() {
	let jocasta = temp_jocasta();
	// A file where the data folder should be: nothing can be appended below it.
	let blocked = jocasta.root().join("blocked");
	std::fs::write(&blocked, "x").unwrap();
	let mut jocasta = jocasta;
	jocasta.set_var("JOCASTA_DATA_DIR", Some(&blocked.join("data").display().to_string()));
	Journal::default().append("gate", About::project("p1"), json!({}));
}
