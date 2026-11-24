//! The day's views: today's activity and the dashboard (0429), `today` (0300) and its text, the inbox (0302) and the
//! worklog on request.

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use regex::Regex;
use rusqlite::params;

use super::{
	ACTIVITY_ROWS, Day, DayThread, EventPriority, PollerName, STUCK_MS, WORKLOG_SO_FAR_MS, event, iso, local, lock,
};
use crate::frontier::protocol::{
	Activity, ActivityEvent, ActivitySummary, CalendarEntry, Dashboard, LandedItem, NeedsYou, NeedsYouKind, NextItem,
	PrivateRow, PrivateSection, RailItem, ServerMessage, ThreadState, Today, wait_label,
};
use crate::frontier::server::worklog::{WorklogWritten, commits_async, project_day, project_day_line, write_worklog};
use crate::shared::day::{DayRef, day_label, day_start, hhmm, local_day, midnight_of};
use crate::shared::errors::{Error, Result};
use crate::shared::events::inbox::{InboxItem, InboxWhere};
use crate::shared::events::queue::{EventRow, EventSource};
use crate::shared::pollers::calendar::Meeting;
use crate::shared::projects::Project;

/// `Date.parse` of an ISO time as the day writes them (`toISOString`); 0 when it isn't one.
pub(super) fn parse_iso(text: &str) -> i64 {
	DateTime::parse_from_rfc3339(text).map_or(0, |at| at.timestamp_millis())
}

/// A thread that landed, for `Day::landed`: its pane and title, its project, how it landed, and its PR when sent as one.
pub struct Landing<'a> {
	pub pane_id: &'a str,
	pub title: &'a str,
	pub project_id: &'a str,
	pub line: &'a str,
	pub url: Option<&'a str>,
}

/// The epoch ms of local `naive` time; a time the clock skips (a spring-forward gap) moves on to the first one after
/// it, and a repeated one is its first, as JavaScript's `new Date(y, m, d, h, mi)` does.
fn local_ms(naive: NaiveDateTime) -> i64 {
	let mut at = naive;
	for _ in 0..(24 * 4) {
		match Local.from_local_datetime(&at) {
			chrono::LocalResult::Single(time) => return time.timestamp_millis(),
			chrono::LocalResult::Ambiguous(first, _) => return first.timestamp_millis(),
			chrono::LocalResult::None => at += Duration::minutes(15),
		}
	}
	naive.and_utc().timestamp_millis()
}

/// When a reminder is due (0302), from the boss: `HH:MM` (today, or tomorrow once that time passed), `YYYY-MM-DD HH:MM`
/// or an ISO time, as epoch ms. Impossible times and moments already past are refused (F-11) with a one-line reason.
pub fn parse_when(at: &str, now: i64) -> std::result::Result<i64, String> {
	let value = at.trim();
	let impossible = format!("\"{at}\" is not a time: hours run 00–23 and minutes 00–59");
	let time = Regex::new(r"^([0-9]{1,2}):([0-9]{2})$").ok().and_then(|re| re.captures(value));
	if let Some(time) = time {
		let (h, mi): (u32, u32) = (time[1].parse().unwrap_or(99), time[2].parse().unwrap_or(99));
		if h > 23 || mi > 59 {
			return Err(impossible);
		}
		let today = local(now).date_naive();
		let due = local_ms(today.and_time(NaiveTime::from_hms_opt(h, mi, 0).unwrap_or(NaiveTime::MIN)));
		if due > now {
			return Ok(due);
		}
		let tomorrow = today.succ_opt().unwrap_or(today);
		return Ok(local_ms(tomorrow.and_time(NaiveTime::from_hms_opt(h, mi, 0).unwrap_or(NaiveTime::MIN))));
	}
	let dated = Regex::new(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})[ T]([0-9]{1,2}):([0-9]{2})$")
		.ok()
		.and_then(|re| re.captures(value));
	let due = if let Some(parts) = dated {
		let number = |i: usize| parts[i].parse::<u32>().unwrap_or(99);
		let (y, mo, d, h, mi) = (number(1), number(2), number(3), number(4), number(5));
		if h > 23 || mi > 59 {
			return Err(impossible);
		}
		// `new Date` rolls 2026-02-30 over to March (and reads a year under 100 as 19xx); a day that doesn't exist is
		// refused instead.
		let date = NaiveDate::from_ymd_opt(i32::try_from(y).unwrap_or(0), mo, d).filter(|_| y >= 100);
		let Some(date) = date else { return Err(format!("\"{at}\" is not a day on the calendar")) };
		Some(local_ms(date.and_time(NaiveTime::from_hms_opt(h, mi, 0).unwrap_or(NaiveTime::MIN))))
	} else if value.len() > 10 && value.as_bytes()[..4].iter().all(u8::is_ascii_digit) && value.as_bytes()[10] == b'T' {
		iso_time(value)
	} else {
		None
	};
	let Some(due) = due else { return Err(format!("\"{at}\" is not a time: use HH:MM or YYYY-MM-DD HH:MM")) };
	if due <= now {
		return Err(format!("\"{at}\" has already passed: give a time still to come"));
	}
	Ok(due)
}

