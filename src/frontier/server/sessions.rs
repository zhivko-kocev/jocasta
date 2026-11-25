//! Reading omp's session files (F1 guide §5): `~/.omp/agent/sessions/<cwd dir>/<stem>.jsonl`; line 1 is a `title`
//! entry, the `session` header with `cwd` follows; assistant messages are `{type: "message", message: {role:
//! "assistant", content, model, usage: {cost: {total}}, timestamp}}`, written once when finished. Subagent and advisor
//! transcripts sit in the session's artifacts dir (`<stem>/<agentId>.jsonl`, `<stem>/__advisor.jsonl`, nested deeper
//! for a subagent's own): they are not sessions, but their cost is the session's (CR-04; `omp stats` counts them the
//! same way, `@oh-my-pi/omp-stats` `parser.ts` `classifyAgentType`, `listSessionFiles`).
//!
//! omp's entry kinds (0464; checked against omp 19.2.0, 0663 and 0690): `model_usage` entries (side
//! calls: memory extraction, cache warming, advisor, judges, session titles; `session-entries.ts:183` `ModelUsageEntry`)
//! count toward spend like
//! assistant turns (0452); an assistant turn marked `retryRecovery` (an attempt a retry superseded, pi-ai
//! `types.ts:1390`) still costs, but never gives the last line, the question or the test result; `memory_recall`
//! entries are ignored; a session moved to a sibling file (its header's `parentSession` names the old one,
//! `session-manager.ts` `#moveOffSessionFile`) is followed there ([`SessionIndex::moved`]), and the entries it copied
//! from the old file aren't counted twice. Entries a file holds without ids are given ids when omp next loads it, and
//! the file is rewritten (`session-migrations.ts:64-77`); the spend ledger keys a call by its ISO time, not its
//! id.
//!
//! A session is a tree (0509): each entry names its `parentId`, and a rewind (`session-manager.ts` `branch`,
//! `branchWithSummary`) leaves the abandoned turns in the file. Cost, spend and message times count every entry in file
//! order, as abandoned turns were billed. The state (model, last line, question, plan, test result, bash runs, handoff
//! time) follows the active branch as omp replays it: each entry's `activeLeafId`, else its id, is the leaf
//! (`session-entries.ts` `sessionEntryLeafId`; an unknown one falls back to the newest entry selecting itself,
//! `lastSelfSelectedLeafId`), and the branch is the walk back from the leaf by `parentId` (`getBranch`). An entry
//! extending the leaf applies at once, an off-branch entry (the leaf unchanged) changes nothing, and any other leaf move
//! replays the branch from its root; each entry's parent and state effect are kept for that. Entries without an id or a
//! `parentId` apply in file order.
//!
//! Files are read incrementally (CR-03): each keeps its byte offset and running stats, and only appended lines are
//! parsed (`omp_session.rs` `SessionTail`). A file checked under a second ago isn't looked at again, and an active
//! session's artifacts dir is re-listed only when the session changed or every 30 s (PM-6, PM-17).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{Map, Value};

use crate::frontier::server::ask_question::ask_question;
use crate::shared::day::{day_start, local_day};
use crate::shared::fsx::as_object;
use crate::shared::jsonl::json_lines;
use crate::shared::ledger::forget_stale_sessions;
use crate::shared::omp_session::{
	FileStat, SessionHeader, SessionTail, SessionWalker, TailVisitor, artifacts_dir, date_parse, read_header,
	transcripts_of,
};
use crate::shared::paths::omp_agent_dir;

/// omp's sessions dir: `<omp agent dir>/sessions` (guide §5, `docs/config-usage.md:80`).
fn omp_sessions_dir() -> PathBuf {
	omp_agent_dir(None).join("sessions")
}

/// The index's clock: epoch ms, as `Date.now()` (tests move their own).
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The wall clock, as `Date.now()`.
pub fn system_clock() -> Clock {
	Arc::new(|| {
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
	})
}

/// One session file with its header.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionFile {
	pub file: PathBuf,
	pub cwd: String,
	pub title: Option<String>,
	pub id: Option<String>,
	pub parent_session: Option<String>,
	pub started: Option<i64>,
	pub mtime_ms: i64,
}

impl SessionFile {
	fn new(header: &SessionHeader, mtime_ms: f64) -> Self {
		SessionFile {
			file: header.file.clone(),
			cwd: header.cwd.clone(),
			title: header.title.clone(),
			id: header.id.clone(),
			parent_session: header.parent_session.clone(),
			started: header.started,
			mtime_ms: mtime_ms as i64,
		}
	}
}

/// How a bash call ended (0453): exit 0, a failure, or still running in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashOutcome {
	Ok,
	Fail,
	Background,
}

/// One bash call of a session and how it ended (0453): exit 0, a failure, or still running in the background; `exit`
/// its exit code (omp's `details.exitCode` on a failed call), `None` when the result doesn't say.
#[derive(Debug, Clone, PartialEq)]
pub struct BashRun {
	pub command: String,
	/// When the call was made, epoch ms.
	pub at: i64,
	pub outcome: BashOutcome,
	pub exit: Option<i64>,
}

/// Bash runs a session's stats keep: the latest, enough for one review turn.
const BASH_RUNS_KEPT: usize = 200;

/// What a `TESTS:` line said (a [`TestsResult`] without `unknown`, which no line can say).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestsReported {
	Pass,
	Fail,
	None,
}

/// The last `TESTS: pass|fail|none — <command>` line of an assistant message (the review turn's last line, 0425,
/// 0453), the command it names, and that message's time.
#[derive(Debug, Clone, PartialEq)]
pub struct TestsReport {
	pub result: TestsReported,
	pub at: i64,
	pub command: Option<String>,
}

/// Bash calls waiting for their result: tool call id → the command and its time.
type PendingBash = HashMap<String, (String, i64)>;

