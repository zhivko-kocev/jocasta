//! The state feed (the Rust port of `feed.ts`): the `/ws` route and the socket behind it.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::Response;
use futures_util::future::BoxFuture as LocalBoxFuture;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use url::Url;

use crate::frontier::protocol::{ClientMessage, ServerMessage, WS_PATH};
use crate::frontier::server::routes::{BoxFuture, Route, ServerHandle, has_bearer, text_response};
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};

/// Answers one client only: a message sent down its socket.
pub type Reply = Arc<dyn Fn(ServerMessage) + Send + Sync>;

/// What the state feed needs of the service: its snapshot and its handling of a client's message.
pub trait FeedService: Send + Sync {
	/// The snapshot every client gets first; an `Err` is the TypeScript's throw, which fails that client's open.
	fn snapshot(&self) -> Result<ServerMessage>;
	/// One message from a client; `reply` answers that client only.
	fn handle(&self, message: ClientMessage, reply: Reply) -> LocalBoxFuture<'_, ()>;
}

/// Sends `message` to every client of the state feed on `server`.
pub fn publish(server: Option<&ServerHandle>, message: &ServerMessage) {
	if let (Some(server), Ok(text)) = (server, serde_json::to_string(message)) {
		server.publish(&text);
	}
}

/// The state feed's socket: the service's snapshot first, then its messages; each client message goes to the service.
#[derive(Clone)]
pub struct FeedSocket {
	service: Arc<dyn FeedService>,
}

/// The state feed: the service's snapshot first, then its messages; each client message goes to the service.
pub fn feed_socket(service: Arc<dyn FeedService>) -> FeedSocket {
	FeedSocket { service }
}

fn text_of(message: &ServerMessage) -> Option<String> {
	serde_json::to_string(message).ok()
}

impl FeedSocket {
	/// One client's socket until it closes or the server stops.
	async fn serve(&self, socket: WebSocket, server: ServerHandle) {
		// Subscribed before the snapshot goes out, so nothing published between the two is lost.
		let mut feed = server.subscribe();
		let (mut sink, mut incoming) = socket.split();
		let (replies, mut replied) = mpsc::unbounded_channel::<String>();
		let snapshot = match self.service.snapshot() {
			Ok(snapshot) => text_of(&snapshot),
			Err(error) => {
				log(LogLevel::Error, "server", &format!("the feed's snapshot failed: {error}"), None);
				None
			}
		};
		let Some(snapshot) = snapshot else { return };
		if sink.send(Message::Text(snapshot.into())).await.is_err() {
			return;
		}
		let reply: Reply = Arc::new(move |message| {
			if let Some(text) = text_of(&message) {
				let _ = replies.send(text);
			}
		});
		loop {
			let text = tokio::select! {
				published = feed.recv() => match published {
					Ok(text) => text,
					Err(broadcast::error::RecvError::Lagged(_)) => continue,
					Err(broadcast::error::RecvError::Closed) => break,
				},
				Some(text) = replied.recv() => text,
				frame = incoming.next() => {
					match frame {
						Some(Ok(Message::Text(data))) => self.received(data.as_str(), &reply),
						Some(Ok(Message::Close(_)) | Err(_)) | None => break,
						// Binary frames are skipped: the service sends text only (and so do its clients).
						Some(Ok(_)) => {}
					}
					continue;
				}
				() = server.closed() => {
					// What was published before the stop (`bye`) still goes out.
					while let Ok(text) = feed.try_recv() {
						if sink.send(Message::Text(text.into())).await.is_err() {
							break;
						}
					}
					break;
				}
			};
			if sink.send(Message::Text(text.into())).await.is_err() {
				break;
			}
		}
		let _ = sink.close().await;
	}

	/// One text frame from the client: not JSON is answered with an `error`; a message the service knows goes to it.
	fn received(&self, data: &str, reply: &Reply) {
		let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) else {
			reply(ServerMessage::Error { message: "not JSON".to_owned() });
			return;
		};
		// Any other type is ignored (SEC-NET-5), as the service ignores it.
		let Ok(message) = serde_json::from_value::<ClientMessage>(parsed) else { return };
		let service = self.service.clone();
		let reply = reply.clone();
		tokio::spawn(async move { service.handle(message, reply).await });
	}
}

/// The state feed's route (`/ws`): the bearer only, and no other origin (SEC-NET-4).
pub struct FeedRoute {
	token: String,
	socket: FeedSocket,
}

/// The state feed's route: see [`FeedRoute`]. `socket` is what runs once a client has upgraded.
pub fn feed_route(token: &str, socket: FeedSocket) -> FeedRoute {
	FeedRoute { token: token.to_owned(), socket }
}

impl Route for FeedRoute {
	fn matches(&self, path: &str) -> bool {
		path == WS_PATH
	}

	fn handle(&self, request: Request, _url: Url, server: ServerHandle) -> BoxFuture<Response> {
		let at = server.port();
		let forbidden = || text_response(StatusCode::FORBIDDEN, "Forbidden");
		if !has_bearer(&request, &self.token) {
			let refused = forbidden();
			return Box::pin(async move { refused });
		}
		let origin = request.headers().get("origin");
		if origin.is_some_and(|origin| origin.as_bytes() != format!("http://127.0.0.1:{at}").as_bytes()) {
			let refused = forbidden();
			return Box::pin(async move { refused });
		}
		let socket = self.socket.clone();
		Box::pin(async move {
			match WebSocketUpgrade::from_request(request, &()).await {
				Ok(upgrade) => upgrade.on_upgrade(move |ws| async move { socket.serve(ws, server).await }),
				Err(_) => text_response(StatusCode::BAD_REQUEST, "Upgrade failed"),
			}
		})
	}
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
