//! The server-level cases of `test/frontier/server/popups.test.ts`: a question through the running server's routes.
//! The rest of that file is in `popups_tests.rs` and `popup_routes_tests.rs`.

use std::sync::{Arc, Mutex, PoisonError};

use reqwest::Response;
use serde_json::{Value, json};

use super::*;
use crate::frontier::protocol::{POPUP_PATH, POPUP_READER_HEADER, PopupView, ServerMessage, token_cookie};
use crate::frontier::server::helpers::{Page, http, start_test_server, until, with_server};
use crate::frontier::server::popups::{ALLOW, CANCEL, Counts, DENY, PopupEnd, STOP, allow_question, stop_question};
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

/// What a request to a question route carries.
#[derive(Default)]
struct Fetch<'a> {
	body: Option<Value>,
	token: Option<&'a str>,
	reader: Option<&'a str>,
	cookie: bool,
}

/// Fetches a question route as what shows it would: the service's token as a bearer, and the reader secret if given.
async fn popup_fetch(server: &JocastaServer, path: &str, with: Fetch<'_>) -> Response {
	let token = with.token.unwrap_or(&server.token);
	let url = format!("http://127.0.0.1:{}{POPUP_PATH}{path}", server.port);
	let client = http();
	let mut request = if with.body.is_some() { client.post(url) } else { client.get(url) };
	request = if with.cookie {
		request.header("cookie", format!("{}={token}", token_cookie(server.port)))
	} else {
		request.bearer_auth(token)
	};
	if let Some(reader) = with.reader {
		request = request.header(POPUP_READER_HEADER, reader);
	}
	if let Some(body) = with.body {
		request = request.header("content-type", "application/json").body(body.to_string());
	}
	request.send().await.expect("an answer")
}

fn choose<'a>(choice: Value, reader: Option<&'a str>, typed: Option<&str>) -> Fetch<'a> {
	let mut body = json!({ "choice": choice });
	if let Some(typed) = typed {
		body["typed"] = json!(typed);
	}
	Fetch { body: Some(body), reader, ..Fetch::default() }
}

fn reader_of(response: &Response) -> String {
	response.headers()[POPUP_READER_HEADER].to_str().unwrap().to_owned()
}

fn is_hex64(text: &str) -> bool {
	text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn code_of(response: Response) -> u16 {
	response.status().as_u16()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_questions_texts_and_answer_need_the_services_token_its_one_time_code_and_its_reader_secret_the_answer_is_taken_once()
 {
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

		// Without the token (the bearer only: a cookie is no token here), with a wrong code or none, nothing is served.
		let wrong = Fetch { token: Some("wrong"), ..Fetch::default() };
		assert_eq!(code_of(popup_fetch(&server, &code, wrong).await).await, 403);
		assert_eq!(code_of(popup_fetch(&server, &code, Fetch { cookie: true, ..Fetch::default() }).await).await, 403);
		assert_eq!(code_of(popup_fetch(&server, &"0".repeat(64), Fetch::default()).await).await, 403);
		assert_eq!(code_of(popup_fetch(&server, "", Fetch::default()).await).await, 403);
		// One character off: the first one changed, whatever it was.
		let near_miss = format!("{}{}", if code.starts_with('0') { "1" } else { "0" }, &code[1..]);
		assert_eq!(code_of(popup_fetch(&server, &near_miss, choose(json!("Allow"), None, None)).await).await, 403);
		let wrong_token = Fetch { token: Some("wrong"), ..choose(json!("Allow"), None, None) };
		assert_eq!(code_of(popup_fetch(&server, &code, wrong_token).await).await, 403);
		// Not read yet: no reader secret, no answer.
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!("Deny"), None, None)).await).await, 403);
		assert!(popup.open());

		// With both: its texts and the reader secret.
		let texts = popup_fetch(&server, &code, Fetch::default()).await;
		assert_eq!(texts.status().as_u16(), 200);
		let reader = reader_of(&texts);
		assert!(is_hex64(&reader));
		assert_eq!(texts.json::<Value>().await.unwrap(), serde_json::to_value(allow_box()).unwrap());

		// The answer carries the reader secret; another one is refused.
		let other = "f".repeat(64);
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!("Deny"), Some(&other), None)).await).await, 403);
		// Only its own choices answer it; a choice it doesn't offer leaves it waiting.
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!(STOP), Some(&reader), None)).await).await, 400);
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!(true), Some(&reader), None)).await).await, 400);
		assert!(popup.open());
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!("Allow"), Some(&reader), None)).await).await, 204);
		assert_eq!(popup.answer().await, "Allow");
		assert_eq!(popup.how().await, PopupEnd::Answered);

		// The code dies with its answer: no second answer, no texts.
		assert_eq!(code_of(popup_fetch(&server, &code, choose(json!("Deny"), Some(&reader), None)).await).await, 403);
		assert_eq!(code_of(popup_fetch(&server, &code, Fetch::default()).await).await, 403);
		assert_eq!(popup.answer().await, "Allow");

		// The state feed never hears of it.
		let heard: Vec<ServerMessage> = page.received();
		assert!(!heard.iter().any(|message| serde_json::to_string(message).unwrap().contains(&code)));
		page.close();
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_read_of_an_open_question_ends_it_as_deny_and_warns_him() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let view = allow_question("Merge acme-api/retry into main?", "d", Some("KQX"));
		let popup = server.question(&view, None, false);
		let code = popup.code().to_owned();
		let first = popup_fetch(&server, &code, Fetch::default()).await;
		assert_eq!(first.status().as_u16(), 200);
		// Another program, with the code from the popup's environment, reads it too: it gets nothing, and the question ends.
		assert_eq!(code_of(popup_fetch(&server, &code, Fetch::default()).await).await, 410);
		assert_eq!(popup.answer().await, DENY);
		assert_eq!(popup.how().await, PopupEnd::SecondReader);
		let reader = reader_of(&first);
		let late = choose(json!(ALLOW), Some(&reader), Some("KQX"));
		assert_eq!(code_of(popup_fetch(&server, &code, late).await).await, 403);
		until(|| !server.herdr.of("notify").is_empty()).await;
		assert!(server.herdr.of("notify").contains(&vec![json!({
			"title": "Jocasta",
			"body": "Another program read your question \"Merge acme-api/retry into main?\"; it counted as Deny.",
			"sound": "none",
		})]));
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
	let code = cancel.code().to_owned();
	assert_eq!(code_of(popup_fetch(&server, &code, choose(json!(STOP), None, None)).await).await, 403);

	let deny = server.question(&allow_box(), None, false);
	let (abort, signal) = tokio::sync::watch::channel(None);
	let gone = server.question(&allow_box(), Some(signal), false);
	abort.send_replace(Some("aborted".to_owned()));
	assert_eq!(gone.answer().await, "Deny");
	server.stop().await;
	assert_eq!(deny.answer().await, "Deny");
}
