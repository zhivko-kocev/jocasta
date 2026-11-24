//! hunk (0394, 0406): the diff pane that opens beside a thread when it finishes a task, the review prompt the thread
//! gets, and the count of its notes. hunk ships as its npm platform binary (`hunkdiff-<platform>@0.23.0`).
//!
//! hunk's live sessions go through one session daemon per loopback port (`hunk daemon serve`, default port 47657),
//! which the first hunk window starts detached in its own working folder and which outlives it (hunk
//! `src/session/broker/brokerLauncher.ts:688-708`). Jocasta's hunk uses its own port (`HUNK_PORT`), so another hunk
//! copy on the machine (another version is refused by the daemon) never breaks the note count (F-03), and Jocasta starts
//! that daemon itself from the home folder before any hunk pane opens, so no daemon sits in a worktree and blocks its
//! removal on Windows (F-02).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::frontier::server::wording::REVIEW_LEAD;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::git::{GitOptions, git_async};
use crate::shared::install::{REPO, install_dir, shipped_config_dir};
use crate::shared::poll::wait_for;
use crate::shared::process::{RunOptions, run_program, wrap_sync};

#[cfg(test)]
#[path = "fake_hunk.rs"]
pub(crate) mod fake_hunk;
#[cfg(test)]
#[path = "hunk_tests.rs"]
mod tests;

/// The loopback port of Jocasta's hunk session daemon (`HUNK_MCP_PORT`; hunk's own default is 47657).
pub const HUNK_PORT: u16 = 47658;

/// The environment of every hunk Jocasta starts, and of thread panes (their review runs hunk's session commands): the
/// daemon's port, and no update notice (bumps are deliberate, 0383; hunk `src/core/process/updateNotice.ts:17`).
pub fn hunk_env(port: u16) -> Vec<(String, String)> {
	vec![("HUNK_MCP_PORT".to_owned(), port.to_string()), ("HUNK_DISABLE_UPDATE_NOTICE".to_owned(), "1".to_owned())]
}

/// `hunk diff`'s argv for a thread's review pane: Tokyo Night like herdr and omp (0401, UX-08), watching the range.
pub fn hunk_diff_args(hunk: &[String], range: &str) -> Vec<String> {
	let mut argv = hunk.to_vec();
	argv.extend(["diff", "--theme", "tokyo-night", range, "--watch"].map(str::to_owned));
	argv
}

/// The npm package holding hunk's binary for this platform (`std::env::consts::OS`/`ARCH` name them `windows`,
/// `linux`, `macos` and `x86_64`, `aarch64`; here the Node names `win32`, `linux`, `darwin` and `x64`, `arm64`), or
/// `None` where hunk ships none.
pub fn hunk_package(platform: &str, arch: &str) -> Option<&'static str> {
	match (platform, arch) {
		("win32", "x64") => Some("hunkdiff-windows-x64"),
		("linux", "x64") => Some("hunkdiff-linux-x64"),
		("darwin", "arm64") => Some("hunkdiff-darwin-arm64"),
		_ => None,
	}
}

/// This build's platform and architecture by Node's names.
pub fn this_platform() -> (&'static str, &'static str) {
	let platform = if cfg!(windows) {
		"win32"
	} else if cfg!(target_os = "macos") {
		"darwin"
	} else {
		std::env::consts::OS
	};
	let arch = match std::env::consts::ARCH {
		"x86_64" => "x64",
		"aarch64" => "arm64",
		other => other,
	};
	(platform, arch)
}

/// The package's binary from its `package.json` `bin.hunk` (`bin/hunk.exe` on Windows, `bin/hunk` elsewhere).
fn package_binary(package_dir: &Path) -> Option<PathBuf> {
	let manifest: Value =
		serde_json::from_str(&std::fs::read_to_string(package_dir.join("package.json")).ok()?).ok()?;
	let bin = match manifest.get("bin")? {
		Value::String(bin) => bin.as_str(),
		Value::Object(map) => map.get("hunk")?.as_str()?,
		_ => return None,
	};
	(!bin.is_empty()).then(|| package_dir.join(bin))
}