/// An ISO date-time as `new Date(text)` reads one: with an offset or `Z`, or in local time without.
fn iso_time(value: &str) -> Option<i64> {
	if let Ok(at) = DateTime::parse_from_rfc3339(value) {
		return Some(at.timestamp_millis());
	}
	if let Ok(at) = DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M%:z") {
		return Some(at.timestamp_millis());
	}
	["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"]
		.iter()
		.find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
		.map(local_ms)
}

/// The first `max` UTF-16 units of `text`, never half of a character.
fn first_units(text: &str, max: usize) -> String {
	let mut used = 0;
	text.chars()
		.take_while(|c| {
			used += c.len_utf16();
			used <= max
		})
		.collect()
}

/// A thread row's `RailItem` fields.
fn rail(t: &DayThread) -> RailItem {
	RailItem {
		pane_id: t.pane_id.clone(),
		title: t.title.clone(),
		project: t.project.clone(),
		line: t.line.clone(),
		overlaps: t.overlaps.clone().filter(|overlaps| !overlaps.is_empty()),
		state: None,
		since: None,
		cost: None,
		context: None,
		diff: t.diff.clone(),
		check: t.check,
		stuck_for: None,
		halted: t.halted.clone(),
		progress: t.progress.clone(),
		budget: t.budget,
	}
}

/// How long a working thread has not written its session file, when that is long enough to call it stuck (0668).
fn stuck_for(t: &DayThread, now: i64) -> Option<i64> {
	let silent = now - t.last_write_ms?;
	(t.state == ThreadState::Working && silent >= STUCK_MS).then_some(silent)
}

fn needs(kind: NeedsYouKind, text: String) -> NeedsYou {
	NeedsYou { kind, text, url: None, pane_id: None, project: None, time: None, since: None, overlaps: None }
}

/// A thread's row in Awaiting you: its pane, project and overlaps.
fn needs_thread(kind: NeedsYouKind, text: String, since: i64, t: &DayThread) -> NeedsYou {
	NeedsYou {
		since: Some(iso(since)),
		pane_id: Some(t.pane_id.clone()),
		project: t.project.clone().filter(|project| !project.is_empty()),
		overlaps: t.overlaps.clone().filter(|overlaps| !overlaps.is_empty()),
		..needs(kind, text)
	}
}

impl Day {
	// ---- the dashboard's third column (0429) ----

	/// Today's activity (0429): the latest delivered `[event]` lines, the worklog so far and the compaction and handoff
	/// summaries saved today, each with its project. The worklog so far is recomputed every 10 minutes off the event
	/// loop (it reads every project's sessions and git log, CO-12), and only once today's sessions were read in steps:
	/// `Some(true)` allows it from now on, `Some(false)` holds it (CO-11).
	pub fn refresh_activity(&self, worklog: Option<bool>) {
		if let Some(ready) = worklog {
			self.st().worklog_ready = ready;
		}
		self.guard("activity", || self.refresh_activity_now());
	}

