//! The service's state feed and question API (4.1, 0409, 0552): typed JSON messages from the Jocasta service to a
//! client (the herdr plugin's panes, 0553) and the questions it asks him. The service owns this file.
//!
//! Transport: a client opens `ws://127.0.0.1:<port>/ws` with the service's token as `Authorization: Bearer <token>`
//! (0254; a browser page has no socket and no API token, SEC-NET-4). Every message is one JSON object with a `type`.
//! Questions (the close question, his Allow) are read and answered over HTTP with a one-time code, never broadcast
//! (0519).

use std::path::PathBuf;

use serde::{Deserialize, Serialize, Serializer};

/// The WebSocket path.
pub const WS_PATH: &str = "/ws";
/// The query parameter a browser page opens with: `http://127.0.0.1:<port>/?t=<code>`, a one-time launch code the
/// service swaps for the page cookie (0254, SEC-LOC-11).
pub const TOKEN_QUERY: &str = "t";
/// The page cookie's name before its port (0254).
const TOKEN_COOKIE: &str = "jocasta_service";

/// The page cookie of the service on `port` (SEC-NET-4): cookies aren't kept apart by port, so the name is.
pub fn token_cookie(port: u16) -> String {
	format!("{TOKEN_COOKIE}_{port}")
}

/// Where a question's view reads its texts (`GET /api/popup/<code>`) and posts its answer (`POST /api/popup/<code>`, body
/// `PopupAnswer`), with the service's token and the question's one-time code (0519). The code is never listed or
/// broadcast: the service hands it only to what shows the question (the herdr popup, 0553). The popup holds
/// `GET /api/popup/<code>/hold` open while it shows the question: the question ends unanswered when that request ends
/// before an answer (closing the popup any other way denies, 0558), and the service ends it once the question is over.
///
/// One reader per question (0578): the first `GET /api/popup/<code>` answers the view with a reader secret in the
/// `POPUP_READER_HEADER` response header; the hold and the answer must carry it in the same request header (403
/// otherwise). A second GET for an open question ends it as its fallback (410) and warns him. The answer's replies:
/// 204 taken; 409 a wrong code, body `{"left": n}` (tries left); 410 the third wrong code, which took the fallback.
pub const POPUP_PATH: &str = "/api/popup/";
/// The suffix of a popup's held request (`POPUP_PATH` + code + this).
pub const POPUP_HOLD: &str = "/hold";
/// The variable that hands the plugin's `ask` popup its question's one-time code (0553), through herdr's `--env`.
pub const POPUP_ENV: &str = "JOCASTA_POPUP";
/// The header carrying a question's reader secret (0578): set on the first view read, required on its hold and answer.
pub const POPUP_READER_HEADER: &str = "x-jocasta-reader";
/// His thread keys (0544, 0554): `POST /api/plugin/action` with the service's token, body `{action, paneId?}`; answered
/// at once (202), the outcome follows as an `action` message and, when refused, a notification.
pub const PLUGIN_ACTION_PATH: &str = "/api/plugin/action";

/// The herdr plugin's actions (0553, 0554): Finish, Stop, Review and New thread on the focused thread; Quit closes Jocasta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginAction {
	#[serde(rename = "finish")]
	Finish,
	#[serde(rename = "stop")]
	Stop,
	#[serde(rename = "review")]
	Review,
	#[serde(rename = "new-thread")]
	NewThread,
	#[serde(rename = "quit")]
	Quit,
}

/// `PLUGIN_ACTIONS`.
pub const PLUGIN_ACTIONS: [PluginAction; 5] =
	[PluginAction::Finish, PluginAction::Stop, PluginAction::Review, PluginAction::NewThread, PluginAction::Quit];

/// A JavaScript number: a whole value goes out without a fraction (`1`, not `1.0`), as `JSON.stringify` writes it.
fn js_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
	if value.is_finite() && value.fract() == 0.0 && value.abs() < 9e15 {
		serializer.serialize_i64(*value as i64)
	} else {
		serializer.serialize_f64(*value)
	}
}

/// [`js_number`] for an optional field (skipped when absent).
fn js_number_opt<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
	match value {
		Some(value) => js_number(value, serializer),
		None => serializer.serialize_none(),
	}
}

