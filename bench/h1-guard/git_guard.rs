//! omp's `bash.gitGuard` as H1 reads it: a REIMPLEMENTATION, the one part of `omp-verdicts.ts` that isn't omp's own
//! code, until `git-guard-check` has measured the real guard in the VM (its lines then replace this for the lines it
//! ran).
//!
//! omp 18.20.2 enforces the git guard inside its embedded shell, not in the bash tool's approval: with
//! `bash.gitGuard: true` the tool passes `PI_GIT_GUARD=1` to the native shell (`exec/bash-executor.ts:833-835`) and that
//! shell's `git` builtin refuses the command when it runs (`exec/settings.ts:155-166`). The builtin lives in
//! pi-natives' Rust binary and exports no check that doesn't run the command, so H1 reads its rules from the setting's
//! description and the refusal messages in `pi_natives.win32-x64-baseline.node`. It first tells whose checkout git is
//! in. In a checkout of the session's own (a linked worktree from `git worktree add`, or an ovnfs checkout) the forms
//! below run after the guard saves the uncommitted changes they would discard, refused when saving fails or the
//! command recurses into submodules, and stash forms that could touch another session's entry stay refused (every
//! worktree shares one stash list) [inference from the messages]. In a repository's main checkout, which other
//! sessions may share, it refuses:
//! - `git stash`, except `stash list`, `stash show` and `stash create`;
//! - `git reset --hard`, and `git reset` to another commit (`HEAD~1`, a sha, a branch); bare `git reset` and
//!   `git reset [--] <path>` (unstaging) run;
//! - `git checkout`/`git switch` to another branch or commit, always; `git checkout -- <path>` and `git restore <path>`
//!   outside a merge or rebase conflict (H1's pane is in none); `--force`/`--discard-changes` and whole-tree pathspecs
//!   always; `git restore --staged <path>` (unstaging) runs;
//! - any of those when `--git-dir`, `--work-tree`, `GIT_DIR`, `GIT_WORK_TREE` or `GIT_INDEX_FILE` points git elsewhere.
//!
//! This reimplements the main-checkout rules, as H1's lines run in one (`git-guard-check`'s fixture at C:\work\demo).
//! Only a word that is exactly `git` reaches the builtin [inference]: `git.exe`, a full path to git, and git inside
//! `pwsh -c`/`cmd /c` start the external binary, which the embedded shell doesn't guard. Telling a commit from a path
//! in `git reset <x>` needs the repository; here only revision-shaped words count as commits (H1's corpus resets all
//! carry `--hard`, so this never decides a corpus line).

use std::sync::LazyLock;

use regex::Regex;

use crate::lib::args::BenchUsageError;
use crate::lib::jsonl::strict_json_lines;
use jocasta::shared::errors::Result;

const GUARDED: [&str; 5] = ["stash", "reset", "checkout", "switch", "restore"];
const GLOBAL_WITH_VALUE: [&str; 7] =
	["-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env", "--exec-path"];

static REDIRECTING_ENV: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^(GIT_DIR|GIT_WORK_TREE|GIT_INDEX_FILE)=").expect("a fixed pattern"));
static ASSIGNMENT: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*=").expect("a fixed pattern"));
static REVISION: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r"^(?:(?:HEAD|ORIG_HEAD|FETCH_HEAD|MERGE_HEAD|@)(?:[~^][0-9]*|@\{[^}]*\})+|[0-9a-f]{7,40}|(?:origin|upstream)/\S+)$",
	)
	.expect("a fixed pattern")
});
static GIT_WORD: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?i)^(?:git|git\.exe|.*[\\/]git(?:\.exe)?)$").expect("a fixed pattern"));

/// Why the git guard refuses this argv, or `None` when it lets it run (or it isn't the `git` builtin).
pub fn git_guard_refusal(argv: &[impl AsRef<str>]) -> Option<String> {
	let argv: Vec<&str> = argv.iter().map(AsRef::as_ref).collect();
	let mut i = 0;
	let mut redirected = false;
	while i < argv.len() && ASSIGNMENT.is_match(argv[i]) {
		if REDIRECTING_ENV.is_match(argv[i]) {
			redirected = true;
		}
		i += 1;
	}
	if argv.get(i) != Some(&"git") {
		return None;
	}
	i += 1;
	while i < argv.len() {
		let word = argv[i];
		if !word.starts_with('-') {
			break;
		}
		let name = word.split('=').next().unwrap_or(word);
		if name == "--git-dir" || name == "--work-tree" {
			redirected = true;
		}
		if GLOBAL_WITH_VALUE.contains(&word) {
			i += 1;
		}
		i += 1;
	}
	let sub = *argv.get(i)?;
	if !GUARDED.contains(&sub) {
		return None;
	}
	let refusal = subcommand_refusal(sub, &argv[i + 1..])?;
	Some(if redirected { format!("git {sub} with git pointed elsewhere") } else { refusal })
}

