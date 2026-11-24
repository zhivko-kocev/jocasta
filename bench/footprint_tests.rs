use serde_json::{Map, Value, json};

use super::{over_budget, ps_seconds};

fn object(value: Value) -> Map<String, Value> {
	value.as_object().cloned().unwrap()
}

#[test]
fn only_numbers_more_than_five_percent_over_a_set_budget_fail() {
	let measured = object(json!({"processes": 3, "idle_rss_mb": 105.0, "startup_ms": 106.0, "bridge_ms": 4.0}));
	let budget = object(json!({"processes": 3, "idle_rss_mb": 100.0, "startup_ms": 100.0, "bridge_ms": null}));
	assert_eq!(over_budget(&measured, &budget), vec!["footprint: startup_ms 106 over budget 100".to_owned()]);
}

#[test]
fn ps_cpu_times_read_days_hours_minutes_and_seconds() {
	assert_eq!(ps_seconds("00:01"), 1.0);
	assert_eq!(ps_seconds("01:02:03"), 3723.0);
	assert_eq!(ps_seconds("1-00:00:01"), 86_401.0);
}
