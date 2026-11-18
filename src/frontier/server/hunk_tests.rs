//! Tests of `hunk.rs` (from `test/frontier/server/hunk.test.ts`). The case "a review fails on each way a proof weakens
//! the tests (0610)" is `ReviewFlow::failed`'s and waits for `review_flow.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use super::fake_hunk::fake_hunk;
use super::*;
use crate::shared::poll::wait_for;
use crate::shared::proctree::kill_tree;
use crate::shared::testing::{TempJocasta, temp_jocasta};

fn strings(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

fn git(dir: &Path, args: &[&str]) -> String {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
	String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A git repo with one commit on `main`, in `dir`.
fn make_repo(dir: &Path) -> PathBuf {
	std::fs::create_dir_all(dir).unwrap();
	git(dir, &["init", "-q", "-b", "main"]);
	std::fs::write(dir.join("README.md"), "demo\n").unwrap();
	git(dir, &["add", "."]);
	git(dir, &["commit", "-q", "-m", "init repo"]);
	dir.to_path_buf()
}

fn write_file(repo: &Path, file: &str, text: &str) {
	let path = repo.join(file);
	std::fs::create_dir_all(path.parent().unwrap()).unwrap();
	std::fs::write(path, text).unwrap();
}

fn context(rules: &[&str]) -> ReviewContext {
	ReviewContext { rules: rules.iter().map(PathBuf::from).collect(), ..ReviewContext::default() }
}

#[test]
fn hunks_npm_platform_packages_0394() {
	assert_eq!(hunk_package("win32", "x64"), Some("hunkdiff-windows-x64"));
	assert_eq!(hunk_package("linux", "x64"), Some("hunkdiff-linux-x64"));
	assert_eq!(hunk_package("darwin", "arm64"), Some("hunkdiff-darwin-arm64"));
	assert_eq!(hunk_package("linux", "arm64"), None);
}

#[test]
fn a_source_run_finds_hunk_in_node_modules_hunkdiff_platform_through_the_packages_bin() {
	let found = find_hunk();
	let (platform, arch) = this_platform();
	if hunk_package(platform, arch).is_none() {
		assert_eq!(found, None);
		return;
	}
	let found = found.expect("hunk in node_modules");
	let shown = found.to_string_lossy().replace('\\', "/");
	assert!(Regex::new(r"node_modules/hunkdiff-[a-z0-9-]+/bin/hunk(\.exe)?$").unwrap().is_match(&shown), "{shown}");
	assert!(found.exists());
}

#[test]
fn the_review_prompt_spells_out_hunks_commands_with_a_quoted_forward_slashed_binary() {
	let prompt =
		review_prompt(&strings(&["C:\\Program Files\\jocasta\\hunk\\hunk.exe"]), "main...feature/x", &context(&[]));
	assert!(prompt.contains("git diff main...feature/x"));
	assert!(prompt.contains("`\"C:/Program Files/jocasta/hunk/hunk.exe\" session comment apply --repo . --stdin`"));
	assert!(
		review_prompt(&strings(&["/opt/hunk"]), "main...b", &context(&[]))
			.contains("`/opt/hunk session review --repo . --json`")
	);
}

#[test]
fn the_reviewer_gets_the_plan_the_rules_and_the_lessons_and_only_material_findings_with_priorities() {
	let gates = "checks: FAIL (tree 3f9a1c)\n  FAIL  test  mise run test";
	let hunk = strings(&["/opt/hunk"]);
	let prompt = review_prompt(
		&hunk,
		"main...b",
		&ReviewContext {
			plan: Some(PathBuf::from("C:\\plans\\p.md")),
			rules: vec![PathBuf::from("C:\\wt\\AGENTS.md"), PathBuf::from("C:\\repo\\AGENTS.local.md")],
			gates: Some(gates.to_owned()),
			checked: true,
			..ReviewContext::default()
		},
	);
	assert!(prompt.contains("the approved plan (C:/plans/p.md)"));
	assert!(prompt.contains("the project's rules (C:/wt/AGENTS.md, C:/repo/AGENTS.local.md)"));
	assert!(prompt.contains("skill://lessons"));
	assert!(prompt.contains("report only material findings; prefer one strong finding over several weak ones"));
	// Jocasta ran the checks: the failing gates go in, the turn runs no tests of its own and ends on no TESTS line.
	assert!(prompt.contains(&format!("they failed; give the reviewer these gates too:\n\n{gates}\n\n")));
	assert!(!prompt.contains("as one bash command"));
	assert!(!prompt.contains("TESTS:"));
	assert!(prompt.contains("\"summary\":\"[P<priority>] <one sentence>\""));
	// Checks that passed say nothing.
	let passed = review_prompt(
		&hunk,
		"main...b",
		&ReviewContext { gates: Some(String::new()), checked: true, ..ReviewContext::default() },
	);
	assert!(passed.starts_with("Check this task's work. When the project has a `verify` skill"));
	// Without a mise check, the project's own way and the TESTS line; without a plan or rules, only the lessons.
	let plain = review_prompt(&hunk, "main...b", &ReviewContext::default());
	assert!(plain.contains("the way its AGENTS.md or package scripts say"));
	assert!(plain.contains("read skill://verify and run it too"));
	assert!(plain.contains("`TESTS: pass — <the test command you ran>`"));
	assert!(!plain.contains("approved plan"));
	assert!(plain.contains("give it its lessons in skill://lessons"));
}

#[test]
fn the_prompt_gives_the_brief_and_the_report_on_one_line_each() {
	let hunk = strings(&["/opt/hunk"]);
	let prompt = review_prompt(
		&hunk,
		"main...b",
		&ReviewContext {
			brief: Some("  fix «the»\n   bug ".to_owned()),
			report: Some(Some("did  it\n".to_owned())),
			..ReviewContext::default()
		},
	);
	assert!(prompt.contains("his brief «fix \"the\" bug», to check the diff against what he asked for"));
	assert!(prompt.contains("the thread's own report «did it» (its claims, not proof)"));
	let none = review_prompt(&hunk, "main...b", &ReviewContext { report: Some(None), ..ReviewContext::default() });
	assert!(none.contains("the fact that the thread handed in no report"));
}

#[tokio::test]
async fn notes_are_counted_from_hunk_session_comment_list_json_only_those_made_since_the_review_began_cr_22() {
	let j = temp_jocasta();
	assert_eq!(
		count_comments(r#"{"comments":[{"commentId":"a"},{"commentId":"b"}]}"#, 0).unwrap(),
		ReviewNotes { notes: 2, blocking: 0 }
	);
	assert_eq!(count_comments(r#"{"comments":[]}"#, 0).unwrap(), ReviewNotes { notes: 0, blocking: 0 });
	assert!(count_comments("{}", 0).unwrap_err().to_string().contains("no comments array"));
	let timed = json!({
		"comments": [
			{ "commentId": "old", "createdAt": "2026-10-14T09:00:00.000Z" },
			{ "commentId": "new", "createdAt": "2026-10-14T10:00:00.000Z" },
		],
	})
	.to_string();
	let since = parse_date("2026-10-14T09:30:00.000Z").unwrap();
	assert_eq!(count_comments(&timed, since).unwrap(), ReviewNotes { notes: 1, blocking: 0 });
	let fake = fake_hunk(j.root());
	assert_eq!(list_comments(&fake, j.root(), 0, HUNK_PORT).await.unwrap().notes, 2);
	let failing = if cfg!(windows) { strings(&["cmd", "/c", "exit", "3"]) } else { strings(&["sh", "-c", "exit 3"]) };
	let error = list_comments(&failing, j.root(), 0, HUNK_PORT).await.unwrap_err().to_string();
	assert!(error.starts_with("hunk session comment list failed (3)"), "{error}");
}

#[test]
fn notes_the_reviewer_marked_p0_or_p1_are_blocking_0453() {
	let json = json!({
		"comments": [
			{ "summary": "[P0] Data loss on retry" },
			{ "summary": "[P1] Race under load" },
			{ "summary": "[P2] Edge case" },
			{ "summary": "P3 nit" },
			{ "summary": "No priority" },
		],
	})
	.to_string();
	assert_eq!(count_comments(&json, 0).unwrap(), ReviewNotes { notes: 5, blocking: 2 });
}

#[tokio::test]
async fn the_reviews_proof_diff_size_changed_test_files_and_added_skip_only_markers_from_one_diff_of_the_test_files() {
	let j = temp_jocasta();
	let repo = make_repo(&j.root().join("proof"));
	git(&repo, &["checkout", "-q", "-b", "feature/x"]);
	write_file(&repo, "app.ts", "export const a = 1;\nexport const b = 2;\n");
	write_file(&repo, "app.test.ts", "test.skip(\"a\", () => {});\n");
	write_file(&repo, "other.test.ts", "test(\"b\", () => {});\n");
	git(&repo, &["add", "."]);
	git(&repo, &["commit", "-q", "-m", "work"]);
	let proof = review_proof(&repo, "main...feature/x").await.unwrap();
	assert_eq!(
		proof,
		Some(ReviewProof {
			added: 4,
			deleted: 0,
			files: 3,
			test_files: strings(&["app.test.ts", "other.test.ts"]),
			skipped: strings(&["app.test.ts"]),
			removed_tests: Some(vec![]),
			fewer_assertions: Some(vec![]),
		})
	);
	let said = proof_said(&ReviewProof {
		added: 3,
		files: 2,
		test_files: strings(&["app.test.ts"]),
		skipped: strings(&["app.test.ts"]),
		..ReviewProof::default()
	});
	assert_eq!(said, "+3/−0 in 2 files, test files changed: app.test.ts, adds skip/only in app.test.ts");
	let one = ReviewProof { added: 1, deleted: 1, files: 1, ..ReviewProof::default() };
	assert_eq!(proof_said(&one), "+1/−1 in 1 file, no test files changed");
}

/// A repo whose trunk holds `base` and whose `feature/x` then applies `change` (a path and its text, or `None` to
/// delete it): its proof.
async fn proof_of(j: &TempJocasta, name: &str, base: &[(&str, &str)], change: &[(&str, Option<&str>)]) -> ReviewProof {
	let repo = make_repo(&j.root().join(name));
	for (file, text) in base {
		write_file(&repo, file, text);
	}
	git(&repo, &["add", "."]);
	git(&repo, &["commit", "-q", "-m", "base"]);
	git(&repo, &["checkout", "-q", "-b", "feature/x"]);
	for (file, text) in change {
		match text {
			None => std::fs::remove_file(repo.join(file)).unwrap(),
			Some(text) => write_file(&repo, file, text),
		}
	}
	git(&repo, &["add", "-A"]);
	git(&repo, &["commit", "-q", "-m", "work"]);
	review_proof(&repo, "main...feature/x").await.unwrap().expect("a proof")
}

const TWO_TESTS: &str = "import { expect, test } from \"bun:test\";\ntest(\"a\", () => {\n  expect(1).toBe(1);\n});\ntest(\"b\", () => {\n  expect(2).toBe(2);\n});\n";

#[tokio::test]
async fn the_proof_finds_a_removed_test_a_deleted_test_file_a_commented_out_test_0610() {
	let j = temp_jocasta();
	let one = TWO_TESTS.split("test(\"b\"").next().unwrap();
	let removed = proof_of(&j, "removed", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(one))]).await;
	assert_eq!(removed.removed_tests, Some(strings(&["a.test.ts"])));
	assert_eq!(removed.fewer_assertions, Some(strings(&["a.test.ts"])));
	assert_eq!(removed.skipped, Vec::<String>::new());
	let deleted =
		proof_of(&j, "deleted", &[("test/a.test.ts", TWO_TESTS), ("src.ts", "x\n")], &[("test/a.test.ts", None)]).await;
	assert_eq!(deleted.removed_tests, Some(strings(&["test/a.test.ts"])));
	let commented_out = TWO_TESTS.replace("test(\"b\", () => {", "// test(\"b\", () => {");
	let commented =
		proof_of(&j, "commented", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(commented_out.as_str()))]).await;
	assert_eq!(commented.removed_tests, Some(strings(&["a.test.ts"])));
}

#[tokio::test]
async fn the_proof_counts_a_moved_or_renamed_test_file_and_a_test_renamed_in_place_as_no_removed_tests_0610() {
	let j = temp_jocasta();
	let moved =
		proof_of(&j, "moved", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", None), ("test/b.test.ts", Some(TWO_TESTS))])
			.await;
	assert_eq!(moved.removed_tests, Some(vec![]));
	assert_eq!(moved.fewer_assertions, Some(vec![]));
	let renamed_text = TWO_TESTS.replace("test(\"b\"", "test(\"b, renamed\"");
	let renamed =
		proof_of(&j, "renamed", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(renamed_text.as_str()))]).await;
	assert_eq!(renamed.removed_tests, Some(vec![]));
	assert_eq!(renamed.fewer_assertions, Some(vec![]));
}