	fn refresh_activity_now(&self) -> Result<()> {
		let now = self.now_ms();
		let midnight = midnight_of(now);
		let midnight_iso = iso(midnight);
		let (stored, summaries) = {
			let db = lock(&self.inner.catalog);
			let mut events = db.prepare_cached(
				"SELECT text, raised_at FROM events WHERE raised_at >= ? ORDER BY raised_at DESC, id DESC LIMIT ?",
			)?;
			let stored = events
				.query_map(params![midnight_iso, ACTIVITY_ROWS as i64], |row| {
					Ok(ActivityEvent { text: row.get("text")?, at: row.get("raised_at")? })
				})?
				.collect::<rusqlite::Result<Vec<_>>>()?;
			let mut rows =
				db.prepare_cached("SELECT cwd, at, summary FROM summaries WHERE at >= ? ORDER BY at DESC LIMIT ?")?;
			let summaries = rows
				.query_map(params![midnight, ACTIVITY_ROWS as i64], |row| {
					Ok((row.get::<_, String>("cwd")?, row.get::<_, i64>("at")?, row.get::<_, String>("summary")?))
				})?
				.collect::<rusqlite::Result<Vec<_>>>()?;
			(stored, summaries)
		};
		let summaries = summaries
			.into_iter()
			.map(|(cwd, at, summary)| {
				// A thread's summary is in its worktree: the service knows its project (N-4).
				let project = self.inner.hooks.project_name(std::path::Path::new(&cwd));
				// The first line of content: a heading such as `## Goal` names a section, not what happened.
				let first = summary
					.split('\n')
					.filter(|line| !line.trim_start().starts_with('#'))
					.map(|line| {
						line.trim_start_matches(|c: char| c == '>' || c == '*' || c == '-' || c.is_whitespace()).trim()
					})
					.find(|line| !line.is_empty())
					.unwrap_or("");
				ActivitySummary { at: iso(at), project, text: first_units(first, 200) }
			})
			.collect();
		let due = {
			let mut state = self.st();
			// The service's own thread lines (done and reviewed, closed, PRs) aren't queue rows (N-4).
			state.service_events.retain(|e| e.at >= midnight_iso);
			let mut events: Vec<ActivityEvent> =
				stored.into_iter().chain(state.service_events.iter().cloned()).collect();
			events.sort_by(|a, b| b.at.cmp(&a.at));
			events.truncate(ACTIVITY_ROWS);
			state.activity = Activity { events, worklog: state.activity.worklog.clone(), summaries };
			let due = now - state.worklog_so_far_at >= WORKLOG_SO_FAR_MS || now < state.worklog_so_far_at;
			let start = state.worklog_ready && due && state.worklog_so_far.is_none();
			if start {
				state.worklog_so_far_at = now;
			}
			start
		};
		if due {
			let (finish, done) = tokio::sync::watch::channel(false);
			self.st().worklog_so_far = Some(done);
			let day = self.clone();
			let today = local_day(now);
			tokio::spawn(async move {
				let _finish = super::Finish(finish);
				day.pass_worklog(&today).await;
			});
		}
		Ok(())
	}

	/// The worklog so far: today's sessions listed once and grouped by project, each project's git email and origin
	/// kept for the day, and `git log` run off the event loop, one project after another (CO-12).
	async fn pass_worklog(&self, day: &str) {
		let found = self.worklog_lines(day).await;
		match found {
			Ok(Some(lines)) => {
				{
					let mut state = self.st();
					state.activity.worklog = lines;
					state.last_error.remove("worklog so far");
				}
				self.push_dashboard();
			}
			Ok(None) => {}
			Err(error) => self.report_once("worklog so far", &format!("worklog so far: {error}"), false),
		}
		self.st().worklog_so_far = None;
	}

	/// The worklog lines, or `None` when the day stopped meanwhile.
	async fn worklog_lines(&self, day: &str) -> Result<Option<Vec<String>>> {
		let projects = self.inner.hooks.projects();
		let by_project = self.inner.hooks.sessions_by_project(day);
		let mut lines = Vec::new();
		for project in &projects {
			if self.stopped() {
				return Ok(None);
			}
			let place = self.inner.places.of(project, day).await?;
			let mine = commits_async(project, day, &place).await?;
			let sessions = by_project.get(&project.id).map_or(&[][..], Vec::as_slice);
			if let Some(found) = project_day(day, sessions, &mine) {
				lines.push(project_day_line(&project.name, &found));
			}
		}
		Ok((!self.stopped()).then_some(lines))
	}

