use super::*;
use crate::shared::testing::temp_jocasta;
use axum::http::Method;

fn request(headers: &[(&str, &str)]) -> Request {
	let mut builder = axum::http::Request::builder().uri("http://127.0.0.1/");
	for (name, value) in headers {
		builder = builder.header(*name, *value);
	}
	builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn the_request_helpers_bearer_the_hosts_answered_args_only_as_an_object() {
	assert_eq!(bearer_token(&request(&[("authorization", "Bearer abc")])), "abc");
	assert_eq!(bearer_token(&request(&[("authorization", "Basic abc")])), "");
	assert_eq!(bearer_token(&request(&[])), "");
	assert!(host_allowed(Some("127.0.0.1:9"), 9));
	assert!(host_allowed(Some("localhost:9"), 9));
	assert!(!host_allowed(Some("127.0.0.1:8"), 9));
	assert!(!host_allowed(Some("host.docker.internal:9"), 9));
	assert!(!host_allowed(None, 9));
	assert_eq!(Value::Object(object_args(&json!({ "a": 1 }))), json!({ "a": 1 }));
	for value in [Value::Null, json!([1]), json!("x")] {
		assert!(object_args(&value).is_empty());
	}
}

#[tokio::test]
async fn a_post_route_refuses_a_missing_token_and_another_method_as_json_or_plain_text() {
	let post = |auth: Option<&str>, method: Method| {
		let mut builder = axum::http::Request::builder().method(method).uri("/api/x");
		if let Some(auth) = auth {
			builder = builder.header("authorization", auth);
		}
		builder.body(Body::empty()).unwrap()
	};
	let body = |response: Response| async move {
		String::from_utf8(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
	};
	let refused = refused_post(&post(None, Method::POST), "t", true).unwrap();
	assert_eq!(refused.status(), 401);
	assert_eq!(refused.headers()[header::CONTENT_TYPE], "application/json;charset=utf-8");
	assert_eq!(body(refused).await, r#"{"text":"invalid token","isError":true}"#);
	let wrong = refused_post(&post(Some("Bearer nope"), Method::POST), "t", false).unwrap();
	assert_eq!(wrong.status(), 401);
	assert_eq!(wrong.headers()[header::CONTENT_TYPE], "text/plain;charset=utf-8");
	assert_eq!(body(wrong).await, "invalid token");
	let method = refused_post(&post(Some("Bearer t"), Method::GET), "t", true).unwrap();
	assert_eq!(method.status(), 405);
	assert_eq!(body(method).await, r#"{"text":"POST only","isError":true}"#);
	assert_eq!(body(refused_post(&post(Some("Bearer t"), Method::PUT), "t", false).unwrap()).await, "POST only");
	assert!(refused_post(&post(Some("Bearer t"), Method::POST), "t", true).is_none());
}

#[tokio::test]
async fn a_body_is_its_json_object_or_array_else_an_empty_object() {
	let with = |text: &str| axum::http::Request::builder().body(Body::from(text.to_owned())).unwrap();
	assert_eq!(body_object(with(r#"{"a":1}"#)).await, json!({ "a": 1 }));
	assert_eq!(body_object(with("[1,2]")).await, json!([1, 2]));
	for text in ["", "nope", "3", "null", "\"x\""] {
		assert_eq!(body_object(with(text)).await, json!({}), "{text:?}");
	}
}

struct Fixed {
	prefix: &'static str,
	answer: &'static str,
}

impl Route for Fixed {
	fn matches(&self, path: &str) -> bool {
		path.starts_with(self.prefix)
	}

	fn handle(&self, _request: Request, url: Url, server: ServerHandle) -> BoxFuture<Response> {
		let answer = format!("{}:{}:{}", self.answer, url.path(), server.port());
		Box::pin(async move { text_response(StatusCode::OK, &answer) })
	}
}

struct Panics;

impl Route for Panics {
	fn matches(&self, path: &str) -> bool {
		path == "/boom"
	}

	fn handle(&self, _request: Request, _url: Url, _server: ServerHandle) -> BoxFuture<Response> {
		Box::pin(async { panic!("a route's own bug") })
	}
}

fn served(routes: Vec<Box<dyn Route>>) -> Served {
	Served {
		routes,
		page: Box::new(|_request, url, port| {
			let answer = format!("page:{}:{port}", url.path());
			Box::pin(async move { text_response(StatusCode::OK, &answer) })
		}),
	}
}

async fn get(client: &reqwest::Client, port: u16, path: &str, host: &str) -> (u16, String) {
	let response = client.get(format!("http://127.0.0.1:{port}{path}")).header("host", host).send().await.unwrap();
	(response.status().as_u16(), response.text().await.unwrap())
}

#[tokio::test]
async fn only_its_own_host_is_answered_dns_rebinding_a_containers_host_docker_internal_no_longer_se_3_0603() {
	let server = listen(0, served(vec![])).unwrap();
	let port = server.port();
	let client = reqwest::Client::new();
	for host in [format!("evil.test:{port}"), format!("host.docker.internal:{port}")] {
		assert_eq!(get(&client, port, "/health", &host).await, (403, "Forbidden".to_owned()));
	}
	assert_eq!(get(&client, port, "/health", &format!("localhost:{port}")).await.0, 200);
	assert_eq!(get(&client, port, "/health", &format!("127.0.0.1:{port}")).await.0, 200);
	server.stop(true).await;
}

#[tokio::test]
async fn the_first_route_that_matches_answers_else_the_page() {
	let routes: Vec<Box<dyn Route>> = vec![
		Box::new(Fixed { prefix: "/api/a", answer: "first" }),
		Box::new(Fixed { prefix: "/api", answer: "second" }),
	];
	let server = listen(0, served(routes)).unwrap();
	let port = server.port();
	let client = reqwest::Client::new();
	let host = format!("127.0.0.1:{port}");
	assert_eq!(get(&client, port, "/api/a/b?x=1", &host).await.1, format!("first:/api/a/b:{port}"));
	assert_eq!(get(&client, port, "/api/b", &host).await.1, format!("second:/api/b:{port}"));
	assert_eq!(get(&client, port, "/other", &host).await.1, format!("page:/other:{port}"));
	server.stop(true).await;
}

#[tokio::test]
async fn a_route_that_panics_answers_500_internal_error_and_the_server_goes_on() {
	let _j = temp_jocasta();
	let server = listen(0, served(vec![Box::new(Panics)])).unwrap();
	let port = server.port();
	let client = reqwest::Client::new();
	let host = format!("127.0.0.1:{port}");
	assert_eq!(get(&client, port, "/boom", &host).await, (500, "Internal error".to_owned()));
	assert_eq!(get(&client, port, "/after", &host).await.0, 200);
	server.stop(true).await;
}

#[tokio::test]
async fn service_port_is_tried_first_when_busy_the_service_takes_a_free_port_0358() {
	let _j = temp_jocasta();
	let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
	let busy = blocker.local_addr().unwrap().port();
	let server = listen(busy, served(vec![])).unwrap();
	assert_ne!(server.port(), busy);
	let client = reqwest::Client::new();
	assert_eq!(get(&client, server.port(), "/", &format!("127.0.0.1:{}", server.port())).await.0, 200);
	server.stop(true).await;
	// A free port is taken as asked.
	drop(blocker);
	let again = listen(busy, served(vec![])).unwrap();
	assert_eq!(again.port(), busy);
	again.stop(true).await;
}

#[tokio::test]
async fn publish_reaches_every_subscriber_and_stop_frees_the_port_and_ends_closed() {
	let server = listen(0, served(vec![])).unwrap();
	let port = server.port();
	let (mut one, mut two) = (server.subscribe(), server.subscribe());
	server.publish(r#"{"type":"x"}"#);
	assert_eq!(one.recv().await.unwrap(), r#"{"type":"x"}"#);
	assert_eq!(two.recv().await.unwrap(), r#"{"type":"x"}"#);
	let waiting = {
		let server = server.clone();
		tokio::spawn(async move { server.closed().await })
	};
	server.stop(true).await;
	tokio::time::timeout(Duration::from_secs(5), waiting).await.expect("closed resolves").unwrap();
	assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(), "the port is free again");
}

#[tokio::test]
async fn stopping_with_close_active_drops_a_request_still_being_answered() {
	struct Slow;
	impl Route for Slow {
		fn matches(&self, path: &str) -> bool {
			path == "/slow"
		}

		fn handle(&self, _request: Request, _url: Url, _server: ServerHandle) -> BoxFuture<Response> {
			Box::pin(async {
				tokio::time::sleep(Duration::from_secs(600)).await;
				text_response(StatusCode::OK, "late")
			})
		}
	}
	let server = listen(0, served(vec![Box::new(Slow)])).unwrap();
	let port = server.port();
	let client = reqwest::Client::new();
	let pending = tokio::spawn(async move { client.get(format!("http://127.0.0.1:{port}/slow")).send().await });
	tokio::time::sleep(Duration::from_millis(200)).await;
	server.stop(true).await;
	let answered = tokio::time::timeout(Duration::from_secs(5), pending).await.expect("the request ends").unwrap();
	match answered {
		Err(_) => {}
		Ok(response) => assert!(response.text().await.is_err(), "the body is cut off"),
	}
}
