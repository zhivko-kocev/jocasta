//! In-process stand-ins for the three private sources' servers (0680), so no test reaches a network: a plain-TCP IMAP
//! server that knows `LOGIN`, `EXAMINE`, `UID SEARCH UNSEEN`, `UID FETCH` and `LOGOUT`, and an HTTP server answering one
//! fixed reply and recording each request it got.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// One unseen mail the fake IMAP server holds.
#[derive(Clone)]
pub(crate) struct FakeMail {
	pub uid: u32,
	pub msgid: u64,
	pub from: &'static str,
	pub subject: &'static str,
}

/// An IMAP server on a free port holding `mails` as unseen, accepting only `password`; returns its `host:port`.
pub(crate) async fn serve_imap(mails: Vec<FakeMail>, password: &'static str) -> String {
	let listener = TcpListener::bind("127.0.0.1:0").await.expect("listening");
	let address = listener.local_addr().expect("its address").to_string();
	tokio::spawn(async move {
		while let Ok((stream, _)) = listener.accept().await {
			let mails = mails.clone();
			tokio::spawn(async move {
				let (read, mut write) = stream.into_split();
				let mut lines = BufReader::new(read).lines();
				let _ = write.write_all(b"* OK IMAP ready\r\n").await;
				while let Ok(Some(line)) = lines.next_line().await {
					let mut words = line.splitn(2, ' ');
					let (tag, command) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
					let reply = imap_reply(tag, command, &mails, password);
					let _ = write.write_all(reply.as_bytes()).await;
					if command.starts_with("LOGOUT") {
						break;
					}
				}
			});
		}
	});
	address
}

fn imap_reply(tag: &str, command: &str, mails: &[FakeMail], password: &str) -> String {
	if command.starts_with("LOGIN") {
		return if command.contains(&format!("\"{password}\"")) {
			format!("{tag} OK LOGIN completed\r\n")
		} else {
			format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n")
		};
	}
	if command.starts_with("EXAMINE") {
		return format!(
			"* FLAGS (\\Seen)\r\n* {} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY 1] ok\r\n{tag} OK [READ-ONLY] EXAMINE done\r\n",
			mails.len()
		);
	}
	if command.starts_with("UID SEARCH") {
		let uids: Vec<String> = mails.iter().map(|mail| mail.uid.to_string()).collect();
		return format!("* SEARCH {}\r\n{tag} OK SEARCH done\r\n", uids.join(" "));
	}
	if let Some(rest) = command.strip_prefix("UID FETCH ") {
		let wanted: Vec<u32> =
			rest.split(' ').next().unwrap_or("").split(',').filter_map(|uid| uid.parse().ok()).collect();
		let mut out = String::new();
		for (seq, mail) in mails.iter().enumerate().filter(|(_, mail)| wanted.contains(&mail.uid)) {
			let header = format!(
				"From: {}\r\nSubject: {}\r\nDate: Mon, 24 Nov 2026 09:00:00 +0000\r\n\r\n",
				mail.from, mail.subject
			);
			out.push_str(&format!(
				"* {} FETCH (UID {} X-GM-MSGID {} BODY[HEADER.FIELDS (FROM SUBJECT DATE)] {{{}}}\r\n{header})\r\n",
				seq + 1,
				mail.uid,
				mail.msgid,
				header.len()
			));
		}
		return format!("{out}{tag} OK FETCH done\r\n");
	}
	if command.starts_with("LOGOUT") {
		return format!("* BYE bye\r\n{tag} OK LOGOUT completed\r\n");
	}
	format!("{tag} BAD unknown command\r\n")
}

/// An HTTP server on a free port answering every request with `status` and `body`; returns its base URL and the
/// requests' heads (request line and headers) as they came.
pub(crate) async fn serve_http(status: u16, body: String) -> (String, Arc<Mutex<Vec<String>>>) {
	let listener = TcpListener::bind("127.0.0.1:0").await.expect("listening");
	let base = format!("http://{}", listener.local_addr().expect("its address"));
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
			seen.lock().unwrap().push(String::from_utf8_lossy(&head).into_owned());
			let reply = format!(
				"HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
				body.len()
			);
			let _ = stream.write_all(reply.as_bytes()).await;
			let _ = stream.shutdown().await;
		}
	});
	(base, asked)
}
