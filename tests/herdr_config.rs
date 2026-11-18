//! `test/config/herdr-config.test.ts`: the shipped herdr config (`config/herdr.toml`, 0440), checked by herdr itself
//! (0608), now the fork built into `jocasta`. `jocasta config check` with `HERDR_CONFIG_PATH` names each key herdr
//! ignores, which a mistyped or renamed key would otherwise do in silence. Setup writes the file as shipped.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jocasta::shared::testing::temp_dir;

/// What `jocasta config check` reports for `text` as a herdr.toml.
fn check(text: &str) -> String {
	let root = temp_dir("herdr-config");
	let path = root.path().join("herdr.toml");
	fs::write(&path, text).expect("writing the config");
	let output = Command::new(env!("CARGO_BIN_EXE_jocasta"))
		.args(["config", "check"])
		.env("HERDR_CONFIG_PATH", &path)
		.env("JOCASTA_DATA_DIR", root.path().join("data"))
		.env("JOCASTA_CODING_AGENT_DIR", root.path().join("config"))
		.output()
		.expect("running jocasta config check");
	format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[test]
fn herdr_reads_the_config_at_herdr_config_path_a_made_up_key_is_named() {
	let report = check("onboarding = false\nmade_up_key = 1\n[theme]\nname = \"tokyo-night\"\n");
	assert!(report.contains("unknown config key made_up_key"), "{report}");
}

#[test]
fn the_shipped_herdr_toml_has_no_key_herdr_ignores() {
	let repo: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join("herdr.toml");
	let report = check(&fs::read_to_string(repo).expect("reading config/herdr.toml"));
	assert!(!report.contains("unknown config key"), "{report}");
}
