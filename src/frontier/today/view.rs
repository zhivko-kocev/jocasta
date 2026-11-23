//! What the `today` pane draws (0551, 0555, 0581, 0634, 0667–0673), as plain coloured lines from the service's feed: a
//! header (date, time, quiet hours, a broken service, weather, CPU and memory, quota, today's spend with its 7-day
//! average and costliest thread, the check queue), NEEDS YOU, UNDERWAY (thread, state, check, diff, time, cost,
//! context), LANDED TODAY (with its PR), CALENDAR, QUOTA (every account), LEFTOVERS, then NEXT (scheduled threads and his
//! notes) and the NEWS box while rows remain, his last key's outcome and a key line. No greeting, no Presence. Tokyo
//! Night from the one palette file (0474). Pure: `main.rs` draws the lines through the shared shell (`../tui.rs`).

use chrono::DateTime;

use std::ops::Range;

use crate::frontier::protocol::{
	Dashboard, Hud, NeedsYouKind, PrCi, QuotaAccount, QuotaCapacity, QuotaSpan, QuotaWindow, ServiceState, ThreadState,
	Today, quota_high, quota_percent, wait_label,
};
use crate::frontier::tui::{Line, Segment, TOKYO_NIGHT, columns, cut, fit, line_columns, wrap};
use crate::shared::day::{DayRef, day_label, hhmm, weekday};

/// What his last key did, or why not, and when (epoch ms): it shows for [`SAID_MS`].
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
	pub text: String,
	pub is_error: bool,
	pub at: i64,
}

/// What the pane knows: the feed's last figures, whether it is connected, the picked row and his last key's outcome.
#[derive(Debug, Clone, Default)]
pub struct TodayState {
	pub up: bool,
	/// Whether the feed has connected since the pane opened; until then it says it is connecting (UX-17).
	pub seen: bool,
	/// When the feed went down after being up (epoch ms).
	pub down_since: Option<i64>,
	pub hud: Option<Hud>,
	pub today: Option<Today>,
	pub dashboard: Option<Dashboard>,
	/// The picked row's key: a thread's pane id, or a link row's URL (0581).
	pub selected: Option<String>,
	pub said: Option<Said>,
	/// The first NEWS line shown (0668): the wheel and up/down move it.
	pub news_top: usize,
}

/// A row Enter acts on (0581): a thread's pane (Enter goes to it) or a link (Enter opens it); a leftover (0670) is
/// picked for `x`, a news row (0668) for the box's scrolling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
	pub key: String,
	pub pane_id: Option<String>,
	pub url: Option<String>,
	/// A NEWS row.
	pub news: bool,
	/// A leftover's folder, as the service listed it.
	pub leftover: Option<String>,
}

/// The NEWS box as drawn (0668), for the wheel and for up/down at its edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsBox {
	/// The lines of `Frame::lines` its items take.
	pub lines: Range<usize>,
	/// The index of its first item among all of them.
	pub top: usize,
	/// Every item's link, newest first (an item without one has an empty string).
	pub keys: Vec<String>,
}

/// What [`render`] draws: the lines, and the pickable rows in their drawn order (up/down move between them).
#[derive(Debug, Clone)]
pub struct Frame {
	pub lines: Vec<Line>,
	pub picks: Vec<Pick>,
	/// Where each pickable row sits: its line in `lines` and its pick's key (0660: a click picks the row under it).
	pub rows: Vec<(usize, String)>,
	/// The news box, when it is drawn.
	pub news: Option<NewsBox>,
}

const P: crate::frontier::tui::Palette = TOKYO_NIGHT;

/// The NEWS box's items (0668).
pub const NEWS_LINES: usize = 5;

/// His last key's outcome shows this long (0581, UX-5).
pub const SAID_MS: i64 = 60_000;

/// The pane's keys, most important first: the key line drops the last ones that don't fit (0581, UX-7, UX-16). `x`
/// (0670) drops first.
const KEYS: [&str; 7] = ["↑↓ pick", "Enter go/open", "f Finish", "s Stop", "r Review", "n New", "x Remove"];
/// The keys joined as the key line shows them when they all fit.
pub const KEY_LINE: &str = "↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New · x Remove";

/// The title of a row keeps at least this many columns before its facts give way (0581, UX-4).
const MIN_TITLE: i64 = 18;

