//! `view`'s tests: the TypeScript's `test/frontier/today/view.test.ts`, drawn into a ratatui buffer.

use chrono::{DateTime, Local, SecondsFormat, TimeZone};

use super::*;
use crate::frontier::protocol::{
	Activity, BriefLink, CalendarEntry, ChecksLoad, Disk, Headline, LandedItem, Leftover, Machine, NeedsYou, NewsList,
	NextItem, PrFacts, PrivateRow, PrivateSection, RailItem, Ram, ScheduledItem, ServiceHealth, ServiceName,
	ThreadDiff, TopThread, Weather,
};
use crate::frontier::tui::tests::draw_rows;
use crate::frontier::tui::{Palette, TOKYO_NIGHT, render_lines};
use crate::shared::private::Private;

/// Tests never load a palette file, so the views draw Tokyo Night.
const P: Palette = TOKYO_NIGHT;

fn at(y: i32, month: u32, d: u32, h: u32, m: u32) -> i64 {
	Local.with_ymd_and_hms(y, month, d, h, m, 0).single().expect("a real local time").timestamp_millis()
}

pub(crate) fn now() -> i64 {
	at(2026, 11, 9, 14, 30)
}

fn iso(ms: i64) -> String {
	DateTime::from_timestamp_millis(ms).expect("a time").to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn text(line: &Line) -> String {
	line.iter().map(|s| s.text.as_str()).collect()
}

/// The frame drawn into a `width` × `height` buffer, as rows of text.
fn lines_at(state: &TodayState, width: usize, height: usize, at: i64) -> Vec<String> {
	let frame = render(state, at, width, height);
	draw_rows(&frame.lines, width as u16, height as u16)
}

fn lines(state: &TodayState, width: usize, height: usize) -> Vec<String> {
	lines_at(state, width, height, now())
}

/// Whether every line of the frame fits `width` columns.
fn fits(state: &TodayState, width: usize, height: usize) -> bool {
	render(state, now(), width, height).lines.iter().all(|l| line_columns(l) <= width)
}

fn rail(pane: &str, title: &str, project: &str, state: Option<ThreadState>) -> RailItem {
	RailItem {
		pane_id: pane.to_string(),
		title: title.to_string(),
		project: Some(project.to_string()),
		line: String::new(),
		overlaps: None,
		state,
		since: None,
		cost: None,
		context: None,
		diff: None,
		check: None,
		stuck_for: None,
		halted: None,
		progress: None,
		budget: None,
	}
}

fn entry(time: &str, title: &str) -> CalendarEntry {
	CalendarEntry { time: time.to_string(), title: title.to_string(), url: None, end: None }
}

fn needs(kind: NeedsYouKind, text: &str) -> NeedsYou {
	NeedsYou {
		kind,
		text: text.to_string(),
		url: None,
		pane_id: None,
		project: None,
		time: None,
		since: None,
		overlaps: None,
	}
}

pub(crate) fn today() -> Today {
	let mut sync = entry("14:00", "Client-a sync");
	sync.end = Some("15:00".to_string());
	sync.url = Some("https://meet.example.org/a".to_string());
	let mut blocked = needs(NeedsYouKind::Blocked, "Retry — which backoff?");
	blocked.pane_id = Some("w2:p1".to_string());
	blocked.project = Some("acme-api".to_string());
	let mut review = needs(NeedsYouKind::Review, "example-org/web#12 — review requested");
	review.url = Some("https://example.org/web/pull/12".to_string());
	let mut working = rail("w3:p1", "Export v2", "acme-api", Some(ThreadState::Working));
	working.line = "writing tests".to_string();
	working.since = Some(now() - 25 * 60_000);
	working.cost = Some(1.5);
	working.context = Some(83_400);
	let mut idle = rail("w4:p1", "Cleanup", "client-a", Some(ThreadState::Idle));
	idle.since = Some(now() - 3_600_000);
	Today {
		date: "2026-11-09".to_string(),
		calendar: vec![
			entry("all day", "Release week"),
			entry("09:00", "Standup"),
			sync,
			entry("16:00", "Review with client-a"),
		],
		needs_you: vec![blocked, review],
		landed: vec![LandedItem {
			pane_id: None,
			title: "Docs pass".to_string(),
			project: Some("acme-api".to_string()),
			line: "merged into main as 1234567".to_string(),
			overlaps: None,
			state: None,
			since: None,
			cost: None,
			context: None,
			pr: None,
		}],
		underway: vec![working, idle],
		next: vec![NextItem {
			id: 7,
			text: "look at the slow search endpoint".to_string(),
			project: Some("acme-api".to_string()),
		}],
		scheduled: Vec::new(),
		queued: Vec::new(),
		mail: None,
		jira: None,
		azure: None,
		briefs: Vec::new(),
	}
}

fn window(span: QuotaSpan, used: f64, resets_at: Option<String>) -> QuotaWindow {
	QuotaWindow { window: span, used, resets_at }
}

fn hud() -> Hud {
	Hud {
		running: 1,
		blocked: 1,
		awaiting: 2,
		spend_today: Some(3.2),
		quota: vec![window(QuotaSpan::FiveHours, 0.84, None), window(QuotaSpan::SevenDays, 0.18, None)],
		quiet: None,
		spend_avg_7d: None,
		top_thread: None,
		accounts: Vec::new(),
		capacity: Vec::new(),
		checks: None,
	}
}

fn headline(title: &str, url: &str) -> Headline {
	Headline {
		title: title.to_string(),
		source: "hn".to_string(),
		url: url.to_string(),
		at: "2026-11-09T10:00:00Z".to_string(),
	}
}

pub(crate) fn dashboard() -> Dashboard {
	Dashboard {
		news: vec![NewsList {
			heading: "Hacker News".to_string(),
			items: vec![headline("A headline", "https://example.org/1"), headline("Another", "https://example.org/2")],
		}],
		news_error: None,
		weather: Some(Weather {
			place: "Example City".to_string(),
			temp_c: 11.6,
			code: 2,
			summary: "Partly cloudy".to_string(),
			high_c: 14.0,
			low_c: 6.0,
			at: "2026-11-09T13:00:00Z".to_string(),
		}),
		weather_error: None,
		machine: Some(Machine { cpu: 0.23, ram: Ram { used: 6, total: 10 }, disk: Disk { free: 1, total: 2 } }),
		services: Vec::new(),
		activity: Activity { events: Vec::new(), worklog: Vec::new(), summaries: Vec::new() },
		leftovers: Vec::new(),
	}
}

fn up(today: Option<Today>, hud: Option<Hud>, dashboard: Option<Dashboard>) -> TodayState {
	TodayState { up: true, today, hud, dashboard, ..TodayState::default() }
}

fn position(rows: &[String], row: &str) -> Option<usize> {
	rows.iter().position(|l| l == row)
}

fn has(rows: &[String], row: &str) -> bool {
	position(rows, row).is_some()
}

#[test]
fn today_header_needs_you_first_with_kind_glyphs_underway_landed_calendar_his_notes_and_news_while_rows_remain_keys() {
	let state = TodayState { selected: Some("w3:p1".to_string()), ..up(Some(today()), Some(hud()), Some(dashboard())) };
	let shown = lines(&state, 100, 34);
	assert_eq!(shown[0], "Mon 9 Nov 14:30 · 12° Partly cloudy · cpu 23% · mem 60% · 5h 84% · 7d 18% · $3.20 today");
	let order: Vec<usize> = ["NEEDS YOU", "UNDERWAY", "LANDED TODAY", "CALENDAR", "NEXT", "NEWS"]
		.iter()
		.map(|h| position(&shown, h).expect("a heading"))
		.collect();
	assert!(order.windows(2).all(|w| w[0] < w[1]));
	assert!(order.iter().all(|i| *i > 0));
	assert!(has(&shown, "  ? acme-api/Retry — which backoff?"));
	// A review request's glyph says it opens.
	assert!(has(&shown, "  ↗ example-org/web#12 — review requested"));
	let working = shown.iter().find(|l| l.contains("acme-api/Export v2")).unwrap();
	assert!(working.starts_with("› "));
	assert!(working.ends_with("working  25 min  $1.50  83k"));
	assert!(shown.iter().find(|l| l.contains("client-a/Cleanup")).unwrap().ends_with("idle  1 h"));
	assert!(has(&shown, "  acme-api/Docs pass (merged into main as 1234567)"));
	// What is still ahead, all-day first; a started meeting stays as "now" until its end, its link marked.
	assert!(has(&shown, "  all day Release week"));
	assert!(has(&shown, "  now   Client-a sync ↗"));
	assert!(has(&shown, "  16:00 Review with client-a"));
	assert!(!has(&shown, "  09:00 Standup"));
	assert!(has(&shown, "  acme-api: look at the slow search endpoint (#7)"));
	assert!(has(&shown, "  Hacker News: A headline ↗"));
	assert_eq!(shown.len(), 34);
	// At 100 columns the last key (`x Remove`) has dropped off the right.
	assert_eq!(
		shown.last().unwrap(),
		"↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New · b Brief · R PR · F CI · X Stop all"
	);
	assert!(KEY_LINE.starts_with(shown.last().unwrap().as_str()));
	assert_eq!(KEY_LINE, KEYS.join(" · "));
	assert_eq!(
		KEY_LINE,
		"↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New · b Brief · R PR · F CI · X Stop all · x Remove"
	);
	// No greeting.
	let all = shown.join("\n").to_lowercase();
	assert!(!["good morning", "good afternoon", "good evening"].iter().any(|g| all.contains(g)));
	// The meeting ended: gone.
	let later = lines_at(&up(Some(today()), Some(hud()), None), 100, 34, at(2026, 11, 9, 15, 0));
	assert!(!has(&later, "  now   Client-a sync ↗"));
}

#[test]
fn today_rows_enter_acts_on_in_drawn_order_threads_go_to_their_pane_link_rows_open() {
	let state = up(Some(today()), Some(hud()), Some(dashboard()));
	let picks = render(&state, now(), 100, 34).picks;
	let keys: Vec<&str> = picks.iter().map(|p| p.key.as_str()).collect();
	assert_eq!(
		keys,
		[
			"w2:p1",
			"https://example.org/web/pull/12",
			"w3:p1",
			"w4:p1",
			"https://meet.example.org/a",
			"https://example.org/1",
			"https://example.org/2"
		]
	);
	assert_eq!(
		picks[0],
		Pick {
			key: "w2:p1".to_string(),
			pane_id: Some("w2:p1".to_string()),
			url: None,
			news: false,
			leftover: None,
			kind: Some(NeedsYouKind::Blocked)
		}
	);
	assert_eq!(
		picks[1],
		Pick {
			key: "https://example.org/web/pull/12".to_string(),
			pane_id: None,
			url: Some("https://example.org/web/pull/12".to_string()),
			news: false,
			leftover: None,
			kind: Some(NeedsYouKind::Review),
		}
	);
	// A picked link row carries the marker like a thread.
	let picked = TodayState { selected: Some("https://meet.example.org/a".to_string()), ..state.clone() };
	let shown = lines(&picked, 100, 34);
	assert!(shown.iter().find(|l| l.contains("Client-a sync")).unwrap().starts_with("› now   Client-a sync ↗"));
	// News only in spare rows: the shorter pane draws none and offers none.
	assert!(!render(&state, now(), 100, 16).picks.iter().any(|p| p.key.contains("example.org/1")));
}

#[test]
fn today_news_only_while_rows_remain_the_list_scrolls_only_as_far_as_the_picked_row_needs_his_last_keys_outcome() {
	let plain = up(Some(today()), Some(hud()), Some(dashboard()));
	let short = lines(&plain, 100, 15);
	assert!(!has(&short, "NEWS"));
	assert_eq!(short.len(), 15);
	let many = Today {
		underway: (0..30)
			.map(|i| {
				let mut thread =
					rail(&format!("w{i}:p1"), &format!("Thread {i}"), "acme-api", Some(ThreadState::Working));
				thread.line = String::new();
				thread
			})
			.collect(),
		..today()
	};
	let said = Said { text: "Stop: acme-api/Thread 25 isn't working.".to_string(), is_error: true, at: now() };
	let state = TodayState {
		selected: Some("w25:p1".to_string()),
		said: Some(said),
		..up(Some(many.clone()), Some(hud()), None)
	};
	let shown = lines(&state, 60, 16);
	assert!(shown.iter().any(|l| l.starts_with("› acme-api/Thread 25")));
	assert_eq!(shown[shown.len() - 2], "Stop: acme-api/Thread 25 isn't working.");
	assert!(fits(&state, 60, 16));
	// A picked row already in view: no scroll, NEEDS YOU keeps its heading.
	let top = lines(&TodayState { selected: Some("w1:p1".to_string()), ..up(Some(many), Some(hud()), None) }, 60, 16);
	assert!(has(&top, "NEEDS YOU"));
	assert!(top.iter().any(|l| l.starts_with("› acme-api/Thread 1 ")));
	// The outcome wraps over two rows at most, and goes after a minute.
	let long = Said {
		text: format!("{}end.", "Started acme-api/Fix the flaky login test: ".repeat(4)),
		is_error: false,
		at: now(),
	};
	let long_state = TodayState { said: Some(long), ..up(Some(today()), Some(hud()), None) };
	let wrapped = lines(&long_state, 40, 20);
	assert!(wrapped[wrapped.len() - 3].starts_with("Started acme-api/Fix the flaky login"));
	assert!(wrapped[wrapped.len() - 2].ends_with('…'));
	let later = lines_at(&long_state, 40, 20, now() + SAID_MS);
	assert!(!later.join("\n").contains("Started"));
}

#[test]
fn today_at_40_columns_titles_keep_18_columns_before_facts_give_way_the_header_yields_to_two_rows_the_key_line_drops_keys()
 {
	let hud_high = Hud {
		spend_today: Some(48.73),
		quota: vec![
			window(QuotaSpan::FiveHours, 0.91, Some(iso(at(2026, 11, 9, 16, 30)))),
			window(QuotaSpan::SevenDays, 0.97, Some(iso(at(2026, 11, 13, 9, 0)))),
		],
		..hud()
	};
	let mut fix = rail("w3:p1", "Fix login redirect loop", "acme-api", Some(ThreadState::Working));
	fix.since = Some(now() - 12 * 60_000);
	fix.cost = Some(0.84);
	fix.context = Some(83_000);
	let mut wide_thread = rail("w5:p1", "修复导出重试的退避逻辑", "acme-api", Some(ThreadState::Working));
	wide_thread.since = Some(now() - 48 * 60_000);
	let narrow = Today { underway: vec![fix, wide_thread], ..today() };
	let machine_dash = Dashboard {
		machine: Some(Machine { cpu: 0.07, ram: Ram { used: 6, total: 10 }, disk: Disk { free: 1, total: 2 } }),
		news: Vec::new(),
		..dashboard()
	};
	let state = up(Some(narrow.clone()), Some(hud_high.clone()), Some(machine_dash));
	let shown = lines(&state, 40, 30);
	assert!(fits(&state, 40, 30));
	assert_eq!(shown[..2], ["Mon 9 Nov 14:30 · cpu 7% · mem 60%", "5h 91% (16:30) · 7d 97% · $48.73 today"]);
	assert!(has(&shown, "  acme-api/Fix login re… working  12 min"));
	assert!(shown.iter().find(|l| l.contains("修复")).unwrap().ends_with("working  48 min"));
	assert_eq!(shown.last().unwrap(), "↑↓ pick · Enter go/open · f Finish");
	// Every line fits from 20 to 120 columns.
	let full = up(Some(narrow), Some(hud_high), Some(dashboard()));
	for width in [20, 30, 35, 60, 120] {
		assert!(fits(&full, width, 24), "width {width}");
		assert_eq!(lines(&full, width, 24).len(), 24);
	}
}

#[test]
fn today_connecting_at_first_offline_keeps_the_time_and_says_since_when_quiet_hours_and_a_broken_service_in_the_header_empty_states_hint_the_key()
 {
	assert_eq!(lines(&TodayState::default(), 60, 5)[0], "Connecting to Jocasta…");
	let offline = TodayState { seen: true, down_since: Some(at(2026, 11, 9, 14, 28)), ..TodayState::default() };
	assert_eq!(lines(&offline, 80, 5)[0], "14:30 · Jocasta isn't answering (since 14:28); trying again…");
	let services = vec![ServiceHealth {
		name: ServiceName::Surface,
		state: ServiceState::Down,
		note: Some("not answering".to_string()),
	}];
	let quiet_hud = Hud { quiet: Some(true), ..hud() };
	let broken = up(None, Some(quiet_hud), Some(Dashboard { services, ..dashboard() }));
	let header = &lines(&broken, 200, 5)[0];
	assert!(header.starts_with("Mon 9 Nov 14:30 · quiet · surface down (not answering) · "));
	let empty = Today {
		needs_you: Vec::new(),
		underway: Vec::new(),
		landed: Vec::new(),
		calendar: Vec::new(),
		next: Vec::new(),
		..today()
	};
	let shown = lines(&up(Some(empty), Some(hud()), None), 60, 14);
	assert!(has(&shown, "  nothing waits on you"));
	assert!(has(&shown, "  no thread at work · n starts one"));
	assert!(has(&shown, "  nothing more on the calendar today"));
	assert!(!has(&shown, "NEXT"));
	assert_eq!([tokens(950), tokens(83_400), tokens(1_250_000)], ["950", "83k", "1.3M"]);
}

#[test]
fn the_picked_row_is_drawn_with_the_marker_and_the_selection_colour_across_the_pane() {
	let state = TodayState { selected: Some("w3:p1".to_string()), ..up(Some(today()), Some(hud()), None) };
	let frame = render(&state, now(), 60, 30);
	let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 60, 30));
	let area = buf.area;
	render_lines(&mut buf, area, &frame.lines);
	let row = frame.lines.iter().position(|l| text(l).starts_with("› acme-api/Export v2")).expect("the picked row");
	let y = row as u16;
	assert_eq!(buf[(0, y)].fg, TOKYO_NIGHT.blue);
	assert_eq!(buf[(0, y)].bg, TOKYO_NIGHT.bg2);
	assert_eq!(buf[(59, y)].bg, TOKYO_NIGHT.bg2);
	// A row that isn't picked keeps the pane's background; a heading is bold blue.
	assert_eq!(buf[(59, y + 1)].bg, TOKYO_NIGHT.bg0);
	assert_eq!(buf[(0, 2)].fg, TOKYO_NIGHT.blue);
	assert!(buf[(0, 2)].modifier.contains(ratatui::style::Modifier::BOLD));
}

