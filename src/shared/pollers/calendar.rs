//! The calendar poller (0301, 0353): the meetings today and tomorrow in an iCal feed. `icalendar` parses the document and
//! `rrule` (through `icalendar`'s `recurrence` feature) walks the recurrence rules; the expansion around them (exceptions,
//! overrides, ongoing events, all-day events) follows node-ical's `expandRecurringEvent`, which the TypeScript used.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Duration as Span, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use icalendar::CalendarDateTime;
use icalendar::{Calendar, Component, DatePerhapsTime, Event, RRuleSet, Tz};
use regex::Regex;

use crate::shared::day::{day_start, local_day};
use crate::shared::errors::{Error, Result};
use crate::shared::link::web_link;

/// One meeting today or tomorrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
	pub uid: String,
	pub title: String,
	/// Epoch ms.
	pub start: i64,
	/// Epoch ms.
	pub end: i64,
	pub all_day: bool,
	/// Its link (0429): the entry's URL, a link in its location, or a Teams/Meet/Zoom link in its description.
	pub url: Option<String>,
}

/// The first http(s) link in a text, without the brackets invites wrap links in.
static ANY_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s<>"'`]+"#).expect("pattern"));
/// A Teams, Google Meet or Zoom join link, as invites put them in the description.
static MEETING_LINK: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r#"(?i)https://(?:teams\.microsoft\.com|teams\.live\.com|meet\.google\.com|(?:[\w-]+\.)?zoom\.us)/[^\s<>"'`]+"#,
	)
	.expect("pattern")
});

/// A meeting's link (0429, DF-1): `URL`, else a link in `LOCATION`, else a meeting link in `DESCRIPTION`; only a
/// well-formed http(s) link up to 300 characters, normalized (SE-4).
pub fn meeting_link(url: Option<&str>, location: Option<&str>, description: Option<&str>) -> Option<String> {
	if let Some(url) = web_link(Some(url.unwrap_or("").trim())) {
		return Some(url);
	}
	let found = ANY_LINK
		.find(location.unwrap_or(""))
		.or_else(|| MEETING_LINK.find(description.unwrap_or("")))
		.map(|found| found.as_str());
	// A link ending a sentence or inside parentheses: the trailing punctuation isn't part of it.
	web_link(found.map(|found| found.trim_end_matches([')', '.', ',', ';', ']'])))
}

fn event_link(event: &Event) -> Option<String> {
	meeting_link(event.property_value("URL"), event.property_value("LOCATION"), event.property_value("DESCRIPTION"))
}

/// Windows zone names (as Outlook writes `TZID`) and the IANA zones `rrule` knows.
const WINDOWS_ZONES: [(&str, &str); 44] = [
	("Dateline Standard Time", "Etc/GMT+12"),
	("UTC", "UTC"),
	("GMT Standard Time", "Europe/London"),
	("Greenwich Standard Time", "Atlantic/Reykjavik"),
	("W. Europe Standard Time", "Europe/Berlin"),
	("Central Europe Standard Time", "Europe/Budapest"),
	("Romance Standard Time", "Europe/Paris"),
	("Central European Standard Time", "Europe/Warsaw"),
	("E. Europe Standard Time", "Europe/Chisinau"),
	("FLE Standard Time", "Europe/Kiev"),
	("GTB Standard Time", "Europe/Bucharest"),
	("Russian Standard Time", "Europe/Moscow"),
	("Turkey Standard Time", "Europe/Istanbul"),
	("Israel Standard Time", "Asia/Jerusalem"),
	("Egypt Standard Time", "Africa/Cairo"),
	("South Africa Standard Time", "Africa/Johannesburg"),
	("Arab Standard Time", "Asia/Riyadh"),
	("Arabian Standard Time", "Asia/Dubai"),
	("India Standard Time", "Asia/Kolkata"),
	("China Standard Time", "Asia/Shanghai"),
	("Singapore Standard Time", "Asia/Singapore"),
	("Tokyo Standard Time", "Asia/Tokyo"),
	("Korea Standard Time", "Asia/Seoul"),
	("AUS Eastern Standard Time", "Australia/Sydney"),
	("E. Australia Standard Time", "Australia/Brisbane"),
	("New Zealand Standard Time", "Pacific/Auckland"),
	("Eastern Standard Time", "America/New_York"),
	("Central Standard Time", "America/Chicago"),
	("Mountain Standard Time", "America/Denver"),
	("US Mountain Standard Time", "America/Phoenix"),
	("Pacific Standard Time", "America/Los_Angeles"),
	("Alaskan Standard Time", "America/Anchorage"),
	("Hawaiian Standard Time", "Pacific/Honolulu"),
	("Canada Central Standard Time", "America/Regina"),
	("Atlantic Standard Time", "America/Halifax"),
	("E. South America Standard Time", "America/Sao_Paulo"),
	("Argentina Standard Time", "America/Buenos_Aires"),
	("Mexico Standard Time", "America/Mexico_City"),
	("SA Pacific Standard Time", "America/Bogota"),
	("Central America Standard Time", "America/Guatemala"),
	("Pakistan Standard Time", "Asia/Karachi"),
	("SE Asia Standard Time", "Asia/Bangkok"),
	("Taipei Standard Time", "Asia/Taipei"),
	("W. Australia Standard Time", "Australia/Perth"),
];

