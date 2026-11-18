use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

fn question() -> Confirmation {
	Confirmation { message: "Merge it?".into(), detail: "Merges the branch.".into() }
}

fn answering(allow: bool, how: PopupEnd) -> Confirm {
	Arc::new(move |_, _| Box::pin(async move { Ok(Answer { allow, how }) }))
}

async fn refusal(gate: &AllowGate, door: Door) -> String {
	Error::from(gate.ask(&question(), "nothing was merged", door).await.unwrap_err()).to_string()
}

#[tokio::test]
async fn allow_resolves_and_deny_ends_say_truly_how_the_question_ended() {
	let gate = AllowGate::new(Some(answering(true, PopupEnd::Answered)), None);
	gate.ask(&question(), "nothing was merged", Door::Boss).await.unwrap();
	let cases = [
		(PopupEnd::Answered, "He chose Deny".to_owned(), true),
		(PopupEnd::WrongCodes, "Three wrong codes, which count as Deny".into(), false),
		(PopupEnd::Closed, "Its terminal closed without an answer, which counts as Deny".into(), false),
		(PopupEnd::Withdrawn, "The question was withdrawn, which counts as Deny".into(), false),
	];
	for (how, why, chose) in cases {
		let gate = AllowGate::new(Some(answering(false, how)), None);
		let Err(AskError::Denied(denied)) = gate.ask(&question(), "nothing was merged", Door::Boss).await else {
			panic!("{how:?} was not a denial");
		};
		assert_eq!((denied.message, denied.chose), (format!("{why}; nothing was merged."), chose));
	}
	let gate = AllowGate::new(Some(answering(false, PopupEnd::Answered)), None);
	assert_eq!(refusal(&gate, Door::Cli).await, "You chose Deny; nothing was merged.");
}

#[tokio::test]
async fn without_a_way_to_ask_it_counts_as_deny_worded_for_the_door() {
	let gate = AllowGate::new(None, None);
	assert_eq!(
		refusal(&gate, Door::Boss).await,
		"Nothing can ask him in this run, which counts as Deny; nothing was merged."
	);
	assert_eq!(
		refusal(&gate, Door::Key).await,
		"Nothing can ask you in this run, which counts as Deny; nothing was merged."
	);
}

#[tokio::test]
async fn a_question_that_fails_counts_as_deny_with_its_message() {
	let confirm: Confirm = Arc::new(|_, _| Box::pin(async { Err(Error::other("no window")) }));
	let gate = AllowGate::new(Some(confirm), None);
	assert_eq!(
		refusal(&gate, Door::Boss).await,
		"Jocasta couldn't ask him (no window), which counts as Deny; nothing was merged."
	);
}

#[tokio::test]
async fn no_answer_in_time_counts_as_deny_and_withdraws_the_dialog() {
	let withdrawn = Arc::new(AtomicBool::new(false));
	let seen = withdrawn.clone();
	let confirm: Confirm = Arc::new(move |_, signal| {
		let seen = seen.clone();
		Box::pin(async move {
			aborted(&mut Some(signal)).await;
			seen.store(true, Ordering::SeqCst);
			Ok(Answer { allow: false, how: PopupEnd::Withdrawn })
		})
	});
	let gate = AllowGate::new(Some(confirm), Some(50));
	assert_eq!(refusal(&gate, Door::Boss).await, "No answer within 0.05 s, which counts as Deny; nothing was merged.");
	tokio::time::sleep(Duration::from_millis(50)).await;
	assert!(withdrawn.load(Ordering::SeqCst));
	let wait = AllowGate::new(None, Some(180_000));
	assert_eq!(wait.wait_ms / 60_000, 3);
}

#[tokio::test]
async fn a_stopping_service_withdraws_every_open_question() {
	let confirm: Confirm = Arc::new(|_, signal| {
		Box::pin(async move {
			aborted(&mut Some(signal)).await;
			Ok(Answer { allow: true, how: PopupEnd::Answered })
		})
	});
	let gate = Arc::new(AllowGate::new(Some(confirm), None));
	let asking = gate.clone();
	let task = tokio::spawn(async move {
		Error::from(asking.ask(&question(), "nothing was merged", Door::Boss).await.unwrap_err()).to_string()
	});
	tokio::time::sleep(Duration::from_millis(30)).await;
	gate.withdraw_all();
	assert_eq!(task.await.unwrap(), "Jocasta is closing, which counts as Deny; nothing was merged.");
	assert!(locked(&gate.asking).is_empty());
}
