//! Tests of `routes.rs`: the hardening of 0678 on each request kind.

use axum::body::to_bytes;

use super::*;
use crate::frontier::brief::model::{Brief, Section};
use crate::shared::testing::temp_jocasta;

const PORT: u16 = 47400;

struct Echo;

impl BriefActions for Echo {
	fn act(&self, id: String, action: Value) -> BoxFuture<Result<Value>> {
		Box::pin(async move {
			if action["op"] == "refuse" {
				return Err(Error::user("not now"));
			}
			let mut brief = store::load(&id).expect("stored");
			brief.subtitle = action["op"].as_str().unwrap_or_default().to_owned();
			store::save(&brief)?;
			Ok(serde_json::to_value(&brief).unwrap())
		})
	}
}

fn routes() -> BriefRoutes {
	BriefRoutes::new(Arc::new(|| PORT), Arc::new(Echo))
}

fn stored() {
	let brief = Brief {
		id: "show-1".to_owned(),
		kind: "show".to_owned(),
		title: "T".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 5,
		sections: vec![Section::Text { heading: None, body: "b".to_owned() }],
	};
	store::save(&brief).unwrap();
}

fn get(path: &str, host: &str) -> Request {
	Request::builder().method("GET").uri(path).header("host", host).body(Body::empty()).unwrap()
}

fn post(path: &str, host: &str, origin: Option<&str>, content_type: &str, body: &str) -> Request {
	let mut builder =
		Request::builder().method("POST").uri(path).header("host", host).header("content-type", content_type);
	if let Some(origin) = origin {
		builder = builder.header("origin", origin);
	}
	builder.body(Body::from(body.to_owned())).unwrap()
}

async fn text_of(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn the_page_its_json_and_its_assets_are_served_to_the_servers_own_host() {
	let _j = temp_jocasta();
	stored();
	let host = format!("127.0.0.1:{PORT}");
	let page = routes().handle(get("/brief/show-1", &host)).await;
	assert_eq!(page.status(), StatusCode::OK);
	assert!(text_of(page).await.contains("/brief/assets/brief.js"));
	let json = routes().handle(get("/brief/show-1.json", &format!("localhost:{PORT}"))).await;
	assert_eq!(json.status(), StatusCode::OK);
	let value: Value = serde_json::from_str(&text_of(json).await).unwrap();
	assert_eq!(value["title"], "T");
	assert_eq!(value["sections"][0]["type"], "text");
	let script = routes().handle(get("/brief/assets/brief.js", &host)).await;
	assert_eq!(script.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
	let styles = routes().handle(get("/brief/assets/brief.css", &host)).await;
	assert_eq!(styles.headers()[header::CONTENT_TYPE], "text/css; charset=utf-8");
	assert_eq!(routes().handle(get("/brief/nope", &host)).await.status(), StatusCode::NOT_FOUND);
	assert_eq!(routes().handle(get("/brief/..%2Fx.json", &host)).await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn another_host_is_refused() {
	let _j = temp_jocasta();
	stored();
	for host in ["evil.test", "127.0.0.1:1", "evil.test:47400"] {
		assert_eq!(routes().handle(get("/brief/show-1.json", host)).await.status(), StatusCode::FORBIDDEN, "{host}");
	}
}

#[tokio::test]
async fn an_action_needs_the_servers_origin_and_json_and_answers_the_whole_brief() {
	let _j = temp_jocasta();
	stored();
	let host = format!("127.0.0.1:{PORT}");
	let origin = format!("http://{host}");
	let path = "/brief/show-1/act";
	let evil = post(path, &host, Some("http://evil.test"), "application/json", r#"{"op":"x"}"#);
	assert_eq!(routes().handle(evil).await.status(), StatusCode::FORBIDDEN);
	let none = post(path, &host, None, "application/json", r#"{"op":"x"}"#);
	assert_eq!(routes().handle(none).await.status(), StatusCode::FORBIDDEN);
	let form = post(path, &host, Some(&origin), "text/plain", r#"{"op":"x"}"#);
	assert_eq!(routes().handle(form).await.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
	let torn = post(path, &host, Some(&origin), "application/json", "{");
	assert_eq!(routes().handle(torn).await.status(), StatusCode::BAD_REQUEST);
	let ok = post(path, &host, Some(&origin), "application/json; charset=utf-8", r#"{"op":"hello"}"#);
	let answered = routes().handle(ok).await;
	assert_eq!(answered.status(), StatusCode::OK);
	let value: Value = serde_json::from_str(&text_of(answered).await).unwrap();
	assert_eq!((value["subtitle"].as_str(), value["id"].as_str()), (Some("hello"), Some("show-1")));
	let refused = post(path, &host, Some(&origin), "application/json", r#"{"op":"refuse"}"#);
	let answered = routes().handle(refused).await;
	assert_eq!(answered.status(), StatusCode::BAD_REQUEST);
	assert_eq!(text_of(answered).await, r#"{"error":"not now"}"#);
	let unknown = post("/brief/show-9/act", &host, Some(&origin), "application/json", "{}");
	assert_eq!(routes().handle(unknown).await.status(), StatusCode::NOT_FOUND);
}