/// `Math.round`: halves go up.
fn js_round(value: f64) -> f64 {
	let floor = value.floor();
	if value - floor >= 0.5 { floor + 1.0 } else { floor }
}

/// The new-thread form of a question (0554): the projects to start one in, the one picked first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupForm {
	pub projects: Vec<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub project: Option<String>,
}

/// One question for him (0434, 0484, 0519, 0558): its choices; `fallback` is Escape's and the answer when the question
/// goes unanswered (Jocasta stops, its wait runs out, the popup closes any other way).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupView {
	pub title: String,
	pub message: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
	pub choices: Vec<String>,
	pub fallback: String,
	/// Allow, Merge, PR and Stop everything (0558, 0568): three random letters he types to take any choice but the
	/// fallback; Enter and the other keys never take those. Absent for New thread, where Enter takes the selected choice.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub code: Option<String>,
	/// New thread (0554): the projects to start one in, the one picked first, and a brief he types.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub form: Option<PopupForm>,
}

/// A popup's answer (`POST /api/popup/<code>`): the choice, the letters he typed (a coded choice), the form's fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupAnswer {
	pub choice: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub typed: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub project: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub brief: Option<String>,
}

/// An open thread's state (0555).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadState {
	Working,
	Blocked,
	Done,
	Idle,
}

/// One thread row in `today` (0409, 0555): a herdr thread pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RailItem {
	pub pane_id: String,
	pub title: String,
	pub project: Option<String>,
	/// One plain line: what it is doing, its question, or "review ready".
	pub line: String,
	/// The titles of the project's other open threads that change the same files (0456); absent when none.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub overlaps: Option<Vec<String>>,
	/// An open thread's state, when it changed (epoch ms), its session's cost (USD) and context (tokens), for `today` (0555).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state: Option<ThreadState>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub since: Option<i64>,
	#[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "js_number_opt")]
	pub cost: Option<f64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub context: Option<u64>,
}

/// A `RailItem` whose pane may be gone: `Omit<RailItem, "paneId"> & { paneId?: string }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandedItem {
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pane_id: Option<String>,
	pub title: String,
	pub project: Option<String>,
	pub line: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub overlaps: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub state: Option<ThreadState>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub since: Option<i64>,
	#[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "js_number_opt")]
	pub cost: Option<f64>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub context: Option<u64>,
}

/// How long a thread has waited on him (0456), short: `25 min`, `3 h`, `2 d`; `1 min` at least.
pub fn wait_label(ms: i64) -> String {
	let minutes = ms.div_euclid(60_000).max(1);
	if minutes < 60 {
		return format!("{minutes} min");
	}
	if minutes < 60 * 24 {
		return format!("{} h", minutes / 60);
	}
	format!("{} d", minutes / (60 * 24))
}

/// A subscription window this used or more (0456) gets its one toast and shows high on the rail: the builder's default.
const QUOTA_WARN: f64 = 0.8;

/// A window's use as the rail and the HUD say it, a whole percent: 0.796 is 80.
pub fn quota_percent(q: &QuotaWindow) -> i64 {
	js_round(q.used * 100.0) as i64
}

/// Whether a window is past `QUOTA_WARN` (0456), judged on the percent he reads, so "80%" is always high: the rail's
/// colour and the toast follow this one rule.
pub fn quota_high(q: &QuotaWindow) -> bool {
	quota_percent(q) >= js_round(QUOTA_WARN * 100.0) as i64
}

/// One meeting of today's calendar (8.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEntry {
	pub time: String,
	pub title: String,
	/// The meeting's link (0429): the entry's URL, a link in its location, or a Teams/Meet/Zoom link in its description.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
	/// `HH:MM`; keeps a started meeting on `today` as "now" until it ends (0581).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub end: Option<String>,
}

/// What kind of row awaits him.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NeedsYouKind {
	Review,
	Pipeline,
	Blocked,
	Reviewed,
	Reminder,
	Disk,
}

