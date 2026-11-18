//! `view`'s tests: the TypeScript's `test/frontier/today/view.test.ts`, drawn into a ratatui buffer.

use chrono::{DateTime, Local, SecondsFormat, TimeZone};

use super::*;
use crate::frontier::protocol::{
	Activity, CalendarEntry, Disk, Headline, LandedItem, Machine, NeedsYou, NewsList, NextItem, QuotaWindow, RailItem,
	Ram, ServiceHealth, ServiceName, Weather,
};
use crate::frontier::tui::render_lines;
use crate::frontier::tui::tests::draw_rows;

fn at(y: i32, month: u32, d: u32, h: u32, m: u32) -> i64 {
	Local.with_ymd_and_hms(y, month, d, h, m, 0).single().expect("a real local time").timestamp_millis()
}

fn now() -> i64 {
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

fn today() -> Today {
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
		}],
		underway: vec![working, idle],
		next: vec![NextItem {
			id: 7,
			text: "look at the slow search endpoint".to_string(),
			project: Some("acme-api".to_string()),
		}],
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

fn dashboard() -> Dashboard {
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
	assert_eq!(shown.last().unwrap(), KEY_LINE);
	assert_eq!(KEY_LINE, KEYS.join(" · "));
	assert_eq!(KEY_LINE, "↑↓ pick · Enter go/open · f Finish · s Stop · r Review · n New");
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
	assert_eq!(picks[0], Pick { key: "w2:p1".to_string(), pane_id: Some("w2:p1".to_string()), url: None });
	assert_eq!(
		picks[1],
		Pick {
			key: "https://example.org/web/pull/12".to_string(),
			pane_id: None,
			url: Some("https://example.org/web/pull/12".to_string())
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
	let services = vec![
		ServiceHealth {
			name: ServiceName::Whiteboard,
			state: ServiceState::Down,
			note: Some("not answering".to_string()),
		},
		ServiceHealth { name: ServiceName::Herdr, state: ServiceState::Up, note: None },
	];
	let quiet_hud = Hud { quiet: Some(true), ..hud() };
	let broken = up(None, Some(quiet_hud), Some(Dashboard { services, ..dashboard() }));
	let header = &lines(&broken, 200, 5)[0];
	assert!(header.starts_with("Mon 9 Nov 14:30 · quiet · whiteboard down (not answering) · "));
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