fn subcommand_refusal(sub: &str, rest: &[&str]) -> Option<String> {
	let before = rest.iter().position(|word| *word == "--").map_or(rest, |at| &rest[..at]);
	let mut positionals = before.iter().filter(|word| !word.starts_with('-'));
	match sub {
		"stash" => (!matches!(rest.first(), Some(&("list" | "show" | "create")))).then(|| "git stash".to_owned()),
		"reset" => {
			if rest.contains(&"--hard") {
				return Some("git reset --hard".to_owned());
			}
			positionals.any(|word| REVISION.is_match(word)).then(|| "git reset to another commit".to_owned())
		}
		"checkout" | "switch" => (!rest.is_empty()).then(|| format!("git {sub}")),
		"restore" => {
			let staged = rest.contains(&"--staged") || rest.contains(&"-S");
			let worktree = rest.contains(&"--worktree") || rest.contains(&"-W");
			(!(rest.is_empty() || (staged && !worktree))).then(|| "git restore".to_owned())
		}
		_ => None,
	}
}

/// The first refusal over a command's segments (omp's tokenizer splits them), or `None`.
pub fn git_guard_refusal_of(segments: &[Vec<String>]) -> Option<String> {
	segments.iter().find_map(|segment| git_guard_refusal(segment))
}

/// The words after which a wrapper's command line starts.
fn wrapper_flags(program: &str) -> Option<&'static [&'static str]> {
	match program {
		"pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" => Some(&["-c", "-command"]),
		"cmd" | "cmd.exe" => Some(&["/c", "/k"]),
		_ => None,
	}
}

/// omp's tokenizer: a command's segments, each a list of words. It asks omp (through the oracle), so it can fail.
pub type Tokenize<'a> = &'a dyn Fn(&str) -> Result<Vec<Vec<String>>>;

/// Whether every segment is `cd`, `echo`, git or a wrapper around such a line; the first git argv met goes in `git`.
fn visit(
	segments: &[Vec<String>],
	nested: bool,
	tokenize: Tokenize<'_>,
	git: &mut Option<Vec<String>>,
) -> Result<bool> {
	for segment in segments {
		let first = segment.first().map_or("", String::as_str);
		if first == "cd" || first == "echo" {
			continue;
		}
		if GIT_WORD.is_match(first) {
			git.get_or_insert_with(|| segment.clone());
			continue;
		}
		let Some(flags) = wrapper_flags(&first.to_lowercase()) else { return Ok(false) };
		if nested {
			return Ok(false);
		}
		let at =
			segment.iter().enumerate().position(|(i, word)| i > 0 && flags.contains(&word.to_lowercase().as_str()));
		let Some(at) = at else { return Ok(false) };
		if !visit(&tokenize(&segment[at + 1..].join(" "))?, true, tokenize, git)? {
			return Ok(false);
		}
	}
	Ok(true)
}

/// The first git argv of a line made only of git (any spelling: `git`, `git.exe`, a path to git, `git -C …`, also
/// inside `pwsh -c`/`cmd /c`), `cd` and `echo`, or `None` for any other line. `tokenize` is omp's tokenizer.
pub fn git_only_argv(command: &str, tokenize: Tokenize<'_>) -> Result<Option<Vec<String>>> {
	let mut git = None;
	let only = visit(&tokenize(command)?, false, tokenize, &mut git)?;
	Ok(if only { git } else { None })
}

/// The git subcommand of a git argv, past its global options (`-C <path>`, `-c <k=v>`, …).
pub fn git_subcommand(argv: &[impl AsRef<str>]) -> String {
	let mut i = 1;
	while i < argv.len() {
		let word = argv[i].as_ref();
		if !word.starts_with('-') {
			return word.to_owned();
		}
		if GLOBAL_WITH_VALUE.contains(&word) {
			i += 1;
		}
		i += 1;
	}
	String::new()
}

/// One measured line of `git-guard-check.jsonl` as `omp-verdicts.ts` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredGitGuardLine {
	pub command: String,
	/// The guard's refusal message, or `None` when omp's shell ran the line with the guard on.
	pub refusal: Option<String>,
}

/// Reads `git-guard-check.jsonl`: by id, in first-seen order, the lines that were measured (a line whose fixture
/// failed is left out). `omp-verdicts.ts` is the reader of record; this one keeps the format's tests beside its writer.
pub fn parse_measured_git_guard(text: &str, file: &str) -> Result<Vec<(String, MeasuredGitGuardLine)>> {
	let mut lines: Vec<(String, MeasuredGitGuardLine)> = Vec::new();
	for line in strict_json_lines(text, file, false) {
		let line = line?;
		let row = &line.row;
		let refusal = match row.get("refusal") {
			Some(serde_json::Value::Null) => Some(None),
			Some(serde_json::Value::String(text)) => Some(Some(text.clone())),
			_ => None,
		};
		let (Some(id), Some(command), Some(measured), Some(refusal)) = (
			row.get("id").and_then(serde_json::Value::as_str),
			row.get("command").and_then(serde_json::Value::as_str),
			row.get("measured").and_then(serde_json::Value::as_bool),
			refusal,
		) else {
			return Err(BenchUsageError::err(format!(
				"{}: needs id, command, measured and refusal (string or null)",
				line.where_
			)));
		};
		if !measured {
			continue;
		}
		let measured_line = MeasuredGitGuardLine { command: command.to_owned(), refusal };
		match lines.iter_mut().find(|(known, _)| known == id) {
			Some(slot) => slot.1 = measured_line,
			None => lines.push((id.to_owned(), measured_line)),
		}
	}
	Ok(lines)
}

#[cfg(test)]
#[path = "git_guard_tests.rs"]
mod tests;
