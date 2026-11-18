//! Tests of `page_routes.rs`: `server.test.ts`'s launch code, cookie and earlier-run-page cases, and the responder's
//! answers (which the TypeScript tests through `withServer` and `Page`).

use std::sync::atomic::{AtomicI64, Ordering};

use axum::body::to_bytes;
use serde_json::Value;

use super::*;
use crate::frontier::server::whiteboard::{RuntimeChoice, WhiteboardOptions, start_whiteboard};
use crate::shared::testing::temp_dir;

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
		whiteboard: None,
		bearer_routes: vec![Box::new(Health)],
	});
	(responder, codes)
}

async fn ask(responder: &PageResponder, method: &str, path: &str, headers: &[(&str, &str)]) -> Response {
	let url = Url::parse(&format!("http://127.0.0.1:9{path}")).unwrap();
	responder(request_at(method, path, headers), url).await
}

#[tokio::test]
async fn a_launch_code_is_swapped_for_the_page_cookie_and_the_rest_of_the_query_stays() {
	let (responder, codes) = responder();
	let code = codes.mint();
	let swapped = ask(&responder, "GET", &format!("/?t={code}&review=a%20b&x=1"), &[]).await;
	assert_eq!(swapped.status(), 302);
	assert_eq!(swapped.headers()[header::LOCATION], "/?review=a+b&x=1");
	assert_eq!(swapped.headers()[header::SET_COOKIE], "jocasta_service_9=page; Path=/; HttpOnly; SameSite=Strict");
	assert_eq!(text_of(swapped).await, "");
	// A code is taken once; another or a used one is the earlier-run page for a browser, else a plain 403.
	let used = ask(&responder, "GET", &format!("/?t={code}"), &[("accept", "text/html")]).await;
	assert_eq!(used.status(), 403);
	assert!(text_of(used).await.contains("earlier run"));
	let nope = ask(&responder, "GET", "/?t=nope", &[]).await;
	assert_eq!((nope.status().as_u16(), text_of(nope).await.as_str()), (403, "Forbidden"));
	// Nothing else in the query: `/`.
	let bare = ask(&responder, "GET", &format!("/?t={}", codes.mint()), &[]).await;
	assert_eq!(bare.headers()[header::LOCATION], "/");
}

#[tokio::test]
async fn the_bearers_routes_answer_before_the_cookie_is_asked_for() {
	let (responder, _) = responder();
	assert_eq!(text_of(ask(&responder, "GET", "/health", &[]).await).await, "bearer's route");
}

#[tokio::test]
async fn the_pages_need_the_page_cookie_or_the_bearer() {
	let (responder, _) = responder();
	let refused = ask(&responder, "GET", "/", &[]).await;
	assert_eq!(refused.status(), 403);
	// Another port's cookie, and the API token as a cookie, open nothing.
	for cookie in ["jocasta_service_8=page", "jocasta_service_9=api", "jocasta_service_9="] {
		assert_eq!(ask(&responder, "GET", "/", &[("cookie", cookie)]).await.status(), 403, "{cookie}");
	}
	let by_cookie = ask(&responder, "GET", "/", &[("cookie", "a=1; jocasta_service_9=page")]).await;
	assert_eq!(by_cookie.status(), 200);
	assert!(by_cookie.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
	assert_eq!(by_cookie.headers()[header::CACHE_CONTROL], "no-store");
	let by_bearer = ask(&responder, "GET", "/", &[("authorization", "Bearer api")]).await;
	assert_eq!(by_bearer.status(), 200);
}

#[tokio::test]
async fn only_get_serves_a_page_and_an_unknown_file_is_not_found() {
	let (responder, _) = responder();
	let cookie = [("cookie", "jocasta_service_9=page")];
	let post = ask(&responder, "POST", "/", &cookie).await;
	assert_eq!((post.status().as_u16(), text_of(post).await.as_str()), (404, "Not found"));
	let missing = ask(&responder, "GET", "/no-such-file.js", &cookie).await;
	assert_eq!((missing.status().as_u16(), text_of(missing).await.as_str()), (404, "Not found"));
	// A canvas path with no canvas build, or outside its assets, is not found as well.
	let canvas = ask(&responder, "GET", "/whiteboard/canvas/../../secret", &cookie).await;
	assert_eq!(canvas.status(), 404);
}

#[tokio::test]
async fn whiteboards_connection_and_worker_need_the_cookie_and_say_why_when_off() {
	let (responder, _) = responder();
	assert_eq!(ask(&responder, "GET", CONNECTION_PATH, &[]).await.status(), 403);
	let off = ask(&responder, "GET", CONNECTION_PATH, &[("cookie", "jocasta_service_9=page")]).await;
	assert_eq!(off.status(), 503);
	assert_eq!(off.headers()[header::CONTENT_TYPE], "application/json;charset=utf-8");
	let body: Value = serde_json::from_str(&text_of(off).await).unwrap();
	assert_eq!(body, json!({ "error": "whiteboard is off" }));
	assert_eq!(ask(&responder, "GET", WORKER_PATH, &[]).await.status(), 403);
	let worker = ask(&responder, "GET", WORKER_PATH, &[("authorization", "Bearer api")]).await;
	assert_eq!(worker.status(), 200);
	assert_eq!(worker.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
}

#[tokio::test]
async fn a_whiteboard_that_never_started_is_a_503_with_its_reason() {
	let home = temp_dir("page-routes-wb");
	let whiteboard = Arc::new(start_whiteboard(WhiteboardOptions {
		runtime: RuntimeChoice::Absent,
		home: Some(home.path().to_path_buf()),
		..WhiteboardOptions::default()
	}));
	let response = whiteboard_connection(Some(&whiteboard)).await;
	assert_eq!(response.status(), 503);
	let body: Value = serde_json::from_str(&text_of(response).await).unwrap();
	assert!(body["error"].is_string(), "{body}");
}

#[tokio::test]
async fn the_boss_shows_a_review_by_opening_its_id_on_the_canvas_page() {
	let home = temp_dir("page-routes-reviews");
	let whiteboard = Arc::new(start_whiteboard(WhiteboardOptions {
		runtime: RuntimeChoice::Absent,
		home: Some(home.path().to_path_buf()),
		..WhiteboardOptions::default()
	}));
	let opened = Arc::new(Mutex::new(Vec::<String>::new()));
	let record = opened.clone();
	let reviews = whiteboard_reviews(whiteboard, Arc::new(move |query| locked(&record).push(query.to_owned())));
	(reviews.open_review)("a b/ü'(1)");
	(reviews.open_review)("plain-id_1.x");
	assert_eq!(*locked(&opened), ["&review=a%20b%2F%C3%BC'(1)", "&review=plain-id_1.x"]);
	// Without a server answering, the lookup fails with why.
	assert!((reviews.find_review)("abc".to_owned()).await.is_err());
}

#[test]
fn encode_uri_component_leaves_what_javascript_leaves() {
	assert_eq!(encode_uri_component("AZaz09-_.!~*'()"), "AZaz09-_.!~*'()");
	assert_eq!(encode_uri_component(" #&=?/+€"), "%20%23%26%3D%3F%2F%2B%E2%82%AC");
}