/// What the service reads from one session: for the sidebar, the rail, the HUD and the worklog.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionStats {
	/// The last assistant message's `provider/model`.
	pub model: Option<String>,
	/// Summed `usage.cost.total` of every assistant message, USD, its subagents' and advisors' included.
	pub cost: f64,
	/// Each assistant message's cost by local day (`YYYY-MM-DD`), its subagents' and advisors' included.
	pub cost_by_day: HashMap<String, f64>,
	/// The last assistant message's text, first line.
	pub last_line: Option<String>,
	/// The question of an `ask` call in the last assistant message: what a blocked omp waits on. From omp 18.18.3 the
	/// call is an HTML form, whose question `ask_question.rs` reads as omp does (0589).
	pub question: Option<String>,
	/// Every message's time, epoch ms, in file order.
	pub message_times: Vec<i64>,
	/// The title of a plan waiting for his approval in omp's Plan Review, or `None`. omp records the proposal as a
	/// successful `propose` command (omp 19.2.0: a bash run whose `toolResult` has `details.toolCommands` with an
	/// entry `{tool: "propose", mode: "execute", exitCode: 0, inner: {title, planFilePath, planExists}}`, or a direct
	/// `propose` tool result whose `details` are that `inner`; `tools/resolve.ts:108-121`, `event-controller.ts:2429-2437`),
	/// then aborts the turn silently (an empty assistant message). Approving leaves plan mode (`mode_change` to `none`,
	/// `interactive-mode.ts:4872`, and a second one with `data.approvedPlanFilePath`); refining sends a user message;
	/// either, or any later assistant output, ends the wait.
	pub plan_awaiting: Option<String>,
	/// Whether the session is in plan mode: its last `mode_change` was to `plan` (omp writes one on entering it).
	pub plan_mode: bool,
	/// The file of the last plan proposed in the session (`inner.planFilePath`), kept after approval: the reviewer
	/// reads it (0453).
	pub plan_file: Option<String>,
	/// How often he sent a waiting plan back (Plan Review's "Refine plan" sends his message while it waits, 0471).
	pub plan_sent_back: u32,
	/// The last `TESTS:` line of an assistant message (0425, 0453); `None` when none.
	pub tests: Option<TestsReport>,
	/// The latest bash calls with their outcome (0453), oldest first.
	pub bash_runs: Vec<BashRun>,
	/// Bash calls waiting for their result, by tool call id: the command and its time.
	pub pending_bash: PendingBash,
	/// The time of the last `/handoff` compaction entry (`method: "handoff"`), epoch ms; `None` when none.
	pub handoff_at: Option<i64>,
	/// The context the last assistant message left (0174): its input, cached and output tokens; `None` before one.
	pub context: Option<u64>,
}

/// A review's test result (0425, 0453): the review turn's test command ran and exited 0 (`pass`), it failed or the
/// turn said it failed (`fail`), the project has none (`none`); `unknown` when no run backs the turn's claim
/// ("tests unverified").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestsResult {
	Pass,
	Fail,
	None,
	Unknown,
}

/// The line `reviewPrompt` asks the review turn to end with: `TESTS: pass — <command>`.
static TESTS_LINE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"(?im)^[\s*_`>-]*TESTS:\s*(pass|fail|none)(?-u:\b)[`*_]*(?:[ \t]*[—–:-]+[ \t]*([^\n]+))?")
		.expect("the TESTS line pattern")
});

/// A session written to this recently may still have subagents or an advisor writing: its artifacts dir is re-listed.
const ACTIVE_MS: i64 = 10 * 60_000;

/// How often an unchanged active session's artifacts dir is listed again.
const RELIST_MS: i64 = 30_000;

/// A file checked this recently is answered from what was read (PM-6).
const CHECK_MS: i64 = 1_000;

/// The ids of every entry in `file` (the session a moved one copied its transcript from).
fn entry_ids(file: &Path) -> HashSet<String> {
	struct Collect(HashSet<String>);
	impl TailVisitor for Collect {
		fn entry(&mut self, entry: Map<String, Value>) {
			if let Some(id) = entry.get("id").and_then(Value::as_str) {
				self.0.insert(id.to_owned());
			}
		}
		fn reset(&mut self) {
			self.0.clear();
		}
	}
	let mut tail = SessionTail::new(file.to_path_buf(), Collect(HashSet::new()));
	let _ = tail.advance(None);
	tail.visitor.0
}

/// A non-empty (after trimming) string field, trimmed.
fn trimmed(value: Option<&Value>) -> Option<String> {
	value.and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

/// What a successful `propose` left in a tool result (omp 19.2.0 `proposeDispatchDetails`, `tools/resolve.ts:108-121`).
struct Proposal<'a> {
	/// The plan's details as the command returned them (`title`, `planFilePath`, `planExists`).
	inner: Option<&'a Map<String, Value>>,
	/// The command's arguments (`title`), the title when the details have none.
	args: Option<&'a Map<String, Value>>,
}

/// The proposal a tool result holds, or `None` for any other result. A direct `propose` call carries its details on
/// the result (an error submitted nothing); a bash run carries one record per tool command it ran in
/// `details.toolCommands`, the last successful `propose` execution counting, each judged by its own exit code.
fn proposal(message: &Map<String, Value>) -> Option<Proposal<'_>> {
	let details = message.get("details").and_then(as_object);
	match message.get("toolName").and_then(Value::as_str) {
		Some("propose") if message.get("isError") != Some(&Value::Bool(true)) => {
			Some(Proposal { inner: details, args: None })
		}
		Some("bash") => {
			let record =
				details?.get("toolCommands")?.as_array()?.iter().rev().filter_map(as_object).find(|record| {
					record.get("tool").and_then(Value::as_str) == Some("propose")
						&& record.get("mode").and_then(Value::as_str) == Some("execute")
						&& record.get("exitCode").and_then(Value::as_f64) == Some(0.0)
				})?;
			Some(Proposal {
				inner: record.get("inner").and_then(as_object),
				args: record.get("args").and_then(as_object),
			})
		}
		_ => None,
	}
}

/// The plan title of a `propose` result, or `None` for any other tool result.
fn proposed_plan(message: &Map<String, Value>) -> Option<String> {
	let proposal = proposal(message)?;
	let title = proposal
		.inner
		.and_then(|inner| inner.get("title"))
		.filter(|title| !title.is_null())
		.or_else(|| proposal.args.and_then(|args| args.get("title")));
	Some(trimmed(title).unwrap_or_else(|| "plan".to_owned()))
}

/// A plan proposal's file (`inner.planFilePath`), or `None`.
fn proposed_plan_file(message: &Map<String, Value>) -> Option<String> {
	trimmed(proposal(message)?.inner?.get("planFilePath"))
}

/// A time from a message's own `timestamp`, else the entry's ISO time; `None` when neither.
fn entry_time(entry: &Map<String, Value>, message: Option<&Map<String, Value>>) -> Option<i64> {
	// omp stats' rule (`coerceEntryTimestamp`): a stored zero means "no time"; the entry's ISO time is next.
	if let Some(at) = message.and_then(|message| message.get("timestamp")).and_then(Value::as_f64)
		&& at > 0.0
	{
		return Some(at as i64);
	}
	entry.get("timestamp").and_then(Value::as_str).and_then(date_parse)
}

/// A model call's `usage.cost.total`, or `None` when it has none.
fn cost_of(usage: Option<&Value>) -> Option<f64> {
	let cost = usage.and_then(as_object)?.get("cost").and_then(as_object)?.get("total")?.as_f64()?;
	cost.is_finite().then_some(cost)
}

/// The tokens a model call's context holds once answered (`usage.totalTokens`, else its parts summed); `None` when none.
fn context_of(usage: Option<&Value>) -> Option<u64> {
	let tokens = usage.and_then(as_object)?;
	if let Some(total) = tokens.get("totalTokens").and_then(Value::as_f64)
		&& total > 0.0
	{
		return Some(total as u64);
	}
	let sum: f64 = ["input", "output", "cacheRead", "cacheWrite"]
		.iter()
		.filter_map(|key| tokens.get(*key).and_then(Value::as_f64))
		.filter(|value| value.is_finite())
		.sum();
	(sum > 0.0).then_some(sum as u64)
}

