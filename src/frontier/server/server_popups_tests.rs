//! The server-level cases of `test/frontier/server/popups.test.ts`: a question through the running server, answered as
//! herdr's client shell sends an answer. The rest of that file is in `popups_tests.rs` and `native_tests.rs`.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::json;

use super::*;
use crate::frontier::protocol::{PopupView, ServerMessage};
use crate::frontier::server::helpers::{Page, http, start_test_server, with_server};
use crate::frontier::server::popups::{CANCEL, Counts, PopupEnd, STOP, stop_question};
use crate::shared::testing::temp_jocasta;

fn allow_box() -> PopupView {
	PopupView {
		title: "Jocasta: Allow or Deny".to_owned(),
		message: "Remove the leftover worktree acme-api/retry?".to_owned(),
		detail: Some("Its branch stays.".to_owned()),
		choices: vec!["Allow".to_owned(), "Deny".to_owned()],
		fallback: "Deny".to_owned(),
		code: None,
		form: None,
	}
}

fn is_hex64(text: &str) -> bool {
	text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_questions_code_goes_only_to_what_shows_it_and_its_answer_is_taken_once_never_over_http() {
	let _j = temp_jocasta();
	let shown: Arc<Mutex<Vec<String>>> = Arc::default();
	let told = Arc::clone(&shown);
	let options = ServerOptions {
		on_question: Some(Arc::new(move |popup| {
			told.lock().unwrap_or_else(PoisonError::into_inner).push(popup.code().to_owned());
		})),
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let page = Page::open(&server, "").await;
		let popup = server.question(&allow_box(), None, false);
		let code = popup.code().to_owned();
		assert!(is_hex64(&code));
		// What shows it gets the code; nothing else does.
		assert_eq!(shown.lock().unwrap().as_slice(), std::slice::from_ref(&code));
		assert_ne!(server.question(&allow_box(), None, false).code(), code);
		// No route of the service's serves a question any more, token or not (fork plan phase 5).
		let url = format!("http://127.0.0.1:{}/api/popup/{code}", server.port);
		let read = http().get(&url).bearer_auth(&server.token).send().await.unwrap();
		assert_ne!(read.status().as_u16(), 200);
		assert!(popup.open());

		// One character off: unknown.
		let near_miss = format!("{}{}", if code.starts_with('0') { "1" } else { "0" }, &code[1..]);
		assert_eq!(server.answer(&near_miss, &json!({ "choice": "Allow" })).0, AnswerTaken::Unknown);
		// Only its own choices answer it; a choice it doesn't offer leaves it waiting.
		assert_eq!(server.answer(&code, &json!({ "choice": STOP })).0, AnswerTaken::Invalid);
		assert_eq!(server.answer(&code, &json!(true)).0, AnswerTaken::Invalid);
		assert!(popup.open());
		assert_eq!(server.answer(&code, &json!({ "choice": "Allow" })).0, AnswerTaken::Taken);
		assert_eq!(popup.answer().await, "Allow");
		assert_eq!(popup.how().await, PopupEnd::Answered);
		// The code dies with its answer.
		assert_eq!(server.answer(&code, &json!({ "choice": "Deny" })).0, AnswerTaken::Unknown);
		assert_eq!(popup.answer().await, "Allow");

		// The state feed never hears of it.
		let heard: Vec<ServerMessage> = page.received();
		assert!(!heard.iter().any(|message| serde_json::to_string(message).unwrap().contains(&code)));
		page.close();
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_closed_unanswered_is_its_fallback_and_so_is_every_open_one_when_the_server_stops() {
	let _j = temp_jocasta();
	let server = start_test_server(ServerOptions::default()).await;
	let cancel = server.question(&stop_question(Counts { working: 1, awaiting: 0 }, None), None, false);
	cancel.close(None);
	assert_eq!(cancel.answer().await, CANCEL);
	assert_eq!(cancel.how().await, PopupEnd::Withdrawn);
	assert!(!cancel.open());
	assert_eq!(server.answer(cancel.code(), &json!({ "choice": STOP })).0, AnswerTaken::Unknown);

	let deny = server.question(&allow_box(), None, false);
	let (abort, signal) = tokio::sync::watch::channel(None);
	let gone = server.question(&allow_box(), Some(signal), false);
	abort.send_replace(Some("aborted".to_owned()));
	assert_eq!(gone.answer().await, "Deny");
	server.stop().await;
	assert_eq!(deny.answer().await, "Deny");
}