#[test]
fn numbers_are_written_as_javascript_writes_them() {
	assert_eq!(to_fixed(1.25, 1), "1.3");
	assert_eq!(to_fixed(0.005, 2), "0.01");
	assert_eq!(to_fixed(3.2, 2), "3.20");
	assert_eq!(to_fixed(48.73, 2), "48.73");
	assert_eq!(to_fixed(9.999, 2), "10.00");
	assert_eq!(money(1.5), "$1.50");
	assert_eq!(js_round(2.5), 3.0);
	assert_eq!(js_round(-2.5), -2.0);
}

/// `n` headlines, each a minute newer than the one before it in the feed's order, so the feed lists them oldest first.
fn stories(n: usize) -> Dashboard {
	let items = (0..n)
		.map(|i| Headline {
			title: format!("Story {i}"),
			source: "hn".to_string(),
			url: format!("https://example.org/s{i}"),
			at: iso(now() - (n - i) as i64 * 60_000),
		})
		.collect();
	Dashboard {
		news: vec![NewsList { heading: "Hacker News".to_string(), items }],
		weather: None,
		machine: None,
		..dashboard()
	}
}

/// A today with nothing in any section: the body is 6 rows (NEEDS YOU, UNDERWAY and CALENDAR, each with its hint).
fn bare() -> Today {
	Today {
		needs_you: Vec::new(),
		underway: Vec::new(),
		landed: Vec::new(),
		calendar: Vec::new(),
		next: Vec::new(),
		scheduled: Vec::new(),
		..today()
	}
}

