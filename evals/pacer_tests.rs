//! Ported from `test/evals/pacer.test.ts`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use jocasta::frontier::protocol::QuotaSpan;
use jocasta::frontier::server::quota::UsageWindow;
use jocasta::shared::errors::{Error, Result};
use serde_json::{Value, json};

use super::pacer::{
	Account, PACER_PROVIDER, PacerDeps, Pause, PausedAccount, RECHECK_MS, RESET_SLACK_MS, create_pacer, pause_for,
	read_accounts,
};

fn now() -> i64 {
	chrono::DateTime::parse_from_rfc3339("2026-10-24T10:00:00Z").unwrap().timestamp_millis()
}

fn at(minutes: i64) -> i64 {
	now() + minutes * 60_000
}

/// One account of `usage_json`: `[5h used, 7d used, 5h reset minutes, 7d reset minutes]`.
type Row = (f64, f64, Option<i64>, Option<i64>);

/// `omp usage --json` with one report per account.
fn usage_json(accounts: &[Row], extra: Option<Value>) -> String {
	let window = |id: &str, reset: Option<i64>| match reset {
		Some(minutes) => json!({ "id": id, "resetsAt": at(minutes) }),
		None => json!({ "id": id }),
	};
	let mut reports: Vec<Value> = accounts
		.iter()
		.map(|(h5, d7, r5, r7)| {
			json!({
				"provider": "anthropic",
				"metadata": { "email": "someone@example.org" },
				"limits": [
					{ "window": window("5h", *r5), "amount": { "usedFraction": h5 } },
					{ "window": window("7d", *r7), "amount": { "used": d7 * 100.0, "unit": "percent" } },
				],
			})
		})
		.collect();
	reports.push(
		json!({ "provider": "github-copilot", "limits": [{ "window": { "id": "5h" }, "amount": { "usedFraction": 0.1 } }] }),
	);
	let mut document = json!({ "generatedAt": now(), "reports": reports, "accountsWithoutUsage": [] });
	if let (Some(Value::Object(extra)), Some(object)) = (extra, document.as_object_mut()) {
		object.extend(extra);
	}
	document.to_string()
}

fn window(span: QuotaSpan, used: f64, resets_at: Option<i64>) -> UsageWindow {
	UsageWindow { window: span, used, resets_at }
}

fn accounts_of(json: &str) -> Vec<Account> {
	read_accounts(json, PACER_PROVIDER).unwrap()
}

#[test]
fn each_anthropic_report_is_one_account_with_its_windows_named_by_position_only_other_providers_are_left_out() {
	let accounts = accounts_of(&usage_json(&[(0.9, 0.2, Some(30), None), (0.1, 0.07, None, None)], None));
	assert_eq!(
		accounts,
		vec![
			Account {
				id: "anthropic#1".to_owned(),
				windows: vec![window(QuotaSpan::FiveHours, 0.9, Some(at(30))), window(QuotaSpan::SevenDays, 0.2, None)],
			},
			Account {
				id: "anthropic#2".to_owned(),
				windows: vec![window(QuotaSpan::FiveHours, 0.1, None), window(QuotaSpan::SevenDays, 0.07, None)],
			},
		]
	);
	let unreported = accounts_of(&usage_json(
		&[(0.9, 0.9, None, None)],
		Some(json!({ "accountsWithoutUsage": [{ "provider": "anthropic" }] })),
	));
	assert_eq!(unreported.iter().map(|a| a.windows.len()).collect::<Vec<_>>(), [2, 0]);
}

#[test]
fn one_full_account_doesnt_pause_the_run_0543_2026_11_05s_second_account_at_its_7d_limit_the_first_at_7_percent() {
	let accounts = accounts_of(&usage_json(&[(0.07, 0.07, None, None), (0.3, 0.86, Some(60), Some(3000))], None));
	assert_eq!(pause_for(&accounts, now()), None);
	assert_eq!(pause_for(&[], now()), None);
	// An account omp lists without a report isn't known to be full.
	let unreported =
		usage_json(&[(0.9, 0.9, None, None)], Some(json!({ "accountsWithoutUsage": [{ "provider": "anthropic" }] })));
	assert_eq!(pause_for(&accounts_of(&unreported), now()), None);
}

