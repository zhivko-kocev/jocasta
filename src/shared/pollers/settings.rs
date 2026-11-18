//! `pollers` in `jocasta.yaml` (0301, 0387; news 0429, 0442), read for each poller. The Rust port of `settings.ts`.
//! The TypeScript's `readPoller(settings, "github")` is `read_poller::<GithubSettings>(&settings)`: each poller's
//! settings type names its own key.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::shared::settings::{Settings, SettingsError, check_settings};

/// `pollers.github` (0301).
#[derive(Debug, Clone, PartialEq)]
pub struct GithubSettings {
	pub every_minutes: f64,
}

/// `pollers.azureDevOps` (0301).
#[derive(Debug, Clone, PartialEq)]
pub struct AzureDevOpsSettings {
	pub orgs: Vec<String>,
	pub every_minutes: f64,
}

/// `pollers.calendar` (0301).
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarSettings {
	pub every_minutes: f64,
}

/// `pollers.news` (0429, 0442).
#[derive(Debug, Clone, PartialEq)]
pub struct NewsSettings {
	/// brutalist.report feeds, in order (0442).
	pub feeds: Vec<NewsFeed>,
	pub every_minutes: f64,
}

/// One brutalist.report feed (0442): a source (`hn`) or a topic (`gaming`), with at most `limit` headlines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NewsFeed {
	Source {
		source: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		limit: Option<u32>,
	},
	Topic {
		topic: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		limit: Option<u32>,
	},
}

/// 0301's intervals: GitHub 2, Azure DevOps 5, Calendar 10 minutes; 0429's news 15.
const DEFAULT_GITHUB_MINUTES: f64 = 2.0;
const DEFAULT_AZURE_DEVOPS_MINUTES: f64 = 5.0;
const DEFAULT_CALENDAR_MINUTES: f64 = 10.0;
const DEFAULT_NEWS_MINUTES: f64 = 15.0;

/// 0442: every Hacker News and LWN link, the newest 10 gaming headlines.
fn default_feeds() -> Vec<NewsFeed> {
	vec![
		NewsFeed::Source { source: "hn".to_owned(), limit: None },
		NewsFeed::Source { source: "lwn".to_owned(), limit: None },
		NewsFeed::Topic { topic: "gaming".to_owned(), limit: Some(10) },
	]
}

/// One poller's settings, with the key it reads (`pollers.<NAME>`).
pub trait PollerSettings: Sized {
	/// The poller's key under `pollers`.
	const NAME: &'static str;

	/// Its settings out of the schema-checked `pollers` mapping, with 0301's and 0442's defaults.
	fn from_pollers(pollers: &Map<String, Value>) -> Self;
}

fn group<'a>(pollers: &'a Map<String, Value>, name: &str) -> Option<&'a Map<String, Value>> {
	pollers.get(name).and_then(Value::as_object)
}

fn minutes(group: Option<&Map<String, Value>>, default: f64) -> f64 {
	group.and_then(|map| map.get("everyMinutes")).and_then(Value::as_f64).unwrap_or(default)
}

impl PollerSettings for GithubSettings {
	const NAME: &'static str = "github";

	fn from_pollers(pollers: &Map<String, Value>) -> Self {
		Self { every_minutes: minutes(group(pollers, Self::NAME), DEFAULT_GITHUB_MINUTES) }
	}
}

impl PollerSettings for AzureDevOpsSettings {
	const NAME: &'static str = "azureDevOps";

	fn from_pollers(pollers: &Map<String, Value>) -> Self {
		let own = group(pollers, Self::NAME);
		let orgs = own
			.and_then(|map| map.get("orgs"))
			.and_then(Value::as_array)
			.map(|orgs| orgs.iter().filter_map(|org| org.as_str().map(str::to_owned)).collect())
			.unwrap_or_default();
		Self { orgs, every_minutes: minutes(own, DEFAULT_AZURE_DEVOPS_MINUTES) }
	}
}

impl PollerSettings for CalendarSettings {
	const NAME: &'static str = "calendar";

	fn from_pollers(pollers: &Map<String, Value>) -> Self {
		Self { every_minutes: minutes(group(pollers, Self::NAME), DEFAULT_CALENDAR_MINUTES) }
	}
}

fn feed_of(feed: &Value) -> Option<NewsFeed> {
	let map = feed.as_object()?;
	let limit = map.get("limit").and_then(Value::as_f64).map(|limit| limit as u32);
	match (map.get("source").and_then(Value::as_str), map.get("topic").and_then(Value::as_str)) {
		(Some(source), _) => Some(NewsFeed::Source { source: source.to_owned(), limit }),
		(None, Some(topic)) => Some(NewsFeed::Topic { topic: topic.to_owned(), limit }),
		(None, None) => None,
	}
}