fn scheduled() -> ScheduledItem {
	ScheduledItem {
		project: "acme-api".to_string(),
		brief: "dependency bumps".to_string(),
		at_ms: at(2026, 11, 16, 9, 0),
	}
}

#[test]
fn the_news_box_is_five_lines_newest_first_across_feeds_moved_by_news_top_and_shrinks_then_hides() {
	let state = up(Some(bare()), Some(hud()), Some(stories(8)));
	let shown = lines(&state, 100, 30);
	let at = position(&shown, "NEWS").expect("the box");
	let expected: Vec<String> = (3..8).rev().map(|i| format!("  Hacker News: Story {i} ↗")).collect();
	assert_eq!(shown[at + 1..at + 6], expected[..]);
	assert!(shown[at + 6].is_empty(), "five items, no more");
	let frame = render(&state, now(), 100, 30);
	let news = frame.news.as_ref().expect("the box is drawn");
	assert_eq!((news.lines.len(), news.top, news.keys.len()), (NEWS_LINES, 0, 8));
	assert_eq!(news.keys[0], "https://example.org/s7");
	assert_eq!(frame.picks.iter().filter(|p| p.news).count(), 5);
	// `news_top` moves the window; one past the end is held where the last item is the box's last line.
	let moved = lines(&TodayState { news_top: 2, ..state.clone() }, 100, 30);
	assert!(has(&moved, "  Hacker News: Story 5 ↗") && has(&moved, "  Hacker News: Story 1 ↗"));
	assert!(!has(&moved, "  Hacker News: Story 7 ↗") && !has(&moved, "  Hacker News: Story 0 ↗"));
	let held = render(&TodayState { news_top: 99, ..state.clone() }, now(), 100, 30);
	assert_eq!(held.news.as_ref().map(|n| n.top), Some(3));
	assert!(has(&lines(&TodayState { news_top: 99, ..state.clone() }, 100, 30), "  Hacker News: Story 0 ↗"));
	// Short of room it shrinks to what is left after its heading, and under three lines it is gone.
	let small = lines(&state, 100, 12);
	assert!(has(&small, "NEWS") && has(&small, "  Hacker News: Story 7 ↗") && !has(&small, "  Hacker News: Story 6 ↗"));
	assert!(!has(&lines(&state, 100, 11), "NEWS"));
}