/// hunk's binary: `<install dir>/hunk/hunk[.exe]` beside `jocasta.exe` (the service finds its install the same way),
/// else `node_modules/hunkdiff-<platform>`'s in a source run; `None` when none is there.
pub fn find_hunk() -> Option<PathBuf> {
	if let Some(install) = install_dir(None) {
		let shipped = install.join("hunk").join(if cfg!(windows) { "hunk.exe" } else { "hunk" });
		if shipped.exists() {
			return Some(shipped);
		}
	}
	let (platform, arch) = this_platform();
	let package = hunk_package(platform, arch)?;
	let binary = package_binary(&Path::new(REPO).join("node_modules").join(package))?;
	binary.exists().then_some(binary)
}

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
	/// The thread's `report_result` as one line (0539); `Some(None)` when it handed in none (the TypeScript's `null`);
	/// `None` without a task file.
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

/// A regex whose `\w` and `\b` are ASCII's, as JavaScript's are.
fn ascii(pattern: &str) -> Regex {
	let pattern = pattern.replace(r"\w", r"(?-u:\w)").replace(r"\b", r"(?-u:\b)");
	Regex::new(&pattern).expect("a valid pattern")
}

static WORDS_ONLY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_./:=-]+$").expect("a valid pattern"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("[{JS_SPACE}]+")).expect("a valid pattern"));

fn slash(path: &Path) -> String {
	path.to_string_lossy().replace('\\', "/")
}

/// Text on one line: spaces collapsed, `«»` as `"`.
fn flat(text: &str) -> String {
	let one = SPACES.replace_all(text, " ").replace(['«', '»'], "\"");
	one.trim_matches(is_js_space).to_owned()
}

/// What the thread is asked once its hunk pane is open (0394, 0425, 0453, 0460, 0546). Jocasta's runner already ran
/// the project's checks (`gates`): their failures are given as gates, a pass is not mentioned; a project without known
/// checks has the turn run its tests as one bash command and end on a `TESTS: pass|fail — <command>` (or `TESTS:
/// none`) line the service checks against the session's bash results (`sessions.ts` `checkedTests`). Then its own
/// `verify` skill when it has one, then omp's reviewer subagent on the range with the approved plan, the project's
/// rules and its lessons, told to report only material findings, then at most three design findings judged by the
/// shipped design rubric (0611; posted as `[P3] Design: …`, so they never count as blocking); every finding goes into the live hunk session with
/// one `comment apply` batch, each summary led by its priority (`[P0]`…`[P3]`). The shipped platform binary doesn't
/// carry hunk's `hunk-review` skill (`hunk skill path` fails there), so the commands and the batch shape are spelled
/// out here. hunk's argv becomes one command line for the thread's bash tool: odd words double-quoted, `\` as `/`.
pub fn review_prompt(hunk: &[String], range: &str, context: &ReviewContext) -> String {
	let cli = hunk
		.iter()
		.map(|word| {
			if WORDS_ONLY.is_match(word) {
				word.clone()
			} else {
				format!("\"{}\"", word.replace('\\', "/").replace('"', "\\\""))
			}
		})
		.collect::<Vec<_>>()
		.join(" ");
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
		"each naming the code this change could delete where there is some. Post a design finding as [P3] with its summary".to_owned(),
		"led by \"Design:\"; design findings never block.".to_owned(),
		"A hunk window beside you shows the same diff. Post every finding in it as one batch: pipe".to_owned(),
		"`{\"comments\":[{\"filePath\":\"<path>\",\"newLine\":<n>,\"summary\":\"[P<priority>] <one sentence>\",\"rationale\":\"<why>\",\"author\":\"reviewer\"}]}`".to_owned(),
		format!("into `{cli} session comment apply --repo . --stdin` (each summary starts with the reviewer's priority, [P0] to [P3];"),
		format!("use \"oldLine\" for removed lines; check the files and hunks first with `{cli} session review --repo . --json`)."),
		format!("Don't run `{cli} diff` yourself, and don't change code in this turn. No findings: post nothing and say so."),
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

/// A review's notes in hunk: how many, and how many the reviewer marked blocking (P0 or P1, 0453).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewNotes {
	pub notes: usize,
	pub blocking: usize,
}

