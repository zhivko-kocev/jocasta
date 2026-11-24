//! `jocasta.yaml` in the config dir: Jocasta-only settings (`service.*`, `dashboard.*`, `pollers.*`, `ntfy.*`,
//! `checks.*`; 0387).
//!
//! The TypeScript validates the file with a typebox schema and reads the problems from the errors; the schema is code
//! here ([`validate`]), reporting the same faults at the same paths.

use std::fs;
use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde_json::{Map, Value};

use crate::shared::errors::Error;
use crate::shared::paths::config_dir;
use crate::shared::yaml::{PathSegment, SchemaProblem, YamlDocument, YamlFileError, parse_yaml_file};

/// `jocasta.yaml` in the config dir: Jocasta-only settings (`service.*`, `dashboard.*`, `pollers.*`; 0387).
pub fn settings_file() -> PathBuf {
	config_dir().join("jocasta.yaml")
}

/// How a problem names the file: by name, as in `jocasta.yaml:5: …` (0584).
const FILE_NAME: &str = "jocasta.yaml";

/// `service.quietHours` (0298): `HH:MM-HH:MM`, hours 0–23.
static QUIET_HOURS: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^\s*([01]?[0-9]|2[0-3]):[0-5][0-9]-([01]?[0-9]|2[0-3]):[0-5][0-9]\s*$").expect("a valid pattern")
});
static ORG_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^https://[^\s"%]+$"#).expect("a valid pattern"));
static MAIL_ADDRESS: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$").expect("a valid pattern"));
static FEED_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new("^[a-z]+$").expect("a valid pattern"));

/// What a bad value at a key means: the reason, and what Jocasta does instead (0584). `map` marks a mapping key.
struct KeyRule {
	reason: String,
	instead: &'static str,
	map: bool,
}

/// What a bad value under `pollers` turns off: its own poller (0584); a bad `pollers` itself turns them all off.
fn poller_off(poller: &str) -> &'static str {
	match poller {
		"github" => "the GitHub poller is off",
		"azureDevOps" => "the Azure DevOps poller is off",
		"calendar" => "the calendar poller is off",
		"gmail" => "the mail poller is off",
		"jira" => "the Jira poller is off",
		"azurePortal" => "the Azure portal poller is off",
		_ => "the news poller is off",
	}
}

const WEATHER_OFF: &str = "the weather is off";
const NEWS_GONE: &str = "pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)";
const NTFY_OFF: &str = "the phone check-in is off";
const CHECKS_DEFAULT: &str = "the check queue uses the default memory reserve";
const COST_DEFAULT: &str = "using the default cost cap, $75 a thread";
const RUNNING_DEFAULT: &str = "using the default running limit, 4 threads";
const THREADS_DEFAULT: &str = "using the default cost cap and running limit";

fn rule(reason: &str, instead: &'static str, map: bool) -> KeyRule {
	KeyRule { reason: reason.to_owned(), instead, map }
}

fn minutes_rule(poller: &str) -> KeyRule {
	rule(&format!("pollers.{poller}.everyMinutes must be a number of minutes, 1 or more"), poller_off(poller), false)
}

fn poller_map(poller: &str) -> KeyRule {
	rule(&format!("pollers.{poller} must be a mapping"), poller_off(poller), true)
}

/// Each key of the schema by its dotted path; a problem belongs to the deepest key on its path.
fn rule_of(key: &str) -> Option<KeyRule> {
	const PORT_QUIET: &str = "using port 47400 and no quiet hours";
	Some(match key {
		"service" => rule("service must be a mapping", PORT_QUIET, true),
		"service.port" => rule("service.port must be a port number from 1 to 65535", "using 47400", false),
		"service.quietHours" => rule("service.quietHours must be like \"22:00-08:00\"", "quiet hours are off", false),
		"service.paneIdleMinutes" => rule(
			"service.paneIdleMinutes must be a number of minutes, 0 or more (0 = panes never sleep)",
			"using 15",
			false,
		),
		"frontier" => rule("frontier must be a mapping", PORT_QUIET, true),
		"frontier.port" => rule("frontier.port must be a port number from 1 to 65535", "using 47400", false),
		"frontier.quietHours" => rule("frontier.quietHours must be like \"22:00-08:00\"", "quiet hours are off", false),
		"dashboard" => rule("dashboard must be a mapping", WEATHER_OFF, true),
		"dashboard.weather" => rule(
			"dashboard.weather must be a place, e.g. \"Example City, Example Country\" or \"12.34, 56.78\"",
			WEATHER_OFF,
			false,
		),
		"pollers" => rule("pollers must be a mapping", "pollers are off", true),
		"pollers.github" => poller_map("github"),
		"pollers.github.everyMinutes" => minutes_rule("github"),
		"pollers.azureDevOps" => poller_map("azureDevOps"),
		"pollers.azureDevOps.orgs" => {
			rule("pollers.azureDevOps.orgs must be a list of https:// org URLs", poller_off("azureDevOps"), false)
		}
		"pollers.azureDevOps.everyMinutes" => minutes_rule("azureDevOps"),
		"pollers.calendar" => poller_map("calendar"),
		"pollers.calendar.everyMinutes" => minutes_rule("calendar"),
		"pollers.gmail" => poller_map("gmail"),
		"pollers.gmail.address" => {
			rule("pollers.gmail.address must be a mail address like someone@example.org", poller_off("gmail"), false)
		}
		"pollers.gmail.everyMinutes" => minutes_rule("gmail"),
		"pollers.jira" => poller_map("jira"),
		"pollers.jira.site" => rule(
			"pollers.jira.site must be an https:// address like https://example.atlassian.net",
			poller_off("jira"),
			false,
		),
		"pollers.jira.email" => {
			rule("pollers.jira.email must be a mail address like someone@example.org", poller_off("jira"), false)
		}
		"pollers.jira.everyMinutes" => minutes_rule("jira"),
		"pollers.azurePortal" => poller_map("azurePortal"),
		"pollers.azurePortal.everyMinutes" => minutes_rule("azurePortal"),
		"pollers.news" => poller_map("news"),
		"pollers.news.feeds" => rule(
			"pollers.news.feeds must be a list of {source: <name>} or {topic: <name>} (a-z), each with an optional limit from 1 to 200",
			poller_off("news"),
			false,
		),
		"pollers.news.everyMinutes" => minutes_rule("news"),
		"pollers.news.topics" | "pollers.news.limit" => rule(NEWS_GONE, poller_off("news"), false),
		"ntfy" => rule("ntfy must be a mapping", NTFY_OFF, true),
		"ntfy.server" => rule("ntfy.server must be an http:// or https:// URL", NTFY_OFF, false),
		"ntfy.ownServer" => rule("ntfy.ownServer must be true or false", "the server is not treated as his own", false),
		"ntfy.token" => rule("ntfy.token must be a non-empty string without spaces", "no token is used", false),
		"checks" => rule("checks must be a mapping", CHECKS_DEFAULT, true),
		"checks.memoryReserveGb" => {
			rule("checks.memoryReserveGb must be a number from 0 to 256", CHECKS_DEFAULT, false)
		}
		"threads" => rule("threads must be a mapping", THREADS_DEFAULT, true),
		"threads.maxCostUsd" => {
			rule("threads.maxCostUsd must be a number of dollars, 0 or more (0 = no cap)", COST_DEFAULT, false)
		}
		"threads.maxRunning" => rule(
			"threads.maxRunning must be a whole number of threads, 0 or more (0 = no limit)",
			RUNNING_DEFAULT,
			false,
		),
		_ => return None,
	})
}

