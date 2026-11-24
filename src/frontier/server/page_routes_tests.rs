//! Tests of `page_routes.rs`: `server.test.ts`'s launch code, cookie and earlier-run-page cases, and the responder's
//! answers (which the TypeScript tests through `withServer` and `Page`).

use std::sync::atomic::{AtomicI64, Ordering};

use axum::body::to_bytes;

use super::*;
use crate::frontier::brief::model::{Brief, Section};
use crate::frontier::brief::routes::BriefActions;
use crate::frontier::brief::store;
use crate::shared::errors::Result;
use crate::shared::testing::temp_jocasta;

fn request(headers: &[(&str, &str)]) -> Request {
	let mut builder = axum::http::Request::builder().uri("http://127.0.0.1/");
	for (name, value) in headers {
		builder = builder.header(*name, *value);
	}
	builder.body(Body::empty()).unwrap()
}

fn request_at(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
	let mut builder = axum::http::Request::builder().method(method).uri(path);
	for (name, value) in headers {
		builder = builder.header(*name, *value);
	}
	builder.body(Body::empty()).unwrap()
}

async fn text_of(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

#[test]
fn a_page_urls_launch_code_is_taken_once_within_its_minute_sec_loc_11() {
	let now = Arc::new(AtomicI64::new(1_000));
	let clock = now.clone();
	let codes = LaunchCodes::new(Some(Arc::new(move || clock.load(Ordering::SeqCst))));
	let first = codes.mint();
	assert_eq!(first.len(), 64);
	assert!(first.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
	assert!(codes.take(&first));
	assert!(!codes.take(&first));
	assert!(!codes.take("nope"));
	let late = codes.mint();
	now.fetch_add(60_000, Ordering::SeqCst);
	assert!(!codes.take(&late));
	// A mint forgets the expired ones.
	let kept = codes.mint();
	assert_ne!(kept, late);
	assert_eq!(locked(&codes.codes).len(), 1);
}

#[test]
fn the_default_launch_codes_run_on_the_real_clock_and_every_code_is_new() {
	let codes = LaunchCodes::default();
	let (a, b) = (codes.mint(), codes.mint());
	assert_ne!(a, b);
	assert!(codes.take(&b));
	assert!(codes.take(&a));
}

#[tokio::test]
async fn the_request_helpers_cookie_by_name_the_earlier_run_page_for_a_browser() {
	assert_eq!(
		cookie_value(&request(&[("cookie", "a=1; jocasta_service_9=x=y")]), "jocasta_service_9"),
		Some("x=y".to_owned())
	);
	assert_eq!(cookie_value(&request(&[("cookie", "a=1")]), "b"), None);
	assert_eq!(cookie_value(&request(&[]), "b"), None);
	let page = refused_page(&request(&[("accept", "text/html")]));
	assert_eq!(page.status(), 403);
	assert_eq!(page.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
	assert!(text_of(page).await.contains("This Jocasta page is from an earlier run."));
	let plain = refused_page(&request(&[]));
	assert_eq!(plain.status(), 403);
	assert_eq!(plain.headers()[header::CONTENT_TYPE], "text/plain;charset=utf-8");
	assert_eq!(text_of(plain).await, "Forbidden");
}

/// A brief with no buttons.
struct NoActions;

impl BriefActions for NoActions {
	fn act(&self, _id: String, _action: serde_json::Value) -> BoxFuture<Result<Brief>> {
		Box::pin(async { Err(crate::shared::errors::Error::user("no actions here")) })
	}
}

struct Health;

impl PageRoute for Health {
	fn matches(&self, path: &str) -> bool {
		path == "/health"
	}

	fn handle(&self, _request: Request, _url: Url) -> BoxFuture<Response> {
		Box::pin(async { text_response(StatusCode::OK, "bearer's route") })
	}
}

fn responder() -> (PageResponder, Arc<LaunchCodes>) {
	let codes = Arc::new(LaunchCodes::default());
	let responder = page_responder(PageParts {
		token: "api".to_owned(),
		page_token: "page".to_owned(),
		codes: codes.clone(),
		port: Arc::new(|| 9),
		briefs: BriefRoutes::new(Arc::new(|| 9), Arc::new(NoActions)),
		bearer_routes: vec![Box::new(Health)],
	});
	(responder, codes)
}

async fn ask(responder: &PageResponder, method: &str, path: &str, headers: &[(&str, &str)]) -> Response {
	let url = Url::parse(&format!("http://127.0.0.1:9{path}")).unwrap();
	let mut all = vec![("host", "127.0.0.1:9")];
	all.extend_from_slice(headers);
	responder(request_at(method, path, &all), url).await
}

#[tokio::test]
async fn a_launch_code_is_swapped_for_the_page_cookie_and_the_rest_of_the_query_stays() {
	let (responder, codes) = responder();
	let code = codes.mint();
	let swapped = ask(&responder, "GET", &format!("/brief/show-1?t={code}&review=a%20b&x=1"), &[]).await;
	assert_eq!(swapped.status(), 302);
	assert_eq!(swapped.headers()[header::LOCATION], "/brief/show-1?review=a+b&x=1");
	assert_eq!(swapped.headers()[header::SET_COOKIE], "jocasta_service_9=page; Path=/; HttpOnly; SameSite=Strict");
	assert_eq!(text_of(swapped).await, "");
	// A code is taken once; another or a used one is the earlier-run page for a browser, else a plain 403.
	let used = ask(&responder, "GET", &format!("/?t={code}"), &[("accept", "text/html")]).await;
	assert_eq!(used.status(), 403);
	assert!(text_of(used).await.contains("earlier run"));
	let nope = ask(&responder, "GET", "/?t=nope", &[]).await;
	assert_eq!((nope.status().as_u16(), text_of(nope).await.as_str()), (403, "Forbidden"));
	// Nothing else in the query: the page alone.
	let bare = ask(&responder, "GET", &format!("/brief/show-1?t={}", codes.mint()), &[]).await;
	assert_eq!(bare.headers()[header::LOCATION], "/brief/show-1");
}

#[tokio::test]
async fn the_bearers_routes_answer_before_the_cookie_is_asked_for() {
	let (responder, _) = responder();
	assert_eq!(text_of(ask(&responder, "GET", "/health", &[]).await).await, "bearer's route");
}

#[tokio::test]
async fn the_briefs_need_the_page_cookie_or_the_bearer_0678() {
	let _j = temp_jocasta();
	store::save(&Brief {
		id: "show-1".to_owned(),
		kind: "show".to_owned(),
		title: "T".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 5,
		sections: vec![Section::Text { heading: None, body: "b".to_owned() }],
	})
	.unwrap();
	let (responder, _) = responder();
	// No cookie: the earlier-run page for a browser, else a 403; never the brief's JSON.
	let refused = ask(&responder, "GET", "/brief/show-1.json", &[]).await;
	assert_eq!(refused.status(), 403);
	let page = ask(&responder, "GET", "/brief/show-1", &[("accept", "text/html")]).await;
	assert_eq!(page.status(), 403);
	assert!(text_of(page).await.contains("earlier run"));
	// Another port's cookie, and the API token as a cookie, open nothing.
	for cookie in ["jocasta_service_8=page", "jocasta_service_9=api", "jocasta_service_9="] {
		let got = ask(&responder, "GET", "/brief/show-1.json", &[("cookie", cookie)]).await;
		assert_eq!(got.status(), 403, "{cookie}");
	}
	let by_cookie = ask(&responder, "GET", "/brief/show-1.json", &[("cookie", "a=1; jocasta_service_9=page")]).await;
	assert_eq!(by_cookie.status(), 200);
	assert!(text_of(by_cookie).await.contains("\"title\":\"T\""));
	let by_bearer = ask(&responder, "GET", "/brief/show-1.json", &[("authorization", "Bearer api")]).await;
	assert_eq!(by_bearer.status(), 200);
}

#[tokio::test]
async fn a_brief_action_from_another_origin_is_refused_and_an_unknown_path_is_not_found() {
	let (responder, _) = responder();
	let cookie = ("cookie", "jocasta_service_9=page");
	let json = ("content-type", "application/json");
	let evil = ask(&responder, "POST", "/brief/show-1/act", &[cookie, json, ("origin", "http://evil.test")]).await;
	assert_eq!(evil.status(), 403);
	let missing = ask(&responder, "GET", "/no-such-file.js", &[cookie]).await;
	assert_eq!((missing.status().as_u16(), text_of(missing).await.as_str()), (404, "Not found"));
	let root = ask(&responder, "GET", "/", &[cookie]).await;
	assert_eq!(root.status(), 404);
}
