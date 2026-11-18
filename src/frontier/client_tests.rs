use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use tokio::sync::Notify;

use super::*;
use crate::shared::fsx::free_port;
use crate::shared::service_file::{ServiceFile, write_service_file};
use crate::shared::testing::temp_jocasta;

const TOKEN: &str = "t0ken";

/// Names the stand-in on `port` in `jocasta-service.json` with this process's (live) pid.
fn name_service(port: u16) {
	write_service_file(&ServiceFile { port, token: TOKEN.to_owned(), pid: std::process::id() }, None).unwrap();
}

/// The path and reader header of each request the stand-in saw.
type Seen = Vec<(String, Option<String>)>;

type Fallback = Arc<dyn Fn(&Method, &Uri, &HeaderMap) -> Response + Send + Sync>;

/// A stand-in service on a free port, named in `jocasta-service.json` with this process's (live) pid: `/ws` upgrades,
/// every other request goes to `fallback`.
async fn serve(fallback: Fallback) {
	let app = Router::new()
		.route(
			"/ws",
			get(|upgrade: WebSocketUpgrade| async {
				upgrade.on_upgrade(|mut socket| async move { while let Some(Ok(_)) = socket.recv().await {} })
			}),
		)
		.fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
			let fallback = Arc::clone(&fallback);
			async move { fallback(&method, &uri, &headers) }
		});
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let port = listener.local_addr().unwrap().port();
	tokio::spawn(async move { axum::serve(listener, app).await });
	name_service(port);
}

fn deny_view() -> PopupView {
	PopupView {
		title: "t".to_owned(),
		message: "m".to_owned(),
		detail: None,
		choices: vec!["Deny".to_owned()],
		fallback: "Deny".to_owned(),
		code: None,
		form: None,
	}
}

#[tokio::test]
async fn a_service_file_whose_process_is_gone_means_no_service_the_one_not_running_text_0581() {
	let _j = temp_jocasta();
	write_service_file(&ServiceFile { port: 1, token: TOKEN.to_owned(), pid: 2_u32.pow(31) - 2 }, None).unwrap();
	assert_eq!(service_at(), None);
	assert_eq!(
		post_action(PluginAction::Finish, Some("w1:p1")).await.as_deref(),
		Some("Jocasta isn't running; start it with jocasta.")
	);
	assert_eq!(question_view("ABCDEF").await.unwrap_err(), NOT_RUNNING);
}

#[tokio::test]
async fn a_service_file_naming_a_live_process_with_nothing_listening_on_its_port_is_the_same_text_0581_ux_13() {
	let _j = temp_jocasta();
	name_service(free_port().unwrap());
	assert!(service_at().is_some());
	assert_eq!(post_action(PluginAction::Finish, Some("w1:p1")).await.as_deref(), Some(NOT_RUNNING));
	assert_eq!(question_view("ABCDEF").await.unwrap_err(), NOT_RUNNING);
	let question = Question { code: "ABCDEF".to_owned(), view: deny_view(), reader: "r".to_owned() };
	let answer = PopupAnswer { choice: "Deny".to_owned(), typed: None, project: None, brief: None };
	assert_eq!(answer_question(&question, &answer).await, AnswerReply::Failed { text: NOT_RUNNING.to_owned() });
}

fn allow(typed: Option<&str>) -> PopupAnswer {
	PopupAnswer { choice: "Allow".to_owned(), typed: typed.map(str::to_owned), project: None, brief: None }
}

#[tokio::test]
async fn a_questions_one_reader_keeps_the_reader_secret_and_sends_it_on_the_hold_and_the_answer_the_answers_replies_0578()
 {
	let _j = temp_jocasta();
	let seen: Arc<Mutex<Seen>> = Arc::default();
	let replies: Arc<Mutex<VecDeque<u16>>> = Arc::new(Mutex::new(VecDeque::from([409, 409, 410])));
	let (log, queue) = (Arc::clone(&seen), Arc::clone(&replies));
	serve(Arc::new(move |method, uri, headers| {
		let path = uri.path().to_owned();
		let reader = headers.get(POPUP_READER_HEADER).and_then(|v| v.to_str().ok()).map(str::to_owned);
		log.lock().unwrap().push((path.clone(), reader));
		if method == Method::GET && path.ends_with("/hold") {
			return String::new().into_response();
		}
		if method == Method::GET {
			let view = json!({
				"title": "Jocasta: Allow or Deny", "message": "m", "choices": ["Allow", "Deny"], "fallback": "Deny",
				"code": "KQX"
			});
			return ([(POPUP_READER_HEADER, "s3cret")], axum::Json(view)).into_response();
		}
		let mut queue = queue.lock().unwrap();
		let status = queue.pop_front().unwrap_or(204);
		if status == 409 {
			let left = if queue.len() == 1 { 1 } else { 2 };
			return (StatusCode::CONFLICT, axum::Json(json!({ "left": left }))).into_response();
		}
		StatusCode::from_u16(status).unwrap().into_response()
	}))
	.await;
	let question = question_view("ABCDEF").await.unwrap();
	assert_eq!(question.reader, "s3cret");
	assert_eq!(answer_question(&question, &allow(Some("AAA"))).await, AnswerReply::Wrong { left: Some(2) });
	assert_eq!(answer_question(&question, &allow(Some("AAA"))).await, AnswerReply::Wrong { left: Some(1) });
	assert_eq!(answer_question(&question, &allow(Some("AAA"))).await, AnswerReply::Exhausted);
	replies.lock().unwrap().clear();
	let deny = PopupAnswer { choice: "Deny".to_owned(), typed: None, project: None, brief: None };
	assert_eq!(answer_question(&question, &deny).await, AnswerReply::Taken);
	let ended = Arc::new(Notify::new());
	let signal = Arc::clone(&ended);
	let _release = hold_question(&question, move || signal.notify_one());
	tokio::time::timeout(Duration::from_secs(10), ended.notified()).await.unwrap();
	let seen = seen.lock().unwrap();
	assert_eq!(seen[0], ("/api/popup/ABCDEF".to_owned(), None));
	assert!(seen[1..].iter().all(|(_, reader)| reader.as_deref() == Some("s3cret")));
	assert_eq!(seen.last().unwrap().0, "/api/popup/ABCDEF/hold");
}

#[tokio::test]
async fn the_state_feed_send_says_whether_it_went_once_closed_no_more_calls_fe_3_fe_5() {
	let _j = temp_jocasta();
	serve(Arc::new(|_, _, _| StatusCode::NOT_FOUND.into_response())).await;
	let ups: Arc<Mutex<Vec<bool>>> = Arc::default();
	let open = Arc::new(Notify::new());
	let (log, signal) = (Arc::clone(&ups), Arc::clone(&open));
	let feed = state_feed(
		|_| {},
		move |up| {
			log.lock().unwrap().push(up);
			if up {
				signal.notify_one();
			}
		},
	);
	tokio::time::timeout(Duration::from_secs(10), open.notified()).await.unwrap();
	let focus = ClientMessage::PaneFocus { pane_id: "w1:p1".to_owned() };
	assert!(feed.send(&focus));
	feed.close();
	assert!(!feed.send(&focus));
	// The socket's own close runs in the client after this; no signal of it reaches the test, so give it a moment.
	tokio::time::sleep(Duration::from_millis(100)).await;
	assert_eq!(*ups.lock().unwrap(), [true]);
}
