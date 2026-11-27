//! The board window without a display: its one-at-a-time lock, its facts line, how a new board patches the rows, and
//! (ignored, by hand) the board drawn offscreen into PNGs to look at.

use super::*;
use crate::frontier::protocol::{ChecksLoad, PrFacts};
use crate::frontier::view::app::tests::write_png;
use crate::shared::testing::temp_jocasta;

const NOW: i64 = 1_795_000_000_000;

fn thread(pane: &str, title: &str, phase: &str, line: &str) -> BoardRow {
	BoardRow {
		pane_id: pane.to_owned(),
		project: "acme-api".to_owned(),
		title: title.to_owned(),
		phase: phase.to_owned(),
		line: line.to_owned(),
		since: NOW - 25 * 60_000,
		cost: Some(1.2),
		budget: Some(5.0),
		context: Some(62_000),
		check: Some(true),
		pr: None,
	}
}

fn board() -> Board {
	let mut review = thread("2", "Review PR #7", "review", "reading src/billing/handlers.rs");
	review.pr = Some(PrFacts { number: 7, ci: PrCi::Failing });
	review.check = None;
	let mut stuck = thread("3", "Retry webhooks", "stuck", "stuck: quick failed 3×");
	stuck.cost = Some(5.1);
	stuck.check = Some(false);
	Board {
		rows: vec![
			thread("1", "Invoice totals ignore refunds", "working", "running cargo test -p billing"),
			review,
			stuck,
			thread("4", "Readme line", "awaiting you", "Which licence header?"),
		],
		queued: vec!["waiting: Release notes · after Invoice totals ignore refunds".to_owned()],
		checks: ChecksLoad { running: 1, waiting: 2 },
		spend_today: Some(12.4),
	}
}

/// A board window that has its first board and whose entrance has settled.
fn settled_watch(board: Board) -> Watch {
	let (mut watch, _) = Watch::open();
	let _ = watch.update(Message::Answer(Ok(board)));
	watch.at_ms = NOW;
	settle(&mut watch);
	watch
}

/// The entrance as if the first board came a second ago (the next answer's clock is the real one).
fn settle(watch: &mut Watch) {
	watch.first = watch.first.map(|first| first.checked_sub(Duration::from_secs(1)).unwrap());
}

#[test]
fn one_board_at_a_time_a_second_hold_gets_none_until_the_first_lets_go() {
	let j = temp_jocasta();
	let first = hold(j.root()).unwrap();
	assert!(first.is_some());
	assert!(hold(j.root()).unwrap().is_none(), "the board is already open");
	drop(first);
	assert!(hold(j.root()).unwrap().is_some());
}

#[test]
fn jocasta_board_starts_the_window_process_with_the_window_flag() {
	let args: Vec<String> = WINDOW.iter().map(|word| (*word).to_owned()).collect();
	assert_eq!(crate::shared::route::route(&args).unwrap(), crate::shared::route::Route::Board { window: true });
}

#[test]
fn a_rows_facts_say_its_age_cost_of_budget_context_check_and_its_prs_ci() {
	let rows = board().rows;
	assert_eq!(facts(&rows[0], NOW), "25 min · $1.20 of $5.00 · 62k context · check passed");
	assert_eq!(facts(&rows[1], NOW), "25 min · $1.20 of $5.00 · 62k context · PR #7 CI failing");
	let bare = BoardRow { cost: None, budget: None, context: None, check: None, ..rows[0].clone() };
	assert_eq!(facts(&bare, NOW), "25 min");
	assert_eq!(summary(&board()), "4 threads open · checks: 1 running, 2 waiting · today $12.40");
	let quiet =
		Board { rows: vec![], queued: vec![], checks: ChecksLoad { running: 0, waiting: 0 }, spend_today: None };
	assert_eq!(summary(&quiet), "0 threads open · no checks running");
}

#[test]
fn the_first_board_plays_the_entrance_once_and_a_board_that_changed_nothing_moves_nothing() {
	let (mut watch, _) = Watch::open();
	assert!(!watch.animating(), "nothing to show yet");
	let _ = watch.update(Message::Answer(Ok(board())));
	assert!(watch.animating(), "the entrance");
	assert_eq!(watch.presence("1", 0).0, 0.0, "rows start transparent");
	settle(&mut watch);
	assert!(!watch.animating());
	let _ = watch.update(Message::Answer(Ok(board())));
	assert!(!watch.animating(), "the same board again: no frames");
	assert!(watch.appeared.is_empty() && watch.lit.is_empty());
	assert_eq!(watch.presence("1", 0), (1.0, 0.0, 0.0));
}

