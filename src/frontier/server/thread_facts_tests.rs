//! Tests of `thread_facts.rs` (0668).

use std::fs;

use serde_json::json;

use super::*;
use crate::frontier::server::helpers::{commit_in, copy_repo, sh};
use crate::shared::testing::temp_dir;

fn pr(number: u64, ci: PrCi) -> PrFacts {
	PrFacts { number, ci }
}

fn exit(command: &str, code: Option<i32>) -> CheckExit {
	CheckExit { command: command.to_owned(), exit: code }
}

#[test]
fn the_shortstat_is_lines_added_and_removed_and_none_without_a_change() {
	assert_eq!(
		parse_shortstat(" 3 files changed, 84 insertions(+), 12 deletions(-)"),
		Some(ThreadDiff { added: 84, removed: 12 })
	);
	assert_eq!(parse_shortstat(" 1 file changed, 1 insertion(+)"), Some(ThreadDiff { added: 1, removed: 0 }));
	assert_eq!(parse_shortstat(" 1 file changed, 4 deletions(-)"), Some(ThreadDiff { added: 0, removed: 4 }));
	assert_eq!(parse_shortstat(""), None);
}

#[tokio::test]
async fn the_diff_is_against_the_merge_base_with_trunk_and_counts_uncommitted_work() {
	let dir = temp_dir("thread-facts");
	let repo = copy_repo(&dir.path().join("demo"));
	sh(&repo, &["checkout", "-q", "-b", "feature/x"]);
	assert_eq!(diff_of(&repo, "main").await, None, "nothing changed yet");
	commit_in(&repo, "demo\none\ntwo\n", "two lines");
	assert_eq!(diff_of(&repo, "main").await, Some(ThreadDiff { added: 2, removed: 0 }));
	fs::write(repo.join("README.md"), "one\ntwo\nthree\n").unwrap();
	assert_eq!(diff_of(&repo, "main").await, Some(ThreadDiff { added: 3, removed: 1 }));
	assert_eq!(diff_of(&repo, "no-such-trunk").await, None, "git failing says nothing");
}

#[test]
fn a_check_is_passed_when_every_check_that_ran_exited_zero() {
	assert_eq!(check_of(&[]), None);
	assert_eq!(check_of(&[exit("mise run check", Some(0)), exit("done", Some(0))]), Some(true));
	assert_eq!(check_of(&[exit("mise run check", Some(0)), exit("done", Some(1))]), Some(false));
	assert_eq!(check_of(&[exit("mise run check", None)]), Some(false), "a check that didn't run hasn't passed");
}

#[test]
fn a_noted_check_shows_at_once_and_unknown_worktrees_read_as_nothing() {
	let facts = ThreadFacts::new();
	let path = Path::new("/work/acme");
	assert_eq!(facts.get(path), Facts::default());
	facts.note_check(path, false, 1_000);
	assert_eq!(facts.get(path).check, Some(false));
	facts.note_check(path, true, 2_000);
	assert_eq!(facts.get(path).check, Some(true));
	assert_eq!(facts.get(Path::new("/work/other")), Facts::default());
}

