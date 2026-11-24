//! `tui`'s tests: the column math and wrapping the TypeScript's `ask/view.test.ts` checked, the lines drawn into a
//! ratatui buffer, and the palette against the brief page's colours.

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
fn the_palette_matches_the_brief_pages_colours() {
	let css = include_str!("brief/page/brief.css");
	let hex = |property: &str| {
		let at = css.find(&format!("{property}:")).unwrap_or_else(|| panic!("brief.css has no {property}"));
		let rest = css[at + property.len() + 1..].trim_start();
		let digits = rest.strip_prefix('#').unwrap_or_else(|| panic!("{property} is not a hex colour"));
		let n = u32::from_str_radix(&digits[..6], 16).unwrap_or(0);
		Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
	};
	let p = TOKYO_NIGHT;
	let table = [
		("--bg", p.bg0),
		("--panel", p.bg2),
		("--fg", p.fg),
		("--blue", p.blue),
		("--yellow", p.yellow),
		("--red", p.red),
		("--green", p.green),
		("--purple", p.purple),
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