/// One row of what awaits him (0432).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYou {
	pub kind: NeedsYouKind,
	/// One line, `<subject> — <what>`; a thread's is `<title> — <what>` with its project apart.
	pub text: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pane_id: Option<String>,
	/// A thread row's project.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub project: Option<String>,
	/// A reminder's time, `HH:MM`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub time: Option<String>,
	/// A reviewed thread's wait (0456): when its review was ready, ISO time.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub since: Option<String>,
	/// A thread row's overlapping open threads (0456), as `RailItem.overlaps`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub overlaps: Option<Vec<String>>,
}

/// An inbox note filed into a project's next work (0302).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextItem {
	pub id: i64,
	pub text: String,
	pub project: Option<String>,
}

/// The `today` view (0300) as the rail shows it: Awaiting you, Landed, Underway, Next (0409).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Today {
	/// The local day, `YYYY-MM-DD`.
	pub date: String,
	/// Today's meetings from the calendar (8.1), by start; all-day ones first with `time` "all day".
	pub calendar: Vec<CalendarEntry>,
	/// What awaits him (0432): reviews requested, red pipelines, threads awaiting him, threads reviewed and not finished
	/// (0456: oldest first, with `since`), inbox reminders due today (0302) and a nearly full disk. Every row counts in
	/// `Hud.awaiting`.
	pub needs_you: Vec<NeedsYou>,
	/// What landed since yesterday (0263, 0300): threads that finished a task and aren't reviewed yet (with their pane),
	/// then threads merged into trunk or sent as a PR, oldest first, whose pane is gone (no `paneId`).
	pub landed: Vec<LandedItem>,
	/// Open threads working, or idle with nothing for him (0555): with their state, time, cost and context.
	pub underway: Vec<RailItem>,
	/// Inbox notes filed into projects' next work (0302), oldest first.
	pub next: Vec<NextItem>,
}

/// One headline from brutalist.report (0429): title, source, when it was published and its link only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headline {
	pub title: String,
	pub source: String,
	pub url: String,
	/// Published, ISO time.
	pub at: String,
}

/// A feed's heading (0442): "Hacker News", "LWN", otherwise its source or topic capitalized ("Gaming").
pub fn news_heading(name: &str) -> String {
	match name {
		"hn" => "Hacker News".to_owned(),
		"lwn" => "LWN".to_owned(),
		_ => {
			let mut chars = name.chars();
			chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
		}
	}
}

/// One feed of `pollers.news.feeds` (0442): its heading and headlines, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsList {
	pub heading: String,
	pub items: Vec<Headline>,
}

/// The weather for `dashboard.weather` from Open-Meteo (0429).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
	/// The place as Open-Meteo's geocoding named it, e.g. "Example City, Example Country".
	pub place: String,
	#[serde(serialize_with = "js_number")]
	pub temp_c: f64,
	/// The WMO weather code.
	pub code: i64,
	/// The code in words, sentence case: "Partly cloudy".
	pub summary: String,
	#[serde(serialize_with = "js_number")]
	pub high_c: f64,
	#[serde(serialize_with = "js_number")]
	pub low_c: f64,
	/// Observed, ISO time.
	pub at: String,
}

/// Memory in bytes: used of total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ram {
	pub used: u64,
	pub total: u64,
}

/// The data dir's disk in bytes: free of total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disk {
	pub free: u64,
	pub total: u64,
}

/// CPU, RAM and the data dir's disk (0429); fractions and bytes, rounded so they change on whole percents.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Machine {
	/// Busy fraction of all cores since the last sample, 0..1.
	#[serde(serialize_with = "js_number")]
	pub cpu: f64,
	pub ram: Ram,
	pub disk: Disk,
}

/// A service Jocasta relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceName {
	Whiteboard,
	Herdr,
}

/// A service's state: `up` answers; `starting` while Jocasta starts it; `down` is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
	Up,
	Starting,
	Down,
}

/// A service Jocasta relies on (0429, 0444).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHealth {
	pub name: ServiceName,
	pub state: ServiceState,
	/// One plain line after the state, e.g. "not started", "not answering".
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub note: Option<String>,
}

