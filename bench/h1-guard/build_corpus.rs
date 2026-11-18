//! Benchmark H1's corpus builder (0482). It takes the source lines a researcher collected from public documentation
//! without reading the guard (`h1-sources.jsonl`: `{command, class, category, source}`) and derives the frozen corpus
//! mechanically: every source line as written, plus the wrapped and respelled forms a shell user or an agent produces
//! without changing what the command does. No command is written here by hand; the derivations are the transforms below.
//!
//! ```text
//! jocasta-bench h1-guard build-corpus --sources D:/thesis/data/bench/h1-sources.jsonl --out D:/thesis/data/bench/h1-corpus.jsonl
//! ```
//!
//! Output lines: `{id, command, expected, group, variant, base, category, source}`. `expected` maps the class to the
//! guard's verdicts (must-deny → deny, must-ask → prompt, must-allow → allow). `group` is `held-out` for every
//! derived line; the `dev` group (the security review's own bypass list, which the guard's author saw) is appended
//! from `--dev <file>` when given. The corpus file's SHA-256 goes beside it in `h1-corpus.sha256`.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::process::ExitCode;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use super::evaluate::sha256_hex;
use jocasta::shared::errors::{Error, Result};

/// One collected line.
#[derive(Debug, Clone)]
pub struct SourceLine {
	pub command: String,
	/// `must-deny`, `must-ask` or `must-allow`.
	pub class: String,
	pub category: Option<String>,
	pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorpusLine {
	pub id: String,
	pub command: String,
	pub expected: &'static str,
	pub group: &'static str,
	pub variant: &'static str,
	pub base: String,
	pub category: String,
	pub source: String,
}

fn expected_of(class: &str) -> Result<&'static str> {
	match class {
		"must-deny" => Ok("deny"),
		"must-ask" => Ok("prompt"),
		"must-allow" => Ok("allow"),
		other => {
			Err(Error::other(format!("a source line's class is must-deny, must-ask or must-allow, not \"{other}\"")))
		}
	}
}

/// PowerShell cmdlets and parameters are case-insensitive: the respelled form runs the same way.
static POWERSHELL: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^(Remove-Item|Invoke-Expression|Invoke-RestMethod|Invoke-WebRequest|Get-Content|Set-Content|ri|rm|iex|irm|iwr)(?-u:\b)")
		.expect("a fixed pattern")
});
static CMD_PROGRAMS: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"^(rd|rmdir|del|erase|format|diskpart|reg|vssadmin)(?-u:\b)").expect("a fixed pattern")
});

/// The ways of writing, in the order the builder applies them.
const TRANSFORMS: [&str; 10] = [
	"as-written",
	"after-cd",
	"semicolon-chain",
	"pwsh-c",
	"cmd-c",
	"lowercase-powershell",
	"git-exe",
	"git-full-path",
	"git-C",
	"extra-spaces",
];

/// One transform of a command, or `None` when it doesn't apply.
fn apply(variant: &str, c: &str) -> Option<String> {
	let after_git = |replacement: &str| c.strip_prefix("git ").map(|rest| format!("{replacement}{rest}"));
	match variant {
		"as-written" => Some(c.to_owned()),
		"after-cd" => Some(format!("cd C:\\work\\demo && {c}")),
		"semicolon-chain" => Some(format!("echo start; {c}; echo done")),
		"pwsh-c" => (!c.contains('"')).then(|| format!("pwsh -c \"{c}\"")),
		"cmd-c" => CMD_PROGRAMS.is_match(c).then(|| format!("cmd /c \"{c}\"")),
		"lowercase-powershell" => (POWERSHELL.is_match(c) && c != c.to_lowercase()).then(|| c.to_lowercase()),
		"git-exe" => after_git("git.exe "),
		"git-full-path" => after_git("\"C:\\Program Files\\Git\\cmd\\git.exe\" "),
		"git-C" => after_git("git -C C:\\work\\demo "),
		"extra-spaces" => c.contains(' ').then(|| c.replace(' ', "  ")),
		_ => None,
	}
}

