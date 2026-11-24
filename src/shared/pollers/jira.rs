//! The Jira poller (0680): the issues assigned to him that aren't done, read-only through Jira's REST API. Private
//! (0681): `text` is the fixed words `open issue`, the real line and link go in `private_text` and `private_url`.
//! Failures name the status only; nothing of the answer or of the site's address.

use std::time::Duration;

use serde::Deserialize;

use crate::shared::errors::{Error, Result};
use crate::shared::events::queue::{EventSource, NewEvent, PollRound};
use crate::shared::link::web_link;
use crate::shared::pollers::text::one_line;

/// The kinds this poller owns, for the queue's sync.
pub const JIRA_KINDS: [&str; 1] = ["issue"];

/// What `text` says of every issue (0681): the issue itself is private.
pub const JIRA_WORDS: &str = "open issue";

/// The refusal line the `today` section shows when the site rejects the email and token (401, 403).
pub const JIRA_SIGN_IN: &str = "jira: sign-in refused; run jocasta setup";

const SEARCH: &str = "/rest/api/3/search/jql?jql=assignee%20%3D%20currentUser()%20AND%20statusCategory%20!%3D%20Done%20ORDER%20BY%20updated%20DESC&fields=summary,status,priority&maxResults=30";

#[derive(Deserialize)]
struct Search {
	#[serde(default)]
	issues: Vec<Issue>,
}

#[derive(Deserialize)]
struct Issue {
	key: String,
	fields: Option<Fields>,
}

#[derive(Deserialize)]
struct Fields {
	summary: Option<String>,
	status: Option<Status>,
}

#[derive(Deserialize)]
struct Status {
	name: Option<String>,
}

/// The events of one answer: one per issue.
fn events_of(site: &str, search: Search) -> Vec<NewEvent> {
	search
		.issues
		.into_iter()
		.filter(|issue| !issue.key.is_empty() && issue.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
		.map(|issue| {
			let fields = issue.fields.unwrap_or(Fields { summary: None, status: None });
			let summary = one_line(fields.summary.as_deref().unwrap_or(""), 160);
			let status = one_line(fields.status.and_then(|status| status.name).as_deref().unwrap_or(""), 40);
			let line = if status.is_empty() {
				format!("{} {summary}", issue.key)
			} else {
				format!("{} {summary} · {status}", issue.key)
			};
			let url = web_link(Some(&format!("{site}/browse/{}", issue.key))).unwrap_or_default();
			NewEvent::private(format!("jira:{}", issue.key), EventSource::Jira, "issue", JIRA_WORDS, (line, url))
		})
		.collect()
}

/// One round: his open issues, newest update first, at most 30. `site` is the Jira Cloud address
/// (`https://example.atlassian.net`), `email` and `token` the basic-auth pair.
pub async fn poll_jira(site: &str, email: &str, token: &str) -> Result<PollRound> {
	let site = site.trim().trim_end_matches('/');
	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(30))
		.build()
		.map_err(|_| Error::other("jira: unreachable"))?;
	let response = client
		.get(format!("{site}{SEARCH}"))
		.basic_auth(email, Some(token))
		.header("accept", "application/json")
		.send()
		.await
		.map_err(|_| Error::other("jira: unreachable"))?;
	let status = response.status();
	if matches!(status.as_u16(), 401 | 403) {
		return Err(Error::other(JIRA_SIGN_IN));
	}
	if !status.is_success() {
		return Err(Error::other(format!("jira: answered {}", status.as_u16())));
	}
	let body = response.text().await.map_err(|_| Error::other("jira: unreachable"))?;
	let search: Search =
		serde_json::from_str(&body).map_err(|_| Error::other("jira: the answer isn't an issue list"))?;
	Ok(PollRound { events: events_of(site, search), observed: Box::new(|_| true), failures: Vec::new() })
}

#[cfg(test)]
#[path = "jira_tests.rs"]
mod tests;
