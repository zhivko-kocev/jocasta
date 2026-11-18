//! What setup has done, kept in `~/.jocasta/agent/setup/` (0466, 0468): one marker per numbered migration it applied
//! (`<NNNN>-<slug>.json`), plus `calendar.json` once the calendar's secret is stored. Each marker names how to undo what
//! it did, so `jocasta uninstall` removes exactly what setup added.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::shared::errors::Result;
use crate::shared::fsx::{read_json_object, write_atomically};

/// How one thing setup added is undone (0468).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Undo {
	/// A file setup wrote.
	#[serde(rename = "file")]
	File { path: String },
	/// A folder setup made (the Terminal fragment's, a skill's), with the file it wrote there (0484): the folder goes
	/// only while that file is in it. A marker from before 0484 has no `file`.
	#[serde(rename = "dir")]
	Dir {
		path: String,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		file: Option<String>,
	},
	/// Jocasta's servers in an omp `mcp.json`; his other servers stay.
	#[serde(rename = "mcp")]
	Mcp { path: String, servers: Vec<String> },
	/// A file of Jocasta's omp snapshot (migration 19): while it is still the snapshot's text (`fingerprint`), the file
	/// it replaced comes back from `backup`, the one kept before Jocasta's first snapshot; with no backup, the snapshot
	/// made it and it goes.
	#[serde(rename = "restore")]
	Restore { path: String, backup: Option<String>, fingerprint: String },
	/// A Windows Credential Manager entry.
	#[serde(rename = "credential")]
	Credential { target: String },
	/// An entry of a kind this build doesn't know, kept as written: the TypeScript passes it through `readMarker`, and
	/// uninstall then fails on it (`no undo kind …`).
	#[serde(skip)]
	Unknown(Value),
}

impl Undo {
	/// The entry as JSON, an unknown one exactly as it was read.
	pub fn to_json(&self) -> Value {
		match self {
			Self::Unknown(value) => value.clone(),
			known => serde_json::to_value(known).unwrap_or(Value::Null),
		}
	}
}

fn undo_entries<S: serde::Serializer>(undo: &[Undo], serializer: S) -> std::result::Result<S::Ok, S::Error> {
	serializer.collect_seq(undo.iter().map(Undo::to_json))
}

/// One marker. Its keys are written in the order the TypeScript's setup wrote them: `fingerprint`, `undo`, then
/// `appliedAt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
	/// The shipped content's fingerprint when it was applied: a new one makes the migration pending again.
	pub fingerprint: String,
	#[serde(serialize_with = "undo_entries")]
	pub undo: Vec<Undo>,
	/// When it was applied (ISO 8601); empty on a marker setup has planned and not yet applied.
	#[serde(default)]
	pub applied_at: String,
}

/// `~/.jocasta/agent/setup`.
pub fn markers_dir(jocasta_config_dir: &Path) -> PathBuf {
	jocasta_config_dir.join("setup")
}