/// A zone `rrule` can compute in: its handle and the name `DTSTART;TZID=` takes.
struct Zone {
	tz: Tz,
	iana: String,
}

/// The zone a `TZID` names (an IANA name, or a Windows one), or `None` when it isn't one `rrule` knows.
fn zone(tzid: &str) -> Option<Zone> {
	let iana =
		WINDOWS_ZONES.iter().find(|(windows, _)| windows.eq_ignore_ascii_case(tzid)).map_or(tzid, |(_, iana)| iana);
	if iana.chars().any(char::is_control) {
		return None;
	}
	let set = RRuleSet::from_str(&format!("DTSTART;TZID={iana}:20000101T000000\nRRULE:FREQ=DAILY;COUNT=1")).ok()?;
	Some(Zone { tz: set.get_dt_start().timezone(), iana: iana.to_owned() })
}

/// The epoch ms of `naive` in `tz`; a time the clock skips moves on to the first one after it, a repeated one is its first.
fn resolve<T: TimeZone>(tz: &T, naive: NaiveDateTime) -> i64 {
	let mut at = naive;
	for _ in 0..(24 * 4) {
		if let Some(time) = tz.from_local_datetime(&at).earliest() {
			return time.timestamp_millis();
		}
		at += Span::minutes(15);
	}
	naive.and_utc().timestamp_millis()
}

fn local_midnight(day: NaiveDate) -> i64 {
	resolve(&Local, day.and_time(NaiveTime::MIN))
}

fn local_date(ms: i64) -> NaiveDate {
	DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&Local).date_naive()
}

/// A date or time from the feed: its instant, its day when it is a `DATE`, and the `DTSTART` line `rrule` reads it from.
struct Stamp {
	ms: i64,
	day: Option<NaiveDate>,
	line: String,
}

const STAMP: &str = "%Y%m%dT%H%M%S";

fn stamp(value: &DatePerhapsTime) -> Stamp {
	match value {
		DatePerhapsTime::Date(day) => Stamp {
			ms: local_midnight(*day),
			day: Some(*day),
			line: format!("DTSTART:{}", day.and_time(NaiveTime::MIN).format(STAMP)),
		},
		DatePerhapsTime::DateTime(CalendarDateTime::Utc(at)) => {
			Stamp { ms: at.timestamp_millis(), day: None, line: format!("DTSTART:{}Z", at.format(STAMP)) }
		}
		DatePerhapsTime::DateTime(CalendarDateTime::Floating(at)) => {
			Stamp { ms: resolve(&Local, *at), day: None, line: format!("DTSTART:{}", at.format(STAMP)) }
		}
		DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, tzid }) => match zone(tzid) {
			Some(zone) => Stamp {
				ms: resolve(&zone.tz, *date_time),
				day: None,
				line: format!("DTSTART;TZID={}:{}", zone.iana, date_time.format(STAMP)),
			},
			// A zone nobody here knows (a feed's own `VTIMEZONE`): its clock reading counts as local time.
			None => Stamp {
				ms: resolve(&Local, *date_time),
				day: None,
				line: format!("DTSTART:{}", date_time.format(STAMP)),
			},
		},
	}
}

/// An RFC 5545 `DURATION` in ms (`P1DT2H30M`, `PT45M`, `P1W`), or `None` when it isn't one.
fn duration_ms(text: &str) -> Option<i64> {
	let trimmed = text.trim();
	let (sign, rest) = match trimmed.strip_prefix('-') {
		Some(rest) => (-1, rest),
		None => (1, trimmed.strip_prefix('+').unwrap_or(trimmed)),
	};
	let mut rest = rest.strip_prefix(['P', 'p'])?;
	let (mut seen_time, mut any, mut total) = (false, false, 0i64);
	while !rest.is_empty() {
		if let Some(after) = rest.strip_prefix(['T', 't']) {
			seen_time = true;
			rest = after;
			continue;
		}
		let digits = rest.find(|c: char| !c.is_ascii_digit()).filter(|&at| (1..=10).contains(&at))?;
		let amount: i64 = rest[..digits].parse().ok()?;
		let unit = rest[digits..].chars().next()?.to_ascii_uppercase();
		let factor = match unit {
			'W' => 604_800_000,
			'D' => 86_400_000,
			'H' if seen_time => 3_600_000,
			'M' if seen_time => 60_000,
			'S' if seen_time => 1_000,
			_ => return None,
		};
		total += amount * factor;
		any = true;
		rest = &rest[digits + 1..];
	}
	any.then_some(sign * total)
}

/// A VEVENT's start and end; with no `DTEND`, its `DURATION` after the start, else a day for a `DATE` and nothing for a time.
struct Times {
	start: Stamp,
	end: Stamp,
}