/// JavaScript's `Math.round`: halves go up.
fn js_round(x: f64) -> f64 {
	(x + 0.5).floor()
}

/// JavaScript's `toFixed` for a non-negative number: the exact decimal value rounded to `digits` places, a tie up.
fn to_fixed(x: f64, digits: usize) -> String {
	let exact = format!("{x:.80}");
	let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, ""));
	let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
	if fraction.as_bytes().get(digits).is_some_and(|d| *d >= b'5') {
		let mut at = kept.len();
		loop {
			if at == 0 {
				kept.insert(0, b'1');
				break;
			}
			at -= 1;
			if kept[at] == b'9' {
				kept[at] = b'0';
			} else {
				kept[at] += 1;
				break;
			}
		}
	}
	let split = kept.len() - digits;
	let text = String::from_utf8_lossy(&kept).into_owned();
	if digits == 0 { text } else { format!("{}.{}", &text[..split], &text[split..]) }
}

/// Text parts joined with " · ": the parts in quiet text, the separators in comment (0581, UX-12).
fn hint(parts: &[&str]) -> Line {
	parts
		.iter()
		.enumerate()
		.flat_map(|(i, text)| {
			let separator = (i > 0).then(|| Segment::new(" · ", P.comment));
			separator.into_iter().chain([Segment::new(*text, P.fg3)])
		})
		.collect()
}

/// The key line for `width`: lower keys dropped while it doesn't fit.
fn key_line(width: usize) -> Line {
	let mut count = KEYS.len();
	while count > 1 && columns(&KEYS[..count].join(" · ")) > width {
		count -= 1;
	}
	fit(&hint(&KEYS[..count]), width)
}

/// `left` and the facts on one line, the facts flush to the edge. While the left part would get fewer than
/// `MIN_TITLE` columns (or its own width, when shorter), the last fact drops; the left part is cut after that. `tail`
/// (a link's ↗) follows the left part and is never cut.
fn spread(left: &[Segment], facts: &[Segment], width: usize, tail: Option<Segment>) -> Line {
	let tails: Line = tail.into_iter().collect();
	let tail_columns = line_columns(&tails) as i64;
	let width_i = width as i64;
	let want = (line_columns(left) as i64 + tail_columns).min(MIN_TITLE);
	let mut kept: Vec<Segment> = facts.iter().filter(|s| !s.text.is_empty()).cloned().collect();
	while !kept.is_empty() && width_i - line_columns(&kept) as i64 - 1 < want {
		kept.pop();
	}
	let room = if kept.is_empty() { width_i } else { width_i - line_columns(&kept) as i64 - 1 };
	let mut shown = fit(left, (room - tail_columns).max(0) as usize);
	shown.extend(tails);
	if kept.is_empty() {
		return fit(&shown, width);
	}
	let pad = (width_i - line_columns(&shown) as i64 - line_columns(&kept) as i64).max(1) as usize;
	shown.push(Segment::plain(" ".repeat(pad)));
	shown.extend(kept);
	shown
}

/// Parts joined with " · ", flowed over as many lines as `width` needs.
fn flow(parts: &[&Line], width: usize) -> Vec<Line> {
	let mut lines: Vec<Line> = Vec::new();
	let mut line: Line = Vec::new();
	let mut used = 0;
	for part in parts {
		let size = line_columns(part);
		if used > 0 && used + 3 + size > width {
			lines.push(std::mem::take(&mut line));
			used = 0;
		}
		if used > 0 {
			line.push(Segment::new(" · ", P.comment));
			used += 3;
		}
		line.extend(part.iter().cloned());
		used += size;
	}
	if !line.is_empty() {
		lines.push(line);
	}
	lines.iter().map(|l| fit(l, width)).collect()
}

/// `$3.20`.
fn money(usd: f64) -> String {
	format!("${}", to_fixed(usd, 2))
}

/// Context tokens, short: `83k`, `1.2M`.
pub fn tokens(n: u64) -> String {
	if n >= 1_000_000 {
		return format!("{}M", to_fixed(n as f64 / 1_000_000.0, 1));
	}
	if n >= 1_000 {
		return format!("{}k", js_round(n as f64 / 1_000.0));
	}
	n.to_string()
}

