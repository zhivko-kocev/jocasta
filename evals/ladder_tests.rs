use std::collections::HashMap;

use super::ladder::{
	MOST_ARMS, TRIAL_CAP_USD, Trial, best_share, charged, live_arms, rotated, standing, subsystem_winners,
};
use super::replay::ReplayRow;
use super::settings::Class;

/// A trial of `arm` in `round` on case `case`, rep `rep`, with its outcome: passed, cost, wall time.
fn trial(arm: &str, round: &str, case: usize, rep: u64, (passed, cost, wall_ms): (bool, f64, f64)) -> Trial {
	Trial {
		arm: arm.to_owned(),
		round: round.to_owned(),
		case: format!("case-{case:02}"),
		rep,
		passed,
		cost,
		tokens: 0.0,
		wall_ms,
		peak_tree_bytes: None,
		min_available_bytes: None,
		infra: None,
		row: None,
	}
}

/// The base on 12 cases, 3 reps each, every case passed, costs and times varying by case.
fn base() -> Vec<Trial> {
	(0..12)
		.flat_map(|case| {
			(0..3).map(move |rep| {
				trial("base", "base", case, rep, (true, 0.10 + case as f64 * 0.01, 60_000.0 + case as f64 * 1000.0))
			})
		})
		.collect()
}

#[test]
fn an_arm_cheaper_on_every_case_wins_and_one_that_fails_a_case_the_base_always_passes_is_out() {
	let mut trials = base();
	trials.extend((0..12).map(|case| trial("a=1", "r2", case, 0, (true, (0.10 + case as f64 * 0.01) * 0.7, 61_000.0))));
	trials.extend((0..12).map(|case| trial("b=1", "r2", case, 0, (case != 5, 0.01, 10_000.0))));
	let cheap = standing(&trials, "a=1", &["r2"], &["base"]);
	assert!(cheap.wins() && !cheap.out && cheap.cases == 12, "{cheap:?}");
	assert!((cheap.cost_ratio - 0.7).abs() < 1e-9);
	let failing = standing(&trials, "b=1", &["r2"], &["base"]);
	assert!(failing.out && !failing.wins());
}

#[test]
fn an_arm_no_cheaper_and_no_faster_does_not_win() {
	let mut trials = base();
	trials.extend((0..12).map(|case| {
		let wobble = if case % 2 == 0 { 1.05 } else { 0.95 };
		trial(
			"c=1",
			"r2",
			case,
			0,
			(true, (0.10 + case as f64 * 0.01) * wobble, (60_000.0 + case as f64 * 1000.0) * wobble),
		)
	}));
	assert!(!standing(&trials, "c=1", &["r2"], &["base"]).wins());
}

#[test]
fn the_best_share_rounds_up_and_leaves_out_arms_out() {
	let mut trials = base();
	for (arm, factor) in [("a=1", 0.5), ("b=1", 0.6), ("c=1", 0.7), ("d=1", 0.8)] {
		trials.extend((0..4).map(|case| trial(arm, "r1", case, 0, (true, 0.2 * factor, 60_000.0))));
	}
	trials.extend((0..4).map(|case| trial("e=1", "r1", case, 0, (false, 0.001, 1.0))));
	let standings: Vec<_> =
		["a=1", "b=1", "c=1", "d=1", "e=1"].iter().map(|arm| standing(&trials, arm, &["r1"], &["base"])).collect();
	assert_eq!(best_share(&standings, 3), vec!["a=1", "b=1"]);
	assert_eq!(best_share(&standings, 9), vec!["a=1"]);
}

#[test]
fn every_arm_takes_its_turn_first() {
	let arms = ["a", "b", "c"];
	assert_eq!(rotated(&arms, 0), ["a", "b", "c"]);
	assert_eq!(rotated(&arms, 4), ["b", "c", "a"]);
	assert!(rotated::<&str>(&[], 2).is_empty());
}

#[test]
fn one_winner_per_subsystem_and_never_a_quality_arm() {
	let mut trials = base();
	for (arm, factor) in [("tui.a=1", 0.5), ("tui.b=1", 0.6), ("edit.c=1", 0.8), ("advisor.d=1", 0.3)] {
		trials.extend(
			(0..12).map(|case| trial(arm, "r2", case, 0, (true, (0.10 + case as f64 * 0.01) * factor, 60_000.0))),
		);
	}
	let arms = ["tui.a=1", "tui.b=1", "edit.c=1", "advisor.d=1"];
	let standings: Vec<_> = arms.iter().map(|arm| standing(&trials, arm, &["r2"], &["base"])).collect();
	let classes: HashMap<String, Class> = arms
		.iter()
		.map(|arm| ((*arm).to_owned(), if arm.starts_with("advisor") { Class::Q } else { Class::C }))
		.collect();
	let winners: Vec<String> = subsystem_winners(&standings, &classes, 12).into_iter().map(|w| w.arm).collect();
	assert_eq!(winners, vec!["tui.a=1", "edit.c=1"]);
	// An arm that hasn't run every case is not a winner yet.
	assert!(subsystem_winners(&standings, &classes, 13).is_empty());
}

#[test]
fn a_trial_that_left_no_row_is_charged_omps_whole_cap() {
	let mut lost = trial("a=1", "r1", 0, 0, (false, 0.0, 0.0));
	lost.infra = Some("crash".to_owned());
	assert_eq!(charged(&lost), TRIAL_CAP_USD);
	assert_eq!(charged(&trial("a=1", "r1", 0, 0, (true, 0.12, 0.0))), 0.12);
}

fn replay_row(arm: &str, verdict: &str, request_bytes: u64) -> ReplayRow {
	ReplayRow {
		arm: arm.to_owned(),
		key: arm.split_once('=').map(|(key, _)| key.to_owned()),
		class: (verdict != "base").then_some(Class::C),
		verdict: verdict.to_owned(),
		because: Vec::new(),
		code: Some(0),
		requests: 10,
		request_bytes,
		wall_ms: 5000,
		peak_tree_bytes: None,
		signature: String::new(),
	}
}

#[test]
fn step_c_takes_the_largest_live_effects_by_the_current_classes_and_sets_the_rest_apart() {
	let mut rows = vec![replay_row("base", "base", 1000)];
	rows.extend((0..MOST_ARMS + 5).map(|i| replay_row(&format!("k{i}=1"), "live", 1000 + i as u64)));
	rows.push(replay_row("inert=1", "inert", 5000));
	// Live in the replay with the largest effect, but classed P since: never tried.
	rows.push(replay_row("gc.strip=true", "live", 9000));
	let mut classes: HashMap<String, Class> = (0..MOST_ARMS + 5).map(|i| (format!("k{i}"), Class::C)).collect();
	classes.insert("inert".to_owned(), Class::C);
	classes.insert("gc.strip".to_owned(), Class::P);
	let (chosen, rest) = live_arms(&rows, &classes);
	assert_eq!(chosen.len(), MOST_ARMS);
	assert_eq!(chosen[0].0, format!("k{}=1", MOST_ARMS + 4));
	assert_eq!(rest.len(), 5);
	assert!(rest.contains(&"k0=1".to_owned()));
	let tried: Vec<&String> = chosen.iter().map(|(arm, _)| arm).chain(&rest).collect();
	assert!(!tried.iter().any(|arm| *arm == "inert=1" || *arm == "gc.strip=true"));
}
