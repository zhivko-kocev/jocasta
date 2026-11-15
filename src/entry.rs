//! The Rust port of `entry.ts` (0625); not yet ported (0628).

/// The `jocasta` command; until the CLI slice lands it only says so.
pub fn main() -> std::process::ExitCode {
	eprintln!("jocasta: the Rust build is not yet usable (0628)");
	std::process::ExitCode::from(2)
}