/// An ISO time as epoch ms, when it is one.
fn parse_time(iso: &str) -> Option<i64> {
	DateTime::parse_from_rfc3339(iso).ok().map(|t| t.timestamp_millis())
}

/// A header part and the header level it gives way at (the weather 1, CPU and memory 2).
struct Part {
	line: Line,
	yields: Option<u8>,
}

fn part(text: String, fg: ratatui::style::Color) -> Part {
	Part { line: vec![Segment::new(text, fg)], yields: None }
}

/// The header (0555, 0581): date and time, `quiet` during quiet hours, each broken service in red with its note, the
/// weather, CPU and memory, the quota windows (a high one with its reset when that is within a day) and today's spend.
/// The weather, then CPU and memory, give way when the header would pass two rows.
fn header(state: &TodayState, now: i64, width: usize) -> Vec<Line> {
	let date = day_label(DayRef::Time(now), false);
	let mut parts = vec![Part { line: vec![Segment::new(format!("{date} {}", hhmm(now)), P.fg).bold()], yields: None }];
	if state.hud.as_ref().and_then(|h| h.quiet) == Some(true) {
		parts.push(part("quiet".to_string(), P.fg3));
	}
	let dashboard = state.dashboard.as_ref();
	for service in dashboard.iter().flat_map(|d| d.services.iter()) {
		if service.state != ServiceState::Down {
			continue;
		}
		let name = format!("{:?}", service.name).to_lowercase();
		let note = service.note.as_deref().filter(|n| !n.is_empty()).map(|n| format!(" ({n})")).unwrap_or_default();
		parts.push(part(format!("{name} down{note}"), P.red));
	}
	if let Some(weather) = dashboard.and_then(|d| d.weather.as_ref()) {
		let text = format!("{}° {}", js_round(weather.temp_c), weather.summary);
		parts.push(Part { yields: Some(1), ..part(text, P.cyan) });
	}
	if let Some(machine) = dashboard.and_then(|d| d.machine.as_ref()) {
		let mem = if machine.ram.total > 0 {
			js_round(machine.ram.used as f64 / machine.ram.total as f64 * 100.0)
		} else {
			0.0
		};
		let text = format!("cpu {}% · mem {mem}%", js_round(machine.cpu * 100.0));
		parts.push(Part { yields: Some(2), ..part(text, P.fg2) });
	}
	for q in state.hud.iter().flat_map(|h| h.quota.iter()) {
		let high = quota_high(q);
		let resets = q.resets_at.as_deref().and_then(parse_time).map(|at| at - now);
		let reset = match (high, resets, q.resets_at.as_deref().and_then(parse_time)) {
			(true, Some(left), Some(at)) if left > 0 && left <= 86_400_000 => format!(" ({})", hhmm(at)),
			_ => String::new(),
		};
		let window = match q.window {
			QuotaSpan::FiveHours => "5h",
			QuotaSpan::SevenDays => "7d",
		};
		parts.push(part(format!("{window} {}%{reset}", quota_percent(q)), if high { P.red } else { P.fg2 }));
	}
	let hud = state.hud.as_ref();
	let spend = hud.and_then(|h| h.spend_today);
	parts.push(part(spend.map_or_else(|| "… today".to_string(), |s| format!("{} today", money(s))), P.green));
	if let Some(avg) = hud.and_then(|h| h.spend_avg_7d) {
		parts.push(Part { yields: Some(2), ..part(format!("avg {}", money(avg)), P.fg2) });
	}
	if let Some(top) = hud.and_then(|h| h.top_thread.as_ref()) {
		let text = format!("top {} {}", cut(&top.title, 20), money(top.cost));
		parts.push(Part { yields: Some(1), ..part(text, P.fg2) });
	}
	if let Some(load) = hud.and_then(|h| h.checks.as_ref()) {
		parts.push(Part {
			yields: Some(2),
			..part(format!("checks: {} running · {} waiting", load.running, load.waiting), P.yellow)
		});
	}
	let all: Vec<&Line> = parts.iter().map(|p| &p.line).collect();
	let mut lines = flow(&all, width);
	for level in [1u8, 2] {
		if lines.len() <= 2 {
			break;
		}
		let kept: Vec<&Line> = parts.iter().filter(|p| p.yields.is_none_or(|y| y > level)).map(|p| &p.line).collect();
		lines = flow(&kept, width);
	}
	lines
}

