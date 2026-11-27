//! The tools a pane's omp calls over the bridge (0675) and the `jocasta thread|worktree` commands (`cli` messages, 0544): one
//! registry each, the same functions and rules behind both doors. Every tool but `history_search`, `report_result` and
//! `check` is the boss's (0392, 0484, 0539, 0542).
//!
//! `Tools` is built after the context and takes the `Arc<ServiceContext>` (see "The service's shape").

use std::path::Path;
use std::sync::{Arc, PoisonError};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::frontier::brief::model::Kind;
use crate::frontier::brief::spec::{self, Part};
use crate::frontier::brief::store;
use crate::frontier::protocol::ToastPriority;
use crate::frontier::server::allow::Confirmation;
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::brief_actions;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::history::{HISTORY_SEARCH, HistoryEntry, add_history, history_answer, search_history};
use crate::frontier::server::lessons::ensure_lessons_skill;
use crate::frontier::server::report::{REPORT_RESULT, ReportedDecision, ThreadReport, parse_report};
use crate::frontier::server::review_notes::{
	MAX_OPINIONS, REVIEW_NOTE, ReviewNotes, checked_card, head_of, review_file,
};
use crate::frontier::server::review_system::{REVIEW_SYSTEM, review_system};
use crate::frontier::server::task_file::{CloseFacts, CloseHow, NewTask, TaskReport, close_task, find_task, new_task};
use crate::frontier::server::thread::{ThreadRef, subject};
use crate::frontier::server::thread_brief;
use crate::frontier::server::thread_lifecycle::Awaiting;
use crate::frontier::server::thread_start::{StartOptions, after_of, budget_of, done_when_of, heavy_of};
use crate::frontier::server::wording::{Door, asker, asks, branch_slug, brief_title, person, quoted};
use crate::frontier::server::works::{REVIEW_TRY, prove, review_try};
use crate::frontier::server::worktrees::remove_checkout;
use crate::shared::clock::now_ms;
use crate::shared::day::{DayRef, day_label, hhmm, local_day};
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::pathkey::same_dir;
use crate::shared::projects::Project;

/// What an MCP tool call answers (`src/mcp/server.ts`'s HTTP contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReply {
	pub text: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub is_error: Option<bool>,
}

/// Who called a tool (0484, 0578): the `jocasta mcp` process's pane key, which only the pane Jocasta started with it has
/// (the service knows the pane by it, never by a pane id the caller states), and in the boss pane this run's boss key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Caller {
	pub pane_key: Option<String>,
	pub boss_key: Option<String>,
}

/// How many lines `history_search` gives when the call names no limit, and the most it gives.
const HISTORY_LIMIT: f64 = 10.0;
const MAX_HISTORY_HITS: f64 = 50.0;

/// One tool of the registry: its name and whether only the boss may call it (`src/mcp/server.rs` lists them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
	pub name: &'static str,
	pub boss: bool,
}

/// The `jocasta mcp` tools, in the TypeScript registry's order.
pub const TOOLS: &[Tool] = &[
	Tool { name: HISTORY_SEARCH, boss: false },
	Tool { name: "projects", boss: true },
	Tool { name: "thread_start", boss: true },
	Tool { name: "thread_status", boss: true },
	Tool { name: "thread_send", boss: true },
	Tool { name: "thread_finish", boss: true },
	Tool { name: "thread_stop", boss: true },
	Tool { name: REPORT_RESULT, boss: false },
	Tool { name: REVIEW_NOTE, boss: false },
	Tool { name: REVIEW_TRY, boss: false },
	Tool { name: REVIEW_SYSTEM, boss: false },
	Tool { name: "check", boss: false },
	Tool { name: "show", boss: true },
	Tool { name: "today", boss: true },
	Tool { name: "note", boss: true },
	Tool { name: "remind", boss: true },
	Tool { name: "inbox_done", boss: true },
	Tool { name: "worklog", boss: true },
	Tool { name: "lesson", boss: true },
	Tool { name: "big_picture_review", boss: true },
	Tool { name: "worktree_remove", boss: true },
];

