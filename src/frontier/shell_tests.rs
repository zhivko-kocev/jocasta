//! `shell.rs`'s tests: the dock and the question box as herdr's client drives them.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::*;
use crate::frontier::protocol::{Dashboard, Headline, Leftover, NewsList, PopupView};
use crate::frontier::today::view::tests::{dashboard, now as today_now, today};
use crate::frontier::tui::tests::buffer_rows;

/// The tests' clock: the today fixtures' "now", moved by `later`.
static CLOCK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn clock() -> i64 {
	today_now() + CLOCK.load(std::sync::atomic::Ordering::SeqCst)
}

fn shell() -> JocastaShell {
	JocastaShell::new(clock)
}

fn key(code: KeyCode) -> KeyEvent {
	KeyEvent::new(code, KeyModifiers::NONE)
}

fn char_key(c: char) -> KeyEvent {
	key(KeyCode::Char(c))
}

fn message<T: serde::Serialize>(shell: &mut JocastaShell, kind: &str, data: &T) -> Reply {
	shell.message(kind, &serde_json::to_string(data).unwrap())
}

/// What a reply sends, as (kind, data).
fn sends(reply: &Reply) -> Vec<(String, Value)> {
	reply.send.iter().map(|(kind, data)| (kind.clone(), serde_json::from_str(data).unwrap())).collect()
}

fn drawn(shell: &mut JocastaShell, width: u16, height: u16, focused: bool) -> Vec<String> {
	let area = Rect::new(0, 0, width, height);
	let mut buffer = Buffer::empty(area);
	shell.draw_dock(&mut buffer, area, focused);
	buffer_rows(&buffer)
}

fn with_today() -> JocastaShell {
	let mut shell = shell();
	message(&mut shell, SHELL_BOSS, &"w1");
	message(&mut shell, SHELL_FEED, &ServerMessage::Today { today: today() });
	shell
}

#[test]
fn the_dock_shows_in_the_boss_workspace_only_and_draws_the_feed_beside_its_bar() {
	let mut shell = shell();
	assert!(!shell.docks("w1"), "no boss workspace known yet");
	assert!(message(&mut shell, SHELL_BOSS, &"w1").redraw);
	assert!(!message(&mut shell, SHELL_BOSS, &"w1").redraw, "the same workspace again changes nothing");
	assert!(shell.docks("w1"));
	assert!(!shell.docks("w2"));
	assert!(message(&mut shell, SHELL_FEED, &ServerMessage::Today { today: today() }).redraw);
	let rows = drawn(&mut shell, 60, 30, false);
	assert!(rows.iter().all(|row| row.starts_with('▎')), "the bar runs down the dock's edge");
	assert!(rows.iter().any(|row| row.contains("Export v2")), "{rows:#?}");
	// A notification is herdr's to raise, not the dock's to draw.
	let toast = json!({"type": "toast", "reason": "failed", "level": "warning", "text": "x"});
	assert!(!shell.message(SHELL_FEED, &toast.to_string()).redraw);
}

#[test]
fn the_docks_keys_pick_rows_go_to_a_thread_act_on_it_and_give_the_rest_back() {
	let mut shell = with_today();
	drawn(&mut shell, 100, 34, true);
	// The first thread is picked at first; Enter goes to its pane.
	assert_eq!(
		sends(&shell.dock_key(key(KeyCode::Enter))),
		[(SHELL_FEED.to_owned(), json!({"type": "pane.focus", "paneId": "w2:p1"}))]
	);
	// Down picks the review link: Enter opens it.
	assert!(shell.dock_key(char_key('j')).redraw);
	drawn(&mut shell, 100, 34, true);
	assert_eq!(
		sends(&shell.dock_key(key(KeyCode::Enter))),
		[(SHELL_FEED.to_owned(), json!({"type": "link.open", "url": "https://example.org/web/pull/12"}))]
	);
	// A thread key on a link row has no thread: said in the dock, nothing sent; New thread needs none.
	let refused = shell.dock_key(char_key('f'));
	assert!(refused.send.is_empty());
	assert_eq!(
		shell.today.said.as_ref().map(|said| (said.text.as_str(), said.is_error)),
		Some(("No thread to act on.", true))
	);
	assert_eq!(sends(&shell.dock_key(char_key('n'))), [(SHELL_ACTION.to_owned(), json!({"action": "new-thread"}))]);
	shell.dock_key(char_key('k'));
	assert_eq!(
		sends(&shell.dock_key(char_key('s'))),
		[(SHELL_ACTION.to_owned(), json!({"action": "stop", "paneId": "w2:p1"}))]
	);
	// Esc gives the keys back; any other key goes on to herdr.
	let released = shell.dock_key(key(KeyCode::Esc));
	assert!(released.release && !released.pass);
	assert!(shell.dock_key(char_key('q')).pass);
	assert!(shell.dock_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)).pass);
}

