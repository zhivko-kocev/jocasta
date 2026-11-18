//! Tests of `relay.rs`: the relay's own, its loopback checks and the pane's relay lookup through a stand-in omp.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Bytes, Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use super::*;
use crate::frontier::server::mise::tests::{captured_shim, stand_in};
use crate::shared::env;
use crate::shared::testing::{temp_dir, temp_jocasta};
use crate::shared::yaml::parse_value;

#[test]
fn a_relay_is_loopback_only_on_localhost_127_x_or_1_over_ws_or_wss_a_bare_host_is_wss() {
	for ok in ["ws://127.0.0.1:47499", "wss://localhost", "ws://[::1]:1", "127.0.0.2:9", "WS://LOCALHOST:1"] {
		assert!(is_loopback_relay(ok), "{ok}");
	}
	for no in ["", "wss://my.omp.sh", "my.omp.sh", "http://127.0.0.1:1", "ws://127.0.0.1.example.com", "ws://10.0.0.1"]
	{
		assert!(!is_loopback_relay(no), "{no}");
	}
}

#[test]
fn the_overlays_relay_keys_name_the_relays_port_and_a_loopback_share_server() {
	let parsed = parse_value(&relay_overlay(47_499).join("\n")).unwrap();
	assert_eq!(
		parsed,
		json!({
			"collab": { "autoStart": "control", "relayUrl": "ws://127.0.0.1:47499" },
			"share": { "serverUrl": "http://127.0.0.1:47499/s" },
		})
	);
	let argv = ["omp", "--config", "a.yml", "-p", "--config=b.yml", "--config"].map(str::to_owned);
	assert_eq!(config_files(&argv), ["a.yml", "b.yml"]);
}

/// The stand-in `omp config get collab.relayUrl --json`: its answer names its arguments, `PI_CONFIG_FILES` and folder.
#[test]
fn fake_omp_config() {
	stand_in(|args| {
		let cwd = std::env::current_dir().unwrap_or_default().display().to_string();
		let value = [args, env::var("PI_CONFIG_FILES").unwrap_or_default(), cwd].join("|");
		let out = serde_json::to_string_pretty(&json!({ "key": "collab.relayUrl", "value": value })).unwrap();
		(format!("{out}\n"), String::new(), 0)
	});
}

/// The stand-in omp whose `config get` fails.
#[test]
fn fake_omp_fails() {
	stand_in(|_| (String::new(), "Unknown setting\n".to_owned(), 1));
}

#[tokio::test]
async fn collab_relay_url_is_asked_of_omp_in_the_panes_folder_with_the_panes_config_files_after_the_envs_own() {
	let temp = temp_jocasta();
	let fake = captured_shim(temp.root(), "omp-config", "frontier::server::relay::tests::fake_omp_config");
	let cwd = temp.root().join("pane");
	std::fs::create_dir_all(&cwd).unwrap();
	let command = ["omp", "--config", "g.yml", "--config", "o.yml"].map(str::to_owned);
	let env = HashMap::from([("PI_CONFIG_FILES".to_owned(), "his.yml".to_owned())]);
	let omp = [fake.to_string_lossy().into_owned()];
	let value = omp_relay_url(&omp, &command, &cwd, Some(&env)).await.unwrap();
	let files = ["his.yml", "g.yml", "o.yml"].join(&DELIMITER.to_string());
	assert_eq!(value, ["config get collab.relayUrl --json".to_owned(), files, cwd.display().to_string()].join("|"));
	let failing = captured_shim(temp.root(), "omp-fails", "frontier::server::relay::tests::fake_omp_fails");
	let failed = omp_relay_url(&[failing.to_string_lossy().into_owned()], &[], &cwd, None).await.unwrap_err();
	assert!(failed.to_string().contains("Unknown setting"), "{failed}");
}

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// One open socket: its sink, and everything it heard so far.
struct Socket {
	sink: SplitSink<Client, Message>,
	heard: Arc<Mutex<Vec<Message>>>,
}

impl Socket {
	fn count(&self) -> usize {
		self.heard.lock().unwrap().len()
	}

	fn text(&self, at: usize) -> String {
		match &self.heard.lock().unwrap()[at] {
			Message::Text(text) => text.to_string(),
			other => panic!("not text: {other:?}"),
		}
	}

	fn binary(&self, at: usize) -> Vec<u8> {
		match &self.heard.lock().unwrap()[at] {
			Message::Binary(bytes) => bytes.to_vec(),
			other => panic!("not binary: {other:?}"),
		}
	}
}

fn room_url(port: u16, room: &str, role: &str) -> String {
	format!("ws://127.0.0.1:{port}/r/{room}?role={role}")
}

async fn open(url: &str) -> Socket {
	let (client, _) = connect_async(url).await.expect("opening a socket");
	let (sink, mut stream) = client.split();
	let heard = Arc::new(Mutex::new(Vec::new()));
	let keep = heard.clone();
	tokio::spawn(async move {
		while let Some(Ok(message)) = stream.next().await {
			if matches!(message, Message::Text(_) | Message::Binary(_)) {
				keep.lock().unwrap().push(message);
			}
		}
	});
	Socket { sink, heard }
}

