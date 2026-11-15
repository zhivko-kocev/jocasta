//! Windows Credential Manager (0430): generic credentials read and written through `advapi32.dll`'s `CredReadW`,
//! `CredWriteW` and `CredDeleteW` (windows-sys). The calendar's secret iCal URL lives in target `jocasta/calendar-ical`;
//! the env var `JOCASTA_CALENDAR_ICAL_SECRET` stays a fallback (0301), taken out of this process's environment at the
//! service's start so no child (herdr and its panes, whiteboard, omp) inherits it (SEC-5). Off Windows there is no
//! store: reads find nothing and writes refuse.

use std::sync::Mutex;

use crate::shared::env;
use crate::shared::errors::{Error, Result};

/// The calendar secret's Credential Manager target (0430).
pub const CALENDAR_CREDENTIAL: &str = "jocasta/calendar-ical";

/// The env var the calendar secret may still come from (0301, now a fallback).
pub const CALENDAR_ENV: &str = "JOCASTA_CALENDAR_ICAL_SECRET";

/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`.
#[cfg(windows)]
const MAX_BLOB: usize = 5 * 512;

/// A blob as text: UTF-8 as written here, or UTF-16LE as `cmdkey /pass:` and the Control Panel store passwords.
#[cfg_attr(not(windows), allow(dead_code))]
fn blob_text(bytes: &[u8]) -> String {
	// Every odd-numbered byte is zero: the mark of UTF-16LE text in the Latin range.
	let utf16 =
		bytes.len() >= 2 && bytes.len().is_multiple_of(2) && bytes.iter().skip(1).step_by(2).all(|byte| *byte == 0);
	if utf16 {
		let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
		String::from_utf16_lossy(&units)
	} else {
		String::from_utf8_lossy(bytes).into_owned()
	}
}

/// The secret of generic credential `target`, or `None` when there is none (or no store on this platform).
#[cfg(windows)]
pub fn read_credential(target: &str) -> Option<String> {
	use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW};
	let name = wide(target);
	let mut found: *mut CREDENTIALW = std::ptr::null_mut();
	// SAFETY: `name` is NUL-terminated and `found` is a valid out pointer.
	if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 || found.is_null() {
		return None;
	}
	// SAFETY: CredReadW returned a credential, valid until the CredFree below; its blob is `size` bytes long.
	let text = unsafe {
		let credential = &*found;
		let size = credential.CredentialBlobSize as usize;
		let text = if size == 0 || credential.CredentialBlob.is_null() {
			None
		} else {
			Some(blob_text(std::slice::from_raw_parts(credential.CredentialBlob, size)))
		};
		CredFree(found.cast());
		text
	};
	text.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
}

/// The secret of generic credential `target`, or `None` when there is none (or no store on this platform).
#[cfg(not(windows))]
pub fn read_credential(_target: &str) -> Option<String> {
	None
}

/// A NUL-terminated UTF-16 string for a `LPCWSTR`.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
	text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Writes `secret` as generic credential `target` for this user (replacing one already there).
#[cfg(windows)]
pub fn write_credential(target: &str, secret: &str) -> Result<()> {
	use windows_sys::Win32::Security::Credentials::{
		CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
	};
	let blob = secret.as_bytes();
	if blob.is_empty() || blob.len() > MAX_BLOB {
		return Err(Error::other(format!("the secret must be 1–{MAX_BLOB} bytes")));
	}
	let mut name = wide(target);
	let mut user = wide("jocasta");
	let credential = CREDENTIALW {
		Type: CRED_TYPE_GENERIC,
		TargetName: name.as_mut_ptr(),
		CredentialBlobSize: blob.len() as u32,
		CredentialBlob: blob.as_ptr().cast_mut(),
		Persist: CRED_PERSIST_LOCAL_MACHINE,
		UserName: user.as_mut_ptr(),
		..CREDENTIALW::default()
	};
	// SAFETY: every pointer in `credential` is valid for the call (CredWriteW copies what it keeps).
	if unsafe { CredWriteW(&credential, 0) } == 0 {
		return Err(Error::other(format!("Credential Manager refused to save {target}")));
	}
	Ok(())
}

/// Writes `secret` as generic credential `target`: there is no store off Windows, so this refuses.
#[cfg(not(windows))]
pub fn write_credential(_target: &str, _secret: &str) -> Result<()> {
	Err(Error::other("Credential Manager is only on Windows"))
}

/// Removes generic credential `target`; `false` when there was none.
#[cfg(windows)]
pub fn delete_credential(target: &str) -> bool {
	use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
	let name = wide(target);
	// SAFETY: `name` is NUL-terminated.
	unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) != 0 }
}

/// Removes generic credential `target`; `false` when there was none (off Windows there never is).
#[cfg(not(windows))]
pub fn delete_credential(_target: &str) -> bool {
	false
}

/// The env fallback, once taken out of the environment.
static CALENDAR_ENV_TAKEN: Mutex<Option<String>> = Mutex::new(None);

/// Moves `JOCASTA_CALENDAR_ICAL_SECRET` out of the environment into this module (SEC-5) and returns it: every process
/// started afterwards (herdr's server and so every pane, whiteboard, omp) inherits an environment without it. The
/// service and the `jocasta` command call it before they start anything; later calls return the value taken.
pub fn take_calendar_env() -> Option<String> {
	let mut taken = CALENDAR_ENV_TAKEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	if let Some(value) = env::var(CALENDAR_ENV) {
		let trimmed = value.trim();
		*taken = (!trimmed.is_empty()).then(|| trimmed.to_owned());
		remove_from_environment();
	}
	taken.clone()
}

#[cfg(not(test))]
fn remove_from_environment() {
	// SAFETY: called at the service's and the command's start, before they start a thread or a program, as
	// `std::env::remove_var` requires.
	unsafe { std::env::remove_var(CALENDAR_ENV) };
}

/// Under test the variable lives in the overlay of [`env`], so that is where it goes.
#[cfg(test)]
fn remove_from_environment() {
	env::overlay::set(CALENDAR_ENV, None);
}

/// The calendar's secret iCal URL (0430): Credential Manager first, else the env var (0301); `None` when neither.
pub fn calendar_secret() -> Option<String> {
	read_credential(CALENDAR_CREDENTIAL).or_else(take_calendar_env)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn the_calendars_env_fallback_leaves_the_environment_once_taken_sec_5() {
		let mut temp = temp_jocasta();
		temp.set_var(CALENDAR_ENV, Some("https://calendar.example/secret.ics"));
		assert_eq!(take_calendar_env().as_deref(), Some("https://calendar.example/secret.ics"));
		assert_eq!(env::var(CALENDAR_ENV), None);
		assert_eq!(take_calendar_env().as_deref(), Some("https://calendar.example/secret.ics"));
	}

	#[test]
	fn a_blob_is_utf8_as_written_here_or_utf16le_as_cmdkey_stores_it() {
		assert_eq!(blob_text(b"https://x.example/a.ics"), "https://x.example/a.ics");
		let utf16: Vec<u8> = "pass".encode_utf16().flat_map(u16::to_le_bytes).collect();
		assert_eq!(blob_text(&utf16), "pass");
		assert_eq!(blob_text("é".as_bytes()), "é");
	}

	#[cfg(windows)]
	#[test]
	fn credential_manager_round_trip_on_a_throwaway_target_0430() {
		let target = format!("jocasta-test/platform-{}", std::process::id());
		assert_eq!(read_credential(&target), None);
		write_credential(&target, "https://calendar.example/secret.ics").unwrap();
		let read = read_credential(&target);
		delete_credential(&target);
		assert_eq!(read.as_deref(), Some("https://calendar.example/secret.ics"));
		assert_eq!(read_credential(&target), None);
	}
}
