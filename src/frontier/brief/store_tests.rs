//! Tests of `store.rs`.

use super::*;
use crate::frontier::brief::model::Section;
use crate::shared::testing::temp_jocasta;

fn brief(id: &str) -> Brief {
	Brief {
		id: id.to_owned(),
		kind: "show".to_owned(),
		title: "A title".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 1,
		sections: vec![Section::Text { heading: None, body: "hello".to_owned() }],
	}
}

#[test]
fn a_brief_is_stored_under_its_id_and_read_back() {
	let _j = temp_jocasta();
	assert!(load("thread-ab12").is_none());
	save(&brief("thread-ab12")).unwrap();
	assert_eq!(load("thread-ab12"), Some(brief("thread-ab12")));
	assert!(brief_dir().join("thread-ab12.json").exists());
}

#[test]
fn an_id_that_could_leave_the_folder_is_refused() {
	let _j = temp_jocasta();
	for id in ["", "../x", "a/b", "a\\b", "a.json", "x y"] {
		assert!(!valid_id(id), "{id}");
		assert!(load(id).is_none());
	}
	assert!(save(&brief("../evil")).is_err());
}

#[test]
fn show_ids_count_up() {
	let _j = temp_jocasta();
	assert_eq!(next_show_id(), "show-1");
	save(&brief("show-1")).unwrap();
	save(&brief("show-7")).unwrap();
	assert_eq!(next_show_id(), "show-8");
}

#[test]
fn a_save_deletes_the_briefs_last_written_over_30_days_ago_with_their_screenshots_0715() {
	let _j = temp_jocasta();
	save(&brief("pr-acme-1")).unwrap();
	save(&brief("show-2")).unwrap();
	let shots = brief_dir().join("assets").join("pr-acme-1");
	std::fs::create_dir_all(&shots).unwrap();
	std::fs::write(shots.join("home.png"), b"png").unwrap();
	let old = SystemTime::now() - KEEP - Duration::from_secs(60);
	std::fs::File::options().write(true).open(brief_dir().join("pr-acme-1.json")).unwrap().set_modified(old).unwrap();
	let young = SystemTime::now() - KEEP + Duration::from_secs(3_600);
	std::fs::File::options().write(true).open(brief_dir().join("show-2.json")).unwrap().set_modified(young).unwrap();
	save(&brief("show-3")).unwrap();
	assert!(!exists("pr-acme-1"));
	assert!(!shots.exists());
	assert!(exists("show-2"));
	assert!(exists("show-3"));
}