	pub fn dashboard(&self) -> Dashboard {
		let state = self.st();
		Dashboard {
			news: state.news.clone(),
			news_error: state.news_error.clone(),
			weather: state.weather.clone(),
			weather_error: state.weather_error.clone(),
			machine: state.machine,
			services: state.services.clone(),
			activity: state.activity.clone(),
			leftovers: state.leftover_list.clone(),
		}
	}

	/// Pushes the dashboard to the page when it changed.
	pub fn push_dashboard(&self) {
		let dashboard = self.dashboard();
		let Ok(json) = serde_json::to_string(&dashboard) else { return };
		{
			let mut state = self.st();
			if state.dashboard_sent == json {
				return;
			}
			state.dashboard_sent = json;
		}
		self.inner.hooks.send(ServerMessage::Dashboard { dashboard });
	}

	// ---- today (0300) ----

	pub fn today(&self) -> Result<Today> {
		let now = self.now_ms();
		let date = local_day(now);
		let (start, end) = (day_start(&date, 0), day_start(&date, 1));
		let yesterday = day_start(&date, -1);
		let meetings: Vec<Meeting> =
			self.st().meetings.iter().filter(|m| m.start < end && m.end > start).cloned().collect();
		let entry = |time: String, m: &Meeting, ends: Option<String>| CalendarEntry {
			time,
			title: m.title.clone(),
			url: m.url.clone().filter(|url| !url.is_empty()),
			end: ends,
		};
		// A timed meeting carries its end (C9, 0581): `today` keeps a started one as "now" until then.
		let calendar = meetings
			.iter()
			.filter(|m| m.all_day)
			.map(|m| entry("all day".to_owned(), m, None))
			.chain(
				meetings
					.iter()
					.filter(|m| !m.all_day && m.start >= start)
					.map(|m| entry(hhmm(m.start), m, Some(hhmm(m.end)))),
			)
			.collect();
		let mut needs_you = Vec::new();
		let open = self.queue(|queue| queue.open())?;
		for row in &open {
			let kind = match row.kind.as_str() {
				"review" => NeedsYouKind::Review,
				"pipeline" => NeedsYouKind::Pipeline,
				"disk" => NeedsYouKind::Disk,
				_ => continue,
			};
			needs_you
				.push(NeedsYou { url: row.url.clone().filter(|url| !url.is_empty()), ..needs(kind, row.text.clone()) });
		}
		let threads = self.inner.hooks.threads();
		self.threads_awaiting(&threads, &mut needs_you);
		// A reminder awaits him once it is due (N-5), not from the morning on; one filed for a project shows its name.
		let projects = self.inner.hooks.projects();
		let project_name = |id: &str| projects.iter().find(|p| p.id == id).map(|p| p.name.clone());
		for item in self.inbox(|inbox| inbox.reminders(start, end))? {
			let due = item.due_at.as_deref().map_or(0, parse_iso);
			if due > now {
				continue;
			}
			needs_you.push(NeedsYou {
				time: Some(hhmm(due)),
				project: item.project_id.as_deref().and_then(project_name),
				..needs(NeedsYouKind::Reminder, item.text)
			});
		}
		let mut next = Vec::new();
		for project in &projects {
			for note in self.inbox(|inbox| inbox.notes(&project.id))? {
				next.push(NextItem { id: note.id, text: note.text, project: Some(project.name.clone()) });
			}
		}
		next.sort_by_key(|note| note.id);
		let mut landed: Vec<LandedItem> = threads
			.iter()
			.filter(|t| t.state == ThreadState::Done && t.reviewed_at.is_none() && t.changed_at >= yesterday)
			.map(|t| landed_item(rail(t)))
			.collect();
		landed.extend(self.finished_since(yesterday, &project_name)?);
		let scheduled = self.scheduled_next(&projects);
		Ok(Today {
			date,
			calendar,
			needs_you,
			landed,
			// Working and idle threads with what `today` shows of them (0555): state, since when, cost and context.
			underway: threads
				.iter()
				.filter(|t| matches!(t.state, ThreadState::Working | ThreadState::Idle))
				.map(|t| RailItem {
					state: Some(t.state),
					since: Some(t.changed_at),
					cost: t.cost,
					context: t.context,
					stuck_for: stuck_for(t, now),
					..rail(t)
				})
				.collect(),
			next,
			scheduled,
			queued: self.inner.hooks.queued(),
			mail: self.private_section(PollerName::Gmail, EventSource::Mail, &open),
			jira: self.private_section(PollerName::Jira, EventSource::Jira, &open),
			azure: self.private_section(PollerName::AzurePortal, EventSource::AzurePortal, &open),
			briefs: self.inner.hooks.briefs(now),
		})
	}