static BLOCKING: LazyLock<Regex> = LazyLock::new(|| ascii(r"(?i)^\s*\[?P[01]\b"));

/// `Date.parse` of a note's `createdAt`: ISO 8601 with an offset, without one (local time) or a bare date (UTC).
fn parse_date(text: &str) -> Option<i64> {
	use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
	if let Ok(at) = DateTime::parse_from_rfc3339(text) {
		return Some(at.timestamp_millis());
	}
	if let Ok(day) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
		return Some(day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
	}
	let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
	Some(Local.from_local_datetime(&naive).earliest()?.timestamp_millis())
}

/// The notes in `hunk session comment list --json`'s output (`{comments: [{createdAt, summary, ...}]}`, hunk
/// `src/session/types.ts:176-183`) made at `since` (epoch ms) or later: the live session keeps earlier reviews' notes
/// (`--watch` keeps the pane across tasks). A note without a time counts; one whose summary starts with `[P0]` or
/// `[P1]` is blocking.
pub fn count_comments(json: &str, since: i64) -> Result<ReviewNotes> {
	let parsed: Value = serde_json::from_str(json)?;
	let Some(comments) = parsed.get("comments").and_then(Value::as_array) else {
		return Err(Error::other("hunk's comment list has no comments array"));
	};
	let recent: Vec<&Value> = comments
		.iter()
		.filter(|comment| {
			let at = comment.get("createdAt").and_then(Value::as_str).and_then(parse_date);
			at.is_none_or(|at| at >= since)
		})
		.collect();
	let blocking = recent
		.iter()
		.filter(|comment| {
			comment.get("summary").and_then(Value::as_str).is_some_and(|summary| BLOCKING.is_match(summary))
		})
		.count();
	Ok(ReviewNotes { notes: recent.len(), blocking })
}

/// How long a hunk command may take before it counts as failed.
const HUNK_COMMAND_MS: i64 = 20_000;

fn home_dir() -> PathBuf {
	std::env::home_dir().unwrap_or_default()
}

/// Runs hunk from the home folder (never inside a worktree) with Jocasta's hunk environment; stdout, or an error.
async fn run_hunk(hunk: &[String], args: &[&str], port: u16) -> Result<String> {
	let mut vars: HashMap<String, String> =
		env::vars().into_iter().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))).collect();
	vars.extend(hunk_env(port));
	let argv: Vec<&str> = hunk.iter().map(String::as_str).chain(args.iter().copied()).collect();
	let run = run_program(
		&argv,
		RunOptions {
			cwd: Some(home_dir()),
			env: Some(vars),
			timeout_ms: Some(HUNK_COMMAND_MS),
			..RunOptions::default()
		},
	)
	.await?;
	let what = format!("hunk {}", args.iter().take(3).copied().collect::<Vec<_>>().join(" "));
	if run.timed_out || run.signal.is_some() {
		return Err(Error::other(format!("{what} took over {} s", HUNK_COMMAND_MS / 1000)));
	}
	if run.code != Some(0) {
		let code = run.code.map_or_else(|| "null".to_owned(), |code| code.to_string());
		let said = if run.stderr.trim().is_empty() { run.stdout.trim() } else { run.stderr.trim() };
		return Err(Error::other(format!("{what} failed ({code}): {said}")));
	}
	Ok(run.stdout)
}