/// One fault the schema finds: the path to it, and whether it is a key the schema doesn't know.
struct Fault {
	path: Vec<String>,
	unknown: bool,
}

fn fault(base: &[&str], name: &str, unknown: bool) -> Fault {
	let mut path: Vec<String> = base.iter().map(|part| (*part).to_owned()).collect();
	path.push(name.to_owned());
	Fault { path, unknown }
}

fn is_integer(value: &Value) -> bool {
	value.as_f64().is_some_and(|number| number.fract() == 0.0)
}

fn integer_between(value: &Value, min: f64, max: f64) -> bool {
	is_integer(value) && value.as_f64().is_some_and(|number| number >= min && number <= max)
}

fn is_minutes(value: &Value) -> bool {
	value.as_f64().is_some_and(|number| number >= 1.0)
}

fn is_quiet_hours(value: &Value) -> bool {
	match value {
		Value::Null => true,
		Value::String(text) => text.is_empty() || QUIET_HOURS.is_match(text),
		_ => false,
	}
}

/// `service` and the old `frontier` group: a port and quiet hours; `service` also the minutes an idle pane waits before
/// it sleeps (0676); `frontier` also knows two keys that are gone.
fn check_service(group: &str, value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], group, false)) };
	for (name, value) in map {
		match name.as_str() {
			"port" if !integer_between(value, 1.0, 65_535.0) => faults.push(fault(&[group], name, false)),
			"quietHours" if !is_quiet_hours(value) => faults.push(fault(&[group], name, false)),
			"paneIdleMinutes" if group == "service" && !value.as_f64().is_some_and(|minutes| minutes >= 0.0) => {
				faults.push(fault(&[group], name, false));
			}
			"port" | "quietHours" => {}
			"paneIdleMinutes" if group == "service" => {}
			"autostart" | "dashboard" if group == "frontier" => {}
			_ => faults.push(fault(&[group], name, true)),
		}
	}
}

fn check_dashboard(value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], "dashboard", false)) };
	for (name, value) in map {
		let place = match value {
			Value::Null => true,
			Value::String(text) => text.chars().any(|c| !c.is_whitespace()),
			_ => false,
		};
		if name != "weather" {
			faults.push(fault(&["dashboard"], name, true));
		} else if !place {
			faults.push(fault(&["dashboard"], name, false));
		}
	}
}

fn is_feed(value: &Value) -> bool {
	let Some(map) = value.as_object() else { return false };
	let name_key = if map.contains_key("source") { "source" } else { "topic" };
	map.iter().all(|(key, value)| match key.as_str() {
		key if key == name_key => value.as_str().is_some_and(|name| FEED_NAME.is_match(name)),
		"limit" => integer_between(value, 1.0, 200.0),
		_ => false,
	}) && map.contains_key(name_key)
}