#[test]
fn news_is_ordered_by_time_across_feeds_with_an_unreadable_time_last_and_the_feeds_order_kept_among_equals() {
	let item = |title: &str, at: &str| Headline {
		title: title.to_string(),
		source: "x".to_string(),
		url: format!("https://example.org/{title}"),
		at: at.to_string(),
	};
	let news = vec![
		NewsList {
			heading: "Hacker News".to_string(),
			items: vec![item("a", "2026-11-09T10:00:00Z"), item("junk", "soon")],
		},
		NewsList {
			heading: "LWN".to_string(),
			items: vec![item("b", "2026-11-09T12:00:00Z"), item("c", "2026-11-09T10:00:00Z")],
		},
	];
	let state = up(Some(bare()), Some(hud()), Some(Dashboard { news, ..dashboard() }));
	let keys = render(&state, now(), 100, 30).news.expect("the box").keys;
	let titles: Vec<&str> = keys.iter().map(|k| k.rsplit('/').next().unwrap()).collect();
	assert_eq!(titles, ["b", "a", "c", "junk"]);
}

#[test]
fn next_shows_the_scheduled_threads_before_his_notes_and_the_news_box_keeps_its_seven_lines_first() {
	let note = NextItem { id: 7, text: "look at the slow search endpoint".to_string(), project: None };
	let state =
		up(Some(Today { next: vec![note], scheduled: vec![scheduled()], ..bare() }), Some(hud()), Some(stories(8)));
	let roomy = lines(&state, 100, 30);
	let due = position(&roomy, "  Mon 09:00 acme-api: dependency bumps").expect("the scheduled thread");
	let jotted = position(&roomy, "  look at the slow search endpoint (#7)").expect("his note");
	assert_eq!(due + 1, jotted);
	assert!(position(&roomy, "NEXT").unwrap() < due && due < position(&roomy, "NEWS").unwrap());
	// Two rows left for NEXT after the news box's seven: its heading and the scheduled thread, not the note.
	let tight = lines(&state, 100, 18);
	assert!(has(&tight, "NEXT") && has(&tight, "  Mon 09:00 acme-api: dependency bumps"));
	assert!(!tight.iter().any(|l| l.contains("(#7)")));
	assert!(has(&tight, "  Hacker News: Story 7 ↗") && has(&tight, "  Hacker News: Story 3 ↗"));
	// With one row less NEXT is gone and the box is whole.
	let tighter = lines(&state, 100, 17);
	assert!(!has(&tighter, "NEXT") && has(&tighter, "  Hacker News: Story 3 ↗"));
}