impl Times {
	fn of(event: &Event) -> Option<Times> {
		let start = stamp(&event.get_start()?);
		let end = match event.get_end() {
			Some(end) => stamp(&end),
			None => {
				let span = event.property_value("DURATION").and_then(duration_ms);
				match (start.day, span) {
					(Some(day), None) => day_stamp(day + Span::days(1)),
					(Some(_), Some(span)) => day_stamp(local_date(start.ms + span)),
					(None, span) => Stamp { ms: start.ms + span.unwrap_or(0), day: None, line: String::new() },
				}
			}
		};
		Some(Times { start, end })
	}

	/// The days a full-day event spans, at least one.
	fn days(&self) -> i64 {
		let day = |stamp: &Stamp| stamp.day.unwrap_or_else(|| local_date(stamp.ms));
		(day(&self.end) - day(&self.start)).num_days().max(1)
	}
}

fn day_stamp(day: NaiveDate) -> Stamp {
	Stamp { ms: local_midnight(day), day: Some(day), line: String::new() }
}

/// One occurrence of a meeting: its times, its title, and the VEVENT whose status and links count.
struct Instance<'a> {
	start: i64,
	end: i64,
	all_day: bool,
	summary: &'a str,
	event: &'a Event,
}

/// The window the meetings must touch: epoch ms, both ends inclusive.
#[derive(Clone, Copy)]
struct Window {
	from: i64,
	to: i64,
}

impl Window {
	/// Whether the instance touches the window (ongoing ones included); an all-day one by its local days.
	fn holds(self, instance: &Instance) -> bool {
		if instance.all_day {
			let (from, to) = (local_date(self.from), local_date(self.to));
			local_date(instance.end) >= from && local_date(instance.start) <= to
		} else {
			instance.end >= self.from && instance.start <= self.to
		}
	}
}

/// The instance of `event` starting at `start`: a full-day one ends at local midnight after its days, a timed one at its
/// start plus the event's own duration.
fn instance_at<'a>(start: i64, all_day: bool, times: &Times, summary: &'a str, event: &'a Event) -> Instance<'a> {
	let end = if all_day {
		local_midnight(local_date(start) + Span::days(times.days()))
	} else {
		start + (times.end.ms - times.start.ms)
	};
	Instance { start, end, all_day, summary, event }
}

/// A recurrence override and the occurrence it replaces.
struct Override<'a> {
	event: &'a Event,
	times: Times,
	/// The `RECURRENCE-ID`: a day for a `DATE`, else an instant.
	key: (Option<NaiveDate>, i64),
	sequence: i64,
}

impl<'a> Override<'a> {
	fn of(event: &'a Event) -> Option<Override<'a>> {
		let id = stamp(&event.get_recurrence_id()?);
		let sequence = event.property_value("SEQUENCE").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
		Some(Override { event, times: Times::of(event)?, key: (id.day, id.ms), sequence })
	}

	/// Whether it replaces the occurrence at `ms`, which falls on `day` when the series is full-day.
	fn replaces(&self, ms: i64, day: Option<NaiveDate>) -> bool {
		match (day, self.key) {
			(Some(day), (Some(key), _)) => key == day,
			(Some(day), (None, at)) => local_date(at) == day,
			(None, (None, at)) => at == ms,
			(None, (Some(_), _)) => false,
		}
	}
}

/// The overrides of a series, a newer `SEQUENCE` replacing an older version of the same occurrence.
fn overrides_of<'a>(events: &[&'a Event]) -> Vec<Override<'a>> {
	let mut found: Vec<Override<'a>> = Vec::new();
	for event in events {
		let Some(next) = Override::of(event) else { continue };
		match found.iter().position(|o| o.key == next.key) {
			Some(at) if next.sequence >= found[at].sequence => found[at] = next,
			Some(_) => {}
			None => found.push(next),
		}
	}
	found
}

static UNTIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"UNTIL=(\d{8})(T\d{6})?(Z)?").expect("pattern"));

/// The `RRULE` text `rrule` reads with the series' `DTSTART`: an `UNTIL` of a full-day series ends with its day, one of a
/// zoned series without a `Z` is that zone's clock reading in UTC.
fn rule_text(rule: &str, all_day: bool, zone: Option<&Zone>) -> Option<String> {
	let rule = &rule[rule.rfind("FREQ=")?..];
	Some(
		UNTIL
			.replace(rule, |until: &regex::Captures| {
				let date = &until[1];
				if all_day {
					return format!("UNTIL={date}T235959");
				}
				match (until.get(2), until.get(3), zone) {
					(Some(time), None, Some(zone)) => {
						let naive = NaiveDateTime::parse_from_str(&format!("{date}{}", time.as_str()), STAMP).ok();
						let utc = naive.and_then(|n| DateTime::from_timestamp_millis(resolve(&zone.tz, n)));
						utc.map_or_else(|| until[0].to_owned(), |utc| format!("UNTIL={}Z", utc.format(STAMP)))
					}
					_ => until[0].to_owned(),
				}
			})
			.into_owned(),
	)
}

