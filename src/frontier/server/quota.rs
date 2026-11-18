//! The subscription windows (0456): `omp usage --json` (omp 18.20.2, `cli/usage-cli.ts`) reports each account's limits
//! with `window.id` ("5h", "7d"), `amount` and `window.resetsAt`. The HUD and the rail show the most used of each window
//! across his accounts; one toast goes out when a window passes `QUOTA_WARN`.

use chrono::{DateTime, SecondsFormat};
use serde_json::{Map, Value};

use crate::frontier::protocol::{QuotaSpan, QuotaWindow, quota_percent};
use crate::shared::day::{hhmm, weekday};
use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, on_path, run_program};

const WINDOWS: [QuotaSpan; 2] = [QuotaSpan::FiveHours, QuotaSpan::SevenDays];

/// A window's id as omp and the protocol spell it.
pub fn span_id(span: QuotaSpan) -> &'static str {
	match span {
		QuotaSpan::FiveHours => "5h",
		QuotaSpan::SevenDays => "7d",
	}
}

/// One report's 5h or 7d window: its used fraction (above 1 past the limit) and its reset in epoch ms when known.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageWindow {
	pub window: QuotaSpan,
	pub used: f64,
	pub resets_at: Option<i64>,
}

/// One account's report: its provider (as omp printed it) and its windows.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageReport {
	pub provider: Value,
	pub windows: Vec<UsageWindow>,
}

/// A finite JSON number.
fn num(value: Option<&Value>) -> Option<f64> {
	value?.as_f64().filter(|n| n.is_finite())
}

/// A limit's used fraction, as omp's `resolveUsedFraction` resolves it: fraction, used/limit, percent, remaining.
fn used_fraction(limit: &Value) -> Option<f64> {
	let empty = Map::new();
	let amount = limit.get("amount").and_then(Value::as_object).unwrap_or(&empty);
	if let Some(fraction) = num(amount.get("usedFraction")) {
		return Some(fraction);
	}
	let used = num(amount.get("used"));
	let max = num(amount.get("limit"));
	if let (Some(used), Some(max)) = (used, max)
		&& max > 0.0
	{
		return Some(used / max);
	}
	if amount.get("unit").and_then(Value::as_str) == Some("percent")
		&& let Some(used) = used
	{
		return Some(used / 100.0);
	}
	num(amount.get("remainingFraction")).map(|remaining| (1.0 - remaining).max(0.0))
}

/// The 5h or 7d window of one limit; `None` for another window or one without a used fraction.
fn usage_window(limit: &Value) -> Option<UsageWindow> {
	let window = limit.get("window");
	let id = window?.get("id")?.as_str()?;
	let span = WINDOWS.into_iter().find(|span| span_id(*span) == id)?;
	let used = used_fraction(limit)?;
	let resets_at = num(window.and_then(|w| w.get("resetsAt"))).map(|ms| ms as i64);
	Some(UsageWindow { window: span, used, resets_at })
}

/// Each report of `omp usage --json` (one per account), in omp's order: its provider and its 5h and 7d windows with a
/// used fraction. The HUD's [`read_quota`] and the benchmark's quota pacer (`evals/pacer.ts`, 0543) both read it.
pub fn usage_reports(parsed: &Value) -> Vec<UsageReport> {
	let reports = parsed.get("reports").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
	reports
		.iter()
		.map(|report| {
			let limits = report.get("limits").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
			UsageReport {
				provider: report.get("provider").cloned().unwrap_or(Value::Null),
				windows: limits.iter().filter_map(usage_window).collect(),
			}
		})
		.collect()
}