/// The one text for a service that has stopped answering (0581 UX-17, 0662): in the header, and in the HUD's status.
pub fn not_answering(since: Option<i64>) -> String {
	let since = since.map(|at| format!(" (since {})", hhmm(at))).unwrap_or_default();
	format!("Jocasta isn't answering{since}")
}

/// The header's place while the feed isn't up (UX-17): connecting at first, then the time and since when it is down.
fn offline(state: &TodayState, now: i64, width: usize) -> Line {
	if !state.seen {
		return fit(&[Segment::new("Connecting to Jocasta…", P.fg3)], width);
	}
	let since = state.down_since;
	fit(
		&[
			Segment::new(hhmm(now), P.fg).bold(),
			Segment::new(" · ", P.comment),
			Segment::new(format!("{}; trying again…", not_answering(since)), P.yellow),
		],
		width,
	)
}

fn heading(text: &str, width: usize) -> Line {
	fit(&[Segment::new(text, P.blue).bold()], width)
}

/// An empty section's line, in quiet text.
fn quiet(parts: &[&str], width: usize) -> Line {
	let mut line = vec![Segment::plain("  ")];
	line.extend(hint(parts));
	fit(&line, width)
}

/// A body row; `pick` when Enter acts on it.
struct Row {
	line: Line,
	pick: Option<Pick>,
}

impl Row {
	fn plain(line: Line) -> Self {
		Self { line, pick: None }
	}
}

/// A pickable row: the picked one carries the marker and the selection colour across the pane.
fn row(line: &[Segment], pick: Option<Pick>, state: &TodayState, width: usize) -> Row {
	let picked = pick.as_ref().is_some_and(|p| Some(&p.key) == state.selected.as_ref());
	let mut with_marker = vec![Segment::new(if picked { "› " } else { "  " }, P.blue)];
	with_marker.extend(line.iter().cloned());
	let mut line = fit(&with_marker, width);
	if picked {
		let fill = width.saturating_sub(line_columns(&line));
		line.push(Segment::plain(" ".repeat(fill)));
		for segment in &mut line {
			segment.bg = Some(P.bg2);
		}
	}
	Row { line, pick }
}

/// What a row with a link shows after its text (0581, UX-7): Enter opens it.
fn link() -> Segment {
	Segment::new(" ↗", P.blue)
}

/// A NEEDS YOU row's kind (0581, UX-4): `?` a question or plan, `✓` reviewed, `↗` a review request, `!` red.
fn glyph(kind: NeedsYouKind) -> &'static str {
	match kind {
		NeedsYouKind::Blocked => "?",
		NeedsYouKind::Reviewed => "✓",
		NeedsYouKind::Review => "↗",
		NeedsYouKind::Pipeline | NeedsYouKind::Disk | NeedsYouKind::Stuck => "!",
		NeedsYouKind::Reminder => "",
	}
}

fn pick_of(pane_id: Option<&str>, url: Option<&str>) -> Option<Pick> {
	if let Some(pane_id) = pane_id.filter(|p| !p.is_empty()) {
		return Some(Pick {
			key: pane_id.to_string(),
			pane_id: Some(pane_id.to_string()),
			url: None,
			news: false,
			leftover: None,
		});
	}
	url.filter(|u| !u.is_empty()).map(|url| Pick {
		key: url.to_string(),
		pane_id: None,
		url: Some(url.to_string()),
		news: false,
		leftover: None,
	})
}

