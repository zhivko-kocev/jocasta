use super::*;

// The direct calls of test/frontier/server/service-finish.test.ts and service-thread-start.test.ts.
#[test]
fn outside_text_in_an_event_line_is_one_line_in_guillemets_with_none_of_its_own_sec_2() {
	assert_eq!(quoted("a «b»\n\tc"), "«a \"b\" c»");
	assert_eq!(quoted("  padded  "), "«padded»");
}

#[test]
fn a_model_as_the_sidebar_shows_it() {
	assert_eq!(short_model("anthropic/claude-opus-5-5"), "opus-5-5");
	assert_eq!(short_model("openai/gpt-5.5"), "gpt-5.5");
	assert_eq!(short_model("claude-haiku"), "haiku");
}

#[test]
fn a_thread_title_is_the_briefs_first_sentence_in_six_words_and_40_characters() {
	assert_eq!(brief_title("Add a README line. Then more."), "Add a README line");
	assert_eq!(brief_title("Fix the login\nand the logout flow"), "Fix the login");
	assert_eq!(brief_title("one two three four five six seven eight"), "one two three four five six");
	assert_eq!(
		brief_title("a-very-long-single-word-that-will-not-fit-in-forty-characters"),
		"a-very-long-single-word-that-will-not-fi"
	);
	assert_eq!(brief_title("  Why? Because.  "), "Why");
	assert_eq!(brief_title(""), "");
}

#[test]
fn a_branch_slug_is_lowercase_ascii_without_marks_in_40_characters() {
	assert_eq!(branch_slug("Add a README line"), "add-a-readme-line");
	assert_eq!(branch_slug("Café déjà vu — Ünïcode"), "cafe-deja-vu-unicode");
	assert_eq!(branch_slug("Straße æther Øl łódź"), "strasse-aether-ol-lodz");
	assert_eq!(branch_slug("Привет мир"), "privet-mir");
	assert_eq!(branch_slug("Ѓубре и Щука"), "gjubre-i-shchuka");
	assert_eq!(branch_slug("Ｆｕｌｌｗｉｄｔｈ ﬁle ½"), "-fullwidth-file-1-2".trim_start_matches('-'));
	assert_eq!(branch_slug("--- spaced   out ---"), "spaced-out");
	assert_eq!(branch_slug("修复登录问题"), "");
	assert_eq!(branch_slug(&"abcd ".repeat(20)), "abcd-abcd-abcd-abcd-abcd-abcd-abcd-abcd");
}

#[test]
fn a_title_with_no_slug_gets_thread_day_n_by_the_local_day() {
	let at = chrono::Local.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap().timestamp_millis();
	assert_eq!(fallback_slug(at, 1), "thread-20261014-1");
	assert_eq!(fallback_slug(at, 12), "thread-20261014-12");
}

use chrono::TimeZone;

#[test]
fn the_service_tells_its_own_pane_lines_from_his() {
	assert!(by_service("[event] thread finished"));
	assert!(by_service("  \n/handoff Keep"));
	assert!(by_service(REPORT_STEER));
	assert!(by_service(&idle_check_text("checks: FAIL")));
	assert!(by_service(PLAN_REVIEW_COMMAND));
	assert!(by_service(&pr_request("origin", "feature/x", "main")));
	assert!(by_service(&format!("{REVIEW_LEAD} Read the diff.")));
	assert!(!by_service("please /handoff"));
	assert!(!by_service(LESSONS_LINE));
}

#[test]
fn the_texts_keep_their_words() {
	assert_eq!(idle_check_text("checks: FAIL"), format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL"),);
	assert_eq!(
		pr_request("origin", "feature/x", "main"),
		"Open a pull request for this branch: run `git push -u origin feature/x`, then `gh pr create --base main --head feature/x` with a title and body that sum up the branch's commits. Don't change code in this turn. End with the PR's URL, or with why it couldn't be opened."
	);
	assert_eq!(WHEN_YOU_FINISH.join("\n").lines().count(), 4);
}

#[test]
fn doors_name_him_and_the_asker() {
	assert_eq!(start_prefix(Door::Boss), "thread_start");
	assert_eq!(start_prefix(Door::Cli), "jocasta thread new");
	assert_eq!(start_prefix(Door::Key), "New thread");
	assert_eq!(person(Door::Boss), Person { he: "he", his: "his", he_cap: "He", him: "him" });
	assert_eq!(person(Door::Key), Person { he: "you", his: "your", he_cap: "You", him: "you" });
	assert_eq!(asker(Door::Cli, "thread new"), "A `jocasta thread new` command");
	assert_eq!(asker(Door::Boss, "thread new"), "The boss");
	// A question his own key opened (0670) is his, not the boss's.
	assert_eq!(asker(Door::Key, "worktree remove"), "You");
	assert_eq!((asks(Door::Key), asks(Door::Boss), asks(Door::Cli)), ("asked", "asks", "asks"));
}

#[test]
fn money_cut_and_the_review_texts() {
	assert_eq!(money(1.75), "$1.75");
	assert_eq!(money(0.0), "$0.00");
	assert_eq!(money(10.754), "$10.75");
	assert_eq!(money(0.125), "$0.13");
	assert_eq!(money(2.5), "$2.50");
	assert_eq!(cut("short", 10), "short");
	assert_eq!(cut("a longer text here", 8), "a longe…");
	assert_eq!(cut("ab cdefgh", 4), "ab…");
	assert_eq!(review_said(Some(2), TestsResult::Pass, Some(1)), "2 notes (1 blocking), tests pass");
	assert_eq!(review_said(Some(1), TestsResult::Fail, Some(0)), "1 note, tests fail");
	assert_eq!(review_said(None, TestsResult::Unknown, None), "notes not counted, tests unverified");
	assert_eq!(review_said(Some(0), TestsResult::None, None), "0 notes, no tests");
	let files = |n: usize| (0..n).map(|i| format!("f{i}.ts")).collect::<Vec<_>>();
	assert_eq!(
		overlap_said(
			&[Overlap { title: "A".into(), files: files(2) }, Overlap { title: "B".into(), files: files(6) },]
		),
		"A: f0.ts, f1.ts; B: f0.ts, f1.ts, f2.ts, f3.ts, f4.ts …"
	);
	assert_eq!(overlap_said(&[]), "");
}
