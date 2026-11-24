//! Tests of the private sources in `Day` (0680, 0681): mail, Jira and Azure operations come in as rows only `today` can
//! show, and everything the boss, a push or a log could read holds the fixed words and counts.

use std::sync::Arc;

use serde_json::json;

use super::tests::{Stand, at, project, write_settings};
use super::*;
use crate::shared::pollers::cli::tests::fake_run;
use crate::shared::pollers::gmail::GMAIL_SIGN_IN;
use crate::shared::pollers::source_fakes::{FakeMail, serve_http, serve_imap};
use crate::shared::testing::temp_jocasta;

const YAML: &str = "pollers:\n  gmail:\n    address: someone@example.org\n  jira:\n    site: https://example.atlassian.net\n    email: someone@example.org\n  azurePortal: {}\n";

/// What no reader of text may hold: the canaries the three servers answer with.
const CANARIES: [&str; 8] = [
	"Canary Sender",
	"canary-subject-4711",
	"CANARY-9",
	"canary-summary-0815",
	"canary-resource",
	"rg-canary",
	"Canary Operation",
	"canary-app-password",
];

fn mails() -> Vec<FakeMail> {
	(1..=3)
		.map(|i| FakeMail {
			uid: i,
			msgid: 0xabc0 + u64::from(i),
			from: "\"Canary Sender\" <sender@example.org>",
			subject: "canary-subject-4711",
		})
		.collect()
}

fn issues() -> String {
	json!({ "issues": [
		{ "key": "CANARY-9", "fields": { "summary": "canary-summary-0815", "status": { "name": "To Do" } } },
		{ "key": "CANARY-10", "fields": { "summary": "canary-summary-0815", "status": { "name": "Done soon" } } },
	] })
	.to_string()
}

fn stand(seams: SourceSeams, run: impl RunCli + 'static) -> Stand {
	Stand::with(
		at(2026, 11, 24, 9, 0),
		vec![project("p1", "acme-api")],
		DayOptions { pollers: Some(false), run: Some(Arc::new(run)), sources: Some(seams), ..DayOptions::default() },
	)
}

#[tokio::test]
async fn three_unread_mails_two_issues_and_a_failed_operation_are_rows_while_the_boss_gets_counts_and_no_subject() {
	let _j = temp_jocasta();
	write_settings(YAML);
	let imap = serve_imap(mails(), "canary-app-password").await;
	let (site, _) = serve_http(200, issues()).await;
	let resource = "/subscriptions/sub-1/resourceGroups/rg-canary/providers/Microsoft.Web/sites/canary-resource";
	let (run, _) = fake_run(vec![
		("az account list", json!(["sub-1"])),
		(
			"az monitor activity-log list",
			json!([{ "operationName": { "value": "w/write", "localizedValue": "Canary Operation" }, "resourceId": resource }]),
		),
	]);
	let s = stand(
		SourceSeams {
			gmail_secret: Some(Arc::new(|| Some("canary-app-password".to_owned()))),
			jira_secret: Some(Arc::new(|| Some("canary-jira-token".to_owned()))),
			imap_endpoint: Some(imap),
			jira_site: Some(site),
		},
		run,
	);
	let day = &s.day;
	// Before any round no section exists, and the boss's `today` has no line for them.
	let before = day.today().unwrap();
	assert_eq!((&before.mail, &before.jira, &before.azure), (&None, &None, &None));
	assert!(!day.today_text().unwrap().contains("unread"));

	for poller in [PollerName::Gmail, PollerName::Jira, PollerName::AzurePortal] {
		day.poll(poller).await;
	}
	let today = day.today().unwrap();
	assert_eq!(today.mail.as_ref().map(|section| (section.rows.len(), section.error.clone())), Some((3, None)));
	assert_eq!(today.jira.as_ref().map(|section| (section.rows.len(), section.error.clone())), Some((2, None)));
	assert_eq!(today.azure.as_ref().map(|section| (section.rows.len(), section.error.clone())), Some((1, None)));

	// The boss's text: counts only.
	let text = day.today_text().unwrap();
	for line in ["mail: 3 unread", "jira: 2 open", "azure: 1 failed"] {
		assert!(text.lines().any(|l| l == line), "{line} in {text}");
	}
	// Everything else a reader of text could meet: the boss's text, `[event]` lines, toasts, notes, the queue's public
	// columns, and Debug prints of what was sent.
	let rows_text = s.day.queue(|queue| {
		queue
			.open()
			.unwrap()
			.iter()
			.map(|row| format!("{}|{}|{:?}|{:?}|{:?}", row.kind, row.text, row.url, row.private_text, row.private_url))
			.collect::<Vec<_>>()
			.join("\n")
	});
	assert_eq!(rows_text.matches("unread mail|None|").count(), 3);
	assert_eq!(rows_text.matches("open issue|None|").count(), 2);
	assert_eq!(rows_text.matches("failed operation|None|").count(), 1);
	let everything =
		[text, rows_text, s.alerts().join("\n"), s.notes().join("\n"), format!("{:?}", s.sent()), format!("{today:?}")]
			.join("\n");
	for canary in CANARIES {
		assert!(!everything.contains(canary), "{canary} leaked into {everything}");
	}
	// Low priority: nothing was delivered to the boss pane or as a toast.
	assert!(s.alerts().is_empty());
	assert!(s.toasts().is_empty());
}

