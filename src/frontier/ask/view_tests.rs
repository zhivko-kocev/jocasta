//! `view`'s tests: the TypeScript's `test/frontier/ask/view.test.ts`, drawn into a ratatui buffer. The question
//! builders are `server/popups.ts`'s (not ported yet): the tests build the same views by hand. `wrap` and the column
//! math, that file's last case, are tested in `tui_tests.rs`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::frontier::tui::line_columns;
use crate::frontier::tui::tests::draw_rows;

fn key(name: &str) -> Key {
	Key::new(name, if name.chars().count() == 1 { name } else { "" })
}

fn typed(name: &str, sequence: &str) -> Key {
	Key::new(name, sequence)
}

/// The popup drawn into a `width` × `height` buffer, as rows of text, the blank rows above kept: a popup shorter than
/// its buffer is not padded to it, so the rows are those of its lines.
fn rows(state: &mut AskState, width: usize, height: usize) -> Vec<String> {
	let lines = ask_lines(state, width, height);
	let drawn = draw_rows(&lines, width as u16, height as u16);
	drawn.into_iter().take(lines.len()).collect()
}

/// A question whose fallback is its last choice, as every one of `server/popups.ts` has it.
fn view(title: &str, message: &str, detail: Option<&str>, choices: &[&str], code: Option<&str>) -> PopupView {
	PopupView {
		title: title.to_string(),
		message: message.to_string(),
		detail: detail.map(ToString::to_string),
		choices: choices.iter().map(ToString::to_string).collect(),
		fallback: choices.last().map(ToString::to_string).unwrap_or_default(),
		code: code.map(ToString::to_string),
		form: None,
	}
}

fn allow_question(message: &str, detail: &str, code: &str) -> PopupView {
	view("Jocasta: Allow or Deny", message, Some(detail), &["Allow", "Deny"], Some(code))
}

fn stop_question(working: u32, awaiting: u32, code: &str) -> PopupView {
	let message = format!(
		"Stop {working} working {} and {awaiting} awaiting you? They pick up where they left off next time.",
		if working == 1 { "thread" } else { "threads" }
	);
	view("Jocasta: Stop everything?", &message, None, &["Stop everything", "Cancel"], Some(code))
}

fn finish_question(name: &str, detail: &str, code: &str) -> PopupView {
	let message = format!("Merge {name} into its trunk, or open a PR?");
	view(&format!("Jocasta: Finish {name}"), &message, Some(detail), &["Merge", "PR", "Cancel"], Some(code))
}

fn new_thread_question(projects: &[&str], project: Option<&str>) -> PopupView {
	PopupView {
		form: Some(PopupForm {
			projects: projects.iter().map(ToString::to_string).collect(),
			project: project.map(ToString::to_string),
		}),
		..view(
			"Jocasta: New thread",
			"Pick the project and write the brief; the thread opens in plan mode.",
			None,
			&["Start", "Cancel"],
			None,
		)
	}
}

fn choice(choice: &str) -> Option<PopupAnswer> {
	Some(PopupAnswer { choice: choice.to_string(), typed: None, project: None, brief: None })
}

fn coded_choice(choice: &str, typed: &str) -> Option<PopupAnswer> {
	Some(PopupAnswer { choice: choice.to_string(), typed: Some(typed.to_string()), project: None, brief: None })
}

fn has(rows: &[String], row: &str) -> bool {
	rows.iter().any(|r| r == row)
}

#[test]
fn allow_only_its_three_letters_typed_allow_enter_and_other_keys_never_do_esc_denies() {
	let mut state = ask_state(allow_question("Merge acme-api/retry into main?", "2 commits.", "KQX"));
	assert!(has(&rows(&mut state, 60, 18), "Type KQX for Allow"));
	assert!(has(&rows(&mut state, 60, 18), "←→ choose · Esc Deny"));
	for k in [key("return"), key("enter"), typed("space", " "), key("1"), key("y")] {
		assert_eq!(press(&mut state, &k), None);
	}
	// "y" was a letter: it counts as typed, so it is taken back.
	press(&mut state, &key("backspace"));
	assert_eq!(press(&mut state, &key("k")), None);
	assert_eq!(press(&mut state, &key("q")), None);
	assert_eq!(press(&mut state, &key("x")), coded_choice("Allow", "KQX"));
	assert_eq!(press(&mut ask_state(state.view.clone()), &key("escape")), choice("Deny"));
	// Deny picked: one key, and letters type nothing.
	let mut deny = ask_state(state.view.clone());
	press(&mut deny, &key("right"));
	for c in ["k", "q", "x"] {
		assert_eq!(press(&mut deny, &key(c)), None);
	}
	assert_eq!(press(&mut deny, &key("return")), choice("Deny"));
}

