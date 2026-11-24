//! Tests of `jira.rs` against an in-process HTTP server: nothing reaches Atlassian.

use serde_json::json;

use super::*;
use crate::shared::pollers::source_fakes::serve_http;
use crate::shared::private::Private;

fn issues() -> String {
	json!({ "issues": [
		{ "key": "ACME-12", "fields": { "summary": "Rotate the\n signing keys", "status": { "name": "In Progress" } } },
		{ "key": "ACME-7", "fields": { "summary": "Fix the login page", "status": { "name": "To Do" } } },
		{ "key": "bad key", "fields": { "summary": "never listed" } },
	] })
	.to_string()
}

#[tokio::test]
async fn open_issues_are_private_events_with_fixed_words_and_a_basic_auth_request() {
	let (site, asked) = serve_http(200, issues()).await;
	let round = poll_jira(&format!("{site}/"), "someone@example.org", "jira-token-value").await.unwrap();
	let ids: Vec<&str> = round.events.iter().map(|event| event.source_id.as_str()).collect();
	assert_eq!(ids, ["jira:ACME-12", "jira:ACME-7"]);
	for event in &round.events {
		assert_eq!(
			(event.source, event.kind.as_str(), event.text.as_str(), event.url.as_deref()),
			(EventSource::Jira, "issue", "open issue", None)
		);
	}
	assert_eq!(
		round.events[0].private_text,
		Some(Private::from("ACME-12 Rotate the signing keys · In Progress".to_owned()))
	);
	assert_eq!(round.events[0].private_url, Some(Private::from(format!("{site}/browse/ACME-12"))));
	let head = asked.lock().unwrap()[0].clone();
	assert!(head.starts_with("GET /rest/api/3/search/jql?jql=assignee%20%3D%20currentUser()"), "{head}");
	assert!(head.contains("maxResults=30"), "{head}");
	// Basic auth of `someone@example.org:jira-token-value`.
	assert!(head.to_lowercase().contains("authorization: basic "), "{head}");
	assert!(head.contains("c29tZW9uZUBleGFtcGxlLm9yZzpqaXJhLXRva2VuLXZhbHVl"), "{head}");
}

#[tokio::test]
async fn a_refusal_and_other_failures_name_the_status_only() {
	let (site, _) = serve_http(401, "{\"errorMessages\":[\"ACME secret detail\"]}".to_owned()).await;
	assert_eq!(poll_jira(&site, "someone@example.org", "x").await.err().unwrap().to_string(), JIRA_SIGN_IN);
	let (site, _) = serve_http(500, "ACME secret detail".to_owned()).await;
	assert_eq!(poll_jira(&site, "someone@example.org", "x").await.err().unwrap().to_string(), "jira: answered 500");
	let (site, _) = serve_http(200, "ACME secret detail".to_owned()).await;
	assert_eq!(
		poll_jira(&site, "someone@example.org", "x").await.err().unwrap().to_string(),
		"jira: the answer isn't an issue list"
	);
}
