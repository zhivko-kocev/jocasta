//! The server-level cases of `test/frontier/server/whiteboard.test.ts`: the page's CSP and CORP headers, and
//! `/whiteboard/connection` through the running server. The rest of that file is in `whiteboard_tests.rs`.

use axum::body::Body;
use axum::http::StatusCode;
use serde_json::Value;

use super::*;
use crate::frontier::server::helpers::{http, with_server};
use crate::frontier::server::whiteboard::tests::fake_runtime;
use crate::frontier::server::whiteboard::{CANVAS_PATH, RuntimeChoice, WhiteboardOptions, canvas_asset};
use crate::shared::paths::data_dir;
use crate::shared::testing::temp_jocasta;

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
	response.headers().get(name).and_then(|value| value.to_str().ok())
}

#[test]
fn the_animation_stages_file_is_served_as_html_without_the_pages_policy_and_frameable_from_the_page_a_neighbouring_asset_and_the_page_keep_todays_headers()
 {
	let temp = temp_jocasta();
	let dir = temp.root().join("canvas-stage");
	std::fs::create_dir_all(dir.join("assets")).unwrap();
	std::fs::write(dir.join("assets").join("stage-1.html"), "<!doctype html>").unwrap();
	std::fs::write(dir.join("assets").join("canvas-1.js"), "export {}").unwrap();
	let stage = format!("{CANVAS_PATH}assets/stage-1.html");
	let script = format!("{CANVAS_PATH}assets/canvas-1.js");
	let served = canvas_asset(&dir, &stage).expect("the stage file");
	assert_eq!(served.content_type, "text/html; charset=utf-8");
	let html = |status: StatusCode, content_type: &str| {
		Response::builder().status(status).header("content-type", content_type).body(Body::from("x")).unwrap()
	};

	let guarded_stage = page_guarded(html(StatusCode::OK, served.content_type), &stage, 1234, None);
	assert_eq!(header(&guarded_stage, "content-security-policy"), None);
	assert_eq!(header(&guarded_stage, "cross-origin-resource-policy"), Some("cross-origin"));
	assert_eq!(header(&guarded_stage, "cross-origin-embedder-policy"), Some("require-corp"));

	let neighbours = [
		(html(StatusCode::OK, "text/javascript"), script),
		(html(StatusCode::OK, "text/html"), "/".to_owned()),
		// The stage's path with nothing behind it (a refusal, a 404) is the page's own response.
		(html(StatusCode::NOT_FOUND, "text/plain"), stage),
	];
	for (response, pathname) in neighbours {
		let guarded = page_guarded(response, &pathname, 1234, None);
		assert_eq!(header(&guarded, "content-security-policy"), Some(content_policy(1234, None).as_str()));
		assert!(header(&guarded, "content-security-policy").unwrap().contains("frame-ancestors 'none'"));
		assert_eq!(header(&guarded, "cross-origin-resource-policy"), Some("same-origin"));
		assert_eq!(header(&guarded, "cross-origin-embedder-policy"), Some("require-corp"));
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_gets_whiteboards_server_behind_its_token_and_the_csp_lets_it_connect_there() {
	let mut temp = temp_jocasta();
	temp.set_var("FAKE_WHITEBOARD", Some("1"));
	let runtime = fake_runtime(&temp);
	let home = data_dir().join("whiteboard");
	let whiteboard =
		WhiteboardOptions { runtime: RuntimeChoice::Given(runtime), home: Some(home), ..Default::default() };
	let options = ServerOptions { whiteboard: Some(Some(whiteboard)), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let client = http();
		let login = client.get(server.url()).send().await.unwrap();
		let cookie = login.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_owned();
		let base = format!("http://127.0.0.1:{}", server.port);
		let refused = client.get(format!("{base}/whiteboard/connection")).send().await.unwrap();
		assert_eq!(refused.status().as_u16(), 403);
		let page = client.get(format!("{base}/whiteboard/connection")).header("cookie", cookie).send().await.unwrap();
		let origin = server.whiteboard.as_ref().expect("the whiteboard").origin();
		let policy = page.headers()["content-security-policy"].to_str().unwrap().to_owned();
		assert!(policy.contains(&format!("connect-src 'self' ws://127.0.0.1:{} {origin}", server.port)), "{policy}");
		let status = page.status().as_u16();
		let body: Value = page.json().await.unwrap();
		// The repo's clone may be unbuilt in a test run: then the page hears why.
		if status == 200 {
			assert_eq!(body["url"], origin);
			assert_eq!(body["token"], "wb-token");
			assert!(body["canvas"]["script"].as_str().unwrap().starts_with(CANVAS_PATH));
		} else {
			assert_eq!(status, 503);
			assert!(body["error"].as_str().unwrap().contains("canvas isn't built"), "{body}");
		}
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_whiteboard_off_the_page_hears_so_and_the_csp_names_only_this_server() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let url = format!("http://127.0.0.1:{}/whiteboard/connection", server.port);
		let response = http().get(url).bearer_auth(&server.token).send().await.unwrap();
		assert_eq!(response.status().as_u16(), 503);
		let policy = response.headers()["content-security-policy"].to_str().unwrap().to_owned();
		assert!(policy.contains(&format!("connect-src 'self' ws://127.0.0.1:{};", server.port)), "{policy}");
		let body: Value = response.json().await.unwrap();
		assert_eq!(body["error"], "whiteboard is off");
	})
	.await;
}
