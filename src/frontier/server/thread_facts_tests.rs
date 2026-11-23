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