/// The recurring occurrences that touch the window, with their exceptions and overrides applied.
fn expand_series<'a>(
	master: &'a Event,
	times: &Times,
	rule: &str,
	overrides: &[Override<'a>],
	window: Window,
) -> Vec<Instance<'a>> {
	let all_day = times.start.day.is_some();
	let tzid = master.get_start().and_then(|start| match start {
		DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { tzid, .. }) => zone(&tzid),
		_ => None,
	});
	let Some(rule) = rule_text(rule, all_day, tzid.as_ref()) else { return plain(master, times, window) };
	let Ok(set) = RRuleSet::from_str(&format!("{}\nRRULE:{rule}", times.start.line)) else {
		return plain(master, times, window);
	};
	let tz = set.get_dt_start().timezone();
	let (from, to) = if all_day {
		(
			local_midnight(local_date(window.from) - Span::days(times.days())),
			local_midnight(local_date(window.to) + Span::days(1)) - 1,
		)
	} else {
		(window.from - (times.end.ms - times.start.ms), window.to)
	};
	let bounds = (tz.timestamp_millis_opt(from).single(), tz.timestamp_millis_opt(to).single());
	let (Some(after), Some(before)) = bounds else { return plain(master, times, window) };
	let dates = set.after(after).before(before).all(u16::MAX).dates;
	let exdates: Vec<Stamp> =
		master.multi_properties().get("EXDATE").into_iter().flatten().flat_map(exdates_of).collect();
	let summary = master.get_summary().unwrap_or("");
	let mut used: HashSet<usize> = HashSet::new();
	let mut instances = Vec::new();
	for date in dates {
		let (ms, day) = (date.timestamp_millis(), all_day.then(|| date.date_naive()));
		if excluded(&exdates, ms, day) {
			continue;
		}
		let replaced = overrides.iter().position(|o| o.replaces(ms, day));
		let instance = match replaced {
			Some(at) => {
				let o = &overrides[at];
				let summary = o.event.get_summary().filter(|s| !s.is_empty()).unwrap_or(summary);
				instance_at(o.times.start.ms, all_day, &o.times, summary, o.event)
			}
			None => instance_at(ms, all_day, times, summary, master),
		};
		if window.holds(&instance) {
			used.extend(replaced);
			instances.push(instance);
		}
	}
	for o in overrides.iter().enumerate().filter(|(at, _)| !used.contains(at)).map(|(_, o)| o) {
		let summary = o.event.get_summary().filter(|s| !s.is_empty()).unwrap_or(summary);
		let instance = instance_at(o.times.start.ms, all_day, &o.times, summary, o.event);
		if window.holds(&instance) {
			instances.push(instance);
		}
	}
	instances.sort_by_key(|instance| instance.start);
	instances
}

/// The dates an `EXDATE` line lists (it may hold several, comma-separated).
fn exdates_of(property: &icalendar::Property) -> Vec<Stamp> {
	let tzid = property.params().get("TZID").map(|tz| tz.value().to_owned());
	let date_only = property.value_type() == Some(icalendar::ValueType::Date);
	property
		.value()
		.split(',')
		.filter_map(|value| {
			let value = value.trim();
			if date_only || value.len() == 8 {
				return NaiveDate::parse_from_str(value, "%Y%m%d").ok().map(day_stamp);
			}
			if let Some(utc) = value.strip_suffix('Z') {
				let at = NaiveDateTime::parse_from_str(utc, STAMP).ok()?;
				return Some(stamp(&DatePerhapsTime::DateTime(CalendarDateTime::Utc(at.and_utc()))));
			}
			let at = NaiveDateTime::parse_from_str(value, STAMP).ok()?;
			Some(stamp(&DatePerhapsTime::DateTime(match &tzid {
				Some(tzid) => CalendarDateTime::WithTimezone { date_time: at, tzid: tzid.clone() },
				None => CalendarDateTime::Floating(at),
			})))
		})
		.collect()
}

/// Whether an `EXDATE` takes out the occurrence at `ms`: by day for a full-day series; for a timed one by instant, or a
/// `DATE` exception by the occurrence's UTC day.
fn excluded(exdates: &[Stamp], ms: i64, day: Option<NaiveDate>) -> bool {
	exdates.iter().any(|ex| match (day, ex.day) {
		(Some(day), Some(ex)) => ex == day,
		(Some(day), None) => local_date(ex.ms) == day,
		(None, Some(ex)) => DateTime::from_timestamp_millis(ms).is_some_and(|at| at.date_naive() == ex),
		(None, None) => ex.ms == ms,
	})
}

/// A one-off VEVENT (or a series' occurrence whose series isn't in the feed) as a meeting, if it touches the window.
fn plain<'a>(event: &'a Event, times: &Times, window: Window) -> Vec<Instance<'a>> {
	let all_day = times.start.day.is_some();
	let instance = instance_at(times.start.ms, all_day, times, event.get_summary().unwrap_or(""), event);
	if window.holds(&instance) { vec![instance] } else { Vec::new() }
}