#[test]
fn the_close_question_stop_everything_takes_its_code_never_enter_cancel_is_one_key() {
	let mut state = ask_state(stop_question(2, 1, "ABC"));
	assert!(has(&rows(&mut state, 100, 18), &state.view.message.clone()));
	assert!(has(&rows(&mut state, 100, 18), "Type ABC for Stop everything"));
	assert_eq!(press(&mut state, &key("return")), None);
	assert_eq!(press(&mut ask_state(state.view.clone()), &key("escape")), choice("Cancel"));
	let mut cancel = ask_state(state.view.clone());
	press(&mut cancel, &key("left"));
	assert!(has(&rows(&mut cancel, 100, 18), "←→ choose · Enter Cancel · Esc Cancel"));
	assert_eq!(press(&mut cancel, &key("return")), choice("Cancel"));
	for c in ["a", "b"] {
		press(&mut state, &key(c));
	}
	assert_eq!(press(&mut state, &key("c")), coded_choice("Stop everything", "ABC"));
}

#[test]
fn finish_merge_and_pr_each_take_the_code_moving_the_pick_clears_the_letters() {
	let mut finish = ask_state(finish_question("acme-api/retry", "2 commits.", "KQX"));
	assert!(has(&rows(&mut finish, 60, 18), "Type KQX for Merge"));
	assert_eq!(press(&mut finish, &key("return")), None);
	press(&mut finish, &key("k"));
	press(&mut finish, &key("right"));
	assert_eq!(finish.typed, "");
	assert!(has(&rows(&mut finish, 60, 18), "Type KQX for PR"));
	for c in ["k", "q"] {
		press(&mut finish, &key(c));
	}
	assert_eq!(press(&mut finish, &key("x")), coded_choice("PR", "KQX"));
	let mut cancel = ask_state(finish.view.clone());
	press(&mut cancel, &key("left"));
	assert_eq!(press(&mut cancel, &key("enter")), choice("Cancel"));
}

#[test]
fn new_thread_opens_on_the_brief_with_a_project_picked_type_to_find_the_caret_and_the_hint_follow_the_field() {
	// No project focused this run: the service's pick may be absent, and the form still opens on the brief.
	let mut form = ask_state(new_thread_question(&["acme-api", "acme-web", "client-a"], None));
	assert_eq!(form.field, Field::Brief);
	assert_eq!(press(&mut form, &key("return")), None);
	assert_eq!(form.note.as_deref(), Some("Write the brief first."));
	for c in "Fix it".chars() {
		let name = if c == ' ' { "space".to_string() } else { c.to_lowercase().to_string() };
		press(&mut form, &typed(&name, &c.to_string()));
	}
	let mut shown = rows(&mut form, 60, 18);
	assert!(has(&shown, "Brief    Fix it▏"));
	assert_eq!(shown.last().unwrap(), "Tab project · Enter start · Esc cancel");
	press(&mut form, &key("tab"));
	shown = rows(&mut form, 60, 18);
	assert!(has(&shown, "Brief    Fix it"));
	assert_eq!(shown.last().unwrap(), "Type to find · ↑↓ project · Enter brief · Esc cancel");
	// A letter jumps to the next project starting with it; the same letter again goes on to the next such one.
	press(&mut form, &key("c"));
	assert_eq!(form.project, 2);
	press(&mut form, &key("a"));
	assert_eq!(form.project, 0);
	press(&mut form, &key("a"));
	assert_eq!(form.project, 1);
	press(&mut form, &key("up"));
	press(&mut form, &key("tab"));
	assert_eq!(
		press(&mut form, &key("enter")),
		Some(PopupAnswer {
			choice: "Start".to_string(),
			typed: None,
			project: Some("acme-api".to_string()),
			brief: Some("Fix it".to_string())
		})
	);
	// A paste goes into the brief as one line.
	paste(&mut form, "and\nthe test");
	assert_eq!(form.brief, "Fix itand the test");
}

