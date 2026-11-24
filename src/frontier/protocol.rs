//! The service's state feed and its messages to herdr's client shell (4.1, 0409, 0552; fork plan phase 5): typed JSON
//! messages from the Jocasta service to a client and the questions it asks him. The service owns this file.
//!
//! Transport: herdr's client shell gets them over herdr's own client connection (the `SHELL_*` kinds below); another
//! client (the benchmark's) opens `ws://127.0.0.1:<port>/ws` with the service's token as `Authorization: Bearer <token>`
//! (0254; a browser page has no socket and no API token, SEC-NET-4). Every feed message is one JSON object with a
//! `type`. A question goes to one client only, never broadcast (0519, 0578).

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

/// What herdr's client shell and the service trade over herdr's client connection (fork plan phase 5), as
/// `EndpointControl` messages of these kinds, each with JSON data. The service sends [`SHELL_FEED`] (a
/// [`ServerMessage`]) and [`SHELL_BOSS`] (the boss's workspace id) to every shell; [`SHELL_QUESTION`] to the question's
/// one reader (0578); [`SHELL_ANSWERED`] to that reader once it answered, and [`SHELL_GONE`] once the question is over.
/// A shell sends [`SHELL_FEED`] (a [`ClientMessage`]), [`SHELL_ACTION`] and [`SHELL_ANSWER`].
pub const SHELL_FEED: &str = "jocasta.feed";
/// The boss's workspace id, for the dock.
pub const SHELL_BOSS: &str = "jocasta.boss";
/// A [`ShellQuestion`] for its reader.
pub const SHELL_QUESTION: &str = "jocasta.question";
/// The code of a question that is over, for every shell.
pub const SHELL_GONE: &str = "jocasta.question.gone";
/// A shell's [`ShellAnswer`].
pub const SHELL_ANSWER: &str = "jocasta.answer";
/// The service's [`ShellAnswered`] to the reader that answered.
pub const SHELL_ANSWERED: &str = "jocasta.answered";
/// A shell's [`ShellAction`]: one of his thread keys.
pub const SHELL_ACTION: &str = "jocasta.action";

/// A question for its reader's shell: its one-time code (it names the question in the answer) and its texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellQuestion {
	pub code: String,
	pub view: PopupView,
}

/// His answer to question `code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellAnswer {
	pub code: String,
	pub answer: PopupAnswer,
}

/// The service's reply to an answer (0578): taken; a wrong code with the tries left; the third wrong code, which took
/// the fallback; or why it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AnswerReply {
	Taken,
	Wrong { left: Option<u64> },
	Exhausted,
	Failed { text: String },
}

/// The reply to the answer to question `code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellAnswered {
	pub code: String,
	pub reply: AnswerReply,
}

/// One of his thread keys from the dock (0544, 0554): the action and the picked thread's pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellAction {
	pub action: PluginAction,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pane_id: Option<String>,
}

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
	/// Every running turn ends, the boss's too (0686).
	#[serde(rename = "stop-all")]
	StopAll,
	#[serde(rename = "quit")]
	Quit,
}

/// `PLUGIN_ACTIONS`.
pub const PLUGIN_ACTIONS: [PluginAction; 6] = [
	PluginAction::Finish,
	PluginAction::Stop,
	PluginAction::Review,
	PluginAction::NewThread,
	PluginAction::StopAll,
	PluginAction::Quit,
];

/// A JavaScript number: a whole value goes out without a fraction (`1`, not `1.0`), as `JSON.stringify` writes it.
pub fn js_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
	if value.is_finite() && value.fract() == 0.0 && value.abs() < 9e15 {
		serializer.serialize_i64(*value as i64)
	} else {
		serializer.serialize_f64(*value)
	}
}

/// A number as JavaScript's `Number.prototype.toString` writes it: the shortest digits that read back as the same
/// double, whole values up to 1e21 without an exponent; `null` for NaN and the infinities, as `JSON.stringify` writes
/// them. The one formatter of the Rust build, for text that carries a number (messages, the bench's rows, the files
/// the TypeScript build wrote with `JSON.stringify` or a template).
pub fn js_number_text(x: f64) -> String {
	if !x.is_finite() {
		return "null".to_owned();
	}
	if x == 0.0 {
		return "0".to_owned();
	}
	// The shortest digits that round-trip, as `d.ddde±x`; the decimal point sits `n` digits in (value = 0.digits × 10^n).
	let scientific = format!("{:e}", x.abs());
	let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
	let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
	let n: i32 = exponent.parse::<i32>().unwrap_or(0) + 1;
	let k = digits.len() as i32;
	let body = if k <= n && n <= 21 {
		format!("{digits}{}", "0".repeat((n - k) as usize))
	} else if 0 < n && n <= 21 {
		format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
	} else if -6 < n && n <= 0 {
		format!("0.{}{digits}", "0".repeat((-n) as usize))
	} else {
		let e = n - 1;
		let sign = if e < 0 { '-' } else { '+' };
		if k == 1 {
			format!("{digits}e{sign}{}", e.abs())
		} else {
			format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
		}
	};
	if x < 0.0 { format!("-{body}") } else { body }
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

/// Lines added and removed in a thread's worktree (0668).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadDiff {
	pub added: u32,
	pub removed: u32,
}