/// NEEDS YOU: one row per item, coloured by kind, with its wait and the mark of a link.
fn needs_you_rows(state: &TodayState, today: &Today, now: i64, width: usize) -> Vec<Row> {
	let mut out = vec![Row::plain(heading("NEEDS YOU", width))];
	if today.needs_you.is_empty() {
		out.push(Row::plain(quiet(&["nothing waits on you"], width)));
	}
	for n in &today.needs_you {
		let colour = match n.kind {
			NeedsYouKind::Blocked => P.red,
			NeedsYouKind::Reviewed => P.orange,
			_ => P.yellow,
		};
		// A reminder keeps its time where the others have their kind's glyph.
		let lead = n.time.as_deref().unwrap_or_else(|| glyph(n.kind));
		let lead = if lead.is_empty() { String::new() } else { format!("{lead} ") };
		let project = match (&n.project, &n.pane_id) {
			(Some(project), Some(pane)) if !project.is_empty() && !pane.is_empty() => format!("{project}/"),
			_ => String::new(),
		};
		let text = format!("{lead}{project}{}", n.text);
		let wait: Vec<Segment> = n
			.since
			.as_deref()
			.filter(|s| !s.is_empty())
			.and_then(parse_time)
			.map(|at| Segment::new(wait_label(now - at), P.fg3))
			.into_iter()
			.collect();
		let pick = pick_of(n.pane_id.as_deref(), n.url.as_deref());
		// A review request's glyph already says it opens; any other row with only a link gets the mark.
		let tail = pick.as_ref().is_some_and(|p| p.url.is_some()).then(link).filter(|_| n.kind != NeedsYouKind::Review);
		out.push(row(&spread(&[Segment::new(text, colour)], &wait, width.saturating_sub(2), tail), pick, state, width));
	}
	out
}

/// A thread's state as the pane says it.
fn state_label(state: ThreadState) -> &'static str {
	match state {
		ThreadState::Working => "working",
		ThreadState::Blocked => "blocked",
		ThreadState::Done => "done",
		ThreadState::Idle => "idle",
	}
}

/// UNDERWAY: one row per thread at work, with its state, check, diff, age, cost and context.
fn underway_rows(state: &TodayState, today: &Today, now: i64, width: usize) -> Vec<Row> {
	let mut out = vec![Row::plain(heading("UNDERWAY", width))];
	if today.underway.is_empty() {
		out.push(Row::plain(quiet(&["no thread at work", "n starts one"], width)));
	}
	for u in &today.underway {
		// Kept in this order as the row narrows, the last dropping first: the state last of all (0581, UX-4, 0668).
		let (label, colour) = if u.stuck_for.is_some() {
			("stuck", P.red)
		} else {
			let colour = if u.state == Some(ThreadState::Idle) { P.fg3 } else { P.green };
			(state_label(u.state.unwrap_or(ThreadState::Working)), colour)
		};
		let mut facts = vec![Segment::new(label, colour)];
		if let Some(passed) = u.check {
			facts.push(Segment::new(if passed { "  ✓" } else { "  ✗" }, if passed { P.green } else { P.red }));
		}
		if let Some(diff) = &u.diff {
			facts.push(Segment::new(format!("  +{}", diff.added), P.green));
			facts.push(Segment::new(format!(" −{}", diff.removed), P.red));
		}
		facts.push(Segment::new(
			u.since.filter(|s| *s != 0).map(|s| format!("  {}", wait_label(now - s))).unwrap_or_default(),
			P.fg2,
		));
		facts.push(Segment::new(u.cost.map(|c| format!("  {}", money(c))).unwrap_or_default(), P.fg2));
		facts.push(Segment::new(u.context.map(|c| format!("  {}", tokens(c))).unwrap_or_default(), P.purple));
		let project = u.project.as_deref().filter(|p| !p.is_empty()).map(|p| format!("{p}/")).unwrap_or_default();
		let name = format!("{project}{}", u.title);
		let left = [Segment::new(name, P.fg)];
		out.push(row(
			&spread(&left, &facts, width.saturating_sub(2), None),
			pick_of(Some(&u.pane_id), None),
			state,
			width,
		));
	}
	out
}

/// LANDED TODAY: its rows, none at all when nothing landed; a thread sent as a PR shows its number and CI state.
fn landed_rows(state: &TodayState, today: &Today, width: usize) -> Vec<Row> {
	if today.landed.is_empty() {
		return Vec::new();
	}
	let mut out = vec![Row::plain(heading("LANDED TODAY", width))];
	for l in &today.landed {
		let has_pane = l.pane_id.as_deref().is_some_and(|p| !p.is_empty());
		let project = l.project.as_deref().filter(|p| !p.is_empty()).map(|p| format!("{p}/")).unwrap_or_default();
		let line = if has_pane { String::new() } else { format!(" ({})", l.line) };
		let text = format!("{project}{}{line}", l.title);
		let colour = if has_pane { P.fg } else { P.fg2 };
		let left = [Segment::new(text, colour)];
		let shown = match &l.pr {
			Some(pr) => {
				let (word, ci) = match pr.ci {
					PrCi::Pending => ("pending", P.yellow),
					PrCi::Passing => ("passing", P.green),
					PrCi::Failing => ("failing", P.red),
					PrCi::Merged => ("merged", P.purple),
					PrCi::Closed => ("closed", P.fg3),
				};
				let facts = [Segment::new(format!("PR #{}", pr.number), P.fg2), Segment::new(format!(" {word}"), ci)];
				spread(&left, &facts, width.saturating_sub(2), None)
			}
			None => left.to_vec(),
		};
		out.push(row(&shown, pick_of(l.pane_id.as_deref(), None), state, width));
	}
	out
}

