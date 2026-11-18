//! Ported from the H7 cases of `test/bench/scripts.test.ts`; the real run with git only is
//! `tests/bench_scripts.rs`.

use super::*;

fn row(location: &str, length: u64, ok: bool) -> H7Row {
	H7Row { location: location.to_owned(), length, run: 0, ok, absolute_length: 0, error: None }
}

#[test]
fn the_deep_path_has_exactly_l_characters() {
	for length in 10..=260 {
		assert_eq!(deep_path(length).unwrap().len(), length);
	}
	let refusal = deep_path(9).unwrap_err().to_string();
	assert!(refusal.contains("below 10"), "{refusal}");
}

#[test]
fn locations_and_the_largest_passing_l() {
	assert_eq!(location_path("<parent>/<name>.wt/<slug>", "/src/acme-app", "/home", "h7-1"), "/src/acme-app.wt/h7-1");
	assert_eq!(
		location_path("<home>/.herdr/worktrees/<name>/<slug>", "/src/acme-app", "/home", "b"),
		"/home/.herdr/worktrees/acme-app/b"
	);
	assert_eq!(
		largest_passing(&[
			row("jocasta", 100, true),
			row("jocasta", 200, true),
			row("herdr", 100, true),
			row("herdr", 200, false),
			row("claude", 100, false),
		]),
		vec![("jocasta".to_owned(), Some(200)), ("herdr".to_owned(), Some(100)), ("claude".to_owned(), None),]
	);
}