/// One model call: its entry, usage and time.
struct Call<'a> {
	entry: &'a Map<String, Value>,
	usage: Option<&'a Value>,
	at: Option<i64>,
}

/// Counts one model call's cost (when it has one): into the stats' total and its day unless the entry is a copy of
/// one the old file holds, and into the day's ledger always (the ledger knows a copy).
fn add_cost(stats: &mut SessionStats, call: &Call, copied: bool, ledger: Option<&mut SpendLedger>) {
	let Some(cost) = cost_of(call.usage) else { return };
	if let Some(ledger) = ledger {
		ledger.add(call.entry, call.usage, call.at, cost);
	}
	if copied {
		return;
	}
	stats.cost += cost;
	if let Some(at) = call.at {
		*stats.cost_by_day.entry(local_day(at)).or_insert(0.0) += cost;
	}
}

/// One day's calls: each call's key, and their cost.
#[derive(Default)]
struct Spent {
	keys: HashSet<String>,
	total: f64,
}

/// Every model call's cost by local day, each counted once wherever it was written (0433, 0452, 0464). A call is known
/// by its entry's ISO `timestamp` and its usage (tokens and cost), which its copies keep (a moved session's sibling, a
/// file rewritten through a rename, a moved session's artifacts) and so does omp's re-journal of a turn it discards
/// durably (an off-branch `model_usage` under a new id with the turn's timestamp, SRV-B-2). So a copy never counts
/// twice, and a call stays counted after the file holding it was rewritten without it, moved or deleted: the day's
/// spend never drops within the day. The day is the message's own time. Calls before `since` aren't kept.
struct SpendLedger {
	since: i64,
	days: HashMap<String, Spent>,
}

impl SpendLedger {
	fn new() -> Self {
		SpendLedger { since: i64::MIN, days: HashMap::new() }
	}

	fn add(&mut self, entry: &Map<String, Value>, usage: Option<&Value>, at: Option<i64>, cost: f64) {
		let Some(at) = at else { return };
		if at < self.since {
			return;
		}
		let usage = usage.and_then(as_object);
		let tokens = ["input", "output", "cacheRead", "cacheWrite"]
			.map(|key| usage.and_then(|usage| usage.get(key)).and_then(Value::as_f64).unwrap_or(0.0).to_string())
			.join("|");
		// An entry without an ISO time (older transcripts): its id, if any, and its time stand for it.
		let stamp = match entry.get("timestamp").and_then(Value::as_str) {
			Some(stamp) => stamp.to_owned(),
			None => format!("{}|{at}", entry.get("id").and_then(Value::as_str).unwrap_or("")),
		};
		let key = format!("{stamp}|{tokens}|{cost}");
		let spent = self.days.entry(local_day(at)).or_default();
		if spent.keys.insert(key) {
			spent.total += cost;
		}
	}

	fn total(&self, day: &str) -> f64 {
		self.days.get(day).map_or(0.0, |spent| spent.total)
	}

	/// Drops the days before `since`'s and keeps no call older than it from now on.
	fn forget_before(&mut self, since: i64) {
		self.since = since;
		let first = local_day(since);
		self.days.retain(|day, _| day.as_str() >= first.as_str());
	}
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Trims what a command's edges hold besides words: whitespace, backticks, `*` and `_`.
fn trim_command(command: &str) -> &str {
	command.trim_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '*' | '_'))
}

/// The test result and command of a `TESTS:` line in `text`, the last one; `None` when there is none.
fn tests_line(text: &str) -> Option<(TestsReported, Option<String>)> {
	let last = TESTS_LINE.captures_iter(text).last()?;
	let result = match last.get(1)?.as_str().to_ascii_lowercase().as_str() {
		"pass" => TestsReported::Pass,
		"fail" => TestsReported::Fail,
		"none" => TestsReported::None,
		_ => return None,
	};
	let command = last.get(2).map(|m| trim_command(m.as_str())).filter(|command| !command.is_empty());
	Some((result, command.map(str::to_owned)))
}

/// A command with its whitespace runs made one space, so a named command matches the one that ran.
fn squash(command: &str) -> String {
	command.split_whitespace().collect::<Vec<_>>().join(" ")
}

static LINE_CONTINUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\\r?\n").expect("pattern"));
static NEWLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\r?\n").expect("pattern"));
static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""(?:[^"\\]|\\.)*"|'[^']*'"#).expect("pattern"));
static FD_REDIRECT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]*>>?&(?:[0-9]+|-)").expect("pattern"));
static BOTH_REDIRECT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&>>?").expect("pattern"));

/// Whether a run's exit code is the named command's own (0453): nothing after the command's last occurrence may take
/// its place, as a pipe does (omp's shell runs without pipefail: `bun test | tail` exits as `tail` does), or `||`, `;`,
/// a background `&` or a next line. Quoted text, redirections (`2>&1`, `> log`), `&&` and commands before it (`cd x &&`)
/// leave it the command's.
fn own_exit(command: &str, named: &str) -> bool {
	let joined = LINE_CONTINUATION.replace_all(command, " ");
	let flat = squash(&NEWLINE.replace_all(&joined, " ; "));
	let Some(at) = flat.rfind(named) else { return false };
	let rest = QUOTED.replace_all(&flat[at + named.len()..], " ");
	let rest = FD_REDIRECT.replace_all(&rest, " ");
	let rest = BOTH_REDIRECT.replace_all(&rest, " ");
	let rest = rest.replace("&&", " ");
	!rest.contains(['|', ';', '&'])
}

/// The last run at or after `since` of a bash call holding the named command.
fn last_run<'a>(stats: &'a SessionStats, named: &str, since: i64) -> Option<&'a BashRun> {
	stats.bash_runs.iter().rev().find(|run| run.at >= since && squash(&run.command).contains(named))
}

/// What a review turn's tests showed (0453): its `TESTS:` line is believed only when the command it names ran in the
/// session at or after `since` and that run's exit code agrees. `pass` needs the named command's last run to exit 0
/// with the exit code its own ([`own_exit`]); a failed run, or the turn saying `fail`, is `fail`; `none` is the project
/// having no tests; no line, no command, no such run, one still running in the background, or one whose exit code a
/// later command gave is `unknown` (unverified).
pub fn checked_tests(stats: &SessionStats, since: i64) -> TestsResult {
	let Some(reported) = stats.tests.as_ref().filter(|tests| tests.at >= since) else { return TestsResult::Unknown };
	if reported.result == TestsReported::None {
		return TestsResult::None;
	}
	let said_fail = reported.result == TestsReported::Fail;
	let fail_or_unknown = if said_fail { TestsResult::Fail } else { TestsResult::Unknown };
	let Some(command) = &reported.command else { return fail_or_unknown };
	let named = squash(command);
	let Some(run) = last_run(stats, &named, since) else { return fail_or_unknown };
	if run.outcome == BashOutcome::Fail || said_fail {
		return TestsResult::Fail;
	}
	if run.outcome == BashOutcome::Ok && own_exit(&run.command, &named) {
		TestsResult::Pass
	} else {
		TestsResult::Unknown
	}
}