#[tokio::test]
async fn the_proof_finds_fewer_assertions_in_a_changed_test_file_and_none_in_a_clean_change_0610() {
	let j = temp_jocasta();
	let fewer = TWO_TESTS.replace("  expect(2).toBe(2);\n", "");
	let weaker = proof_of(&j, "weaker", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(fewer.as_str()))]).await;
	assert_eq!(weaker.fewer_assertions, Some(strings(&["a.test.ts"])));
	assert_eq!(weaker.removed_tests, Some(vec![]));
	let skipping = TWO_TESTS.replace("test(\"b\"", "test.skip(\"b\"");
	let skip = proof_of(&j, "skip", &[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(skipping.as_str()))]).await;
	assert_eq!(skip.skipped, strings(&["a.test.ts"]));
	let more = format!("{TWO_TESTS}test(\"c\", () => {{\n  expect(3).toBe(3);\n}});\n");
	let clean = proof_of(
		&j,
		"clean",
		&[("a.test.ts", TWO_TESTS), ("app.ts", "export const a = 1;\n")],
		&[("a.test.ts", Some(more.as_str())), ("app.ts", Some("export const a = 2;\n"))],
	)
	.await;
	assert_eq!(clean.removed_tests, Some(vec![]));
	assert_eq!(clean.fewer_assertions, Some(vec![]));
	assert_eq!(clean.skipped, Vec::<String>::new());
}