/// A thread's worktree left on disk after its thread was closed in herdr (0456): its branch, commits not on the trunk
/// and changed files not committed. Removed only on his word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leftover {
	pub project: String,
	/// The worktree's folder, absolute.
	pub folder: PathBuf,
	pub branch: Option<String>,
	pub ahead: u32,
	pub dirty: u32,
}

/// An event line of the dashboard's activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
	pub at: String,
	pub text: String,
}

/// A compaction or handoff summary saved today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySummary {
	pub at: String,
	pub project: Option<String>,
	pub text: String,
}

/// Today's activity on the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
	/// The latest delivered `[event]` lines, latest first, at most 10.
	pub events: Vec<ActivityEvent>,
	/// Today's worklog so far, one line per project: `acme-api · 1h 20m · 3 commits`.
	pub worklog: Vec<String>,
	/// Compaction and handoff summaries saved today, latest first: their first line.
	pub summaries: Vec<ActivitySummary>,
}

/// The dashboard (0428, 0429): news, weather, machine and services, today's activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
	/// One list per feed in `pollers.news.feeds` (0442); the last good lists stay on failure.
	pub news: Vec<NewsList>,
	/// The news poller's last failure, one plain line; null once a round works.
	pub news_error: Option<String>,
	/// Null until `dashboard.weather` is set (the section is hidden) or the first round answered.
	pub weather: Option<Weather>,
	pub weather_error: Option<String>,
	/// Null until the first sample.
	pub machine: Option<Machine>,
	pub services: Vec<ServiceHealth>,
	pub activity: Activity,
	/// Thread worktrees left on disk (0456), by project and folder.
	pub leftovers: Vec<Leftover>,
}

/// A subscription window's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaSpan {
	#[serde(rename = "5h")]
	FiveHours,
	#[serde(rename = "7d")]
	SevenDays,
}

/// One subscription window from `omp usage --json` (0456): the most used of that window across his accounts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
	pub window: QuotaSpan,
	/// Fraction used, 0..1 (above 1 past the limit).
	#[serde(serialize_with = "js_number")]
	pub used: f64,
	/// When the window resets, ISO time, when omp knows it.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub resets_at: Option<String>,
}

/// The HUD figures (0403): the same ones `<data>/hud.txt` carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud {
	/// Thread panes working.
	pub running: u32,
	/// Thread panes waiting on him.
	pub blocked: u32,
	/// Every row of `today.needsYou` (0432): the count `today` and `<data>/hud.txt` say.
	pub awaiting: u32,
	/// Today's spend in USD, summed from today's assistant messages in omp's session files (local day); null while the
	/// service is still reading today's sessions after its start.
	pub spend_today: Option<f64>,
	/// The subscription windows (0456), 5h then 7d; empty until `omp usage` answered with them.
	pub quota: Vec<QuotaWindow>,
	/// Whether quiet hours are on now (0298, 0581): `today`'s header says so.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub quiet: Option<bool>,
}

/// Why a notification was raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastReason {
	Blocked,
	Done,
	Failed,
	Event,
}

/// A notification's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastLevel {
	Info,
	Warning,
	Error,
}

/// A notification's priority, for `event` (0299).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastPriority {
	Urgent,
	Normal,
}

/// Service → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ServerMessage {
	/// Sent on connect: everything `today` shows (0409).
	Snapshot {
		hud: Hud,
		today: Today,
		/// The dashboard (0428, 0429).
		dashboard: Dashboard,
	},
	/// The dashboard changed (0429): pushed only when it differs from the last one sent.
	Dashboard {
		dashboard: Dashboard,
	},
	Hud {
		hud: Hud,
	},
	/// A notification (0407): raised in Windows through herdr (0554), urgent and blocked ones with herdr's request
	/// sound; `pane_id` names the pane it is about.
	Toast {
		reason: ToastReason,
		level: ToastLevel,
		text: String,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		pane_id: Option<String>,
		/// For `event` (0299): urgent goes through quiet hours, and sounds like a blocked thread.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		priority: Option<ToastPriority>,
		#[serde(default, skip_serializing_if = "Option::is_none")]
		url: Option<String>,
	},
	/// The `today` view (0300): pushed while it changes.
	Today {
		today: Today,
	},
	/// What one of his thread keys did, or why it didn't (0544, 0554): `today` shows the last one.
	Action {
		text: String,
		is_error: bool,
	},
	Error {
		message: String,
	},
	/// The service is stopping (Jocasta closing as one; 0434).
	Bye,
}