/// CALENDAR: meetings still ahead, and a started one until its end, as "now" (0581, UX-14).
fn calendar_rows(state: &TodayState, today: &Today, now: i64, width: usize) -> Vec<Row> {
	let clock = hhmm(now);
	let meetings: Vec<_> = today
		.calendar
		.iter()
		.filter(|c| c.time == "all day" || c.time >= clock || c.end.as_deref().is_some_and(|end| end > clock.as_str()))
		.collect();
	let mut out = vec![Row::plain(heading("CALENDAR", width))];
	if meetings.is_empty() {
		out.push(Row::plain(quiet(&["nothing more on the calendar today"], width)));
	}
	for c in meetings {
		let when = if c.time == "all day" {
			c.time.clone()
		} else if c.time < clock {
			format!("{:<5}", "now")
		} else {
			c.time.clone()
		};
		let pick = pick_of(None, c.url.as_deref());
		let line = [Segment::new(format!("{when} "), P.cyan), Segment::new(c.title.clone(), P.fg)];
		let tail = pick.is_some().then(link);
		out.push(row(&spread(&line, &[], width.saturating_sub(2), tail), pick, state, width));
	}
	out
}

/// `1 account`, `3 accounts`.
fn accounts_of(n: u32) -> String {
	format!("{n} account{}", if n == 1 { "" } else { "s" })
}

/// An account's line (0669): `name 5h 42% · 7d 18% · resets 16:30`, a high window in red; the reset is the soonest of
/// its windows, with the weekday when it is more than a day away.
fn account_line(account: &QuotaAccount, now: i64, width: usize) -> Line {
	let windows: Vec<(&str, &QuotaWindow)> = [("5h", account.five_h.as_ref()), ("7d", account.seven_d.as_ref())]
		.into_iter()
		.filter_map(|(label, w)| w.map(|w| (label, w)))
		.collect();
	let mut line = vec![Segment::plain("  "), Segment::new(account.name.clone(), P.fg)];
	let mut first = true;
	let separate = |line: &mut Line, first: &mut bool| {
		line.push(if *first { Segment::plain(" ") } else { Segment::new(" · ", P.comment) });
		*first = false;
	};
	for (label, w) in &windows {
		separate(&mut line, &mut first);
		line.push(Segment::new(format!("{label} {}%", quota_percent(w)), if quota_high(w) { P.red } else { P.fg2 }));
	}
	let reset =
		windows.iter().filter_map(|(_, w)| w.resets_at.as_deref().and_then(parse_time)).filter(|at| *at > now).min();
	if let Some(at) = reset {
		separate(&mut line, &mut first);
		let when = if at - now <= 86_400_000 { hhmm(at) } else { format!("{} {}", weekday(at), hhmm(at)) };
		line.push(Segment::new(format!("resets {when}"), P.fg3));
	}
	fit(&line, width)
}

/// A `left` line (0669): how many accounts still have room in a window.
fn capacity_line(capacity: &QuotaCapacity, width: usize) -> Line {
	let window = match capacity.window {
		QuotaSpan::FiveHours => "5h",
		QuotaSpan::SevenDays => "7d",
	};
	let text = format!("  left {window}: {} of {}", capacity.remaining, accounts_of(capacity.accounts));
	fit(&[Segment::new(text, P.fg3)], width)
}

/// QUOTA (0669): a line per logged-in account; none while no account is known.
fn quota_rows(state: &TodayState, now: i64, width: usize) -> Vec<Row> {
	let Some(hud) = state.hud.as_ref().filter(|h| !h.accounts.is_empty()) else { return Vec::new() };
	let mut out = vec![Row::plain(heading("QUOTA", width))];
	out.extend(hud.accounts.iter().map(|a| Row::plain(account_line(a, now, width))));
	out
}

