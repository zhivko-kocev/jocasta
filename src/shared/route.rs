//! The `jocasta` command's words, kept apart from what each runs so `entry` loads only the one it routes to (PM-11).
//! clap parses them (0637): its help and its usage errors replace the hand-written usage text; the words and flags, the
//! internal roles kept out of the help and the exit codes (0140) stay as decided.

use clap::{Parser, Subcommand};

/// The `jocasta` command's words (0387, 0467, 0468, 0472, 0544, 0694, 0700, 0721): open the work surface,
/// `setup [--refresh <file> | --yes]`, `uninstall`, `diagnose`, `status --json`, `project …`, `thread …`,
/// `worktree leftover|remove <path>`, `view <brief-id>`, `--version`;
/// and, not listed in the help, `push-hook <remote> <address>`, the `pre-push` hook git runs in every Jocasta pane
/// (0613). `--help` and a word that is no command are not routes: [`route`] returns clap's error for them. The shell is
/// Jocasta's own (0683), so herdr's words are gone; a sleeping pane is drawn by the shell (0689), so no role runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
	Surface,
	Setup { refresh: Option<String>, yes: bool },
	Uninstall,
	Diagnose,
	Status,
	Project { args: Vec<String> },
	Thread { args: Vec<String> },
	Worktree { args: Vec<String> },
	PushHook { args: Vec<String> },
	View { id: String },
	Version,
}

#[derive(Parser)]
#[command(
	name = "jocasta",
	about = "Open the work surface (Jocasta's own shell; starts the Jocasta service)",
	override_usage = "jocasta [COMMAND]",
	disable_help_subcommand = true,
	disable_version_flag = true,
	args_conflicts_with_subcommands = true
)]
struct Cli {
	/// Print the versions of Jocasta and omp
	#[arg(short = 'v', long = "version")]
	version: bool,
	#[command(subcommand)]
	command: Option<Word>,
}

#[derive(Subcommand)]
enum Word {
	/// Show what Jocasta adds to omp, Windows Terminal and mise; write after `y`
	Setup {
		/// Reset one Jocasta-owned file to the shipped version, keeping a backup: jocasta.json
		#[arg(long, value_name = "FILE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
		refresh: Option<String>,
		/// Write every pending change without asking
		#[arg(short = 'y', long, conflicts_with = "refresh")]
		yes: bool,
	},
	/// Remove what install and setup added, after `y`
	Uninstall,
	/// A read-only pane that explains what went wrong from Jocasta's log
	Diagnose,
	/// The running service's counts, today's spend and open threads as one JSON line; exit 3 when it isn't running
	Status {
		/// Print JSON (the only form)
		#[arg(long, required = true)]
		json: bool,
	},
	/// Open a stored brief in its own window
	View {
		/// The brief's id (`thread-…`, `pr-…`, `morning-…`, `show-…`)
		#[arg(value_name = "BRIEF-ID")]
		id: String,
	},
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
	/// Leftover worktrees through the running service; changes ask you first
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
}

impl From<Word> for Route {
	fn from(word: Word) -> Self {
		match word {
			Word::Setup { refresh, yes } => Self::Setup { refresh, yes },
			Word::Uninstall => Self::Uninstall,
			Word::Diagnose => Self::Diagnose,
			Word::Status { .. } => Self::Status,
			Word::Project { args } => Self::Project { args },
			Word::Thread { args } => Self::Thread { args },
			Word::Worktree { args } => Self::Worktree { args },
			Word::PushHook { args } => Self::PushHook { args },
			Word::View { id } => Self::View { id },
		}
	}
}

/// Routes argv, the program's name left out (0387). `Err` is clap's help (`--help`, `-h`: exit 0, to stdout) or its
/// usage error (exit 2, to stderr, 0140); `error.print()` writes either and `error.exit_code()` is its code.
pub fn route(args: &[String]) -> Result<Route, clap::Error> {
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