impl PollerSettings for NewsSettings {
	const NAME: &'static str = "news";

	fn from_pollers(pollers: &Map<String, Value>) -> Self {
		let own = group(pollers, Self::NAME);
		let feeds = match own.and_then(|map| map.get("feeds")).and_then(Value::as_array) {
			Some(feeds) => feeds.iter().filter_map(feed_of).collect(),
			None => default_feeds(),
		};
		Self { feeds, every_minutes: minutes(own, DEFAULT_NEWS_MINUTES) }
	}
}

/// Reads one poller's `pollers.<name>` (0301) as `jocasta.yaml`'s schema checked it (0584): a bad value there (or a
/// bad `pollers`) fails with a [`SettingsError`] naming the file, line and key (principle 2) and turns that poller
/// alone off; the other pollers' keys aren't looked at. Unknown keys are ignored here and said by `settings_problems`.
pub fn read_poller<P: PollerSettings>(settings: &Settings) -> Result<P, SettingsError> {
	check_settings(settings, &format!("pollers.{}", P::NAME))?;
	let empty = Map::new();
	let pollers = settings.get("pollers").and_then(Value::as_object).unwrap_or(&empty);
	Ok(P::from_pollers(pollers))
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;
	use crate::shared::settings::{read_settings, settings_file, settings_problems};
	use crate::shared::testing::temp_jocasta;

	fn settings_of(value: Value) -> Settings {
		match value {
			Value::Object(map) => Settings::from_map(map),
			other => panic!("not a mapping: {other}"),
		}
	}

	fn feeds_error(feeds: Value) -> String {
		let settings = settings_of(json!({ "pollers": { "news": { "feeds": feeds } } }));
		read_poller::<NewsSettings>(&settings).unwrap_err().message
	}

	fn news_defaults() -> NewsSettings {
		NewsSettings { feeds: default_feeds(), every_minutes: 15.0 }
	}

	#[test]
	fn poller_settings_defaults_of_0301_and_0442_a_malformed_value_names_its_key_and_turns_off_only_its_poller() {
		let empty = settings_of(json!({}));
		assert_eq!(read_poller::<GithubSettings>(&empty).unwrap(), GithubSettings { every_minutes: 2.0 });
		assert_eq!(
			read_poller::<AzureDevOpsSettings>(&empty).unwrap(),
			AzureDevOpsSettings { orgs: vec![], every_minutes: 5.0 }
		);
		assert_eq!(read_poller::<CalendarSettings>(&empty).unwrap(), CalendarSettings { every_minutes: 10.0 });
		assert_eq!(read_poller::<NewsSettings>(&empty).unwrap(), news_defaults());
		assert_eq!(
			serde_json::to_value(&news_defaults().feeds).unwrap(),
			json!([{ "source": "hn" }, { "source": "lwn" }, { "topic": "gaming", "limit": 10 }])
		);

		let set = settings_of(json!({ "pollers": {
			"azureDevOps": { "orgs": ["https://dev.azure.com/example-org"] },
			"github": { "everyMinutes": 3 },
			"news": { "feeds": [{ "topic": "science", "limit": 5 }, { "source": "acme" }] },
		} }));
		assert_eq!(read_poller::<GithubSettings>(&set).unwrap(), GithubSettings { every_minutes: 3.0 });
		assert_eq!(
			read_poller::<AzureDevOpsSettings>(&set).unwrap(),
			AzureDevOpsSettings { orgs: vec!["https://dev.azure.com/example-org".to_owned()], every_minutes: 5.0 }
		);
		assert_eq!(read_poller::<CalendarSettings>(&set).unwrap(), CalendarSettings { every_minutes: 10.0 });
		assert_eq!(
			read_poller::<NewsSettings>(&set).unwrap(),
			NewsSettings {
				feeds: vec![
					NewsFeed::Topic { topic: "science".to_owned(), limit: Some(5) },
					NewsFeed::Source { source: "acme".to_owned(), limit: None },
				],
				every_minutes: 15.0,
			}
		);
		let none = settings_of(json!({ "pollers": { "news": { "feeds": [] } } }));
		assert_eq!(read_poller::<NewsSettings>(&none).unwrap().feeds, []);

		// A bad GitHub value turns off GitHub only; the other pollers read their own keys.
		let bad_github =
			settings_of(json!({ "pollers": { "github": { "everyMinutes": 0 }, "calendar": { "everyMinutes": 20 } } }));
		assert!(
			read_poller::<GithubSettings>(&bad_github).unwrap_err().message.contains("pollers.github.everyMinutes")
		);
		assert_eq!(read_poller::<CalendarSettings>(&bad_github).unwrap(), CalendarSettings { every_minutes: 20.0 });
		assert_eq!(read_poller::<NewsSettings>(&bad_github).unwrap(), news_defaults());
		// A `pollers` that isn't a mapping turns them all off.
		let five = settings_of(json!({ "pollers": 5 }));
		assert!(
			read_poller::<CalendarSettings>(&five).unwrap_err().message.contains("settings: pollers must be a mapping")
		);
		let bad_orgs = settings_of(json!({ "pollers": { "azureDevOps": { "orgs": ["example-org"] } } }));
		assert!(
			read_poller::<AzureDevOpsSettings>(&bad_orgs).unwrap_err().message.contains("pollers.azureDevOps.orgs")
		);
		let shape = "pollers.news.feeds must be a list of {source: <name>} or {topic: <name>}";
		for feeds in [
			json!({ "source": "hn" }),
			json!([{ "source": "hn", "topic": "gaming" }]),
			json!([{ "limit": 5 }]),
			json!([{ "source": "Hacker News" }]),
			json!([{ "topic": "gaming", "limit": 201 }]),
			json!([{ "topic": "gaming", "limit": 2.5 }]),
		] {
			let said = feeds_error(feeds.clone());
			assert!(said.contains(shape), "{feeds}: {said}");
		}
		let gone = "settings: pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)";
		for old in [json!({ "topics": ["tech"] }), json!({ "limit": 10 })] {
			let said =
				read_poller::<NewsSettings>(&settings_of(json!({ "pollers": { "news": old } }))).unwrap_err().message;
			assert!(said.contains(gone), "{said}");
		}
		let every = settings_of(json!({ "pollers": { "news": { "everyMinutes": 0 } } }));
		assert!(read_poller::<NewsSettings>(&every).unwrap_err().message.contains("pollers.news.everyMinutes"));
	}

	fn write_settings(text: &str) {
		let file = settings_file();
		std::fs::create_dir_all(file.parent().unwrap()).unwrap();
		std::fs::write(file, text).unwrap();
	}

	/// `test/shared/settings.test.ts`' case of an unknown key: the rest of `pollers` still reads.
	#[test]
	fn an_unknown_key_is_said_once_with_its_line_and_ignored_and_the_rest_is_read() {
		let _j = temp_jocasta();
		write_settings(
			&["pollers:", "  gihub:", "    everyMinutes: 3", "  github:", "    everyMinutes: 4", "poller: {}"]
				.join("\n"),
		);
		assert_eq!(
			settings_problems(),
			["jocasta.yaml:2: unknown key gihub in pollers; ignored", "jocasta.yaml:6: unknown key poller; ignored"]
		);
		let settings = read_settings().unwrap();
		assert_eq!(read_poller::<GithubSettings>(&settings).unwrap(), GithubSettings { every_minutes: 4.0 });
	}

	/// `test/shared/settings.test.ts`' case of a bad value: the poller readers' part.
	#[test]
	fn a_bad_value_turns_off_only_its_own_poller_naming_the_line() {
		let _j = temp_jocasta();
		write_settings(
			&[
				"service:",
				"  port: 70000",
				"  quietHours: late",
				"dashboard:",
				"  weather: Example City",
				"pollers:",
				"  github:",
				"    everyMinutes: 0",
			]
			.join("\n"),
		);
		let settings = read_settings().unwrap();
		// Before the service said them at start, a reader's settings error isn't marked said.
		assert!(!check_settings(&settings, "pollers.github").unwrap_err().said);
		assert_eq!(
			settings_problems(),
			[
				"jocasta.yaml:2: service.port must be a port number from 1 to 65535; using 47400",
				"jocasta.yaml:3: service.quietHours must be like \"22:00-08:00\"; quiet hours are off",
				"jocasta.yaml:8: pollers.github.everyMinutes must be a number of minutes, 1 or more; the GitHub poller is off",
			]
		);
		let github = read_poller::<GithubSettings>(&settings).unwrap_err();
		assert_eq!(
			github.message,
			"jocasta.yaml:8: pollers.github.everyMinutes must be a number of minutes, 1 or more"
		);
		assert!(github.said);
		// The other pollers read their own keys and run.
		assert_eq!(read_poller::<CalendarSettings>(&settings).unwrap(), CalendarSettings { every_minutes: 10.0 });
		assert_eq!(read_poller::<NewsSettings>(&settings).unwrap().every_minutes, 15.0);
	}
}
