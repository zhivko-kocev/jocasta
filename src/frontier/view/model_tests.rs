use serde_json::json;

use super::*;
use crate::frontier::brief::model::{DiffHunk, DiffLine, Kind, Proof, Severity};

fn card(id: &str, text: &str) -> Card {
	Card {
		id: id.to_owned(),
		kind: Kind::Finding,
		severity: Severity::P1,
		file: "src/a.rs".to_owned(),
		line: 3,
		quote: "let a = 1;".to_owned(),
		text: text.to_owned(),
		verdict: Verdict::Post,
		proof: Proof { sha: "abc1234".to_owned(), ok: true },
		edited: None,
		published: None,
		ran: None,
	}
}

fn brief(cards: Vec<Card>) -> Brief {
	Brief {
		id: "pr-p1-7".to_owned(),
		kind: "pr".to_owned(),
		title: "PR #7".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 0,
		sections: vec![
			Section::Text { heading: None, body: "Intro".to_owned() },
			Section::Cards { heading: None, cards },
		],
	}
}

#[test]
fn each_kind_of_brief_opens_with_the_app_id_ultron_places() {
	assert_eq!(app_id("thread-task-1"), "jocasta-review");
	assert_eq!(app_id("pr-p1-7"), "jocasta-review");
	assert_eq!(app_id("show-3"), "jocasta-show");
	assert_eq!(app_id("morning-2026-11-27"), "jocasta-brief");
	assert_eq!(app_id("weekly-2026-W48"), "jocasta-brief");
}

#[test]
fn the_diff_waits_behind_a_count_of_files_and_lines() {
	let line = |kind: &str| DiffLine { kind: kind.to_owned(), old: None, new: None, text: "x".to_owned() };
	let hunk = DiffHunk { header: "@@".to_owned(), lines: vec![line("+"), line("+"), line("-"), line(" ")] };
	let file = |path: &str| DiffFile { path: path.to_owned(), hunks: vec![hunk.clone()] };
	assert_eq!(diff_summary(&[file("a")]), "Show code (1 file, +2 \u{2212}1)");
	assert_eq!(diff_summary(&[file("a"), file("b")]), "Show code (2 files, +4 \u{2212}2)");
}

#[test]
fn times_and_exits_say_what_happened_instead_of_zero_or_nothing() {
	assert_eq!(duration(None), "not timed");
	assert_eq!(duration(Some(0)), "not timed");
	assert_eq!(duration(Some(340)), "340 ms");
	assert_eq!(duration(Some(1_250)), "1.2 s");
	assert_eq!(duration(Some(125_000)), "2 min 5 s");
	assert_eq!(exit(None), "didn't run");
	assert_eq!(exit(Some(1)), "exit 1");
	assert_eq!(works_state(WorksState::NotRun), "not run");
	let ran = RanProof { run: "cargo test".to_owned(), expect: "ok".to_owned(), exit: None, tail: "boom\n".to_owned() };
	assert_eq!(proof_text(&ran), "$ cargo test\nexpect: ok\nexit: didn't end\nboom");
}

#[test]
fn a_cards_words_are_his_edit_and_publish_waits_for_a_checked_kept_unpublished_card() {
	let mut shown = card("n1", "Reviewer's words");
	assert_eq!(card_text(&shown), "Reviewer's words");
	shown.edited = Some(String::new());
	assert_eq!(card_text(&shown), "Reviewer's words");
	shown.edited = Some("His words".to_owned());
	assert_eq!(card_text(&shown), "His words");
	assert!(publishable(&shown));
	shown.verdict = Verdict::Skip;
	assert!(!publishable(&shown));
	shown.verdict = Verdict::Question;
	shown.proof.ok = false;
	assert!(!publishable(&shown));
}

#[test]
fn each_button_sends_the_op_the_service_takes() {
	let card = || "n1".to_owned();
	let cases = [
		(Act::Edit { card: card(), text: "t".to_owned() }, json!({"op": "edit", "card": "n1", "text": "t"})),
		(
			Act::Reword { card: card(), instruction: "shorter".to_owned() },
			json!({"op": "reword", "card": "n1", "instruction": "shorter"}),
		),
		(
			Act::Verdict { card: card(), verdict: Verdict::Skip },
			json!({"op": "verdict", "card": "n1", "verdict": "skip"}),
		),
		(Act::Publish { card: card() }, json!({"op": "publish", "card": "n1"})),
		(Act::Finish { mode: "merge" }, json!({"op": "finish", "mode": "merge"})),
		(Act::Send { text: "again".to_owned() }, json!({"op": "send", "text": "again"})),
	];
	for (act, wire) in cases {
		assert_eq!(act.to_json(), wire);
		assert_eq!(act.card().is_some(), wire.get("card").is_some());
	}
	assert_eq!(Act::Finish { mode: "pr" }.waiting(), "Waiting for your Allow in Jocasta\u{2026}");
}

#[test]
fn an_answer_patches_only_its_card_and_carries_a_rewords_suggestion() {
	let mut shown = brief(vec![card("n1", "one"), card("n2", "two")]);
	let mut after = brief(vec![card("n1", "one"), card("n2", "two")]);
	after.sections[0] = Section::Text { heading: None, body: "Changed elsewhere".to_owned() };
	if let Section::Cards { cards, .. } = &mut after.sections[1] {
		cards[0].edited = Some("his".to_owned());
		cards[1].text = "changed by someone else".to_owned();
	}
	let mut value = serde_json::to_value(&after).unwrap();
	value["suggestion"] = json!({"card": "n1", "text": "shorter"});
	let (answer, suggestion) = answer_of(&value).unwrap();
	assert_eq!(suggestion, Some(("n1".to_owned(), "shorter".to_owned())));
	assert!(patch_card(&mut shown, &answer, "n1"));
	assert_eq!(card_text(card_of(&shown, "n1")), "his");
	assert_eq!(card_text(card_of(&shown, "n2")), "two");
	assert_eq!(shown.sections[0], Section::Text { heading: None, body: "Intro".to_owned() });
	assert!(!patch_card(&mut shown, &answer, "n9"));
	assert!(answer_of(&json!({"error": "x"})).is_err());
}

fn card_of<'a>(brief: &'a Brief, id: &str) -> &'a Card {
	super::card(brief, id).unwrap()
}

#[test]
fn sections_know_when_they_are_empty_and_text_parts_at_blank_lines() {
	assert!(is_empty(&Section::Cards { heading: None, cards: vec![] }));
	assert!(is_empty(&Section::Text { heading: None, body: " \n".to_owned() }));
	assert!(!is_empty(&brief(vec![card("n1", "x")]).sections[1]));
	assert_eq!(paragraphs("one\nline\n\n\n two \n\n"), vec!["one\nline", "two"]);
}

#[test]
fn a_screenshot_is_read_only_from_its_briefs_folder() {
	let _j = crate::shared::testing::temp_jocasta();
	let path = screenshot("thread-t1", "shot-1.png").unwrap();
	assert!(path.ends_with("briefs/assets/thread-t1/shot-1.png"), "{path:?}");
	for (id, name) in [("../x", "a.png"), ("thread-t1", "../a.png"), ("thread-t1", ".a.png"), ("thread-t1", "a.svg")] {
		assert_eq!(screenshot(id, name), None, "{id} {name}");
	}
}
