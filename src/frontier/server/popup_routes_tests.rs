//! Tests of `popup_routes.rs`: the route cases of `popups.test.ts`. The TypeScript runs most of them through
//! `withServer` (a whole service, a `Page` on the feed, herdr's fake); here each runs on the route alone, with stand-ins
//! for the service. What needs `server.rs` waits for wave 11 (see the report).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::to_bytes;
use futures_util::FutureExt;
use tokio::sync::watch;

use super::*;
use crate::frontier::protocol::PopupView;
use crate::frontier::server::keys::{FinishHow, KeyFinishable, KeyOutcome, NewThreadForm};
use crate::frontier::server::popups::Counts;
use crate::frontier::server::popups::{ALLOW, DENY, PopupEnd, STOP, allow_question, stop_question};
use crate::shared::errors::Error;
use crate::shared::errors::Result;
use crate::shared::testing::temp_jocasta;

#[derive(Default)]
struct Notes(Mutex<Vec<(Toast, bool)>>);

impl Notifier for Notes {
	fn notify(&self, toast: Toast, show: bool) -> LocalBoxFuture<'_, ()> {
		locked(&self.0).push((toast, show));
		async {}.boxed()
	}
}

/// The keys' service these tests don't reach: each key refuses.
impl KeyService for Notes {
	fn key_stop<'a>(&'a self, _pane_id: Option<&'a str>) -> LocalBoxFuture<'a, Result<String>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_review<'a>(&'a self, _pane_id: Option<&'a str>) -> LocalBoxFuture<'a, Result<String>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_finishable<'a>(&'a self, _pane_id: Option<&'a str>) -> LocalBoxFuture<'a, Result<KeyFinishable>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_finish<'a>(&'a self, _pane_id: &'a str, _how: FinishHow) -> LocalBoxFuture<'a, Result<KeyOutcome>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn new_thread_form<'a>(&'a self, _pane_id: Option<&'a str>) -> LocalBoxFuture<'a, Result<NewThreadForm>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_start<'a>(&'a self, _project: &'a str, _brief: &'a str) -> LocalBoxFuture<'a, Result<KeyOutcome>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}
}

#[derive(Default)]
struct Holder {
	held: AtomicUsize,
	fronted: AtomicUsize,
}

impl PopupHolder for Holder {
	fn hold(&self, code: &str) -> Response {
		self.held.fetch_add(1, Ordering::SeqCst);
		text_response(StatusCode::OK, &format!("holding {code}"))
	}

	fn front(&self, _popup: &Popup) {
		self.fronted.fetch_add(1, Ordering::SeqCst);
	}
}

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

fn allow_box_json() -> Value {
	json!({
		"title": "Jocasta: Allow or Deny",
		"message": "Remove the leftover worktree acme-api/retry?",
		"detail": "Its branch stays.",
		"choices": ["Allow", "Deny"],
		"fallback": "Deny"
	})
}

struct Fixture {
	popups: Arc<Popups>,
	notes: Arc<Notes>,
	holder: Arc<Holder>,
	routes: PopupRoutes,
}

fn fixture() -> Fixture {
	let (popups, notes, holder) = (Arc::new(Popups::new()), Arc::new(Notes::default()), Arc::new(Holder::default()));
	let routes = popup_routes(PopupParts {
		token: "t".to_owned(),
		popups: popups.clone(),
		ask_popups: Some(holder.clone()),
		service: notes.clone(),
	});
	Fixture { popups, notes, holder, routes }
}

/// A request as what shows a question would send it: the token as a bearer, the reader secret if given.
fn request(method: &str, path: &str, token: Option<&str>, reader: Option<&str>, body: Option<Value>) -> Request {
	let mut builder = axum::http::Request::builder().method(method).uri(path);
	if let Some(token) = token {
		builder = builder.header("authorization", format!("Bearer {token}"));
	}
	if let Some(reader) = reader {
		builder = builder.header(POPUP_READER_HEADER, reader);
	}
	builder.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap()
}

fn url_of(path: &str) -> Url {
	Url::parse(&format!("http://127.0.0.1{path}")).unwrap()
}

async fn question(route: &QuestionRoute, request: Request) -> Response {
	let url = url_of(request.uri().path());
	PageRoute::handle(route, request, url).await
}

