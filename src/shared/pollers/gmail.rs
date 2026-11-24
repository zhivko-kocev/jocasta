//! The Gmail poller (0680): his unread mail, read-only over IMAP. `LOGIN`, `EXAMINE INBOX` (nothing is marked seen),
//! `UID SEARCH UNSEEN`, the newest 20, `UID FETCH … (X-GM-MSGID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])`, `LOGOUT`.
//! Private (0681): `text` is the fixed words `unread mail`; the sender, subject and link go in `private_text` and
//! `private_url`. Failures say nothing of the mailbox.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_imap::Client;
use async_imap::error::Error as ImapError;
use futures_util::TryStreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::shared::errors::{Error, Result};
use crate::shared::events::queue::{EventSource, NewEvent, PollRound};
use crate::shared::pollers::mail_header::{header_value, sender_name};
use crate::shared::pollers::text::one_line;

/// The kinds this poller owns, for the queue's sync.
pub const GMAIL_KINDS: [&str; 1] = ["mail"];

/// What `text` says of every mail (0681): the mail itself is private.
pub const GMAIL_WORDS: &str = "unread mail";

/// The refusal line the `today` section shows when Gmail rejects the address and app password.
pub const GMAIL_SIGN_IN: &str = "mail: sign-in refused; run jocasta setup";

const HOST: &str = "imap.gmail.com";
const PORT: u16 = 993;
/// The newest this many unread mails are listed.
const NEWEST: usize = 20;
const TIMEOUT: Duration = Duration::from_secs(45);

fn unreachable() -> Error {
	Error::other("mail: couldn't read the inbox")
}

/// Where the round connects: Gmail over TLS, or a plain `host:port` (the tests' fake server).
#[derive(Debug, Clone, Default)]
pub struct Endpoint(Option<String>);

impl Endpoint {
	/// A plain-TCP server instead of Gmail (the tests' in-process one).
	pub fn plain(address: impl Into<String>) -> Self {
		Self(Some(address.into()))
	}
}

/// One round: the newest 20 unread mails of the INBOX of `address`, signed in with `password`.
pub async fn poll_gmail(endpoint: &Endpoint, address: &str, password: &str) -> Result<PollRound> {
	let round = async {
		match &endpoint.0 {
			Some(plain) => {
				let stream = TcpStream::connect(plain.as_str()).await.map_err(|_| unreachable())?;
				poll_over(stream, address, password).await
			}
			None => {
				let tcp = TcpStream::connect((HOST, PORT)).await.map_err(|_| unreachable())?;
				let name = ServerName::try_from(HOST).map_err(|_| unreachable())?;
				let tls = connector()?.connect(name, tcp).await.map_err(|_| unreachable())?;
				poll_over(tls, address, password).await
			}
		}
	};
	tokio::time::timeout(TIMEOUT, round).await.map_err(|_| unreachable())?
}

/// TLS with the platform's certificate verifier, as the rest of the build's HTTPS has.
fn connector() -> Result<TlsConnector> {
	use rustls_platform_verifier::ConfigVerifierExt;
	let config = ClientConfig::with_platform_verifier().map_err(|_| unreachable())?;
	Ok(TlsConnector::from(Arc::new(config)))
}

/// The session on an open connection.
async fn poll_over<S>(stream: S, address: &str, password: &str) -> Result<PollRound>
where
	S: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
	let mut session = match Client::new(stream).login(address, password).await {
		Ok(session) => session,
		Err((ImapError::No(_) | ImapError::Bad(_), _)) => return Err(Error::other(GMAIL_SIGN_IN)),
		Err(_) => return Err(unreachable()),
	};
	let events = unread(&mut session).await;
	let _ = session.logout().await;
	Ok(PollRound { events: events?, observed: Box::new(|_| true), failures: Vec::new() })
}

async fn unread<S>(session: &mut async_imap::Session<S>) -> Result<Vec<NewEvent>>
where
	S: AsyncRead + AsyncWrite + Unpin + fmt::Debug + Send,
{
	session.examine("INBOX").await.map_err(|_| unreachable())?;
	let mut uids: Vec<u32> = session.uid_search("UNSEEN").await.map_err(|_| unreachable())?.into_iter().collect();
	uids.sort_unstable_by(|a, b| b.cmp(a));
	uids.truncate(NEWEST);
	if uids.is_empty() {
		return Ok(Vec::new());
	}
	let set = uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
	let fetched: Vec<_> = session
		.uid_fetch(set, "(X-GM-MSGID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])")
		.await
		.map_err(|_| unreachable())?
		.try_collect()
		.await
		.map_err(|_| unreachable())?;
	let mut mails: Vec<(u32, NewEvent)> = fetched
		.iter()
		.filter_map(|fetch| {
			let uid = fetch.uid?;
			let block = String::from_utf8_lossy(fetch.header()?).into_owned();
			let from = one_line(&sender_name(&header_value(&block, "from").unwrap_or_default()), 60);
			let subject = one_line(&header_value(&block, "subject").unwrap_or_default(), 160);
			let subject = if subject.is_empty() { "(no subject)".to_owned() } else { subject };
			let url = match fetch.gmail_msg_id() {
				Some(id) => format!("https://mail.google.com/mail/u/0/#inbox/{id:x}"),
				None => "https://mail.google.com/mail/u/0/#inbox".to_owned(),
			};
			let line = format!("{from}: {subject}");
			Some((uid, NewEvent::private(format!("mail:{uid}"), EventSource::Mail, "mail", GMAIL_WORDS, (line, url))))
		})
		.collect();
	mails.sort_by_key(|(uid, _)| std::cmp::Reverse(*uid));
	let mails = mails.into_iter().map(|(_, event)| event).collect();
	Ok(mails)
}

#[cfg(test)]
#[path = "gmail_tests.rs"]
mod tests;