/// Client → service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum ClientMessage {
	/// A row was chosen: focus that pane in herdr (0407).
	#[serde(rename = "pane.focus")]
	PaneFocus { pane_id: String },
	/// A link row was chosen (0581): the service opens `url` in his browser only when it is one of the links of its
	/// current `today` or dashboard, and http(s).
	#[serde(rename = "link.open")]
	LinkOpen { url: String },
}

#[cfg(test)]
mod tests {
	use serde::de::DeserializeOwned;
	use serde_json::{Value, json};

	use super::*;

	/// Serializes `value`, checks it equals `expected` (the JSON the TypeScript sends) and reads it back.
	fn wire<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T, expected: &Value) {
		assert_eq!(&serde_json::to_value(value).unwrap(), expected);
		assert_eq!(&serde_json::from_value::<T>(expected.clone()).unwrap(), value);
		let text = serde_json::to_string(value).unwrap();
		assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), *expected);
	}

	fn hud() -> (Hud, Value) {
		(
			Hud {
				running: 1,
				blocked: 2,
				awaiting: 3,
				spend_today: None,
				quota: vec![
					QuotaWindow {
						window: QuotaSpan::FiveHours,
						used: 0.796,
						resets_at: Some("2026-10-08T14:00:00.000Z".to_owned()),
					},
					QuotaWindow { window: QuotaSpan::SevenDays, used: 1.0, resets_at: None },
				],
				quiet: Some(true),
			},
			json!({
				"running": 1, "blocked": 2, "awaiting": 3, "spendToday": null,
				"quota": [{"window": "5h", "used": 0.796, "resetsAt": "2026-10-08T14:00:00.000Z"}, {"window": "7d", "used": 1}],
				"quiet": true,
			}),
		)
	}

	fn today() -> (Today, Value) {
		let rail = RailItem {
			pane_id: "p1".to_owned(),
			title: "Fix login".to_owned(),
			project: Some("acme-api".to_owned()),
			line: "working".to_owned(),
			overlaps: Some(vec!["Add SSO".to_owned()]),
			state: Some(ThreadState::Working),
			since: Some(1_791_450_000_000),
			cost: Some(1.25),
			context: Some(48_000),
		};
		let today = Today {
			date: "2026-10-08".to_owned(),
			calendar: vec![
				CalendarEntry { time: "all day".to_owned(), title: "Offsite".to_owned(), url: None, end: None },
				CalendarEntry {
					time: "10:00".to_owned(),
					title: "Standup".to_owned(),
					url: Some("https://meet.example.org/x".to_owned()),
					end: Some("10:15".to_owned()),
				},
			],
			needs_you: vec![
				NeedsYou {
					kind: NeedsYouKind::Review,
					text: "acme/api #7 — review requested".to_owned(),
					url: Some("https://github.com/acme/api/pull/7".to_owned()),
					pane_id: None,
					project: None,
					time: None,
					since: None,
					overlaps: None,
				},
				NeedsYou {
					kind: NeedsYouKind::Reviewed,
					text: "Fix login — review ready".to_owned(),
					url: None,
					pane_id: Some("p1".to_owned()),
					project: Some("acme-api".to_owned()),
					time: Some("15:00".to_owned()),
					since: Some("2026-10-08T09:00:00.000Z".to_owned()),
					overlaps: Some(vec!["Add SSO".to_owned()]),
				},
			],
			landed: vec![LandedItem {
				pane_id: None,
				title: "Old".to_owned(),
				project: None,
				line: "merged".to_owned(),
				overlaps: None,
				state: None,
				since: None,
				cost: None,
				context: None,
			}],
			underway: vec![rail],
			next: vec![NextItem { id: 4, text: "try the new parser".to_owned(), project: Some("acme-api".to_owned()) }],
		};
		let json = json!({
			"date": "2026-10-08",
			"calendar": [
				{"time": "all day", "title": "Offsite"},
				{"time": "10:00", "title": "Standup", "url": "https://meet.example.org/x", "end": "10:15"},
			],
			"needsYou": [
				{"kind": "review", "text": "acme/api #7 — review requested", "url": "https://github.com/acme/api/pull/7"},
				{"kind": "reviewed", "text": "Fix login — review ready", "paneId": "p1", "project": "acme-api", "time": "15:00",
				 "since": "2026-10-08T09:00:00.000Z", "overlaps": ["Add SSO"]},
			],
			"landed": [{"title": "Old", "project": null, "line": "merged"}],
			"underway": [{
				"paneId": "p1", "title": "Fix login", "project": "acme-api", "line": "working", "overlaps": ["Add SSO"],
				"state": "working", "since": 1_791_450_000_000_i64, "cost": 1.25, "context": 48000,
			}],
			"next": [{"id": 4, "text": "try the new parser", "project": "acme-api"}],
		});
		(today, json)
	}

	fn dashboard() -> (Dashboard, Value) {
		let dashboard = Dashboard {
			news: vec![NewsList {
				heading: "Hacker News".to_owned(),
				items: vec![Headline {
					title: "A post".to_owned(),
					source: "hn".to_owned(),
					url: "https://example.org/p".to_owned(),
					at: "2026-10-08T08:00:00.000Z".to_owned(),
				}],
			}],
			news_error: None,
			weather: Some(Weather {
				place: "Example City, Example Country".to_owned(),
				temp_c: 12.0,
				code: 2,
				summary: "Partly cloudy".to_owned(),
				high_c: 15.5,
				low_c: 7.0,
				at: "2026-10-08T09:00:00.000Z".to_owned(),
			}),
			weather_error: Some("timed out".to_owned()),
			machine: Some(Machine {
				cpu: 0.0,
				ram: Ram { used: 8_589_934_592, total: 17_179_869_184 },
				disk: Disk { free: 100, total: 200 },
			}),
			services: vec![
				ServiceHealth {
					name: ServiceName::Whiteboard,
					state: ServiceState::Down,
					note: Some("not started".to_owned()),
				},
				ServiceHealth { name: ServiceName::Herdr, state: ServiceState::Up, note: None },
			],
			activity: Activity {
				events: vec![ActivityEvent {
					at: "2026-10-08T09:00:00.000Z".to_owned(),
					text: "pipeline red".to_owned(),
				}],
				worklog: vec!["acme-api · 1h 20m · 3 commits".to_owned()],
				summaries: vec![ActivitySummary {
					at: "2026-10-08T09:30:00.000Z".to_owned(),
					project: None,
					text: "handoff".to_owned(),
				}],
			},
			leftovers: vec![Leftover {
				project: "acme-api".to_owned(),
				folder: PathBuf::from("D:/work/acme-api-wt"),
				branch: None,
				ahead: 2,
				dirty: 0,
			}],
		};
		let json = json!({
			"news": [{"heading": "Hacker News", "items": [{"title": "A post", "source": "hn", "url": "https://example.org/p", "at": "2026-10-08T08:00:00.000Z"}]}],
			"newsError": null,
			"weather": {"place": "Example City, Example Country", "tempC": 12, "code": 2, "summary": "Partly cloudy", "highC": 15.5, "lowC": 7, "at": "2026-10-08T09:00:00.000Z"},
			"weatherError": "timed out",
			"machine": {"cpu": 0, "ram": {"used": 8589934592_u64, "total": 17179869184_u64}, "disk": {"free": 100, "total": 200}},
			"services": [{"name": "whiteboard", "state": "down", "note": "not started"}, {"name": "herdr", "state": "up"}],
			"activity": {
				"events": [{"at": "2026-10-08T09:00:00.000Z", "text": "pipeline red"}],
				"worklog": ["acme-api · 1h 20m · 3 commits"],
				"summaries": [{"at": "2026-10-08T09:30:00.000Z", "project": null, "text": "handoff"}],
			},
			"leftovers": [{"project": "acme-api", "folder": "D:/work/acme-api-wt", "branch": null, "ahead": 2, "dirty": 0}],
		});
		(dashboard, json)
	}

	#[test]
	fn the_cookie_is_named_after_the_port_and_the_paths_are_the_services() {
		assert_eq!(token_cookie(5173), "jocasta_service_5173");
		assert_eq!((WS_PATH, TOKEN_QUERY, POPUP_PATH, POPUP_HOLD), ("/ws", "t", "/api/popup/", "/hold"));
		assert_eq!(
			(POPUP_ENV, POPUP_READER_HEADER, PLUGIN_ACTION_PATH),
			("JOCASTA_POPUP", "x-jocasta-reader", "/api/plugin/action")
		);
	}

	#[test]
	fn the_plugin_actions_are_finish_stop_review_new_thread_and_quit() {
		let names: Vec<Value> = PLUGIN_ACTIONS.iter().map(|action| serde_json::to_value(action).unwrap()).collect();
		assert_eq!(names, json!(["finish", "stop", "review", "new-thread", "quit"]).as_array().unwrap().clone());
		assert_eq!(serde_json::from_value::<PluginAction>(json!("new-thread")).unwrap(), PluginAction::NewThread);
	}

	#[test]
	fn a_question_and_its_answer_keep_the_typescripts_keys() {
		let view = PopupView {
			title: "Allow?".to_owned(),
			message: "Push to main".to_owned(),
			detail: Some("acme-api".to_owned()),
			choices: vec!["Allow".to_owned(), "Deny".to_owned()],
			fallback: "Deny".to_owned(),
			code: Some("qwx".to_owned()),
			form: None,
		};
		wire(
			&view,
			&json!({"title": "Allow?", "message": "Push to main", "detail": "acme-api", "choices": ["Allow", "Deny"], "fallback": "Deny", "code": "qwx"}),
		);
		let new_thread = PopupView {
			title: "New thread".to_owned(),
			message: String::new(),
			detail: None,
			choices: vec!["Start".to_owned(), "Cancel".to_owned()],
			fallback: "Cancel".to_owned(),
			code: None,
			form: Some(PopupForm { projects: vec!["a".to_owned(), "b".to_owned()], project: Some("b".to_owned()) }),
		};
		wire(
			&new_thread,
			&json!({"title": "New thread", "message": "", "choices": ["Start", "Cancel"], "fallback": "Cancel", "form": {"projects": ["a", "b"], "project": "b"}}),
		);
		wire(
			&PopupAnswer { choice: "Allow".to_owned(), typed: Some("qwx".to_owned()), project: None, brief: None },
			&json!({"choice": "Allow", "typed": "qwx"}),
		);
		wire(
			&PopupAnswer {
				choice: "Start".to_owned(),
				typed: None,
				project: Some("b".to_owned()),
				brief: Some("do it".to_owned()),
			},
			&json!({"choice": "Start", "project": "b", "brief": "do it"}),
		);
	}

	#[test]
	fn the_snapshot_message_carries_the_hud_today_and_dashboard() {
		let (hud, hud_json) = hud();
		let (today, today_json) = today();
		let (dashboard, dashboard_json) = dashboard();
		wire(
			&ServerMessage::Snapshot { hud, today, dashboard },
			&json!({"type": "snapshot", "hud": hud_json, "today": today_json, "dashboard": dashboard_json}),
		);
	}

	#[test]
	fn the_dashboard_hud_and_today_messages_carry_their_one_field() {
		let (dashboard, dashboard_json) = dashboard();
		wire(&ServerMessage::Dashboard { dashboard }, &json!({"type": "dashboard", "dashboard": dashboard_json}));
		let (hud, hud_json) = hud();
		wire(&ServerMessage::Hud { hud }, &json!({"type": "hud", "hud": hud_json}));
		let (today, today_json) = today();
		wire(&ServerMessage::Today { today }, &json!({"type": "today", "today": today_json}));
	}

	#[test]
	fn a_toast_leaves_out_what_it_has_no_value_for() {
		wire(
			&ServerMessage::Toast {
				reason: ToastReason::Blocked,
				level: ToastLevel::Warning,
				text: "Fix login is blocked".to_owned(),
				pane_id: Some("p1".to_owned()),
				priority: None,
				url: None,
			},
			&json!({"type": "toast", "reason": "blocked", "level": "warning", "text": "Fix login is blocked", "paneId": "p1"}),
		);
		wire(
			&ServerMessage::Toast {
				reason: ToastReason::Event,
				level: ToastLevel::Error,
				text: "pipeline red".to_owned(),
				pane_id: None,
				priority: Some(ToastPriority::Urgent),
				url: Some("https://example.org/run/1".to_owned()),
			},
			&json!({"type": "toast", "reason": "event", "level": "error", "text": "pipeline red", "priority": "urgent", "url": "https://example.org/run/1"}),
		);
		for (reason, name) in [(ToastReason::Done, "done"), (ToastReason::Failed, "failed")] {
			assert_eq!(serde_json::to_value(reason).unwrap(), json!(name));
		}
		assert_eq!(serde_json::to_value(ToastLevel::Info).unwrap(), json!("info"));
		assert_eq!(serde_json::to_value(ToastPriority::Normal).unwrap(), json!("normal"));
	}

	#[test]
	fn action_error_and_bye_messages() {
		wire(
			&ServerMessage::Action { text: "Finished Fix login".to_owned(), is_error: false },
			&json!({"type": "action", "text": "Finished Fix login", "isError": false}),
		);
		wire(
			&ServerMessage::Error { message: "bad frame".to_owned() },
			&json!({"type": "error", "message": "bad frame"}),
		);
		wire(&ServerMessage::Bye, &json!({"type": "bye"}));
	}

	#[test]
	fn the_clients_messages_are_a_focus_and_a_link() {
		wire(&ClientMessage::PaneFocus { pane_id: "p1".to_owned() }, &json!({"type": "pane.focus", "paneId": "p1"}));
		wire(
			&ClientMessage::LinkOpen { url: "https://example.org".to_owned() },
			&json!({"type": "link.open", "url": "https://example.org"}),
		);
	}

	#[test]
	fn whole_numbers_go_out_as_javascript_writes_them() {
		let (hud, _) = hud();
		let text = serde_json::to_string(&hud.quota[1]).unwrap();
		assert_eq!(text, r#"{"window":"7d","used":1}"#);
		let weather = serde_json::to_string(&dashboard().0.weather).unwrap();
		assert!(
			weather.contains(r#""tempC":12,"#)
				&& weather.contains(r#""highC":15.5,"#)
				&& weather.contains(r#""lowC":7,"#)
		);
	}

	#[test]
	fn a_wait_is_said_in_minutes_hours_or_days_one_minute_at_least() {
		let minute = 60_000;
		for (ms, label) in [
			(0, "1 min"),
			(-5, "1 min"),
			(59_999, "1 min"),
			(25 * minute, "25 min"),
			(59 * minute, "59 min"),
			(60 * minute, "1 h"),
			(3 * 60 * minute + 59 * minute, "3 h"),
			(24 * 60 * minute, "1 d"),
			(2 * 24 * 60 * minute + 5 * minute, "2 d"),
		] {
			assert_eq!(wait_label(ms), label, "{ms}");
		}
	}

	#[test]
	fn quota_is_a_whole_percent_and_high_from_80() {
		let window = |used| QuotaWindow { window: QuotaSpan::FiveHours, used, resets_at: None };
		assert_eq!(quota_percent(&window(0.796)), 80);
		assert_eq!(quota_percent(&window(0.5)), 50);
		assert_eq!(quota_percent(&window(0.004)), 0);
		assert_eq!(quota_percent(&window(0.005)), 1);
		assert!(quota_high(&window(0.796)));
		assert!(quota_high(&window(0.8)));
		assert!(quota_high(&window(1.2)));
		assert!(!quota_high(&window(0.794)));
	}

	#[test]
	fn a_feeds_heading_says_hn_and_lwn_in_words_and_capitalizes_the_rest() {
		assert_eq!(news_heading("hn"), "Hacker News");
		assert_eq!(news_heading("lwn"), "LWN");
		assert_eq!(news_heading("gaming"), "Gaming");
		assert_eq!(news_heading("Already"), "Already");
		assert_eq!(news_heading(""), "");
	}
}
