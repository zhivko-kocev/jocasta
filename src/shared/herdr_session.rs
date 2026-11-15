//! The herdr session Jocasta's work surface lives in.

use crate::shared::env;
use crate::shared::errors::{Error, Result};

/// The herdr session Jocasta's work surface lives in (0392): `jocasta`, or `JOCASTA_HERDR_SESSION` for live checks in
/// a throwaway session (0441).
pub fn herdr_session() -> Result<String> {
	let Some(value) = env::var_nonempty("JOCASTA_HERDR_SESSION") else { return Ok("jocasta".to_owned()) };
	if !valid_name(&value) {
		return Err(Error::user("JOCASTA_HERDR_SESSION must be letters, digits, '.', '_' or '-'"));
	}
	Ok(value)
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.
fn valid_name(value: &str) -> bool {
	let bytes = value.as_bytes();
	(1..=64).contains(&bytes.len())
		&& bytes[0].is_ascii_alphanumeric()
		&& bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;

	#[test]
	fn herdrs_session_is_jocasta_or_the_override_and_a_bad_name_is_an_invocation_error() {
		let mut temp = temp_jocasta();
		temp.set_var("JOCASTA_HERDR_SESSION", None);
		assert_eq!(herdr_session().unwrap(), "jocasta");
		temp.set_var("JOCASTA_HERDR_SESSION", Some(""));
		assert_eq!(herdr_session().unwrap(), "jocasta");
		temp.set_var("JOCASTA_HERDR_SESSION", Some("jocasta-polish"));
		assert_eq!(herdr_session().unwrap(), "jocasta-polish");
		for bad in ["../x", "-x", "a b", &"x".repeat(65)] {
			temp.set_var("JOCASTA_HERDR_SESSION", Some(bad));
			let Err(Error::User { message, exit_code }) = herdr_session() else { panic!("{bad} was accepted") };
			assert_eq!(exit_code, 2);
			assert_eq!(message, "JOCASTA_HERDR_SESSION must be letters, digits, '.', '_' or '-'");
		}
	}
}