#[test]
fn an_underway_row_shows_state_check_diff_age_cost_and_context_dropped_from_the_right_as_it_narrows() {
	let mut fix = rail("w3:p1", "Fix login", "acme-api", Some(ThreadState::Working));
	fix.since = Some(now() - 12 * 60_000);
	fix.cost = Some(0.84);
	fix.context = Some(83_000);
	fix.check = Some(true);
	fix.diff = Some(ThreadDiff { added: 84, removed: 12 });
	let mut failed = rail("w4:p1", "Cleanup", "acme-api", Some(ThreadState::Working));
	failed.check = Some(false);
	let state = up(Some(Today { underway: vec![fix, failed], ..bare() }), Some(hud()), None);
	let wide = lines(&state, 100, 20);
	assert!(
		wide.iter()
			.find(|l| l.contains("acme-api/Fix login"))
			.unwrap()
			.ends_with("working  ✓  +84 −12  12 min  $0.84  83k")
	);
	assert!(wide.iter().find(|l| l.contains("acme-api/Cleanup")).unwrap().ends_with("working  ✗"));
	// At 40 columns the title keeps its 18: context, cost and age went first.
	let narrow = lines(&state, 40, 20);
	let row = narrow.iter().find(|l| l.contains("acme-api/Fix login")).unwrap();
	assert!(row.ends_with("working  ✓  +84 −12") && !row.contains("min"), "{row}");
	assert!(fits(&state, 40, 20));
	// The diff is green for what was added and red for what was removed.
	let frame = render(&state, now(), 100, 20);
	let colour = |text: &str| frame.lines.iter().flatten().find(|s| s.text.trim() == text).and_then(|s| s.fg);
	assert_eq!((colour("+84"), colour("−12")), (Some(P.green), Some(P.red)));
	assert_eq!((colour("✓"), colour("✗")), (Some(P.green), Some(P.red)));
}

