//! The reviewer's notes (0678, 0679): the thread tool `review_note` takes one finding with the code it is about and
//! accepts it only when the committed file reads that way at that line; accepted notes are kept in the thread's folder
//! as `review.json` (`{head, notes: [Card]}`). They replace hunk's comments: the review's count comes from this file
//! and the thread's brief shows them as cards.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::frontier::brief::model::{Card, Proof, RanProof, Severity, Verdict, WorksRow, WorksState};
use crate::frontier::server::task_file::task_dir;
use crate::frontier::server::tools::Args;
use crate::frontier::server::worktrees::git_out;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;

#[cfg(test)]
#[path = "review_notes_tests.rs"]
mod tests;

/// The tool's name.
pub const REVIEW_NOTE: &str = "review_note";

/// `<thread dir>/review.json`: the notes of the review of `head`, and what showed it works or doesn't (0706).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewNotes {
	pub head: String,
	pub notes: Vec<Card>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub works: Vec<WorksRow>,
}

/// A review's notes counted: all of them, and the blocking ones (P0 and P1, 0453).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoteCounts {
	pub notes: usize,
	pub blocking: usize,
}

/// The thread's `review.json`.
pub fn review_file(project_id: &str, task_id: &str) -> PathBuf {
	task_dir(project_id, task_id).join("review.json")
}

impl ReviewNotes {
	/// The notes stored at `file`; none when it isn't there or isn't readable.
	pub fn read(file: &Path) -> Self {
		std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
	}

	/// Stores the notes at `file`, atomically.
	pub fn write(&self, file: &Path) -> Result<()> {
		write_atomically(file, serde_json::to_string_pretty(self)?, None)
	}

	/// The notes counted, all and blocking.
	pub fn counts(&self) -> NoteCounts {
		NoteCounts {
			notes: self.notes.len(),
			blocking: self.notes.iter().filter(|card| card.severity.blocking()).count(),
		}
	}

	/// The notes at `file` when they are of `head`; a fresh, empty review of `head` otherwise.
	pub fn of_head(file: &Path, head: &str) -> Self {
		let notes = Self::read(file);
		if notes.head == head { notes } else { Self { head: head.to_owned(), ..Self::default() } }
	}

	/// What ended the review (0706): the first check or try that failed.
	pub fn ended(&self) -> Option<&str> {
		self.works.iter().find(|row| row.state == WorksState::Failed).map(|row| row.label.as_str())
	}

	/// Records that `what` doesn't work: its failed row and the one P0 card `It doesn't work: <what> failed`, proven by
	/// the command's tail and not for publishing (0706).
	pub fn failed(&mut self, row: WorksRow, proof: RanProof) {
		self.notes.push(Card {
			id: format!("c{}", self.notes.len() + 1),
			severity: Severity::P0,
			file: String::new(),
			line: 0,
			quote: String::new(),
			text: format!("It doesn't work: {} failed", row.label),
			verdict: Verdict::Skip,
			proof: Proof { sha: self.head.clone(), ok: true },
			edited: None,
			published: None,
			ran: Some(proof),
		});
		self.works.push(row);
	}
}

/// Runs of whitespace as one space, the ends trimmed: how a quote and the file's lines are compared.
pub fn collapse(text: &str) -> String {
	text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn short(sha: &str) -> &str {
	&sha[..sha.len().min(7)]
}

/// Whether the lines of `text` that start at `line` (1-based) hold `quote`: as many lines as the quote has, whitespace
/// collapsed.
pub fn reads(text: &str, line: usize, quote: &str) -> bool {
	let wanted = collapse(quote);
	if wanted.is_empty() || line == 0 {
		return false;
	}
	let count = quote.lines().filter(|line| !line.trim().is_empty()).count().max(1);
	let window: Vec<&str> = text.lines().skip(line - 1).take(count).collect();
	collapse(&window.join("\n")).contains(&wanted)
}

/// The lines around `line` for a refused note: `Lines a–b are:` and each as `<n>: <text>`.
fn around(text: &str, line: usize) -> String {
	let total = text.lines().count();
	let from = line.saturating_sub(2).max(1);
	let to = (line + 2).min(total.max(from));
	let shown: Vec<String> = text
		.lines()
		.enumerate()
		.skip(from - 1)
		.take(to + 1 - from)
		.map(|(index, text)| format!("{}: {text}", index + 1))
		.collect();
	format!("Lines {from}–{to} are:\n{}", shown.join("\n"))
}

/// One note's arguments, read and checked against the committed file at `head`: the card, or the tool's refusal.
pub async fn checked_card(cwd: &Path, head: &str, args: &Args<'_>, number: usize) -> Result<Card> {
	let file = args.text("file")?;
	let quote = args.text("quote")?;
	let text = args.text("text")?;
	let line = line_of(args.values, REVIEW_NOTE)?;
	let severity = match args.text("severity")?.trim() {
		"P0" => Severity::P0,
		"P1" => Severity::P1,
		"P2" => Severity::P2,
		other => return Err(Error::user(format!("{REVIEW_NOTE}: severity is P0, P1 or P2, not `{other}`."))),
	};
	let verdict = match args.optional("verdict").as_deref().map(str::trim) {
		None | Some("post") => Verdict::Post,
		Some("question") => Verdict::Question,
		Some("skip") => Verdict::Skip,
		Some(other) => {
			return Err(Error::user(format!("{REVIEW_NOTE}: verdict is post, question or skip, not `{other}`.")));
		}
	};
	let Some(content) = git_out_raw(cwd, head, &file).await? else {
		return Err(Error::user(format!("{REVIEW_NOTE}: {file} isn't in the commit {}.", short(head))));
	};
	if !reads(&content, line, &quote) {
		return Err(Error::user(format!(
			"{REVIEW_NOTE}: {file}:{line} at {} doesn't read that. {}",
			short(head),
			around(&content, line)
		)));
	}
	Ok(Card {
		id: format!("c{number}"),
		severity,
		file,
		line,
		quote,
		text,
		verdict,
		proof: Proof { sha: head.to_owned(), ok: true },
		edited: None,
		published: None,
		ran: None,
	})
}

/// `line` as a positive whole number, given as a number or as its digits.
fn line_of(values: &Map<String, Value>, tool: &str) -> Result<usize> {
	let parsed = match values.get("line") {
		Some(Value::Number(number)) => number.as_u64(),
		Some(Value::String(digits)) => digits.trim().parse::<u64>().ok(),
		_ => None,
	};
	parsed
		.filter(|line| *line >= 1)
		.and_then(|line| usize::try_from(line).ok())
		.ok_or_else(|| Error::user(format!("{tool}: line is the 1-based line number in the file.")))
}

/// The file's text at `head`, untrimmed (`git show <head>:<file>`); `None` when the commit has no such file.
pub(super) async fn git_out_raw(cwd: &Path, head: &str, file: &str) -> Result<Option<String>> {
	use crate::shared::git::{GitOptions, git_async};
	let run = git_async(cwd, &["show", &format!("{head}:{file}")], 30_000, GitOptions::default()).await?;
	Ok((run.code == Some(0)).then_some(run.stdout))
}

/// HEAD of the worktree at `cwd`.
pub async fn head_of(cwd: &Path) -> Result<String> {
	git_out(cwd, &["rev-parse", "HEAD"])
		.await?
		.filter(|head| !head.is_empty())
		.ok_or_else(|| Error::user(format!("{REVIEW_NOTE}: {} has no commit to read.", cwd.display())))
}