#[test]
fn uppercase_x_stops_every_turn_with_no_thread_picked_and_lowercase_x_stays_remove_0686() {
	let mut shell = with_today();
	drawn(&mut shell, 100, 34, false);
	let shifted = KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT);
	assert_eq!(sends(&shell.dock_key(shifted)), [(SHELL_ACTION.to_owned(), json!({"action": "stop-all"}))]);
	assert_eq!(sends(&shell.dock_key(char_key('X'))), [(SHELL_ACTION.to_owned(), json!({"action": "stop-all"}))]);
	let lower = shell.dock_key(char_key('x'));
	assert!(!sends(&lower).iter().any(|(_, data)| data["action"] == "stop-all"));
}

#[test]
fn a_click_on_a_row_picks_it_and_does_what_enter_does_and_one_elsewhere_does_nothing() {
	let mut shell = with_today();
	drawn(&mut shell, 100, 34, false);
	let line_of = |shell: &JocastaShell, key: &str| {
		let (at, _) = shell.rows.iter().find(|(_, row_key)| row_key == key).expect("the row is drawn");
		u16::try_from(*at).unwrap()
	};
	// A link row opens its url (0581's service-side checks stand behind `link.open`).
	let link = shell.dock_mouse(5, line_of(&shell, "https://example.org/web/pull/12"));
	assert!(link.redraw);
	assert_eq!(
		sends(&link),
		[(SHELL_FEED.to_owned(), json!({"type": "link.open", "url": "https://example.org/web/pull/12"}))]
	);
	assert_eq!(shell.today.selected.as_deref(), Some("https://example.org/web/pull/12"));
	// A thread row focuses its pane.
	drawn(&mut shell, 100, 34, false);
	let thread = shell.dock_mouse(5, line_of(&shell, "w2:p1"));
	assert_eq!(sends(&thread), [(SHELL_FEED.to_owned(), json!({"type": "pane.focus", "paneId": "w2:p1"}))]);
	// The header's row, and a click left of the dock's text, act on nothing.
	let header = shell.dock_mouse(5, 0);
	assert!(header.send.is_empty() && !header.redraw);
	assert!(shell.dock_mouse(0, line_of(&shell, "w2:p1")).send.is_empty());
}

const WORKTREE: &str = "D:/work/acme-api-wt";

/// The dock with today's rows, eight headlines (Story 7 the newest) and one leftover worktree.
fn loaded() -> JocastaShell {
	let mut shell = with_today();
	let items = (0..8)
		.map(|i| Headline {
			title: format!("Story {i}"),
			source: "hn".to_owned(),
			url: format!("https://example.org/s{i}"),
			at: format!("2026-11-09T10:0{i}:00Z"),
		})
		.collect();
	let dashboard = Dashboard {
		news: vec![NewsList { heading: "Hacker News".to_owned(), items }],
		leftovers: vec![Leftover {
			project: "acme-api".to_owned(),
			folder: WORKTREE.into(),
			branch: None,
			ahead: 0,
			dirty: 1,
		}],
		..dashboard()
	};
	message(&mut shell, SHELL_FEED, &ServerMessage::Dashboard { dashboard });
	drawn(&mut shell, 100, 40, true);
	shell
}

#[test]
fn x_on_a_picked_leftover_asks_the_service_to_remove_it_and_on_anything_else_says_there_is_none() {
	let mut shell = loaded();
	shell.today.selected = Some(WORKTREE.to_owned());
	drawn(&mut shell, 100, 40, true);
	let removal = shell.dock_key(char_key('x'));
	assert!(removal.redraw && !removal.pass);
	assert_eq!(sends(&removal), [(SHELL_FEED.to_owned(), json!({"type": "leftover.remove", "folder": WORKTREE}))]);
	// Enter has nothing to open on a leftover.
	assert!(shell.dock_key(key(KeyCode::Enter)).send.is_empty());
	// A thread is not a leftover: nothing is sent, and the dock says so.
	shell.today.selected = Some("w2:p1".to_owned());
	drawn(&mut shell, 100, 40, true);
	let refused = shell.dock_key(char_key('x'));
	assert!(refused.send.is_empty() && !refused.pass);
	assert_eq!(
		shell.today.said.as_ref().map(|said| (said.text.as_str(), said.is_error)),
		Some(("No leftover to remove.", true))
	);
}

