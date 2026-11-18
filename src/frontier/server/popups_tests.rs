use serde_json::json;

use super::*;

const ALLOW_BOX_DETAIL: &str = "Its branch stays.";

fn allow_box() -> PopupView {
	PopupView {
		title: "Jocasta: Allow or Deny".to_owned(),
		message: "Remove the leftover worktree acme-api/retry?".to_owned(),
		detail: Some(ALLOW_BOX_DETAIL.to_owned()),
		choices: vec!["Allow".to_owned(), "Deny".to_owned()],
		fallback: "Deny".to_owned(),
		code: None,
		form: None,
	}
}

fn one_working() -> Counts {
	Counts { working: 1, awaiting: 0 }
}

fn is_typed_code(code: &str) -> bool {
	code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase() && b != b'I' && b != b'O')
}

#[test]
fn the_close_question_counts_working_and_awaiting_threads_stop_everything_or_cancel() {
	assert_eq!(
		close_question(Counts { working: 1, awaiting: 0 }),
		"Stop 1 working thread? They pick up where they left off next time."
	);
	assert_eq!(
		close_question(Counts { working: 0, awaiting: 2 }),
		"Stop 2 threads awaiting you? They pick up where they left off next time."
	);
	assert_eq!(
		close_question(Counts { working: 2, awaiting: 1 }),
		"Stop 2 working threads and 1 awaiting you? They pick up where they left off next time."
	);
	let view = stop_question(Counts { working: 1, awaiting: 1 }, Some("ABC"));
	assert_eq!(view.choices, vec![STOP, CANCEL]);
	assert_eq!(view.fallback, CANCEL);
	assert_eq!(view.code.as_deref(), Some("ABC"));
}

#[tokio::test]
async fn popups_each_code_answers_once_with_one_of_its_own_choices_and_close_all_ends_the_rest_unanswered() {
	let popups = Popups::new();
	let a = popups.open(&allow_box(), false);
	let b = popups.open(&stop_question(one_working(), None), false);
	assert_eq!(popups.list(), vec![a.clone(), b.clone()]);
	assert_eq!(popups.view(a.code()), Some(allow_box()));
	assert_eq!(popups.answer(a.code(), &json!("Maybe")), AnswerTaken::Invalid);
	assert_eq!(popups.answer(b.code(), &json!(STOP)), AnswerTaken::Wrong);
	assert_eq!(popups.answer(a.code(), &json!("Deny")), AnswerTaken::Taken);
	assert_eq!(popups.answer(a.code(), &json!("Allow")), AnswerTaken::Unknown);
	assert_eq!(popups.view(a.code()), None);
	assert_eq!(a.answer().await, "Deny");
	popups.close_all();
	assert_eq!(b.answer().await, CANCEL);
	assert_eq!(popups.list(), Vec::<Popup>::new());
	assert_eq!(popups.answer(b.code(), &json!(STOP)), AnswerTaken::Unknown);
}

#[tokio::test]
async fn allow_takes_its_three_letters_only_enters_bare_allow_and_wrong_letters_dont_allow_the_third_wrong_code_denies()
{
	let popups = Popups::new();
	let view = allow_question("Remove it?", ALLOW_BOX_DETAIL, Some("KQX"));
	assert_eq!(view.choices, vec![ALLOW, DENY]);
	assert_eq!(view.fallback, DENY);
	assert_eq!(view.code.as_deref(), Some("KQX"));
	assert!(is_typed_code(allow_question("m", "d", None).code.as_deref().unwrap()));
	let right = popups.open(&view, false);
	assert_eq!(popups.answer(right.code(), &json!({"choice": ALLOW})), AnswerTaken::Wrong);
	assert_eq!(popups.answer(right.code(), &json!({"choice": ALLOW, "typed": "KQZ"})), AnswerTaken::Wrong);
	assert!(right.open());
	assert_eq!(popups.answer(right.code(), &json!({"choice": ALLOW, "typed": "kqx"})), AnswerTaken::Taken);
	assert_eq!(right.answer().await, ALLOW);

	let guessed = popups.open(&view, false);
	for typed in ["AAA", "BBB"] {
		assert_eq!(popups.answer(guessed.code(), &json!({"choice": ALLOW, "typed": typed})), AnswerTaken::Wrong);
	}
	assert_eq!(popups.answer(guessed.code(), &json!({"choice": ALLOW, "typed": "CCC"})), AnswerTaken::Exhausted);
	assert_eq!(guessed.answer().await, DENY);
	assert_eq!(WRONG_CODES, 3);
}

