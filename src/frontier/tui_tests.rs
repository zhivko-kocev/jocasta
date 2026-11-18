//! `tui`'s tests: the column math and wrapping the TypeScript's `ask/view.test.ts` checked, the lines drawn into a
//! ratatui buffer, the palette against `palette.css`, and the pane loop on a test backend.

use std::sync::{Arc, Mutex};

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::*;

/// The rows of `buf` as text: a wide character's covered cell skipped, trailing blanks dropped.
pub(crate) fn buffer_rows(buf: &Buffer) -> Vec<String> {
	let width = usize::from(buf.area.width);
	(0..buf.area.height)
		.map(|y| {
			let mut row = String::new();
			let mut x = 0;
			while x < width {
				let symbol = buf[(u16::try_from(x).unwrap_or(0), y)].symbol();
				row.push_str(symbol);
				x += columns(symbol).max(1);
			}
			row.trim_end().to_string()
		})
		.collect()
}

/// `lines` drawn into a `width` × `height` test terminal, as rows of text.
pub(crate) fn draw_rows(lines: &[Line], width: u16, height: u16) -> Vec<String> {
	let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
	let area = buf.area;
	render_lines(&mut buf, area, lines);
	buffer_rows(&buf)
}

#[test]
fn wrap_keeps_words_whole_within_the_width_and_splits_a_word_longer_than_the_line_keeping_every_character() {
	assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
	let path = "D:\\work\\acme\\.jocasta\\worktrees\\acme-api-export-retry-backoff-with-jitter";
	let lines = wrap(&format!("asks to delete {path}"), 30);
	assert!(lines.iter().all(|l| columns(l) <= 30));
	assert_eq!(lines.join("").replace(' ', ""), format!("askstodelete{path}"));
	// Wide characters count two columns each.
	let wide = wrap("修复导出重试的退避逻辑", 8);
	assert!(wide.iter().all(|l| columns(l) <= 8));
	assert_eq!(wide.join(""), "修复导出重试的退避逻辑");
}

#[test]
fn columns_count_wide_characters_twice_and_cut_ends_in_an_ellipsis() {
	assert_eq!(columns("abc"), 3);
	assert_eq!(columns("修复"), 4);
	assert_eq!(cut("hello", 5), "hello");
	assert_eq!(cut("hello world", 6), "hello…");
	assert_eq!(cut("hello", 1), "…");
	assert_eq!(cut("hello", 0), "");
	// A wide character never splits: five columns of "修" leave two wide ones and the ellipsis.
	assert_eq!(cut("修复导出", 6), "修复…");
	assert_eq!(cut("修复导出", 5), "修复…");
	assert_eq!(cut("修复导出", 8), "修复导出");
}

#[test]
fn fit_cuts_a_line_to_its_width_across_segments() {
	let line = vec![Segment::new("abcd", TOKYO_NIGHT.fg), Segment::new("efgh", TOKYO_NIGHT.red).bold()];
	let fitted = fit(&line, 6);
	assert_eq!(fitted.iter().map(|s| s.text.as_str()).collect::<String>(), "abcde…");
	assert_eq!(fitted[1].text, "e…");
	assert!(fitted[1].bold);
	assert_eq!(line_columns(&fit(&line, 5)), 5);
	assert!(fit(&line, 0).is_empty());
}

#[test]
fn combining_marks_stay_with_their_base_when_a_word_is_split() {
	let pieces = wrap("e\u{301}e\u{301}e\u{301}", 2);
	assert_eq!(pieces, ["e\u{301}e\u{301}", "e\u{301}"]);
}

#[test]
fn render_lines_draws_each_line_a_row_in_its_colours_and_clips_at_the_edge() {
	let lines: Vec<Line> = vec![
		vec![Segment::new("ab", TOKYO_NIGHT.red).bold(), Segment::plain("cd").on(TOKYO_NIGHT.bg2)],
		vec![],
		vec![Segment::plain("修复导出")],
		vec![Segment::plain("dropped")],
	];
	let mut buf = Buffer::empty(Rect::new(0, 0, 6, 3));
	let area = buf.area;
	render_lines(&mut buf, area, &lines);
	assert_eq!(buffer_rows(&buf), ["abcd", "", "修复导"]);
	assert_eq!(buf[(0, 0)].fg, TOKYO_NIGHT.red);
	assert!(buf[(0, 0)].modifier.contains(Modifier::BOLD));
	assert_eq!(buf[(0, 0)].bg, TOKYO_NIGHT.bg0);
	assert_eq!(buf[(2, 0)].fg, TOKYO_NIGHT.fg);
	assert_eq!(buf[(2, 0)].bg, TOKYO_NIGHT.bg2);
	assert_eq!(buf[(5, 1)].bg, TOKYO_NIGHT.bg0);
}

