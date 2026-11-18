//! The `jocasta` command (0387): routing and each word live in the library's `entry` module.

fn main() -> std::process::ExitCode {
	jocasta::shared::env::use_real_environment();
	jocasta::entry::main()
}