fn check_poller(poller: &str, value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&["pollers"], poller, false)) };
	let base = ["pollers", poller];
	for (name, value) in map {
		match (poller, name.as_str()) {
			(_, "everyMinutes") if !is_minutes(value) => faults.push(fault(&base, name, false)),
			("azureDevOps", "orgs") => {
				let orgs = value.as_array();
				let listed =
					orgs.is_some_and(|orgs| orgs.iter().all(|o| o.as_str().is_some_and(|u| ORG_URL.is_match(u))));
				if !listed {
					faults.push(fault(&base, name, false));
				}
			}
			("gmail", "address") | ("jira", "email") => {
				if !value.as_str().is_some_and(|address| MAIL_ADDRESS.is_match(address)) {
					faults.push(fault(&base, name, false));
				}
			}
			("jira", "site") => {
				if !value.as_str().is_some_and(|site| ORG_URL.is_match(site)) {
					faults.push(fault(&base, name, false));
				}
			}
			("news", "feeds") => {
				if !value.as_array().is_some_and(|feeds| feeds.iter().all(is_feed)) {
					faults.push(fault(&base, name, false));
				}
			}
			("news", "topics" | "limit") => faults.push(fault(&base, name, false)),
			(_, "everyMinutes") => {}
			_ => faults.push(fault(&base, name, true)),
		}
	}
}

fn check_pollers(value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], "pollers", false)) };
	for (name, value) in map {
		match name.as_str() {
			"github" | "azureDevOps" | "calendar" | "news" | "gmail" | "jira" | "azurePortal" => {
				check_poller(name, value, faults)
			}
			_ => faults.push(fault(&["pollers"], name, true)),
		}
	}
}

/// What is wrong in `settings` by the schema (0298, 0301, 0358, 0387, 0429, 0442, 0557, 0584).
fn validate(settings: &Map<String, Value>) -> Vec<Fault> {
	let mut faults = Vec::new();
	for (name, value) in settings {
		match name.as_str() {
			"service" | "frontier" => check_service(name, value, &mut faults),
			"dashboard" => check_dashboard(value, &mut faults),
			"pollers" => check_pollers(value, &mut faults),
			"ntfy" => check_ntfy(value, &mut faults),
			"checks" => check_checks(value, &mut faults),
			"threads" => check_threads(value, &mut faults),
			_ => faults.push(fault(&[], name, true)),
		}
	}
	faults
}

/// `ntfy.server` (0672): an `http://` or `https://` URL without whitespace.
fn is_ntfy_server(url: &str) -> bool {
	["http://", "https://"].iter().any(|scheme| url.strip_prefix(scheme).is_some_and(|rest| !rest.is_empty()))
		&& !url.chars().any(char::is_whitespace)
}

/// `ntfy.token` (0672): not empty, no whitespace.
fn is_token(token: &str) -> bool {
	!token.is_empty() && !token.chars().any(char::is_whitespace)
}

fn is_token_value(value: &Value) -> bool {
	value.as_str().is_some_and(is_token)
}

fn check_ntfy(value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], "ntfy", false)) };
	for (name, value) in map {
		let good = match name.as_str() {
			"server" => value.as_str().is_some_and(is_ntfy_server),
			"ownServer" => value.is_boolean(),
			"token" => is_token_value(value),
			_ => {
				faults.push(fault(&["ntfy"], name, true));
				continue;
			}
		};
		if !good {
			faults.push(fault(&["ntfy"], name, false));
		}
	}
}

fn check_checks(value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], "checks", false)) };
	for (name, value) in map {
		match name.as_str() {
			"memoryReserveGb" if !value.as_f64().is_some_and(|gb| (0.0..=256.0).contains(&gb)) => {
				faults.push(fault(&["checks"], name, false));
			}
			"memoryReserveGb" => {}
			_ => faults.push(fault(&["checks"], name, true)),
		}
	}
}

fn check_threads(value: &Value, faults: &mut Vec<Fault>) {
	let Some(map) = value.as_object() else { return faults.push(fault(&[], "threads", false)) };
	for (name, value) in map {
		match name.as_str() {
			"maxCostUsd" if !value.as_f64().is_some_and(|dollars| dollars >= 0.0) => {
				faults.push(fault(&["threads"], name, false));
			}
			"maxCostUsd" => {}
			"maxRunning" if value.as_u64().is_none() => faults.push(fault(&["threads"], name, false)),
			"maxRunning" => {}
			_ => faults.push(fault(&["threads"], name, true)),
		}
	}
}

/// One thing wrong in `jocasta.yaml`: an unknown key (ignored) or a bad value at `key` (its feature falls back).
#[derive(Debug)]
struct Problem {
	/// The dotted key the problem belongs to: the bad value's key, or the mapping holding the unknown key.
	key: String,
	/// The 1-based line, when the settings came from the file.
	line: Option<usize>,
	/// `unknown key gihub in pollers`, or the bad value's reason.
	reason: String,
	/// `ignored`, or what Jocasta does instead.
	instead: String,
	unknown: bool,
	/// Whether [`settings_problems`] returned it, so the service said it at start (C7).
	said: bool,
}

/// `jocasta.yaml` as read, and where its lines are: the settings object every reader checks its own group of
/// (`check_settings`). The TypeScript keeps the lines and the problems beside the object by its identity; here they are
/// its fields. It dereferences to its mapping.
#[derive(Debug)]
pub struct Settings {
	map: Map<String, Value>,
	document: Option<YamlDocument<()>>,
	problems: Mutex<Option<Vec<Problem>>>,
}

impl Deref for Settings {
	type Target = Map<String, Value>;

	fn deref(&self) -> &Map<String, Value> {
		&self.map
	}
}

impl Settings {
	/// Settings that didn't come from the file (a test's, a trial's): their problems have no line.
	pub fn from_map(map: Map<String, Value>) -> Self {
		Self { map, document: None, problems: Mutex::new(None) }
	}

	fn line_of(&self, path: &[String]) -> Option<usize> {
		let segments: Vec<PathSegment> = path.iter().map(|part| PathSegment::Key(part.clone())).collect();
		self.document.as_ref().map(|document| document.line_of(&segments))
	}

