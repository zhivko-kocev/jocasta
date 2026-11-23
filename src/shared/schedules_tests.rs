use chrono::{Local, NaiveDate, TimeZone};

use super::*;
use crate::shared::testing::temp_jocasta;

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
	let naive = NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap();
	Local.from_local_datetime(&naive).earliest().unwrap().timestamp_millis()
}

fn mondays() -> Schedule {
	let source = "- brief: Bump dependencies.\n  days: [mon]\n  time: \"09:00\"\n";
	parse_schedules(source, "schedules.yml").unwrap().remove(0)
}

#[test]
fn a_schedule_parses_with_its_optional_fields_and_an_id_that_changes_when_it_is_edited() {
	let source = "- brief: Bump dependencies.\n  days: [mon, Thu]\n  time: \"09:00\"\n  done_when: [\"cargo test\"]\n  heavy: true\n- brief: Tidy.\n  days: [sun]\n  time: \"23:30\"\n";
	let schedules = parse_schedules(source, "schedules.yml").unwrap();
	assert_eq!(schedules.len(), 2);
	assert_eq!(schedules[0].days, vec![Weekday::Mon, Weekday::Thu]);
	assert_eq!(schedules[0].time, (9, 0));
	assert_eq!(schedules[0].done_when, vec!["cargo test".to_owned()]);
	assert!(schedules[0].heavy);
	assert!(!schedules[1].heavy && schedules[1].done_when.is_empty());
	assert_eq!(schedules[0].id.len(), 12);
	assert_ne!(schedules[0].id, schedules[1].id);
	let edited = parse_schedules(&source.replace("09:00", "09:30"), "schedules.yml").unwrap();
	assert_ne!(edited[0].id, schedules[0].id);
	assert_eq!(edited[1].id, schedules[1].id);
	assert_eq!(parse_schedules("", "schedules.yml").unwrap(), Vec::new());
}

#[test]
fn a_bad_file_is_named_by_file_line_and_reason() {
	let bad_time = "- brief: A.\n  days: [mon]\n  time: \"25:00\"\n";
	let error = parse_schedules(bad_time, "p/schedules.yml").unwrap_err();
	assert!(error.starts_with("p/schedules.yml:3: "), "{error}");
	assert!(error.contains("[0].time"), "{error}");
	let bad_day = "- brief: A.\n  days: [funday]\n  time: \"09:00\"\n";
	assert!(parse_schedules(bad_day, "f").unwrap_err().starts_with("f:2: "));
	let unknown = "- brief: A.\n  days: [mon]\n  time: \"09:00\"\n  every: day\n";
	let error = parse_schedules(unknown, "f").unwrap_err();
	assert!(error.starts_with("f:4: ") && error.contains("every"), "{error}");
	assert!(parse_schedules("- days: [mon]\n  time: \"09:00\"\n", "f").unwrap_err().contains("brief"));
	assert!(parse_schedules("brief: x\n", "f").unwrap_err().contains("list of schedules"));
}

#[test]
fn the_latest_slot_is_the_last_matching_time_at_or_before_now_and_the_next_run_the_first_after() {
	let schedule = mondays();
	// 2026-11-23 is a Monday.
	let monday_nine = at(2026, 11, 23, 9, 0);
	assert_eq!(schedule.latest_slot(at(2026, 11, 23, 9, 1)), Some(monday_nine));
	assert_eq!(schedule.latest_slot(monday_nine), Some(monday_nine));
	assert_eq!(schedule.latest_slot(at(2026, 11, 23, 8, 59)), Some(at(2026, 11, 16, 9, 0)));
	assert_eq!(schedule.latest_slot(at(2026, 11, 29, 23, 59)), Some(monday_nine));
	assert_eq!(schedule.next_run(at(2026, 11, 23, 8, 0)), Some(monday_nine));
	assert_eq!(schedule.next_run(monday_nine), Some(at(2026, 11, 30, 9, 0)));
	let none = Schedule { days: Vec::new(), ..mondays() };
	assert_eq!(none.latest_slot(monday_nine), None);
	assert_eq!(none.next_run(monday_nine), None);
}

#[test]
fn several_days_give_the_nearest_slot_of_any_of_them() {
	let schedule = Schedule { days: vec![Weekday::Mon, Weekday::Thu], ..mondays() };
	assert_eq!(schedule.latest_slot(at(2026, 11, 27, 12, 0)), Some(at(2026, 11, 26, 9, 0)));
	assert_eq!(schedule.next_run(at(2026, 11, 24, 12, 0)), Some(at(2026, 11, 26, 9, 0)));
}

#[test]
fn the_state_remembers_when_a_schedule_was_first_seen_and_last_started_and_a_corrupt_file_is_empty() {
	let _env = temp_jocasta();
	let mut state = ScheduleState::read("p1");
	assert_eq!(state.baseline("a"), None);
	state.see("a", 100);
	state.see("a", 900);
	assert_eq!(state.baseline("a"), Some(100));
	state.started("a", 500);
	assert_eq!(state.baseline("a"), Some(500));
	state.write("p1").unwrap();
	let again = ScheduleState::read("p1");
	assert_eq!(again, state);
	assert_eq!(again.baseline("a"), Some(500));
	std::fs::write(project_dir("p1").join(STATE_FILE), "{not json").unwrap();
	assert_eq!(ScheduleState::read("p1").baseline("a"), None);
}

#[test]
fn read_schedules_has_none_for_a_missing_file_and_names_a_bad_one() {
	let _env = temp_jocasta();
	assert_eq!(read_schedules("p2"), Ok(Vec::new()));
	std::fs::create_dir_all(project_dir("p2")).unwrap();
	std::fs::write(project_dir("p2").join(SCHEDULES_FILE), "- brief: A.\n  days: [mon]\n  time: soon\n").unwrap();
	assert!(read_schedules("p2").unwrap_err().contains("schedules.yml:3: "));
}