/// A short fingerprint of shipped content.
pub fn fingerprint(text: &str) -> String {
	let digest = Sha256::digest(text.as_bytes());
	digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

/// The marker `<name>.json`, or `None` when there is none (or it isn't one). An `undo` entry of a kind this build
/// doesn't know comes through as `Undo::Unknown`, as the TypeScript passes it through.
pub fn read_marker(jocasta_config_dir: &Path, name: &str) -> Option<Marker> {
	let marker = read_json_object(&markers_dir(jocasta_config_dir).join(format!("{name}.json")))?;
	let fingerprint = marker.get("fingerprint")?.as_str()?.to_owned();
	let undo = marker
		.get("undo")?
		.as_array()?
		.iter()
		.map(|entry| serde_json::from_value::<Undo>(entry.clone()).unwrap_or_else(|_| Undo::Unknown(entry.clone())))
		.collect();
	Some(Marker {
		fingerprint,
		undo,
		applied_at: marker.get("appliedAt").and_then(Value::as_str).unwrap_or_default().to_owned(),
	})
}

pub fn write_marker(jocasta_config_dir: &Path, name: &str, marker: &Marker) -> Result<()> {
	let text = format!("{}\n", serde_json::to_string_pretty(marker)?);
	write_atomically(&markers_dir(jocasta_config_dir).join(format!("{name}.json")), text, None)
}

/// Every undo the markers `names` name, in that order: a marker of a migration this build no longer has is ignored
/// (fork plan phase 6).
pub fn recorded_undo(jocasta_config_dir: &Path, names: &[String]) -> Vec<Undo> {
	names.iter().flat_map(|name| read_marker(jocasta_config_dir, name).map_or_else(Vec::new, |m| m.undo)).collect()
}

#[cfg(test)]
mod tests {
	use std::fs;

	use serde_json::json;

	use super::*;
	use crate::shared::testing::temp_dir;

	fn marker(fingerprint: &str, undo: Vec<Undo>) -> Marker {
		Marker { fingerprint: fingerprint.to_owned(), undo, applied_at: "2026-11-01T00:00:00.000Z".to_owned() }
	}

	#[test]
	fn a_fingerprint_is_the_first_16_hex_digits_of_the_sha_256() {
		assert_eq!(fingerprint(""), "e3b0c44298fc1c14");
		assert_eq!(fingerprint("abc"), "ba7816bf8f01cfea");
	}

	#[test]
	fn a_marker_is_written_with_its_keys_in_setups_order_and_read_back() {
		let dir = temp_dir("markers");
		let undo = vec![
			Undo::Mcp { path: "C:/m.json".to_owned(), servers: vec!["jocasta".to_owned()] },
			Undo::Dir { path: "C:/d".to_owned(), file: None },
		];
		let written = marker("abc", undo);
		write_marker(dir.path(), "0001-mcp", &written).unwrap();
		let text = fs::read_to_string(markers_dir(dir.path()).join("0001-mcp.json")).unwrap();
		assert_eq!(
			text,
			concat!(
				"{\n  \"fingerprint\": \"abc\",\n  \"undo\": [\n    {\n      \"kind\": \"mcp\",\n      \"path\": \"C:/m.json\",\n",
				"      \"servers\": [\n        \"jocasta\"\n      ]\n    },\n    {\n      \"kind\": \"dir\",\n",
				"      \"path\": \"C:/d\"\n    }\n  ],\n",
				"  \"appliedAt\": \"2026-11-01T00:00:00.000Z\"\n}\n"
			)
		);
		assert_eq!(read_marker(dir.path(), "0001-mcp"), Some(written));
		assert_eq!(read_marker(dir.path(), "0002-none"), None);
	}

	#[test]
	fn a_file_that_is_not_a_marker_reads_as_none() {
		let dir = temp_dir("markers");
		let setup = markers_dir(dir.path());
		fs::create_dir_all(&setup).unwrap();
		fs::write(setup.join("a.json"), "{\"fingerprint\": 1, \"undo\": []}").unwrap();
		fs::write(setup.join("b.json"), "{\"fingerprint\": \"x\"}").unwrap();
		fs::write(setup.join("c.json"), "[]").unwrap();
		assert_eq!(read_marker(dir.path(), "a"), None);
		assert_eq!(read_marker(dir.path(), "b"), None);
		assert_eq!(read_marker(dir.path(), "c"), None);
	}

	#[test]
	fn an_undo_entry_of_an_unknown_kind_is_passed_through_and_written_back_as_it_was() {
		let dir = temp_dir("markers");
		let setup = markers_dir(dir.path());
		fs::create_dir_all(&setup).unwrap();
		let odd = json!({ "kind": "from-the-future", "path": "p", "n": 2 });
		let text = json!({ "fingerprint": "f", "undo": [{ "kind": "file", "path": "a" }, odd] });
		fs::write(setup.join("0001-x.json"), text.to_string()).unwrap();
		let read = read_marker(dir.path(), "0001-x").unwrap();
		assert_eq!(read.undo, vec![Undo::File { path: "a".to_owned() }, Undo::Unknown(odd.clone())]);
		assert_eq!(recorded_undo(dir.path(), &["0001-x".to_owned()]), read.undo);
		write_marker(dir.path(), "0002-y", &read).unwrap();
		let back: Value = serde_json::from_str(&fs::read_to_string(setup.join("0002-y.json")).unwrap()).unwrap();
		assert_eq!(back["undo"][1], odd);
	}

	#[test]
	fn the_recorded_undo_is_the_named_markers_in_the_order_named_and_no_others() {
		let dir = temp_dir("markers");
		let names = ["calendar", "0003-c", "0001-a"].map(str::to_owned);
		assert_eq!(recorded_undo(dir.path(), &names), vec![]);
		let file = |path: &str| Undo::File { path: path.to_owned() };
		write_marker(dir.path(), "0001-a", &marker("1", vec![file("a1"), file("a2")])).unwrap();
		write_marker(dir.path(), "0002-gone", &marker("2", vec![file("b")])).unwrap();
		write_marker(dir.path(), "0003-c", &marker("3", vec![file("c")])).unwrap();
		write_marker(dir.path(), "calendar", &marker("cal", vec![Undo::Credential { target: "t".to_owned() }]))
			.unwrap();
		assert_eq!(
			recorded_undo(dir.path(), &names),
			vec![Undo::Credential { target: "t".to_owned() }, file("c"), file("a1"), file("a2")]
		);
	}
}
