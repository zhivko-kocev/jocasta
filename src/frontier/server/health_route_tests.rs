//! Tests of `health_route.rs`. The TypeScript has no test of its own for it (`server.test.ts` reads `/health` through
//! the whole service); these check the route's answers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::body::to_bytes;
use serde_json::Value;

use super::*;

fn request(path: &str, bearer: Option<&str>) -> Request {
	let mut builder = Request::builder().uri(path);
	if let Some(bearer) = bearer {
		builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
	}
	builder.body(Body::empty()).unwrap()
}

async fn body_of(response: Response) -> Value {
	let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
	serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_is_forbidden_without_the_bearer_and_answers_pid_and_switches_with_it() {
	let closing = Arc::new(AtomicBool::new(false));
	let flag = Arc::clone(&closing);
	let route = health_route("secret", move || flag.load(Ordering::SeqCst));
	assert!(route.matches("/health"));
	assert!(!route.matches("/healthy"));
	let url = Url::parse("http://127.0.0.1/health").unwrap();
	let refused = route.handle(request("/health", Some("wrong")), url.clone()).await;
	assert_eq!(refused.status(), StatusCode::FORBIDDEN);
	let refused = route.handle(request("/health", None), url.clone()).await;
	assert_eq!(refused.status(), StatusCode::FORBIDDEN);

	let answered = route.handle(request("/health", Some("secret")), url.clone()).await;
	assert_eq!(answered.status(), StatusCode::OK);
	let body = body_of(answered).await;
	let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
	assert_eq!(keys, ["pid", "layer", "hosts"]);
	assert_eq!(body["pid"], std::process::id());

	closing.store(true, Ordering::SeqCst);
	let body = body_of(route.handle(request("/health", Some("secret")), url).await).await;
	assert_eq!(body["closing"], true);
}