/// A uid's VEVENTs: its series (the first one without `RECURRENCE-ID`), and the overrides.
#[derive(Default)]
struct Entry<'a> {
	master: Option<&'a Event>,
	overrides: Vec<&'a Event>,
}

/// The VEVENTs grouped by `UID`, in order of first appearance; each without a `UID` is its own.
fn entries(calendar: &Calendar) -> Vec<Entry<'_>> {
	let mut entries: Vec<Entry> = Vec::new();
	let mut index: HashMap<&str, usize> = HashMap::new();
	for event in calendar.events() {
		let is_override = event.property_value("RECURRENCE-ID").is_some();
		let at = match event.get_uid().filter(|uid| !uid.is_empty()) {
			Some(uid) => *index.entry(uid).or_insert_with(|| {
				entries.push(Entry::default());
				entries.len() - 1
			}),
			None => {
				entries.push(Entry::default());
				entries.len() - 1
			}
		};
		if is_override {
			entries[at].overrides.push(event);
		} else if entries[at].master.is_none() {
			entries[at].master = Some(event);
		}
	}
	entries
}

fn meeting(instance: &Instance, uid: &str) -> Option<Meeting> {
	if instance.event.property_value("STATUS").is_some_and(|status| status.to_uppercase() == "CANCELLED") {
		return None;
	}
	Some(Meeting {
		uid: uid.to_owned(),
		title: if instance.summary.is_empty() { "(no title)".to_owned() } else { instance.summary.to_owned() },
		start: instance.start,
		end: instance.end,
		all_day: instance.all_day,
		url: event_link(instance.event),
	})
}

/// The meetings today and tomorrow in an iCal document (0353): recurrences expanded with their exceptions and
/// overrides, cancelled ones left out, by start; each occurrence ends at its start plus the event's duration. An override
/// belongs to the series with its `UID`; one whose series isn't in the feed (an invitation to one meeting of a series,
/// SHR-4) counts as a meeting of its own. `now` is epoch ms.
pub fn meetings_in(parsed: &Calendar, now: i64) -> Vec<Meeting> {
	let today = local_day(now);
	let window = Window { from: day_start(&today, 0), to: day_start(&today, 2) - 1 };
	let mut meetings = Vec::new();
	for entry in entries(parsed) {
		let uid = |event: &Event| event.get_uid().unwrap_or("").to_owned();
		let Some(master) = entry.master else {
			for event in entry.overrides {
				let Some(times) = Times::of(event) else { continue };
				meetings.extend(plain(event, &times, window).iter().filter_map(|i| meeting(i, &uid(event))));
			}
			continue;
		};
		let Some(times) = Times::of(master) else { continue };
		let instances = match master.property_value("RRULE") {
			Some(rule) => expand_series(master, &times, rule, &overrides_of(&entry.overrides), window),
			None => plain(master, &times, window),
		};
		meetings.extend(instances.iter().filter_map(|i| meeting(i, &uid(master))));
	}
	meetings.sort_by_key(|meeting| meeting.start);
	meetings
}

/// What a fetch of the feed came back with.
pub struct Fetched {
	pub status: u16,
	pub body: String,
}

/// The feed over HTTP, ended after 30 s; `None` when it couldn't be fetched at all.
async fn fetch(url: String) -> Option<Fetched> {
	let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().ok()?;
	let response = client.get(url).send().await.ok()?;
	let status = response.status().as_u16();
	Some(Fetched { status, body: response.text().await.ok()? })
}

/// Reads the calendar from the secret URL (0301); a `webcal://` subscription link is fetched over https (D7). The URL
/// is only ever passed to the fetch; a failure's message never holds it. `now` is epoch ms.
pub async fn poll_calendar(url: &str, now: i64) -> Result<Vec<Meeting>> {
	poll_calendar_with(url, now, fetch).await
}

/// [`poll_calendar`] with its own fetch (the tests'): `fetch_url` gets the address and answers `None` when it couldn't
/// fetch it.
pub async fn poll_calendar_with<F, R>(url: &str, now: i64, fetch_url: F) -> Result<Vec<Meeting>>
where
	F: FnOnce(String) -> R,
	R: Future<Output = Option<Fetched>>,
{
	let address = match url.get(..9) {
		Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => format!("https://{}", &url[9..]),
		_ => url.to_owned(),
	};
	let fetched = match fetch_url(address).await {
		Some(fetched) if (200..300).contains(&fetched.status) => fetched,
		Some(fetched) => return Err(Error::other(format!("calendar: HTTP {}", fetched.status))),
		None => return Err(Error::other("calendar: it couldn't be fetched")),
	};
	let lines = node_ical_lines(&fetched.body).ok_or_else(|| Error::other("calendar: the feed isn't iCal"))?;
	// node-ical answers `{}` for text it can't read (an HTML page, say): no meetings.
	let Ok(parsed) = Calendar::from_str(&lines) else { return Ok(Vec::new()) };
	Ok(meetings_in(&parsed, now))
}

/// A content line as node-ical reads one: a name, parameters and a value.
static CONTENT_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#"^[A-Za-z0-9_-]+(?:;[A-Za-z0-9_-]+=(?:"[^"]*"|[^":;]+))*:.*$"#).expect("pattern"));