#[test]
fn the_palette_matches_palette_css() {
	let css = include_str!("canvas/palette.css");
	let hex = |property: &str| {
		let at = css.find(&format!("{property}:")).unwrap_or_else(|| panic!("palette.css has no {property}"));
		let rest = css[at + property.len() + 1..].trim_start();
		let digits = rest.strip_prefix('#').unwrap_or_else(|| panic!("{property} is not a hex colour"));
		let n = u32::from_str_radix(&digits[..6], 16).unwrap_or(0);
		Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
	};
	let p = TOKYO_NIGHT;
	let table = [
		("--bg-0", p.bg0),
		("--bg-1", p.bg1),
		("--bg-2", p.bg2),
		("--bg-3", p.bg3),
		("--fg", p.fg),
		("--fg-2", p.fg2),
		("--fg-3", p.fg3),
		("--mark-3", p.mark3),
		("--comment", p.comment),
		("--bright-black", p.bright_black),
		("--blue", p.blue),
		("--cyan", p.cyan),
		("--yellow", p.yellow),
		("--red", p.red),
		("--green", p.green),
		("--purple", p.purple),
		("--orange", p.orange),
	];
	for (property, colour) in table {
		assert_eq!(hex(property), colour, "{property}");
	}
}

#[test]
fn strip_ansi_removes_colour_codes_and_title_sequences_but_keeps_text() {
	assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0m b"), "ared b");
	assert_eq!(strip_ansi("x\u{1b}]0;title\u{7}y\u{1b}]8;;u\u{1b}\\z"), "xyz");
	assert_eq!(strip_ansi("plain 修复"), "plain 修复");
}

/// A program that records what the loop hands it.
struct Probe {
	log: Arc<Mutex<Vec<String>>>,
	pane: Pane,
	ends_on_q: bool,
}

impl PaneProgram for Probe {
	fn lines(&mut self, width: u16, height: u16) -> Vec<Line> {
		self.log.lock().unwrap().push(format!("lines {width}x{height}"));
		vec![vec![Segment::plain("hello")]]
	}

	fn on_key(&mut self, key: KeyEvent) {
		self.log.lock().unwrap().push(format!("key {:?}", key.code));
		if self.ends_on_q && key.code == KeyCode::Char('q') {
			self.pane.end(5);
		}
	}

	fn on_paste(&mut self, text: &str) {
		self.log.lock().unwrap().push(format!("paste {text}"));
	}

	fn cleanup(&mut self) {
		self.log.lock().unwrap().push("cleanup".to_string());
	}
}

/// Runs `Probe` on a 20×4 test terminal with `queued` already waiting; resolves its code, its log and the rows.
async fn run_probe(queued: Vec<Msg>, ends_on_q: bool) -> (i32, Vec<String>, Vec<String>) {
	let log = Arc::new(Mutex::new(Vec::new()));
	let (tx, rx) = unbounded_channel();
	for msg in queued {
		tx.send(msg).unwrap();
	}
	let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
	let code = drive(&mut terminal, tx, rx, |pane| Probe { log: Arc::clone(&log), pane, ends_on_q }).await.unwrap();
	let rows = buffer_rows(terminal.backend().buffer());
	let log = log.lock().unwrap().clone();
	(code, log, rows)
}

fn key(code: KeyCode, kind: KeyEventKind) -> Msg {
	Msg::Event(Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)))
}

#[tokio::test]
async fn the_loop_draws_at_once_redraws_on_resize_and_cleans_up_when_the_program_ends() {
	let (code, log, rows) = run_probe(vec![Msg::Event(Event::Resize(20, 4)), Msg::Draw, Msg::End(3)], false).await;
	assert_eq!(code, 3);
	assert_eq!(log, ["lines 20x4", "lines 20x4", "lines 20x4", "cleanup"]);
	assert_eq!(rows[0], "hello");
}

#[tokio::test]
async fn keys_reach_the_program_but_releases_do_not_and_paste_arrives_stripped_of_escape_sequences() {
	let queued = vec![
		key(KeyCode::Char('a'), KeyEventKind::Press),
		key(KeyCode::Char('b'), KeyEventKind::Release),
		Msg::Event(Event::Paste("x\u{1b}[31my".to_string())),
		Msg::End(0),
	];
	let (_, log, _) = run_probe(queued, false).await;
	assert_eq!(log, ["lines 20x4", "key Char('a')", "paste xy", "cleanup"]);
}

#[tokio::test]
async fn the_first_end_counts_and_a_draw_after_it_does_nothing() {
	let (code, log, _) = run_probe(vec![Msg::End(7), Msg::End(9), Msg::Draw], false).await;
	assert_eq!(code, 7);
	assert_eq!(log, ["lines 20x4", "cleanup"]);
}

#[tokio::test]
async fn a_program_ends_itself_through_its_pane_from_a_key() {
	let (code, log, _) = run_probe(vec![key(KeyCode::Char('q'), KeyEventKind::Press)], true).await;
	assert_eq!(code, 5);
	assert_eq!(log.last().map(String::as_str), Some("cleanup"));
}

#[tokio::test]
async fn pane_draw_from_another_task_redraws_the_loop() {
	let log = Arc::new(Mutex::new(Vec::new()));
	let (tx, rx) = unbounded_channel();
	let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
	let code = drive(&mut terminal, tx, rx, |pane| {
		let other = pane.clone();
		tokio::spawn(async move {
			other.draw();
			other.end(2);
		});
		Probe { log: Arc::clone(&log), pane, ends_on_q: false }
	})
	.await
	.unwrap();
	assert_eq!(code, 2);
	assert_eq!(log.lock().unwrap().as_slice(), ["lines 20x4", "lines 20x4", "cleanup"]);
}
