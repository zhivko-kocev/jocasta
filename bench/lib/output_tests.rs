//! The result files' bytes: `JSON.stringify`'s, numbers included.

use serde_json::json;

use super::*;

fn number(x: f64) -> String {
	json_text(&serde_json::to_value(x).unwrap(), false)
}

#[test]
fn a_number_is_written_as_javascript_writes_it() {
	assert_eq!(number(25.0), "25");
	assert_eq!(number(-0.0), "0");
	assert_eq!(number(15.9), "15.9");
	assert_eq!(number(0.1 + 0.2), "0.30000000000000004");
	assert_eq!(number(0.000_001), "0.000001");
	assert_eq!(number(1.5e-7), "1.5e-7");
	assert_eq!(number(1e21), "1e+21");
	assert_eq!(number(123_456_789_012_345_680_000.0), "123456789012345680000");
	assert_eq!(number(-2.5), "-2.5");
	assert_eq!(number(f64::NAN), "null");
}

#[test]
fn pretty_json_is_two_spaces_with_empty_containers_inline_and_compact_json_has_no_spaces() {
	let value = json!({ "a": [1, { "b": 2.5 }], "c": [], "d": {}, "e": "x\"y\n" });
	assert_eq!(
		json_text(&value, true),
		"{\n  \"a\": [\n    1,\n    {\n      \"b\": 2.5\n    }\n  ],\n  \"c\": [],\n  \"d\": {},\n  \"e\": \"x\\\"y\\n\"\n}"
	);
	assert_eq!(json_text(&value, false), "{\"a\":[1,{\"b\":2.5}],\"c\":[],\"d\":{},\"e\":\"x\\\"y\\n\"}");
}

#[test]
fn the_os_name_is_pretty_name_else_name() {
	assert_eq!(
		os_release_name("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n").as_deref(),
		Some("Arch Linux")
	);
	assert_eq!(os_release_name("NAME=Fedora\nPRETTY_NAME=\"\"\n").as_deref(), Some("Fedora"));
	assert_eq!(os_release_name("ID=x\n"), None);
}

#[test]
fn the_gpus_are_lspcis_display_classes() {
	let text = "00:00.0 Host bridge: Acme Root Complex\n01:00.0 VGA compatible controller: Acme Radeon 580 (rev c7)\n\
		02:00.0 3D controller: Acme Tesla\n03:00.0 Audio device: Acme HDMI Audio\n";
	assert_eq!(lspci_gpus(text).as_deref(), Some("Acme Radeon 580 (rev c7); Acme Tesla"));
	assert_eq!(lspci_gpus("00:00.0 Host bridge: Acme\n"), None);
}

#[test]
fn the_machine_stamp_is_in_every_header() {
	let value = serde_json::to_value(conditions(&[] as &[&str])).unwrap();
	for key in ["os", "kernel", "cpu", "memoryGb", "gpu"] {
		assert!(value["machine"].get(key).is_some(), "machine.{key} missing: {value}");
	}
	assert!(value.get("ompVersion").is_some() && value.get("jocastaCommit").is_some(), "{value}");
	// The bench is built from this checkout, so its commit is known.
	assert!(value["jocastaCommit"].as_str().is_some_and(|commit| commit.len() >= 40), "{value}");
}