/// The feed as node-ical's `parseICS` reads it: lines unfolded, quotes dropped from a `TZID` line, and a line that
/// isn't `name[;param=value]:value` skipped. `None` where node-ical throws: a second `DTSTART`, `DTEND` or `DUE` in
/// one component.
fn node_ical_lines(body: &str) -> Option<String> {
	let raw: Vec<&str> = body.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
	let mut kept = String::new();
	let mut seen: Vec<HashSet<String>> = vec![HashSet::new()];
	let mut at = 0;
	while at < raw.len() {
		let mut line = raw[at].to_owned();
		while let Some(next) = raw.get(at + 1).filter(|next| next.starts_with([' ', '\t'])) {
			line.push_str(&next[1..]);
			at += 1;
		}
		at += 1;
		if line.contains("TZID=") && !line.contains("\"(") {
			line.retain(|c| c != '"');
		}
		if !CONTENT_LINE.is_match(&line) {
			continue;
		}
		let name = line.split([';', ':']).next().unwrap_or_default();
		match name {
			"BEGIN" => seen.push(HashSet::new()),
			"END" if seen.len() > 1 => {
				seen.pop();
			}
			"DTSTART" | "DTEND" | "DUE" if !seen.last_mut()?.insert(name.to_owned()) => return None,
			_ => {}
		}
		kept.push_str(&line);
		kept.push_str("\r\n");
	}
	Some(kept)
}

#[cfg(test)]
mod tests {
	use std::sync::Mutex;

	use tokio::io::{AsyncReadExt, AsyncWriteExt};
	use tokio::net::TcpListener;

	use super::*;

	/// A local iCal feed: a weekly standup with one day skipped and one moved, a cancelled one-off, one occurrence of a
	/// series whose master isn't in the feed, an all-day event.
	const FEED: &str = concat!(
		"BEGIN:VCALENDAR\r\n",
		"VERSION:2.0\r\n",
		"PRODID:-//test//EN\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:standup@test\r\n",
		"SUMMARY:standup (client-a)\r\n",
		"DTSTART:20261001T080000Z\r\n",
		"DTEND:20261001T081500Z\r\n",
		"RRULE:FREQ=DAILY\r\n",
		"EXDATE:20261009T080000Z\r\n",
		"END:VEVENT\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:standup@test\r\n",
		"RECURRENCE-ID:20261008T080000Z\r\n",
		"SUMMARY:standup (moved)\r\n",
		"DTSTART:20261008T090000Z\r\n",
		"DTEND:20261008T091500Z\r\n",
		"END:VEVENT\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:cancelled@test\r\n",
		"SUMMARY:dropped\r\n",
		"STATUS:CANCELLED\r\n",
		"DTSTART:20261008T120000Z\r\n",
		"DTEND:20261008T130000Z\r\n",
		"END:VEVENT\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:weekly-sync@client-a.example\r\n",
		"RECURRENCE-ID:20261008T100000Z\r\n",
		"SUMMARY:weekly sync (one occurrence)\r\n",
		"DTSTART:20261008T103000Z\r\n",
		"DTEND:20261008T110000Z\r\n",
		"END:VEVENT\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:allday@test\r\n",
		"SUMMARY:conference\r\n",
		"DTSTART;VALUE=DATE:20261008\r\n",
		"DTEND;VALUE=DATE:20261009\r\n",
		"END:VEVENT\r\n",
		"BEGIN:VEVENT\r\n",
		"UID:later@test\r\n",
		"SUMMARY:next week\r\n",
		"DTSTART:20261015T100000Z\r\n",
		"DTEND:20261015T110000Z\r\n",
		"END:VEVENT\r\n",
		"END:VCALENDAR\r\n",
	);

	fn ms(iso: &str) -> i64 {
		DateTime::parse_from_rfc3339(iso).unwrap().timestamp_millis()
	}

	fn iso(ms: i64) -> String {
		DateTime::from_timestamp_millis(ms).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
	}

