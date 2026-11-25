//! The subscription windows (0456): `omp usage --json` (omp 19.2.0, `cli/usage-cli.ts`) reports each account's limits
//! with `window.id` ("5h", "7d"), `amount` and `window.resetsAt`. The HUD and the rail show the most used of each window
//! across his accounts; one toast goes out when a window passes `QUOTA_WARN`.

use chrono::{DateTime, SecondsFormat};
use serde_json::{Map, Value};

use crate::frontier::protocol::{QuotaAccount, QuotaCapacity, QuotaSpan, QuotaWindow, quota_percent};
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

/// The toast text after the label: `5h at 84%, resets at 16:30`; the weekly window's reset names its day
/// (`resets Thu 09:00`).
fn warning_text(q: &QuotaWindow) -> String {
	let at =
		q.resets_at.as_deref().and_then(|text| DateTime::parse_from_rfc3339(text).ok()).map(|t| t.timestamp_millis());
	let resets = match at {
		None => String::new(),
		Some(at) if q.window == QuotaSpan::FiveHours => format!(", resets at {}", hhmm(at)),
		Some(at) => format!(", resets {} {}", weekday(at), hhmm(at)),
	};
	format!("{}{resets}", quota_part(q).replacen(' ', " at ", 1))
}

/// The toast when a window passes the threshold: `Subscription quota: 5h at 84%, resets at 16:30`; the weekly window's
/// reset names its day (`resets Thu 09:00`).
pub fn quota_warning(q: &QuotaWindow) -> String {
	format!("Subscription quota: {}", warning_text(q))
}

/// The toast for one account's window: `Subscription quota (name): 5h at 84%, resets at 16:30`.
pub fn account_warning(account: &str, w: &QuotaWindow) -> String {
	format!("Subscription quota ({account}): {}", warning_text(w))
}

/// Every account of `omp usage --json --redact` and the remaining capacity (0669).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRead {
	pub accounts: Vec<QuotaAccount>,
	pub capacity: Vec<QuotaCapacity>,
}

/// Whether a limit is a model-tier one (omp's `meter`, e.g. `anthropic:7d:fable`): it is not the plan's 5h or 7d line.
fn is_metered(limit: &Value) -> bool {
	let present = |v: Option<&Value>| v.is_some_and(|m| !m.is_null());
	present(limit.get("meter")) || present(limit.get("scope").and_then(|s| s.get("meter")))
}

/// An account's name: its (redacted) email, else its account id, else `#<credentialId>`, else its provider.
fn account_name(report: &Value) -> String {
	let metadata = report.get("metadata");
	let text = |key: &str| {
		let value = metadata?.get(key)?;
		let text = value.as_str().map(str::to_owned).or_else(|| value.as_i64().map(|n| n.to_string()))?;
		Some(text).filter(|t| !t.is_empty())
	};
	text("email")
		.or_else(|| text("accountId"))
		.or_else(|| text("credentialId").map(|id| format!("#{id}")))
		.or_else(|| report.get("provider").and_then(Value::as_str).map(str::to_owned))
		.unwrap_or_else(|| "account".to_owned())
}

/// The window as the protocol carries it: a zero reset is none.
fn quota_window(w: UsageWindow) -> QuotaWindow {
	QuotaWindow { window: w.window, used: w.used, resets_at: w.resets_at.filter(|ms| *ms != 0).and_then(iso) }
}

/// One report as an account: the most used plan 5h and 7d window (model-tier limits skipped).
fn account_of(report: &Value) -> QuotaAccount {
	let limits = report.get("limits").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
	let mut most: [Option<UsageWindow>; 2] = [None, None];
	for window in limits.iter().filter(|l| !is_metered(l)).filter_map(usage_window) {
		let slot = &mut most[WINDOWS.iter().position(|w| *w == window.window).unwrap_or(0)];
		if slot.as_ref().is_none_or(|q| q.used < window.used) {
			*slot = Some(window);
		}
	}
	let [five_h, seven_d] = most;
	QuotaAccount { name: account_name(report), five_h: five_h.map(quota_window), seven_d: seven_d.map(quota_window) }
}

/// The capacity entries of `capacity.anthropic[]` for the 5h and 7d windows, metered ones skipped.
fn capacity_of(parsed: &Value) -> Vec<QuotaCapacity> {
	let entries = parsed.pointer("/capacity/anthropic").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
	entries
		.iter()
		.filter(|entry| !is_metered(entry))
		.filter_map(|entry| {
			let window = entry.get("window")?;
			let id = window.as_str().or_else(|| window.get("id")?.as_str())?;
			let window = WINDOWS.into_iter().find(|span| span_id(*span) == id)?;
			let count = |key: &str| num(entry.get(key)).map(|n| n.max(0.0).round() as u32);
			Some(QuotaCapacity { window, remaining: count("remainingAccounts")?, accounts: count("accounts")? })
		})
		.collect()
}

/// Each account and the capacity of `omp usage --json --redact`'s output; `None` when it isn't JSON with `reports`.
/// A report without `metadata` is named by its provider; accounts keep omp's order.
pub fn read_accounts(text: &str) -> Option<AccountRead> {
	let parsed: Value = serde_json::from_str(text).ok()?;
	let reports = parsed.get("reports")?.as_array()?;
	Some(AccountRead { accounts: reports.iter().map(account_of).collect(), capacity: capacity_of(&parsed) })
}