#[tokio::test]
async fn the_proof_reads_python_go_rust_and_ruby_tests_0610() {
	let j = temp_jocasta();
	let base = [
		("test_a.py", "def test_a():\n    assert 1 == 1\n\n\ndef test_b():\n    assert 2 == 2\n"),
		(
			"a_test.go",
			"func TestA(t *testing.T) {\n\tt.Errorf(\"x\")\n}\n\nfunc TestB(t *testing.T) {\n\tt.Errorf(\"y\")\n}\n",
		),
		(
			"tests/a.rs",
			"#[test]\nfn a() {\n    assert_eq!(1, 1);\n}\n\n#[tokio::test]\nasync fn b() {\n    assert!(true);\n}\n",
		),
		("spec/a_spec.rb", "it \"a\" do\n  expect(1).to eq(1)\nend\n\nit \"b\" do\n  expect(2).to eq(2)\nend\n"),
	];
	let proof = proof_of(
		&j,
		"languages",
		&base,
		&[
			("test_a.py", Some("def test_a():\n    assert 1 == 1\n")),
			("a_test.go", Some("func TestA(t *testing.T) {\n\tt.Errorf(\"x\")\n}\n")),
			("tests/a.rs", Some("#[test]\nfn a() {\n    assert_eq!(1, 1);\n}\n")),
			("spec/a_spec.rb", Some("it \"a\" do\n  expect(1).to eq(1)\nend\n")),
		],
	)
	.await;
	let all = Some(strings(&["a_test.go", "spec/a_spec.rb", "test_a.py", "tests/a.rs"]));
	assert_eq!(proof.removed_tests, all);
	assert_eq!(proof.fewer_assertions, all);
}

