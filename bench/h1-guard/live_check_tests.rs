//! `test/bench/h1-guard.test.ts`'s cases for the live check, case for case.

use std::collections::HashSet;

use serde_json::json;

use super::*;
use jocasta::shared::yaml::parse_value;

#[test]
fn the_live_check_seeded_distinct_picks_the_acp_overlay_and_command_line() {
	let picks = pick_sample(1285, 30, 482);
	assert_eq!(picks.iter().collect::<HashSet<_>>().len(), 30);
	assert_eq!(pick_sample(1285, 30, 482), picks);
	assert_eq!(pick_sample(3, 30, DEFAULT_SEED).len(), 3);
	assert_eq!(
		parse_value(LIVE_OVERLAY).unwrap(),
		json!({ "tools": { "approvalMode": "yolo" }, "launch": { "enabled": false }, "async": { "enabled": false } })
	);
	let argv = omp_argv(&OmpArgv {
		omp_bin: "omp",
		model: "m",
		guard: "g.yml",
		overlay: "o.yml",
		extension: "r.js",
		session_dir: "s",
		max_cost: "0.05",
	});
	let shown = display_argv(&argv);
	assert!(shown.contains("omp --mode acp --session-dir s --model m --tools bash --config g.yml --config o.yml"));
	assert!(shown.contains("--no-extensions -e r.js"));
}

#[test]
fn the_recorder_refuses_an_omp_or_settings_it_can_t_record() {
	let ready = RecorderState {
		version: "18.20.2".into(),
		client_terminal_route: true,
		permission_kinds: ["allow_once", "allow_always", "reject_once", "reject_always"].map(str::to_owned).to_vec(),
		approval_mode_configured: true,
		approval_mode: "yolo".into(),
		interceptor_enabled: false,
		launch_enabled: false,
		async_enabled: false,
		patterns: 258,
	};
	assert_eq!(recorder_problems(&ready), Vec::<String>::new());
	let no_route = RecorderState { client_terminal_route: false, permission_kinds: vec![], ..ready.clone() };
	assert_eq!(
		recorder_problems(&no_route),
		[
			"omp 18.20.2's bash tool has no ACP client-terminal route, so the recorder can't see what it runs",
			"omp 18.20.2's ACP permission gate offers no reject_once, so a prompt can't be declined once",
		]
	);
	// The schema's implicit yolo makes ACP ask for every bash call: only an explicit one keeps a pane's gate.
	assert_eq!(
		recorder_problems(&RecorderState { approval_mode_configured: false, ..ready.clone() }),
		["tools.approvalMode isn't set to yolo explicitly (ACP would ask for every bash call)"]
	);
	assert_eq!(recorder_problems(&RecorderState { interceptor_enabled: true, async_enabled: true, ..ready }).len(), 2);
}

#[test]
fn a_live_pick_s_prompt_shows_the_command_as_written_backslashes_and_quotes_unescaped_on_a_line_of_its_own() {
	// Shown JSON-escaped, the model copied the escapes into its call and the pick no longer compared.
	for command in ["pwsh -c \"git tag v1.0.0\"", "cd C:\\work\\demo && touch .gitkeep", "docker  compose  down  -v"] {
		assert!(live_prompt(command).split('\n').any(|line| line == command), "{command}");
	}
}
