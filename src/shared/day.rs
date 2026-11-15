//! Local calendar days as `YYYY-MM-DD`: the one place they are named and turned back into times.

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike};

/// The local time of epoch-ms `ms`.
fn local(ms: i64) -> DateTime<Local> {
	DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&Local)
}

/// The epoch ms of local `naive` time; a time the clock skips (a spring-forward gap) moves on to the first one after
/// it, and a repeated one is its first, as JavaScript's `new Date(y, m, d)` does.
fn local_ms(naive: NaiveDateTime) -> i64 {
	let mut at = naive;
	for _ in 0..(24 * 4) {
		match Local.from_local_datetime(&at) {
			LocalResult::Single(time) => return time.timestamp_millis(),
			LocalResult::Ambiguous(first, _) => return first.timestamp_millis(),
			LocalResult::None => at += Duration::minutes(15),
		}
	}
	naive.and_utc().timestamp_millis()
}

/// The local calendar day of a time, as `YYYY-MM-DD`.
pub fn local_day(ms: i64) -> String {
	local(ms).format("%Y-%m-%d").to_string()
}

/// The local midnight starting `day` (`YYYY-MM-DD`) plus `plus_days` calendar days, epoch ms (right across DST). A
/// part of `day` that is missing or isn't a number counts as year 0, month 1 and date 1 do in the TypeScript's
/// destructuring defaults.
pub fn day_start(day: &str, plus_days: i64) -> i64 {
	let mut parts = day.split('-').map(|part| part.trim().parse::<i64>().ok());
	let year = parts.next().flatten().unwrap_or(0);
	let month0 = parts.next().flatten().unwrap_or(1) - 1;
	let date = parts.next().flatten().unwrap_or(1);
	// Month and date overflow as `new Date(year, month, date)` lets them.
	let year = i32::try_from(year + month0.div_euclid(12)).unwrap_or(0);
	let first =
		NaiveDate::from_ymd_opt(year, u32::try_from(month0.rem_euclid(12) + 1).unwrap_or(1), 1).unwrap_or_default();
	let day = first.checked_add_signed(Duration::try_days(date - 1 + plus_days).unwrap_or_default()).unwrap_or(first);
	local_ms(day.and_time(chrono::NaiveTime::MIN))
}

/// The local midnight starting the day of `ms`, epoch ms.
pub fn midnight_of(ms: i64) -> i64 {
	local_ms(local(ms).date_naive().and_time(chrono::NaiveTime::MIN))
}

/// A time's local clock as `HH:MM`.
pub fn hhmm(at: i64) -> String {
	let time = local(at);
	format!("{:02}:{:02}", time.hour(), time.minute())
}

/// A time's local weekday, short: `Mon`.
pub fn weekday(at: i64) -> String {
	local(at).format("%a").to_string()
}

/// A day as `day_label` takes it: a time on it, or its `YYYY-MM-DD`.
pub enum DayRef<'a> {
	Time(i64),
	Day(&'a str),
}

/// A month's short name as `en-GB` writes it (September is `Sept`).
fn month_short(month: u32) -> &'static str {
	const NAMES: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec"];
	NAMES[(month as usize + 11) % 12]
}

/// A day's label: `Mon 3 Nov`, or with `year` `Mon 3 Nov 2026`.
pub fn day_label(day: DayRef, year: bool) -> String {
	let at = match day {
		DayRef::Time(ms) => ms,
		DayRef::Day(day) => day_start(day, 0),
	};
	let date = local(at);
	let mut label = format!("{} {} {}", weekday(at), date.day(), month_short(date.month()));
	if year {
		label.push_str(&format!(" {}", date.year()));
	}
	label
}

#[cfg(test)]
mod tests {
	use super::*;

	/// `new Date(2026, 10, 3, 9, 5)`: local time of 3 Nov 2026.
	fn at(month: u32, day: u32, hour: u32, minute: u32) -> i64 {
		local_ms(NaiveDate::from_ymd_opt(2026, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap())
	}

	#[test]
	fn local_days_and_their_midnights() {
		let time = at(11, 3, 9, 5);
		assert_eq!(local_day(time), "2026-11-03");
		assert_eq!(day_start("2026-11-03", 0), at(11, 3, 0, 0));
		assert_eq!(day_start("2026-11-03", 1), at(11, 4, 0, 0));
		assert_eq!(midnight_of(time), at(11, 3, 0, 0));
	}

	#[test]
	fn hhmm_the_weekday_and_the_day_label_as_every_page_says_them() {
		let time = at(11, 3, 9, 5);
		assert_eq!(hhmm(time), "09:05");
		assert_eq!(weekday(time), "Tue");
		assert_eq!(day_label(DayRef::Time(time), false), "Tue 3 Nov");
		assert_eq!(day_label(DayRef::Day("2026-11-03"), true), "Tue 3 Nov 2026");
		assert_eq!(day_label(DayRef::Day("2026-09-03"), false), "Thu 3 Sept");
	}

	#[test]
	fn days_overflow_across_months_and_years() {
		assert_eq!(local_day(day_start("2026-12-31", 1)), "2027-01-01");
		assert_eq!(local_day(day_start("2026-03-01", -1)), "2026-02-28");
	}
}
