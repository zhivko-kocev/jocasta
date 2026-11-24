//! Tests of `review_proof.rs` (moved from `hunk_tests.rs`, 0679).

use std::path::{Path, PathBuf};

use super::*;
use crate::shared::env;
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
