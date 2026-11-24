//! Tests of `gmail.rs` against an in-process IMAP server: nothing reaches Gmail.

use super::*;
use crate::shared::pollers::source_fakes::{FakeMail, serve_imap};
use crate::shared::private::Private;

fn mails(n: u32) -> Vec<FakeMail> {
	(1..=n)
		.map(|i| FakeMail {
			uid: i * 10,
			msgid: 0x1a2b_3c00 + u64::from(i),
			from: if i == 1 {
				"=?UTF-8?B?SmFuZSBEb2U=?= <jane@example.org>"
			} else {
				"\"Acme Billing\" <billing@example.org>"
			},
			subject: if i == 1 { "=?UTF-8?Q?Caf=C3=A9_menu?=" } else { "Invoice 42 is due" },
		})
		.collect()
}

#[tokio::test]
async fn unread_mail_is_one_private_event_each_newest_first_with_fixed_words_in_the_public_fields() {
	let address = serve_imap(mails(3), "app-password-1").await;
	let round = poll_gmail(&Endpoint::plain(address), "someone@example.org", "app-password-1").await.unwrap();
	let ids: Vec<&str> = round.events.iter().map(|event| event.source_id.as_str()).collect();
	assert_eq!(ids, ["mail:30", "mail:20", "mail:10"]);
	for event in &round.events {
		assert_eq!(
			(event.source, event.kind.as_str(), event.text.as_str(), event.url.as_deref()),
			(EventSource::Mail, "mail", "unread mail", None)
		);
	}
	// The real line and link are private: compared as values, never read.
	let first = &round.events[2];
	assert_eq!(first.private_text, Some(Private::from("Jane Doe: Café menu".to_owned())));
	assert_eq!(first.private_url, Some(Private::from("https://mail.google.com/mail/u/0/#inbox/1a2b3c01".to_owned())));
	assert_eq!(round.events[0].private_text, Some(Private::from("Acme Billing: Invoice 42 is due".to_owned())));
	assert!(round.failures.is_empty());
}

#[tokio::test]
async fn only_the_newest_twenty_are_listed() {
	let address = serve_imap(mails(25), "app-password-1").await;
	let round = poll_gmail(&Endpoint::plain(address), "someone@example.org", "app-password-1").await.unwrap();
	assert_eq!(round.events.len(), 20);
	assert_eq!(round.events[0].source_id, "mail:250");
	assert_eq!(round.events[19].source_id, "mail:60");
}

#[tokio::test]
async fn a_refused_login_is_the_sign_in_line_and_says_nothing_else() {
	let address = serve_imap(mails(1), "app-password-1").await;
	let error = poll_gmail(&Endpoint::plain(address), "someone@example.org", "wrong").await.err().unwrap();
	assert_eq!(error.to_string(), GMAIL_SIGN_IN);
}

#[tokio::test]
async fn an_unreachable_server_is_one_fixed_line() {
	let error = poll_gmail(&Endpoint::plain("127.0.0.1:1"), "someone@example.org", "x").await.err().unwrap();
	assert_eq!(error.to_string(), "mail: couldn't read the inbox");
}
