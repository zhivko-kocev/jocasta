//! The quota pacer (0482, amended by 0543): before each trial it reads `omp usage --json` and waits only while every
//! signed-in account of the arms' provider has its 5-hour or 7-day window at or past [`PAUSE_AT`] used; then it
//! holds the run until the first of those accounts frees up (the latest reset among its full windows), and reads again.
//! omp spreads sessions across his accounts, so one full account doesn't stop the run; a trial that hits a full
//! account's rate limit is an infra error and retried (`classifyInfra`'s `accountFull`). An account omp lists without a
//! usage report counts as not full. A window without a known reset is read again after [`RECHECK_MS`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use jocasta::frontier::protocol::{QuotaSpan, js_number};
use jocasta::frontier::server::quota::{UsageWindow, iso, omp_usage, span_id, usage_reports};
use jocasta::shared::clock::now_ms;
use jocasta::shared::errors::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The share of a window at which an account counts as full (0482).
pub const PAUSE_AT: f64 = 0.85;
/// How long to wait before reading again when omp reports no reset time.
pub const RECHECK_MS: i64 = 10 * 60_000;
/// Slack after a reset, so the next read sees the new window.
pub const RESET_SLACK_MS: i64 = 60_000;
/// The provider whose accounts the arms' trials use (every arm's model is Anthropic's; A7 bills its API key).
pub const PACER_PROVIDER: &str = "anthropic";

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
	/// `<provider>#<n>` in omp's order: run records name no account (0482: no user names).
	pub id: String,
	pub windows: Vec<UsageWindow>,
}

/// One account's fullest window at a pause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PausedAccount {
	pub account: String,
	pub window: QuotaSpan,
	#[serde(serialize_with = "js_number")]
	pub used: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pause {
	/// Epoch ms to wait until.
	pub until: i64,
	/// Each account's fullest window at the pause.
	pub accounts: Vec<PausedAccount>,
}

/// Whether `value` names `provider` (omp spells it in any case).
fn of_provider(value: Option<&Value>, provider: &str) -> bool {
	value.and_then(Value::as_str).is_some_and(|name| name.to_lowercase() == provider)
}

/// The provider's accounts in `omp usage --json`: each report is one account with its 5h and 7d windows; each
/// `accountsWithoutUsage` entry of the provider is an account with none.
pub fn read_accounts(json: &str, provider: &str) -> Result<Vec<Account>> {
	let parsed: Value = serde_json::from_str(json)?;
	let mut accounts: Vec<Account> = Vec::new();
	for report in usage_reports(&parsed) {
		if !of_provider(Some(&report.provider), provider) {
			continue;
		}
		accounts.push(Account { id: format!("{provider}#{}", accounts.len() + 1), windows: report.windows });
	}
	if let Some(Value::Array(without)) = parsed.get("accountsWithoutUsage") {
		for account in without {
			if of_provider(account.get("provider"), provider) {
				accounts.push(Account { id: format!("{provider}#{}", accounts.len() + 1), windows: Vec::new() });
			}
		}
	}
	Ok(accounts)
}

/// Whether an account has a window at or past [`PAUSE_AT`].
fn is_full(account: &Account) -> bool {
	account.windows.iter().any(|window| window.used >= PAUSE_AT)
}

/// The pause the accounts call for at `now`: none while any account isn't full (or omp lists none); otherwise until the
/// earliest moment one of them frees up, each freeing at the latest reset among its full windows.
pub fn pause_for(accounts: &[Account], now: i64) -> Option<Pause> {
	if accounts.is_empty() || !accounts.iter().all(is_full) {
		return None;
	}
	let until = accounts
		.iter()
		.map(|account| {
			account.windows.iter().filter(|window| window.used >= PAUSE_AT).fold(now, |frees, window| {
				let reset = match window.resets_at {
					Some(at) if at > now => at + RESET_SLACK_MS,
					_ => now + RECHECK_MS,
				};
				frees.max(reset)
			})
		})
		.min()?;
	fn fullest(account: &Account) -> Option<&UsageWindow> {
		let first = account.windows.first()?;
		Some(account.windows.iter().fold(first, |a, b| if b.used > a.used { b } else { a }))
	}
	Some(Pause {
		until,
		accounts: accounts
			.iter()
			.filter_map(|account| {
				let window = fullest(account)?;
				Some(PausedAccount { account: account.id.clone(), window: window.window, used: window.used })
			})
			.collect(),
	})
}

pub trait PacerDeps: Send + Sync {
	/// `omp usage --json`'s output.
	fn usage(&self) -> BoxFuture<'_, Result<String>>;
	fn sleep(&self, ms: i64) -> BoxFuture<'_, ()>;
	fn now(&self) -> i64;
	fn log(&self, line: &str);
}

/// The real thing: omp, the timer, the clock and stderr.
pub struct RealPacerDeps;

impl PacerDeps for RealPacerDeps {
	fn usage(&self) -> BoxFuture<'_, Result<String>> {
		Box::pin(omp_usage())
	}

	fn sleep(&self, ms: i64) -> BoxFuture<'_, ()> {
		Box::pin(tokio::time::sleep(Duration::from_millis(ms.max(0).unsigned_abs())))
	}

	fn now(&self) -> i64 {
		now_ms()
	}

	fn log(&self, line: &str) {
		eprintln!("{line}");
	}
}

/// The runner's `pace` hook, and whether the last reading had a full account (for `classifyInfra`).
pub struct Pacer {
	deps: Arc<dyn PacerDeps>,
	pauses: Arc<Mutex<Vec<Pause>>>,
	full: AtomicBool,
}

/// The pacer. A failed read is logged and the trial runs: without omp's numbers nothing says to wait, and a run that
/// hits the limit retries its trials as infra errors. Each pause is logged with every account's fullest window and the
/// time it waits until; `pauses` collects them for the run's record.
pub fn create_pacer(deps: Arc<dyn PacerDeps>, pauses: Arc<Mutex<Vec<Pause>>>) -> Pacer {
	Pacer { deps, pauses, full: AtomicBool::new(false) }
}

/// `Math.round` of a share as a percentage: halves go up.
fn percent(used: f64) -> i64 {
	(used * 100.0 + 0.5).floor() as i64
}

impl Pacer {
	/// True when the latest reading had at least one full account; false before any reading or after a failed one.
	pub fn account_full(&self) -> bool {
		self.full.load(Ordering::SeqCst)
	}

	/// Waits while every account is full.
	pub async fn pace(&self) {
		loop {
			let accounts = match self.deps.usage().await.and_then(|json| read_accounts(&json, PACER_PROVIDER)) {
				Ok(accounts) => accounts,
				Err(error) => {
					self.full.store(false, Ordering::SeqCst);
					self.deps.log(&format!("quota pacer: omp usage failed ({error}); running the trial unpaced"));
					return;
				}
			};
			self.full.store(accounts.iter().any(is_full), Ordering::SeqCst);
			let Some(pause) = pause_for(&accounts, self.deps.now()) else { return };
			let who = pause
				.accounts
				.iter()
				.map(|a| format!("{} {} {}%", a.account, span_id(a.window), percent(a.used)))
				.collect::<Vec<_>>()
				.join(", ");
			let until = iso(pause.until).unwrap_or_default();
			self.deps.log(&format!(
				"quota pacer: every account is at {}% or more ({who}); waiting until {until}",
				PAUSE_AT * 100.0
			));
			let wait = (pause.until - self.deps.now()).max(0);
			self.pauses.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(pause);
			self.deps.sleep(wait).await;
		}
	}
}