/// Counts the worktree's live notes made since `since` (`hunk session comment list --repo <worktree> --json`).
pub async fn list_comments(hunk: &[String], worktree: &Path, since: i64, port: u16) -> Result<ReviewNotes> {
	let worktree = worktree.to_string_lossy();
	let json = run_hunk(hunk, &["session", "comment", "list", "--repo", &worktree, "--json"], port).await?;
	count_comments(&json, since)
}

/// What a reviewed diff did (0453, 0610): its size, the test files it changed, and the test files that weaken the tests:
/// an added skip/only, a removed test, fewer assertions. A review whose proof names any of the three fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProof {
	pub added: usize,
	pub deleted: usize,
	pub files: usize,
	pub test_files: Vec<String>,
	/// Test files that add a skip/only mark.
	pub skipped: Vec<String>,
	/// Test files that lost a test declaration no test in the diff replaces (a deleted file counts). Absent in proofs stored before 0610.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub removed_tests: Option<Vec<String>>,
	/// Changed test files that lost more assertions than they gained. Absent in proofs stored before 0610.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub fewer_assertions: Option<Vec<String>>,
}

/// A test file by its path: under a `test`/`tests`/`__tests__`/`spec` folder, or named like one.
static TEST_FILE: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r"(?i)(^|/)(tests?|__tests__|specs?)/|[._-](test|spec)\.[^/]+$|(^|/)test_[^/]+\.py$|_test\.(go|py|rb|exs?)$",
	)
	.expect("a valid pattern")
});

/// An added line that skips or focuses tests: `.skip(`/`.only(`, `xit(`, `fit(`, pytest/unittest skips, `#[ignore]`, `t.Skip(`.
static SKIP_MARK: LazyLock<Regex> = LazyLock::new(|| {
	ascii(
		r"\b(it|test|describe|context|suite)\.(skip|only|todo)\s*\(|\b(xit|xdescribe|xtest|fit|fdescribe)\s*\(|@pytest\.mark\.(skip|xfail)|@unittest\.skip|#\[ignore\]|\bt\.Skip(Now|f)?\s*\(",
	)
});