/// Each check's exit code in the review turn (0453, 0540): the last run at or after `since` of a bash call holding the
/// command, its exit code when that code is the command's own ([`own_exit`]: not piped or followed by another), else
/// `None`, as for a check that didn't run or still runs in the background.
pub fn check_exits(stats: &SessionStats, commands: &[String], since: i64) -> Vec<(String, Option<i64>)> {
	commands
		.iter()
		.map(|command| {
			let named = squash(command);
			let exit = last_run(stats, &named, since)
				.filter(|run| run.outcome != BashOutcome::Background && own_exit(&run.command, &named))
				.and_then(|run| run.exit);
			(command.clone(), exit)
		})
		.collect()
}

/// A bash call in an assistant message.
#[derive(Debug, Clone)]
struct BashCall {
	id: String,
	command: String,
	at: i64,
}

/// An assistant message's effect.
#[derive(Debug, Clone)]
struct AssistantEffect {
	/// It has content: it ends a plan's wait.
	said: bool,
	model: Option<String>,
	/// A superseded retry (0464): the model only.
	retry: bool,
	first_line: Option<String>,
	tests: Option<TestsReport>,
	question: Option<String>,
	bash: Vec<BashCall>,
	context: Option<u64>,
}

/// A bash call's result: its tool call id, outcome and exit code.
#[derive(Debug, Clone)]
struct ResultEffect {
	call_id: String,
	outcome: BashOutcome,
	exit: Option<i64>,
}

/// What one entry does to the active branch's state (0509): only what the state needs, no message texts.
#[derive(Debug, Clone, Default)]
struct Effect {
	/// A `mode_change`'s mode.
	mode: Option<String>,
	/// A `/handoff` compaction's time.
	handoff_at: Option<i64>,
	/// A `propose` result: the plan's title and file.
	plan: Option<(String, Option<String>)>,
	result: Option<ResultEffect>,
	/// A user message; the flag is `synthetic` when omp wrote it.
	user: Option<bool>,
	assistant: Option<AssistantEffect>,
}

/// An entry's state effect, or `None` when it has none.
fn effect_of(entry: &Map<String, Value>) -> Option<Effect> {
	let kind = entry.get("type").and_then(Value::as_str);
	if kind == Some("mode_change") {
		let mode = match entry.get("mode") {
			Some(Value::String(mode)) => mode.clone(),
			Some(other) => other.to_string(),
			None => "undefined".to_owned(),
		};
		return Some(Effect { mode: Some(mode), ..Effect::default() });
	}
	if kind == Some("compaction") && entry.get("method").and_then(Value::as_str) == Some("handoff") {
		let at = entry.get("timestamp").and_then(Value::as_str).and_then(date_parse)?;
		return Some(Effect { handoff_at: Some(at), ..Effect::default() });
	}
	if kind != Some("message") {
		return None;
	}
	let message = entry.get("message").and_then(as_object)?;
	match message.get("role").and_then(Value::as_str) {
		Some("toolResult") => tool_result_effect(message),
		Some("user") => {
			Some(Effect { user: Some(message.get("synthetic") == Some(&Value::Bool(true))), ..Effect::default() })
		}
		Some("assistant") => Some(assistant_effect(entry, message)),
		_ => None,
	}
}

/// A tool result's effect: the plan it proposed and/or the outcome of its call; `None` when it has neither.
fn tool_result_effect(message: &Map<String, Value>) -> Option<Effect> {
	let mut effect = Effect::default();
	if let Some(title) = proposed_plan(message) {
		effect.plan = Some((title, proposed_plan_file(message)));
	}
	if let Some(call_id) = message.get("toolCallId").and_then(Value::as_str) {
		let details = message.get("details").and_then(as_object);
		let outcome = if details.and_then(|details| details.get("async")).is_some_and(|value| value.is_object()) {
			BashOutcome::Background
		} else if message.get("isError") == Some(&Value::Bool(true)) {
			BashOutcome::Fail
		} else {
			BashOutcome::Ok
		};
		let code = details.and_then(|details| details.get("exitCode")).and_then(Value::as_f64);
		let exit = match outcome {
			BashOutcome::Ok => Some(0),
			BashOutcome::Fail => code.map(|code| code as i64),
			BashOutcome::Background => None,
		};
		effect.result = Some(ResultEffect { call_id: call_id.to_owned(), outcome, exit });
	}
	(effect.plan.is_some() || effect.result.is_some()).then_some(effect)
}

/// An assistant message's effect: what the thread said, asked and ran, and the context it used.
fn assistant_effect(entry: &Map<String, Value>, message: &Map<String, Value>) -> Effect {
	let at = entry_time(entry, Some(message));
	let model = message.get("model").and_then(Value::as_str).map(|model| {
		match message.get("provider").and_then(Value::as_str) {
			Some(provider) => format!("{provider}/{model}"),
			None => model.to_owned(),
		}
	});
	let content: &[Value] = message.get("content").and_then(Value::as_array).map_or(&[], Vec::as_slice);
	let said = !content.is_empty();
	// An attempt a retry superseded: billed, but not what the thread said or asked (0464).
	let retry = message.get("retryRecovery").is_some_and(|value| match value {
		Value::Null | Value::Bool(false) => false,
		Value::String(text) => !text.is_empty(),
		Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
		_ => true,
	});
	if retry {
		return Effect {
			assistant: Some(AssistantEffect {
				said,
				model,
				retry: true,
				first_line: None,
				tests: None,
				question: None,
				bash: Vec::new(),
				context: None,
			}),
			..Effect::default()
		};
	}
	let mut text = String::new();
	let mut question = None;
	let mut bash = Vec::new();
	for part in content.iter().filter_map(as_object) {
		let kind = part.get("type").and_then(Value::as_str);
		if kind == Some("text")
			&& let Some(piece) = part.get("text").and_then(Value::as_str)
		{
			text.push_str(piece);
		}
		if kind != Some("toolCall") {
			continue;
		}
		let name = part.get("name").and_then(Value::as_str);
		if name == Some("ask") {
			question = ask_question(part.get("arguments").unwrap_or(&Value::Null)).or(question);
		}
		if name == Some("bash")
			&& let Some(id) = part.get("id").and_then(Value::as_str)
			&& let Some(command) =
				part.get("arguments").and_then(as_object).and_then(|a| a.get("command")).and_then(Value::as_str)
		{
			bash.push(BashCall { id: id.to_owned(), command: command.to_owned(), at: at.unwrap_or(0) });
		}
	}
	let tests = tests_line(&text).map(|(result, command)| TestsReport { result, at: at.unwrap_or(0), command });
	let first_line = text.trim().split('\n').next().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned);
	Effect {
		assistant: Some(AssistantEffect {
			said,
			model,
			retry: false,
			first_line,
			tests,
			question,
			bash,
			context: context_of(message.get("usage")),
		}),
		..Effect::default()
	}
}