/// The source lines of a JSONL file: blank lines skipped, every other line parsed.
fn read_lines(path: &str) -> Result<Vec<SourceLine>> {
	let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
	text.split('\n')
		.map(|line| line.strip_suffix('\r').unwrap_or(line))
		.filter(|line| !line.trim().is_empty())
		.map(|line| {
			let value: Value = serde_json::from_str(line)?;
			let text_of = |field: &str| value.get(field).and_then(Value::as_str).map(str::to_owned);
			let command =
				text_of("command").ok_or_else(|| Error::other(format!("a source line has no command: {line}")))?;
			let class = text_of("class").ok_or_else(|| Error::other(format!("a source line has no class: {line}")))?;
			Ok(SourceLine { command, class, category: text_of("category"), source: text_of("source") })
		})
		.collect()
}

/// The corpus built from the collected `sources` and the `dev` list.
pub fn build_corpus(sources: &[SourceLine], dev: &[SourceLine]) -> Result<Vec<CorpusLine>> {
	let mut lines: Vec<CorpusLine> = Vec::new();
	let mut seen: HashSet<(&'static str, String)> = HashSet::new();
	let mut push = |group: &'static str, variant: &'static str, command: String, source: &SourceLine| -> Result<()> {
		if !seen.insert((group, command.clone())) {
			return Ok(());
		}
		lines.push(CorpusLine {
			id: format!("h1-{:04}", lines.len() + 1),
			command,
			expected: expected_of(&source.class)?,
			group,
			variant,
			base: source.command.clone(),
			category: source.category.clone().unwrap_or_default(),
			source: source.source.clone().unwrap_or_default(),
		});
		Ok(())
	};
	for source in sources {
		for variant in TRANSFORMS {
			if let Some(command) = apply(variant, &source.command) {
				push("held-out", variant, command, source)?;
			}
		}
	}
	for source in dev {
		push("dev", "as-written", source.command.clone(), source)?;
	}
	Ok(lines)
}

const USAGE: &str = "usage: jocasta-bench h1-guard build-corpus --sources <h1-sources.jsonl> --out <h1-corpus.jsonl> [--dev <dev.jsonl>]";

/// The value after `flag` in `argv`.
fn get<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
	let at = argv.iter().position(|arg| arg == flag)?;
	argv.get(at + 1).map(String::as_str)
}

fn build(sources: &str, out: &str, dev: Option<&str>) -> Result<()> {
	let corpus = build_corpus(&read_lines(sources)?, &dev.map(read_lines).transpose()?.unwrap_or_default())?;
	let mut text = String::new();
	for line in &corpus {
		text.push_str(&serde_json::to_string(line)?);
		text.push('\n');
	}
	fs::write(out, &text)?;
	let sha = sha256_hex(text.as_bytes());
	let stem = out.strip_suffix(".jsonl").unwrap_or(out);
	let name = out.rsplit(['\\', '/']).next().unwrap_or(out);
	fs::write(format!("{stem}.sha256"), format!("{sha}  {name}\n"))?;
	let mut by: BTreeMap<String, usize> = BTreeMap::new();
	for line in &corpus {
		*by.entry(format!("{}/{}", line.group, line.expected)).or_default() += 1;
	}
	println!("{} lines → {out}; sha256 {sha}", corpus.len());
	for (key, count) in by {
		println!("  {key}: {count}");
	}
	Ok(())
}

/// The script: `jocasta-bench h1-guard build-corpus …`. Missing flags print the usage and exit 2; any other failure
/// prints its message and exits 1 (the TypeScript's uncaught error printed Bun's stack trace).
pub fn main(argv: &[String]) -> ExitCode {
	let (sources, out) =
		(get(argv, "--sources").filter(|v| !v.is_empty()), get(argv, "--out").filter(|v| !v.is_empty()));
	let (Some(sources), Some(out)) = (sources, out) else {
		eprintln!("{USAGE}");
		return ExitCode::from(2);
	};
	match build(sources, out, get(argv, "--dev").filter(|dev| !dev.is_empty())) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("{error}");
			ExitCode::from(1)
		}
	}
}