#[test]
fn a_pr_is_read_from_gh_json() {
	let view = |state: &str, rollup: serde_json::Value| {
		pr_from_json(&json!({"number": 7, "state": state, "statusCheckRollup": rollup}).to_string())
	};
	let run = |conclusion: &str| json!({"__typename": "CheckRun", "status": "COMPLETED", "conclusion": conclusion});
	let running = json!({"__typename": "CheckRun", "status": "IN_PROGRESS", "conclusion": ""});
	assert_eq!(view("OPEN", json!([run("SUCCESS"), run("SKIPPED")])), Some(pr(7, PrCi::Passing)));
	assert_eq!(view("OPEN", json!([run("SUCCESS"), run("FAILURE")])), Some(pr(7, PrCi::Failing)));
	assert_eq!(view("OPEN", json!([run("SUCCESS"), running.clone()])), Some(pr(7, PrCi::Pending)));
	assert_eq!(view("OPEN", json!([running, run("FAILURE")])), Some(pr(7, PrCi::Failing)));
	assert_eq!(view("OPEN", json!([{"__typename": "StatusContext", "state": "PENDING"}])), Some(pr(7, PrCi::Pending)));
	assert_eq!(view("OPEN", json!([{"__typename": "StatusContext", "state": "ERROR"}])), Some(pr(7, PrCi::Failing)));
	assert_eq!(view("OPEN", json!([])), Some(pr(7, PrCi::Pending)), "no check reported yet");
	assert_eq!(view("MERGED", json!([run("FAILURE")])), Some(pr(7, PrCi::Merged)));
	assert_eq!(view("CLOSED", json!([])), Some(pr(7, PrCi::Closed)));
	assert_eq!(pr_from_json("not json"), None);
	assert_eq!(pr_from_json(r#"{"state": "OPEN"}"#), None);
}

#[test]
fn a_pr_is_asked_again_after_2_minutes_while_pending_10_once_settled_and_never_once_merged_or_closed() {
	let facts = ThreadFacts::new();
	let url = "https://example.org/acme/api/pull/7";
	assert_eq!(facts.pr_facts(url), None, "the first ask only registers it");
	assert_eq!(facts.prs_due(0), vec![url.to_owned()]);
	facts.pr_answered(url, Some(pr(7, PrCi::Pending)), 1_000);
	assert_eq!(facts.pr_facts(url), Some(pr(7, PrCi::Pending)));
	assert!(facts.prs_due(1_000 + 2 * MINUTE - 1).is_empty());
	assert_eq!(facts.prs_due(1_000 + 2 * MINUTE).len(), 1);
	facts.pr_answered(url, Some(pr(7, PrCi::Passing)), 200_000);
	assert!(facts.prs_due(200_000 + 10 * MINUTE - 1).is_empty());
	assert_eq!(facts.prs_due(200_000 + 10 * MINUTE).len(), 1);
	facts.pr_answered(url, Some(pr(7, PrCi::Merged)), 900_000);
	assert!(facts.prs_due(i64::MAX).is_empty(), "no polling after merged");
	assert_eq!(facts.pr_facts(url), Some(pr(7, PrCi::Merged)));
}

#[test]
fn a_failing_gh_backs_off_from_2_to_30_minutes_keeps_the_last_good_facts_and_is_logged_once() {
	let facts = ThreadFacts::new();
	let url = "https://example.org/acme/api/pull/8";
	facts.pr_facts(url);
	facts.pr_answered(url, Some(pr(8, PrCi::Pending)), 0);
	let mut now = 2 * MINUTE;
	let mut waited = Vec::new();
	let mut first = Vec::new();
	for _ in 0..6 {
		assert_eq!(facts.prs_due(now).len(), 1);
		first.push(facts.pr_answered(url, None, now));
		let next = lock(&facts.state).prs[url].next_at;
		waited.push((next - now) / MINUTE);
		now = next;
	}
	assert_eq!(waited, vec![2, 4, 8, 16, 30, 30]);
	assert_eq!(first, vec![true, false, false, false, false, false], "said once");
	assert_eq!(facts.pr_facts(url), Some(pr(8, PrCi::Pending)), "the last good value stays");
	facts.pr_answered(url, Some(pr(8, PrCi::Passing)), now);
	assert_eq!(lock(&facts.state).prs[url].errors, 0, "an answer ends the run of failures");
	assert!(facts.pr_answered(url, None, now + MINUTE), "and the next failure is said again");
}

#[test]
fn at_most_ten_prs_are_watched_the_one_asked_about_longest_ago_goes() {
	let facts = ThreadFacts::new();
	let url = |n: usize| format!("https://example.org/acme/api/pull/{n}");
	for n in 0..10 {
		facts.pr_facts(&url(n));
	}
	facts.pr_facts(&url(0));
	facts.pr_facts(&url(10));
	let state = lock(&facts.state);
	assert_eq!(state.prs.len(), 10);
	assert!(state.prs.contains_key(&url(0)) && state.prs.contains_key(&url(10)));
	assert!(!state.prs.contains_key(&url(1)));
}

/// Commits `files` (name, text) in `repo` as one commit.
fn commit_files(repo: &Path, files: &[(&str, &str)], message: &str) {
	for (name, text) in files {
		fs::write(repo.join(name), text).unwrap();
	}
	sh(repo, &["add", "."]);
	sh(repo, &["commit", "-q", "-m", message]);
}

#[tokio::test]
async fn a_review_gets_its_ranges_commits_and_files_under_the_facts_line_computed_by_code_0685() {
	let dir = temp_dir("thread-facts");
	let repo = copy_repo(&dir.path().join("demo"));
	sh(&repo, &["checkout", "-q", "-b", "feature/x"]);
	commit_files(&repo, &[("a.txt", "a\n"), ("b.txt", "b\n")], "Add a and b");
	commit_files(&repo, &[("c.txt", "c\n")], "Add c");
	commit_files(&repo, &[("d.txt", "d\n"), ("e.txt", "e\ne\n")], "Add d and e");
	let sha = sh(&repo, &["rev-parse", "--short=7", "HEAD"]);
	let checks = [CheckFact { command: "mise run verify".to_owned(), exit: Some(0) }];
	let facts = review_facts(&repo, "main...feature/x", &checks, Some("0123456789abcdef"), None).await;
	let lines: Vec<&str> = facts.lines().collect();
	assert_eq!(
		lines[0],
		format!(
			"Facts as of {sha} (computed by Jocasta; don't re-run git status, git log or git diff --stat for them):"
		)
	);
	assert_eq!(lines[1], "Commits (3):");
	assert!(lines[2..5].iter().all(|line| line.starts_with("  ")));
	assert!(lines[2].ends_with("Add d and e") && lines[4].ends_with("Add a and b"), "{facts}");
	assert_eq!(lines[5], "Changed files, added/removed (5):");
	assert_eq!(lines[6..11].iter().filter(|line| line.contains(".txt")).count(), 5, "{facts}");
	assert!(lines[6..11].contains(&"  2 0 e.txt"), "{facts}");
	assert_eq!(lines[11], "Checks on tree 012345:");
	assert_eq!(lines[12], "  `mise run verify` exit 0");
	assert_eq!(lines.len(), 13);
}

#[tokio::test]
async fn a_start_gets_branch_trunk_head_and_the_mise_tasks_0685() {
	let dir = temp_dir("thread-facts");
	let repo = copy_repo(&dir.path().join("demo"));
	let sha = sh(&repo, &["rev-parse", "--short=7", "HEAD"]);
	let tasks = ["quick".to_owned(), "verify".to_owned()];
	let facts = start_facts(&repo, Some("main"), "feature/x", &tasks).await;
	assert_eq!(
		facts,
		format!(
			"Facts as of {sha} (computed by Jocasta; don't re-run git status, git log or git diff --stat for them):\n- branch: feature/x\n- trunk: main\n- HEAD: {sha}\n- mise tasks: quick, verify"
		)
	);
	let none = start_facts(&repo, None, "feature/x", &[]).await;
	assert!(none.contains("- trunk: none") && none.ends_with("- mise tasks: none of quick, verify, test"), "{none}");
}

#[tokio::test]
async fn a_block_over_60_lines_is_written_whole_and_the_prompt_carries_40_lines_and_the_path_0685() {
	let dir = temp_dir("thread-facts");
	let repo = copy_repo(&dir.path().join("demo"));
	sh(&repo, &["checkout", "-q", "-b", "feature/x"]);
	let names: Vec<String> = (0..70).map(|n| format!("f{n:02}.txt")).collect();
	let files: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "x\n")).collect();
	commit_files(&repo, &files, "Add many");
	let file = dir.path().join("facts.md");
	let facts = review_facts(&repo, "main...feature/x", &[], None, Some(&file)).await;
	let lines: Vec<&str> = facts.lines().collect();
	assert_eq!(lines.len(), 41);
	assert_eq!(lines[40], format!("Full facts: {}", file.display()));
	let whole = fs::read_to_string(&file).unwrap();
	assert!(whole.contains("  … 10 more files"), "{whole}");
	assert_eq!(whole.lines().count(), 1 + 1 + 1 + 1 + 60 + 1);
	assert!(whole.starts_with(&lines[..40].join("\n")));
}