	/// The problems in the settings, by line; unknown keys and bad values both (0584). Worked out once.
	fn with_problems<T>(&self, work: impl FnOnce(&mut Vec<Problem>) -> T) -> T {
		let mut cached = self.problems.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		work(cached.get_or_insert_with(|| self.problems_of()))
	}

	fn problems_of(&self) -> Vec<Problem> {
		let mut problems: Vec<(String, Problem)> = Vec::new();
		for found in validate(&self.map) {
			let path = &found.path;
			let depth = (1..=path.len()).rev().find(|i| rule_of(&path[..*i].join(".")).is_some()).unwrap_or(0);
			let key = path[..depth].join(".");
			let rule = rule_of(&key);
			let unknown_key =
				found.unknown && path.len() == depth + 1 && (depth == 0 || rule.as_ref().is_some_and(|r| r.map));
			if unknown_key {
				let name = &path[depth];
				let id = format!("{key}\0{name}");
				if problems.iter().any(|(existing, _)| *existing == id) {
					continue;
				}
				let reason =
					if depth == 0 { format!("unknown key {name}") } else { format!("unknown key {name} in {key}") };
				let line = self.line_of(path);
				problems.push((
					id,
					Problem { key, line, reason, instead: "ignored".to_owned(), unknown: true, said: false },
				));
			} else if let Some(rule) = rule
				&& !problems.iter().any(|(existing, _)| *existing == key)
			{
				let line = self.line_of(&path[..depth]);
				let problem = Problem {
					key: key.clone(),
					line,
					reason: rule.reason,
					instead: rule.instead.to_owned(),
					unknown: false,
					said: false,
				};
				problems.push((key, problem));
			}
		}
		// `ntfy.ownServer` without a token (0672): the phone stays content-free; said once, at the key's line.
		let own_without_token = self.map.get("ntfy").and_then(Value::as_object).is_some_and(|ntfy| {
			ntfy.get("ownServer") == Some(&Value::Bool(true)) && !ntfy.get("token").is_some_and(is_token_value)
		});
		if own_without_token && !problems.iter().any(|(key, _)| key == "ntfy.token") {
			let line = self.line_of(&["ntfy".to_owned(), "ownServer".to_owned()]);
			let reason = "ntfy.ownServer is true but ntfy.token is not set".to_owned();
			let problem = Problem {
				key: "ntfy.token".to_owned(),
				line,
				reason,
				instead: "pushes stay content-free".to_owned(),
				unknown: false,
				said: false,
			};
			problems.push(("ntfy.token".to_owned(), problem));
		}
		let mut sorted: Vec<Problem> = problems.into_iter().map(|(_, problem)| problem).collect();
		sorted.sort_by_key(|problem| problem.line.unwrap_or(0));
		sorted
	}
}

/// A bad value a reader met ([`check_settings`]): `jocasta.yaml:<line>: <reason>`. `said` is whether the service already
/// said it at start (0584, C7), so a reader that reports failures says it only when the file changed since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
	pub message: String,
	pub said: bool,
}

impl std::fmt::Display for SettingsError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for SettingsError {}

impl From<SettingsError> for Error {
	fn from(error: SettingsError) -> Self {
		Error::Other(error.message)
	}
}

/// For the reader of one feature (`pollers.github`, `dashboard.weather`, `service.quietHours`…): the first bad value
/// at, under or above `key` as a [`SettingsError`] `jocasta.yaml:<line>: <reason>` (`settings: <reason>` for settings
/// not read from the file), so that feature alone is off (0584, principle 4). Unknown keys never fail;
/// [`settings_problems`] says them.
pub fn check_settings(settings: &Settings, key: &str) -> Result<(), SettingsError> {
	let found = settings.with_problems(|problems| {
		problems
			.iter()
			.find(|p| {
				!p.unknown
					&& (p.key == key
						|| p.key.starts_with(&format!("{key}."))
						|| key.starts_with(&format!("{}.", p.key)))
			})
			.map(|p| (p.line, p.reason.clone(), p.said))
	});
	let Some((line, reason, said)) = found else { return Ok(()) };
	let place = line.map_or_else(|| "settings".to_owned(), |line| format!("{FILE_NAME}:{line}"));
	Err(SettingsError { message: format!("{place}: {reason}"), said })
}

/// The keys of the `service` group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKey {
	Port,
	QuietHours,
}

impl ServiceKey {
	fn name(self) -> &'static str {
		match self {
			Self::Port => "port",
			Self::QuietHours => "quietHours",
		}
	}
}

/// A key of the `service` group (`port`, `quietHours`), read where his file sets it (0584): `service.<name>`, else the
/// old `frontier.<name>`; `service` wins when both set it. Checked like [`check_settings`], so a bad value fails
/// naming the key as his file spells it; `None` when neither sets it. A `port` is a number, `quietHours` a string or
/// `null`.
pub fn service_setting(settings: &Settings, name: ServiceKey) -> Result<Option<Value>, SettingsError> {
	let name = name.name();
	// A group speaks for the key when it sets it, or is there but no mapping (its own problem is then the one thrown).
	let sets = |group: Option<&Value>| match group {
		None => false,
		Some(Value::Object(map)) => map.contains_key(name),
		Some(_) => true,
	};
	let group = if sets(settings.get("service")) || !sets(settings.get("frontier")) { "service" } else { "frontier" };
	check_settings(settings, &format!("{group}.{name}"))?;
	Ok(settings.get(group).and_then(Value::as_object).and_then(|map| map.get(name)).cloned())
}