#[test]
fn up_and_down_move_through_the_headlines_and_scroll_the_box_at_its_edges() {
	let mut shell = loaded();
	let first = shell.news.as_ref().expect("the box is drawn").keys[0].clone();
	assert_eq!(first, "https://example.org/s7");
	shell.today.selected = Some(first);
	drawn(&mut shell, 100, 40, true);
	// Down through the five shown: the box stays; one more scrolls it by one, the pick on the new last line.
	for _ in 0..4 {
		shell.dock_key(char_key('j'));
		drawn(&mut shell, 100, 40, true);
	}
	assert_eq!((shell.today.news_top, shell.today.selected.as_deref()), (0, Some("https://example.org/s3")));
	assert!(shell.dock_key(char_key('j')).redraw);
	let rows = drawn(&mut shell, 100, 40, true);
	assert_eq!((shell.today.news_top, shell.today.selected.as_deref()), (1, Some("https://example.org/s2")));
	assert!(rows.iter().any(|row| row.contains("Story 2")) && !rows.iter().any(|row| row.contains("Story 7")));
	// Up from the first shown scrolls back the other way; at the very first headline up leaves the box.
	shell.today.selected = Some("https://example.org/s6".to_owned());
	drawn(&mut shell, 100, 40, true);
	shell.dock_key(char_key('k'));
	drawn(&mut shell, 100, 40, true);
	assert_eq!((shell.today.news_top, shell.today.selected.as_deref()), (0, Some("https://example.org/s7")));
	shell.dock_key(char_key('k'));
	assert_ne!(shell.today.selected.as_deref(), Some("https://example.org/s7"));
	assert_eq!(shell.today.news_top, 0);
	// Down at the last headline stays on it.
	shell.today.selected = Some("https://example.org/s0".to_owned());
	shell.today.news_top = 3;
	drawn(&mut shell, 100, 40, true);
	shell.dock_key(char_key('j'));
	assert_eq!(shell.today.selected.as_deref(), Some("https://example.org/s0"));
}

#[test]
fn the_wheel_over_the_news_box_scrolls_it_and_anywhere_else_does_nothing() {
	let mut shell = loaded();
	let lines = shell.news.as_ref().expect("the box is drawn").lines.clone();
	let row = |line: usize| u16::try_from(line).unwrap();
	// Over the box, one headline a step, held at both ends.
	for expected in [1, 2, 3, 3] {
		let before = shell.today.news_top;
		let reply = shell.dock_scroll(5, row(lines.start + 1), 1);
		assert_eq!(reply.redraw, expected != before, "from {before}");
		drawn(&mut shell, 100, 40, true);
		assert_eq!(shell.today.news_top, expected);
	}
	let up = shell.dock_scroll(5, row(lines.start), -1);
	assert!(up.redraw);
	assert_eq!(shell.today.news_top, 2);
	drawn(&mut shell, 100, 40, true);
	// Over the header, or left of the dock's text: nothing moves.
	assert!(!shell.dock_scroll(5, 0, -1).redraw);
	assert!(!shell.dock_scroll(0, row(lines.start), -1).redraw);
	assert_eq!(shell.today.news_top, 2);
	// A picked headline pushed out of view by the wheel moves to the box's edge.
	shell.today.news_top = 0;
	shell.today.selected = Some("https://example.org/s7".to_owned());
	drawn(&mut shell, 100, 40, true);
	shell.dock_scroll(5, row(lines.start), 1);
	assert_eq!(shell.today.selected.as_deref(), Some("https://example.org/s6"));
	drawn(&mut shell, 100, 40, true);
	assert!(shell.picked().is_some_and(|pick| pick.news));
}

/// The silence test's own clock, so the others' stays still.
static SILENCE_CLOCK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn silence_clock() -> i64 {
	today_now() + SILENCE_CLOCK.load(std::sync::atomic::Ordering::SeqCst)
}