#[test]
fn a_stuck_thread_says_stuck_in_red_and_a_stuck_needs_you_row_has_the_bang() {
	let mut stuck = rail("w3:p1", "Fix login", "acme-api", Some(ThreadState::Working));
	stuck.stuck_for = Some(660_000);
	let mut row = needs(NeedsYouKind::Stuck, "Fix login has been quiet for 11 min");
	row.pane_id = Some("w3:p1".to_string());
	row.project = Some("acme-api".to_string());
	let state = up(Some(Today { underway: vec![stuck], needs_you: vec![row], ..bare() }), Some(hud()), None);
	let shown = lines(&state, 100, 20);
	assert!(shown.iter().any(|l| l.starts_with("  acme-api/Fix login") && l.ends_with("stuck")), "{shown:#?}");
	assert!(has(&shown, "  ! acme-api/Fix login has been quiet for 11 min"));
	let frame = render(&state, now(), 100, 20);
	assert!(frame.lines.iter().flatten().any(|s| s.text == "stuck" && s.fg == Some(P.red)));
}

#[test]
fn a_landed_row_shows_its_pr_and_ci_state() {
	let landed = |title: &str, pr| LandedItem {
		pane_id: None,
		title: title.to_string(),
		project: Some("acme-api".to_string()),
		line: "sent as a PR".to_string(),
		overlaps: None,
		state: None,
		since: None,
		cost: None,
		context: None,
		pr,
	};
	let today = Today {
		landed: vec![
			landed("Add SSO", Some(PrFacts { number: 7, ci: PrCi::Failing })),
			landed("Docs", Some(PrFacts { number: 8, ci: PrCi::Merged })),
			landed("Plain", None),
		],
		..bare()
	};
	let shown = lines(&up(Some(today), Some(hud()), None), 100, 30);
	let sso = shown.iter().find(|l| l.contains("Add SSO")).unwrap();
	assert!(sso.starts_with("  acme-api/Add SSO (sent as a PR)") && sso.ends_with("PR #7 failing"), "{sso}");
	assert!(shown.iter().find(|l| l.contains("Docs")).unwrap().ends_with("PR #8 merged"));
	assert_eq!(shown.iter().find(|l| l.contains("Plain")).unwrap(), "  acme-api/Plain (sent as a PR)");
}

#[test]
fn the_header_shows_the_seven_day_average_the_costliest_thread_and_the_check_queue_which_give_way_first() {
	let busy = Hud {
		spend_avg_7d: Some(4.5),
		top_thread: Some(TopThread { title: "Fix login".to_string(), cost: 2.0 }),
		checks: Some(ChecksLoad { running: 1, waiting: 2 }),
		..hud()
	};
	let state = up(Some(bare()), Some(busy), None);
	assert_eq!(
		lines(&state, 200, 10)[0],
		"Mon 9 Nov 14:30 · 5h 84% · 7d 18% · $3.20 today · avg $4.50 · top Fix login $2.00 · checks: 1 running · 2 waiting"
	);
	// Where the header would pass two rows the costliest thread goes first, then the average and the queue.
	let narrow = lines(&state, 40, 10).join("\n");
	assert!(narrow.contains("$3.20 today"));
	assert!(!narrow.contains("top Fix") && !narrow.contains("avg") && !narrow.contains("checks:"), "{narrow}");
	// Nothing recorded and nothing queued: none of it.
	let plain = lines(&up(Some(bare()), Some(hud()), None), 200, 10)[0].clone();
	assert!(plain.ends_with("$3.20 today"), "{plain}");
}

