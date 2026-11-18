//! The cases of `test/evals/compare.test.ts` and `test/evals/args.test.ts` that ran `evals/run.ts` as a process, as
//! black-box tests of the built `jocasta-bench evals`: each one starts it with a data folder of its own, never his.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

/// A folder of the test's own, removed when it ends.
struct Sandbox(PathBuf);

impl Sandbox {
	fn new() -> Self {
		static COUNTER: AtomicU32 = AtomicU32::new(0);
		let dir = std::env::temp_dir().join(format!(
			"jocasta-evals-cli-{}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::SeqCst)
		));
		fs::create_dir_all(&dir).expect("making the sandbox");
		Self(dir)
	}

	fn path(&self) -> &Path {
		&self.0
	}
}

impl Drop for Sandbox {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn bench(sandbox: &Sandbox, args: &[&str]) -> Output {
	Command::new(env!("CARGO_BIN_EXE_jocasta-bench"))
		.args(args)
		.env("JOCASTA_DATA_DIR", sandbox.path().join("data"))
		.env_remove("JOCASTA_EVALS_REAL")
		.output()
		.expect("starting jocasta-bench")
}

fn row(id: &str, passed: bool, tokens: f64) -> Value {
	let usage = |total_tokens: f64| json!({ "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": total_tokens, "cost": 0 });
	json!({
		"case": id,
		"rep": 0,
		"variant": "default",
		"harness": "omp",
		"harnessVersion": "omp/0.0.0-test",
		"startedAt": "2026-10-24T10:00:00.000Z",
		"model": "faux-1",
		"usage": usage(0.0),
		"totalUsage": usage(tokens),
		"requests": 1,
		"turns": 1,
		"toolCalls": 1,
		"prompts": 1,
		"wallMs": 1000,
		"linesAdded": 10,
		"linesRemoved": 0,
		"grade": {},
		"hiddenPassed": null,
		"findings": null,
		"passed": passed,
	})
}

#[test]
fn bun_run_evals_compare_prints_the_table_and_verdict_and_writes_compare_md_into_b() {
	let sandbox = Sandbox::new();
	let dir_a = sandbox.path().join("run-a");
	let dir_b = sandbox.path().join("run-b");
	for (dir, tokens) in [(&dir_a, 1000.0), (&dir_b, 500.0)] {
		fs::create_dir_all(dir).unwrap();
		let rows: String = (0..2).map(|_| format!("{}\n", row("bug-paginate", true, tokens))).collect();
		fs::write(dir.join("results.jsonl"), rows).unwrap();
	}
	let (a, b) = (dir_a.to_string_lossy().into_owned(), dir_b.to_string_lossy().into_owned());
	let run = bench(&sandbox, &["evals", "compare", &a, &b]);
	let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
	assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
	assert!(stdout.contains("| bug-paginate | 2/2 | 2/2 | 0.50 | 1.00 | 1.00 | 1.00 |"), "{stdout}");
	assert!(stdout.contains("**Verdict: keep.**"));
	assert!(stdout.contains("## Paired statistics (0482), 1 tasks in both runs"));
	assert_eq!(fs::read_to_string(dir_b.join("compare.md")).unwrap(), format!("{}\n", stdout.trim_end()));
	let missing = bench(&sandbox, &["evals", "compare", &a]);
	assert_eq!(missing.status.code(), Some(2));
}

fn exits_2_with_a_jocasta_line(args: &[&str], message: &str) {
	let sandbox = Sandbox::new();
	let mut argv = vec!["evals"];
	argv.extend_from_slice(args);
	let run = bench(&sandbox, &argv);
	assert_eq!(run.status.code(), Some(2), "{args:?}");
	assert_eq!(String::from_utf8_lossy(&run.stderr).trim(), message);
	assert_eq!(String::from_utf8_lossy(&run.stdout), "");
}

#[test]
fn reps_0_exits_2_with_a_jocasta_line_before_any_trial_runs() {
	exits_2_with_a_jocasta_line(&["--reps", "0"], "jocasta: --reps needs a whole number of 1 or more, not \"0\"");
}

#[test]
fn variant_nope_exits_2_with_a_jocasta_line_before_any_trial_runs() {
	exits_2_with_a_jocasta_line(&["--variant", "nope"], "jocasta: unknown variant: nope");
}

#[test]
fn variant_exits_2_with_a_jocasta_line_before_any_trial_runs() {
	exits_2_with_a_jocasta_line(&["--variant"], "jocasta: --variant needs a name");
}

/// `bun run evals`'s environment checks, as `jocasta-bench evals` makes them before any trial.
#[test]
fn needs_jocasta_evals_real_refuses_ci_and_needs_omp_on_path() {
	let sandbox = Sandbox::new();
	let run = |real: Option<&str>, ci: Option<&str>, path: Option<&Path>| {
		let mut command = Command::new(env!("CARGO_BIN_EXE_jocasta-bench"));
		command.arg("evals").env("JOCASTA_DATA_DIR", sandbox.path().join("data"));
		command.env_remove("JOCASTA_EVALS_REAL").env_remove("CI");
		if let Some(real) = real {
			command.env("JOCASTA_EVALS_REAL", real);
		}
		if let Some(ci) = ci {
			command.env("CI", ci);
		}
		if let Some(path) = path {
			command.env("PATH", path).env_remove("Path");
		}
		command.output().expect("starting jocasta-bench")
	};
	let stderr = |output: &Output| String::from_utf8_lossy(&output.stderr).into_owned();
	let no_real = run(None, None, None);
	assert_eq!(no_real.status.code(), Some(2));
	assert!(stderr(&no_real).contains("set JOCASTA_EVALS_REAL=1"), "{}", stderr(&no_real));
	let ci = run(Some("1"), Some("true"), None);
	assert_eq!(ci.status.code(), Some(2));
	assert!(stderr(&ci).contains("never run in CI"), "{}", stderr(&ci));
	let empty = sandbox.path().join("empty");
	fs::create_dir_all(&empty).unwrap();
	let no_omp = run(Some("1"), None, Some(&empty));
	assert_eq!(no_omp.status.code(), Some(2));
	assert!(stderr(&no_omp).contains("omp is not on PATH"), "{}", stderr(&no_omp));
}
