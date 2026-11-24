//! The `jocasta` command's words, kept apart from what each runs so `entry` loads only the one it routes to (PM-11).
//! clap parses them (0637): its help and its usage errors replace the hand-written usage text; the words and flags, the
//! internal roles kept out of the help and the exit codes (0140) stay as decided.

use clap::{Parser, Subcommand};

/// The `jocasta` command's words (0387, 0467, 0468, 0472, 0544): open the work surface, `setup [--refresh <file>]`,
/// `uninstall`, `diagnose`, `project …`, `thread …`, `worktree leftover|remove <path>`, `--version`;
/// and, not listed in the help, `push-hook <remote> <address>`, the `pre-push` hook git runs in every Jocasta pane
/// (0613), and `pane-sleep <text>`, what a sleeping pane runs (0676).
/// Every other word of
/// herdr's (the fork built in, `crates/herdr`) is
/// [`Route::Herdr`]: herdr's own entry reads the command line. `--help` and a word that is no command are not routes:
/// [`route`] returns clap's error for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
	Surface,
	Setup { refresh: Option<String> },
	Uninstall,
	Diagnose,
	Project { args: Vec<String> },
	Thread { args: Vec<String> },
	Worktree { args: Vec<String> },
	PushHook { args: Vec<String> },
	PaneSleep { args: Vec<String> },
	Version,
	Herdr,
}

/// herdr's words (`crates/herdr/src/cli.rs`'s `maybe_run` and `lib.rs`'s `main`); `worktree` is shared with Jocasta's
/// and decided by [`herdrs`].
const HERDR_WORDS: [&str; 16] = [
	"server",
	"client",
	"api",
	"status",
	"completion",
	"completions",
	"config",
	"workspace",
	"tab",
	"notification",
	"agent",
	"terminal",
	"pane",
	"integration",
	"session",
	"plugin",
];

/// herdr's flags that may come first. `--notification-activate` is how Windows starts the exe again when he clicks one
/// of herdr's toasts (its `main` handles it before anything else).
const HERDR_FLAGS: [&str; 5] = ["--session", "--default-config", "--skill", "-V", "--notification-activate"];

/// Whether the command line is herdr's. `worktree leftover` and `worktree remove [<path>]` are Jocasta's leftover
/// worktrees (0544); herdr's `worktree list|create|open|remove --workspace <id>` are its.
fn herdrs(args: &[String]) -> bool {
	let word = |at: usize| args.get(at).map(String::as_str);
	match word(0) {
		None => false,
		Some("worktree") => match word(1) {
			Some("leftover") => false,
			Some("remove") => word(2).is_some_and(|next| next.starts_with('-')),
			_ => true,
		},
		Some(first) => {
			HERDR_WORDS.contains(&first)
				|| HERDR_FLAGS.contains(&first)
				|| first.split_once('=').is_some_and(|(flag, _)| flag == "--session")
		}
	}
}

#[derive(Parser)]
#[command(
	name = "jocasta",
	about = "Open the work surface (herdr session `jocasta`; starts the Jocasta service)",
	override_usage = "jocasta [COMMAND]",
	disable_help_subcommand = true,
	disable_version_flag = true,
	args_conflicts_with_subcommands = true,
	after_help = "herdr's commands, built in (`jocasta <command> --help`): server, status, config, session, workspace, \
	              worktree, tab, pane, agent, terminal, notification, plugin, integration, api, completion; and \
	              `--session <name>`, `--default-config`, `--skill`, `-V` (herdr's version)."
)]
struct Cli {
	/// Print the versions of Jocasta, omp and herdr
	#[arg(short = 'v', long = "version")]
	version: bool,
	#[command(subcommand)]
	command: Option<Word>,
}

#[derive(Subcommand)]
enum Word {
	/// Show what Jocasta adds to omp, herdr, Windows Terminal and mise; write after `y`
	Setup {
		/// Reset one Jocasta-owned file to the shipped version, keeping a backup: herdr.toml or jocasta.json
		#[arg(long, value_name = "FILE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
		refresh: Option<String>,
	},
	/// Remove what install and setup added, after `y`
	Uninstall,
	/// A read-only pane that explains what went wrong from Jocasta's log
	Diagnose,
	/// list | add <path> | remove <name|id>
	#[command(disable_help_flag = true)]
	Project {
		#[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
		args: Vec<String>,
	},
	/// Threads through the running service; changes ask you first
	///
	/// new <project> "<brief>" [--title <t>] [--done-when "<cmd>"]... [--heavy] | list [<project>] | status <thread> |
	/// send <thread> "<text>" | stop <thread> | finish <thread> merge|pr
	#[command(disable_help_flag = true)]
	Thread {
		#[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
		args: Vec<String>,
	},
	/// Leftover worktrees through the running service; changes ask you first (herdr's own `worktree list|create|open|
	/// remove --workspace <id>` stay herdr's)
	///
	/// leftover | remove <path>
	#[command(disable_help_flag = true)]
	Worktree {
		#[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
		args: Vec<String>,
	},
	/// Git's `pre-push` hook in every Jocasta pane (0613)
	#[command(name = "push-hook", hide = true, disable_help_flag = true)]
	PushHook {
		#[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "ARGS")]
		args: Vec<String>,
	},
	/// What a sleeping pane runs instead of omp (0676)
	#[command(name = "pane-sleep", hide = true, disable_help_flag = true)]
	PaneSleep {
		#[arg(trailing_var_arg = true, allow_hyphen_values = true, value_name = "TEXT")]
		args: Vec<String>,
	},
}

impl From<Word> for Route {
	fn from(word: Word) -> Self {
		match word {
			Word::Setup { refresh } => Self::Setup { refresh },
			Word::Uninstall => Self::Uninstall,
			Word::Diagnose => Self::Diagnose,
			Word::Project { args } => Self::Project { args },
			Word::Thread { args } => Self::Thread { args },
			Word::Worktree { args } => Self::Worktree { args },
			Word::PushHook { args } => Self::PushHook { args },
			Word::PaneSleep { args } => Self::PaneSleep { args },
		}
	}
}

/// Routes argv, the program's name left out (0387). herdr's words come first ([`Route::Herdr`]); `Err` is clap's help
/// (`--help`, `-h`: exit 0, to stdout) or its usage error (exit 2, to stderr, 0140); `error.print()` writes either and
/// `error.exit_code()` is its code.
pub fn route(args: &[String]) -> Result<Route, clap::Error> {
	if herdrs(args) {
		return Ok(Route::Herdr);
	}
	let cli = Cli::try_parse_from(std::iter::once("jocasta").chain(args.iter().map(String::as_str)))?;
	Ok(match (cli.version, cli.command) {
		(true, _) => Route::Version,
		(false, None) => Route::Surface,
		(false, Some(word)) => word.into(),
	})
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