/// `ntfy.*` (0672): the phone check-in. `server` `None` = off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NtfySettings {
	pub server: Option<String>,
	/// Marks the server as his own: with a token, pushes carry text and Allow buttons.
	pub own_server: bool,
	/// His ntfy access token: a secret, never logged.
	pub token: Option<String>,
}

fn ntfy_of(settings: &Settings) -> NtfySettings {
	let Some(group) = settings.get("ntfy").and_then(Value::as_object) else { return NtfySettings::default() };
	let read = |name: &str| check_settings(settings, &format!("ntfy.{name}")).ok().and_then(|()| group.get(name));
	NtfySettings {
		server: read("server").and_then(Value::as_str).filter(|url| is_ntfy_server(url)).map(str::to_owned),
		own_server: read("ownServer").and_then(Value::as_bool).unwrap_or(false),
		token: read("token").and_then(Value::as_str).filter(|token| is_token(token)).map(str::to_owned),
	}
}

/// The `ntfy` group of `jocasta.yaml` (0672); a bad value reads as unset ([`settings_problems`] says it).
pub fn read_ntfy() -> NtfySettings {
	read_settings().map(|settings| ntfy_of(&settings)).unwrap_or_default()
}

/// `checks.memoryReserveGb` (0671): memory kept free when admitting a check; 2 when unset or bad.
pub fn read_check_reserve_gb() -> f64 {
	const DEFAULT: f64 = 2.0;
	let Ok(settings) = read_settings() else { return DEFAULT };
	if check_settings(&settings, "checks.memoryReserveGb").is_err() {
		return DEFAULT;
	}
	settings
		.get("checks")
		.and_then(|checks| checks.get("memoryReserveGb"))
		.and_then(Value::as_f64)
		.filter(|gb| (0.0..=256.0).contains(gb))
		.unwrap_or(DEFAULT)
}

/// `threads.maxCostUsd` (0686): a thread's cost cap in dollars; 75 when unset or bad, 0 = no cap.
pub fn read_max_cost_usd() -> f64 {
	const DEFAULT: f64 = 75.0;
	let Ok(settings) = read_settings() else { return DEFAULT };
	if check_settings(&settings, "threads.maxCostUsd").is_err() {
		return DEFAULT;
	}
	settings
		.get("threads")
		.and_then(|threads| threads.get("maxCostUsd"))
		.and_then(Value::as_f64)
		.filter(|dollars| *dollars >= 0.0)
		.unwrap_or(DEFAULT)
}

/// `threads.maxRunning` (0687): how many threads may run at once; 4 when unset or bad, 0 = no limit.
pub fn read_max_running() -> u64 {
	const DEFAULT: u64 = 4;
	let Ok(settings) = read_settings() else { return DEFAULT };
	if check_settings(&settings, "threads.maxRunning").is_err() {
		return DEFAULT;
	}
	settings.get("threads").and_then(|threads| threads.get("maxRunning")).and_then(Value::as_u64).unwrap_or(DEFAULT)
}

/// `service.paneIdleMinutes` (0676): minutes a pane's omp idles before it sleeps; 15 when unset or bad, 0 = never.
pub fn read_pane_idle_minutes() -> f64 {
	const DEFAULT: f64 = 15.0;
	let Ok(settings) = read_settings() else { return DEFAULT };
	if check_settings(&settings, "service.paneIdleMinutes").is_err() {
		return DEFAULT;
	}
	settings
		.get("service")
		.and_then(|service| service.get("paneIdleMinutes"))
		.and_then(Value::as_f64)
		.filter(|minutes| *minutes >= 0.0)
		.unwrap_or(DEFAULT)
}

/// The last text read of each file and what it parsed to, so an unchanged file is parsed and validated once. Keyed by
/// file: two roots (tests, in one process) don't share one parse and its said-marks, or evict each other's.
type Parsed = std::collections::HashMap<PathBuf, (String, Arc<Settings>)>;
static LAST: Mutex<Option<Parsed>> = Mutex::new(None);

/// Why [`read_settings`] failed: the file isn't YAML or isn't a mapping (naming file and line), or can't be read.
#[derive(Debug)]
pub enum SettingsReadError {
	Yaml(YamlFileError),
	Other(Error),
}

impl std::fmt::Display for SettingsReadError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Yaml(error) => error.fmt(f),
			Self::Other(error) => error.fmt(f),
		}
	}
}

impl std::error::Error for SettingsReadError {}

impl From<SettingsReadError> for Error {
	fn from(error: SettingsReadError) -> Self {
		match error {
			SettingsReadError::Yaml(error) => Error::Other(error.to_string()),
			SettingsReadError::Other(error) => error,
		}
	}
}

/// The Jocasta-only settings. A missing or empty file reads as `{}`; a file that isn't YAML or isn't a mapping fails,
/// naming the file and line (principle 2). Unknown keys and bad values stay in the object: each reader checks its own
/// group ([`check_settings`]) and [`settings_problems`] says them all.
pub fn read_settings() -> Result<Arc<Settings>, SettingsReadError> {
	let file = settings_file();
	let text = match fs::read_to_string(&file) {
		Ok(text) => text,
		Err(error) if error.kind() == io::ErrorKind::NotFound => {
			return Ok(Arc::new(Settings::from_map(Map::new())));
		}
		Err(error) => return Err(SettingsReadError::Other(error.into())),
	};
	let mut cache = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	let last = cache.get_or_insert_with(std::collections::HashMap::new);
	if let Some((cached, settings)) = last.get(&file)
		&& *cached == text
	{
		return Ok(Arc::clone(settings));
	}
	let mut map = Map::new();
	let document = parse_yaml_file(&text, &file.to_string_lossy(), |value| match value {
		Value::Object(object) => {
			map = object.clone();
			Ok(())
		}
		Value::Null => Ok(()),
		_ => Err(SchemaProblem::Invalid { path: vec![], message: "must be object".to_owned() }),
	})
	.map_err(SettingsReadError::Yaml)?;
	let settings = Arc::new(Settings { map, document: Some(document), problems: Mutex::new(None) });
	last.insert(file, (text, Arc::clone(&settings)));
	Ok(settings)
}