/// A PR's CI state (0668).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrCi {
	Pending,
	Passing,
	Failing,
	Merged,
	Closed,
}

/// A PR's number and CI state (0668).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrFacts {
	pub number: u64,
	pub ci: PrCi,
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
	/// Lines added and removed in its worktree against trunk (0668).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub diff: Option<ThreadDiff>,
	/// The last check: `Some(true)` passed, `Some(false)` failed, `None` none yet (0668).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub check: Option<bool>,
	/// Ms with no write to its session file, only when stuck (0668).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stuck_for: Option<i64>,
	/// Why the loop cap or the cost cap stopped it (0677, 0686): `stuck: quick failed 3×`, `over budget: $5.10 of $5`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub halted: Option<String>,
	/// Its todo count and context use, `4/9 · 62%`; a part that is unknown is left out (0677).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub progress: Option<String>,
	/// Its cost cap in dollars (0686): the row reads `$spent/$budget`.
	#[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "js_number_opt")]
	pub budget: Option<f64>,
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
	/// The PR a finished thread was sent as (0668).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pr: Option<PrFacts>,
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
	/// A working thread whose session file was not written for a long while (0668).
	Stuck,
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

/// A scheduled thread's next run (0673).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledItem {
	pub project: String,
	pub brief: String,
	pub at_ms: i64,
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
	/// The soonest scheduled threads (0673), at most 3.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub scheduled: Vec<ScheduledItem>,
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

/// One logged-in account's windows (0669): its name is the redacted email, else the account id, else `#<credential>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAccount {
	pub name: String,
	#[serde(default, skip_serializing_if = "Option::is_none", rename = "5h")]
	pub five_h: Option<QuotaWindow>,
	#[serde(default, skip_serializing_if = "Option::is_none", rename = "7d")]
	pub seven_d: Option<QuotaWindow>,
}

/// What is left of one window across the accounts (0669).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaCapacity {
	pub window: QuotaSpan,
	pub remaining: u32,
	pub accounts: u32,
}

/// The check queue's load (0671).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksLoad {
	pub running: u32,
	pub waiting: u32,
}

