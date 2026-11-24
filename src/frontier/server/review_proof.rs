//! What a reviewed diff did (0453, 0610): its size, the test files it changed, and the test files that weaken the
//! tests. Moved out of the hunk module when hunk was removed (0679); the review's notes now come from `review_notes.rs`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::shared::errors::Result;
use crate::shared::git::{GitOptions, git_async};

#[cfg(test)]
#[path = "review_proof_tests.rs"]
mod tests;

/// A regex whose `\w` and `\b` are ASCII's, as JavaScript's are.
fn ascii(pattern: &str) -> Regex {
	let pattern = pattern.replace(r"\w", r"(?-u:\w)").replace(r"\b", r"(?-u:\b)");
	Regex::new(&pattern).expect("a valid pattern")
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