fn account(name: &str, five_h: Option<(f64, i64)>, seven_d: Option<(f64, i64)>) -> QuotaAccount {
	let of = |span, (used, reset): (f64, i64)| window(span, used, Some(iso(reset)));
	QuotaAccount {
		name: name.to_string(),
		five_h: five_h.map(|w| of(QuotaSpan::FiveHours, w)),
		seven_d: seven_d.map(|w| of(QuotaSpan::SevenDays, w)),
	}
}

fn quota_hud() -> Hud {
	Hud {
		accounts: vec![
			account("zi*", Some((0.42, at(2026, 11, 9, 16, 30))), Some((0.18, at(2026, 11, 13, 9, 0)))),
			account("98*", Some((0.91, at(2026, 11, 9, 15, 0))), None),
		],
		capacity: vec![
			QuotaCapacity { window: QuotaSpan::FiveHours, remaining: 1, accounts: 2 },
			QuotaCapacity { window: QuotaSpan::SevenDays, remaining: 2, accounts: 2 },
		],
		..hud()
	}
}

#[test]
fn the_quota_section_follows_the_calendar_with_a_line_per_account_and_a_left_line_per_window() {
	let state = up(Some(today()), Some(quota_hud()), None);
	let shown = lines(&state, 100, 30);
	let heading = position(&shown, "QUOTA").expect("the section");
	assert!(position(&shown, "CALENDAR").unwrap() < heading);
	assert_eq!(
		shown[heading + 1..heading + 5],
		[
			"  zi* 5h 42% · 7d 18% · resets 16:30",
			"  98* 5h 91% · resets 15:00",
			"  left 5h: 1 of 2 accounts",
			"  left 7d: 2 of 2 accounts",
		]
	);
	// A high window is red, a low one is not.
	let frame = render(&state, now(), 100, 30);
	let colour = |text: &str| frame.lines.iter().flatten().find(|s| s.text == text).and_then(|s| s.fg);
	assert_eq!((colour("5h 91%"), colour("5h 42%")), (Some(P.red), Some(P.fg2)));
	// Without a known account there is no section.
	assert!(!has(&lines(&up(Some(today()), Some(hud()), None), 100, 30), "QUOTA"));
}

#[test]
fn at_small_heights_the_quota_left_lines_go_first_then_the_section_and_needs_you_and_underway_stay() {
	let state = up(Some(today()), Some(quota_hud()), None);
	// 17 rows with the `left` lines, 15 without; 18 rows of height leave 15.
	let tight = lines(&state, 100, 18);
	assert!(has(&tight, "  zi* 5h 42% · 7d 18% · resets 16:30"));
	assert!(!tight.iter().any(|l| l.contains("left 5h")), "{tight:#?}");
	let roomy = lines(&state, 100, 20);
	assert!(has(&roomy, "  left 7d: 2 of 2 accounts"));
	for height in [10, 14] {
		let shown = lines(&state, 100, height);
		assert!(has(&shown, "NEEDS YOU") && has(&shown, "UNDERWAY"), "height {height}");
		assert!(!has(&shown, "QUOTA"), "height {height}");
	}
}

fn leftover(project: &str, folder: &str) -> Leftover {
	Leftover { project: project.to_string(), folder: folder.into(), branch: None, ahead: 0, dirty: 2 }
}

#[test]
fn leftovers_show_one_pickable_row_each_and_no_section_while_there_are_none() {
	let with = Dashboard { leftovers: vec![leftover("acme-api", "D:/work/acme-api-wt")], ..dashboard() };
	let state =
		TodayState { selected: Some("D:/work/acme-api-wt".to_string()), ..up(Some(today()), Some(hud()), Some(with)) };
	let shown = lines(&state, 100, 40);
	assert!(position(&shown, "CALENDAR").unwrap() < position(&shown, "LEFTOVERS").unwrap());
	assert!(shown.iter().any(|l| l.trim_end() == "› acme-api/acme-api-wt"), "{shown:#?}");
	let frame = render(&state, now(), 100, 40);
	let pick = frame.picks.iter().find(|p| p.leftover.is_some()).expect("a pickable leftover");
	assert_eq!(
		*pick,
		Pick {
			key: "D:/work/acme-api-wt".to_string(),
			pane_id: None,
			url: None,
			news: false,
			leftover: Some("D:/work/acme-api-wt".to_string()),
			kind: None,
		}
	);
	assert!(frame.rows.iter().any(|(_, key)| key == "D:/work/acme-api-wt"));
	assert!(!has(&lines(&up(Some(today()), Some(hud()), Some(dashboard())), 100, 40), "LEFTOVERS"));
}

