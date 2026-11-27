//! Tests of `feed.rs`. The TypeScript's feed case (`server.test.ts`: every client gets the state feed) runs the whole
//! service; here the route and its socket run on a listening server with a stand-in service. What needs `server.rs`
//! (the service file, the shutdown) waits for wave 11 (see the report).

use std::sync::Mutex;
use std::time::Duration;

use futures_util::future::BoxFuture as LocalBoxFuture;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite};

use super::*;
use crate::frontier::server::routes::{Served, listen};

fn snapshot() -> ServerMessage {
	serde_json::from_value(json!({
		"type": "snapshot",
		"hud": { "running": 1, "blocked": 0, "awaiting": 0, "spendToday": null, "quota": [] },
		"today": { "date": "2026-10-08", "calendar": [], "needsYou": [], "landed": [], "underway": [], "next": [] },
		"dashboard": {
			"news": [], "newsError": null, "weather": null, "weatherError": null, "machine": null, "services": [],
			"activity": { "events": [], "worklog": [], "summaries": [] }, "leftovers": []
		}
	}))
	.unwrap()
}

#[derive(Default)]
struct Stand {
	handled: Mutex<Vec<ClientMessage>>,
}

impl FeedService for Stand {
	fn snapshot(&self) -> Result<ServerMessage> {
		Ok(snapshot())
	}

	fn handle(&self, message: ClientMessage, reply: Reply) -> LocalBoxFuture<'_, ()> {
		Box::pin(async move {
			if let ClientMessage::PaneFocus { pane_id } = &message {
				reply(ServerMessage::Error { message: format!("no pane {pane_id}") });
			}
			self.handled.lock().unwrap().push(message);
		})
	}
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Running {
	stand: Arc<Stand>,
	server: ServerHandle,
	client: reqwest::Client,
}

fn running() -> Running {
	let stand = Arc::new(Stand::default());
	let route = feed_route("t", feed_socket(stand.clone()));
	let server = listen(
		0,
		Served {
			routes: vec![Box::new(route)],
			page: Box::new(|_request, _url, _port| Box::pin(async { text_response(StatusCode::NOT_FOUND, "page") })),
		},
	)
	.unwrap();
	Running { stand, server, client: reqwest::Client::new() }
}

impl Running {
	async fn connect(&self, headers: &[(&str, String)]) -> Result<Socket, tungstenite::Error> {
		let mut request = format!("ws://127.0.0.1:{}{WS_PATH}", self.server.port()).into_client_request().unwrap();
		for (name, value) in headers {
			request
				.headers_mut()
				.insert(tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap());
		}
		tokio_tungstenite::connect_async(request).await.map(|(socket, _)| socket)
	}

	async fn open(&self) -> Socket {
		self.connect(&[("authorization", "Bearer t".to_owned())]).await.unwrap()
	}
}

async fn next(socket: &mut Socket) -> Value {
	loop {
		let frame = tokio::time::timeout(Duration::from_secs(5), socket.next()).await.expect("a frame in time");
		match frame {
			Some(Ok(tungstenite::Message::Text(text))) => return serde_json::from_str(text.as_str()).unwrap(),
			Some(Ok(_)) => {}
			other => panic!("the feed ended: {other:?}"),
		}
	}
}

async fn send(socket: &mut Socket, text: &str) {
	socket.send(tungstenite::Message::text(text)).await.unwrap();
}

fn message_json(message: &ServerMessage) -> Value {
	serde_json::to_value(message).unwrap()
}

#[tokio::test]
async fn every_client_gets_the_snapshot_first_and_then_what_is_published() {
	let r = running();
	let (mut page, mut second) = (r.open().await, r.open().await);
	assert_eq!(next(&mut page).await, message_json(&snapshot()));
	assert_eq!(next(&mut second).await, message_json(&snapshot()));
	publish(Some(&r.server), &ServerMessage::Action { text: "Finished".to_owned(), is_error: false });
	let published = json!({ "type": "action", "text": "Finished", "isError": false });
	assert_eq!(next(&mut page).await, published);
	assert_eq!(next(&mut second).await, published);
	// Without a server there is nobody to publish to.
	publish(None, &ServerMessage::Bye);
	r.server.stop(true).await;
}