async fn until(check: impl Fn() -> bool) {
	for _ in 0..500 {
		if check() {
			return;
		}
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	panic!("the condition never held");
}

/// The close code a socket the relay refuses ends with.
async fn closed_with(url: &str) -> u16 {
	let (mut client, _) = connect_async(url).await.expect("opening a socket");
	loop {
		match client.next().await {
			Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
			Some(Ok(_)) => {}
			other => panic!("no close frame: {other:?}"),
		}
	}
}

#[tokio::test]
async fn the_relay_forwards_a_rooms_frames_between_its_host_and_guests_by_peer_id_and_says_who_joins_and_leaves() {
	let relay = start_relay(Some(0)).unwrap();
	let port = relay.port;
	let url = |role: &str| room_url(port, "room-0123456789", role);
	let host = open(&url("host")).await;
	let mut guest = open(&url("guest")).await;
	until(|| host.count() == 1).await;
	assert_eq!(host.text(0), json!({ "t": "peer-joined", "peer": 1 }).to_string());
	// A guest's frame reaches the host carrying the guest's id; the host's broadcast (peer 0) reaches the guest.
	guest.sink.send(Message::Binary(Bytes::from_static(&[0, 0, 0, 0, 7, 7]))).await.unwrap();
	until(|| host.count() == 2).await;
	assert_eq!(host.binary(1), [0, 0, 0, 1, 7, 7]);
	let mut host = host;
	host.sink.send(Message::Binary(Bytes::from_static(&[0, 0, 0, 0, 9]))).await.unwrap();
	until(|| guest.count() == 1).await;
	assert_eq!(guest.binary(0), [0, 0, 0, 0, 9]);
	// A second host for the room is refused; a guest of no room too.
	assert_eq!(closed_with(&url("host")).await, 4009);
	assert_eq!(closed_with(&room_url(port, "no-room-000000", "guest")).await, 4004);
	guest.sink.close().await.unwrap();
	until(|| host.count() == 3).await;
	assert_eq!(host.text(2), json!({ "t": "peer-left", "peer": 1 }).to_string());
	assert_eq!(reqwest::get(format!("http://127.0.0.1:{port}/s")).await.unwrap().status().as_u16(), 404);
	host.sink.close().await.unwrap();
	relay.stop().await;
}

/// Whether anything listens on `port`.
async fn listening(port: u16) -> bool {
	TcpStream::connect(("127.0.0.1", port)).await.is_ok()
}

#[tokio::test]
async fn a_relay_port_another_program_holds_fails_the_start_loudly_a_stopped_relay_frees_its_port() {
	let relay = start_relay(Some(0)).unwrap();
	let refused = start_relay(Some(relay.port)).err().expect("the port is held");
	assert!(refused.is_user());
	assert!(refused.to_string().contains(&format!("127.0.0.1:{}", relay.port)), "{refused}");
	relay.stop().await;
	assert!(!listening(relay.port).await);
	relay.stop().await;
	start_relay(Some(relay.port)).unwrap().stop().await;
}

/// The result of opening a socket the relay may refuse: the socket, or the HTTP status it answered instead.
async fn opened(url: &str, origin: Option<&str>) -> std::result::Result<Client, u16> {
	let mut request = url.into_client_request().unwrap();
	if let Some(origin) = origin {
		request.headers_mut().insert("origin", origin.parse().unwrap());
	}
	match connect_async(request).await {
		Ok((client, _)) => Ok(client),
		Err(WsError::Http(response)) => Err(response.status().as_u16()),
		Err(other) => panic!("{other}"),
	}
}

#[tokio::test]
async fn the_relay_refuses_a_web_pages_origin_and_more_guests_than_a_room_takes() {
	let relay = start_relay(Some(0)).unwrap();
	let room = |role: &str| room_url(relay.port, "room-abcdefgh", role);
	assert_eq!(opened(&room("host"), Some("https://example.org")).await.err(), Some(403));
	// omp's client sends no Origin: it hosts and its guests join, up to four.
	let mut sockets = vec![opened(&room("host"), None).await.unwrap()];
	for _ in 0..4 {
		sockets.push(opened(&room("guest"), None).await.unwrap());
	}
	assert_eq!(opened(&room("guest"), None).await.err(), Some(503));
	for mut socket in sockets {
		let _ = socket.close(None).await;
	}
	relay.stop().await;
}

#[test]
fn a_pane_whose_relay_isnt_loopback_is_refused_with_its_value_and_config_files() {
	let _dir = temp_dir("relay-refusal");
	let said = not_loopback(Path::new("C:/wt/a"), "wss://my.omp.sh", &["g.yml".to_owned(), "o.yml".to_owned()]);
	assert_eq!(
		said.to_string(),
		"omp wasn't started in C:/wt/a: its collab.relayUrl resolves to \"wss://my.omp.sh\", not Jocasta's relay on 127.0.0.1, so it would host its session elsewhere (0561). Its --config files: g.yml, o.yml."
	);
	assert!(not_loopback(Path::new("x"), "", &[]).to_string().ends_with("resolves to nothing, not Jocasta's relay on 127.0.0.1, so it would host its session elsewhere (0561). Its --config files: none."));
}