/// Applies one entry's effect to the state fields of `stats`.
fn apply_effect(stats: &mut SessionStats, effect: &Effect) {
	if let Some(mode) = &effect.mode {
		stats.plan_mode = mode == "plan";
		if mode != "plan" {
			stats.plan_awaiting = None;
		}
	}
	if let Some(at) = effect.handoff_at {
		stats.handoff_at = Some(at);
	}
	if let Some((title, file)) = &effect.plan {
		stats.plan_awaiting = Some(title.clone());
		if file.is_some() {
			stats.plan_file.clone_from(file);
		}
	}
	if let Some(result) = &effect.result
		&& let Some((command, at)) = stats.pending_bash.remove(&result.call_id)
	{
		stats.bash_runs.push(BashRun { command, at, outcome: result.outcome, exit: result.exit });
		if stats.bash_runs.len() > BASH_RUNS_KEPT {
			let excess = stats.bash_runs.len() - BASH_RUNS_KEPT;
			stats.bash_runs.drain(..excess);
		}
	}
	if let Some(synthetic) = effect.user {
		if stats.plan_awaiting.is_some() && !synthetic {
			stats.plan_sent_back += 1;
		}
		stats.plan_awaiting = None;
	}
	if let Some(said) = &effect.assistant {
		apply_assistant(stats, said);
	}
}

/// An assistant message's part of [`apply_effect`].
fn apply_assistant(stats: &mut SessionStats, said: &AssistantEffect) {
	if said.said {
		stats.plan_awaiting = None;
	}
	if said.model.is_some() {
		stats.model.clone_from(&said.model);
	}
	if said.retry {
		return;
	}
	for call in &said.bash {
		stats.pending_bash.insert(call.id.clone(), (call.command.clone(), call.at));
	}
	if said.first_line.is_some() {
		stats.last_line.clone_from(&said.first_line);
	}
	if said.tests.is_some() {
		stats.tests.clone_from(&said.tests);
	}
	if said.context.is_some() {
		stats.context = said.context;
	}
	stats.question.clone_from(&said.question);
}

/// The state fields of `stats` set back to a session's start.
fn reset_state(stats: &mut SessionStats) {
	stats.model = None;
	stats.last_line = None;
	stats.question = None;
	stats.plan_awaiting = None;
	stats.plan_mode = false;
	stats.plan_file = None;
	stats.plan_sent_back = 0;
	stats.tests = None;
	stats.bash_runs = Vec::new();
	stats.pending_bash = HashMap::new();
	stats.handoff_at = None;
	stats.context = None;
}

/// One entry of the session tree: its parent and its state effect.
struct Node {
	parent: Option<String>,
	effect: Option<Effect>,
}

/// omp's session tree as far as the state needs it (0509): each entry's parent and effect, and the active leaf.
#[derive(Default)]
struct Branches {
	nodes: HashMap<String, Node>,
	/// The active leaf the state was built for; `None` before the first entry or after a leaf reset.
	leaf: Option<String>,
	/// The newest entry that selected itself: the leaf when one names an unknown entry.
	last_self: Option<String>,
}

/// The state fields of `stats` rebuilt for the branch from `branches.leaf` back to its root.
fn replay_branch(stats: &mut SessionStats, branches: &Branches) {
	let mut effects: Vec<&Effect> = Vec::new();
	let mut visited: HashSet<&str> = HashSet::new();
	let mut id = branches.leaf.as_deref();
	while let Some(current) = id {
		if !visited.insert(current) {
			break;
		}
		let Some(node) = branches.nodes.get(current) else { break };
		effects.extend(node.effect.as_ref());
		id = node.parent.as_deref();
	}
	reset_state(stats);
	for effect in effects.into_iter().rev() {
		apply_effect(stats, effect);
	}
}

/// What [`add_entry`] knows besides the entry: the entry ids already counted in the file this one moved from, the day's
/// ledger, and whether only the cost counts.
#[derive(Default)]
struct EntryContext<'a> {
	seen: Option<&'a HashSet<String>>,
	ledger: Option<&'a mut SpendLedger>,
	cost_only: bool,
}

/// Folds one session entry into `stats`; `seen` holds entry ids already counted in the file this one moved from;
/// `ledger` counts the day's spend across files. Cost and times count in file order; the state follows the active
/// branch (`branches`), an entry without an id or a `parentId` applying in file order. `cost_only` (a nested
/// transcript, whose cost alone the session uses, SRV-B-11) counts the cost and keeps no times, branches or state.
fn add_entry(
	stats: &mut SessionStats,
	branches: &mut Branches,
	entry: &Map<String, Value>,
	context: &mut EntryContext,
) {
	let id = entry.get("id").and_then(Value::as_str);
	// A copy of an entry the old file holds: its cost and time are counted there.
	let copied = matches!((context.seen, id), (Some(seen), Some(id)) if seen.contains(id));
	if entry.get("type").and_then(Value::as_str) == Some("model_usage") {
		let call = Call { entry, usage: entry.get("usage"), at: entry_time(entry, None) };
		add_cost(stats, &call, copied, context.ledger.as_deref_mut());
	}
	let message = if entry.get("type").and_then(Value::as_str) == Some("message") {
		entry.get("message").and_then(as_object)
	} else {
		None
	};
	if let Some(message) = message {
		let at = entry_time(entry, Some(message));
		if let Some(at) = at
			&& !copied
			&& !context.cost_only
		{
			stats.message_times.push(at);
		}
		if message.get("role").and_then(Value::as_str) == Some("assistant") {
			let call = Call { entry, usage: message.get("usage"), at };
			add_cost(stats, &call, copied, context.ledger.as_deref_mut());
		}
	}
	if context.cost_only {
		return;
	}
	let effect = effect_of(entry);
	let Some(id) = id.filter(|_| entry.contains_key("parentId")) else {
		if let Some(effect) = &effect {
			apply_effect(stats, effect);
		}
		return;
	};
	track_branch(stats, branches, entry, id, effect);
}

/// Places one entry on the session tree and moves the state to the active leaf (0509).
fn track_branch(
	stats: &mut SessionStats,
	branches: &mut Branches,
	entry: &Map<String, Value>,
	id: &str,
	effect: Option<Effect>,
) {
	let parent = entry.get("parentId").and_then(Value::as_str).map(str::to_owned);
	// omp's replay (`session-entries.ts` `sessionEntryLeafId`, `lastSelfSelectedLeafId`).
	let selected: Option<&str> = match entry.get("activeLeafId") {
		Some(Value::Null) => None,
		Some(Value::String(selected)) => Some(selected),
		_ => Some(id),
	};
	if selected == Some(id) {
		branches.last_self = Some(id.to_owned());
	}
	let leaf = match selected {
		Some(selected) if selected != id && !branches.nodes.contains_key(selected) => branches.last_self.clone(),
		selected => selected.map(str::to_owned),
	};
	let same = leaf == branches.leaf; // off the branch: the state stands
	let grows = !same && leaf.as_deref() == Some(id) && parent == branches.leaf;
	if grows && let Some(effect) = &effect {
		apply_effect(stats, effect);
	}
	branches.nodes.insert(id.to_owned(), Node { parent, effect });
	if same {
		return;
	}
	branches.leaf = leaf;
	if !grows {
		replay_branch(stats, branches);
	}
}

