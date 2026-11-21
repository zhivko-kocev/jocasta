//! The H1 and tokscale-parity scripts' cases that run `jocasta-bench` as a process (`test/bench/h1-guard.test.ts` and
//! `test/bench/tokscale-parity.test.ts` spawned the TypeScript scripts), as black-box tests of the built binary, and the
//! contract `test/config/guard.test.ts` relies on: `h1-guard evaluate` writes each corpus line with its `verdict` and
//! `rule` to `h1-guard.jsonl`.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

use jocasta::shared::testing::temp_dir;

fn bench(args: &[&str]) -> Output {
	Command::new(env!("CARGO_BIN_EXE_jocasta-bench")).args(args).output().expect("starting jocasta-bench")
}

fn stdout(run: &Output) -> String {
	String::from_utf8_lossy(&run.stdout).into_owned()
}

fn stderr(run: &Output) -> String {
	String::from_utf8_lossy(&run.stderr).into_owned()
}

fn jsonl(rows: &[Value]) -> String {
	rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")
}

fn read_jsonl(file: &Path) -> Vec<Value> {
	std::fs::read_to_string(file)
		.unwrap()
		.lines()
		.filter(|line| !line.trim().is_empty())
		.map(|line| serde_json::from_str(line).unwrap())
		.collect()
}

fn text(path: &Path) -> &str {
	path.to_str().unwrap()
}