#[tokio::test]
async fn a_client_that_sends_something_that_is_not_json_is_told_and_stays_connected() {
	let r = running();
	let mut page = r.open().await;
	next(&mut page).await;
	send(&mut page, "not json at all").await;
	assert_eq!(next(&mut page).await, json!({ "type": "error", "message": "not JSON" }));
	// Binary frames are skipped, a type nobody knows is ignored (SEC-NET-5): the feed goes on.
	page.send(tungstenite::Message::binary(vec![1, 2, 3])).await.unwrap();
	send(&mut page, r#"{"type":"pane.shoot","paneId":"p1"}"#).await;
	send(&mut page, "7").await;
	send(&mut page, "still not json").await;
	assert_eq!(next(&mut page).await, json!({ "type": "error", "message": "not JSON" }));
	assert!(r.stand.handled.lock().unwrap().is_empty());
	r.server.stop(true).await;
}

#[tokio::test]
async fn a_clients_message_goes_to_the_service_and_its_reply_goes_to_that_client_only() {
	let r = running();
	let (mut page, mut other) = (r.open().await, r.open().await);
	next(&mut page).await;
	next(&mut other).await;
	send(&mut page, r#"{"type":"pane.focus","paneId":"p7"}"#).await;
	assert_eq!(next(&mut page).await, json!({ "type": "error", "message": "no pane p7" }));
	send(&mut page, r#"{"type":"link.open","url":"https://example.org/x"}"#).await;
	for _ in 0..200 {
		if r.stand.handled.lock().unwrap().len() == 2 {
			break;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	assert_eq!(
		*r.stand.handled.lock().unwrap(),
		[
			ClientMessage::PaneFocus { pane_id: "p7".to_owned() },
			ClientMessage::LinkOpen { url: "https://example.org/x".to_owned() }
		]
	);
	// The other client heard nothing of it: its next frame is its own later message.
	send(&mut other, "nope").await;
	assert_eq!(next(&mut other).await, json!({ "type": "error", "message": "not JSON" }));
	r.server.stop(true).await;
}

#[tokio::test]
async fn the_feed_is_for_the_bearer_only_and_no_other_origin() {
	let r = running();
	let port = r.server.port();
	let route = feed_route("t", feed_socket(r.stand.clone()));
	assert!(Route::matches(&route, "/ws"));
	assert!(!Route::matches(&route, "/ws/"));
	for headers in [
		vec![],
		vec![("authorization", "Bearer nope".to_owned())],
		// A cookie is no bearer (SEC-NET-4).
		vec![("cookie", format!("jocasta_service_{port}=t"))],
		vec![("authorization", "Bearer t".to_owned()), ("origin", "https://evil.test".to_owned())],
		vec![("authorization", "Bearer t".to_owned()), ("origin", format!("http://localhost:{port}"))],
	] {
		let Err(tungstenite::Error::Http(response)) = r.connect(&headers).await else {
			panic!("{headers:?} connected")
		};
		assert_eq!(response.status(), 403, "{headers:?}");
		assert_eq!(response.body().as_deref(), Some(&b"Forbidden"[..]));
	}
	let own = vec![("authorization", "Bearer t".to_owned()), ("origin", format!("http://127.0.0.1:{port}"))];
	let mut page = r.connect(&own).await.unwrap();
	assert_eq!(next(&mut page).await["type"], "snapshot");
	r.server.stop(true).await;
}

#[tokio::test]
async fn a_request_that_does_not_upgrade_is_a_plain_400() {
	let r = running();
	let port = r.server.port();
	let plain = r.client.get(format!("http://127.0.0.1:{port}/ws")).bearer_auth("t").send().await.unwrap();
	assert_eq!(plain.status(), 400);
	assert_eq!(plain.headers()["content-type"], "text/plain;charset=utf-8");
	assert_eq!(plain.text().await.unwrap(), "Upgrade failed");
	r.server.stop(true).await;
}

#[tokio::test]
async fn what_was_published_before_the_server_stops_still_reaches_the_client_and_then_the_feed_closes() {
	let r = running();
	let mut page = r.open().await;
	next(&mut page).await;
	publish(Some(&r.server), &ServerMessage::Bye);
	let server = r.server.clone();
	tokio::spawn(async move { server.stop(true).await });
	assert_eq!(next(&mut page).await, json!({ "type": "bye" }));
	let ended = tokio::time::timeout(Duration::from_secs(5), async {
		loop {
			match page.next().await {
				None | Some(Err(_) | Ok(tungstenite::Message::Close(_))) => return,
				Some(Ok(_)) => {}
			}
		}
	})
	.await;
	assert!(ended.is_ok());
}
