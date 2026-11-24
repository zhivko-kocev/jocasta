use std::cell::Cell;

use super::*;

fn args(text: &str) -> Vec<String> {
	vec![text.to_owned()]
}

#[tokio::test]
async fn it_prints_its_text_and_the_wake_line_then_asks_for_a_wake_on_each_key_and_ends_with_its_input() {
	let mut out = Vec::new();
	let keys = Cell::new(2);
	let asked = Cell::new(0);
	let code = run_pane_sleep(
		&args("acme · asleep"),
		&mut out,
		|| {
			let left = keys.get();
			keys.set(left - 1);
			Ok(left > 0)
		},
		|| {
			asked.set(asked.get() + 1);
			async { Ok(()) }
		},
	)
	.await;
	let shown = String::from_utf8(out).unwrap();
	assert_eq!(code, 0);
	assert_eq!(asked.get(), 2);
	assert!(shown.starts_with("\r\n  acme · asleep\r\n  press any key to wake\r\n"), "{shown:?}");
}

#[tokio::test]
async fn a_wake_the_service_refuses_is_said_and_the_sleeper_goes_on_waiting() {
	let mut out = Vec::new();
	let keys = Cell::new(1);
	run_pane_sleep(
		&args("acme"),
		&mut out,
		|| {
			let left = keys.get();
			keys.set(left - 1);
			Ok(left > 0)
		},
		|| async { Err("Jocasta isn't running.".to_owned()) },
	)
	.await;
	let shown = String::from_utf8(out).unwrap();
	assert!(shown.contains("Jocasta isn't running."), "{shown:?}");
	assert!(shown.ends_with("press any key to wake\r\n"), "{shown:?}");
}

#[test]
fn control_characters_in_its_text_never_reach_the_terminal() {
	assert_eq!(screen("a\u{1b}[31mb\u{7}"), "\r\n  a[31mb\r\n  press any key to wake\r\n");
}