/// JavaScript's `String.prototype.trim` whitespace.
fn js_space(c: char) -> bool {
	(c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

fn js_trim(text: &str) -> &str {
	text.trim_matches(js_space)
}

/// `value.replace(/\s+/g, " ").trim()`.
fn flatten(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut in_space = false;
	for c in text.chars() {
		if js_space(c) {
			if !in_space {
				out.push(' ');
			}
			in_space = true;
		} else {
			in_space = false;
			out.push(c);
		}
	}
	js_trim(&out).to_owned()
}

/// JavaScript's `String(value)` for a JSON value (`undefined` for a missing one).
fn js_string(value: Option<&Value>) -> String {
	match value {
		None => "undefined".to_owned(),
		Some(Value::Null) => "null".to_owned(),
		Some(Value::Bool(flag)) => flag.to_string(),
		Some(Value::Number(number)) => match number.as_f64() {
			Some(n) if n.fract() == 0.0 && n.abs() < 1e21 => format!("{n:.0}"),
			_ => number.to_string(),
		},
		Some(Value::String(text)) => text.clone(),
		Some(Value::Array(items)) => items
			.iter()
			.map(|item| if item.is_null() { String::new() } else { js_string(Some(item)) })
			.collect::<Vec<_>>()
			.join(","),
		Some(Value::Object(_)) => "[object Object]".to_owned(),
	}
}

/// One call's arguments, read with the refusals named by its door: the tool's name, or `jocasta <command>`.
pub struct Args<'a> {
	pub values: &'a Map<String, Value>,
	named: String,
}

impl<'a> Args<'a> {
	pub fn new(values: &'a Map<String, Value>, named: impl Into<String>) -> Self {
		Self { values, named: named.into() }
	}

	/// A required text argument.
	pub fn text(&self, key: &str) -> Result<String> {
		match self.values.get(key) {
			Some(Value::String(value)) if !js_trim(value).is_empty() => Ok(value.clone()),
			_ => Err(Error::user(format!("{}: {key} is required.", self.named))),
		}
	}

	/// An optional text argument; empty is absent.
	pub fn optional(&self, key: &str) -> Option<String> {
		match self.values.get(key) {
			Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
			_ => None,
		}
	}

	/// A number given as a number or as its digits (CO-22, F-11): models send either; anything else as it came.
	pub fn count(&self, key: &str) -> Option<Value> {
		let value = self.values.get(key)?;
		if let Value::String(text) = value {
			let digits = js_trim(text);
			if !digits.is_empty()
				&& digits.chars().all(|c| c.is_ascii_digit())
				&& let Ok(number) = digits.parse::<f64>()
				&& let Some(number) = serde_json::Number::from_f64(number)
			{
				return Some(Value::Number(number));
			}
		}
		Some(value.clone())
	}
}

/// A tool call as its run sees it: its arguments, whether the boss called, and the pane the caller's key names.
struct ToolCall<'a> {
	args: Args<'a>,
	boss: bool,
	pane_id: Option<String>,
	/// What a refusal adds for a pane started before panes had keys (0578), or empty.
	unkeyed: &'static str,
}

/// A lesson as the boss's `lesson` tool gives it.
#[derive(Debug, Clone, Default)]
pub struct Lesson {
	pub title: String,
	pub applies_when: String,
	pub rule: String,
	pub source: Option<String>,
	pub retire_when: Option<String>,
}