#[test]
fn thirty_seconds_without_a_message_shows_jocasta_not_answering_dimmed_and_the_next_message_clears_it() {
	let mut shell = JocastaShell::new(silence_clock);
	message(&mut shell, SHELL_BOSS, &"w1");
	let at = |ms: i64| SILENCE_CLOCK.store(ms, std::sync::atomic::Ordering::SeqCst);
	at(0);
	message(&mut shell, SHELL_FEED, &ServerMessage::Today { today: today() });
	// Heartbeats keep it up and change nothing to draw.
	at(10_000);
	assert!(!message(&mut shell, SHELL_FEED, &ServerMessage::Alive).redraw);
	at(39_999);
	shell.tick();
	assert!(shell.today.up, "29.999 s after the last word it is still up");
	// 30 s after the last message: not answering since that message, picture kept, dimmed.
	at(40_000);
	assert!(shell.tick());
	assert!(!shell.today.up);
	let since = crate::shared::day::hhmm(today_now() + 10_000);
	assert_eq!(shell.status().as_deref(), Some(format!("Jocasta isn't answering (since {since})").as_str()));
	let rows = drawn(&mut shell, 100, 34, false);
	assert!(rows.iter().any(|row| row.contains("Jocasta isn't answering")), "{rows:#?}");
	assert!(rows.iter().any(|row| row.contains("Export v2")), "the last picture stays");
	let area = Rect::new(0, 0, 100, 34);
	let mut buffer = Buffer::empty(area);
	shell.draw_dock(&mut buffer, area, false);
	assert!(buffer.cell((5, 10)).unwrap().modifier.contains(ratatui::style::Modifier::DIM));
	// The next message, a heartbeat even, clears it.
	at(50_000);
	assert!(message(&mut shell, SHELL_FEED, &ServerMessage::Alive).redraw);
	assert!(shell.today.up);
	let rows = drawn(&mut shell, 100, 34, false);
	assert!(!rows.iter().any(|row| row.contains("Jocasta isn't answering")), "{rows:#?}");
	assert!(!shell.status().is_some_and(|text| text.contains("isn't answering")));
}

fn allow_view(code: &str) -> PopupView {
	PopupView {
		title: "Jocasta: Allow or Deny".to_owned(),
		message: "Remove the leftover worktree acme-api/retry?".to_owned(),
		detail: None,
		choices: vec!["Allow".to_owned(), "Deny".to_owned()],
		fallback: "Deny".to_owned(),
		code: Some(code.to_owned()),
		form: None,
	}
}

fn ask(shell: &mut JocastaShell, code: &str) {
	let question = ShellQuestion { code: "q1".to_owned(), view: allow_view(code) };
	assert!(message(shell, SHELL_QUESTION, &question).redraw);
}

fn type_code(shell: &mut JocastaShell, code: &str) -> Reply {
	let mut last = Reply::default();
	for c in code.chars() {
		last = shell.question_key(char_key(c));
	}
	last
}

fn answered(shell: &mut JocastaShell, reply: AnswerReply) -> Reply {
	message(shell, SHELL_ANSWERED, &ShellAnswered { code: "q1".to_owned(), reply })
}

#[test]
fn a_question_takes_his_keys_after_a_grace_and_sends_his_answer_once_the_services_reply_says_what_came_of_it() {
	CLOCK.store(0, std::sync::atomic::Ordering::SeqCst);
	let mut shell = shell();
	ask(&mut shell, "ABC");
	assert!(shell.asking());
	// Typing meant for a pane, in the first 750 ms, spends nothing (0578).
	assert!(type_code(&mut shell, "abc").send.is_empty());
	CLOCK.store(KEY_GRACE_MS, std::sync::atomic::Ordering::SeqCst);
	assert_eq!(
		sends(&type_code(&mut shell, "xyz")),
		[(SHELL_ANSWER.to_owned(), json!({"code": "q1", "answer": {"choice": "Allow", "typed": "XYZ"}}))]
	);
	// While that answer is out, Esc waits for its reply; a wrong code sends it then.
	assert!(shell.question_key(key(KeyCode::Esc)).send.is_empty());
	assert_eq!(
		sends(&answered(&mut shell, AnswerReply::Wrong { left: Some(1) })),
		[(SHELL_ANSWER.to_owned(), json!({"code": "q1", "answer": {"choice": "Deny"}}))]
	);
	let note = shell.question.as_ref().and_then(|asking| asking.state.note.clone()).unwrap();
	assert!(note.contains("one try left"), "{note}");
	assert!(answered(&mut shell, AnswerReply::Taken).redraw);
	assert!(!shell.asking());
}