	/// The blocked threads, then the reviewed ones, as Awaiting-you rows.
	/// A private source's section of `today` (0680): there once its poller ran a round, with the rows of its open events
	/// and its last round's failure line.
	fn private_section(&self, poller: PollerName, source: EventSource, open: &[EventRow]) -> Option<PrivateSection> {
		let error = self.st().private_rounds.get(&poller)?.clone();
		let rows = open
			.iter()
			.filter(|row| row.source == source)
			.filter_map(|row| Some(PrivateRow { text: row.private_text.clone()?, url: row.private_url.clone() }))
			.collect();
		Some(PrivateSection { rows, error })
	}

	fn threads_awaiting(&self, threads: &[DayThread], needs_you: &mut Vec<NeedsYou>) {
		let now = self.now_ms();
		// A blocked thread says since when it waits (UX-4, 0581): its last state change.
		for t in threads.iter().filter(|t| t.state == ThreadState::Blocked) {
			let line = if t.line.is_empty() { "awaiting you" } else { &t.line };
			needs_you.push(needs_thread(NeedsYouKind::Blocked, format!("{} — {line}", t.title), t.changed_at, t));
		}
		// A thread the loop cap or the cost cap stopped waits for his word (0677, 0686).
		for t in threads {
			if let Some(why) = &t.halted {
				needs_you.push(needs_thread(NeedsYouKind::Stuck, format!("{} — {why}", t.title), t.changed_at, t));
			}
		}
		// A working thread whose session file wasn't written for 10 minutes (0668): since its last write.
		for t in threads {
			if let (Some(stuck), Some(wrote)) = (stuck_for(t, now), t.last_write_ms) {
				let text = format!("{} — stuck, no writes for {}", t.title, wait_label(stuck));
				needs_you.push(needs_thread(NeedsYouKind::Stuck, text, wrote, t));
			}
		}
		// Reviewed and not finished (0456): they wait on him, oldest first, for as long as it takes.
		let mut reviewed: Vec<(&DayThread, i64)> = threads
			.iter()
			.filter(|t| t.state == ThreadState::Done)
			.filter_map(|t| t.reviewed_at.map(|at| (t, at)))
			.collect();
		reviewed.sort_by_key(|(_, at)| *at);
		for (t, at) in reviewed {
			let line = if t.line.is_empty() { "reviewed" } else { &t.line };
			needs_you.push(needs_thread(NeedsYouKind::Reviewed, format!("{} — {line}", t.title), at, t));
		}
	}

	/// A thread that landed (0263, 0300): squash-merged into trunk or sent as a PR, its pane gone. Kept as a low event
	/// (0299: only `today`), so today's Landed lists it since yesterday, across restarts; `line` says how it landed.
	pub fn landed(&self, landing: &Landing<'_>) -> Result<()> {
		let Landing { pane_id, title, project_id, line, url } = *landing;
		let source_id = format!("thread:landed:{pane_id}");
		let mut landed =
			event(&source_id, EventSource::Thread, "landed", EventPriority::Low, &format!("{title} — {line}"));
		landed.project_id = Some(project_id.to_owned());
		landed.url = url.filter(|url| !url.is_empty()).map(str::to_owned);
		self.queue(|queue| queue.add(&landed))?;
		self.queue(|queue| queue.resolve(&source_id))?;
		self.push_today()
	}