#[test]
fn every_section_at_heights_10_20_and_40_keeps_needs_you_and_underway_and_fits() {
	let note = NextItem { id: 7, text: "look at the slow search endpoint".to_string(), project: None };
	let full = Hud {
		spend_avg_7d: Some(4.5),
		top_thread: Some(TopThread { title: "Fix login".to_string(), cost: 2.0 }),
		checks: Some(ChecksLoad { running: 1, waiting: 2 }),
		..quota_hud()
	};
	let dash = Dashboard { leftovers: vec![leftover("acme-api", "D:/work/acme-api-wt")], ..stories(8) };
	let today = Today { next: vec![note], scheduled: vec![scheduled()], ..today() };
	let state = up(Some(today), Some(full), Some(dash));
	for height in [10, 20, 40] {
		let shown = lines(&state, 100, height);
		assert_eq!(shown.len(), height);
		assert!(fits(&state, 100, height), "height {height}");
		assert!(has(&shown, "NEEDS YOU") && has(&shown, "UNDERWAY"), "height {height}");
		// The keys drop from the right as the dock narrows: at 100 columns the last one no longer fits.
		let keys = shown.last().unwrap();
		assert!(
			KEY_LINE.starts_with(keys.as_str()) && keys.starts_with("↑↓ pick") && keys.contains("b Brief"),
			"{keys}"
		);
	}
	// 10 and 20 rows have no room for NEXT and NEWS; 40 has every section, in the budget's order.
	assert!(!has(&lines(&state, 100, 20), "NEWS") && !has(&lines(&state, 100, 20), "NEXT"));
	let shown = lines(&state, 100, 40);
	let order: Vec<usize> = ["NEEDS YOU", "UNDERWAY", "LANDED TODAY", "CALENDAR", "QUOTA", "LEFTOVERS", "NEXT", "NEWS"]
		.iter()
		.map(|h| position(&shown, h).unwrap_or_else(|| panic!("{h} missing in {shown:#?}")))
		.collect();
	assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
}

fn private_row(text: &str, url: Option<&str>) -> PrivateRow {
	PrivateRow { text: Private::from(text.to_owned()), url: url.map(|url| Private::from(url.to_owned())) }
}

#[test]
fn today_draws_mail_jira_and_azure_after_the_calendar_only_once_their_pollers_ran_with_their_failure_line_and_links() {
	// Not configured: not there at all.
	let shown = lines(&up(Some(today()), Some(hud()), None), 100, 40);
	assert!(!["MAIL", "JIRA", "AZURE"].iter().any(|heading| has(&shown, heading)));

	let mut with = today();
	with.mail = Some(PrivateSection {
		rows: vec![private_row("Jane Doe: Café menu", Some("https://mail.google.com/mail/u/0/#inbox/1a2b3c01"))],
		error: None,
	});
	with.jira = Some(PrivateSection { rows: Vec::new(), error: None });
	with.azure = Some(PrivateSection {
		rows: Vec::new(),
		error: Some("azure: az account list failed: not signed in".to_owned()),
	});
	let state = up(Some(with), Some(hud()), None);
	let shown = lines(&state, 100, 40);
	let at = |row: &str| position(&shown, row).unwrap_or_else(|| panic!("{row} in {shown:#?}"));
	assert!(at("CALENDAR") < at("MAIL") && at("MAIL") < at("JIRA") && at("JIRA") < at("AZURE"));
	assert_eq!(shown[at("MAIL") + 1], "  Jane Doe: Café menu ↗");
	assert_eq!(shown[at("JIRA") + 1], "  no open issues");
	assert_eq!(shown[at("AZURE") + 1], "  azure: az account list failed: not signed in");
	// Enter opens the mail row's link.
	let picks = render(&state, now(), 100, 40).picks;
	assert!(picks.iter().any(|pick| pick.url.as_deref() == Some("https://mail.google.com/mail/u/0/#inbox/1a2b3c01")));
}

#[test]
fn today_draws_the_briefs_after_the_private_sources_only_once_built_and_enter_opens_the_picked_one_0680() {
	let shown = lines(&up(Some(today()), Some(hud()), None), 100, 40);
	assert!(!has(&shown, "BRIEFS"));

	let mut with = today();
	with.briefs = vec![
		BriefLink { label: "morning brief".to_owned(), url: "http://127.0.0.1:7/brief/morning-2026-11-09".to_owned() },
		BriefLink { label: "weekly brief".to_owned(), url: "http://127.0.0.1:7/brief/weekly-2026-W46".to_owned() },
	];
	with.mail = Some(PrivateSection { rows: Vec::new(), error: None });
	let state = up(Some(with), Some(hud()), None);
	let shown = lines(&state, 100, 40);
	let at = |row: &str| position(&shown, row).unwrap_or_else(|| panic!("{row} in {shown:#?}"));
	assert!(at("MAIL") < at("BRIEFS"));
	assert_eq!(shown[at("BRIEFS") + 1], "  morning brief ↗");
	assert_eq!(shown[at("BRIEFS") + 2], "  weekly brief ↗");
	let picks = render(&state, now(), 100, 40).picks;
	let opened: Vec<&str> = picks.iter().filter_map(|pick| pick.url.as_deref()).collect();
	assert!(opened.contains(&"http://127.0.0.1:7/brief/morning-2026-11-09"), "{opened:?}");
	assert!(opened.contains(&"http://127.0.0.1:7/brief/weekly-2026-W46"), "{opened:?}");
}