async fn text_of(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

fn popup_path(code: &str) -> String {
	format!("{POPUP_PATH}{code}")
}

fn choose(choice: Value, typed: Option<&str>) -> Value {
	typed.map_or_else(|| json!({ "choice": choice }), |typed| json!({ "choice": choice, "typed": typed }))
}

async fn until(mut done: impl FnMut() -> bool) {
	for _ in 0..200 {
		if done() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	panic!("never happened");
}

fn reader_of_response(response: &Response) -> String {
	response.headers()[POPUP_READER_HEADER].to_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_questions_texts_and_answer_need_the_services_token_its_one_time_code_and_its_reader_secret_the_answer_is_taken_once()
 {
	let _j = temp_jocasta();
	let f = fixture();
	let popup = f.popups.open(&allow_box(), false);
	let code = popup.code().to_owned();
	assert_eq!(code.len(), 64);
	assert!(code.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
	assert_ne!(f.popups.open(&allow_box(), false).code(), code);
	let q = &f.routes.question;
	let status = |response: Response| response.status().as_u16();

	// Without the token (the bearer only: a cookie is no token here), with a wrong code or none, nothing is served.
	assert_eq!(status(question(q, request("GET", &popup_path(&code), Some("wrong"), None, None)).await), 403);
	let by_cookie = axum::http::Request::builder()
		.uri(popup_path(&code))
		.header("cookie", "jocasta_service_9=t")
		.body(Body::empty())
		.unwrap();
	assert_eq!(status(question(q, by_cookie).await), 403);
	assert_eq!(status(question(q, request("GET", &popup_path(&"0".repeat(64)), Some("t"), None, None)).await), 403);
	assert_eq!(status(question(q, request("GET", &popup_path(""), Some("t"), None, None)).await), 403);
	// One character off: the first one changed, whatever it was.
	let near_miss = format!("{}{}", if code.starts_with('0') { "1" } else { "0" }, &code[1..]);
	let post = |path: String, token: &str, reader: Option<&str>, body: Value| {
		request("POST", &path, Some(token), reader, Some(body))
	};
	assert_eq!(status(question(q, post(popup_path(&near_miss), "t", None, choose(json!("Allow"), None))).await), 403);
	assert_eq!(status(question(q, post(popup_path(&code), "wrong", None, choose(json!("Allow"), None))).await), 403);
	// Not read yet: no reader secret, no answer.
	assert_eq!(status(question(q, post(popup_path(&code), "t", None, choose(json!("Deny"), None))).await), 403);
	assert!(popup.open());

	// With both: its texts and the reader secret.
	let texts = question(q, request("GET", &popup_path(&code), Some("t"), None, None)).await;
	assert_eq!(texts.status(), 200);
	assert_eq!(texts.headers()[header::CACHE_CONTROL], "no-store");
	assert_eq!(texts.headers()[header::CONTENT_TYPE], "application/json;charset=utf-8");
	let reader = reader_of_response(&texts);
	assert_eq!(reader.len(), 64);
	assert!(reader.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
	assert_eq!(serde_json::from_str::<Value>(&text_of(texts).await).unwrap(), allow_box_json());

	// The answer carries the reader secret; another one is refused.
	let other = "f".repeat(64);
	assert_eq!(status(question(q, post(popup_path(&code), "t", Some(&other), choose(json!("Deny"), None))).await), 403);
	// Only its own choices answer it; a choice it doesn't offer leaves it waiting.
	let bad = |choice: Value| post(popup_path(&code), "t", Some(&reader), choose(choice, None));
	let refused = question(q, bad(json!(STOP))).await;
	assert_eq!((refused.status().as_u16(), text_of(refused).await.as_str()), (400, "not one of its choices"));
	assert_eq!(status(question(q, bad(json!(true))).await), 400);
	assert!(popup.open());
	let taken = question(q, bad(json!("Allow"))).await;
	assert_eq!(taken.status(), 204);
	assert_eq!(text_of(taken).await, "");
	assert_eq!(popup.answer().await, "Allow");
	assert_eq!(popup.how().await, PopupEnd::Answered);

	// The code dies with its answer: no second answer, no texts.
	assert_eq!(
		status(question(q, post(popup_path(&code), "t", Some(&reader), choose(json!("Deny"), None))).await),
		403
	);
	assert_eq!(status(question(q, request("GET", &popup_path(&code), Some("t"), None, None)).await), 403);
	assert_eq!(popup.answer().await, "Allow");
}

#[tokio::test]
async fn a_second_read_of_an_open_question_ends_it_as_deny_and_warns_him() {
	let _j = temp_jocasta();
	let f = fixture();
	let view = allow_question("Merge acme-api/retry into main?", "d", Some("KQX"));
	let popup = f.popups.open(&view, false);
	let q = &f.routes.question;
	let first = question(q, request("GET", &popup_path(popup.code()), Some("t"), None, None)).await;
	assert_eq!(first.status(), 200);
	// Another program, with the code from the popup's environment, reads it too: it gets nothing, and the question ends.
	let second = question(q, request("GET", &popup_path(popup.code()), Some("t"), None, None)).await;
	assert_eq!((second.status().as_u16(), text_of(second).await.as_str()), (410, "read already"));
	assert_eq!(popup.answer().await, DENY);
	assert_eq!(popup.how().await, PopupEnd::SecondReader);
	let reader = reader_of_response(&first);
	let late =
		request("POST", &popup_path(popup.code()), Some("t"), Some(&reader), Some(choose(json!(ALLOW), Some("KQX"))));
	assert_eq!(question(q, late).await.status(), 403);
	until(|| !locked(&f.notes.0).is_empty()).await;
	let notes = locked(&f.notes.0);
	assert_eq!(notes.len(), 1);
	let (toast, show) = &notes[0];
	assert!(*show);
	assert_eq!(
		(toast.reason, toast.level, toast.text.as_str()),
		(
			ToastReason::Failed,
			ToastLevel::Warning,
			"Jocasta — Another program read your question \"Merge acme-api/retry into main?\"; it counted as Deny."
		)
	);
}

#[tokio::test]
async fn a_hold_without_the_reader_secret_ends_an_open_question_as_deny_and_warns_him() {
	let _j = temp_jocasta();
	let f = fixture();
	let hold = &f.routes.hold;
	let hold_of = |code: &str, headers: &[(&str, &str)]| {
		let path = format!("{POPUP_PATH}{code}{POPUP_HOLD}");
		let mut builder = axum::http::Request::builder().uri(&path);
		for (name, value) in headers {
			builder = builder.header(*name, *value);
		}
		(builder.body(Body::empty()).unwrap(), url_of(&path))
	};
	let popup = f.popups.open(&allow_box_with("Merge acme-api/retry into main?"), false);
	// The popup's own first read.
	f.popups.read(popup.code());
	// Without the service's token: refused, and the question stays open.
	let (request, url) = hold_of(popup.code(), &[]);
	assert_eq!(hold.0.hold_response(request, url).await.status(), 403);
	assert!(popup.open());
	// The token without the first reader's secret: another program holds the code, and the question ends.
	let wrong = "f".repeat(64);
	let (request, url) = hold_of(popup.code(), &[("authorization", "Bearer t"), (POPUP_READER_HEADER, &wrong)]);
	let ended = hold.0.hold_response(request, url).await;
	assert_eq!(ended.status(), 410);
	assert_eq!(text_of(ended).await, "read already");
	assert_eq!(popup.answer().await, DENY);
	assert_eq!(popup.how().await, PopupEnd::SecondReader);
	until(|| !locked(&f.notes.0).is_empty()).await;
	{
		let notes = locked(&f.notes.0);
		assert_eq!(notes.len(), 1);
		assert_eq!(
			notes[0].0,
			Toast::new(
				ToastReason::Failed,
				ToastLevel::Warning,
				"Jocasta — Another program read your question \"Merge acme-api/retry into main?\"; it counted as Deny."
			)
		);
	}
	// Once ended, its code holds nothing.
	let (request, url) = hold_of(popup.code(), &[("authorization", "Bearer t")]);
	assert_eq!(hold.0.hold_response(request, url).await.status(), 403);
	assert_eq!(f.holder.held.load(Ordering::SeqCst), 0);
}

fn allow_box_with(message: &str) -> PopupView {
	allow_question(message, "d", Some("KQX"))
}

#[tokio::test]
async fn a_hold_with_the_first_readers_secret_is_the_ask_popups_held_request() {
	let _j = temp_jocasta();
	let f = fixture();
	let popup = f.popups.open(&allow_box(), false);
	let Some(PopupRead::First { reader, .. }) = f.popups.read(popup.code()) else { panic!("a first read") };
	let path = format!("{POPUP_PATH}{}{POPUP_HOLD}", popup.code());
	assert!(Route::matches(&f.routes.hold, &path));
	let held =
		f.routes.hold.0.hold_response(request("GET", &path, Some("t"), Some(&reader), None), url_of(&path)).await;
	assert_eq!((held.status().as_u16(), text_of(held).await), (200, format!("holding {}", popup.code())));
	assert_eq!(f.holder.held.load(Ordering::SeqCst), 1);
	assert!(popup.open());
	// A service without `AskPopups` refuses every hold.
	let bare = popup_routes(PopupParts {
		token: "t".to_owned(),
		popups: f.popups.clone(),
		ask_popups: None,
		service: f.notes.clone(),
	});
	let refused = bare.hold.0.hold_response(request("GET", &path, Some("t"), Some(&reader), None), url_of(&path)).await;
	assert_eq!(refused.status(), 403);
	// A path that is only the suffix holds nothing.
	let odd = format!("{POPUP_PATH}hold");
	assert!(Route::matches(&f.routes.hold, &odd));
	let refused = f.routes.hold.0.hold_response(request("GET", &odd, Some("t"), None, None), url_of(&odd)).await;
	assert_eq!(refused.status(), 403);
}

#[tokio::test]
async fn wrong_codes_answer_409_with_the_tries_left_the_third_is_410_and_takes_the_fallback() {
	let _j = temp_jocasta();
	let f = fixture();
	let popup = f.popups.open(&allow_question("m", "d", Some("KQX")), false);
	let q = &f.routes.question;
	let read = question(q, request("GET", &popup_path(popup.code()), Some("t"), None, None)).await;
	let reader = reader_of_response(&read);
	let wrong = |typed: &'static str| {
		request("POST", &popup_path(popup.code()), Some("t"), Some(&reader), Some(choose(json!(ALLOW), Some(typed))))
	};
	let first = question(q, wrong("AAA")).await;
	assert_eq!(first.status(), 409);
	assert_eq!(first.headers()[header::CONTENT_TYPE], "application/json;charset=utf-8");
	assert_eq!(serde_json::from_str::<Value>(&text_of(first).await).unwrap(), json!({ "left": 2 }));
	let second = question(q, wrong("BBB")).await;
	assert_eq!(serde_json::from_str::<Value>(&text_of(second).await).unwrap(), json!({ "left": 1 }));
	let third = question(q, wrong("CCC")).await;
	assert_eq!((third.status().as_u16(), text_of(third).await.as_str()), (410, "wrong code three times"));
	assert_eq!(popup.answer().await, DENY);
	assert_eq!(popup.how().await, PopupEnd::WrongCodes);
}

#[tokio::test]
async fn a_questions_texts_lose_their_control_characters_before_anything_reads_them_sec_net_3() {
	let _j = temp_jocasta();
	let f = fixture();
	let mut view = allow_box();
	view.detail = Some("run \x1b]52;c;ZXZpbA==\x07this\x1b[2J".to_owned());
	let popup = f.popups.open(&view, false);
	let texts = question(&f.routes.question, request("GET", &popup_path(popup.code()), Some("t"), None, None)).await;
	let body: Value = serde_json::from_str(&text_of(texts).await).unwrap();
	assert_eq!(body["detail"], "run ]52;c;ZXZpbA==this[2J");
}

#[tokio::test]
async fn the_question_route_refuses_a_code_with_a_slash_and_any_method_but_get_and_post() {
	let _j = temp_jocasta();
	let f = fixture();
	let q = &f.routes.question;
	assert!(PageRoute::matches(q, "/api/popup/abc"));
	assert!(!PageRoute::matches(q, "/api/popups"));
	let nested = question(q, request("GET", &popup_path("abc/def"), Some("t"), None, None)).await;
	assert_eq!((nested.status().as_u16(), text_of(nested).await.as_str()), (404, "Not found"));
	let put = question(q, request("PUT", &popup_path("abc"), Some("t"), None, None)).await;
	assert_eq!((put.status().as_u16(), text_of(put).await.as_str()), (405, "Method not allowed"));
	let unauthorized = question(q, request("PUT", &popup_path("abc"), None, None, None)).await;
	assert_eq!((unauthorized.status().as_u16(), text_of(unauthorized).await.as_str()), (403, "Forbidden"));
	// An answer without a body is no choice.
	let popup = f.popups.open(&stop_question(Counts { working: 1, awaiting: 0 }, Some("XYZ")), false);
	let read = question(q, request("GET", &popup_path(popup.code()), Some("t"), None, None)).await;
	let reader = reader_of_response(&read);
	let empty = question(q, request("POST", &popup_path(popup.code()), Some("t"), Some(&reader), None)).await;
	assert_eq!(empty.status(), 400);
	assert!(popup.open());
}

fn aborting() -> (watch::Sender<Option<String>>, AbortSignal) {
	watch::channel(None)
}

#[tokio::test]
async fn the_asker_opens_each_question_shows_it_and_ends_it_unanswered_on_its_signal() {
	let _j = temp_jocasta();
	let popups = Arc::new(Popups::new());
	let shown = Arc::new(Mutex::new(Vec::<Popup>::new()));
	let record = shown.clone();
	let show: Show = Arc::new(move |popup| {
		locked(&record).push(popup.clone());
		Ok(())
	});
	let ask = asker(popups.clone(), Some(show));
	// What shows it gets the popup; nothing else does.
	let popup = ask(&allow_box(), None, false);
	assert_eq!(*locked(&shown), std::slice::from_ref(&popup));
	assert!(popup.open());
	assert!(!popup.from_key());
	assert_eq!(popups.list(), std::slice::from_ref(&popup));
	assert!(ask(&allow_box(), None, true).from_key());

	// Aborted later: the fallback.
	let (abort, signal) = aborting();
	let gone = ask(&allow_box(), Some(signal), false);
	assert!(gone.open());
	abort.send_replace(Some("no longer wanted".to_owned()));
	assert_eq!(gone.answer().await, "Deny");
	assert_eq!(gone.how().await, PopupEnd::Withdrawn);

	// Aborted already: closed before it is shown.
	let before = locked(&shown).len();
	let (abort, signal) = aborting();
	abort.send_replace(Some("already".to_owned()));
	let never = ask(&allow_box(), Some(signal), false);
	assert!(!never.open());
	assert_eq!(never.answer().await, "Deny");
	assert_eq!(locked(&shown).len(), before);
}

#[tokio::test]
async fn a_question_that_cant_be_shown_is_logged_and_stays_open() {
	let _j = temp_jocasta();
	let popups = Arc::new(Popups::new());
	let show: Show = Arc::new(|_popup| Err(Error::other("herdr is gone")));
	let popup = asker(popups, Some(show))(&allow_box(), None, false);
	assert!(popup.open());
	let without = asker(Arc::new(Popups::new()), None)(&allow_box(), None, false);
	assert!(without.open());
}

struct Actors {
	notes: Arc<Notes>,
	broadcast: Arc<Mutex<Vec<ServerMessage>>>,
	route: ActionRoute,
}

fn actors() -> Actors {
	let (notes, holder) = (Arc::new(Notes::default()), Arc::new(Holder::default()));
	let sent = Arc::new(Mutex::new(Vec::new()));
	let record = sent.clone();
	let route = action_route(ActionParts {
		token: "t".to_owned(),
		service: notes.clone(),
		ask: asker(Arc::new(Popups::new()), None),
		ask_popups: Some(holder),
		broadcast: Arc::new(move |message| locked(&record).push(message)),
		quit: Arc::new(|| async { crate::frontier::server::close::CloseOutcome::Kept }.boxed()),
		waiting_ms: 1_000,
	});
	Actors { notes, broadcast: sent, route }
}

async fn act(route: &ActionRoute, request: Request) -> Response {
	let url = url_of(request.uri().path());
	let server = crate::frontier::server::routes::listen(0, test_served()).unwrap();
	let response = Route::handle(route, request, url, server.clone()).await;
	server.stop(true).await;
	response
}

fn test_served() -> crate::frontier::server::routes::Served {
	crate::frontier::server::routes::Served {
		routes: vec![],
		page: Box::new(|_request, _url, _port| Box::pin(async { text_response(StatusCode::NOT_FOUND, "x") })),
	}
}

#[tokio::test]
async fn the_action_route_takes_a_post_with_the_token_and_one_of_the_actions() {
	let _j = temp_jocasta();
	let a = actors();
	assert!(Route::matches(&a.route, "/api/plugin/action"));
	assert!(!Route::matches(&a.route, "/api/plugin/actions"));
	let refused = act(&a.route, request("POST", PLUGIN_ACTION_PATH, None, None, None)).await;
	assert_eq!((refused.status().as_u16(), text_of(refused).await.as_str()), (401, "invalid token"));
	let method = act(&a.route, request("GET", PLUGIN_ACTION_PATH, Some("t"), None, None)).await;
	assert_eq!((method.status().as_u16(), text_of(method).await.as_str()), (405, "POST only"));
	let list = "action is one of finish, stop, review, new-thread, quit";
	for body in [None, Some(json!({ "action": "format" })), Some(json!({ "action": 3 })), Some(json!([1]))] {
		let bad = act(&a.route, request("POST", PLUGIN_ACTION_PATH, Some("t"), None, body)).await;
		assert_eq!((bad.status().as_u16(), text_of(bad).await.as_str()), (400, list));
	}
	for action in ["finish", "stop", "review", "new-thread", "quit"] {
		let body = Some(json!({ "action": action, "paneId": "p_1" }));
		let accepted = act(&a.route, request("POST", PLUGIN_ACTION_PATH, Some("t"), None, body)).await;
		assert_eq!(accepted.status(), 202, "{action}");
		assert_eq!(text_of(accepted).await, "");
	}
	let blank_pane = Some(json!({ "action": "stop", "paneId": "" }));
	assert_eq!(act(&a.route, request("POST", PLUGIN_ACTION_PATH, Some("t"), None, blank_pane)).await.status(), 202);
}

#[tokio::test]
async fn an_outcome_is_broadcast_a_refusal_also_notifies_once_per_text_each_thirty_seconds() {
	let _j = temp_jocasta();
	let a = actors();
	let state = a.route.state.reporter.clone();
	state.report("Finished Fix login", false);
	assert!(locked(&a.notes.0).is_empty());
	state.report("Stop: no thread here", true);
	state.report("Stop: no thread here", true);
	state.report("Other refusal", true);
	until(|| locked(&a.notes.0).len() == 3).await;
	{
		let notes = locked(&a.notes.0);
		let shown: Vec<(&str, bool)> = notes.iter().map(|(toast, show)| (toast.text.as_str(), *show)).collect();
		assert_eq!(shown, [("Stop: no thread here", true), ("Stop: no thread here", false), ("Other refusal", true)]);
		assert!(
			notes.iter().all(|(toast, _)| toast.reason == ToastReason::Failed && toast.level == ToastLevel::Warning)
		);
	}
	assert_eq!(
		*locked(&a.broadcast),
		[
			ServerMessage::Action { text: "Finished Fix login".to_owned(), is_error: false },
			ServerMessage::Action { text: "Stop: no thread here".to_owned(), is_error: true },
			ServerMessage::Action { text: "Stop: no thread here".to_owned(), is_error: true },
			ServerMessage::Action { text: "Other refusal".to_owned(), is_error: true },
		]
	);
	// Thirty seconds later the same text is shown again.
	for (_, at) in locked(&state.refusals_shown).iter_mut() {
		*at -= REFUSAL_TOAST_MS;
	}
	state.report("Stop: no thread here", true);
	until(|| locked(&a.notes.0).len() == 4).await;
	assert!(locked(&a.notes.0)[3].1);
	// Control characters are cleaned from what he reads.
	state.report("bad \x1b[2J text", false);
	let last = locked(&a.broadcast).last().cloned();
	assert_eq!(last, Some(ServerMessage::Action { text: pane_text("bad \x1b[2J text"), is_error: false }));
}
