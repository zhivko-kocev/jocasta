//! The process environment as Jocasta reads it (Rust only; the TypeScript reads `process.env`). Every module reads
//! variables here, never through `std::env::var`, so a unit test can point the Jocasta and omp roots at its own
//! folders without changing the real environment, which other test threads read at the same time: under `cfg(test)`
//! an overlay answers first ([`crate::shared::testing`]), and [`apply_to`] hands it to the programs a test starts.

use std::ffi::OsString;

/// The variable's value, or `None` when it is unset or not Unicode.
pub fn var(name: &str) -> Option<String> {
	#[cfg(test)]
	if let Some(value) = overlay::get(name) {
		return value;
	}
	std::env::var(name).ok()
}

/// The variable's value when it is set and not empty: the TypeScript's `process.env.X || fallback`.
pub fn var_nonempty(name: &str) -> Option<String> {
	var(name).filter(|value| !value.is_empty())
}

/// Every variable of the environment a program Jocasta starts inherits, as `(name, value)` pairs.
pub fn vars() -> Vec<(OsString, OsString)> {
	#[allow(unused_mut)]
	let mut all: Vec<(OsString, OsString)> = std::env::vars_os().collect();
	#[cfg(test)]
	overlay::merge(&mut all);
	all
}

/// Hands the test overlay to a program about to start; outside tests the program inherits the real environment as
/// it is, so this does nothing.
pub fn apply_to(_command: &mut std::process::Command) {
	#[cfg(test)]
	overlay::apply(_command);
}

/// [`apply_to`] for a tokio command.
pub fn apply_to_tokio(_command: &mut tokio::process::Command) {
	#[cfg(test)]
	overlay::apply_tokio(_command);
}

#[cfg(test)]
pub(crate) mod overlay {
	use std::collections::HashMap;
	use std::ffi::OsString;
	use std::sync::{LazyLock, Mutex, MutexGuard};

	/// Each name's test value: `Some(value)` set, `None` unset.
	type Values = HashMap<String, Option<String>>;

	static VALUES: LazyLock<Mutex<Values>> = LazyLock::new(|| Mutex::new(crate::shared::testing::sandbox_values()));

	fn values() -> MutexGuard<'static, Values> {
		VALUES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	pub(crate) fn get(name: &str) -> Option<Option<String>> {
		values().get(name).cloned()
	}

	/// Sets (or with `None` unsets) a test value; returns what it was, for the caller to put back.
	pub(crate) fn set(name: &str, value: Option<String>) -> Option<Option<String>> {
		values().insert(name.to_owned(), value)
	}

	/// Puts back what [`set`] returned.
	pub(crate) fn restore(name: &str, before: Option<Option<String>>) {
		let mut values = values();
		match before {
			Some(value) => values.insert(name.to_owned(), value),
			None => values.remove(name),
		};
	}

	pub(crate) fn merge(all: &mut Vec<(OsString, OsString)>) {
		for (name, value) in values().iter() {
			all.retain(|(n, _)| n.to_str() != Some(name.as_str()));
			if let Some(value) = value {
				all.push((name.into(), value.into()));
			}
		}
	}

	pub(crate) fn apply(command: &mut std::process::Command) {
		for (name, value) in values().iter() {
			match value {
				Some(value) => command.env(name, value),
				None => command.env_remove(name),
			};
		}
	}

	pub(crate) fn apply_tokio(command: &mut tokio::process::Command) {
		for (name, value) in values().iter() {
			match value {
				Some(value) => command.env(name, value),
				None => command.env_remove(name),
			};
		}
	}
}