#[test]
fn a_10_line_brief_at_84x18_keeps_the_buttons_the_code_its_echo_and_the_hint_on_screen_and_pgdn_reads_on() {
	let brief: Vec<String> = (1..=10).map(|i| format!("Line {i} of the boss's brief for the export client.")).collect();
	let view = allow_question(
		"Start the thread \"Export retry backoff\" in acme-api?",
		&format!(
			"The last project pane you focused is in client-a, not acme-api.\n\nThe boss's brief:\n{}",
			brief.join("\n")
		),
		"KQX",
	);
	let mut state = ask_state(view.clone());
	// The popup is 84 columns; `main.rs` keeps one column clear at each side.
	let mut shown = rows(&mut state, 82, 18);
	assert_eq!(shown.len(), 18);
	assert_eq!(shown[0], "Jocasta: Allow or Deny");
	assert!(has(&shown, " Allow    Deny"));
	assert!(has(&shown, "Type KQX for Allow"));
	assert!(has(&shown, "> ___"));
	assert_eq!(shown.last().unwrap(), "←→ choose · PgUp/PgDn read · Esc Deny");
	let more = shown.iter().find(|l| l.starts_with("… ")).expect("a more line");
	let digits =
		more.strip_prefix("… ").and_then(|r| r.strip_suffix(" more lines · PgDn reads on")).expect("its shape");
	assert!(!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()));
	assert!(has(&shown, "Line 1 of the boss's brief for the export client."));
	assert!(!has(&shown, "Line 10 of the boss's brief for the export client."));
	// PgDn until the end: the last line shows and the "more" line goes.
	for _ in 0..5 {
		press(&mut state, &key("pagedown"));
	}
	shown = rows(&mut state, 82, 18);
	assert_eq!(shown.len(), 18);
	assert!(has(&shown, "Line 10 of the boss's brief for the export client."));
	assert!(!shown.iter().any(|l| l.starts_with("… ")));
	assert!(has(&shown, "> ___"));
	press(&mut state, &key("pageup"));
	assert!(rows(&mut state, 82, 18).iter().any(|l| l.starts_with("… ")));
	// A note shows above the hint without pushing the code off.
	state.note = Some(wrong_note(Some(1), "Deny"));
	shown = rows(&mut state, 82, 18);
	assert_eq!(shown.len(), 18);
	assert!(has(&shown, "Wrong code; one try left, then it counts as Deny."));
	assert!(has(&shown, "> ___"));
	// Every size from tiny to wide keeps the code's echo within the rows.
	for height in [12, 14, 16, 18] {
		for width in [40, 60, 82, 140] {
			let mut fresh = ask_state(view.clone());
			let lines = ask_lines(&mut fresh, width, height);
			assert!(lines.len() <= height);
			assert!(has(
				&lines.iter().map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>()).collect::<Vec<_>>(),
				"> ___"
			));
			assert!(lines.iter().all(|l| line_columns(l) <= width));
		}
	}
}

#[test]
fn typed_code_notes_a_letter_that_isnt_latin_says_to_switch_the_layout_one_try_left_the_third_wrong_code() {
	let mut state = ask_state(allow_question("Merge acme-api/retry into main?", "", "KQX"));
	assert_eq!(press(&mut state, &typed("к", "к")), None);
	assert_eq!(state.typed, "");
	assert_eq!(state.note.as_deref(), Some("Type the Latin letters KQX (switch the keyboard layout)."));
	assert_eq!(latin_note("KQX"), state.note.clone().unwrap());
	// A Latin letter clears it.
	press(&mut state, &key("k"));
	assert_eq!(state.note, None);
	assert_eq!(wrong_note(Some(2), "Deny"), "Wrong code; nothing was done.");
	assert_eq!(wrong_note(Some(1), "Deny"), "Wrong code; one try left, then it counts as Deny.");
	assert_eq!(exhausted_note("Deny"), "Wrong code three times; Deny was taken.");
}

#[test]
fn crossterm_keys_become_the_names_the_popup_reads() {
	let of = |code, modifiers| Key::from(KeyEvent::new(code, modifiers));
	assert_eq!(of(KeyCode::Enter, KeyModifiers::NONE).name, "return");
	assert_eq!(of(KeyCode::Esc, KeyModifiers::NONE).name, "escape");
	assert_eq!(of(KeyCode::Char(' '), KeyModifiers::NONE), Key::new("space", " "));
	assert_eq!(of(KeyCode::Char('K'), KeyModifiers::SHIFT), Key { shift: true, ..Key::new("k", "K") });
	let back = of(KeyCode::BackTab, KeyModifiers::SHIFT);
	assert_eq!((back.name.as_str(), back.shift), ("tab", true));
	assert_eq!(of(KeyCode::PageDown, KeyModifiers::NONE).name, "pagedown");
	// Ctrl+C answers with the fallback, as Esc does; Ctrl and Alt letters type nothing.
	let mut state = ask_state(allow_question("Merge?", "", "KQX"));
	assert_eq!(press(&mut state, &of(KeyCode::Char('c'), KeyModifiers::CONTROL)), choice("Deny"));
	assert_eq!(press(&mut state, &of(KeyCode::Char('k'), KeyModifiers::ALT)), None);
	assert_eq!(state.typed, "");
}

#[test]
fn the_buttons_are_drawn_with_the_picked_one_in_blue_and_the_form_marks_its_field() {
	let mut state = ask_state(allow_question("Merge?", "", "KQX"));
	let lines = ask_lines(&mut state, 40, 12);
	let drawn = draw_rows(&lines, 40, 12);
	let y = drawn.iter().position(|r| r == " Allow    Deny").expect("the buttons") as u16;
	let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 40, 12));
	let area = buf.area;
	crate::frontier::tui::render_lines(&mut buf, area, &lines);
	assert_eq!(buf[(1, y)].bg, TOKYO_NIGHT.blue);
	assert_eq!(buf[(1, y)].fg, TOKYO_NIGHT.bg0);
	assert_eq!(buf[(10, y)].bg, TOKYO_NIGHT.bg2);
	assert_eq!(buf[(1, 0)].fg, TOKYO_NIGHT.blue);
}