/// Everything wrong in `jocasta.yaml`, each as one line for a notification at start (0584, contract C7):
/// `jocasta.yaml:<line>: unknown key <k> in <group>; ignored`, or `jocasta.yaml:<line>: <reason>; <what Jocasta does
/// instead>`. A file that isn't YAML is one problem; this never fails.
pub fn settings_problems() -> Vec<String> {
	let settings = match read_settings() {
		Ok(settings) => settings,
		Err(SettingsReadError::Yaml(error)) => {
			return vec![format!("{FILE_NAME}:{}: {}; its settings are off", error.line, error.problem)];
		}
		Err(SettingsReadError::Other(error)) => return vec![format!("{FILE_NAME}: {error}; its settings are off")],
	};
	settings.with_problems(|problems| {
		for problem in problems.iter_mut() {
			problem.said = true;
		}
		problems.iter().map(|p| format!("{FILE_NAME}:{}: {}; {}", p.line.unwrap_or(1), p.reason, p.instead)).collect()
	})
}

/// Settings that are gone (0557), each as the line that says so: `frontier.autostart` (Jocasta no longer starts at logon)
/// and `frontier.dashboard.display` (there is no companion window). A `jocasta.yaml` that still sets them works as
/// before otherwise.
pub fn gone_settings(settings: &Map<String, Value>) -> Vec<String> {
	let Some(frontier) = settings.get("frontier").and_then(Value::as_object) else { return Vec::new() };
	let mut gone = Vec::new();
	if frontier.contains_key("autostart") {
		gone.push(
			"settings: frontier.autostart is gone; Jocasta no longer starts at logon: run `jocasta` (0557)".to_owned(),
		);
	}
	if frontier.get("dashboard").and_then(Value::as_object).is_some_and(|dashboard| dashboard.contains_key("display")) {
		gone.push(
			"settings: frontier.dashboard.display is gone; there is no companion window to place (0557)".to_owned(),
		);
	}
	gone
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;
	use crate::shared::events::quiet::{QuietHours, read_quiet_hours};
	use crate::shared::testing::{TempJocasta, temp_jocasta};

	fn write_settings(env: &TempJocasta, text: &str) {
		let _ = env;
		fs::create_dir_all(config_dir()).unwrap();
		fs::write(config_dir().join("jocasta.yaml"), text).unwrap();
	}

	fn object(value: Value) -> Map<String, Value> {
		value.as_object().cloned().expect("an object")
	}

	fn problems_of(settings: &Settings) -> Vec<String> {
		settings.with_problems(|problems| problems.iter().map(|p| p.reason.clone()).collect())
	}

	#[test]
	fn read_settings_reads_jocasta_yaml_in_the_config_dir() {
		let env = temp_jocasta();
		assert_eq!(settings_file(), config_dir().join("jocasta.yaml"));
		write_settings(
			&env,
			&[
				"frontier:",
				"  port: 7420",
				"  autostart: false",
				"  quietHours: { start: '22:00', end: '07:00' }",
				"pollers:",
				"  azureDevOps:",
				"    orgs: [example-org]",
				"graph:",
				"  small: anthropic/example-small",
			]
			.join("\n"),
		);
		assert_eq!(
			**read_settings().unwrap(),
			object(json!({
				"frontier": { "port": 7420, "autostart": false, "quietHours": { "start": "22:00", "end": "07:00" } },
				"pollers": { "azureDevOps": { "orgs": ["example-org"] } },
				"graph": { "small": "anthropic/example-small" },
			}))
		);
	}

	#[test]
	fn a_missing_empty_or_comment_only_file_reads_as_an_empty_mapping() {
		let env = temp_jocasta();
		assert_eq!(**read_settings().unwrap(), Map::new());
		write_settings(&env, "");
		assert_eq!(**read_settings().unwrap(), Map::new());
		write_settings(&env, "# nothing set yet\n");
		assert_eq!(**read_settings().unwrap(), Map::new());
	}

	#[test]
	fn a_file_that_isnt_yaml_or_isnt_a_mapping_fails_naming_the_file_and_line() {
		let env = temp_jocasta();
		let file = config_dir().join("jocasta.yaml").display().to_string();
		write_settings(&env, "frontier:\n  port: 7420\npollers: [unclosed\n");
		let Err(SettingsReadError::Yaml(error)) = read_settings() else { panic!("not a YAML error") };
		assert_eq!((error.file.as_str(), error.line), (file.as_str(), 3));
		assert!(read_settings().unwrap_err().to_string().starts_with(&format!("{file}:3: ")));
		write_settings(&env, "- a\n- b\n");
		assert!(read_settings().unwrap_err().to_string().starts_with(&format!("{file}:1: ")));
	}

	#[test]
	fn pis_settings_json_is_no_longer_read() {
		let _env = temp_jocasta();
		fs::create_dir_all(config_dir()).unwrap();
		fs::write(config_dir().join("settings.json"), json!({ "frontier": { "port": 1 } }).to_string()).unwrap();
		assert_eq!(**read_settings().unwrap(), Map::new());
	}

	#[test]
	fn frontier_autostart_and_frontier_dashboard_display_are_said_to_be_gone() {
		assert_eq!(gone_settings(&Map::new()), Vec::<String>::new());
		assert_eq!(
			gone_settings(&object(json!({ "frontier": { "port": 7420, "quietHours": "22:00-07:00" } }))),
			Vec::<String>::new()
		);
		assert_eq!(
			gone_settings(&object(json!({ "frontier": { "autostart": false, "dashboard": { "display": 2 } } }))),
			[
				"settings: frontier.autostart is gone; Jocasta no longer starts at logon: run `jocasta` (0557)",
				"settings: frontier.dashboard.display is gone; there is no companion window to place (0557)",
			]
		);
	}

	#[test]
	fn an_unknown_key_is_said_once_with_its_line_and_ignored() {
		let env = temp_jocasta();
		write_settings(
			&env,
			&["pollers:", "  gihub:", "    everyMinutes: 3", "  github:", "    everyMinutes: 4", "poller: {}"]
				.join("\n"),
		);
		assert_eq!(
			settings_problems(),
			["jocasta.yaml:2: unknown key gihub in pollers; ignored", "jocasta.yaml:6: unknown key poller; ignored"]
		);
		// The rest applies: `readPoller` is `pollers/settings`' own port, which checks `pollers.github` first.
		assert!(check_settings(&read_settings().unwrap(), "pollers.github").is_ok());
		assert_eq!(read_settings().unwrap()["pollers"]["github"], json!({ "everyMinutes": 4 }));
	}

	#[test]
	fn a_bad_value_turns_off_only_its_own_feature_naming_the_line() {
		let env = temp_jocasta();
		write_settings(
			&env,
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
		let github = check_settings(&settings, "pollers.github").unwrap_err();
		assert_eq!(
			github.message,
			"jocasta.yaml:8: pollers.github.everyMinutes must be a number of minutes, 1 or more"
		);
		assert!(github.said);
		// The other pollers and the weather read their own keys and run.
		assert!(check_settings(&settings, "pollers.calendar").is_ok());
		assert!(check_settings(&settings, "pollers.news").is_ok());
		assert!(check_settings(&settings, "dashboard.weather").is_ok());
		let quiet = read_quiet_hours(&settings).unwrap_err();
		assert!(quiet.message.starts_with("jocasta.yaml:3: service.quietHours must be like \"22:00-08:00\""));
	}

	#[test]
	fn known_and_gone_keys_pass_the_old_frontier_group_is_read_and_a_malformed_file_is_one_problem() {
		let env = temp_jocasta();
		write_settings(
			&env,
			&[
				"frontier:",
				"  port: 7420",
				"  quietHours: 22:00-07:00",
				"  autostart: false",
				"  dashboard: { display: primary }",
				"pollers:",
				"  news: { feeds: [{ source: hn }, { topic: gaming, limit: 5 }] }",
			]
			.join("\n"),
		);
		assert_eq!(settings_problems(), Vec::<String>::new());
		assert_eq!(
			read_quiet_hours(&read_settings().unwrap()).unwrap(),
			Some(QuietHours { from: 22 * 60, to: 7 * 60 })
		);
		assert_eq!(service_setting(&read_settings().unwrap(), ServiceKey::Port).unwrap(), Some(json!(7420)));
		write_settings(&env, "frontier:\n  port: 7420\npollers: [unclosed\n");
		let problems = settings_problems();
		assert_eq!(problems.len(), 1);
		assert!(problems[0].starts_with("jocasta.yaml:3: ") && problems[0].ends_with("; its settings are off"));
	}

	#[test]
	fn service_wins_over_the_old_frontier_group_key_by_key_and_a_problem_names_the_key_as_the_file_spells_it() {
		let env = temp_jocasta();
		write_settings(
			&env,
			&["frontier:", "  port: 7420", "  quietHours: 22:00-07:00", "service:", "  port: 7421"].join("\n"),
		);
		assert_eq!(settings_problems(), Vec::<String>::new());
		assert_eq!(service_setting(&read_settings().unwrap(), ServiceKey::Port).unwrap(), Some(json!(7421)));
		// `service` doesn't set quiet hours: `frontier`'s still apply.
		assert_eq!(
			read_quiet_hours(&read_settings().unwrap()).unwrap(),
			Some(QuietHours { from: 22 * 60, to: 7 * 60 })
		);
		write_settings(&env, &["service:", "  quietHours: 23:00-06:00", "frontier:", "  quietHours: late"].join("\n"));
		assert_eq!(
			read_quiet_hours(&read_settings().unwrap()).unwrap(),
			Some(QuietHours { from: 23 * 60, to: 6 * 60 })
		);
		write_settings(&env, &["frontier:", "  port: 0"].join("\n"));
		let bad = service_setting(&read_settings().unwrap(), ServiceKey::Port).unwrap_err();
		assert!(bad.message.starts_with("jocasta.yaml:2: frontier.port must be a port number from 1 to 65535"));
		assert_eq!(service_setting(&Settings::from_map(Map::new()), ServiceKey::Port).unwrap(), None);
	}

	#[test]
	fn settings_that_did_not_come_from_the_file_have_no_line() {
		let settings = Settings::from_map(object(json!({
			"service": { "quietHours": "late", "port": 7 },
			"pollers": { "news": { "topics": [], "feeds": [{ "source": "hn", "topic": "x" }] }, "azureDevOps": { "orgs": ["http://x"] } },
			"dashboard": { "weather": "  " },
			"zzz": 1,
		})));
		assert_eq!(
			check_settings(&settings, "service.quietHours").unwrap_err().message,
			"settings: service.quietHours must be like \"22:00-08:00\""
		);
		assert!(check_settings(&settings, "service.port").is_ok());
		assert_eq!(check_settings(&settings, "service").unwrap_err().message.split(':').next(), Some("settings"));
		assert!(check_settings(&settings, "pollers.news").is_err());
		assert!(check_settings(&settings, "pollers.azureDevOps.orgs").is_err());
		assert!(check_settings(&settings, "dashboard").is_err());
		let reasons = problems_of(&settings);
		assert!(reasons.contains(&"unknown key zzz".to_owned()));
		assert!(reasons.iter().any(|r| r.starts_with("pollers.news.topics and pollers.news.limit are gone")));
	}

	#[test]
	fn ntfy_and_checks_settings_are_read_and_a_bad_value_falls_back_to_unset() {
		let env = temp_jocasta();
		assert_eq!(read_ntfy(), NtfySettings::default());
		assert_eq!(read_check_reserve_gb(), 2.0);
		write_settings(
			&env,
			&[
				"ntfy:",
				"  server: https://ntfy.example.org",
				"  ownServer: true",
				"  token: tk_example",
				"checks:",
				"  memoryReserveGb: 4.5",
			]
			.join("\n"),
		);
		assert_eq!(settings_problems(), Vec::<String>::new());
		assert_eq!(
			read_ntfy(),
			NtfySettings {
				server: Some("https://ntfy.example.org".to_owned()),
				own_server: true,
				token: Some("tk_example".to_owned()),
			}
		);
		assert_eq!(read_check_reserve_gb(), 4.5);
		write_settings(
			&env,
			&[
				"ntfy:",
				"  server: ntfy.example.org",
				"  ownServer: maybe",
				"  token: has space",
				"checks:",
				"  memoryReserveGb: 300",
			]
			.join("\n"),
		);
		assert_eq!(read_ntfy(), NtfySettings::default());
		assert_eq!(read_check_reserve_gb(), 2.0);
		let problems = settings_problems();
		assert_eq!(problems.len(), 4);
		assert!(problems[0].starts_with("jocasta.yaml:2: ntfy.server must be an http:// or https:// URL"));
		assert!(problems[3].starts_with("jocasta.yaml:6: checks.memoryReserveGb must be a number from 0 to 256"));
		write_settings(&env, "ntfy:\n  serv: x\nchecks: 3\n");
		assert_eq!(
			settings_problems(),
			[
				"jocasta.yaml:2: unknown key serv in ntfy; ignored",
				"jocasta.yaml:3: checks must be a mapping; the check queue uses the default memory reserve",
			]
		);
	}

	#[test]
	fn own_server_without_a_token_is_said_once_and_pushes_stay_content_free() {
		let env = temp_jocasta();
		write_settings(&env, &["ntfy:", "  server: https://ntfy.example.org", "  ownServer: true"].join("\n"));
		assert_eq!(
			settings_problems(),
			["jocasta.yaml:3: ntfy.ownServer is true but ntfy.token is not set; pushes stay content-free"]
		);
		let ntfy = read_ntfy();
		assert!(ntfy.own_server && ntfy.token.is_none());
		write_settings(&env, &["ntfy:", "  server: https://ntfy.example.org", "  ownServer: false"].join("\n"));
		assert_eq!(settings_problems(), Vec::<String>::new());
	}

	#[test]
	fn threads_max_cost_usd_is_75_unless_set_0_is_no_cap_and_a_bad_value_falls_back_0686() {
		let env = temp_jocasta();
		assert_eq!(read_max_cost_usd(), 75.0);
		write_settings(&env, "threads:\n  maxCostUsd: 20.5\n");
		assert_eq!(settings_problems(), Vec::<String>::new());
		assert_eq!(read_max_cost_usd(), 20.5);
		write_settings(&env, "threads:\n  maxCostUsd: 0\n");
		assert_eq!(read_max_cost_usd(), 0.0);
		write_settings(&env, "threads:\n  maxCostUsd: -1\n");
		assert_eq!(read_max_cost_usd(), 75.0);
		assert_eq!(settings_problems().len(), 1);
		write_settings(&env, "threads:\n  maxCostUsd: lots\n");
		assert_eq!(read_max_cost_usd(), 75.0);
	}

	#[test]
	fn threads_max_running_is_4_unless_set_0_is_no_limit_and_a_bad_value_falls_back_0687() {
		let env = temp_jocasta();
		assert_eq!(read_max_running(), 4);
		write_settings(&env, "threads:\n  maxRunning: 2\n  maxCostUsd: 10\n");
		assert_eq!(settings_problems(), Vec::<String>::new());
		assert_eq!(read_max_running(), 2);
		write_settings(&env, "threads:\n  maxRunning: 0\n");
		assert_eq!(read_max_running(), 0);
		write_settings(&env, "threads:\n  maxRunning: -1\n");
		assert_eq!(read_max_running(), 4);
		assert_eq!(settings_problems().len(), 1);
		write_settings(&env, "threads:\n  maxRunning: 2.5\n");
		assert_eq!(read_max_running(), 4);
		write_settings(&env, "threads:\n  maxRunning: many\n");
		assert_eq!(read_max_running(), 4);
	}
}