/// Stats of a session file's text, read whole. Tests use it as the oracle the incremental reader
/// ([`SessionIndex::read`]) must match.
pub fn read_stats(text: &str) -> SessionStats {
	let mut stats = SessionStats::default();
	let mut branches = Branches::default();
	for entry in json_lines::<Map<String, Value>>(text) {
		add_entry(&mut stats, &mut branches, &entry, &mut EntryContext::default());
	}
	stats
}

/// What one transcript's [`SessionTail`] hands its entries to: the stats read of it so far.
struct TailState {
	stats: SessionStats,
	/// The session tree read so far, which the state follows (0509).
	branches: Branches,
	/// For a session moved to this file: the ids of the old file's entries, whose cost and times count there.
	seen: Option<HashSet<String>>,
	ledger: Arc<Mutex<SpendLedger>>,
	/// A nested transcript: only its cost is read ([`add_entry`]'s `cost_only`).
	cost_only: bool,
}

impl TailVisitor for TailState {
	fn entry(&mut self, entry: Map<String, Value>) {
		let mut ledger = locked(&self.ledger);
		let mut context =
			EntryContext { seen: self.seen.as_ref(), ledger: Some(&mut ledger), cost_only: self.cost_only };
		add_entry(&mut self.stats, &mut self.branches, &entry, &mut context);
	}

	/// A rewritten file (omp's rewrite through a rename, possibly larger) is read again from its start.
	fn reset(&mut self) {
		self.stats = SessionStats::default();
		self.branches = Branches::default();
	}
}

/// One transcript followed incrementally (`SessionTail`, CR-03) and the stats read of it so far.
type Tail = SessionTail<TailState>;

fn new_tail(file: &Path, ledger: &Arc<Mutex<SpendLedger>>, cost_only: bool) -> Tail {
	let state = TailState {
		stats: SessionStats::default(),
		branches: Branches::default(),
		seen: None,
		ledger: Arc::clone(ledger),
		cost_only,
	};
	SessionTail::new(file.to_path_buf(), state)
}

/// A session's own transcript, the nested ones found in its artifacts dir, and their stats combined.
struct Followed {
	main: Tail,
	nested: HashMap<PathBuf, Tail>,
	/// The main transcript's `(mtime, size)` when its artifacts dir was last listed.
	listed_at: Option<(f64, u64)>,
	/// When the artifacts dir was last listed, by the index's clock.
	listed_time: i64,
	/// When `read` last looked at the files, by the index's clock.
	checked_at: Option<i64>,
	/// When a nested transcript last grew, by the index's clock: its folder is re-listed while it works (SRV-B-19).
	nested_grew_at: Option<i64>,
	/// The stats with the nested transcripts' cost added; `None` while there are none (the main stats stand).
	combined: Option<SessionStats>,
}

/// Bytes parsed before `warm` lets the runtime run.
const WARM_STEP: u64 = 1 << 20;

impl Followed {
	/// A session's stats with its nested transcripts' cost added.
	fn combine(&mut self) {
		if self.nested.is_empty() {
			self.combined = None;
			return;
		}
		let mut combined = self.main.visitor.stats.clone();
		for tail in self.nested.values() {
			let stats = &tail.visitor.stats;
			combined.cost += stats.cost;
			for (day, value) in &stats.cost_by_day {
				*combined.cost_by_day.entry(day.clone()).or_insert(0.0) += value;
			}
		}
		self.combined = Some(combined);
	}

	fn view(&self) -> &SessionStats {
		self.combined.as_ref().unwrap_or(&self.main.visitor.stats)
	}
}

/// The cached key of a listed session: `(mtime, size)` of the main transcript.
fn listing_key(tail: &Tail) -> (f64, u64) {
	(tail.mtime_ms.unwrap_or(-1.0), tail.size.unwrap_or(0))
}

/// A day before yesterday asked for: its own index and the day.
struct Past {
	day: String,
	index: SessionIndex,
}

/// Per cwd dir: its mtime when last read, and each `parentSession` id → its newest child file.
struct Children {
	mtime_ms: f64,
	by_parent: HashMap<String, PathBuf>,
}

/// Session files under `dir` modified at or after `since_ms`, with their headers, and each session's stats read
/// incrementally. Headers are cached by path, as a session's cwd never changes. At the first call of a new local day,
/// message times before yesterday are dropped, and sessions not written since yesterday's midnight and files that are
/// gone are forgotten (PM-17, PERF-4). A day before yesterday is read through a throwaway index
/// ([`SessionIndex::times_on`], SRV-B-4).
pub struct SessionIndex {
	pub dir: PathBuf,
	now: Clock,
	/// How many days before today the ledger and the roll-over keep: 1 (yesterday) for the shared index.
	horizon: i64,
	headers: HashMap<PathBuf, SessionHeader>,
	followed: HashMap<PathBuf, Followed>,
	/// The local day of the last call.
	day: Option<String>,
	/// The spend of today and yesterday, across every file read (0464): [`SessionIndex::spent_on`].
	ledger: Arc<Mutex<SpendLedger>>,
	/// A day before yesterday asked for (the worklog on request): read whole by its own index, dropped at rollover.
	past: Option<Box<Past>>,
	/// The session files under `dir`, cwd dirs unchanged since the last list answered from it (SRV-B-6).
	walker: SessionWalker,
	children_by_dir: HashMap<PathBuf, Children>,
	/// What a missing file reads as.
	empty: SessionStats,
}

impl SessionIndex {
	/// An index over `dir` (omp's sessions dir by default) on a clock (the wall clock by default).
	pub fn new(dir: Option<PathBuf>, now: Option<Clock>) -> Self {
		Self::with_horizon(dir.unwrap_or_else(omp_sessions_dir), 1, now.unwrap_or_else(system_clock))
	}

	/// A throwaway index whose ledger keeps `days_back` prior days instead of yesterday only (0668: the 7-day spend
	/// backfill); the shared index must not use it, as its roll-over would then keep that many days of sessions.
	pub fn with_horizon(dir: PathBuf, days_back: i64, now: Clock) -> Self {
		SessionIndex {
			walker: SessionWalker::new(dir.clone()),
			dir,
			now,
			horizon: days_back,
			headers: HashMap::new(),
			followed: HashMap::new(),
			day: None,
			ledger: Arc::new(Mutex::new(SpendLedger::new())),
			past: None,
			children_by_dir: HashMap::new(),
			empty: SessionStats::default(),
		}
	}

	pub fn list(&mut self, since_ms: i64) -> Vec<SessionFile> {
		let now = (self.now)();
		self.roll_over(now);
		let mut found = Vec::new();
		for file in self.walker.files() {
			// Removed since the listing: skipped.
			let Some(stat) = FileStat::of(&file) else { continue };
			if stat.mtime_ms < since_ms as f64 {
				continue;
			}
			if let Some(header) = self.header(&file) {
				found.push(SessionFile::new(&header, stat.mtime_ms));
			}
		}
		found
	}