#[tokio::test]
async fn a_refused_mail_login_is_the_sections_line_and_the_boss_reads_that_line_not_a_count() {
	let _j = temp_jocasta();
	write_settings(YAML);
	let imap = serve_imap(mails(), "right-password").await;
	let (run, _) = fake_run(vec![]);
	let s = stand(
		SourceSeams {
			gmail_secret: Some(Arc::new(|| Some("wrong-password".to_owned()))),
			imap_endpoint: Some(imap),
			..SourceSeams::default()
		},
		run,
	);
	s.day.poll(PollerName::Gmail).await;
	let mail = s.day.today().unwrap().mail.expect("a section once its poller ran");
	assert_eq!((mail.rows.len(), mail.error.as_deref()), (0, Some(GMAIL_SIGN_IN)));
	assert!(s.day.today_text().unwrap().lines().any(|line| line == GMAIL_SIGN_IN));
	assert_eq!(s.notes(), [format!("Poller gmail failed: {GMAIL_SIGN_IN}")]);
}

#[tokio::test]
async fn mail_read_in_gmail_leaves_the_rows_at_the_next_round() {
	let _j = temp_jocasta();
	write_settings(YAML);
	let (run, _) = fake_run(vec![]);
	let seams = |imap: String| SourceSeams {
		gmail_secret: Some(Arc::new(|| Some("canary-app-password".to_owned()))),
		imap_endpoint: Some(imap),
		..SourceSeams::default()
	};
	let three = serve_imap(mails(), "canary-app-password").await;
	let s = stand(seams(three), run);
	s.day.poll(PollerName::Gmail).await;
	assert_eq!(s.day.today().unwrap().mail.unwrap().rows.len(), 3);
	// The same day on a server where two were read: the queue resolves them.
	let (run, _) = fake_run(vec![]);
	let one = serve_imap(mails().into_iter().take(1).collect(), "canary-app-password").await;
	let again = Day::new(
		Arc::clone(&s.catalog),
		Arc::clone(&s.shared) as Arc<dyn DayHooks>,
		DayOptions {
			pollers: Some(false),
			run: Some(Arc::new(run)),
			sources: Some(seams(one)),
			..DayOptions::default()
		},
	)
	.unwrap();
	again.poll(PollerName::Gmail).await;
	assert_eq!(again.today().unwrap().mail.unwrap().rows.len(), 1);
}
