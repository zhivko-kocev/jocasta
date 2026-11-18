//! `test/config/herdr-config.test.ts`: the shipped herdr config (`config/herdr.toml`, 0440), checked by herdr itself
//! (0608). `herdr config check` with `HERDR_CONFIG_PATH` names each key herdr ignores, which a mistyped or renamed key
//! would otherwise do in silence. The template is filled in by the library's own `herdr_config`, as setup fills it.
//! Each test is skipped, with its reason printed, where herdr isn't installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jocasta::frontier::server::herdr::find_herdr;
use jocasta::setup::setup::herdr_config;
use jocasta::shared::testing::temp_dir;

/// What `herdr config check` reports for `text` as a herdr.toml, or `None` where herdr isn't installed.
fn check(text: &str) -> Option<String> {
	let herdr = find_herdr().or_else(|| {
		eprintln!("skipped: herdr isn't installed");
		None
	})?;
	let root = temp_dir("herdr-config");
	let path = root.path().join("herdr.toml");
	fs::write(&path, text).expect("writing the config");
	let output = Command::new(herdr)
		.args(["config", "check"])
		.env("HERDR_CONFIG_PATH", &path)
		.output()
		.expect("running herdr config check");
	Some(format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)))
}

#[test]
fn herdr_reads_the_config_at_herdr_config_path_a_made_up_key_is_named() {
	let Some(report) = check("onboarding = false\nmade_up_key = 1\n[theme]\nname = \"tokyo-night\"\n") else { return };
	assert!(report.contains("unknown config key made_up_key"), "{report}");
}

#[test]
fn the_shipped_herdr_toml_filled_in_as_setup_fills_it_has_no_key_herdr_ignores() {
	let repo: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("config").join("herdr.toml");
	let template = fs::read_to_string(repo).expect("reading config/herdr.toml");
	let Some(report) = check(&herdr_config(&template, Path::new("data"), None)) else { return };
	assert!(!report.contains("unknown config key"), "{report}");
}
