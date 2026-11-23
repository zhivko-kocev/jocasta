//! `check_costs.rs`'s tests: the record's arithmetic and its file.

use super::*;
use crate::shared::testing::temp_dir;

const GIB: u64 = 1 << 30;

fn key() -> String {
	CheckCosts::key(Path::new("/work/acme"), "mise run check")
}

#[test]
fn a_first_measurement_is_the_record_and_later_ones_keep_a_decaying_maximum_and_a_mean_of_cores() {
	let costs = CheckCosts::in_memory();
	assert_eq!(costs.get(&key()), None);
	costs.record(&key(), 4 * GIB, 8.0, 10);
	assert_eq!(costs.get(&key()), Some(CheckCost { peak_rss_bytes: 4 * GIB, avg_cores: 8.0, runs: 1, at: 10 }));
	// A smaller run: the old peak decays by a quarter, not to the new one.
	costs.record(&key(), GIB, 2.0, 20);
	assert_eq!(costs.get(&key()), Some(CheckCost { peak_rss_bytes: 3 * GIB, avg_cores: 5.0, runs: 2, at: 20 }));
	// A larger run takes over at once.
	costs.record(&key(), 5 * GIB, 4.0, 30);
	assert_eq!(costs.get(&key()), Some(CheckCost { peak_rss_bytes: 5 * GIB, avg_cores: 4.5, runs: 3, at: 30 }));
}

#[test]
fn the_key_is_the_project_folder_and_the_command() {
	assert_ne!(key(), CheckCosts::key(Path::new("/work/client-a"), "mise run check"));
	assert_ne!(key(), CheckCosts::key(Path::new("/work/acme"), "mise run test"));
}

#[test]
fn records_round_trip_through_the_file() {
	let dir = temp_dir("check-costs");
	let file = dir.path().join("checks").join("costs.json");
	let costs = CheckCosts::load(file.clone());
	costs.record(&key(), 2 * GIB, 3.0, 99);
	let again = CheckCosts::load(file);
	assert_eq!(again.get(&key()), Some(CheckCost { peak_rss_bytes: 2 * GIB, avg_cores: 3.0, runs: 1, at: 99 }));
}

#[test]
fn an_unreadable_file_is_no_records_and_the_next_record_replaces_it() {
	let dir = temp_dir("check-costs");
	let file = dir.path().join("costs.json");
	for text in ["not json", "[1, 2]", ""] {
		std::fs::write(&file, text).unwrap();
		assert_eq!(CheckCosts::load(file.clone()).get(&key()), None, "{text:?}");
	}
	let costs = CheckCosts::load(file.clone());
	costs.record(&key(), GIB, 1.0, 5);
	assert_eq!(CheckCosts::load(file).get(&key()).map(|cost| cost.runs), Some(1));
}

#[test]
fn an_entry_of_the_wrong_shape_is_dropped_and_its_neighbours_stay() {
	let dir = temp_dir("check-costs");
	let file = dir.path().join("costs.json");
	let good = serde_json::json!({ "peak_rss_bytes": 7, "avg_cores": 1.5, "runs": 2, "at": 3 });
	std::fs::write(&file, serde_json::json!({ (key()): good, "other :: cmd": "bad" }).to_string()).unwrap();
	let costs = CheckCosts::load(file);
	assert_eq!(costs.get(&key()), Some(CheckCost { peak_rss_bytes: 7, avg_cores: 1.5, runs: 2, at: 3 }));
	assert_eq!(costs.get("other :: cmd"), None);
}
