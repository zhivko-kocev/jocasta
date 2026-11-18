//! A stand-in for hunk's binary in the service's tests (the Rust twin of `test/frontier/server/fake-hunk.ts`, 0630):
//! `session comment list --repo <path> --json` prints two live notes in hunk 0.23.0's shape (`{comments: [...]}`);
//! `daemon serve` answers `/health` on `HUNK_MCP_PORT` until its process tree is ended; anything else exits 0
//! silently. It is this test executable run again on [`stand_in_hunk`] through a shim named `hunk`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::frontier::server::mise::tests::{captured_shim, stand_in};
use crate::shared::env;

/// The test function the shim runs.
pub(crate) const FAKE_HUNK_TEST: &str = "frontier::server::hunk::fake_hunk::stand_in_hunk";

/// The hunk argv of a fake hunk in `dir`: one word, the path of a shim named `hunk` there.
pub(crate) fn fake_hunk(dir: &Path) -> Vec<String> {
	vec![captured_shim(dir, "hunk", FAKE_HUNK_TEST).to_string_lossy().into_owned()]
}

/// Answers every request on `port` with `200 {"ok":true}`, for as long as the process lives.
fn serve_health(port: u16) {
	let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else { return };
	for stream in listener.incoming().flatten() {
		let mut stream = stream;
		let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
		let _ = stream.read(&mut [0u8; 1024]);
		let body = r#"{"ok":true}"#;
		let reply = format!(
			"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
			body.len()
		);
		let _ = stream.write_all(reply.as_bytes());
	}
}

/// The stand-in side of [`fake_hunk`]: does nothing in a normal test run.
#[test]
fn stand_in_hunk() {
	stand_in(|args| {
		if args == "daemon serve" {
			let port = env::var("HUNK_MCP_PORT").and_then(|port| port.parse().ok()).unwrap_or(0);
			serve_health(port);
		}
		let words: Vec<&str> = args.split(' ').collect();
		if words.starts_with(&["session", "comment", "list"]) && words.contains(&"--json") {
			let note = |line: u32| {
				json!({
					"commentId": format!("mcp:test:{line}"),
					"filePath": "README.md",
					"hunkIndex": 0,
					"side": "new",
					"line": line,
					"summary": format!("Note on line {line}."),
					"author": "reviewer",
				})
			};
			return (format!("{}\n", json!({ "comments": [note(1), note(2)] })), String::new(), 0);
		}
		(String::new(), String::new(), 0)
	});
}

/// Answers every request on a free loopback port `200 {"ok":true}`: Jocasta's hunk session daemon as the service finds
/// it, so none is ever started. Returns the port.
pub(crate) async fn health_daemon() -> u16 {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listening");
	let port = listener.local_addr().expect("the daemon's address").port();
	tokio::spawn(async move {
		while let Ok((mut stream, _)) = listener.accept().await {
			tokio::spawn(async move {
				let _ = stream.read(&mut [0u8; 1024]).await;
				let body = r#"{"ok":true}"#;
				let head =
					format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n", body.len());
				let _ = stream.write_all(format!("{head}connection: close\r\n\r\n{body}").as_bytes()).await;
			});
		}
	});
	port
}
