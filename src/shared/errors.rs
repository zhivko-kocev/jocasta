//! Jocasta's errors. `Error::User` is the TypeScript `UserError`: a refusal Jocasta explains in one line after
//! `jocasta: `, and the CLI exits with its code (0140: 2 = invocation error). Every other variant is a fault.

/// Every error a Jocasta function returns.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// A refusal he reads, in one line; the CLI exits with `exit_code`.
	#[error("{message}")]
	User { message: String, exit_code: u8 },
	#[error(transparent)]
	Io(#[from] std::io::Error),
	#[error(transparent)]
	Json(#[from] serde_json::Error),
	#[error(transparent)]
	Sqlite(#[from] rusqlite::Error),
	#[error(transparent)]
	Http(#[from] reqwest::Error),
	/// A fault with its own message.
	#[error("{0}")]
	Other(String),
}

/// Jocasta's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
	/// A refusal with the invocation-error exit code, 2 (0140).
	pub fn user(message: impl Into<String>) -> Self {
		Self::User { message: message.into(), exit_code: 2 }
	}

	/// A refusal with its own exit code.
	pub fn user_with_code(message: impl Into<String>, exit_code: u8) -> Self {
		Self::User { message: message.into(), exit_code }
	}

	/// A fault with its own message.
	pub fn other(message: impl Into<String>) -> Self {
		Self::Other(message.into())
	}

	/// Whether this is a refusal he reads.
	pub fn is_user(&self) -> bool {
		matches!(self, Self::User { .. })
	}
}

/// `context` on a fallible value: the error becomes `Error::Other("<what>: <error>")`.
pub trait Context<T> {
	fn context(self, what: impl std::fmt::Display) -> Result<T>;
}

impl<T, E: std::fmt::Display> Context<T> for std::result::Result<T, E> {
	fn context(self, what: impl std::fmt::Display) -> Result<T> {
		self.map_err(|error| Error::Other(format!("{what}: {error}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_user_error_exits_2_unless_given_a_code() {
		let Error::User { exit_code, message } = Error::user("no such project") else { panic!("not a user error") };
		assert_eq!((exit_code, message.as_str()), (2, "no such project"));
		assert!(matches!(Error::user_with_code("x", 3), Error::User { exit_code: 3, .. }));
	}

	#[test]
	fn context_prefixes_the_cause() {
		let failed: std::result::Result<(), &str> = Err("disk full");
		assert_eq!(
			failed.context("writing projects.yaml").unwrap_err().to_string(),
			"writing projects.yaml: disk full"
		);
	}
}