	/// What model calls cost on `day` (local, `YYYY-MM-DD`; today or yesterday) in the files read so far, subagents and
	/// advisors included, each call once however many files hold it; a call stays counted after its file changed or
	/// went.
	pub fn spent_on(&mut self, day: &str) -> f64 {
		let now = (self.now)();
		self.roll_over(now);
		locked(&self.ledger).total(day)
	}

	/// A session's stats, its subagents' and advisors' cost included; only lines appended since the last call are
	/// parsed. A missing file has empty stats. A file checked within a second is answered from what was read, unless
	/// `fresh`: a state change is judged on what the file holds now.
	pub fn read(&mut self, file: &Path, fresh: bool) -> &SessionStats {
		let now = (self.now)();
		self.roll_over(now);
		let checked = self.followed.get(file).is_some_and(|followed| {
			!fresh && followed.checked_at.is_some_and(|checked_at| now - checked_at < CHECK_MS)
		});
		if checked {
			return self.followed[file].view();
		}
		let mut followed = match self.followed.remove(file) {
			Some(followed) => followed,
			None if file.exists() => self.new_followed(file),
			None => return &self.empty,
		};
		self.refresh(file, &mut followed, now);
		self.followed.insert(file.to_path_buf(), followed);
		self.followed[file].view()
	}

	/// The body of [`SessionIndex::read`]: brings `followed` up to the files and lists the artifacts dir when due.
	/// A file that can't be read leaves its tail as it was.
	fn refresh(&mut self, file: &Path, followed: &mut Followed, now: i64) {
		followed.checked_at = Some(now);
		let mut changed = followed.main.advance(None).unwrap_or(false);
		// The known subagent and advisor transcripts are followed on every read (one stat each), so a long `task` call
		// that leaves the session untouched still shows its spend as it goes (SRV-B-19).
		for tail in followed.nested.values_mut() {
			if tail.advance(None).unwrap_or(false) {
				changed = true;
				followed.nested_grew_at = Some(now);
			}
		}
		// The artifacts dir is listed again when the session changed, or every `RELIST_MS` while it or a transcript in
		// it is active (a subagent or the advisor may start after the session's last line).
		let key = listing_key(&followed.main);
		let main_active = followed.main.mtime_ms.is_some_and(|mtime| (now as f64) - mtime < ACTIVE_MS as f64);
		let active = main_active || followed.nested_grew_at.is_some_and(|grew| now - grew < ACTIVE_MS);
		if followed.listed_at != Some(key) || (active && now - followed.listed_time >= RELIST_MS) {
			followed.listed_at = Some(key);
			followed.listed_time = now;
			for nested in transcripts_of(file) {
				if followed.nested.contains_key(&nested) {
					continue;
				}
				if self.nested_tail(file, followed, &nested).advance(None).unwrap_or(false) {
					changed = true;
					followed.nested_grew_at = Some(now);
				}
			}
		}
		if changed {
			followed.combine();
		}
	}

	/// Whether `file` was read already, so a `read` of it parses only what was appended.
	pub fn follows(&self, file: &Path) -> bool {
		self.followed.contains_key(file)
	}

	/// The message times of session `file` that a worklog of `day` needs (SRV-B-4). Today's and yesterday's come from
	/// the followed session; a day before yesterday's, whose times this index dropped at rollover, from a throwaway
	/// index kept for that day ([`SessionIndex::prepare`] reads it in steps first).
	pub fn times_on(&mut self, file: &Path, day: &str) -> &[i64] {
		&self.index_for(day).read(file, false).message_times
	}

	/// Reads the sessions that may hold `day`'s messages (written since its start) in steps, letting the runtime run, so
	/// [`SessionIndex::times_on`] for that day then only parses what was appended. A day before yesterday goes to its
	/// throwaway index.
	pub async fn prepare(&mut self, day: &str, stopped: impl Fn() -> bool) {
		let index = self.index_for(day);
		let files: Vec<PathBuf> = index.list(day_start(day, 0)).into_iter().map(|session| session.file).collect();
		index.warm(&files, stopped).await;
	}

	/// This index for today and yesterday; for an older day, a fresh one (its times kept whole), one day at a time.
	fn index_for(&mut self, day: &str) -> &mut SessionIndex {
		let now = (self.now)();
		self.roll_over(now);
		let first = local_day(day_start(self.day.as_deref().unwrap_or_default(), -1));
		if day >= first.as_str() {
			return self;
		}
		if self.past.as_ref().is_some_and(|past| past.day != day) {
			self.past = None;
		}
		let (dir, clock) = (self.dir.clone(), Arc::clone(&self.now));
		let past = self.past.get_or_insert_with(|| {
			Box::new(Past { day: day.to_owned(), index: SessionIndex::new(Some(dir), Some(clock)) })
		});
		&mut past.index
	}

	/// Reads `files` (sessions with their nested transcripts) the way `read` does, about 1 MB at a time with the runtime
	/// let run between steps, so a later `read` only parses what was appended. Stops early when `stopped` says so. A
	/// file that can't be read is left for `read`.
	pub async fn warm(&mut self, files: &[PathBuf], stopped: impl Fn() -> bool) {
		let now = (self.now)();
		self.roll_over(now);
		let mut done: HashSet<&Path> = HashSet::new();
		for file in files {
			if !done.insert(file) {
				continue;
			}
			if stopped() {
				return;
			}
			if !file.exists() {
				continue;
			}
			let mut followed = match self.followed.remove(file) {
				Some(followed) => followed,
				None => self.new_followed(file),
			};
			let _ = followed.main.advance_in_steps(WARM_STEP, &stopped).await;
			for nested in transcripts_of(file) {
				let _ = self.nested_tail(file, &mut followed, &nested).advance_in_steps(WARM_STEP, &stopped).await;
			}
			followed.listed_at = Some(listing_key(&followed.main));
			followed.listed_time = (self.now)();
			followed.combine();
			self.followed.insert(file.clone(), followed);
		}
	}

	/// Where a session went on (0464): when omp moved it to a sibling file (another process wrote the old one) or
	/// started one from it, the sibling's header names it as `parentSession` (by id; a fork names the file); the newest
	/// such sibling is followed, and on from there. `file` itself when nothing moved.
	pub fn moved(&mut self, file: &Path) -> PathBuf {
		let mut current = file.to_path_buf();
		for _ in 0..8 {
			let dir = current.parent().unwrap_or(Path::new("")).to_path_buf();
			let id = self.header(&current).and_then(|header| header.id);
			let children = self.children(&dir);
			let next = id
				.and_then(|id| children.get(&id))
				.or_else(|| children.get(current.to_string_lossy().as_ref()))
				.cloned();
			match next {
				Some(next) if next != current => current = next,
				_ => break,
			}
		}
		current
	}