#[test]
fn with_every_account_full_the_run_waits_for_the_first_account_to_free_up_each_at_its_latest_full_windows_reset() {
	let accounts = vec![
		Account {
			id: "anthropic#1".to_owned(),
			windows: vec![
				window(QuotaSpan::FiveHours, 0.9, Some(at(30))),
				window(QuotaSpan::SevenDays, 0.95, Some(at(600))),
			],
		},
		Account {
			id: "anthropic#2".to_owned(),
			windows: vec![window(QuotaSpan::FiveHours, 0.85, Some(at(120))), window(QuotaSpan::SevenDays, 0.4, None)],
		},
	];
	assert_eq!(
		pause_for(&accounts, now()),
		Some(Pause {
			until: at(120) + RESET_SLACK_MS,
			accounts: vec![
				PausedAccount { account: "anthropic#1".to_owned(), window: QuotaSpan::SevenDays, used: 0.95 },
				PausedAccount { account: "anthropic#2".to_owned(), window: QuotaSpan::FiveHours, used: 0.85 },
			],
		})
	);
	let one = |window: UsageWindow| vec![Account { id: "a".to_owned(), windows: vec![window] }];
	assert_eq!(
		pause_for(&one(window(QuotaSpan::SevenDays, 1.2, None)), now()).map(|p| p.until),
		Some(now() + RECHECK_MS)
	);
	assert_eq!(
		pause_for(&one(window(QuotaSpan::FiveHours, 0.9, Some(at(-5)))), now()).map(|p| p.until),
		Some(now() + RECHECK_MS)
	);
}

/// A fake usage source that answers each read from `reads` and a clock that sleeps by moving forward.
struct Fake {
	reads: Mutex<VecDeque<std::result::Result<String, String>>>,
	now: AtomicI64,
	slept: Mutex<Vec<i64>>,
	lines: Mutex<Vec<String>>,
}

impl Fake {
	fn new(reads: Vec<std::result::Result<String, String>>) -> Arc<Self> {
		Arc::new(Self {
			reads: Mutex::new(reads.into()),
			now: AtomicI64::new(now()),
			slept: Mutex::new(Vec::new()),
			lines: Mutex::new(Vec::new()),
		})
	}
}

impl PacerDeps for Fake {
	fn usage(&self) -> BoxFuture<'_, Result<String>> {
		let next = self.reads.lock().unwrap().pop_front();
		Box::pin(async move {
			match next {
				None => Err(Error::other("no more reads")),
				Some(Err(message)) => Err(Error::other(message)),
				Some(Ok(json)) => Ok(json),
			}
		})
	}

	fn sleep(&self, ms: i64) -> BoxFuture<'_, ()> {
		self.slept.lock().unwrap().push(ms);
		self.now.fetch_add(ms, Ordering::SeqCst);
		Box::pin(async {})
	}

	fn now(&self) -> i64 {
		self.now.load(Ordering::SeqCst)
	}

	fn log(&self, line: &str) {
		self.lines.lock().unwrap().push(line.to_owned());
	}
}

#[tokio::test]
async fn the_pacer_waits_while_every_account_is_full_reads_again_and_reports_a_full_account_to_the_infra_classifier() {
	let fake = Fake::new(vec![
		Ok(usage_json(&[(0.9, 0.2, Some(20), None), (0.95, 0.3, Some(50), None)], None)),
		Ok(usage_json(&[(0.1, 0.2, None, None), (0.95, 0.3, Some(30), None)], None)),
	]);
	let pauses = Arc::new(Mutex::new(Vec::new()));
	let pacer = create_pacer(fake.clone(), pauses.clone());
	assert!(!pacer.account_full());
	pacer.pace().await;
	assert_eq!(*fake.slept.lock().unwrap(), [20 * 60_000 + RESET_SLACK_MS]);
	assert_eq!(pauses.lock().unwrap().len(), 1);
	let lines = fake.lines.lock().unwrap();
	assert!(
		lines[0].contains("quota pacer: every account is at 85% or more (anthropic#1 5h 90%, anthropic#2 5h 95%)"),
		"{}",
		lines[0]
	);
	assert!(!lines[0].contains("example.org"));
	// The second read: account 2 is still full, so a rate limit in this trial is an infra error.
	assert!(pacer.account_full());
}

#[tokio::test]
async fn a_failed_usage_read_is_logged_and_the_trial_runs() {
	let fake = Fake::new(vec![Err("omp is not on PATH".to_owned())]);
	let pacer = create_pacer(fake.clone(), Arc::new(Mutex::new(Vec::new())));
	pacer.pace().await;
	assert!(fake.slept.lock().unwrap().is_empty());
	assert!(!pacer.account_full());
	assert_eq!(
		*fake.lines.lock().unwrap(),
		["quota pacer: omp usage failed (omp is not on PATH); running the trial unpaced"]
	);
}