/// `omp usage --json --redact`'s output (emails and account ids shortened); `None` when omp is absent, fails, or is too
/// old for `--redact`, so the HUD keeps its last good value.
pub async fn omp_usage_redacted() -> Option<String> {
	run_usage(&["--redact"]).await.ok()
}

/// `omp usage --json`'s output; fails when omp isn't on PATH or fails. Ended after a minute.
pub async fn omp_usage() -> Result<String> {
	run_usage(&[]).await
}

async fn run_usage(extra: &[&str]) -> Result<String> {
	let omp = on_path("omp", None).ok_or_else(|| Error::other("omp is not on PATH"))?;
	let mut argv = vec![omp.into_os_string(), "usage".into(), "--json".into()];
	argv.extend(extra.iter().map(|arg| (*arg).into()));
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

	/// `omp usage --json` as omp 19.2.0 prints it (`cli/usage-cli.ts`), trimmed to what matters here.
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

	/// `omp usage --json --redact` for two accounts: a fable-tier limit and capacity entries with and without `meter`.
	fn two_accounts() -> Value {
		json!({
			"reports": [
				{
					"provider": "anthropic",
					"metadata": { "email": "ab*", "accountId": "98*", "credentialId": 7 },
					"limits": [
						{ "id": "anthropic:5h", "window": { "id": "5h", "resetsAt": local_ms() }, "amount": { "usedFraction": 0.42 } },
						{ "id": "anthropic:7d", "window": { "id": "7d" }, "amount": { "usedFraction": 0.18 } },
						{ "id": "anthropic:7d:fable", "meter": "fable", "window": { "id": "7d" }, "amount": { "usedFraction": 0.97 } },
					],
				},
				{
					"provider": "anthropic",
					"metadata": { "accountId": "12*", "credentialId": "c2" },
					"limits": [
						{ "id": "anthropic:5h", "window": { "id": "5h", "resetsAt": 0 }, "amount": { "usedFraction": 0.9 } },
					],
				},
				{ "provider": "anthropic", "metadata": { "credentialId": "c3" }, "limits": [] },
			],
			"capacity": { "anthropic": [
				{ "window": "5h", "accounts": 3, "usedAccounts": 2, "remainingAccounts": 1 },
				{ "window": "7d", "accounts": 3, "usedAccounts": 1, "remainingAccounts": 2 },
				{ "window": "7d", "meter": "fable", "accounts": 3, "usedAccounts": 3, "remainingAccounts": 0 },
			] },
		})
	}

	#[test]
	fn each_account_has_its_own_5h_and_7d_windows_named_by_email_then_account_id_then_credential_skipping_fable_limits()
	{
		let read = read_accounts(&two_accounts().to_string()).unwrap();
		let reset = Some(iso(local_ms()).unwrap());
		assert_eq!(
			read.accounts,
			vec![
				QuotaAccount {
					name: "ab*".into(),
					five_h: Some(QuotaWindow { window: QuotaSpan::FiveHours, used: 0.42, resets_at: reset }),
					seven_d: Some(QuotaWindow { window: QuotaSpan::SevenDays, used: 0.18, resets_at: None }),
				},
				QuotaAccount {
					name: "12*".into(),
					five_h: Some(QuotaWindow { window: QuotaSpan::FiveHours, used: 0.9, resets_at: None }),
					seven_d: None,
				},
				QuotaAccount { name: "#c3".into(), five_h: None, seven_d: None },
			]
		);
	}

	#[test]
	fn capacity_keeps_the_5h_and_7d_entries_without_a_meter() {
		let read = read_accounts(&two_accounts().to_string()).unwrap();
		assert_eq!(
			read.capacity,
			vec![
				QuotaCapacity { window: QuotaSpan::FiveHours, remaining: 1, accounts: 3 },
				QuotaCapacity { window: QuotaSpan::SevenDays, remaining: 2, accounts: 3 },
			]
		);
	}

	#[test]
	fn a_report_without_metadata_is_named_by_its_provider_and_no_capacity_gives_none() {
		let bare = json!({ "reports": [{ "provider": "anthropic", "limits": [
			{ "window": { "id": "5h" }, "amount": { "usedFraction": 0.5 } },
		] }] });
		let read = read_accounts(&bare.to_string()).unwrap();
		assert_eq!(read.accounts.len(), 1);
		assert_eq!(read.accounts[0].name, "anthropic");
		assert_eq!(read.accounts[0].five_h.as_ref().map(|w| w.used), Some(0.5));
		assert_eq!(read.capacity, vec![]);
	}

	#[test]
	fn text_that_is_not_json_or_has_no_reports_reads_as_none() {
		assert_eq!(read_accounts("not json"), None);
		assert_eq!(read_accounts("{}"), None);
		assert_eq!(
			read_accounts(&json!({ "reports": [] }).to_string()),
			Some(AccountRead { accounts: vec![], capacity: vec![] })
		);
	}

	#[test]
	fn the_account_toast_names_the_account() {
		let at = Some(iso(local_ms()).unwrap());
		let w = QuotaWindow { window: QuotaSpan::FiveHours, used: 0.84, resets_at: at };
		assert_eq!(account_warning("ab*", &w), "Subscription quota (ab*): 5h at 84%, resets at 16:30");
	}
}
