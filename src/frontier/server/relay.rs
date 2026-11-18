//! Jocasta's local collab relay (0536, 0561): every omp pane Jocasta starts hosts its session into it
//! (`collab.autoStart: control`), so each omp publishes itself in omp's local host registry, where the service reads
//! whether it works or asks (`hosts.rs`). Content-blind, 127.0.0.1 only, one fixed port; it stops with Jocasta (0549).
//! The relay contract is omp 18.20.2's (`src/collab/relay-client.ts`, `pi-wire` `src/index.ts:414-440,488-490`, the
//! spike's `relay.ts`): `GET /r/<roomId>?role=host|guest` upgrades to a WebSocket; binary frames are
//! `[4-byte big-endian peer id][sealed payload]`, a host's peer id 0 meaning every guest; a guest's frames reach the
//! host with the guest's id; the host hears `{"t":"peer-joined"|"peer-left","peer":N}`, a guest `{"t":"room-closed"}`;
//! close codes 4001 room closed, 4004 no such room, 4009 a second host. Pings are WebSocket pings, which the
//! WebSocket layer answers itself.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, run_program};

/// The relay's port on 127.0.0.1 (0561): fixed, so every pane's overlay names it.
pub const RELAY_PORT: u16 = 47_499;

/// The relay's URL for omp's `collab.relayUrl`.
pub fn relay_url(port: u16) -> String {
	format!("ws://127.0.0.1:{port}")
}

/// omp's `share.serverUrl` for Jocasta's panes (0561): `/share` uploads to the relay's port, which serves nothing but
/// rooms, so a share never leaves this machine.
pub fn share_url(port: u16) -> String {
	format!("http://127.0.0.1:{port}/s")
}

/// A string as JSON writes it (`JSON.stringify`).
fn quoted(text: &str) -> String {
	Value::String(text.to_owned()).to_string()
}

/// The overlay lines every Jocasta omp pane carries (0536, 0561), as YAML.
pub fn relay_overlay(port: u16) -> Vec<String> {
	vec![
		"collab:".to_owned(),
		"  autoStart: control".to_owned(),
		format!("  relayUrl: {}", quoted(&relay_url(port))),
		"share:".to_owned(),
		format!("  serverUrl: {}", quoted(&share_url(port))),
	]
}