	/// A local HTTP server answering the feed at `/private-abc123/basic.ics` and 404 elsewhere; returns its port.
	async fn serve(feed: &'static str) -> u16 {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		tokio::spawn(async move {
			loop {
				let Ok((mut socket, _)) = listener.accept().await else { return };
				tokio::spawn(async move {
					let mut request = Vec::new();
					let mut chunk = [0u8; 1024];
					while !request.windows(4).any(|w| w == b"\r\n\r\n") {
						match socket.read(&mut chunk).await {
							Ok(0) | Err(_) => return,
							Ok(n) => request.extend_from_slice(&chunk[..n]),
						}
					}
					let text = String::from_utf8_lossy(&request);
					let path = text.split_whitespace().nth(1).unwrap_or("").to_owned();
					let (status, body) =
						if path == "/private-abc123/basic.ics" { ("200 OK", feed) } else { ("404 Not Found", "no") };
					let answer = format!(
						"HTTP/1.1 {status}\r\ncontent-type: text/calendar\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
						body.len()
					);
					let _ = socket.write_all(answer.as_bytes()).await;
					let _ = socket.shutdown().await;
				});
			}
		});
		port
	}

	#[tokio::test]
	async fn calendar_secret_ical_url_is_read_over_http_today_and_tomorrow_expanded_with_exceptions_failures_never_show_the_url()
	 {
		let port = serve(FEED).await;
		let url = format!("http://127.0.0.1:{port}/private-abc123/basic.ics");
		let meetings = poll_calendar(&url, ms("2026-10-08T06:00:00Z")).await.unwrap();
		let seen: Vec<String> = meetings
			.iter()
			.map(|m| format!("{} {}{}", iso(m.start), m.title, if m.all_day { " (all day)" } else { "" }))
			.collect();
		assert!(seen.contains(&"2026-10-08T09:00:00.000Z standup (moved)".to_owned()));
		assert!(!seen.iter().any(|s| s.starts_with("2026-10-08T08:00")));
		assert!(!seen.iter().any(|s| s.contains("2026-10-09T08:00")));
		assert!(!seen.iter().any(|s| s.contains("dropped") || s.contains("next week")));
		assert!(seen.iter().any(|s| s.ends_with("conference (all day)")));
		// One occurrence of a series whose master isn't in the feed shows as a meeting of its own (SHR-4).
		assert!(seen.contains(&"2026-10-08T10:30:00.000Z weekly sync (one occurrence)".to_owned()));
		// Each meeting carries its end, for today's calendar (C9): an override's own, a lone occurrence's own.
		let ends = |list: &[Meeting]| -> Vec<String> {
			list.iter().filter(|m| !m.all_day).map(|m| format!("{} {}-{}", m.title, iso(m.start), iso(m.end))).collect()
		};
		assert_eq!(
			ends(&meetings),
			[
				"standup (moved) 2026-10-08T09:00:00.000Z-2026-10-08T09:15:00.000Z",
				"weekly sync (one occurrence) 2026-10-08T10:30:00.000Z-2026-10-08T11:00:00.000Z",
			]
		);
		// A series' occurrence ends at its start plus the event's duration.
		assert_eq!(
			ends(&poll_calendar(&url, ms("2026-10-02T06:00:00Z")).await.unwrap()),
			[
				"standup (client-a) 2026-10-02T08:00:00.000Z-2026-10-02T08:15:00.000Z",
				"standup (client-a) 2026-10-03T08:00:00.000Z-2026-10-03T08:15:00.000Z",
			]
		);

		let missing = format!("http://127.0.0.1:{port}/private-secret-xyz/none.ics");
		let error = poll_calendar(&missing, 0).await.unwrap_err();
		assert_eq!(error.to_string(), "calendar: HTTP 404");
		let unreachable = poll_calendar("http://127.0.0.1:1/private-secret-xyz", 0).await.unwrap_err();
		assert_eq!(unreachable.to_string(), "calendar: it couldn't be fetched");
		assert!(!unreachable.to_string().contains("private-secret-xyz"));
	}

	#[tokio::test]
	async fn calendar_webcal_subscription_link_is_fetched_over_https() {
		let fetched = Mutex::new(Vec::new());
		poll_calendar_with("webcal://calendar.example/x.ics", ms("2026-10-08T06:00:00Z"), |address| {
			fetched.lock().unwrap().push(address);
			async { Some(Fetched { status: 200, body: FEED.to_owned() }) }
		})
		.await
		.unwrap();
		assert_eq!(*fetched.lock().unwrap(), ["https://calendar.example/x.ics"]);
	}

	#[tokio::test]
	async fn calendar_a_feed_that_isnt_ical_is_an_empty_list_as_node_ical_reads_it() {
		for body in ["<html>", "", "<html>\n<body>no calendar here</body>\n</html>"] {
			let meetings = poll_calendar_with("https://x.example/a.ics", 0, |_| async {
				Some(Fetched { status: 200, body: body.into() })
			})
			.await
			.unwrap();
			assert_eq!(meetings, vec![], "{body:?}");
		}
	}

	#[tokio::test]
	async fn calendar_a_second_dtstart_in_one_event_is_the_feed_isnt_ical() {
		let body = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261101T100000Z\r\nDTSTART:20261101T110000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
		let error = poll_calendar_with("https://x.example/a.ics", 0, |_| async {
			Some(Fetched { status: 200, body: body.into() })
		})
		.await
		.unwrap_err();
		assert_eq!(error.to_string(), "calendar: the feed isn't iCal");
	}

	#[test]
	fn node_ical_skips_a_line_that_is_not_a_content_line() {
		let kept = node_ical_lines("junk\r\nBEGIN:VCALENDAR\r\n<b>\r\nSUMMARY:a\r\n b\r\nEND:VCALENDAR").unwrap();
		assert_eq!(kept, "BEGIN:VCALENDAR\r\nSUMMARY:ab\r\nEND:VCALENDAR\r\n");
	}

	#[test]
	fn links_from_a_calendar_are_kept_only_as_http_links_up_to_300_characters() {
		assert_eq!(meeting_link(Some("javascript:alert(1)"), None, None), None);
		assert_eq!(
			meeting_link(Some("HTTPS://Meet.Example/abc"), None, None).as_deref(),
			Some("https://meet.example/abc")
		);
		assert_eq!(meeting_link(None, Some(&format!("https://meet.example/{}", "x".repeat(300))), None), None);
	}

	#[test]
	fn calendar_meeting_links_url_else_location_else_teams_meet_zoom_in_the_description() {
		assert_eq!(
			meeting_link(Some("https://example.test/m/1"), None, None).as_deref(),
			Some("https://example.test/m/1")
		);
		assert_eq!(
			meeting_link(None, Some("Room 4 (https://zoom.example.test/j/9)"), None).as_deref(),
			Some("https://zoom.example.test/j/9")
		);
		assert_eq!(
			meeting_link(
				None,
				Some("Microsoft Teams Meeting"),
				Some(
					"Join: <https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0?context=%7b%7d>\nhttps://example.test/other"
				),
			)
			.as_deref(),
			Some("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0?context=%7b%7d")
		);
		assert_eq!(
			meeting_link(None, None, Some("Join with Google Meet: https://meet.google.com/abc-defg-hij.")).as_deref(),
			Some("https://meet.google.com/abc-defg-hij")
		);
		assert_eq!(
			meeting_link(None, None, Some("Zoom: https://acme.zoom.us/j/123?pwd=x")).as_deref(),
			Some("https://acme.zoom.us/j/123?pwd=x")
		);
		assert_eq!(meeting_link(None, Some("Room 4"), Some("see https://example.test/agenda")), None);

		let feed = concat!(
			"BEGIN:VCALENDAR\r\n",
			"VERSION:2.0\r\n",
			"BEGIN:VEVENT\r\n",
			"UID:review@test\r\n",
			"SUMMARY:Design review\r\n",
			"DTSTART:20261008T120000Z\r\n",
			"DTEND:20261008T130000Z\r\n",
			"DESCRIPTION:Join with Google Meet: https://meet.google.com/abc-defg-hij\r\n",
			"END:VEVENT\r\n",
			"END:VCALENDAR\r\n",
		);
		let parsed = Calendar::from_str(feed).unwrap();
		let meetings = meetings_in(&parsed, ms("2026-10-08T06:00:00Z"));
		assert_eq!(meetings[0].url.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
	}

	#[test]
	fn a_series_in_a_windows_zone_expands_in_that_zone_and_a_date_exception_takes_out_its_day() {
		let feed = concat!(
			"BEGIN:VCALENDAR\r\n",
			"VERSION:2.0\r\n",
			"BEGIN:VEVENT\r\n",
			"UID:zoned@test\r\n",
			"SUMMARY:sync\\, weekly\r\n",
			"DTSTART;TZID=W. Europe Standard Time:20260101T100000\r\n",
			"DTEND;TZID=W. Europe Standard Time:20260101T103000\r\n",
			"RRULE:FREQ=DAILY;UNTIL=20261231T100000\r\n",
			"EXDATE;VALUE=DATE:20261008\r\n",
			"END:VEVENT\r\n",
			"END:VCALENDAR\r\n",
		);
		let parsed = Calendar::from_str(feed).unwrap();
		let on = |day: &str| {
			meetings_in(&parsed, ms(&format!("{day}T12:00:00Z")))
				.iter()
				.map(|m| format!("{} {}", iso(m.start), m.title))
				.collect::<Vec<_>>()
		};
		// October 7 and 9 (the 8th is excepted); Berlin is UTC+2 in October.
		assert_eq!(on("2026-10-07").first().map(String::as_str), Some("2026-10-07T08:00:00.000Z sync, weekly"));
		assert!(on("2026-10-07").iter().all(|m| !m.starts_with("2026-10-08")));
		assert!(on("2026-10-09").iter().any(|m| m.starts_with("2026-10-09T08:00")));
	}

	#[test]
	fn a_full_day_series_runs_to_its_until_day_and_not_past_it() {
		let feed = concat!(
			"BEGIN:VCALENDAR\r\n",
			"VERSION:2.0\r\n",
			"BEGIN:VEVENT\r\n",
			"UID:holiday@test\r\n",
			"SUMMARY:day off\r\n",
			"DTSTART;VALUE=DATE:20261005\r\n",
			"DTEND;VALUE=DATE:20261006\r\n",
			"RRULE:FREQ=WEEKLY;UNTIL=20261012\r\n",
			"END:VEVENT\r\n",
			"END:VCALENDAR\r\n",
		);
		let parsed = Calendar::from_str(feed).unwrap();
		let titles = |now: &str| -> Vec<(String, bool)> {
			meetings_in(&parsed, ms(now)).into_iter().map(|m| (m.title, m.all_day)).collect()
		};
		assert_eq!(titles("2026-10-12T12:00:00Z"), [("day off".to_owned(), true)]);
		assert!(titles("2026-10-19T12:00:00Z").is_empty());
	}
}