/// QUOTA's `left` lines (0669), a line per window after the accounts; none while no account is known.
fn capacity_rows(state: &TodayState, width: usize) -> Vec<Row> {
	let Some(hud) = state.hud.as_ref().filter(|h| !h.accounts.is_empty()) else { return Vec::new() };
	hud.capacity.iter().map(|c| Row::plain(capacity_line(c, width))).collect()
}

/// LEFTOVERS (0670): thread worktrees still on disk, one pickable row each (`x` removes it, behind his Allow); the
/// section is not there while none is.
fn leftover_rows(state: &TodayState, width: usize) -> Vec<Row> {
	let leftovers = state.dashboard.as_ref().map(|d| d.leftovers.as_slice()).unwrap_or_default();
	if leftovers.is_empty() {
		return Vec::new();
	}
	let mut out = vec![Row::plain(heading("LEFTOVERS", width))];
	for l in leftovers {
		let folder = l.folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
		let key = l.folder.to_string_lossy().into_owned();
		let pick = Pick { key: key.clone(), pane_id: None, url: None, news: false, leftover: Some(key) };
		out.push(row(&[Segment::new(format!("{}/{folder}", l.project), P.fg2)], Some(pick), state, width));
	}
	out
}

/// The sections above NEXT and NEWS, each row with what Enter does on it (for the picked row and scrolling). QUOTA's
/// `left` lines are there only when every row fits in `room`: they are what gives way first at small heights.
fn body(state: &TodayState, now: i64, width: usize, room: usize) -> Vec<Row> {
	let Some(today) = &state.today else { return Vec::new() };
	let mut rows = needs_you_rows(state, today, now, width);
	rows.extend(underway_rows(state, today, now, width));
	rows.extend(landed_rows(state, today, width));
	rows.extend(calendar_rows(state, today, now, width));
	rows.extend(quota_rows(state, now, width));
	let capacity = capacity_rows(state, width);
	let leftovers = leftover_rows(state, width);
	if rows.len() + capacity.len() + leftovers.len() <= room {
		rows.extend(capacity);
	}
	rows.extend(leftovers);
	rows
}

/// His last key's outcome over up to two rows (0581, UX-5), gone a minute after it came.
fn said(state: &TodayState, now: i64, width: usize) -> Vec<Line> {
	let Some(outcome) = &state.said else { return Vec::new() };
	if now - outcome.at >= SAID_MS {
		return Vec::new();
	}
	let colour = if outcome.is_error { P.red } else { P.green };
	let lines = wrap(&outcome.text, width);
	let shown = if lines.len() > 2 { vec![lines[0].clone(), cut(&lines[1..].join(" "), width)] } else { lines };
	shown.iter().map(|text| fit(&[Segment::new(text.as_str(), colour)], width)).collect()
}

/// The pickable rows once each key is counted once, in drawn order.
fn unique(picks: Vec<Pick>) -> Vec<Pick> {
	let mut out: Vec<Pick> = Vec::new();
	for pick in picks {
		match out.iter_mut().find(|p| p.key == pick.key) {
			Some(seen) => *seen = pick,
			None => out.push(pick),
		}
	}
	out
}

/// The headlines of every feed, newest first (an unparseable time last, the feeds' order kept among equals).
fn news_of(state: &TodayState) -> Vec<(&String, &crate::frontier::protocol::Headline)> {
	let mut news: Vec<_> = state
		.dashboard
		.iter()
		.flat_map(|d| d.news.iter())
		.flat_map(|feed| feed.items.iter().map(move |item| (&feed.heading, item)))
		.collect();
	news.sort_by_key(|(_, item)| match parse_time(&item.at) {
		Some(at) => (0u8, -at),
		None => (1, 0),
	});
	news
}