#[tokio::test]
async fn finishs_merge_and_pr_and_the_close_questions_stop_everything_take_the_code_cancel_doesnt_three_wrong_cancel() {
	let popups = Popups::new();
	let finish =
		popups.open(&finish_question("acme-api/retry", "2 commits on feature/retry, onto main.", Some("KQX")), false);
	assert_eq!(finish.view().choices, vec![MERGE, PR, CANCEL]);
	assert_eq!(finish.view().fallback, CANCEL);
	assert_eq!(finish.view().code.as_deref(), Some("KQX"));
	assert!(is_typed_code(finish_question("a", "d", None).code.as_deref().unwrap()));
	assert_eq!(popups.answer(finish.code(), &json!({"choice": MERGE})), AnswerTaken::Wrong);
	assert_eq!(popups.answer(finish.code(), &json!({"choice": PR, "typed": "KQZ"})), AnswerTaken::Wrong);
	assert_eq!(popups.answer(finish.code(), &json!({"choice": PR, "typed": "kqx"})), AnswerTaken::Taken);
	assert_eq!(finish.answer().await, PR);

	let guessed = popups.open(&finish_question("acme-api/retry", "d", Some("KQX")), false);
	for typed in ["AAA", "BBB"] {
		assert_eq!(popups.answer(guessed.code(), &json!({"choice": MERGE, "typed": typed})), AnswerTaken::Wrong);
	}
	assert_eq!(popups.answer(guessed.code(), &json!({"choice": MERGE, "typed": "CCC"})), AnswerTaken::Exhausted);
	assert_eq!(guessed.answer().await, CANCEL);

	let cancel = popups.open(&finish_question("acme-api/retry", "d", Some("KQX")), false);
	assert_eq!(popups.answer(cancel.code(), &json!({"choice": CANCEL})), AnswerTaken::Taken);
	assert_eq!(cancel.answer().await, CANCEL);

	let stop = popups.open(&stop_question(one_working(), Some("XYZ")), false);
	assert_eq!(popups.answer(stop.code(), &json!(STOP)), AnswerTaken::Wrong);
	assert_eq!(popups.answer(stop.code(), &json!({"choice": STOP, "typed": "xyz"})), AnswerTaken::Taken);
	assert_eq!(stop.answer().await, STOP);
	let kept = popups.open(&stop_question(one_working(), Some("XYZ")), false);
	for typed in ["AAA", "BBB", "CCC"] {
		popups.answer(kept.code(), &json!({"choice": STOP, "typed": typed}));
	}
	assert_eq!(kept.answer().await, CANCEL);
}

#[tokio::test]
async fn new_thread_takes_a_listed_project_and_a_brief_without_a_code() {
	let popups = Popups::new();
	let projects = vec!["acme-api".to_owned(), "client-a".to_owned()];
	let thread = popups.open(&new_thread_question(&projects, Some("client-a")), false);
	assert_eq!(thread.view().form, Some(PopupForm { projects, project: Some("client-a".to_owned()) }));
	let start = |project: &str, brief: &str| json!({"choice": START, "project": project, "brief": brief});
	assert_eq!(popups.answer(thread.code(), &start("other", "Fix it.")), AnswerTaken::Invalid);
	assert_eq!(popups.answer(thread.code(), &start("acme-api", "  ")), AnswerTaken::Invalid);
	assert_eq!(popups.answer(thread.code(), &start("acme-api", " Fix the retry. ")), AnswerTaken::Taken);
	assert_eq!(thread.answer().await, START);
	assert_eq!(
		thread.fields(),
		PopupFields { project: Some("acme-api".to_owned()), brief: Some("Fix the retry.".to_owned()) }
	);
}

#[tokio::test]
async fn a_question_closed_unanswered_is_its_fallback_and_its_code_dies() {
	let popups = Popups::new();
	let cancel = popups.open(&stop_question(one_working(), None), false);
	cancel.close(None);
	assert_eq!(cancel.answer().await, CANCEL);
	assert_eq!(cancel.how().await, PopupEnd::Withdrawn);
	assert!(!cancel.open());
	assert_eq!(popups.answer(cancel.code(), &json!(STOP)), AnswerTaken::Unknown);
	assert_eq!(popups.view(cancel.code()), None);
}

#[test]
fn a_questions_texts_lose_their_control_characters_before_anything_reads_them() {
	let popups = Popups::new();
	let view = PopupView { detail: Some("run \x1b]52;c;ZXZpbA==\x07this\x1b[2J".to_owned()), ..allow_box() };
	let popup = popups.open(&view, false);
	assert_eq!(popups.view(popup.code()).unwrap().detail.as_deref(), Some("run ]52;c;ZXZpbA==this[2J"));
	assert!(popup.code().len() == 64 && popup.code().bytes().all(|b| b.is_ascii_hexdigit()));
}
