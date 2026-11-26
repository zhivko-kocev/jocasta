use clap::error::ErrorKind;

use super::*;
use crate::shared::push_hook::PUSH_HOOK_ROLE;

fn words(args: &[&str]) -> Vec<String> {
	args.iter().map(|word| (*word).to_owned()).collect()
}

fn go(args: &[&str]) -> Route {
	route(&words(args)).unwrap_or_else(|error| panic!("{args:?}: {error}"))
}

fn refused(args: &[&str]) -> clap::Error {
	match route(&words(args)) {
		Ok(route) => panic!("{args:?} routed to {route:?}"),
		Err(error) => error,
	}
}

/// What `jocasta --help` prints.
fn help() -> String {
	refused(&["--help"]).to_string()
}

#[test]
fn the_jocasta_commands_words_0387_0467_0468_0472_0544_0694_anything_else_is_a_usage_error() {
	assert_eq!(go(&[]), Route::Surface);
	assert_eq!(go(&["setup"]), Route::Setup { refresh: None, yes: false });
	assert_eq!(
		go(&["setup", "--refresh", "herdr.toml"]),
		Route::Setup { refresh: Some("herdr.toml".to_owned()), yes: false }
	);
	assert_eq!(go(&["setup", "--yes"]), Route::Setup { refresh: None, yes: true });
	assert_eq!(go(&["setup", "-y"]), Route::Setup { refresh: None, yes: true });
	assert_eq!(go(&["uninstall"]), Route::Uninstall);
	assert_eq!(go(&["diagnose"]), Route::Diagnose);
	assert_eq!(go(&["status", "--json"]), Route::Status);
	assert_eq!(go(&["project", "list"]), Route::Project { args: words(&["list"]) });
	assert_eq!(go(&["thread", "list"]), Route::Thread { args: words(&["list"]) });
	assert_eq!(go(&["worktree", "remove", "x"]), Route::Worktree { args: words(&["remove", "x"]) });
	assert_eq!(go(&["--version"]), Route::Version);
	assert_eq!(go(&["-v"]), Route::Version);
	assert_eq!(refused(&["--help"]).kind(), ErrorKind::DisplayHelp);
	assert_eq!(refused(&["-h"]).kind(), ErrorKind::DisplayHelp);
	// Retired words (0387) are no command and are named; stray arguments to a command are invocation errors.
	for args in [&["guard"][..], &["mcp"], &["pane-check"], &["harness"], &["memory", "import"], &["fix it"]] {
		let error = refused(args);
		assert_eq!(error.kind(), ErrorKind::InvalidSubcommand, "{args:?}");
		assert!(error.to_string().contains(&format!("'{}'", args[0])), "{error}");
	}
	for args in [
		&["setup", "now"][..],
		&["setup", "--refresh"],
		&["setup", "--refresh", ""],
		&["setup", "--yes", "--refresh", "jocasta.json"],
		&["uninstall", "--yes"],
		&["diagnose", "now"],
		&["status"],
		&["service"],
		&["--version", "setup"],
		&["--version", "now"],
		&["-x"],
	] {
		let error = refused(args);
		assert!(error.use_stderr(), "{args:?}");
		assert_eq!(error.exit_code(), 2, "{args:?}");
	}
}

#[test]
fn help_is_clap_text_that_lists_the_words_and_exits_0_to_stdout() {
	let error = refused(&["--help"]);
	assert!(!error.use_stderr());
	assert_eq!(error.exit_code(), 0);
	let text = help();
	assert!(text.contains("Usage: jocasta"), "{text}");
	for word in ["setup", "uninstall", "diagnose", "status", "project", "thread", "worktree", "--version", "--help"] {
		assert!(text.contains(word), "{word} missing from {text}");
	}
	assert!(refused(&["setup", "--help"]).to_string().contains("--refresh <FILE>"));
	assert!(refused(&["setup", "--help"]).to_string().contains("--yes"));
	assert!(refused(&["status", "--help"]).to_string().contains("--json"));
}

#[test]
fn project_thread_and_worktree_pass_every_remaining_word_on_unchanged_help_flags_too() {
	assert_eq!(go(&["project"]), Route::Project { args: vec![] });
	assert_eq!(go(&["project", "--help"]), Route::Project { args: words(&["--help"]) });
	assert_eq!(go(&["thread", "-h"]), Route::Thread { args: words(&["-h"]) });
	assert_eq!(
		go(&[
			"thread",
			"new",
			"demo",
			"fix it",
			"--title",
			"T",
			"--done-when",
			"bun test",
			"--done-when",
			"x",
			"--heavy"
		]),
		Route::Thread {
			args: words(&[
				"new",
				"demo",
				"fix it",
				"--title",
				"T",
				"--done-when",
				"bun test",
				"--done-when",
				"x",
				"--heavy"
			])
		}
	);
	assert_eq!(go(&["worktree", "leftover"]), Route::Worktree { args: words(&["leftover"]) });
	assert_eq!(go(&["thread", "finish", "x", "squash"]), Route::Thread { args: words(&["finish", "x", "squash"]) });
}

#[test]
fn the_command_word_routes_to_the_hook_with_gits_arguments_and_stays_out_of_the_help_0613() {
	assert_eq!(PUSH_HOOK_ROLE, "push-hook");
	assert!(!help().contains("push-hook"));
	assert_eq!(
		go(&["push-hook", "origin", "https://example.org/a.git"]),
		Route::PushHook { args: words(&["origin", "https://example.org/a.git"]) }
	);
	assert_eq!(go(&["push-hook"]), Route::PushHook { args: vec![] });
}

#[test]
fn the_service_and_plugin_words_are_hidden_from_the_help() {
	let text = help();
	for line in text.lines().map(str::trim_start) {
		assert!(!line.starts_with("service ") && !line.starts_with("plugin "), "{text}");
	}
}

#[test]
fn herdrs_words_are_gone_and_the_worktree_word_is_jocastas_0683() {
	assert_eq!(go(&["worktree", "leftover"]), Route::Worktree { args: words(&["leftover"]) });
	assert_eq!(go(&["worktree", "remove", "/tmp/w"]), Route::Worktree { args: words(&["remove", "/tmp/w"]) });
	assert_eq!(go(&["worktree", "remove"]), Route::Worktree { args: words(&["remove"]) });
	assert_eq!(go(&["--version"]), Route::Version);
	for args in [&["server"][..], &["pane", "list"], &["plugin", "today"], &["-V"], &["--session", "t"]] {
		assert!(route(&words(args)).is_err(), "{args:?} is no command now");
	}
	assert!(!help().contains("herdr"), "{}", help());
}