/// The pane's frame for `width` × `height` at `now` (epoch ms): the header, the sections (scrolled only as far as the
/// picked row needs; QUOTA's `left` lines go first when they don't fit), then in what is left NEXT (scheduled threads,
/// his notes) and the NEWS box (up to [`NEWS_LINES`] items, reserving its heading and blank line before NEXT takes
/// any), then his last key's outcome and the key line. Every line fits the width.
pub fn render(state: &TodayState, now: i64, width: usize, height: usize) -> Frame {
	let mut bottom = said(state, now, width);
	bottom.push(key_line(width));
	let mut top = if state.up { header(state, now, width) } else { vec![offline(state, now, width)] };
	top.push(Vec::new());
	let room = height.saturating_sub(top.len() + bottom.len());
	let mut rows = body(state, now, width, room);
	let mut picks: Vec<Pick> = rows.iter().filter_map(|r| r.pick.clone()).collect();
	if rows.len() > room {
		let picked = rows.iter().position(|r| r.pick.as_ref().is_some_and(|p| Some(&p.key) == state.selected.as_ref()));
		let start = match picked {
			Some(at) if at >= room => at + 1 - room,
			_ => 0,
		};
		rows = rows.into_iter().skip(start).take(room).collect();
	}
	let mut rows_at: Vec<(usize, String)> =
		rows.iter().enumerate().filter_map(|(i, r)| r.pick.as_ref().map(|p| (top.len() + i, p.key.clone()))).collect();
	let mut lines = top;
	lines.extend(rows.into_iter().map(|r| r.line));
	let spare = |lines: &[Line]| height as i64 - lines.len() as i64 - bottom.len() as i64;
	let news = news_of(state);
	// The NEWS box keeps its heading, a blank line and up to five items before NEXT gets what is left (0668).
	let reserve = if !news.is_empty() && spare(&lines) >= 3 { (2 + NEWS_LINES).min(spare(&lines) as usize) } else { 0 };
	// Scheduled threads, then his notes, after the calendar, while rows remain (0581 UX-14, 0673).
	let notes = state.today.as_ref().map(|t| t.next.as_slice()).unwrap_or_default();
	let scheduled = state.today.as_ref().map(|t| t.scheduled.as_slice()).unwrap_or_default();
	let next_room = spare(&lines) - reserve as i64;
	if next_room >= 2 && (!notes.is_empty() || !scheduled.is_empty()) {
		lines.push(heading("NEXT", width));
		let due =
			scheduled.iter().map(|s| format!("{} {} {}: {}", weekday(s.at_ms), hhmm(s.at_ms), s.project, s.brief));
		let jotted = notes.iter().map(|n| {
			let project = n.project.as_deref().filter(|p| !p.is_empty()).map(|p| format!("{p}: ")).unwrap_or_default();
			format!("{project}{} (#{})", n.text, n.id)
		});
		for text in due.chain(jotted).take((next_room - 1) as usize) {
			lines.push(fit(&[Segment::new(format!("  {text}"), P.fg)], width));
		}
	}
	let mut news_box = None;
	if !news.is_empty() && spare(&lines) >= 3 {
		let shown = (spare(&lines) as usize - 2).min(NEWS_LINES).min(news.len());
		let first = state.news_top.min(news.len() - shown);
		lines.push(Vec::new());
		lines.push(heading("NEWS", width));
		let start = lines.len();
		for (feed, item) in news.iter().skip(first).take(shown) {
			let pick = pick_of(None, Some(&item.url)).map(|p| Pick { news: true, ..p });
			let line = [Segment::new(format!("{feed}: "), P.fg3), Segment::new(item.title.clone(), P.fg2)];
			let tail = pick.is_some().then(link);
			lines.push(row(&spread(&line, &[], width.saturating_sub(2), tail), pick.clone(), state, width).line);
			if let Some(pick) = &pick {
				rows_at.push((lines.len() - 1, pick.key.clone()));
			}
			picks.extend(pick);
		}
		let keys = news.iter().map(|(_, item)| item.url.clone()).collect();
		news_box = Some(NewsBox { lines: start..lines.len(), top: first, keys });
	}
	let above = height.saturating_sub(bottom.len());
	while lines.len() < above {
		lines.push(Vec::new());
	}
	lines.truncate(above);
	lines.extend(bottom);
	lines.truncate(height);
	rows_at.retain(|(at, _)| *at < above);
	Frame { lines, picks: unique(picks), rows: rows_at, news: news_box }
}

#[cfg(test)]
#[path = "view_tests.rs"]
pub(crate) mod tests;