/// `a == b` without stopping at the first difference (`timingSafeEqual`, after the lengths agree).
fn same_key(given: &[u8], key: &[u8]) -> bool {
	given.len() == key.len() && given.iter().zip(key).fold(0_u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

/// An error that isn't a refusal as `<tool>: <message>`; a refusal as it is.
fn named(tool: &str, error: Error) -> Error {
	match error {
		Error::User { .. } => error,
		other => Error::user(format!("{tool}: {other}")),
	}
}

/// The tools and commands, over the service's context.
#[derive(Clone)]
pub struct Tools {
	pub c: Arc<ServiceContext>,
}

impl Tools {
	pub fn new(c: Arc<ServiceContext>) -> Self {
		Self { c }
	}

	/// Whether the caller holds this run's boss key (0484), which only the boss pane's environment has.
	fn is_boss(&self, caller: &Caller) -> bool {
		same_key(caller.boss_key.as_deref().unwrap_or("").as_bytes(), self.c.panes.boss_key().as_bytes())
	}

	/// One tool call. A boss tool takes this run's boss key, which only the boss pane's environment has; the others
	/// answer the pane the caller's pane key names (0578), never a pane id it states. Without OS isolation they raise the
	/// bar; they are no boundary against a process of his user that reads another's environment.
	pub async fn tool(&self, name: &str, args: &Map<String, Value>, caller: &Caller) -> Result<String> {
		let boss = self.is_boss(caller);
		let pane_id = self.c.launched.pane_of_key(caller.pane_key.as_deref());
		// A pane started before panes had keys sends none: Jocasta starts it again with one once it is idle.
		let unkeyed = if caller.pane_key.as_deref().is_some_and(|key| !key.is_empty()) {
			""
		} else {
			"; a pane Jocasta started before panes had keys gets one when Jocasta starts it again, once it is idle"
		};
		let tool = TOOLS.iter().find(|tool| tool.name == name);
		if tool.is_none_or(|tool| tool.boss) && !boss {
			return Err(self.boss_refusal(name, pane_id.as_deref(), caller));
		}
		if tool.is_none() {
			return Err(Error::user(format!("no tool {name}.")));
		}
		self.run(name, ToolCall { args: Args::new(args, name), boss, pane_id, unkeyed }).await
	}

	/// Only the boss pane an earlier run started, which this run hasn't given its key yet, gets told why; a wrong key, or
	/// no key once the boss pane has this run's, gets the plain refusal, which reveals nothing.
	fn boss_refusal(&self, name: &str, pane_id: Option<&str>, caller: &Caller) -> Error {
		let boss_pane = self.c.boss().map(|boss| boss.with(|boss| boss.pane_id.clone()));
		let sent_key = caller.boss_key.as_deref().is_some_and(|key| !key.is_empty());
		if let (Some(pane), Some(boss_pane)) = (pane_id, boss_pane)
			&& pane == boss_pane
			&& !sent_key
			&& self.c.panes.boss_keyed().as_deref() != Some(boss_pane.as_str())
		{
			return Error::user(format!(
				"{name}: this boss session started before Jocasta's current run and lacks its boss key; Jocasta starts it again with the key once it is idle."
			));
		}
		Error::user(format!(
			"{name} is the boss's tool; only the boss pane Jocasta started in workspace Jocasta can use it."
		))
	}

	async fn run(&self, name: &str, call: ToolCall<'_>) -> Result<String> {
		let c = &self.c;
		let args = &call.args;
		match name {
			HISTORY_SEARCH => self.history_tool(&call),
			"projects" => c.status.projects_text().await,
			"thread_start" => self.start_tool(args, Door::Boss).await,
			"thread_status" => c.status.thread_status(args.optional("thread").as_deref(), None).await,
			"thread_send" => {
				let thread = c.threads.find(&args.text("thread")?)?;
				self.send(&thread, &args.text("text")?, Door::Boss).await
			}
			"thread_finish" => {
				let thread = c.threads.find(&args.text("thread")?)?;
				c.finisher.finish(&thread, &args.text("how")?, Door::Boss).await
			}
			"thread_stop" => self.stop_named(&args.text("thread")?, Door::Boss).await,
			REPORT_RESULT => self.report_result(call.pane_id.as_deref(), call.args.values, call.unkeyed).await,
			REVIEW_NOTE => self.review_note(call.pane_id.as_deref(), args, call.unkeyed).await,
			REVIEW_TRY => {
				let Some(thread) = call.pane_id.as_deref().and_then(|pane| c.threads.get(pane)) else {
					return Err(Error::user(format!(
						"{REVIEW_TRY}: only a thread's pane tries a change, and this pane isn't a thread's{}.",
						call.unkeyed
					)));
				};
				review_try(c, &thread, args.values).await
			}
			REVIEW_SYSTEM => {
				let Some(thread) = call.pane_id.as_deref().and_then(|pane| c.threads.get(pane)) else {
					return Err(Error::user(format!(
						"{REVIEW_SYSTEM}: only a thread's pane hands in a system view, and this pane isn't a thread's{}.",
						call.unkeyed
					)));
				};
				review_system(c, &thread, args.values).await
			}
			"check" => c.review.check_tool(call.pane_id.as_deref(), call.unkeyed).await,
			"worktree_remove" => self.remove_leftover(&args.text("path")?, Door::Boss).await,
			other => self.run_boss(other, args).await,
		}
	}

	/// The boss's tools that take no thread and no pane.
	async fn run_boss(&self, name: &str, args: &Args<'_>) -> Result<String> {
		let c = &self.c;
		match name {
			"show" => self.show(args).await,
			"today" => c.day.today_text(),
			"note" => self.note(args),
			"remind" => self.remind(args),
			"inbox_done" => self.inbox_done(args),
			"worklog" => self.worklog(args).await,
			"lesson" => {
				let project = c.places.find(&args.text("project")?)?;
				self.lesson(
					&project,
					&Lesson {
						title: args.text("title")?,
						applies_when: args.text("applies_when")?,
						rule: args.text("rule")?,
						source: args.optional("source"),
						retire_when: args.optional("retire_when"),
					},
				)
				.await
			}
			"big_picture_review" => self.big_picture_review(args).await,
			other => Err(Error::user(format!("no tool {other}."))),
		}
	}

	/// `history_search`: the boss names the project; any other pane searches its own recorded worktree's project (0484).
	fn history_tool(&self, call: &ToolCall<'_>) -> Result<String> {
		let (c, args) = (&self.c, &call.args);
		let limit = args.count("limit");
		let project = if call.boss {
			let Some(named) = args.optional("project") else {
				return Err(Error::user(format!("{HISTORY_SEARCH}: name the project whose history to search.")));
			};
			c.places.find(&named)?
		} else {
			let project = call.pane_id.as_deref().map(|pane_id| c.starts.pane_project(pane_id)).transpose()?.flatten();
			let Some(project) = project else {
				return Ok(format!(
					"No project here: {HISTORY_SEARCH} answers in the project and thread panes Jocasta started{}.",
					call.unkeyed
				));
			};
			if let Some(named) = args.optional("project")
				&& c.places.find(&named)?.id != project.id
			{
				return Err(Error::user(format!(
					"{HISTORY_SEARCH}: this pane searches only its own project's history ({}).",
					project.name
				)));
			}
			project
		};
		let limit = limit.as_ref().and_then(Value::as_f64).unwrap_or(HISTORY_LIMIT);
		self.history_search(&project, &args.text("query")?, limit)
	}

	/// `thread_start` and `jocasta thread new`: the options as given, read before the start.
	async fn start_tool(&self, args: &Args<'_>, door: Door) -> Result<String> {
		let (project, title, brief);
		if door == Door::Cli {
			brief = args.text("brief")?;
			project = args.text("project")?;
			title = args.optional("title").unwrap_or_else(|| brief_title(&brief));
		} else {
			project = args.text("project")?;
			title = args.text("title")?;
			brief = args.text("brief")?;
		}
		let done_when = done_when_of(args.values.get("done_when"), door)?;
		let heavy = heavy_of(args.values.get("heavy"), door)?;
		let budget = budget_of(args.values.get("budget"), door)?;
		let after = after_of(args.values.get("after"), door)?;
		let options = StartOptions { door, done_when, heavy, budget, after, ..StartOptions::new(door) };
		let started = self.c.starts.start_with(&project, &title, &brief, options).await?;
		Ok(started.text)
	}

	/// `show` (0679, 0714) and `jocasta brief open` (0716): a brief drawn from a spec, checked by `spec::read`, its
	/// reference sections filled from their threads, stored as `show-<n>` and opened in its window.
	async fn show(&self, args: &Args<'_>) -> Result<String> {
		let spec = spec::read(args.values)?;
		let Some(briefs) = self.c.options.briefs.clone() else {
			return Err(Error::user("show: this Jocasta has no page server; nothing can be shown."));
		};
		let mut sections = Vec::with_capacity(spec.sections.len());
		for (index, part) in spec.sections.into_iter().enumerate() {
			sections.push(match part {
				Part::Given(section) => section,
				Part::Thread { of, heading, thread } => {
					let found = self.c.threads.find(&thread).map_err(|error| {
						Error::user(format!("show: section {} ({}): {error}", index + 1, of.type_name()))
					})?;
					thread_brief::reference(&self.c, &found, of, heading).await
				}
			});
		}
		let brief = spec::show_brief(spec.title, sections);
		store::save(&brief)?;
		briefs.open(&brief.id);
		Ok(format!("Shown: {}", brief.title))
	}

	/// `review_note` (0678, 0707): the reviewer hands in one note with the code it is about. It is kept only when the
	/// committed file reads that way at that line (else the tool's error says what the lines are); a finding only once
	/// its proof reproduced where the review's steps run, and a review keeps at most three opinions.
	async fn review_note(&self, pane_id: Option<&str>, args: &Args<'_>, unkeyed: &str) -> Result<String> {
		let c = &self.c;
		let Some(thread) = pane_id.and_then(|pane| c.threads.get(pane)) else {
			return Err(Error::user(format!(
				"{REVIEW_NOTE}: only a thread's pane hands in notes, and this pane isn't a thread's{unkeyed}."
			)));
		};
		let (path, project_id, task_id) =
			thread.with(|t| (t.path.clone(), t.project.id.clone(), t.task.as_ref().map(|task| task.id.clone())));
		let Some(task_id) = task_id else {
			return Err(Error::user(format!("{REVIEW_NOTE}: this thread has no task file to keep notes in.")));
		};
		let head = head_of(&path).await?;
		let file = review_file(&project_id, &task_id);
		let notes = ReviewNotes::of_head(&file, &head);
		if let Some(what) = notes.ended() {
			return Err(Error::user(format!(
				"{REVIEW_NOTE}: the review ended: {what} failed. Hand in nothing more and end your turn."
			)));
		}
		let (mut card, proof) = checked_card(&path, &head, args, notes.notes.len() + 1).await?;
		if let Some(step) = proof {
			card.ran = Some(prove(c, &thread, &step, REVIEW_NOTE, "hand it in as a question or an opinion").await?);
		}
		// A proof can take minutes: the notes are read again before this one joins them.
		let mut notes = ReviewNotes::of_head(&file, &head);
		let opinions = notes.notes.iter().filter(|note| note.kind == Kind::Opinion).count();
		if card.kind == Kind::Opinion && opinions >= MAX_OPINIONS {
			return Err(Error::user(format!(
				"{REVIEW_NOTE}: this review has {MAX_OPINIONS} opinions already, the most it keeps; hand in findings with proofs or questions."
			)));
		}
		card.id = format!("c{}", notes.notes.len() + 1);
		let proven = if card.ran.is_some() { ", proven" } else { "" };
		let said = format!(
			"Note {} stored ({} {} at {}:{}{proven}).",
			card.id,
			card.severity.as_str(),
			card.kind.as_str(),
			card.file,
			card.line
		);
		notes.notes.push(card);
		notes.write(&file)?;
		Ok(said)
	}

	fn note(&self, args: &Args<'_>) -> Result<String> {
		let Some(project_name) = args.optional("project") else {
			return Err(Error::user("note: which project? Notes go into a project's next work."));
		};
		let project = self.c.places.find(&project_name)?;
		let item = self.c.day.note(&args.text("text")?, &project)?;
		Ok(format!("Filed into {}'s next work (inbox {}).", project.name, item.id))
	}

	fn remind(&self, args: &Args<'_>) -> Result<String> {
		let project = args.optional("project").map(|name| self.c.places.find(&name)).transpose()?;
		let item = self
			.c
			.day
			.remind(&args.text("text")?, &args.text("at")?, project.as_ref())
			.map_err(|error| named("remind", error))?;
		let due = item.due_at.as_deref().and_then(|due| chrono::DateTime::parse_from_rfc3339(due).ok());
		let at = due.map_or(0, |due| due.timestamp_millis());
		// Date and time apart: ICU joins dateStyle and timeStyle as "…, 15:00" or "… at 15:00" by version (macOS).
		let label = day_label(DayRef::Time(at), true);
		let date = label.split_once(' ').map_or(label.as_str(), |(_, date)| date);
		let who = project.as_ref().map_or_else(String::new, |project| format!(" ({})", project.name));
		Ok(format!("Reminder {} at {date}, {}{who}; it shows in today.", item.id, hhmm(at)))
	}

	fn inbox_done(&self, args: &Args<'_>) -> Result<String> {
		let id = args.count("id");
		let number = id.as_ref().and_then(Value::as_f64).filter(|n| n.fract() == 0.0 && n.abs() < 9e15);
		#[allow(clippy::cast_possible_truncation)]
		let item = match number {
			Some(number) => self.c.day.inbox(|inbox| inbox.get(number as i64))?,
			None => None,
		};
		match item {
			Some(item) if self.c.day.done(item.id)? => Ok(format!("Done: {}", item.text)),
			_ => Err(Error::user(format!("no open inbox item {}.", js_string(id.as_ref())))),
		}
	}

	async fn worklog(&self, args: &Args<'_>) -> Result<String> {
		let c = &self.c;
		let projects = match args.optional("project") {
			Some(name) => vec![c.places.find(&name)?],
			None => c.places.listed(),
		};
		let written = c
			.day
			.worklog(args.optional("date").as_deref(), Some(&projects))
			.await
			.map_err(|error| named("worklog", error))?;
		let path = written.path.display();
		Ok(if written.projects.is_empty() {
			format!("Nothing to log; {path} is unchanged.")
		} else {
			format!("Worklog written to {path}: {}.", written.projects.join(", "))
		})
	}

	/// `big_picture_review`: started now, its result an [event] once it ends (0577): the boss's turn doesn't wait
	/// minutes for it.
	async fn big_picture_review(&self, args: &Args<'_>) -> Result<String> {
		let c = &self.c;
		let project = c.places.find(&args.text("project")?)?;
		if c.weekly.running(&project.id) {
			return Ok(format!(
				"{}'s big-picture review is already running; its result comes as an [event].",
				project.name
			));
		}
		if c.weekly.range(&project).await?.is_none() {
			return Ok(format!(
				"{} had fewer than two commits on its trunk in the past 7 days; nothing to review.",
				project.name
			));
		}
		let ctx = Arc::clone(c);
		let reviewed = project.clone();
		tokio::spawn(async move {
			let line = match ctx.weekly.review(&reviewed).await {
				Ok(said) => format!(
					"[event] Weekly review: {} — {}",
					reviewed.name,
					said.unwrap_or_else(|| {
						"fewer than two commits on its trunk in the past 7 days; nothing to review".to_owned()
					})
				),
				Err(error) => format!("[event] Weekly review of {}: {error}", reviewed.name),
			};
			ctx.events.tell(&line, ToastPriority::Normal, None).await;
		});
		Ok(format!("Started {}'s big-picture review; its result comes as an [event].", project.name))
	}

	/// One `jocasta thread|worktree` command (0544), or a brief window's (0721).
	pub async fn cli(&self, command: &str, args: &Map<String, Value>) -> Result<String> {
		let c = &self.c;
		let args = Args::new(args, format!("jocasta {command}"));
		match command {
			"ping" => Ok("ok".to_owned()),
			// `jocasta status --json` (0700): what the service holds, for a status bar.
			"status json" => c.status.status_json(),
			// A sleeping pane's key was pressed (0676): its sleeper asks for its omp back. Internal, no Allow.
			"wake" => c.sleep.wake_by_key(&args.text("key")?).await,
			// A brief window's button (0721): the brief's action through the same gate; the brief after it as JSON.
			"brief act" => {
				let action = args.values.get("action").cloned().unwrap_or(Value::Null);
				Ok(brief_actions::act(c, self, &args.text("id")?, &action).await?.to_string())
			}
			// Whether a brief window's buttons have a live thread behind them (0721).
			"brief live" => Ok(brief_actions::live(c, &args.text("id")?).to_string()),
			// `jocasta brief open` (0716): a spec opened as the boss's `show` opens it.
			"brief open" => self.show(&args).await,
			// The pane's visible grid as text (0683): the benchmark and live checks read panes through the pipe.
			"pane_text" => Ok(c.herdr.screen(&args.text("pane")?).await.map_err(Error::from)?),
			// The panes as JSON text (0683): the benchmark and live checks list the shell's panes through the pipe.
			"panes" => self.panes_json().await,
			// `jocasta diagnose` (0472): a read-only omp in a tab of the boss's workspace.
			"diagnose tab" => self.diagnose_tab(&args).await,
			"thread new" => self.start_tool(&args, Door::Cli).await,
			"thread list" => {
				let project = args.optional("project").map(|name| c.places.find(&name)).transpose()?;
				c.status.thread_status(None, project.as_ref()).await
			}
			"thread status" => c.status.thread_status(Some(&args.text("thread")?), None).await,
			"thread send" => {
				let thread = c.threads.find(&args.text("thread")?)?;
				self.send(&thread, &args.text("text")?, Door::Cli).await
			}
			"thread stop" => self.stop_named(&args.text("thread")?, Door::Cli).await,
			"thread finish" => {
				let thread = c.threads.find(&args.text("thread")?)?;
				c.finisher.finish(&thread, &args.text("how")?, Door::Cli).await
			}
			"worktree list" => self.leftovers_text().await,
			"worktree remove" => self.remove_leftover(&args.text("path")?, Door::Cli).await,
			_ => Err(Error::user(format!("no command jocasta {command}."))),
		}
	}

	/// Every pane as `{"pane_id","workspace_id","workspace","tab_id","agent_state","agent","label","cwd","session_path",
	/// "root"}` (`root` is the pid of the pane's process), read together from the surface.
	async fn panes_json(&self) -> Result<String> {
		let herdr = &self.c.herdr;
		let snapshot = herdr.snapshot().await?;
		let mut panes = Vec::with_capacity(snapshot.panes.len());
		for pane in &snapshot.panes {
			let workspace = snapshot.workspaces.iter().find(|workspace| workspace.id == pane.workspace_id);
			let root = herdr.pane_root(&pane.pane_id, Some(2_000)).await.ok().flatten();
			panes.push(json!({
				"pane_id": pane.pane_id,
				"workspace_id": pane.workspace_id,
				"workspace": workspace.map(|workspace| workspace.label.as_str()),
				"tab_id": pane.tab_id,
				"agent_state": pane.agent_state.as_str(),
				"agent": pane.agent,
				"label": pane.label,
				"cwd": pane.cwd,
				"session_path": pane.session_path,
				"root": root,
			}));
		}
		Ok(Value::Array(panes).to_string())
	}

	/// `jocasta diagnose` asks for its read-only omp to open as a tab of the boss's workspace and be brought to the
	/// front. Only an omp command is run, so the pipe's token never starts another program.
	async fn diagnose_tab(&self, args: &Args<'_>) -> Result<String> {
		let command: Vec<String> = match args.values.get("command") {
			Some(Value::Array(words)) => words.iter().filter_map(|word| word.as_str().map(str::to_owned)).collect(),
			_ => Vec::new(),
		};
		let is_omp = command.first().is_some_and(|program| {
			Path::new(program).file_stem().is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case("omp"))
		});
		if !is_omp {
			return Err(Error::user("jocasta diagnose tab: the command must be omp."));
		}
		let cwd = std::path::PathBuf::from(args.text("cwd")?);
		let herdr = &self.c.herdr;
		let workspace = herdr
			.workspaces()
			.await?
			.into_iter()
			.find(|workspace| workspace.label == crate::frontier::server::boss::BOSS_WORKSPACE)
			.ok_or_else(|| {
				Error::user("jocasta diagnose tab: Jocasta's workspace isn't open yet; try again in a moment.")
			})?;
		let spec = crate::frontier::server::surface::TabSpec { title: "diagnose".to_owned(), cwd, command, env: None };
		let tab = herdr.create_tab(&workspace.id, &spec).await?;
		herdr.focus(&tab.pane_id).await?;
		Ok(format!("Opened a read-only diagnose pane in workspace {}.", workspace.label))
	}

	async fn leftovers_text(&self) -> Result<String> {
		let leftovers = self.c.status.leftovers().await?;
		if leftovers.is_empty() {
			return Ok("No leftover worktrees.".to_owned());
		}
		Ok(leftovers
			.iter()
			.map(|l| {
				let changed = if l.dirty == 0 { "clean".to_owned() } else { format!("{} changed", l.dirty) };
				format!(
					"{}  {} [{}] {} ahead, {changed}",
					l.folder.display(),
					l.project,
					l.branch.as_deref().unwrap_or("detached"),
					l.ahead
				)
			})
			.collect::<Vec<_>>()
			.join("\n"))
	}

	/// `history_search` (0389, 0484, 0603, 0604): the project's best lines for the query, each quoted «…» as data with its
	/// source and date, as outside text reaches the boss; sealed to the project.
	pub fn history_search(&self, project: &Project, query: &str, limit: f64) -> Result<String> {
		let most = limit.trunc().clamp(1.0, MAX_HISTORY_HITS);
		#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
		let most = most as usize;
		let hits = {
			let db = self.c.catalog.lock().unwrap_or_else(PoisonError::into_inner);
			search_history(&db, &project.id, query, most)?
		};
		Ok(history_answer(&project.name, &hits))
	}

	/// Refuses a send into a thread whose question or Plan Review waits on him (boss manual rule 4; 0405, 0407).
	async fn refuse_send(&self, thread: &ThreadRef, door: Door) -> Result<()> {
		let name = thread.with(|t| subject(t));
		let who = person(door);
		if !self.c.threads.includes(thread) {
			return Err(Error::user(format!("{name} closed meanwhile.")));
		}
		match self.c.lifecycle.awaiting_him(thread).await? {
			Some(Awaiting::Question) => Err(Error::user(if door == Door::Boss {
				format!("{name} is awaiting his answer to its question; he answers it in its pane.")
			} else {
				format!("{name} is awaiting your answer to its question; answer it in its pane.")
			})),
			Some(Awaiting::Plan) => Err(Error::user(if door == Door::Boss {
				format!("{name}'s plan is ready and awaiting {} approval; {} decides in its pane.", who.his, who.he)
			} else {
				format!("{name}'s plan is ready and awaiting your approval; decide in its pane.")
			})),
			None => Ok(()),
		}
	}

	/// `thread_send` and `jocasta thread send` (0544): never typed into a question or omp's Plan Review, whose default
	/// would approve the plan (boss manual rule 4; 0405, 0407), judged on herdr's pane and the session as they are now.
	/// The command waits for his Allow first and looks again after it. Worded for the door (0581).
	pub async fn send(&self, thread: &ThreadRef, text: &str, door: Door) -> Result<String> {
		let name = thread.with(|t| subject(t));
		self.refuse_send(thread, door).await?;
		if door == Door::Cli {
			let ask = Confirmation {
				message: format!("Send this to {name}?"),
				detail: format!(
					"{} asks to type it into the thread's pane:\n\n{}",
					asker(door, "thread send"),
					js_trim(text)
				),
			};
			self.c.allow.ask(&ask, "nothing was sent", door).await?;
			self.refuse_send(thread, door).await?;
		}
		self.c.sleep.wake_thread(thread).await?;
		// His word ends a tripped breaker (0677) and goes into the journal as a send back after a ready review.
		self.c.review.reset_breaker(thread);
		thread.with(|t| {
			if t.review.as_ref().is_some_and(|review| review.result.is_some()) {
				self.c.journal.thread("sent_back", t, json!({}));
			}
		});
		let pane_id = thread.with(|t| {
			t.sent_at = Some(now_ms());
			t.pane_id.clone()
		});
		self.c.bridge.prompt(&pane_id, self.c.bridge.seal(text), Deliver::Steer).await?;
		Ok(format!("Sent to {name}."))
	}

	/// `thread_stop` and `jocasta thread stop` by name: an open thread's run is stopped; a start that waits (0687) is
	/// removed, and what waited for it is blocked.
	async fn stop_named(&self, wanted: &str, door: Door) -> Result<String> {
		let c = &self.c;
		match c.threads.find(wanted) {
			Ok(thread) => self.stop(&thread, door).await,
			Err(error) => match c.pending.remove(wanted).await? {
				Some(removed) => Ok(format!("Removed the waiting start {}.", removed.title)),
				None => Err(error),
			},
		}
	}

	/// `thread_stop` and `jocasta thread stop` (0544): one Escape into its omp; the command waits for his Allow first.
	pub async fn stop(&self, thread: &ThreadRef, door: Door) -> Result<String> {
		let name = thread.with(|t| subject(t));
		if self.c.launched.asleep(&thread.with(|t| t.pane_id.clone())) {
			return Ok(format!("{name} is asleep; nothing is running to stop."));
		}
		if door == Door::Cli {
			let ask = Confirmation {
				message: format!("Stop {name}'s run?"),
				detail: format!(
					"{} asks to stop the thread's current run (one Escape into its omp).",
					asker(door, "thread stop")
				),
			};
			self.c.allow.ask(&ask, "nothing was stopped", door).await?;
			if !self.c.threads.includes(thread) {
				return Err(Error::user(format!("{name} closed meanwhile.")));
			}
		}
		let pane_id = thread.with(|t| t.pane_id.clone());
		self.c.lifecycle.abort_turn(&pane_id).await?;
		// What waits for this thread's work in trunk won't get it from a thread he stopped (0687).
		if let Some(task_id) = thread.with(|t| t.task.as_ref().map(|task| task.id.clone())) {
			self.c.pending.block_waiting_on(&task_id, "stopped").await;
		}
		self.c.pending.tick_soon();
		Ok(format!("Stopped {name}'s run."))
	}

	/// `report_result` (0539): a thread's report, validated against its schema and stored with the thread (its task
	/// file's record, made now for a thread found in herdr without one). Its decisions go into the project's history,
	/// and the boss hears them so it can offer the lasting ones to him as lessons (0459's Allow).
	pub async fn report_result(
		&self,
		pane_id: Option<&str>,
		args: &Map<String, Value>,
		unkeyed: &str,
	) -> Result<String> {
		let c = &self.c;
		let Some(thread) = pane_id.and_then(|pane| c.threads.get(pane)) else {
			return Err(Error::user(format!(
				"{REPORT_RESULT}: only a thread's pane hands in a report, and this pane isn't a thread's{unkeyed}."
			)));
		};
		let report = parse_report(&Value::Object(args.clone())).map_err(Error::user)?;
		// The full check decides before anything is stored (0677); what it ran and what git says are the report's tests
		// and files, whatever the thread said of them.
		let tests = c.review.full_gate(&thread).await?;
		let files = c.review.changed_files(&thread).await;
		let report = ThreadReport { files, tests, ..report };
		let at = now_ms();
		let title = thread.with(|t| {
			if t.task.is_none() {
				t.task = Some(new_task(
					NewTask {
						project_id: t.project.id.clone(),
						project: t.project.name.clone(),
						title: t.title.clone(),
						brief: String::new(),
						done_when: Vec::new(),
						path: Some(t.path.to_string_lossy().into_owned()),
						branch: Some(t.branch.clone()),
						..NewTask::default()
					},
					None,
				));
			}
			if let Some(task) = t.task.as_mut() {
				task.report = Some(TaskReport { at, value: report.clone() });
			}
			t.title.clone()
		});
		c.status.write_task(&thread);
		thread.with(|t| c.journal.thread("report", t, json!({ "outcome": report.outcome })));
		if !report.decisions.is_empty() {
			self.index_decisions(&thread, at, &report.decisions);
			let said = report
				.decisions
				.iter()
				.map(|d| quoted(&format!("{} = {} ({})", d.question, d.choice, d.why)))
				.collect::<Vec<_>>()
				.join(" ");
			let what = format!("reported its decisions: {said} · offer him the lasting ones as lessons, once");
			c.events.thread(&thread, &what, ToastPriority::Normal, None).await;
		}
		Ok(format!("Report stored for {title}; its review reads it."))
	}

	/// A report's decisions into the project's history (0539, 0603); a failure is logged.
	fn index_decisions(&self, thread: &ThreadRef, at: i64, decisions: &[ReportedDecision]) {
		let (project_id, name, source, title, branch, project) = thread.with(|t| {
			let id = t.task.as_ref().map_or_else(|| t.branch.clone(), |task| task.id.clone());
			(
				t.project.id.clone(),
				subject(t),
				format!("decisions {id} {}", t.title),
				t.title.clone(),
				t.branch.clone(),
				t.project.name.clone(),
			)
		});
		let lines = decisions.iter().map(|d| format!("- {}: {}. Why: {}", d.question, d.choice, d.why));
		let text = std::iter::once(format!("Decisions settled by the thread \"{title}\" ({branch}) in {project}:"))
			.chain(lines)
			.collect::<Vec<_>>()
			.join("\n");
		let added = {
			let db = self.c.catalog.lock().unwrap_or_else(PoisonError::into_inner);
			add_history(&db, &project_id, &HistoryEntry { source: &source, at, text: &text })
		};
		if let Err(error) = added {
			self.c.warn(format!("{name}'s decisions didn't reach the history: {error}"));
		}
	}

	/// `worktree_remove` and `jocasta worktree remove` (0456, 0484, 0544): a leftover worktree goes after his Allow; a
	/// dirty one is refused, its branch stays.
	pub async fn remove_leftover(&self, path: &str, door: Door) -> Result<String> {
		let c = &self.c;
		// Through links and short names (macOS's /tmp is /private/tmp, git names a worktree by its real path).
		let wanted = Path::new(js_trim(path));
		let found = c.status.leftovers().await?.into_iter().find(|l| same_dir(&l.folder, wanted));
		let Some(leftover) = found else {
			return Err(Error::user(format!(
				"{path} isn't a leftover worktree (a thread folder no open thread holds); nothing removed."
			)));
		};
		let folder = leftover.folder.display();
		if leftover.dirty > 0 {
			let plural = if leftover.dirty == 1 { "" } else { "s" };
			return Err(Error::user(format!(
				"{folder} has {} uncommitted change{plural}; nothing removed.",
				leftover.dirty
			)));
		}
		let project = c.places.find(&leftover.project)?;
		let plural = if leftover.ahead == 1 { "" } else { "s" };
		let branch_line = match &leftover.branch {
			Some(branch) => format!("Its branch {branch} stays ({} commit{plural} ahead).", leftover.ahead),
			None => "It has no branch.".to_owned(),
		};
		let base = leftover.folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
		let ask = Confirmation {
			message: format!("Remove the leftover worktree {base} of {}?", project.name),
			detail: [format!("{} {} to delete {folder}.", asker(door, "worktree remove"), asks(door)), branch_line]
				.join("\n"),
		};
		c.allow.ask(&ask, "nothing removed", door).await?;
		let failed = remove_checkout(&project.main_path, &leftover.folder, None).await;
		// A folder that stays keeps its port offset number (0457, SVC-14).
		if failed.is_none() {
			c.numbers.forget(&leftover.folder)?;
		}
		let again = Arc::clone(c);
		c.calls.spawn(async move { again.day.refresh_leftovers().await });
		if let Some(failed) = failed {
			return Err(Error::user(format!("{folder} couldn't be removed ({failed}); it stays.")));
		}
		if let Some(mut task) = find_task(&project.id, &leftover.folder) {
			close_task(&mut task, CloseFacts { how: CloseHow::Removed, commit: None }, |error| c.warn(error), None);
		}
		let stays = leftover
			.branch
			.as_ref()
			.map_or_else(String::new, |branch| format!("; branch {branch} stays ({} ahead)", leftover.ahead));
		Ok(format!("Removed {folder}{stays}."))
	}

	/// `lesson` (0459, 0471, 0484): writes a lesson for the project once he confirms its text in his Allow window,
	/// `<slug>.md` beside the skill's `SKILL.md` (front matter `title`, `applies_when`, `date`, `source`, optional
	/// `retire_when`; the rule as its body), and its line in `SKILL.md`'s index under `## Lessons`. A lesson of the same
	/// title is replaced, not duplicated.
	pub async fn lesson(&self, project: &Project, lesson: &Lesson) -> Result<String> {
		// `skill.md` would be the index on a case-insensitive disk.
		let slug = branch_slug(&lesson.title);
		let slug = if slug == "skill" { "skill-lesson".to_owned() } else { slug };
		if slug.is_empty() {
			return Err(Error::user("lesson: the title needs letters or digits for its file name."));
		}
		let source = lesson.source.as_deref().map(js_trim).filter(|source| !source.is_empty());
		let mut detail = vec![
			format!("Title: {}", js_trim(&lesson.title)),
			format!("Applies when: {}", js_trim(&lesson.applies_when)),
			format!("Source: {}", source.unwrap_or("none given; it's saved as your word")),
		];
		if let Some(retire) = lesson.retire_when.as_deref().filter(|retire| !retire.is_empty()) {
			detail.push(format!("Retire when: {}", js_trim(retire)));
		}
		detail.extend([String::new(), js_trim(&lesson.rule).to_owned()]);
		let ask = Confirmation {
			message: format!("Save this lesson for {}? Every new thread there reads it first.", project.name),
			detail: detail.join("\n"),
		};
		self.c.allow.ask(&ask, "the lesson wasn't saved", Door::Boss).await?;
		let dir = ensure_lessons_skill(&project.id, None)?;
		let file = format!("{slug}.md");
		let existed = dir.join(&file).exists();
		write_atomically(&dir.join(&file), lesson_file(lesson), None)?;
		let index = dir.join("SKILL.md");
		let entry = format!("- [{}]({file}) — {}", flatten(&lesson.title), flatten(&lesson.applies_when));
		let text = std::fs::read_to_string(&index)?;
		write_atomically(&index, indexed(&text, &file, &entry), None)?;
		Ok(format!(
			"{} the lesson \"{}\" for {} ({}).",
			if existed { "Updated" } else { "Saved" },
			js_trim(&lesson.title),
			project.name,
			dir.join(&file).display()
		))
	}
}

/// A lesson's file: front matter, then the rule.
fn lesson_file(lesson: &Lesson) -> String {
	let flat = |value: &str| serde_json::to_string(&flatten(value)).unwrap_or_default();
	let mut lines = vec![
		"---".to_owned(),
		format!("title: {}", flat(&lesson.title)),
		format!("applies_when: {}", flat(&lesson.applies_when)),
		format!("date: {}", local_day(now_ms())),
		format!("source: {}", flat(lesson.source.as_deref().unwrap_or("his word"))),
	];
	if let Some(retire) = lesson.retire_when.as_deref().filter(|retire| !retire.is_empty()) {
		lines.push(format!("retire_when: {}", flat(retire)));
	}
	lines.extend(["---".to_owned(), js_trim(&lesson.rule).to_owned(), String::new()]);
	lines.join("\n")
}

/// `SKILL.md` with `entry` in place of the line that links `file`, or added under `## Lessons`.
fn indexed(text: &str, file: &str, entry: &str) -> String {
	let body = text.trim_end_matches(js_space);
	let mut lines: Vec<&str> = body.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
	let link = format!("]({file})");
	if let Some(at) = lines.iter().position(|line| line.contains(&link)) {
		lines[at] = entry;
	} else {
		let heading = Regex::new(r"^##\s+Lessons\s*$").ok();
		if !lines.iter().any(|line| heading.as_ref().is_some_and(|heading| heading.is_match(line))) {
			lines.extend(["", "## Lessons"]);
		}
		lines.push(entry);
	}
	format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