	/// The threads `landed` recorded since `since` (epoch ms), oldest first, as Landed rows without a pane.
	fn finished_since(&self, since: i64, project_name: &dyn Fn(&str) -> Option<String>) -> Result<Vec<LandedItem>> {
		let rows = {
			let db = lock(&self.inner.catalog);
			let mut statement = db.prepare_cached(
				"SELECT text, project_id, url FROM events WHERE kind = 'landed' AND created_at >= ? ORDER BY created_at, id",
			)?;
			statement
				.query_map(params![iso(since)], |row| {
					Ok((
						row.get::<_, String>("text")?,
						row.get::<_, Option<String>>("project_id")?,
						row.get::<_, Option<String>>("url")?,
					))
				})?
				.collect::<rusqlite::Result<Vec<_>>>()?
		};
		Ok(rows
			.into_iter()
			.map(|(text, project_id, url)| {
				// The line is Jocasta's own words, without " — "; a title may hold one.
				let cut = text.rfind(" — ");
				LandedItem {
					pane_id: None,
					title: cut.map_or_else(|| text.clone(), |at| text[..at].to_owned()),
					project: project_id.as_deref().filter(|id| !id.is_empty()).and_then(project_name),
					line: cut.map_or_else(String::new, |at| text[at + " — ".len()..].to_owned()),
					overlaps: None,
					state: None,
					since: None,
					cost: None,
					context: None,
					pr: url.as_deref().filter(|url| !url.is_empty()).and_then(|url| self.inner.hooks.pr_facts(url)),
				}
			})
			.collect())
	}

	/// Pushes `today` to the page when it changed.
	pub fn push_today(&self) -> Result<()> {
		let today = self.today()?;
		self.push_built_today(today)
	}

	/// Pushes `today` to the page when it changed; `today` is one the caller built already.
	pub fn push_built_today(&self, today: Today) -> Result<()> {
		let json = serde_json::to_string(&today)?;
		{
			let mut state = self.st();
			if state.today_sent == json {
				return Ok(());
			}
			state.today_sent = json;
		}
		self.inner.hooks.send(ServerMessage::Today { today });
		Ok(())
	}

	/// `today` as text for the boss (0300's layout).
	pub fn today_text(&self) -> Result<String> {
		let t = self.today()?;
		let label = day_label(DayRef::Day(&t.date), false);
		let row = |name: &str, items: Vec<String>| {
			let said = if items.is_empty() { "nothing".to_owned() } else { items.join(" · ") };
			format!("{name:<12} {said}")
		};
		let now = self.now_ms();
		let thread = |project: Option<&String>, title: &str, overlaps: Option<&Vec<String>>| {
			let project = project.filter(|p| !p.is_empty()).map_or_else(String::new, |p| format!("{p}/"));
			let overlaps = overlaps.map_or_else(String::new, |o| format!(" (overlaps {})", o.join(", ")));
			format!("{project}{title}{overlaps}")
		};
		let awaiting =
			t.needs_you
				.iter()
				.map(|n| {
					let said = match (&n.kind, &n.project) {
						(NeedsYouKind::Reminder, _) => format!("{} {}", n.time.as_deref().unwrap_or(""), n.text),
						(_, Some(project)) if !project.is_empty() => format!("{project}/{}", n.text),
						_ => n.text.clone(),
					};
					let waited =
						n.since.as_deref().filter(|since| !since.is_empty()).map_or_else(String::new, |since| {
							format!(" (waiting {})", wait_label(now - parse_iso(since)))
						});
					let overlaps =
						n.overlaps.as_ref().map_or_else(String::new, |o| format!(" (overlaps {})", o.join(", ")));
					format!("{said}{waited}{overlaps}")
				})
				.collect();
		let mut lines = vec![
			format!("TODAY · {label}"),
			row("Calendar", t.calendar.iter().map(|c| format!("{} {}", c.time, c.title)).collect()),
			row("Awaiting you", awaiting),
			// A merged or PR'd thread says how it landed; one with a pane is done and awaits its review.
			row(
				"Landed",
				t.landed
					.iter()
					.map(|r| {
						let said = thread(r.project.as_ref(), &r.title, r.overlaps.as_ref());
						if r.pane_id.is_none() { format!("{said} ({})", r.line) } else { said }
					})
					.collect(),
			),
			row(
				"Underway",
				t.underway
					.iter()
					.map(|r| thread(r.project.as_ref(), &r.title, r.overlaps.as_ref()))
					.chain(t.queued.iter().cloned())
					.collect(),
			),
			row(
				"Next",
				t.next
					.iter()
					.map(|n| {
						let project = n
							.project
							.as_deref()
							.filter(|p| !p.is_empty())
							.map_or_else(String::new, |p| format!("{p}: "));
						format!("{project}{} (#{})", n.text, n.id)
					})
					.collect(),
			),
		];
		// The private sources reach the boss as counts only (0681): never a subject, summary or resource.
		lines.splice(2..2, private_lines(&t));
		Ok(lines.join("\n"))
	}