#[test]
fn a_stored_proof_from_before_0610_says_what_it_knew_the_new_facts_are_said_when_present_0610() {
	let old = ReviewProof { added: 1, files: 1, test_files: strings(&["a.test.ts"]), ..ReviewProof::default() };
	assert_eq!(proof_said(&old), "+1/−0 in 1 file, test files changed: a.test.ts");
	let newer = ReviewProof {
		removed_tests: Some(strings(&["a.test.ts"])),
		fewer_assertions: Some(strings(&["a.test.ts", "b.test.ts"])),
		..old.clone()
	};
	assert_eq!(
		proof_said(&newer),
		"+1/−0 in 1 file, test files changed: a.test.ts, removes tests in a.test.ts, fewer assertions in a.test.ts, b.test.ts"
	);
	// A stored proof has the TypeScript's keys, and a pre-0610 one has neither new key.
	let stored: ReviewProof =
		serde_json::from_str(r#"{"added":1,"deleted":0,"files":1,"testFiles":["a.test.ts"],"skipped":[]}"#).unwrap();
	assert_eq!(stored, old);
	assert_eq!(
		serde_json::to_string(&newer).unwrap(),
		r#"{"added":1,"deleted":0,"files":1,"testFiles":["a.test.ts"],"skipped":[],"removedTests":["a.test.ts"],"fewerAssertions":["a.test.ts","b.test.ts"]}"#
	);
}

#[test]
fn the_review_prompt_gives_the_reviewer_the_shipped_design_rubric_and_a_cap_of_three_non_blocking_design_findings_0611()
{
	let prompt = review_prompt(&strings(&["/opt/hunk"]), "main...b", &ReviewContext::default());
	assert!(
		prompt.contains(&format!("{}/design-rubric.md", shipped_config_dir(None).to_string_lossy().replace('\\', "/")))
	);
	assert!(prompt.contains("at most three design findings"));
	assert!(prompt.contains("the code this change could delete"));
	assert!(prompt.contains("[P3]"));
	assert!(prompt.contains("never block"));
	// A design note as the prompt asks for it is not blocking.
	let design = json!({ "comments": [{ "summary": "[P3] Design: a pass-through method" }] }).to_string();
	assert_eq!(count_comments(&design, 0).unwrap().blocking, 0);
	assert!(shipped_config_dir(None).join("design-rubric.md").exists());
}

#[test]
fn hunks_argv_and_environment() {
	assert_eq!(
		hunk_diff_args(&strings(&["/opt/hunk"]), "main...b"),
		strings(&["/opt/hunk", "diff", "--theme", "tokyo-night", "main...b", "--watch"])
	);
	assert_eq!(
		hunk_env(HUNK_PORT),
		vec![
			("HUNK_MCP_PORT".to_owned(), "47658".to_owned()),
			("HUNK_DISABLE_UPDATE_NOTICE".to_owned(), "1".to_owned())
		]
	);
}

#[tokio::test]
async fn the_daemon_is_started_once_reports_its_pid_and_is_left_alone_when_it_answers() {
	let j = temp_jocasta();
	let port = {
		let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		probe.local_addr().unwrap().port()
	};
	let fake = fake_hunk(j.root());
	assert!(!daemon_up(port).await);
	let pid = AtomicU32::new(0);
	let record = |started: u32| pid.store(started, Ordering::SeqCst);
	let first = ensure_hunk_daemon(&fake, port, Some(&record)).await;
	let started = pid.load(Ordering::SeqCst);
	let up = daemon_up(port).await;
	let again = AtomicU32::new(0);
	let second = ensure_hunk_daemon(&fake, port, Some(&|started| again.store(started, Ordering::SeqCst))).await;
	if started != 0 {
		kill_tree(started).await;
	}
	first.unwrap();
	second.unwrap();
	assert_ne!(started, 0);
	assert!(up);
	assert_eq!(again.load(Ordering::SeqCst), 0);
	assert!(wait_for(|| async { Ok(!daemon_up(port).await) }, 5_000, 50).await.unwrap());
}

#[tokio::test]
async fn a_daemon_that_never_answers_is_an_error_and_no_program_is_no_start() {
	let port = {
		let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		probe.local_addr().unwrap().port()
	};
	ensure_hunk_daemon(&[], port, None).await.unwrap();
	let silent = if cfg!(windows) { strings(&["cmd", "/c", "exit", "0"]) } else { strings(&["true"]) };
	let error = ensure_hunk_daemon(&silent, port, None).await.unwrap_err().to_string();
	assert_eq!(error, format!("hunk's session daemon didn't start on 127.0.0.1:{port}"));
}