#[test]
fn a_changed_phase_lights_its_row_a_new_row_fades_in_and_a_gone_row_goes() {
	let mut watch = settled_watch(board());
	let mut next = board();
	next.rows[0].phase = "review".to_owned();
	next.rows.remove(3);
	next.rows.push(thread("5", "Release notes", "working", ""));
	let _ = watch.update(Message::Answer(Ok(next)));
	assert!(watch.animating());
	assert_eq!(watch.lit.keys().collect::<Vec<_>>(), ["1"]);
	assert_eq!(watch.appeared.keys().collect::<Vec<_>>(), ["5"]);
	let (alpha, rise, light) = watch.presence("1", 0);
	assert_eq!((alpha, rise), (1.0, 0.0), "a changed row stays in place");
	assert!(light > 0.99, "lit at once");
	assert_eq!(watch.presence("5", 3).0, 0.0, "the new row starts transparent");
	assert!(watch.board.as_ref().unwrap().rows.iter().all(|row| row.pane_id != "4"));
	let settled_at = watch.now + Duration::from_millis(230);
	let _ = watch.update(Message::Tick(settled_at));
	assert!(!watch.animating(), "settled by 220 ms; no frames asked for");
	assert!(watch.lit.is_empty() && watch.appeared.is_empty());
}

#[test]
fn without_a_running_jocasta_it_says_so_keeps_asking_and_its_rows_come_back_without_the_entrance() {
	let mut watch = settled_watch(board());
	let _ = watch.update(Message::Answer(Err(NOT_RUNNING.to_owned())));
	assert!(watch.board.is_none() && watch.why.as_deref() == Some(NOT_RUNNING));
	let _ = watch.update(Message::Poll);
	assert!(watch.asking);
	let _ = watch.update(Message::Answer(Ok(board())));
	assert!(watch.why.is_none());
	assert_eq!(watch.appeared.len(), 4, "they fade in; the entrance played once already");
	// A poll while one is out asks nothing more.
	watch.asking = true;
	let _ = watch.update(Message::Poll);
	assert!(watch.asking);
}

/// Draws `watch` offscreen at `size` into `<temp>/jocasta-view-shots/<name>.png`.
fn shoot(watch: &Watch, name: &str, size: Size) {
	let path = std::env::temp_dir().join("jocasta-view-shots").join(format!("{name}.png"));
	let settings = iced::Settings { default_font: style::ui_font(), ..iced::Settings::default() };
	let mut simulator = iced_test::Simulator::with_size(settings, size, watch.view());
	write_png(&simulator.snapshot(&style::theme()).expect("it draws"), &path);
}

/// By hand (`cargo nextest run --run-ignored only board_shots`): the board as PNGs to look at.
#[test]
#[ignore = "writes PNGs for a person to look at"]
fn board_shots() {
	let _j = temp_jocasta();
	let size = Size::new(760.0, 640.0);
	shoot(&settled_watch(board()), "board", size);
	// Mid-entrance: 100 ms after the first board.
	let (mut entering, _) = Watch::open();
	let _ = entering.update(Message::Answer(Ok(board())));
	entering.at_ms = NOW;
	let at = entering.now + Duration::from_millis(100);
	let _ = entering.update(Message::Tick(at));
	shoot(&entering, "board-entrance-100ms", size);
	// A phase changed and a row came, 60 ms in.
	let mut changed = settled_watch(board());
	let mut next = board();
	next.rows[0].phase = "review".to_owned();
	next.rows.push(thread("5", "Release notes", "working", ""));
	let _ = changed.update(Message::Answer(Ok(next)));
	changed.at_ms = NOW;
	let at = changed.now + Duration::from_millis(60);
	let _ = changed.update(Message::Tick(at));
	shoot(&changed, "board-change-60ms", size);
	// break-ui: a narrow window with a long title, and the board with Jocasta stopped and with no threads.
	let mut long = board();
	long.rows[0].title =
		"Supercalifragilisticexpialidocious_reconciliation_of_refunded_invoice_lines_across_every_tenant".to_owned();
	shoot(&settled_watch(long), "board-narrow", Size::new(420.0, 640.0));
	let mut down = settled_watch(board());
	let _ = down.update(Message::Answer(Err(NOT_RUNNING.to_owned())));
	shoot(&down, "board-down", Size::new(760.0, 320.0));
	let empty =
		Board { rows: vec![], queued: vec![], checks: ChecksLoad { running: 0, waiting: 0 }, spend_today: Some(0.0) };
	shoot(&settled_watch(empty), "board-empty", Size::new(760.0, 320.0));
}
