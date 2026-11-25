use serde_json::{Map, Value, json};

use super::{over_budget, ps_seconds, ratchet, split};
use crate::lib::proctree::ProcessInfo;

fn process(pid: u32, ppid: u32, name: &str) -> ProcessInfo {
	let name = name.to_owned();
	ProcessInfo { pid, ppid, name, command_line: String::new(), working_set: 0, private_bytes: None, created: None }
}

#[test]
fn the_count_is_jocastas_own_processes_without_the_agents_or_the_console_hosts_0689() {
	let tree = [
		process(1, 0, "jocasta.exe"),
		process(2, 1, "OpenConsole.exe"),
		process(3, 1, "omp.exe"),
		process(4, 3, "bun.exe"),
		process(5, 1, "conhost.exe"),
		process(6, 1, "git.exe"),
	];
	let (own, agents) = split(&tree);
	assert_eq!(own.iter().map(|p| p.pid).collect::<Vec<_>>(), [1, 6]);
	assert_eq!(agents.iter().map(|p| p.pid).collect::<Vec<_>>(), [3, 4]);
}

fn object(value: Value) -> Map<String, Value> {
	value.as_object().cloned().unwrap()
}

#[test]
fn only_a_steady_number_more_than_five_percent_over_a_set_budget_fails() {
	let budget = object(json!({"processes": 20.0, "idle_rss_mb": 100.0, "binary_mb": 40.0, "bridge_ms": null}));
	let at_limit = object(json!({"processes": 21.0, "idle_rss_mb": 105.0, "binary_mb": 42.0, "bridge_ms": 4.0}));
	assert!(over_budget(&at_limit, &budget).is_empty());
	let over = object(json!({"processes": 22.0, "idle_rss_mb": 105.5, "binary_mb": 42.5}));
	assert_eq!(
		over_budget(&over, &budget),
		vec![
			"footprint: processes 22 over budget 20".to_owned(),
			"footprint: idle_rss_mb 105.5 over budget 100".to_owned(),
			"footprint: binary_mb 42.5 over budget 40".to_owned(),
		]
	);
}

#[test]
fn a_noisy_number_may_go_over_by_the_larger_of_five_percent_and_its_absolute_slack() {
	let budget = object(json!({"startup_ms": 100.0, "idle_cpu_pct": 2.2, "bridge_ms": 1.5, "processes": 3.0}));
	let within = object(json!({"startup_ms": 125.0, "idle_cpu_pct": 3.2, "bridge_ms": 3.5}));
	assert!(over_budget(&within, &budget).is_empty());
	let over = object(json!({"startup_ms": 125.5, "idle_cpu_pct": 3.3, "bridge_ms": 3.6}));
	assert_eq!(
		over_budget(&over, &budget),
		vec![
			"footprint: startup_ms 125.5 over budget 100".to_owned(),
			"footprint: idle_cpu_pct 3.3 over budget 2.2".to_owned(),
			"footprint: bridge_ms 3.6 over budget 1.5".to_owned(),
		]
	);
	// Five percent of a big budget is more than the slack: the relative limit then applies.
	let big = object(json!({"startup_ms": 1000.0}));
	assert!(over_budget(&object(json!({"startup_ms": 1050.0})), &big).is_empty());
	assert_eq!(over_budget(&object(json!({"startup_ms": 1051.0})), &big).len(), 1);
	// A steady number has no slack.
	assert_eq!(over_budget(&object(json!({"processes": 4.0})), &budget).len(), 1);
}

#[test]
fn a_key_with_no_budget_or_a_null_budget_is_skipped() {
	let measured = object(json!({"startup_ms": 9999.0, "bridge_ms": 99.0, "unlisted": 1.0}));
	let budget = object(json!({"startup_ms": null, "bridge_ms": null}));
	assert!(over_budget(&measured, &budget).is_empty());
}

#[test]
fn a_written_budget_never_goes_up_and_a_null_budget_takes_the_measured_number() {
	let measured = object(json!({"idle_rss_mb": 170.0, "binary_mb": 43.3, "bridge_ms": 0.1}));
	let old = object(json!({"idle_rss_mb": 166.1, "binary_mb": 45.1, "bridge_ms": null}));
	assert_eq!(ratchet(&measured, &old), object(json!({"idle_rss_mb": 166.1, "binary_mb": 43.3, "bridge_ms": 0.1})));
}

#[test]
fn ps_cpu_times_read_days_hours_minutes_and_seconds() {
	assert_eq!(ps_seconds("00:01"), 1.0);
	assert_eq!(ps_seconds("01:02:03"), 3723.0);
	assert_eq!(ps_seconds("1-00:00:01"), 86_401.0);
}