#[test]
fn the_third_wrong_code_shows_its_note_for_two_seconds_and_the_box_draws_over_the_middle() {
	CLOCK.store(KEY_GRACE_MS, std::sync::atomic::Ordering::SeqCst);
	let mut shell = shell();
	ask(&mut shell, "ABC");
	CLOCK.store(KEY_GRACE_MS * 2, std::sync::atomic::Ordering::SeqCst);
	type_code(&mut shell, "xyz");
	answered(&mut shell, AnswerReply::Exhausted);
	// The service's gone comes with it; the note stays its 2 s, and keys do nothing meanwhile.
	assert!(!shell.message(SHELL_GONE, "\"q1\"").redraw);
	assert!(shell.question_key(key(KeyCode::Esc)).send.is_empty());
	let area = Rect::new(0, 0, 100, 30);
	let mut buffer = Buffer::empty(area);
	let outer = shell.draw_question(&mut buffer, area);
	assert_eq!((outer.width, outer.x), (70, 15));
	let rows = buffer_rows(&buffer);
	assert!(rows.iter().any(|row| row.contains("Jocasta asks")));
	assert!(rows.iter().any(|row| row.contains("Remove the leftover worktree")));
	// At most 88 columns wide and at least half the window high (0717): 80×30, a wide window, a tall one.
	for (width, height, outer_width, outer_height) in [(80, 30, 56, 20), (200, 30, 88, 20), (200, 60, 88, 30)] {
		let area = Rect::new(0, 0, width, height);
		let outer = shell.draw_question(&mut Buffer::empty(area), area);
		assert_eq!((outer.width, outer.height), (outer_width, outer_height), "{width}×{height}");
	}
	CLOCK.store(KEY_GRACE_MS * 2 + EXHAUSTED_MS, std::sync::atomic::Ordering::SeqCst);
	assert!(shell.tick());
	assert!(!shell.asking());
	// A question answered elsewhere goes at once.
	ask(&mut shell, "ABC");
	assert!(shell.message(SHELL_GONE, "\"q1\"").redraw);
	assert!(!shell.asking());
}

#[test]
fn uppercase_r_starts_the_picked_review_request_and_uppercase_f_needs_a_red_pipeline_0680() {
	let mut shell = with_today();
	drawn(&mut shell, 100, 34, true);
	shell.dock_key(char_key('j'));
	drawn(&mut shell, 100, 34, true);
	let review = shell.dock_key(char_key('R'));
	assert!(review.redraw && !review.pass);
	assert_eq!(
		sends(&review),
		[(SHELL_FEED.to_owned(), json!({"type": "pr.review", "url": "https://example.org/web/pull/12"}))]
	);
	// The picked row is a review request, not a red pipeline: nothing is sent and the dock says what to pick.
	let refused = shell.dock_key(char_key('F'));
	assert!(refused.send.is_empty() && !refused.pass);
	assert_eq!(
		shell.today.said.as_ref().map(|said| (said.text.as_str(), said.is_error)),
		Some(("Pick a red pipeline first.", true))
	);
	// Lowercase `r` and `f` stay Review and Finish on a thread.
	assert!(!sends(&shell.dock_key(char_key('r'))).iter().any(|(_, data)| data["type"] == "pr.review"));
}

#[test]
fn lowercase_b_asks_the_service_for_the_morning_brief_and_says_so_0680() {
	let mut shell = with_today();
	drawn(&mut shell, 100, 34, true);
	let key = shell.dock_key(char_key('b'));
	assert!(key.redraw && !key.pass);
	assert_eq!(sends(&key), [(SHELL_FEED.to_owned(), json!({"type": "brief.morning"}))]);
	assert_eq!(
		shell.today.said.as_ref().map(|said| (said.text.as_str(), said.is_error)),
		Some(("Building the morning brief…", false))
	);
	// The service's answer is said the way every action's is.
	let opened = ServerMessage::Action { text: "Morning brief opened.".to_owned(), is_error: false };
	shell.message(SHELL_FEED, &serde_json::to_string(&opened).unwrap());
	assert_eq!(shell.today.said.as_ref().map(|said| said.text.as_str()), Some("Morning brief opened."));
}
