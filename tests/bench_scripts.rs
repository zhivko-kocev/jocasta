//! The process cases of `test/bench/scripts.test.ts`, as black-box tests of the built `jocasta-bench`: each script's dry
//! run needs no sandbox, no model and no `--out`, and H7's real run with git only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

/// A folder of the test's own, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Self {
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let dir = std::env::temp_dir().join(format!(
			"jocasta-bench-scripts-{}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::SeqCst)
		));
		fs::create_dir_all(&dir).expect("making the scratch folder");
		Self(dir)
	}

	fn path(&self) -> &Path {
		&self.0
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn bench(args: &[&str], config: &Path) -> Output {
	Command::new(env!("CARGO_BIN_EXE_jocasta-bench"))
		.args(args)
		.env_remove("JOCASTA_DATA_DIR")
		.env_remove("JOCASTA_HERDR_SESSION")
		.env_remove("JOCASTA_EVALS_REAL")
		.env("JOCASTA_CODING_AGENT_DIR", config)
		.output()
		.expect("starting jocasta-bench")
}

fn dry_run(command: &[&str], expected: &str) {
	let config = Scratch::new();
	let mut args = command.to_vec();
	args.push("--dry-run");
	let run = bench(&args, config.path());
	assert_eq!(String::from_utf8_lossy(&run.stderr), "");
	assert!(run.status.success(), "{:?}", run.status);
	let stdout = String::from_utf8_lossy(&run.stdout);
	assert!(stdout.contains(expected), "{stdout}");
}

#[test]
fn h2_toast_dry_run() {
	dry_run(&["h2-toast"], "H2 toast latency (0482) dry run");
}

#[test]
fn h3_recovery_dry_run() {
	dry_run(&["h3-recovery"], "H3 crash recovery (0482) dry run");
}

#[test]
fn h4_isolation_dry_run() {
	dry_run(&["h4-isolation"], "H4 memory isolation (0482) dry run");
}

#[test]
fn h5_startup_dry_run() {
	dry_run(&["h5-startup"], "H5 startup (0482) dry run");
}

#[test]
fn h6_memory_dry_run() {
	dry_run(&["h6-memory"], "H6 memory (0482) dry run");
}

#[test]
fn h7_longpath_dry_run() {
	dry_run(&["h7-longpath"], "H7 long paths (0482) dry run");
}

#[test]
fn h8_parallel_dry_run() {
	dry_run(&["h8-parallel"], "H8 parallel isolation (0482) dry run");
}

#[test]
fn h9_footprint_dry_run() {
	dry_run(&["h9-footprint"], "H9 install footprint (0482) dry run");
}

#[test]
fn k11_0543_the_layer_off_in_the_sandbox() {
	dry_run(&["h8-parallel", "--layer", "off"], "K11 (0543): the layer off in the sandbox");
}

#[test]
fn a_real_run_with_git_only_every_location_checks_out_short_paths() {
	let scratch = Scratch::new();
	let out = scratch.path().join("out");
	let root = scratch.path().join("root");
	let home = scratch.path().join("home");
	for folder in [&root, &home] {
		fs::create_dir_all(folder).unwrap();
	}
	let config = Scratch::new();
	let run = bench(
		&[
			"h7-longpath",
			"--out",
			&out.to_string_lossy(),
			"--root",
			&root.to_string_lossy(),
			"--home",
			&home.to_string_lossy(),
			"--lengths",
			"20,40",
		],
		config.path(),
	);
	assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
	let summary: Value = serde_json::from_str(&fs::read_to_string(out.join("h7-longpath.json")).unwrap()).unwrap();
	assert_eq!(summary["largestPassing"], json!({ "jocasta": 40, "herdr": 40, "claude": 40 }));
}
