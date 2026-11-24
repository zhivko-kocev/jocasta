//! The review prompt a finished task's thread gets (0394, 0425, 0453, 0460, 0546, 0678): what the reviewer is given and
//! how it hands its findings in. A finding goes in through the thread's `review_note` tool, which checks the quoted code
//! against the committed file (`review_notes.rs`); hunk, which this prompt used to drive, is gone (0679).

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::frontier::server::wording::REVIEW_LEAD;
use crate::shared::install::shipped_config_dir;

#[cfg(test)]
#[path = "review_prompt_tests.rs"]
mod tests;

/// What the review turn hands omp's reviewer, and the checks Jocasta ran for it (0453, 0459, 0473, 0539, 0540, 0546).
#[derive(Debug, Clone, Default)]
pub struct ReviewContext {
	/// The approved plan's file, when the thread's session has one.
	pub plan: Option<PathBuf>,
	/// The project's rules files that exist: the worktree's `AGENTS.md`, the project's `AGENTS.local.md` (0449).
	pub rules: Vec<PathBuf>,
	/// The gate reply of the checks Jocasta's runner ran on the task's tree (the project's mise check and each
	/// `done_when`, 0542, 0546): `""` when every gate passed or none ran.
	pub gates: Option<String>,
	/// Jocasta's runner ran the project's own check; otherwise the turn runs the project's tests itself.
	pub checked: bool,
	/// The thread's brief from its task file (0540), when recorded.
	pub brief: Option<String>,
	/// The thread's `report_result` as one line (0539); `Some(None)` when it handed in none; `None` without a task file.
	pub report: Option<Option<String>>,
	/// The facts block code computed for the range (0685): after the lead line; `None` adds nothing.
	pub facts: Option<String>,
}

/// JavaScript's `\s` (and `trim`): these characters, and none of Rust's others (U+0085).
const JS_SPACE: &str =
	r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

fn is_js_space(c: char) -> bool {
	matches!(
		c,
		'\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
			..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
	)
}

static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("[{JS_SPACE}]+")).expect("a valid pattern"));

fn slash(path: &Path) -> String {
	path.to_string_lossy().replace('\\', "/")
}

/// Text on one line: spaces collapsed, `«»` as `"`.
fn flat(text: &str) -> String {
	let one = SPACES.replace_all(text, " ").replace(['«', '»'], "\"");
	one.trim_matches(is_js_space).to_owned()
}

/// What the thread is asked once its task ended (0394, 0425, 0453, 0460, 0546, 0678). Jocasta's runner already ran the
/// project's checks (`gates`): their failures are given as gates, a pass is not mentioned; a project without known
/// checks has the turn run its tests as one bash command and end on a `TESTS: pass|fail — <command>` (or `TESTS:
/// none`) line the service checks against the session's bash results (`sessions.ts` `checkedTests`). Then its own
/// `verify` skill when it has one, then omp's reviewer subagent on the range with the approved plan, the project's
/// rules and its lessons, told to report only material findings, then at most three design findings judged by the
/// shipped design rubric (0611; priority P2 and led by `Design:`, so they never count as blocking). Every finding goes
/// in through `review_note` with its code quoted exactly: a note whose quote the file doesn't hold is refused.
pub fn review_prompt(range: &str, context: &ReviewContext) -> String {
	let mut given = Vec::new();
	if let Some(brief) = context.brief.as_deref().filter(|brief| !brief.is_empty()) {
		given.push(format!("his brief «{}», to check the diff against what he asked for", flat(brief)));
	}
	if let Some(plan) = context.plan.as_deref().filter(|plan| !plan.as_os_str().is_empty()) {
		given.push(format!("the approved plan ({}), to check the diff against it", slash(plan)));
	}
	if !context.rules.is_empty() {
		let rules: Vec<String> = context.rules.iter().map(|rule| slash(rule)).collect();
		given.push(format!("the project's rules ({})", rules.join(", ")));
	}
	given.push("its lessons in skill://lessons (a change that contradicts a lesson is a finding citing it)".to_owned());
	match &context.report {
		Some(None) => given.push("the fact that the thread handed in no report".to_owned()),
		Some(Some(report)) if !report.is_empty() => {
			given.push(format!("the thread's own report «{}» (its claims, not proof)", flat(report)));
		}
		_ => {}
	}
	let ran_tests = !context.checked;
	let failed = context.gates.as_deref().filter(|gates| !gates.is_empty()).map(|gates| {
		format!(
			"Jocasta ran the task's checks on this tree and they failed; give the reviewer these gates too:\n\n{gates}\n\n"
		)
	});
	let rubric = slash(&shipped_config_dir(None).join("design-rubric.md"));
	let mut parts: Vec<String> = vec![REVIEW_LEAD.to_owned()];
	if let Some(facts) = context.facts.as_deref().filter(|facts| !facts.is_empty()) {
		parts.push(format!("\n\n{facts}\n\n"));
	}
	parts.extend(failed);
	if ran_tests {
		parts.push(
			"First run the project's tests the way its AGENTS.md or package scripts say, as one bash command;"
				.to_owned(),
		);
		parts.push("when the project has a `verify` skill, read skill://verify and run it too. Don't fix failures in this turn.".to_owned());
	} else {
		parts.push(
			"When the project has a `verify` skill, read skill://verify and run it. Don't fix failures in this turn."
				.to_owned(),
		);
	}
	parts.extend([
		format!("Then run the reviewer subagent on `git diff {range}` in this worktree. In its task give it {},", given.join(", ")),
		"and tell it: report only material findings; prefer one strong finding over several weak ones; then, after those,".to_owned(),
		format!("read the design rubric ({rubric}) and report at most three design findings judged by it,"),
		"each naming the code this change could delete where there is some. A design finding is P2 with its text".to_owned(),
		"led by \"Design:\"; design findings never block.".to_owned(),
		"Hand every finding in with the `review_note` tool, one call each: `file` and `line` (the line in the committed file),".to_owned(),
		"`quote` (the code at that line, copied exactly: Jocasta checks it against the file and refuses a note whose quote it doesn't find),".to_owned(),
		"`severity` (P0 breaks the task or the build, P1 is a likely bug, both block; P2 is a smaller or design finding and doesn't),".to_owned(),
		"`text` (one sentence for the finding, then why) and `verdict` (`post` when it belongs in a PR comment, `question`, or `skip`).".to_owned(),
		"Run `review_note --help` for the exact arguments. Don't change code in this turn. No findings: hand in nothing and say so.".to_owned(),
	]);
	if ran_tests {
		parts.push("End your reply with one line on its own: `TESTS: pass — <the test command you ran>`,".to_owned());
		parts.push(
			"`TESTS: fail — <the test command you ran>`, or `TESTS: none` when the project has no tests to run."
				.to_owned(),
		);
	}
	parts.join(" ")
}
