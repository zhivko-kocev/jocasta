//! `tui`'s tests: the column math and wrapping the TypeScript's `ask/view.test.ts` checked, the lines drawn into a
//! ratatui buffer, and the palette file.

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
fn the_palette_file_replaces_valid_colours_by_name_and_keeps_the_rest() {
	let file = serde_json::json!({
		"ompTheme": "dark-terminal",
		"colors": {"bg-0": "#2e3440", "red": "#BF616A", "orange": "#d08770", "blue": "nope", "green": "#a3be8", "pink": "#ff00ff"}
	});
	let palette = apply_colours(&palette_colours(&file));
	assert_eq!(palette.bg0, Color::Rgb(0x2e, 0x34, 0x40));
	assert_eq!(palette.red, Color::Rgb(0xbf, 0x61, 0x6a));
	assert_eq!(palette.orange, Color::Rgb(0xd0, 0x87, 0x70));
	assert_eq!(palette.blue, TOKYO_NIGHT.blue);
	assert_eq!(palette.green, TOKYO_NIGHT.green);
	assert_eq!(palette.fg, TOKYO_NIGHT.fg);
}

#[test]
fn no_colours_mean_tokyo_night_and_the_font_is_a_plain_family_name() {
	for file in [serde_json::json!([1]), serde_json::json!({"colors": "x"}), serde_json::json!({})] {
		assert_eq!(apply_colours(&palette_colours(&file)), TOKYO_NIGHT);
		assert_eq!(palette_font(&file), None);
	}
	assert_eq!(palette(), &TOKYO_NIGHT);
	assert_eq!(font_family(), None);
	let font = |name: &str| palette_font(&serde_json::json!({ "font": name }));
	assert_eq!(font(" JetBrainsMono Nerd Font ").as_deref(), Some("JetBrainsMono Nerd Font"));
	for bad in ["", "  ", "a\nb", &"x".repeat(101)] {
		assert_eq!(font(bad), None, "{bad:?}");
	}
}

#[test]
fn strip_ansi_removes_colour_codes_and_title_sequences_but_keeps_text() {
	assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0m b"), "ared b");
	assert_eq!(strip_ansi("x\u{1b}]0;title\u{7}y\u{1b}]8;;u\u{1b}\\z"), "xyz");
	assert_eq!(strip_ansi("plain 修复"), "plain 修复");
}