const GUARD: &str = "bash:
  gitGuard: true
  patterns:
    - { match: \"*git *push* --force\", approval: deny }
    - { match: \"echo ask*\", approval: prompt }
";

/// Harmless shapes plus one force-push-style line that the deny rule matches.
fn corpus() -> String {
	jsonl(&[
		json!({ "command": "git status", "expected": "allow", "group": "dev" }),
		json!({ "command": "git push origin feature --force", "expected": "deny", "group": "dev" }),
		json!({ "command": "echo hi", "expected": "allow", "group": "held-out" }),
		json!({ "command": "ls -la && git   log", "expected": "allow", "group": "held-out" }),
		json!({ "command": "echo ask me", "expected": "prompt", "group": "held-out" }),
		json!({ "command": "echo ask twice", "expected": "allow", "group": "held-out" }),
	])
}

#[test]
fn the_cli_prints_the_table_writes_its_results_outside_the_repo_and_dry_runs_without_judging() {
	let dir = temp_dir("jocasta-h1-");
	let (corpus_file, guard_file, out) =
		(dir.path().join("corpus.jsonl"), dir.path().join("guard.yml"), dir.path().join("out"));
	std::fs::write(&corpus_file, corpus()).unwrap();
	std::fs::write(&guard_file, GUARD).unwrap();
	let run = bench(&[
		"h1-guard",
		"evaluate",
		"--corpus",
		text(&corpus_file),
		"--guard",
		text(&guard_file),
		"--out",
		text(&out),
		"--resamples",
		"200",
	]);
	assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
	assert!(stdout(&run).contains("| dev | 1/0/1 | 100.0%"), "{}", stdout(&run));
	let summary: Value = serde_json::from_str(&std::fs::read_to_string(out.join("h1-guard.json")).unwrap()).unwrap();
	assert_eq!((&summary["benchmark"], &summary["commands"], &summary["rules"]), (&json!("H1"), &json!(6), &json!(2)));
	assert_eq!(read_jsonl(&out.join("h1-guard.jsonl")).len(), 6);
	let dry =
		bench(&["h1-guard", "evaluate", "--corpus", text(&corpus_file), "--guard", text(&guard_file), "--dry-run"]);
	assert_eq!(dry.status.code(), Some(0));
	assert!(stdout(&dry).contains("judge 6 commands (groups: dev, held-out) against 2 rules"), "{}", stdout(&dry));
	let inside_out = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench").join("out");
	let inside = bench(&[
		"h1-guard",
		"evaluate",
		"--corpus",
		text(&corpus_file),
		"--guard",
		text(&guard_file),
		"--out",
		text(&inside_out),
	]);
	assert_eq!(inside.status.code(), Some(2));
	assert!(stderr(&inside).contains("--out must be outside the repo"), "{}", stderr(&inside));
	assert!(!inside_out.exists());
}

/// One collected command written three ways, plus a collected command that the builder filed under another's rewrite.
fn variant_corpus() -> (String, String) {
	let line = |id: &str, command: &str, base: &str, variant: &str, expected: &str| json!({ "id": id, "command": command, "base": base, "variant": variant, "expected": expected, "group": "held-out", "category": "x" });
	let rows = [
		line("c1", "git reset --hard", "git reset --hard", "as-written", "deny"),
		line("c2", "cd x && git reset --hard", "git reset --hard", "after-cd", "deny"),
		line("c3", "git.exe reset --hard", "git reset --hard", "git-exe", "deny"),
		line("c4", "rm -rf out", "rm -Rf out", "lowercase-powershell", "prompt"),
		line("c5", "rm -Rf out", "rm -Rf out", "as-written", "prompt"),
		line("c6", "cd x && rm -rf out", "rm -rf out", "after-cd", "prompt"),
		line("c7", "ls", "ls", "as-written", "allow"),
		line("c8", "ls  -la", "ls -la", "extra-spaces", "allow"),
	];
	let verdicts = [
		("deny", "guard-pattern", json!("*git*reset* --hard*")),
		("deny", "guard-pattern", json!("*git*reset* --hard*")),
		("allow", "mode-default", Value::Null),
		("prompt", "guard-pattern", json!("rm -*r*")),
		("allow", "mode-default", Value::Null),
		("prompt", "guard-pattern", json!("rm -*r*")),
		("allow", "mode-default", Value::Null),
		("deny", "git-guard", json!("git stash")),
	];
	let verdict_rows: Vec<Value> = rows
		.iter()
		.zip(verdicts)
		.map(|(row, (verdict, mechanism, matched))| {
			json!({ "id": row["id"], "verdict": verdict, "mechanism": mechanism, "match": matched, "command": row["command"] })
		})
		.collect();
	(jsonl(&rows), jsonl(&verdict_rows))
}

#[test]
fn the_cli_scores_omp_s_verdicts_with_verdicts_and_reports_each_way_of_writing() {
	let dir = temp_dir("jocasta-h1-");
	let (corpus_file, guard_file, verdicts_file) =
		(dir.path().join("corpus.jsonl"), dir.path().join("guard.yml"), dir.path().join("omp-verdicts.jsonl"));
	let (corpus, verdicts) = variant_corpus();
	std::fs::write(&corpus_file, corpus).unwrap();
	std::fs::write(&guard_file, GUARD).unwrap();
	std::fs::write(&verdicts_file, verdicts).unwrap();
	let out = dir.path().join("out");
	let run = bench(&[
		"h1-guard",
		"evaluate",
		"--corpus",
		text(&corpus_file),
		"--guard",
		text(&guard_file),
		"--verdicts",
		text(&verdicts_file),
		"--out",
		text(&out),
		"--resamples",
		"200",
	]);
	assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
	assert!(stdout(&run).contains("matcher: omp's own verdicts"));
	assert!(stdout(&run).contains("| as-written | 1/2/1 | 100.0%"), "{}", stdout(&run));
	let summary: Value = serde_json::from_str(&std::fs::read_to_string(out.join("h1-guard.json")).unwrap()).unwrap();
	assert!(summary["matcher"].as_str().unwrap().starts_with("omp's own verdicts"));
	let note = summary["notes"]["asWritten"].as_str().unwrap();
	assert!(note.contains("the 4 lines whose command is one of the collected commands"), "{note}");
	assert!(note.contains("3 carry the as-written tag, and c4 (rm -rf out, tagged lowercase-powershell) is"), "{note}");
	let variants: Vec<&str> =
		summary["variants"].as_array().unwrap().iter().map(|row| row["variant"].as_str().unwrap()).collect();
	assert!(variants.contains(&"git-exe"));
	let rows = read_jsonl(&out.join("h1-guard.jsonl"));
	assert_eq!(rows[2]["id"], "c3");
	assert_eq!(
		(&rows[2]["verdict"], &rows[2]["mechanism"], &rows[2]["reimplemented"]),
		(&json!("allow"), &json!("mode-default"), &json!("allow"))
	);
	let dry = bench(&[
		"h1-guard",
		"evaluate",
		"--corpus",
		text(&corpus_file),
		"--verdicts",
		text(&verdicts_file),
		"--dry-run",
	]);
	assert!(stdout(&dry).contains("matcher: omp's own verdicts"), "{}", stdout(&dry));
}

#[test]
fn evaluate_writes_each_corpus_line_with_its_verdict_and_rule_byte_for_byte() {
	let dir = temp_dir("jocasta-h1-");
	let (corpus_file, guard_file, out) =
		(dir.path().join("corpus.jsonl"), dir.path().join("guard.yml"), dir.path().join("out"));
	std::fs::write(
		&corpus_file,
		jsonl(&[
			json!({ "id": "g1", "command": "git push origin main --force", "expected": "deny", "group": "held-out", "variant": "as-written", "base": "git push origin main --force", "category": "force-push" }),
			json!({ "id": "g2", "command": "git status", "expected": "allow", "group": "held-out" }),
		]) + "\n",
	)
	.unwrap();
	std::fs::write(&guard_file, GUARD).unwrap();
	let run = bench(&[
		"h1-guard",
		"evaluate",
		"--corpus",
		text(&corpus_file),
		"--guard",
		text(&guard_file),
		"--out",
		text(&out),
		"--resamples",
		"50",
	]);
	assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
	let rows = std::fs::read_to_string(out.join("h1-guard.jsonl")).unwrap();
	let expected = [
		r#"{"command":"git push origin main --force","expected":"deny","group":"held-out","id":"g1","variant":"as-written","base":"git push origin main --force","category":"force-push","verdict":"deny","rule":"*git *push* --force"}"#,
		r#"{"command":"git status","expected":"allow","group":"held-out","id":"g2","verdict":"allow","rule":null}"#,
	];
	assert_eq!(rows, format!("{}\n", expected.join("\n")));
}

#[test]
fn a_run_folder_with_nothing_to_compare_exits_2_instead_of_passing() {
	let run_dir = temp_dir("jocasta-parity-");
	std::fs::write(
		run_dir.path().join("results.jsonl"),
		format!(
			"{}\n{}\n",
			json!({ "task": "t", "arm": "a0", "rep": 0, "harness": "omp" }),
			json!({ "task": "t", "arm": "a4", "rep": 0, "harness": "claude" })
		),
	)
	.unwrap();
	// A sessions folder whose names match no trial, as after a rename.
	std::fs::create_dir_all(run_dir.path().join("sessions").join("t-a0-0-renamed")).unwrap();
	let run = bench(&["tokscale-parity", "--out", text(run_dir.path())]);
	assert_eq!(run.status.code(), Some(2));
	assert_eq!(
		stderr(&run).trim(),
		format!(
			"bench: no saved sessions to compare in {} (results.jsonl names 2 trials)",
			run_dir.path().join("sessions").display()
		)
	);
}

#[test]
fn omp_verdicts_writes_omp_s_verdict_for_each_line_with_the_git_guard_on_top_byte_for_byte() {
	// omp's own code loads only on Bun (the oracle, `omp-verdicts.ts`), and only the pinned version: without Bun or
	// omp's package at that version the case says why, as `test/config/guard.test.ts` does.
	let found = Command::new("bun").args(["-e", "process.exit(0)"]).output().is_ok();
	let omp = std::process::Command::new("bun")
		.args([
			"-e",
			"import { readFileSync } from 'node:fs'; \
			 import { PINNED_OMP, findOmpDir } from './bench/h1-guard/omp-verdicts.ts'; \
			 const dir = findOmpDir(); \
			 process.exit(dir && JSON.parse(readFileSync(`${dir}/package.json`, 'utf8')).version === PINNED_OMP ? 0 : 1);",
		])
		.current_dir(env!("CARGO_MANIFEST_DIR"))
		.output();
	if !found || !omp.is_ok_and(|run| run.status.success()) {
		println!("omp-verdicts: skipped, Bun or omp's pinned package isn't installed");
		return;
	}
	let dir = temp_dir("jocasta-h1-");
	let (corpus_file, guard_file, out) =
		(dir.path().join("corpus.jsonl"), dir.path().join("guard.yml"), dir.path().join("out"));
	std::fs::write(
		&corpus_file,
		jsonl(&[
			json!({ "id": "g1", "command": "git push origin main --force", "expected": "deny", "group": "held-out", "variant": "as-written", "base": "git push origin main --force", "category": "force-push" }),
			json!({ "id": "g2", "command": "git status", "expected": "allow", "group": "held-out" }),
			json!({ "id": "g3", "command": "git stash", "expected": "deny", "group": "held-out" }),
		]),
	)
	.unwrap();
	std::fs::write(&guard_file, GUARD).unwrap();
	let args = |extra: &[&str]| {
		let mut all: Vec<&str> =
			vec!["h1-guard", "omp-verdicts", "--corpus", text(&corpus_file), "--guard", text(&guard_file)];
		all.extend_from_slice(extra);
		bench(&all)
	};
	let run = args(&["--out", text(&out)]);
	assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
	let rows = read_jsonl(&out.join("omp-verdicts.jsonl"));
	assert_eq!(
		rows[0].to_string(),
		r#"{"id":"g1","command":"git push origin main --force","expected":"deny","variant":"as-written","base":"git push origin main --force","category":"force-push","verdict":"deny","mechanism":"guard-pattern","match":"*git *push* --force","approval":"deny","critical":false,"gitGuard":null,"gitGuardSource":"reimplemented","reimplemented":"deny"}"#
	);
	assert_eq!(
		rows[2].to_string(),
		r#"{"id":"g3","command":"git stash","expected":"deny","variant":"","base":"","category":"","verdict":"deny","mechanism":"git-guard","match":"git stash","approval":"allow","critical":false,"gitGuard":"git stash","gitGuardSource":"reimplemented","reimplemented":"allow"}"#
	);
	let summary: Value =
		serde_json::from_str(&std::fs::read_to_string(out.join("omp-verdicts.json")).unwrap()).unwrap();
	assert_eq!((&summary["benchmark"], &summary["commands"]), (&json!("H1"), &json!(3)));
	assert_eq!(summary["gitGuardSources"], json!({ "reimplemented": 3 }));
	assert!(stdout(&run).contains("git guard sources: {\"reimplemented\":3}"), "{}", stdout(&run));
	let again = args(&["--out", text(&out)]);
	assert_eq!(again.status.code(), Some(2));
	assert!(stderr(&again).contains("exists; results are never overwritten"), "{}", stderr(&again));
	let dry = args(&["--dry-run"]);
	assert_eq!(dry.status.code(), Some(0), "{}", stderr(&dry));
	assert!(stdout(&dry).contains("ask omp's resolveApproval for each of 3 commands"), "{}", stdout(&dry));
}
