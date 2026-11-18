//! The news poller (the Rust port of `news.ts`): brutalist.report's headlines for the dashboard (0429, 0442).

use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use regex::Regex;
use serde_json::Value;

use crate::frontier::protocol::{Headline, NewsList, news_heading};
use crate::shared::errors::{Error, Result};
use crate::shared::pollers::settings::NewsFeed;

/// brutalist.report's headline endpoint (0429). Undocumented: it answers a JSON array of items with `title`, `url`,
/// `source`, `published_at` and much more (article text, keywords, sentiment), of which only title, source, time and
/// link are kept: the dashboard shows headlines, never republishes articles.
pub const NEWS_ENDPOINT: &str = "https://brutalist.report/api/v1/headlines";

static HTTP_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^https?://").expect("the http(s) pattern"));

/// JavaScript's `\s`, which `String.prototype.trim` shares.
fn is_js_space(c: char) -> bool {
	matches!(
		c,
		'\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
			..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
	)
}

/// `text` as `Date.parse` reads the ISO forms an item carries, epoch ms: a date alone is UTC, a time without an offset
/// is local.
fn parse_time(text: &str) -> Option<i64> {
	if let Ok(at) = DateTime::parse_from_rfc3339(text) {
		return Some(at.timestamp_millis());
	}
	for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f"] {
		if let Ok(local) = NaiveDateTime::parse_from_str(text, format) {
			return Local.from_local_datetime(&local).earliest().map(|at| at.timestamp_millis());
		}
	}
	NaiveDate::parse_from_str(text, "%Y-%m-%d")
		.ok()
		.map(|day| day.and_time(chrono::NaiveTime::MIN).and_utc().timestamp_millis())
}

/// One item as a headline; its `at` is empty when the item has no time.
fn headline(value: &Value) -> Option<Headline> {
	let item = value.as_object()?;
	let text = |key: &str| item.get(key).and_then(Value::as_str);
	let title = text("title").map_or_else(String::new, |title| {
		title.split(is_js_space).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
	});
	let url = text("url").map_or("", |url| url.trim_matches(is_js_space));
	if title.is_empty() || !HTTP_URL.is_match(url) {
		return None;
	}
	let at = ["published_at", "inserted_at"].iter().filter_map(|key| text(key)).find_map(parse_time);
	let source = text("source").map(|source| source.trim_matches(is_js_space)).filter(|source| !source.is_empty());
	Some(Headline {
		title,
		source: source.unwrap_or("unknown").to_owned(),
		url: url.to_owned(),
		at: at
			.and_then(|ms| Utc.timestamp_millis_opt(ms).single())
			.map_or_else(String::new, |at| at.to_rfc3339_opts(SecondsFormat::Millis, true)),
	})
}

/// The headlines of one answer, newest first; fails when it isn't a list.
pub fn headlines_in(body: &Value) -> Result<Vec<Headline>> {
	let Some(items) = body.as_array() else {
		return Err(Error::other("brutalist.report's answer isn't a headline list"));
	};
	let mut found: Vec<Headline> = items.iter().filter_map(headline).collect();
	found.sort_by(|a, b| b.at.cmp(&a.at));
	Ok(found)
}

/// `encodeURIComponent`.
fn encode_component(text: &str) -> String {
	let mut out = String::new();
	for byte in text.bytes() {
		if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
			out.push(char::from(byte));
		} else {
			out.push_str(&format!("%{byte:02X}"));
		}
	}
	out
}

/// One round (0442): one request per feed, in order (`?source=hn`, `?topic=gaming&limit=10`), each newest first and cut
/// to its `limit` when it has one; without one brutalist.report answers every item it holds. A failure names the feed.
pub async fn poll_news(feeds: &[NewsFeed]) -> Result<Vec<NewsList>> {
	poll_news_at(feeds, NEWS_ENDPOINT).await
}

/// What a request for one feed failed on: the status it answered, or that it couldn't be reached.
enum Failure {
	Status(u16),
	Unreachable,
}

/// The body one request answers.
async fn fetch(url: &str) -> std::result::Result<String, Failure> {
	let client =
		reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|_| Failure::Unreachable)?;
	let response =
		client.get(url).header("accept", "application/json").send().await.map_err(|_| Failure::Unreachable)?;
	if !response.status().is_success() {
		return Err(Failure::Status(response.status().as_u16()));
	}
	response.text().await.map_err(|_| Failure::Unreachable)
}

/// [`poll_news`] asking `endpoint` instead of brutalist.report's (the tests' in-process server).
pub async fn poll_news_at(feeds: &[NewsFeed], endpoint: &str) -> Result<Vec<NewsList>> {
	let mut lists = Vec::new();
	for feed in feeds {
		let (key, name, limit) = match feed {
			NewsFeed::Source { source, limit } => ("source", source, limit),
			NewsFeed::Topic { topic, limit } => ("topic", topic, limit),
		};
		let heading = news_heading(name);
		let limit_query = limit.map_or_else(String::new, |limit| format!("&limit={limit}"));
		let url = format!("{endpoint}?{key}={}{limit_query}", encode_component(name));
		let text = fetch(&url).await.map_err(|failure| {
			let reason = match failure {
				Failure::Status(status) => format!("brutalist.report answered HTTP {status}"),
				Failure::Unreachable => "brutalist.report couldn't be reached".to_owned(),
			};
			Error::other(format!("News ({heading}): {reason}"))
		})?;
		let not_a_list = || Error::other(format!("News ({heading}): brutalist.report's answer isn't a headline list"));
		let body: Value = serde_json::from_str(&text).map_err(|_| not_a_list())?;
		let mut items = headlines_in(&body).map_err(|_| not_a_list())?;
		if let Some(limit) = limit {
			items.truncate(*limit as usize);
		}
		lists.push(NewsList { heading, items });
	}
	Ok(lists)
}

#[cfg(test)]
#[path = "news_tests.rs"]
mod tests;