/// A line that is only a comment (`//`, `#`, `*`, `/*`, `--`): never a test or an assertion, so commenting one out removes it.
static COMMENT_LINE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*(?://|#(?:[^\[]|$)|\*|/\*|--)").expect("a valid pattern"));

/// The test declarations at the start of a changed line, each with its name in group 1. The TypeScript's one pattern
/// is these in order (its `\1` back-reference becomes one alternative per quote, of which the text's own quote picks
/// one); the first three match only from the line's start, so `t.Run(` is tried last, as the alternation does.
struct TestDeclaration {
	/// `it`/`test`/`specify` with a string, in `"`, `'` or a backtick.
	js: [Regex; 3],
	py: Regex,
	go: Regex,
	sub: Regex,
}

static TEST_DECLARATION: LazyLock<TestDeclaration> = LazyLock::new(|| {
	let js = |quote: &str| {
		ascii(&format!(r"^\s*(?:it|test|specify|scenario|example)(?:\.\w+)*\s*\(?\s*{quote}(.+?){quote}"))
	};
	TestDeclaration {
		js: [js("\""), js("'"), js("`")],
		py: ascii(r"^\s*(?:async\s+)?def\s+(test\w*)\s*\("),
		go: ascii(r"^\s*func\s+((?:Test|Benchmark|Fuzz|Example)\w*)\s*\("),
		sub: ascii(r#"\bt\.Run\(\s*"(.+?)""#),
	}
});

impl TestDeclaration {
	/// The test's name when `text` declares one.
	fn name(&self, text: &str) -> Option<String> {
		let caps = self
			.js
			.iter()
			.find_map(|re| re.captures(text))
			.or_else(|| self.py.captures(text))
			.or_else(|| self.go.captures(text))
			.or_else(|| self.sub.captures(text))?;
		Some(caps[1].to_owned())
	}
}

/// Rust's test attribute (`#[test]`, `#[tokio::test]`, `#[test_case(..)]`): the declaration is its next `fn`.
static RUST_TEST_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| ascii(r"^\s*#\[(?:\w+::)*test(?:_case)?\b"));

/// The `fn` a Rust test attribute introduces.
static RUST_FN: LazyLock<Regex> = LazyLock::new(|| ascii(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"));

/// What may sit between a Rust test attribute and its `fn`: more attributes and blank lines.
static RUST_BETWEEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:#\[|$)").expect("a valid pattern"));

/// An assertion on a changed line: `expect(`/`expect {`, `assert*(` and `assert_eq!(` (any language), a bare Python
/// `assert `, Go's `t.Error*`/`t.Fatal*`/`t.Fail` and testify's `require.X(`, RSpec's `.should` and `is_expected`.
static ASSERTION: LazyLock<Regex> = LazyLock::new(|| {
	ascii(
		r"\b(?:expect|assert[A-Za-z_]*|should)\s*[.(!{]|^\s*assert\s|\bt\.(?:Errorf?|Fatalf?|Fail|FailNow)\s*\(|\brequire\.[A-Za-z]+\s*\(|\.should\b|\bis_expected\b",
	)
});

static DIFF_PARTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^diff --git ").expect("a valid pattern"));
static NEW_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\+\+\+ b/(.+)$").expect("a valid pattern"));
static OLD_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^--- a/(.+)$").expect("a valid pattern"));
static NEW_SIDE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\+\+\+ b/").expect("a valid pattern"));

/// One test file's changed lines, as the proof reads them from a `-U0` diff.
struct TestFileChange {
	file: String,
	/// Names of the test declarations removed and added.
	removed_tests: Vec<String>,
	added_tests: Vec<String>,
	removed_assertions: usize,
	added_assertions: usize,
	skips: bool,
	/// The file is gone after the change: its tests count as removed, its assertions are not "fewer".
	gone: bool,
}

/// The changes of one `diff --git` part (its header text without the `diff --git ` start); `None` for a binary file.
fn test_file_change(part: &str) -> Option<TestFileChange> {
	let file = NEW_NAME.captures(part).or_else(|| OLD_NAME.captures(part)).map(|caps| caps[1].to_owned());
	let hunks = part.find("\n@@");
	let (file, hunks) = (file?, hunks?);
	let mut change = TestFileChange {
		file,
		removed_tests: Vec::new(),
		added_tests: Vec::new(),
		removed_assertions: 0,
		added_assertions: 0,
		skips: false,
		gone: !NEW_SIDE.is_match(part),
	};
	let mut rust_attribute: Option<char> = None;
	for line in part[hunks..].split('\n') {
		let sign = match line.chars().next() {
			Some(sign @ ('+' | '-')) => sign,
			_ => continue,
		};
		let text = &line[1..];
		if sign == '+' && SKIP_MARK.is_match(text) {
			change.skips = true;
		}
		if COMMENT_LINE.is_match(text) {
			continue;
		}
		if RUST_TEST_ATTRIBUTE.is_match(text) {
			rust_attribute = Some(sign);
			continue;
		}
		let tests = if sign == '+' { &mut change.added_tests } else { &mut change.removed_tests };
		if rust_attribute == Some(sign)
			&& let Some(caps) = RUST_FN.captures(text)
		{
			tests.push(caps[1].to_owned());
		}
		if !RUST_BETWEEN.is_match(text) {
			rust_attribute = None;
		}
		if let Some(name) = TEST_DECLARATION.name(text) {
			tests.push(name);
		}
		if ASSERTION.is_match(text) {
			if sign == '+' {
				change.added_assertions += 1;
			} else {
				change.removed_assertions += 1;
			}
		}
	}
	Some(change)
}

/// The test files that lost tests: a removed declaration counts unless a declaration of the same name is added anywhere
/// in the diff (a moved or renamed file) or, failing that, the same file adds another declaration for it (a test
/// renamed in place).
fn files_with_removed_tests(changes: &[TestFileChange]) -> Vec<String> {
	let mut added_by_name: HashMap<&str, VecDeque<usize>> = HashMap::new();
	for (at, change) in changes.iter().enumerate() {
		for name in &change.added_tests {
			added_by_name.entry(name).or_default().push_back(at);
		}
	}
	let mut spare: HashMap<&str, i64> =
		changes.iter().map(|change| (change.file.as_str(), change.added_tests.len() as i64)).collect();
	let mut lost: HashMap<&str, i64> = HashMap::new();
	for change in changes {
		for name in &change.removed_tests {
			match added_by_name.get_mut(name.as_str()).and_then(VecDeque::pop_front) {
				Some(matched) => {
					let file = changes[matched].file.as_str();
					let left = spare.get(file).copied().unwrap_or(1);
					spare.insert(file, left - 1);
				}
				None => *lost.entry(change.file.as_str()).or_insert(0) += 1,
			}
		}
	}
	// Names matched elsewhere used up added declarations of their own; what is left in a file replaces its own losses.
	changes
		.iter()
		.filter(|change| {
			lost.get(change.file.as_str()).copied().unwrap_or(0) > spare.get(change.file.as_str()).copied().unwrap_or(0)
		})
		.map(|change| change.file.clone())
		.collect()
}

/// One git call of the proof (CO-10): 30 s at most, off the event loop.
const PROOF_GIT_MS: i64 = 30_000;

/// Test files per `git diff` call, well inside Windows' 32K command line.
const PROOF_FILES_PER_CALL: usize = 100;

/// The proof's facts for `range` (`<trunk>...<branch>`) in `cwd`, from git without holding the event loop: one
/// `--numstat` for the size and the files, and `-U0` diffs of the test files in chunks (split at their `diff --git`
/// headers) for added skip/only marks, removed tests and fewer assertions (0610; the patterns above are heuristics:
/// they read changed lines only and no language's syntax). `None` when git can't tell.
pub async fn review_proof(cwd: &Path, range: &str) -> Result<Option<ReviewProof>> {
	// Paths as they are (no C-quoting of non-ASCII), so the test files' names match and split back out.
	let unquoted = ["-c", "core.quotePath=false"];
	let numstat_args = [unquoted.as_slice(), &["diff", "--numstat", "--no-renames", range]].concat();
	let numstat = git_async(cwd, &numstat_args, PROOF_GIT_MS, GitOptions::default()).await?;
	if numstat.code != Some(0) {
		return Ok(None);
	}
	let (mut added, mut deleted) = (0, 0);
	let mut paths: Vec<String> = Vec::new();
	for line in numstat.stdout.split('\n').filter(|line| !line.is_empty()) {
		let mut fields = line.split('\t');
		added += fields.next().and_then(|plus| plus.parse::<usize>().ok()).unwrap_or(0);
		deleted += fields.next().and_then(|minus| minus.parse::<usize>().ok()).unwrap_or(0);
		paths.push(fields.collect::<Vec<_>>().join("\t"));
	}
	let test_files: Vec<String> = paths.iter().filter(|path| TEST_FILE.is_match(path)).cloned().collect();
	let mut changes: Vec<TestFileChange> = Vec::new();
	for files in test_files.chunks(PROOF_FILES_PER_CALL) {
		let mut args: Vec<&str> = unquoted.to_vec();
		args.extend(["diff", "-U0", "--no-color", "--no-renames", range, "--"]);
		args.extend(files.iter().map(String::as_str));
		let diff = git_async(cwd, &args, PROOF_GIT_MS, GitOptions::default()).await?;
		changes.extend(DIFF_PARTS.split(&diff.stdout).filter_map(test_file_change));
	}
	let skipped: HashSet<&str> =
		changes.iter().filter(|change| change.skips).map(|change| change.file.as_str()).collect();
	let weaker: HashSet<&str> = changes
		.iter()
		.filter(|change| !change.gone && change.removed_assertions > change.added_assertions)
		.map(|change| change.file.as_str())
		.collect();
	let among = |set: &HashSet<&str>| test_files.iter().filter(|file| set.contains(file.as_str())).cloned().collect();
	Ok(Some(ReviewProof {
		added,
		deleted,
		files: paths.len(),
		skipped: among(&skipped),
		removed_tests: Some(files_with_removed_tests(&changes)),
		fewer_assertions: Some(among(&weaker)),
		test_files,
	}))
}

/// The proof as the review event says it: `+12/−3 in 2 files, test files changed: a.test.ts, adds skip/only in a.test.ts,
/// removes tests in b.test.ts, fewer assertions in a.test.ts`.
pub fn proof_said(proof: &ReviewProof) -> String {
	let plural = if proof.files == 1 { "" } else { "s" };
	let mut parts = vec![format!("+{}/−{} in {} file{plural}", proof.added, proof.deleted, proof.files)];
	if proof.test_files.is_empty() {
		parts.push("no test files changed".to_owned());
	} else {
		parts.push(format!("test files changed: {}", proof.test_files.join(", ")));
	}
	if !proof.skipped.is_empty() {
		parts.push(format!("adds skip/only in {}", proof.skipped.join(", ")));
	}
	if let Some(removed) = proof.removed_tests.as_ref().filter(|files| !files.is_empty()) {
		parts.push(format!("removes tests in {}", removed.join(", ")));
	}
	if let Some(fewer) = proof.fewer_assertions.as_ref().filter(|files| !files.is_empty()) {
		parts.push(format!("fewer assertions in {}", fewer.join(", ")));
	}
	parts.join(", ")
}

/// Whether Jocasta's hunk daemon answers its health check (hunk `brokerLauncher.ts:588`).
async fn daemon_up(port: u16) -> bool {
	let Ok(client) = reqwest::Client::builder().no_proxy().build() else { return false };
	client
		.get(format!("http://127.0.0.1:{port}/health"))
		.timeout(Duration::from_millis(1_000))
		.send()
		.await
		.is_ok_and(|response| response.status().is_success())
}

/// Jocasta's hunk session daemon, started detached from the home folder when it isn't up (F-02), waiting up to 5 s for
/// it. hunk's own lock and health check keep a second daemon from starting on the same port. `started` gets the pid of
/// a daemon started here, so Jocasta's close can end it (0434, 0486).
pub async fn ensure_hunk_daemon(hunk: &[String], port: u16, started: Option<&(dyn Fn(u32) + Sync)>) -> Result<()> {
	if daemon_up(port).await {
		return Ok(());
	}
	let Some((command, args)) = hunk.split_first() else { return Ok(()) };
	let mut daemon = std::process::Command::new(command);
	daemon
		.args(args)
		.args(["daemon", "serve"])
		.current_dir(home_dir())
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	env::apply_to(&mut daemon);
	daemon.envs(hunk_env(port));
	// In a job or group of its own (0641) with no kill on close, so it outlives this process as before; ending it
	// across runs stays the snapshot walk by pid, since no job handle survives a restart. CREATE_NO_WINDOW |
	// CREATE_NEW_PROCESS_GROUP: no console window and its own group.
	// A program that can't start has no pid to report; the wait below then reports it.
	if let Ok(child) = wrap_sync(daemon, 0x0800_0200).spawn()
		&& let Some(started) = started
	{
		started(child.id());
	}
	// The first look after 100 ms: the daemon was just started.
	tokio::time::sleep(Duration::from_millis(100)).await;
	if wait_for(|| async { Ok(daemon_up(port).await) }, 4_900, 100).await? {
		return Ok(());
	}
	Err(Error::other(format!("hunk's session daemon didn't start on 127.0.0.1:{port}")))
}