/// An epoch-ms time as `Date#toISOString` prints it.
pub fn iso(ms: i64) -> Option<String> {
	Some(DateTime::from_timestamp_millis(ms)?.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// The 5h and 7d windows of `omp usage --json`'s output: the most used of each across reports; none when absent.
pub fn read_quota(json: &str) -> Result<Vec<QuotaWindow>> {
	let parsed: Value = serde_json::from_str(json)?;
	let mut most: [Option<QuotaWindow>; 2] = [None, None];
	for report in usage_reports(&parsed) {
		for UsageWindow { window, used, resets_at } in report.windows {
			let slot = &mut most[WINDOWS.iter().position(|w| *w == window).unwrap_or(0)];
			if slot.as_ref().map_or(-1.0, |q| q.used) >= used {
				continue;
			}
			let resets_at = resets_at.filter(|ms| *ms != 0).and_then(iso);
			*slot = Some(QuotaWindow { window, used, resets_at });
		}
	}
	Ok(most.into_iter().flatten().collect())
}

/// A window's share as the HUD says it: `5h 42%`.
fn quota_part(q: &QuotaWindow) -> String {
	format!("{} {}%", span_id(q.window), quota_percent(q))
}

/// The toast when a window passes the threshold: `Subscription quota: 5h at 84%, resets at 16:30`; the weekly window's
/// reset names its day (`resets Thu 09:00`).
pub fn quota_warning(q: &QuotaWindow) -> String {
	let at =
		q.resets_at.as_deref().and_then(|text| DateTime::parse_from_rfc3339(text).ok()).map(|t| t.timestamp_millis());
	let resets = match at {
		None => String::new(),
		Some(at) if q.window == QuotaSpan::FiveHours => format!(", resets at {}", hhmm(at)),
		Some(at) => format!(", resets {} {}", weekday(at), hhmm(at)),
	};
	format!("Subscription quota: {}{resets}", quota_part(q).replacen(' ', " at ", 1))
}

/// `omp usage --json`'s output; fails when omp isn't on PATH or fails. Ended after a minute.
pub async fn omp_usage() -> Result<String> {
	let omp = on_path("omp", None).ok_or_else(|| Error::other("omp is not on PATH"))?;
	let argv = [omp.into_os_string(), "usage".into(), "--json".into()];
	let run = run_program(&argv, RunOptions { timeout_ms: Some(60_000), ..RunOptions::default() }).await?;
	if run.code != Some(0) {
		let why = if run.timed_out {
			"ran past a minute".to_owned()
		} else {
			let how = run.code.map(|c| c.to_string()).or(run.signal).unwrap_or_else(|| "null".to_owned());
			format!("exit {how}")
		};
		let first = run.stderr.trim().split('\n').next().unwrap_or_default();
		let said = if first.is_empty() { why.as_str() } else { first };
		return Err(Error::other(format!("omp usage failed: {said}")));
	}
	Ok(run.stdout)
}

#[cfg(test)]
mod tests {
	use chrono::{Local, TimeZone};
	use serde_json::json;

	use super::*;

	fn local_ms() -> i64 {
		Local.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap().timestamp_millis()
	}

	/// `omp usage --json` as omp 18.20.2 prints it (`cli/usage-cli.ts`), trimmed to what matters here.
	fn usage() -> Value {
		json!({
			"generatedAt": 1,
			"reports": [
				{
					"provider": "anthropic",
					"fetchedAt": 1,
					"limits": [
						{
							"id": "anthropic:5h",
							"label": "5 Hour",
							"scope": { "provider": "anthropic", "windowId": "5h" },
							"window": { "id": "5h", "label": "5 Hour", "resetsAt": local_ms() },
							"amount": { "usedFraction": 0.42, "unit": "percent" },
							"status": "ok",
						},
						{
							"id": "anthropic:7d",
							"label": "7 Day",
							"scope": { "provider": "anthropic", "windowId": "7d" },
							"window": { "id": "7d", "label": "7 Day" },
							"amount": { "used": 18, "unit": "percent" },
						},
						{ "id": "anthropic:extra", "label": "Extra", "scope": {}, "window": { "id": "extra" }, "amount": { "usedFraction": 0.99 } },
					],
				},
				{
					"provider": "openai-codex",
					"fetchedAt": 1,
					"limits": [
						// Another account's 5-hour window, more used: the HUD shows the closest to its limit.
						{ "id": "codex:5h", "window": { "id": "5h", "label": "5 hours" }, "amount": { "used": 3, "limit": 4, "unit": "requests" } },
						{ "id": "codex:7d", "window": { "id": "7d", "label": "7 days" }, "amount": { "remainingFraction": 0.9, "unit": "percent" } },
					],
				},
			],
			"accountsWithoutUsage": [],
			"disabledCredentials": [],
			"capacity": {},
		})
	}

	#[test]
	fn the_5h_and_7d_windows_are_the_most_used_of_each_across_accounts_read_as_omp_resolves_a_used_fraction() {
		let quota = read_quota(&usage().to_string()).unwrap();
		assert_eq!(
			quota,
			vec![
				QuotaWindow { window: QuotaSpan::FiveHours, used: 0.75, resets_at: None },
				QuotaWindow { window: QuotaSpan::SevenDays, used: 0.18, resets_at: None },
			]
		);
	}

	#[test]
	fn no_reports_or_windows_without_amounts_give_no_windows() {
		assert_eq!(read_quota(&json!({ "reports": [] }).to_string()).unwrap(), vec![]);
		let bare = json!({ "reports": [{ "limits": [{ "window": { "id": "5h" }, "amount": {} }] }] });
		assert_eq!(read_quota(&bare.to_string()).unwrap(), vec![]);
		assert!(read_quota("not json").is_err());
	}

	#[test]
	fn the_toast_names_the_window_its_share_and_when_it_resets() {
		let at = Some(iso(local_ms()).unwrap());
		let window = |window, used, resets_at| QuotaWindow { window, used, resets_at };
		assert_eq!(
			quota_warning(&window(QuotaSpan::FiveHours, 0.84, at.clone())),
			"Subscription quota: 5h at 84%, resets at 16:30"
		);
		assert_eq!(
			quota_warning(&window(QuotaSpan::SevenDays, 0.91, at)),
			"Subscription quota: 7d at 91%, resets Fri 16:30"
		);
		assert_eq!(quota_warning(&window(QuotaSpan::SevenDays, 0.91, None)), "Subscription quota: 7d at 91%");
	}
}
