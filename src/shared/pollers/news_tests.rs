//! Tests of `news.rs`, ported from the news case of `test/pollers/dashboard.test.ts`; the headline endpoint is served
//! in-process, so nothing reaches the network.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

/// Two items as brutalist.report's `/api/v1/headlines` answers them (fields trimmed from a live answer, 2026-10-14).
fn headlines() -> Value {
	json!([
		{
			"uuid": "f8d2098b",
			"title": "Amazon Alexa Tablet 12 Pro review:\n gorgeous screen",
			"url": "https://www.techmeme.com/261014/p20#a261014p20",
			"comment_url": null,
			"source": "techmeme",
			"topic": "tech",
			"age": "[2m]",
			"published_at": "2026-10-14T13:20:02Z",
			"inserted_at": "2026-10-14T13:21:29.952846Z",
			"content": "the whole article text, never kept",
			"keywords": ["Amazon"],
			"sentiment_compound": 0.57,
		},
		{
			"uuid": "a1",
			"title": "Kernel 7.1 released",
			"url": "https://lwn.net/Articles/1/",
			"source": "lwn",
			"published_at": "2026-10-14T13:40:00Z",
		},
		{ "uuid": "bad", "title": "no link", "url": "javascript:alert(1)", "source": "x", "published_at": "2026-10-14T13:00:00Z" },
	])
}

/// An in-process HTTP server answering from a table of request-target prefixes `(prefix, status, body)` and recording
/// each target.
struct Fake {
	endpoint: String,
	asked: Arc<Mutex<Vec<String>>>,
}

impl Fake {
	async fn serve(answers: Vec<(&'static str, u16, String)>) -> Self {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listening");
		let endpoint = format!("http://{}/api/v1/headlines", listener.local_addr().expect("its address"));
		let asked = Arc::new(Mutex::new(Vec::new()));
		let seen = Arc::clone(&asked);
		tokio::spawn(async move {
			while let Ok((mut stream, _)) = listener.accept().await {
				let mut head = Vec::new();
				let mut chunk = [0u8; 1024];
				while !head.windows(4).any(|window| window == b"\r\n\r\n") {
					match stream.read(&mut chunk).await {
						Ok(0) | Err(_) => break,
						Ok(n) => head.extend_from_slice(&chunk[..n]),
					}
				}
				let text = String::from_utf8_lossy(&head).into_owned();
				let target = text.split_whitespace().nth(1).unwrap_or("").to_owned();
				seen.lock().unwrap().push(target.clone());
				let (status, body) = answers
					.iter()
					.find(|(prefix, ..)| target.starts_with(prefix))
					.map_or((404, String::new()), |(_, status, body)| (*status, body.clone()));
				let reply = format!(
					"HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
					body.len()
				);
				let _ = stream.write_all(reply.as_bytes()).await;
				let _ = stream.shutdown().await;
			}
		});
		Self { endpoint, asked }
	}

	fn asked(&self) -> Vec<String> {
		self.asked.lock().unwrap().clone()
	}
}

fn source(name: &str) -> NewsFeed {
	NewsFeed::Source { source: name.to_owned(), limit: None }
}

fn titles(items: &[Headline]) -> Vec<&str> {
	items.iter().map(|item| item.title.as_str()).collect()
}

#[tokio::test]
async fn news_0429_0442_titles_source_time_and_link_only_newest_first_one_request_per_feed_failures_name_the_feed_never_the_payload()
 {
	let found = headlines_in(&headlines()).unwrap();
	assert_eq!(
		found,
		vec![
			Headline {
				title: "Kernel 7.1 released".to_owned(),
				source: "lwn".to_owned(),
				url: "https://lwn.net/Articles/1/".to_owned(),
				at: "2026-10-14T13:40:00.000Z".to_owned(),
			},
			Headline {
				title: "Amazon Alexa Tablet 12 Pro review: gorgeous screen".to_owned(),
				source: "techmeme".to_owned(),
				url: "https://www.techmeme.com/261014/p20#a261014p20".to_owned(),
				at: "2026-10-14T13:20:02.000Z".to_owned(),
			},
		]
	);
	let fake = Fake::serve(vec![
		("/api/v1/headlines?source=hn", 200, headlines().to_string()),
		("/api/v1/headlines?source=lwn", 200, "[]".to_owned()),
		("/api/v1/headlines?topic=gaming", 200, headlines().to_string()),
		("/api/v1/headlines?topic=acme", 200, "[]".to_owned()),
	])
	.await;
	let feeds = [
		source("hn"),
		source("lwn"),
		NewsFeed::Topic { topic: "gaming".to_owned(), limit: Some(1) },
		NewsFeed::Topic { topic: "acme".to_owned(), limit: None },
	];
	let lists = poll_news_at(&feeds, &fake.endpoint).await.unwrap();
	// No `limit` for a feed without one: brutalist.report then answers every item it holds.
	assert_eq!(
		fake.asked(),
		[
			"/api/v1/headlines?source=hn",
			"/api/v1/headlines?source=lwn",
			"/api/v1/headlines?topic=gaming&limit=1",
			"/api/v1/headlines?topic=acme",
		]
	);
	let shown: Vec<(&str, Vec<&str>)> = lists.iter().map(|l| (l.heading.as_str(), titles(&l.items))).collect();
	assert_eq!(
		shown,
		vec![
			("Hacker News", vec!["Kernel 7.1 released", "Amazon Alexa Tablet 12 Pro review: gorgeous screen"]),
			("LWN", vec![]),
			("Gaming", vec!["Kernel 7.1 released"]),
			("Acme", vec![]),
		]
	);

	let down = Fake::serve(vec![("/api/v1/headlines", 503, "busy".to_owned())]).await;
	let failure = poll_news_at(&[source("hn")], &down.endpoint).await.unwrap_err();
	assert_eq!(failure.to_string(), "News (Hacker News): brutalist.report answered HTTP 503");
	// Nothing listening: bound, then closed.
	let closed = {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		format!("http://{}/api/v1/headlines", listener.local_addr().unwrap())
	};
	let failure = poll_news_at(&[source("lwn")], &closed).await.unwrap_err();
	assert_eq!(failure.to_string(), "News (LWN): brutalist.report couldn't be reached");
	let moved =
		Fake::serve(vec![("/api/v1/headlines", 200, json!({"message": "this API has been moved"}).to_string())]).await;
	let feed = NewsFeed::Topic { topic: "gaming".to_owned(), limit: Some(10) };
	let failure = poll_news_at(&[feed], &moved.endpoint).await.unwrap_err();
	assert_eq!(failure.to_string(), "News (Gaming): brutalist.report's answer isn't a headline list");
}