/// Whether a host name is `localhost`, a 127.x address or `::1` (as a URL spells it, `[::1]`).
fn is_loopback_host(host: &str) -> bool {
	let host = host.to_lowercase();
	if host == "localhost" || host == "[::1]" {
		return true;
	}
	let parts: Vec<&str> = host.split('.').collect();
	parts.len() == 4
		&& parts[0] == "127"
		&& parts[1..].iter().all(|part| (1..=3).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether omp would dial `value` on this machine only: a `ws://` or `wss://` URL to `localhost`, a 127.x address or
/// `::1`. omp reads a value without a scheme as `wss://<value>` (`collab/controller.ts` `#resolveRelayUrl`).
pub fn is_loopback_relay(value: &str) -> bool {
	let text = value.trim();
	if text.is_empty() {
		return false;
	}
	let spelled = if text.contains("://") { text.to_owned() } else { format!("wss://{text}") };
	let Ok(url) = url::Url::parse(&spelled) else { return false };
	matches!(url.scheme(), "ws" | "wss") && url.host_str().is_some_and(is_loopback_host)
}

/// The `--config` files of an omp argv, in order.
pub fn config_files(command: &[String]) -> Vec<String> {
	let mut files = Vec::new();
	for (i, arg) in command.iter().enumerate() {
		if arg == "--config" {
			if let Some(next) = command.get(i + 1) {
				files.push(next.clone());
			}
		} else if let Some(file) = arg.strip_prefix("--config=") {
			files.push(file.to_owned());
		}
	}
	files
}

/// How long `omp config get` may take before the start is refused (it takes about 0.7 s).
const RESOLVE_TIMEOUT_MS: i64 = 20_000;

/// The separator of a path list (`path.delimiter`).
const DELIMITER: char = if cfg!(windows) { ';' } else { ':' };

/// `collab.relayUrl` as the omp of `command` would resolve it in `cwd` with `env` (0561): omp's own
/// `omp config get collab.relayUrl --json` in that folder, with the pane's `--config` files as `PI_CONFIG_FILES` after
/// any the environment names. omp loads `PI_CONFIG_FILES` and then `--config` into the same overlay list
/// (`config/settings.ts`, `main.ts` `Settings.init`), so the value is the one the pane gets: his config, the project's
/// own `.omp/config.yml` and every overlay, as omp merges them. Asked of omp, not merged here, since a merge of our own
/// would miss whatever omp adds to its layers. `omp` is omp's own argv prefix; `env` adds to the process's environment.
pub async fn omp_relay_url(
	omp: &[String],
	command: &[String],
	cwd: &Path,
	env: Option<&HashMap<String, String>>,
) -> Result<String> {
	let mut merged: HashMap<String, String> = env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect();
	merged.extend(env.into_iter().flatten().map(|(name, value)| (name.clone(), value.clone())));
	let mut files: Vec<String> = merged
		.get("PI_CONFIG_FILES")
		.map(|list| list.split(DELIMITER).filter(|file| !file.is_empty()).map(str::to_owned).collect())
		.unwrap_or_default();
	files.extend(config_files(command));
	if omp.is_empty() {
		return Err(Error::other("no omp command"));
	}
	merged.insert("PI_CONFIG_FILES".to_owned(), files.join(&DELIMITER.to_string()));
	let argv: Vec<&str> =
		omp.iter().map(String::as_str).chain(["config", "get", "collab.relayUrl", "--json"]).collect();
	let run = run_program(
		&argv,
		RunOptions {
			cwd: Some(cwd.to_path_buf()),
			env: Some(merged),
			timeout_ms: Some(RESOLVE_TIMEOUT_MS),
			..RunOptions::default()
		},
	)
	.await?;
	let out = run.stdout;
	if run.code != Some(0) {
		let code = run.code.map_or_else(|| "null".to_owned(), |code| code.to_string());
		let said = if run.stderr.trim().is_empty() { out.trim() } else { run.stderr.trim() };
		return Err(Error::other(format!("`omp config get collab.relayUrl` failed (exit {code}): {said}")));
	}
	let answer: Option<Map<String, Value>> = match serde_json::from_str::<Value>(&out) {
		Ok(Value::Object(object)) => Some(object),
		_ => None,
	};
	let Some(answer) = answer else {
		let head: String = out.trim().chars().take(200).collect();
		return Err(Error::other(format!("`omp config get collab.relayUrl --json` answered no JSON object: {head}")));
	};
	Ok(answer.get("value").and_then(Value::as_str).unwrap_or_default().to_owned())
}

/// The refusal of a pane whose relay isn't loopback (0536, 0561).
pub fn not_loopback(cwd: &Path, value: &str, files: &[String]) -> Error {
	let resolved = if value.is_empty() { "nothing".to_owned() } else { format!("\"{value}\"") };
	let files = if files.is_empty() { "none".to_owned() } else { files.join(", ") };
	Error::user(format!(
		"omp wasn't started in {}: its collab.relayUrl resolves to {resolved}, not Jocasta's relay on 127.0.0.1, so it would host its session elsewhere (0561). Its --config files: {files}.",
		cwd.display()
	))
}

/// The relay's bounds (SEC-NET-7): Jocasta's panes host one room each and it has no guests of its own, so these leave
/// room for his own `/join` while a web page can't grow the service's memory. omp keeps a frame's payload under 1 MB
/// plus its envelope (`collab/replication-shrink.ts` `MAX_REPLICATED_PAYLOAD_BYTES`).
const MAX_ROOMS: usize = 64;
const MAX_GUESTS: usize = 4;
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// What the relay says to one socket.
enum Out {
	Text(String),
	Binary(Bytes),
	Close(u16, &'static str),
}

/// One connected socket: its id, to tell a host from a second one, and where its messages go.
struct Peer {
	conn: u64,
	tx: mpsc::UnboundedSender<Out>,
}

impl Peer {
	fn send(&self, out: Out) {
		let _ = self.tx.send(out);
	}
}

struct Room {
	host: Peer,
	guests: BTreeMap<u32, Peer>,
	next_peer: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
	Host,
	Guest,
}

#[derive(Default)]
struct Shared {
	rooms: Mutex<HashMap<String, Room>>,
	conns: AtomicU64,
}

/// Which socket a frame or a close is about.
#[derive(Clone, Copy)]
struct Who<'a> {
	room: &'a str,
	role: Role,
	conn: u64,
	peer: u32,
}

impl Shared {
	fn rooms(&self) -> MutexGuard<'_, HashMap<String, Room>> {
		self.rooms.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	/// Whether the room table has room for this role's socket.
	fn has_room_for(&self, room: &str, role: Role) -> bool {
		let rooms = self.rooms();
		let open = rooms.get(room);
		match role {
			Role::Host => open.is_some() || rooms.len() < MAX_ROOMS,
			Role::Guest => open.map_or(0, |open| open.guests.len()) < MAX_GUESTS,
		}
	}

	/// A socket opened: a host takes its room, a guest joins it. `Err` is the close it gets instead.
	fn open(&self, room_id: &str, role: Role, peer: Peer) -> std::result::Result<u32, (u16, &'static str)> {
		let mut rooms = self.rooms();
		match (role, rooms.get_mut(room_id)) {
			(Role::Host, Some(_)) => Err((4009, "host already connected")),
			(Role::Host, None) => {
				rooms.insert(room_id.to_owned(), Room { host: peer, guests: BTreeMap::new(), next_peer: 1 });
				Ok(0)
			}
			(Role::Guest, None) => Err((4004, "no such room")),
			(Role::Guest, Some(room)) => {
				let id = room.next_peer;
				room.next_peer += 1;
				room.guests.insert(id, peer);
				room.host.send(Out::Text(json!({ "t": "peer-joined", "peer": id }).to_string()));
				Ok(id)
			}
		}
	}

	/// One binary frame from a socket: a host's goes to the guest it names (every guest for 0), a guest's to the
	/// host with the guest's id in front.
	fn forward(&self, who: &Who, frame: &Bytes) {
		let Who { room: room_id, role, conn, peer } = *who;
		let rooms = self.rooms();
		let Some(room) = rooms.get(room_id) else { return };
		let Some(head) = frame.first_chunk::<4>() else { return };
		if role == Role::Host {
			if room.host.conn != conn {
				return;
			}
			match u32::from_be_bytes(*head) {
				0 => room.guests.values().for_each(|guest| guest.send(Out::Binary(frame.clone()))),
				target => {
					if let Some(guest) = room.guests.get(&target) {
						guest.send(Out::Binary(frame.clone()));
					}
				}
			}
			return;
		}
		let mut stamped = frame.to_vec();
		stamped[..4].copy_from_slice(&peer.to_be_bytes());
		room.host.send(Out::Binary(Bytes::from(stamped)));
	}

	/// A socket closed: a host's room ends and its guests hear so; a guest's leaving is told to the host.
	fn close(&self, who: &Who) {
		let Who { room: room_id, role, conn, peer } = *who;
		let mut rooms = self.rooms();
		let Some(room) = rooms.get_mut(room_id) else { return };
		if role == Role::Host && room.host.conn == conn {
			if let Some(room) = rooms.remove(room_id) {
				for guest in room.guests.values() {
					guest.send(Out::Text(json!({ "t": "room-closed" }).to_string()));
					guest.send(Out::Close(4001, "room closed"));
				}
			}
		} else if role == Role::Guest && room.guests.remove(&peer).is_some() {
			room.host.send(Out::Text(json!({ "t": "peer-left", "peer": peer }).to_string()));
		}
	}
}

/// A room's path, `/r/<roomId>`: the id is 10–64 of `A-Za-z0-9_-`.
fn room_of(path: &str) -> Option<&str> {
	let id = path.strip_prefix("/r/")?;
	let fits = (10..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
	fits.then_some(id)
}

/// Whether a WebSocket's `Origin` may open a room: none (omp's client sends none) or a loopback page.
fn origin_allowed(origin: Option<&str>) -> bool {
	let Some(origin) = origin else { return true };
	url::Url::parse(origin).ok().is_some_and(|url| url.host_str().is_some_and(is_loopback_host))
}

fn refusal(status: StatusCode, text: &'static str) -> Response {
	(status, text).into_response()
}

async fn handle(
	State(shared): State<Arc<Shared>>,
	uri: Uri,
	headers: HeaderMap,
	upgrade: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
	let room = room_of(uri.path());
	let role = uri
		.query()
		.and_then(|query| url::form_urlencoded::parse(query.as_bytes()).find(|(key, _)| key == "role"))
		.and_then(|(_, value)| match value.as_ref() {
			"host" => Some(Role::Host),
			"guest" => Some(Role::Guest),
			_ => None,
		});
	let (Some(room), Some(role)) = (room, role) else { return refusal(StatusCode::NOT_FOUND, "Not found") };
	// A web page elsewhere can't open rooms here (browsers allow cross-origin WebSockets).
	let origin = headers.get("origin").map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
	if !origin_allowed(origin.as_deref()) {
		return refusal(StatusCode::FORBIDDEN, "Forbidden");
	}
	if !shared.has_room_for(room, role) {
		return refusal(StatusCode::SERVICE_UNAVAILABLE, "Too many");
	}
	let Ok(upgrade) = upgrade else { return refusal(StatusCode::UPGRADE_REQUIRED, "Upgrade required") };
	let room = room.to_owned();
	upgrade
		.max_message_size(MAX_FRAME_BYTES)
		.max_frame_size(MAX_FRAME_BYTES)
		.on_upgrade(move |socket| run_socket(socket, shared, room, role))
}

/// Sends a close frame and gives the other end a moment to answer it.
async fn finish(mut socket: WebSocket, code: u16, reason: &'static str) {
	let frame = CloseFrame { code, reason: reason.into() };
	if socket.send(Message::Close(Some(frame))).await.is_ok() {
		let _ =
			tokio::time::timeout(Duration::from_millis(500), async { while socket.recv().await.is_some() {} }).await;
	}
}

/// One socket's life: opened into its room, its frames forwarded, closed out of it.
async fn run_socket(mut socket: WebSocket, shared: Arc<Shared>, room_id: String, role: Role) {
	let conn = shared.conns.fetch_add(1, Ordering::Relaxed);
	let (tx, mut rx) = mpsc::unbounded_channel();
	let peer = match shared.open(&room_id, role, Peer { conn, tx }) {
		Ok(peer) => peer,
		Err((code, reason)) => return finish(socket, code, reason).await,
	};
	let mut closing = None;
	loop {
		tokio::select! {
			incoming = socket.recv() => match incoming {
				Some(Ok(Message::Binary(frame))) => shared.forward(&Who { room: &room_id, role, conn, peer }, &frame),
				Some(Ok(Message::Close(_)) | Err(_)) | None => break,
				Some(Ok(_)) => {}
			},
			out = rx.recv() => match out {
				Some(Out::Text(text)) => {
					if socket.send(Message::Text(text.into())).await.is_err() {
						break;
					}
				}
				Some(Out::Binary(frame)) => {
					if socket.send(Message::Binary(frame)).await.is_err() {
						break;
					}
				}
				Some(Out::Close(code, reason)) => {
					closing = Some((code, reason));
					break;
				}
				// The relay stopped: every room was dropped.
				None => break,
			},
		}
	}
	shared.close(&Who { room: &room_id, role, conn, peer });
	if let Some((code, reason)) = closing {
		finish(socket, code, reason).await;
	}
}

/// A running relay.
pub struct Relay {
	pub port: u16,
	shared: Arc<Shared>,
	task: Mutex<Option<JoinHandle<()>>>,
}

impl Relay {
	/// Closes every room and stops listening, returning once the listener's task has ended and the port is free; again
	/// is a no-op.
	pub async fn stop(&self) {
		let Some(task) = self.task.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() else { return };
		self.shared.rooms().clear();
		task.abort();
		let _ = task.await;
	}
}

/// Starts the relay on 127.0.0.1:`port` (`Some(0)` picks a free one: tests; `None` is [`RELAY_PORT`]). A port another
/// program holds fails the start with a UserError naming it: Jocasta doesn't run without its relay. Runs on the
/// current tokio runtime.
pub fn start_relay(port: Option<u16>) -> Result<Relay> {
	let port = port.unwrap_or(RELAY_PORT);
	let listener = std::net::TcpListener::bind(("127.0.0.1", port))
		.and_then(|listener| listener.set_nonblocking(true).map(|()| listener))
		.map_err(|error| {
			Error::user(format!(
				"Jocasta's collab relay couldn't listen on 127.0.0.1:{port}: {error}. Another program holds the port; Jocasta needs it for its omp panes (0561)."
			))
		})?;
	let bound = listener.local_addr()?.port();
	let listener = tokio::net::TcpListener::from_std(listener)?;
	let shared = Arc::new(Shared::default());
	let app = Router::new().fallback(handle).with_state(shared.clone());
	let task = tokio::spawn(async move {
		let _ = axum::serve(listener, app).await;
	});
	Ok(Relay { port: bound, shared, task: Mutex::new(Some(task)) })
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;
