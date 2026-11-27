//! `first_run`'s tests: each fact's text, and omp's login read from `omp usage`.

use super::*;

fn fine() -> Facts {
	Facts {
		omp: true,
		signed_in: Some(true),
		git: Some("git version 2.54.0\n".to_owned()),
		notify_send: Some(true),
		xdg_open: Some(true),
		keys: Some(true),
	}
}

#[test]
fn a_machine_with_everything_has_no_problem_and_skipped_checks_say_nothing() {
	assert!(problems(&fine()).is_empty());
	let windows = Facts { notify_send: None, xdg_open: None, keys: None, ..fine() };
	assert!(problems(&windows).is_empty());
}

#[test]
fn each_missing_piece_is_said_with_its_fix_in_the_checks_order() {
	let bare = Facts {
		omp: false,
		signed_in: None,
		git: Some("git version 2.43.0".to_owned()),
		notify_send: Some(false),
		xdg_open: Some(false),
		keys: Some(false),
	};
	assert_eq!(
		problems(&bare),
		[
			"omp isn't on PATH: install omp, run omp, then /login",
			"git 2.43 is older than 2.54: install git 2.54 or newer, or pushes to main, master and develop go unasked",
			"notify-send isn't installed: install libnotify, or Jocasta's notifications stay off",
			"xdg-open isn't installed: install xdg-utils, or links and screenshots won't open",
			"this terminal doesn't send Alt and Shift keys Jocasta needs; use foot, kitty, WezTerm or Ghostty",
		]
	);
	let unsigned = Facts { signed_in: Some(false), git: None, ..fine() };
	assert_eq!(
		problems(&unsigned),
		[
			"omp isn't signed in: type /login in the boss pane",
			"git isn't installed: install git 2.54 or newer, or pushes to main, master and develop go unasked",
		]
	);
}

#[test]
fn only_a_failed_usage_that_asks_for_a_login_means_not_signed_in() {
	let said = "No credentials found. Run `omp` and use /login to add accounts.";
	assert!(asks_for_login(Some(1), said));
	assert!(!asks_for_login(Some(0), said));
	assert!(!asks_for_login(Some(1), "fetch failed: network unreachable"));
	assert!(!asks_for_login(None, ""));
}