/// The costliest thread of today (0668).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopThread {
	pub title: String,
	#[serde(serialize_with = "js_number")]
	pub cost: f64,
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
	/// The mean of the recorded prior days' spend (0668, up to 7); null with none recorded.
	#[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "js_number_opt")]
	pub spend_avg_7d: Option<f64>,
	/// The thread with the highest cost today (0668).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub top_thread: Option<TopThread>,
	/// Every logged-in account's 5h and 7d use (0669).
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub accounts: Vec<QuotaAccount>,
	/// What is left per window across the accounts (0669).
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub capacity: Vec<QuotaCapacity>,
	/// The check queue (0671); present only while a check runs or waits.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub checks: Option<ChecksLoad>,
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
// A wire message: built, serialized and dropped at once, so its size on the stack doesn't matter.
#[allow(clippy::large_enum_variant)]
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
	/// The service is up and its refresh read the day's state (0662): every 10 s at most; the dock calls the service
	/// silent after 30 s without a message.
	Alive,
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
	/// A leftover's `x` key (0670): the service removes the worktree folder only when it is one of the current
	/// `dashboard.leftovers`, and only behind his Allow.
	#[serde(rename = "leftover.remove")]
	LeftoverRemove { folder: PathBuf },
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

	#[test]
	fn js_number_text_writes_a_number_as_number_prototype_to_string_does() {
		let cases = [
			(0.0, "0"),
			(-0.0, "0"),
			(1.0, "1"),
			(-2.5, "-2.5"),
			(0.1, "0.1"),
			(120_000.0, "120000"),
			(1e21, "1e+21"),
			(123_456_789_012_345_680.0, "123456789012345680"),
			(1.5e-7, "1.5e-7"),
			(0.000_001, "0.000001"),
			(f64::NAN, "null"),
			(f64::INFINITY, "null"),
		];
		for (value, text) in cases {
			assert_eq!(js_number_text(value), text, "{value}");
		}
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
				spend_avg_7d: Some(4.5),
				top_thread: Some(TopThread { title: "Fix login".to_owned(), cost: 2.0 }),
				accounts: vec![QuotaAccount {
					name: "zi*".to_owned(),
					five_h: Some(QuotaWindow { window: QuotaSpan::FiveHours, used: 0.42, resets_at: None }),
					seven_d: None,
				}],
				capacity: vec![QuotaCapacity { window: QuotaSpan::FiveHours, remaining: 2, accounts: 3 }],
				checks: Some(ChecksLoad { running: 1, waiting: 2 }),
			},
			json!({
				"running": 1, "blocked": 2, "awaiting": 3, "spendToday": null,
				"quota": [{"window": "5h", "used": 0.796, "resetsAt": "2026-10-08T14:00:00.000Z"}, {"window": "7d", "used": 1}],
				"quiet": true,
				"spendAvg7d": 4.5,
				"topThread": {"title": "Fix login", "cost": 2},
				"accounts": [{"name": "zi*", "5h": {"window": "5h", "used": 0.42}}],
				"capacity": [{"window": "5h", "remaining": 2, "accounts": 3}],
				"checks": {"running": 1, "waiting": 2},
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
			diff: Some(ThreadDiff { added: 84, removed: 12 }),
			check: Some(false),
			stuck_for: Some(660_000),
			halted: None,
			progress: None,
			budget: None,
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
				pr: Some(PrFacts { number: 7, ci: PrCi::Passing }),
			}],
			underway: vec![rail],
			next: vec![NextItem { id: 4, text: "try the new parser".to_owned(), project: Some("acme-api".to_owned()) }],
			scheduled: vec![ScheduledItem {
				project: "acme-api".to_owned(),
				brief: "dependency bumps".to_owned(),
				at_ms: 1_791_500_000_000,
			}],
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
			"landed": [{"title": "Old", "project": null, "line": "merged", "pr": {"number": 7, "ci": "passing"}}],
			"underway": [{
				"paneId": "p1", "title": "Fix login", "project": "acme-api", "line": "working", "overlaps": ["Add SSO"],
				"state": "working", "since": 1_791_450_000_000_i64, "cost": 1.25, "context": 48000,
				"diff": {"added": 84, "removed": 12}, "check": false, "stuckFor": 660_000,
			}],
			"next": [{"id": 4, "text": "try the new parser", "project": "acme-api"}],
			"scheduled": [{"project": "acme-api", "brief": "dependency bumps", "atMs": 1_791_500_000_000_i64}],
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
			services: vec![ServiceHealth {
				name: ServiceName::Herdr,
				state: ServiceState::Down,
				note: Some("not started".to_owned()),
			}],
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
			"services": [{"name": "herdr", "state": "down", "note": "not started"}],
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
	fn the_cookie_is_named_after_the_port_and_the_shell_kinds_carry_the_prefix_herdr_routes_by() {
		assert_eq!(token_cookie(5173), "jocasta_service_5173");
		assert_eq!((WS_PATH, TOKEN_QUERY), ("/ws", "t"));
		let kinds = [SHELL_FEED, SHELL_BOSS, SHELL_QUESTION, SHELL_GONE, SHELL_ANSWER, SHELL_ANSWERED, SHELL_ACTION];
		assert!(kinds.iter().all(|kind| kind.starts_with(herdr::embed::KIND_PREFIX)));
		wire(&AnswerReply::Wrong { left: Some(2) }, &json!({"type": "wrong", "left": 2}));
		wire(&AnswerReply::Failed { text: "x".to_owned() }, &json!({"type": "failed", "text": "x"}));
		wire(
			&ShellAction { action: PluginAction::Stop, pane_id: Some("w1:p1".to_owned()) },
			&json!({"action": "stop", "paneId": "w1:p1"}),
		);
	}

	#[test]
	fn the_plugin_actions_are_finish_stop_review_new_thread_stop_all_and_quit() {
		let names: Vec<Value> = PLUGIN_ACTIONS.iter().map(|action| serde_json::to_value(action).unwrap()).collect();
		assert_eq!(
			names,
			json!(["finish", "stop", "review", "new-thread", "stop-all", "quit"]).as_array().unwrap().clone()
		);
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
		wire(&ServerMessage::Alive, &json!({"type": "alive"}));
	}

	#[test]
	fn the_clients_messages_are_a_focus_a_link_and_a_leftover_removal() {
		wire(&ClientMessage::PaneFocus { pane_id: "p1".to_owned() }, &json!({"type": "pane.focus", "paneId": "p1"}));
		wire(
			&ClientMessage::LinkOpen { url: "https://example.org".to_owned() },
			&json!({"type": "link.open", "url": "https://example.org"}),
		);
		wire(
			&ClientMessage::LeftoverRemove { folder: PathBuf::from("acme-api/fix-login") },
			&json!({"type": "leftover.remove", "folder": "acme-api/fix-login"}),
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