	/// A thread's session files (0538, 0546): `file` and the others in its folder (omp keeps one per working folder)
	/// whose session began at or after `since` (epoch ms), oldest first. omp starts a new session on "Approve and
	/// execute" (`/clear`, no `parentSession`), so following [`SessionIndex::moved`] alone loses the earlier ones.
	pub fn sessions_since(&mut self, file: &Path, since: i64) -> Vec<PathBuf> {
		let dir = file.parent().unwrap_or(Path::new(""));
		let mut found: Vec<(PathBuf, i64)> =
			vec![(file.to_path_buf(), self.header(file).and_then(|header| header.started).unwrap_or(i64::MAX))];
		for path in jsonl_files(dir) {
			let started = self.header(&path).and_then(|header| header.started);
			if let Some(started) = started
				&& started >= since
				&& !found.iter().any(|(known, _)| *known == path)
			{
				found.push((path, started));
			}
		}
		found.sort_by_key(|(_, started)| *started);
		found.into_iter().map(|(path, _)| path).collect()
	}

	/// A file's header, cached once it has one; `None` while it has none or the file is gone.
	fn header(&mut self, file: &Path) -> Option<SessionHeader> {
		if let Some(header) = self.headers.get(file) {
			return Some(header.clone());
		}
		let header = read_header(file).ok().flatten()?;
		self.headers.insert(file.to_path_buf(), header.clone());
		Some(header)
	}

	/// Per cwd dir (re-read when the dir changed): each `parentSession` id → its newest child file.
	fn children(&mut self, dir: &Path) -> &HashMap<String, PathBuf> {
		let mtime_ms = FileStat::of(dir).map_or(-1.0, |stat| stat.mtime_ms);
		if self.children_by_dir.get(dir).is_none_or(|known| known.mtime_ms != mtime_ms) {
			let mut newest: HashMap<String, (PathBuf, f64)> = HashMap::new();
			for file in jsonl_files(dir) {
				let Some(parent) = self.header(&file).and_then(|header| header.parent_session) else { continue };
				let at = FileStat::of(&file).map_or(0.0, |stat| stat.mtime_ms);
				if newest.get(&parent).is_none_or(|(_, known)| *known < at) {
					newest.insert(parent, (file, at));
				}
			}
			let by_parent = newest.into_iter().map(|(parent, (file, _))| (parent, file)).collect();
			self.children_by_dir.insert(dir.to_path_buf(), Children { mtime_ms, by_parent });
		}
		&self.children_by_dir[dir].by_parent
	}

	/// The file of session `id` in `dir`, or `None`.
	fn session_file(&mut self, dir: &Path, id: &str) -> Option<PathBuf> {
		let files = jsonl_files(dir);
		// omp names a session `<time>_<id>.jsonl`; a header is read only when no name says so.
		let suffix = format!("_{id}.jsonl");
		if let Some(named) =
			files.iter().find(|file| file.file_name().is_some_and(|name| name.to_string_lossy().ends_with(&suffix)))
		{
			return Some(named.clone());
		}
		files.into_iter().find(|file| self.header(file).is_some_and(|header| header.id.as_deref() == Some(id)))
	}

	/// A followed session, new. One moved from or started off another (`parentSession`) skips the entries it holds that
	/// are copies of the old file's, so their cost and times aren't counted twice (0452, 0464).
	fn new_followed(&mut self, file: &Path) -> Followed {
		let mut main = new_tail(file, &self.ledger, false);
		if let Some(parent) = self.parent_file(file) {
			main.visitor.seen = Some(entry_ids(&parent));
		}
		Followed {
			main,
			nested: HashMap::new(),
			listed_at: None,
			listed_time: 0,
			checked_at: None,
			nested_grew_at: None,
			combined: None,
		}
	}

	/// The file `file`'s session moved from or forked off: `parentSession` is an id, or (a fork) the file itself.
	fn parent_file(&mut self, file: &Path) -> Option<PathBuf> {
		let parent = self.header(file)?.parent_session?;
		if parent.ends_with(".jsonl") {
			return Path::new(&parent).exists().then(|| PathBuf::from(parent));
		}
		self.session_file(file.parent().unwrap_or(Path::new("")), &parent)
	}

	/// A nested transcript's tail; one omp copied along with a moved session's artifacts skips the copied entries.
	fn nested_tail<'a>(&mut self, file: &Path, followed: &'a mut Followed, nested: &Path) -> &'a mut Tail {
		let moved = followed.main.visitor.seen.is_some();
		followed.nested.entry(nested.to_path_buf()).or_insert_with(|| {
			let mut tail = new_tail(nested, &self.ledger, true);
			let parent = if moved { self.parent_file(file) } else { None };
			if let Some(parent) = parent
				&& let Ok(relative) = nested.strip_prefix(artifacts_dir(file))
			{
				let twin = artifacts_dir(&parent).join(relative);
				if twin.exists() {
					tail.visitor.seen = Some(entry_ids(&twin));
				}
			}
			tail
		})
	}

	/// At the first call of a new local day: message times and spent calls before yesterday's midnight are dropped;
	/// sessions not written since then (no transcript of theirs either) and files that no longer exist are forgotten, so
	/// the index holds today's and yesterday's sessions only (PERF-4, SRV-B-11). A forgotten session read again later is
	/// read from its start; its calls are in the ledger already, so none counts twice. An older day's index goes too.
	fn roll_over(&mut self, now: i64) {
		let day = local_day(now);
		if self.day.as_deref() == Some(day.as_str()) {
			return;
		}
		let first = self.day.is_none();
		let since = day_start(&day, -self.horizon);
		self.day = Some(day);
		locked(&self.ledger).forget_before(since);
		self.past = None;
		if first {
			return;
		}
		self.followed.retain(|file, followed| {
			let old = |tail: &Tail| tail.mtime_ms.is_some_and(|mtime| mtime < since as f64);
			if !file.exists() || (old(&followed.main) && followed.nested.values().all(old)) {
				return false;
			}
			followed.main.visitor.stats.message_times.retain(|at| *at >= since);
			for tail in followed.nested.values_mut() {
				tail.visitor.stats.message_times.retain(|at| *at >= since);
			}
			followed.combine();
			true
		});
		self.headers.retain(|file, _| file.exists());
		// The thread ledger's parsed files go the same way, except those of a session still followed (SRV-B-11).
		let kept: Vec<String> = self
			.followed
			.keys()
			.map(|file| {
				let text = file.to_string_lossy();
				text.strip_suffix(".jsonl").unwrap_or(&text).to_owned()
			})
			.collect();
		let keep = |file: &Path| {
			let text = file.to_string_lossy();
			kept.iter().any(|stem| {
				text.strip_prefix(stem.as_str())
					.is_some_and(|rest| rest == ".jsonl" || rest.starts_with(MAIN_SEPARATOR))
			})
		};
		forget_stale_sessions(since, Some(&keep));
	}
}

/// The `*.jsonl` files directly in `dir`, by name; none when it is gone.
fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
	let Ok(names) = fs::read_dir(dir) else { return Vec::new() };
	let mut files: Vec<PathBuf> =
		names.flatten().map(|entry| entry.path()).filter(|path| path.to_string_lossy().ends_with(".jsonl")).collect();
	files.sort();
	files
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
