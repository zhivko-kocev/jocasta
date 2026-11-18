//! The service's feed, recorded (A5): the service's log holds warnings only, so toasts are taken from its state socket
//! (`/ws`, the token as a bearer), which broadcasts every snapshot, toast and HUD message; each message is appended
//! with its arrival time.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Message, http};

use crate::lib::output::json_text;
use jocasta::frontier::protocol::WS_PATH;
use jocasta::shared::clock::now_ms;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::jsonl::json_lines;
use jocasta::shared::service_file::read_service_file;

/// How long a stopped recording waits for the service to finish the close handshake.
const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// One recorded message: when it arrived, and the message (`type`, `reason`, `level`, `text`, `paneId`, `priority`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedEntry {
	/// Epoch ms when the message arrived.
	pub at: f64,
	pub message: Map<String, Value>,
}

impl FeedEntry {
	/// A string field of the message.
	pub fn text(&self, key: &str) -> Option<&str> {
		self.message.get(key).and_then(Value::as_str)
	}
}

pub fn parse_feed(text: &str) -> Vec<FeedEntry> {
	json_lines::<Map<String, Value>>(text)
		.into_iter()
		.filter_map(|entry| {
			let at = entry.get("at")?.as_f64()?;
			let message = entry.get("message")?.as_object()?.clone();
			Some(FeedEntry { at, message })
		})
		.collect()
}

/// The toasts in a feed; `urgent_only` keeps those the service also raises as a herdr notification (`blocked`, urgent).
pub fn toasts_of(entries: &[FeedEntry], urgent_only: bool) -> Vec<&FeedEntry> {
	entries
		.iter()
		.filter(|entry| {
			entry.text("type") == Some("toast")
				&& (!urgent_only || entry.text("reason") == Some("blocked") || entry.text("priority") == Some("urgent"))
		})
		.collect()
}

/// A recording of the service's feed: [`FeedRecording::stop`] ends it, [`FeedRecording::closed`] waits for the socket
/// to close.
pub struct FeedRecording {
	stop: Arc<Notify>,
	task: JoinHandle<()>,
}

impl FeedRecording {
	/// A handle that ends the recording from elsewhere (a Ctrl+C handler) while [`FeedRecording::closed`] waits.
	pub fn stopper(&self) -> Arc<Notify> {
		self.stop.clone()
	}

	/// Resolves once the socket has closed, whoever closed it.
	pub async fn closed(self) {
		let _ = self.task.await;
	}
}

fn append(file: &Path, message: Value) {
	let line = format!("{}\n", json_text(&json!({ "at": now_ms(), "message": message }), false));
	if let Ok(mut handle) = fs::OpenOptions::new().create(true).append(true).open(file) {
		let _ = handle.write_all(line.as_bytes());
	}
}

/// Records the running service's feed into `file` (JSON lines) until the returned recording is stopped. The service is
/// the one `<data>/jocasta-service.json` names (`JOCASTA_DATA_DIR` picks a sandbox's).
pub async fn record_feed(file: &Path) -> Result<FeedRecording> {
	let running = read_service_file()
		.ok_or_else(|| Error::other("no running service: <data>/jocasta-service.json is missing"))?;
	if let Some(dir) = file.parent() {
		fs::create_dir_all(dir)?;
	}
	let refused = || Error::other("the service's socket refused the connection");
	let mut request =
		format!("ws://127.0.0.1:{}{WS_PATH}", running.port).into_client_request().map_err(|_| refused())?;
	let bearer = http::HeaderValue::from_str(&format!("Bearer {}", running.token)).map_err(|_| refused())?;
	request.headers_mut().insert("authorization", bearer);
	let (mut socket, _) = tokio_tungstenite::connect_async(request).await.map_err(|_| refused())?;
	let stop = Arc::new(Notify::new());
	let wait = stop.clone();
	let file = file.to_path_buf();
	let record = move |message: Message| {
		if let Message::Text(text) = message
			&& let Ok(message) = serde_json::from_str::<Value>(text.as_str())
		{
			append(&file, message);
		}
	};
	let task = tokio::spawn(async move {
		loop {
			tokio::select! {
				() = wait.notified() => {
					// Messages that arrive before the close handshake ends are still recorded.
					let _ = socket.close(None).await;
					while let Ok(Some(Ok(message))) = tokio::time::timeout(CLOSE_GRACE, socket.next()).await {
						record(message);
					}
					break;
				}
				next = socket.next() => match next {
					Some(Ok(message)) => record(message),
					_ => break,
				},
			}
		}
	});
	Ok(FeedRecording { stop, task })
}
