//! Ported from the H4 cases of `test/bench/scripts.test.ts`.

use std::fs;

use jocasta::shared::testing::temp_dir;

use super::*;

/// The TypeScript's counter: `${++n}` padded to 12 digits.
fn counter() -> impl FnMut(usize) -> String {
	let mut n = 0;
	move |_| {
		n += 1;
		format!("{n:0>12}")
	}
}

fn result(leaked_tokens: &[&str], is_error: bool) -> QueryResult {
	QueryResult {
		query: "q".to_owned(),
		leaked: leaked_tokens.iter().map(|token| (*token).to_owned()).collect(),
		is_error,
	}
}

fn repeat(count: usize, make: impl Fn() -> QueryResult) -> Vec<QueryResult> {
	(0..count).map(|_| make()).collect()
}

#[test]
fn canaries_queries_and_leak_checks() {
	let canaries = make_canaries(3, &mut counter());
	assert_eq!(
		canaries[0],
		Canary {
			token: "CANARY-000000000001".to_owned(),
			topic: "build server".to_owned(),
			fact: "In the acme project the build server codename is CANARY-000000000001.".to_owned(),
		}
	);
	let queries = make_queries(&canaries, 20);
	assert_eq!(queries.len(), 20);
	assert!(queries.contains(&"CANARY-000000000002".to_owned()));
	assert_eq!(leaked("found CANARY-000000000003 here", &canaries), vec!["CANARY-000000000003"]);
	assert!(leaked("nothing", &canaries).is_empty());
}

#[test]
fn the_h4_plant_threads_brief_names_each_canary_as_a_decision_for_report_result_0576() {
	let canaries = make_canaries(2, &mut |_| "000000000001".to_owned());
	let brief = plant_brief(&canaries);
	assert!(brief.contains("report_result tool exactly once"));
	assert!(brief.contains(
		r#"[{"question":"build server codename","choice":"CANARY-000000000001","why":"H4 canary"},{"question":"release train codename","choice":"CANARY-000000000001","why":"H4 canary"}]"#
	));
}

#[test]
fn only_answered_queries_count_and_no_bound_without_a_positive_control_0576() {
	// The search tool down: every query errored, nothing was checked, and nothing bounds a leak rate.
	let down = score_h4(&[result(&[], true), result(&[], true)], &[result(&[], true)], 50);
	assert_eq!((down.checks, down.errored_queries, down.inconclusive, down.upper_bound95), (0, 2, true, None));
	// B leaked nothing, but A's own pane found no canary either: inconclusive.
	let blind = score_h4(&[result(&[], false), result(&[], false)], &[result(&[], false), result(&[], false)], 50);
	assert_eq!((blind.checks, blind.search_leaks, blind.inconclusive, blind.upper_bound95), (100, 0, true, None));
	assert_eq!(
		h4_line(&blind),
		"0 leaks in 100 answered checks (0 queries errored); positive control found no canary: inconclusive"
	);
	// A found canaries; B's 98 answered queries leaked none: the bound is over answered checks only.
	let mut from_b = repeat(98, || result(&[], false));
	from_b.extend([result(&[], true), result(&[], true)]);
	let mut from_a = repeat(92, || result(&["CANARY-1"], false));
	from_a.extend(repeat(8, || result(&[], false)));
	let clean = score_h4(&from_b, &from_a, 50);
	assert_eq!((clean.checks, clean.inconclusive, clean.upper_bound95), (4_900, false, Some(3.0 / 4_900.0)));
	assert_eq!(
		h4_line(&clean),
		"0 leaks in 4,900 answered checks (2 queries errored); positive control found canaries in 92/100 queries; 95% upper bound 3/4,900"
	);
}

#[test]
fn a_bank_leak_is_found_in_mnemopi_db_or_its_write_ahead_log() {
	let dir = temp_dir("jocasta-bench");
	let canaries = make_canaries(2, &mut counter());
	assert!(bank_leaks(dir.path(), &canaries).is_empty());
	fs::write(dir.path().join("mnemopi.db"), b"SQLite format 3\0nothing here").unwrap();
	fs::write(dir.path().join("mnemopi.db-wal"), format!("frames\0{}\0", canaries[1].token)).unwrap();
	assert_eq!(bank_leaks(dir.path(), &canaries), vec!["CANARY-000000000002"]);
}