	// ---- inbox (0302) ----

	/// Files a note into a project's next work.
	pub fn note(&self, text: &str, project: &Project) -> Result<InboxItem> {
		let item =
			self.inbox(|inbox| inbox.add(text.trim(), InboxWhere { project_id: Some(&project.id), due_at: None }))?;
		self.push_today()?;
		Ok(item)
	}

	/// Files a reminder into `today` at a time; it is delivered as a normal event when due.
	pub fn remind(&self, text: &str, at: &str, project: Option<&Project>) -> Result<InboxItem> {
		let body = text.trim();
		if body.is_empty() {
			return Err(Error::other("a reminder needs its text"));
		}
		let due = parse_when(at, self.now_ms()).map_err(Error::other)?;
		let item = self.inbox(|inbox| {
			inbox.add(body, InboxWhere { project_id: project.map(|p| p.id.as_str()), due_at: Some(due) })
		})?;
		self.push_today()?;
		Ok(item)
	}

	pub fn done(&self, id: i64) -> Result<bool> {
		let done = self.inbox(|inbox| inbox.done(id))?;
		if done {
			self.queue(|queue| queue.resolve(&format!("inbox:{id}")))?;
		}
		self.push_today()?;
		Ok(done)
	}

	/// The worklog on request (0302): today's unless a day is given; every project's unless some are given. The day's
	/// sessions are read in steps first (CO-11); a day not on the calendar (2026-02-30) is refused (CO-22).
	pub async fn worklog(&self, day: Option<&str>, projects: Option<&[Project]>) -> Result<WorklogWritten> {
		let wanted =
			day.map(str::trim).filter(|day| !day.is_empty()).map_or_else(|| local_day(self.now_ms()), str::to_owned);
		let parts = Regex::new(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})$").ok().and_then(|re| {
			re.captures(&wanted).map(|c| {
				(c[1].parse::<i32>().unwrap_or(0), c[2].parse::<u32>().unwrap_or(0), c[3].parse::<u32>().unwrap_or(0))
			})
		});
		let Some((y, m, d)) = parts else {
			return Err(Error::other(format!("\"{}\" is not a day: use YYYY-MM-DD", day.unwrap_or_default())));
		};
		// `new Date(y, m - 1, d)` reads a year under 100 as 19xx, so that year is no such day either.
		if y < 100 || NaiveDate::from_ymd_opt(y, m, d).is_none() {
			return Err(Error::other(format!("no such day: {wanted}")));
		}
		self.inner.hooks.prepare_sessions(&wanted).await?;
		let by_project = self.inner.hooks.sessions_by_project(&wanted);
		let all;
		let projects = match projects {
			Some(projects) => projects,
			None => {
				all = self.inner.hooks.projects();
				&all
			}
		};
		write_worklog(
			projects,
			&wanted,
			|project, _| by_project.get(&project.id).cloned().unwrap_or_default(),
			Some(&self.inner.places),
			None,
		)
		.await
	}
}

/// The boss's lines for the private sources (0681): `mail: 3 unread`, `jira: 5 open`, `azure: 2 failed`, or the section's
/// failure line; none for a source that isn't configured.
fn private_lines(today: &Today) -> Vec<String> {
	let line = |section: &Option<PrivateSection>, what: &str, name: &str| {
		let section = section.as_ref()?;
		Some(match &section.error {
			Some(error) => error.clone(),
			None => format!("{name}: {} {what}", section.rows.len()),
		})
	};
	[line(&today.mail, "unread", "mail"), line(&today.jira, "open", "jira"), line(&today.azure, "failed", "azure")]
		.into_iter()
		.flatten()
		.collect()
}

fn landed_item(rail: RailItem) -> LandedItem {
	LandedItem {
		pane_id: Some(rail.pane_id),
		title: rail.title,
		project: rail.project,
		line: rail.line,
		overlaps: rail.overlaps,
		state: None,
		since: None,
		cost: None,
		context: None,
		pr: None,
	}
}
