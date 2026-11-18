//! The task benchmark's tasks (0482): `D:/thesis/data/bench/tasks/tasks.jsonl` read and checked, the seeded trial
//! order and pilot pick, each task's fixture and hidden tests prepared into a cache, the task as the runner's
//! [`EvalCase`], the leak scan, the infra classifier and the resume state. `evals/bench.rs` schedules the trials.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::evals::harness::{adapter, copy_tree};
use crate::evals::runner::{EvalCase, ResultRow, process_env};
use crate::evals::variants::Harness;
use crate::lib::args::BenchUsageError;
use crate::lib::git::git;
use crate::lib::jsonl::strict_json_lines;
use crate::lib::stats::seeded_random;
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::git::git as git_read;
use jocasta::shared::jsonl::read_jsonl_file;
use jocasta::shared::ledger::TrialTotals;
use jocasta::shared::projects::locale_compare;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLanguage {
	Python,
	TypeScript,
	JavaScript,
	Go,
}

const TASK_LANGUAGES: [&str; 4] = ["Python", "TypeScript", "JavaScript", "Go"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskSet {
	Main,
	Spare,
}

/// One line of `tasks.jsonl`: the fields the benchmark uses (the file's README lists them all).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchTask {
	pub instance_id: String,
	pub set: TaskSet,
	/// `<owner>/<name>`.
	pub repo: String,
	/// Where the base commit is fetched from: GitHub's URL, or a local repository's path.
	pub repo_url: String,
	pub base_commit: String,
	pub problem_statement: String,
	pub gold_patch: String,
	pub test_patch: String,
	pub hidden_test_files: Vec<String>,
	pub setup_command: String,
	pub test_command: String,
	pub language: TaskLanguage,
	pub pr_url: String,
	pub issue_urls: Vec<String>,
	pub merge_commit: String,
}

const STRING_FIELDS: [&str; 11] = [
	"instance_id",
	"repo",
	"repo_url",
	"base_commit",
	"problem_statement",
	"gold_patch",
	"test_patch",
	"setup_command",
	"test_command",
	"pr_url",
	"merge_commit",
];

fn pattern(source: &str) -> Regex {
	Regex::new(source).expect("a fixed pattern")
}

/// Reads `tasks.jsonl`; a missing or malformed field, or an id twice, throws naming the line.
pub fn load_tasks(file: &Path) -> Result<Vec<BenchTask>> {
	static REPO: LazyLock<Regex> = LazyLock::new(|| pattern(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$"));
	static SHA: LazyLock<Regex> = LazyLock::new(|| pattern(r"^[0-9a-f]{40}$"));
	if !file.exists() {
		return Err(BenchUsageError::err(format!("no tasks file at {}", file.display())));
	}
	let text = String::from_utf8_lossy(&fs::read(file)?).into_owned();
	let mut tasks: Vec<BenchTask> = Vec::new();
	for line in strict_json_lines(&text, &file.display().to_string(), true) {
		let line = line?;
		let (value, at) = (&line.row, &line.where_);
		for key in STRING_FIELDS {
			if !value.get(key).and_then(Value::as_str).is_some_and(|text| !text.is_empty()) {
				return Err(BenchUsageError::err(format!("{at}: `{key}` must be a non-empty string")));
			}
		}
		for key in ["hidden_test_files", "issue_urls"] {
			let listed = value
				.get(key)
				.and_then(Value::as_array)
				.is_some_and(|list| list.iter().all(|item| item.as_str().is_some_and(|text| !text.is_empty())));
			if !listed {
				return Err(BenchUsageError::err(format!("{at}: `{key}` must be a list of strings")));
			}
		}
		if !matches!(value.get("set").and_then(Value::as_str), Some("main" | "spare")) {
			return Err(BenchUsageError::err(format!("{at}: `set` must be main or spare")));
		}
		if !value.get("language").and_then(Value::as_str).is_some_and(|name| TASK_LANGUAGES.contains(&name)) {
			return Err(BenchUsageError::err(format!("{at}: `language` must be one of {}", TASK_LANGUAGES.join(", "))));
		}
		let field = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default();
		if !REPO.is_match(field("repo")) {
			return Err(BenchUsageError::err(format!("{at}: `repo` must be <owner>/<name>")));
		}
		if !SHA.is_match(field("base_commit")) || !SHA.is_match(field("merge_commit")) {
			return Err(BenchUsageError::err(format!(
				"{at}: `base_commit` and `merge_commit` must be full commit SHAs"
			)));
		}
		let task: BenchTask = serde_json::from_value(Value::Object(line.row.clone()))?;
		if tasks.iter().any(|other| other.instance_id == task.instance_id) {
			return Err(BenchUsageError::err(format!("{at}: {} is in the file twice", task.instance_id)));
		}
		tasks.push(task);
	}
	Ok(tasks)
}

/// The prompt of every arm (0482): this line, a blank line, then the task's issue text verbatim.
pub const TASK_PROMPT_PREFIX: &str = "Fix the issue below in the repository in the current folder.\n\n";

fn task_prompt(task: &BenchTask) -> String {
	format!("{TASK_PROMPT_PREFIX}{}", task.problem_statement)
}

/// A Fisher–Yates shuffle of a copy of `items` with [`seeded_random`].
pub fn shuffle<T: Clone>(items: &[T], seed: u32) -> Vec<T> {
	let mut random = seeded_random(seed);
	let mut copy = items.to_vec();
	for i in (1..copy.len()).rev() {
		let j = (random() * (i + 1) as f64).floor() as usize;
		copy.swap(i, j);
	}
	copy
}

/// The pilot's strata (0482: 25 trials = 5 tasks × 5 arms × 1 rep).
const PILOT_STRATA: [(&[TaskLanguage], usize); 3] = [
	(&[TaskLanguage::Python], 2),
	(&[TaskLanguage::TypeScript, TaskLanguage::JavaScript], 2),
	(&[TaskLanguage::Go], 1),
];

fn language_name(language: TaskLanguage) -> &'static str {
	match language {
		TaskLanguage::Python => "Python",
		TaskLanguage::TypeScript => "TypeScript",
		TaskLanguage::JavaScript => "JavaScript",
		TaskLanguage::Go => "Go",
	}
}

/// The pilot's tasks: per stratum, the first ones of the stratum's tasks (in id order) shuffled with `seed`.
pub fn pick_pilot(tasks: &[BenchTask], seed: u32) -> Result<Vec<BenchTask>> {
	let mut picked = Vec::new();
	for (languages, count) in PILOT_STRATA {
		let mut stratum: Vec<BenchTask> =
			tasks.iter().filter(|task| languages.contains(&task.language)).cloned().collect();
		stratum.sort_by(|a, b| locale_compare(&a.instance_id, &b.instance_id));
		if stratum.len() < count {
			let names: Vec<&str> = languages.iter().map(|language| language_name(*language)).collect();
			return Err(BenchUsageError::err(format!(
				"the pilot needs {count} {} tasks; the selection has {}",
				names.join("/"),
				stratum.len()
			)));
		}
		picked.extend(shuffle(&stratum, seed).into_iter().take(count));
	}
	Ok(picked)
}

/// One trial of the run: a task, an arm, a rep, and its place in the run's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trial {
	pub task: String,
	pub arm: String,
	pub rep: u64,
	/// 0-based position in the seeded order.
	pub index: usize,
}

/// A trial's key as a results or errors row names it.
pub fn row_key(task: &str, arm: &str, rep: u64) -> String {
	format!("{task}/{arm}/{rep}")
}

pub fn trial_key(trial: &Trial) -> String {
	row_key(&trial.task, &trial.arm, trial.rep)
}

/// Every task × arm × rep, shuffled with `seed` so arms interleave over the run's hours (0482, [S16]).
pub fn plan_trials(tasks: &[BenchTask], arms: &[String], reps: u64, seed: u32) -> Vec<Trial> {
	let mut all: Vec<Trial> = Vec::new();
	for task in tasks {
		for arm in arms {
			for rep in 0..reps {
				all.push(Trial { task: task.instance_id.clone(), arm: arm.clone(), rep, index: 0 });
			}
		}
	}
	shuffle(&all, seed).into_iter().enumerate().map(|(index, trial)| Trial { index, ..trial }).collect()
}

/// A trial runs once, and twice more at most after infra errors (0482).
pub const MAX_ATTEMPTS: u64 = 3;

/// A run folder's `results.jsonl` row: the runner's row plus the benchmark's fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchRow {
	#[serde(flatten)]
	pub row: ResultRow,
	pub task: String,
	pub arm: String,
	pub language: TaskLanguage,
	/// 1 for the first try, 2–3 after infra errors.
	pub attempt: u64,
	pub trial_index: usize,
	/// The leak scan's matches in the transcript; empty when none.
	pub leak: Vec<String>,
	/// The trial's ledger totals at the pinned price table (0538, 0546): tokens, the priced cost, cache read share, the
	/// waste counts and the peak context, over its saved sessions (`sessions`); its rows are in `ledger.jsonl`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ledger: Option<LedgerSummary>,
}

/// `TrialLedger["totals"] & { sessions: number }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerSummary {
	#[serde(flatten)]
	pub totals: TrialTotals,
	pub sessions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InfraKind {
	Setup,
	Auth,
	RateLimit,
	NoResponse,
	Error,
}

/// An `errors.jsonl` row: an infra error, not a task failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfraRow {
	pub task: String,
	pub arm: String,
	pub rep: u64,
	pub attempt: u64,
	pub trial_index: usize,
	pub harness: String,
	pub kind: InfraKind,
	pub message: String,
	pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeState {
	/// Keys of trials with a row in `results.jsonl`.
	pub done: HashSet<String>,
	/// Infra errors so far per key, from `errors.jsonl`.
	pub failures: HashMap<String, u64>,
}

#[derive(Deserialize)]
struct RowKey {
	task: String,
	arm: String,
	rep: u64,
}

/// What a run folder already holds: done trials and each trial's infra errors.
pub fn read_resume_state(out_dir: &Path) -> Result<ResumeState> {
	let done = read_jsonl_file::<RowKey>(&out_dir.join("results.jsonl"))?
		.iter()
		.map(|row| row_key(&row.task, &row.arm, row.rep))
		.collect();
	let mut failures: HashMap<String, u64> = HashMap::new();
	for row in read_jsonl_file::<RowKey>(&out_dir.join("errors.jsonl"))? {
		*failures.entry(row_key(&row.task, &row.arm, row.rep)).or_insert(0) += 1;
	}
	Ok(ResumeState { done, failures })
}

pub struct Pending {
	pub pending: Vec<Trial>,
	pub exhausted: Vec<Trial>,
}

/// The trials still to run, in order; `exhausted` are those whose infra errors used up `max_attempts`
/// ([`MAX_ATTEMPTS`] in the run).
pub fn pending_trials(order: &[Trial], state: &ResumeState, max_attempts: u64) -> Pending {
	let mut pending = Vec::new();
	let mut exhausted = Vec::new();
	for trial in order {
		let key = trial_key(trial);
		if state.done.contains(&key) {
			continue;
		}
		if state.failures.get(&key).copied().unwrap_or(0) >= max_attempts {
			exhausted.push(trial.clone());
		} else {
			pending.push(trial.clone());
		}
	}
	Pending { pending, exhausted }
}

/// A task prepared in the cache: the base commit's tree, the hidden test files and both patches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTask {
	pub dir: PathBuf,
	pub fixture: PathBuf,
	pub hidden: PathBuf,
	pub gold_patch: PathBuf,
	pub test_patch: PathBuf,
	/// Upstream's nearest tag at the base commit, which the trial repo's fixture commit carries; none when no tag is reachable.
	pub tag: Option<String>,
}

/// Git without line-ending conversion, so the cache holds the commit's bytes on Windows too; long paths allowed.
const RAW: [&str; 6] = ["-c", "core.autocrlf=false", "-c", "core.eol=lf", "-c", "core.longpaths=true"];

/// Prepares a task once into `<cache>/<instance_id>/` (0482): the base commit fetched alone into the shared bare repo
/// `<cache>/repos/<owner>__<name>.git` (`git fetch --depth 1 <repo_url> <base_commit>`; GitHub serves reachable SHAs);
/// its tree written to `fixture/` without history (through a throwaway index, not `git archive`, which drops
/// `export-ignore` files); `test.patch` applied in a scratch repo and each `hidden_test_files` entry copied to `hidden/`
/// with its path; `gold.patch` and `test.patch` saved beside them; upstream's nearest tag at the base commit found
/// ([`nearest_tag`]). `prepared.json` holds the base commit, the patches' SHA-256 and the tag, so a second call with
/// the same task does nothing. A test patch that deletes a hidden file throws: grading only overlays files (tasks.jsonl
/// has no deletions).
pub fn prepare_task(task: &BenchTask, cache_dir: &Path) -> Result<PreparedTask> {
	let dir = cache_dir.join(&task.instance_id);
	let prepared = PreparedTask {
		dir: dir.clone(),
		fixture: dir.join("fixture"),
		hidden: dir.join("hidden"),
		gold_patch: dir.join("gold.patch"),
		test_patch: dir.join("test.patch"),
		tag: None,
	};
	let marker = dir.join("prepared.json");
	let digest = Sha256::digest(format!("{}\0{}", task.gold_patch, task.test_patch));
	let patches_sha256: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
	let done: Option<Value> = fs::read_to_string(&marker).ok().and_then(|text| serde_json::from_str(&text).ok());
	// The tag depends on the base commit alone: changed patches keep it, with no fetch.
	let known_tag: Option<Option<String>> = done.as_ref().and_then(|done| {
		if done.get("base_commit").and_then(Value::as_str) != Some(task.base_commit.as_str()) {
			return None;
		}
		match done.get("tag") {
			Some(Value::String(tag)) => Some(Some(tag.clone())),
			Some(Value::Null) => Some(None),
			_ => None,
		}
	});
	if let Some(tag) = &known_tag
		&& done.as_ref().and_then(|done| done.get("patches_sha256")).and_then(Value::as_str)
			== Some(patches_sha256.as_str())
	{
		return Ok(PreparedTask { tag: tag.clone(), ..prepared });
	}
	remove_tree(&dir)?;
	fs::create_dir_all(&dir)?;

	let bare = cache_dir.join("repos").join(format!("{}.git", task.repo.replacen('/', "__", 1)));
	if !bare.exists() {
		fs::create_dir_all(&bare)?;
		git(&bare, &["init", "-q", "--bare"], None)?;
	}
	if git_read(&bare, &["cat-file", "-e", &format!("{}^{{commit}}", task.base_commit)])?.is_none() {
		git(
			&bare,
			&[
				"fetch",
				"-q",
				"--depth",
				"1",
				&task.repo_url,
				&format!("{}:refs/tasks/{}", task.base_commit, task.instance_id),
			],
			None,
		)?;
	}

	fs::create_dir_all(&prepared.fixture)?;
	let mut index = process_env();
	index.insert("GIT_INDEX_FILE".to_owned(), dir.join("fixture.index").display().to_string());
	git(&bare, &["read-tree", &task.base_commit], Some(index.clone()))?;
	let mut checkout: Vec<String> = RAW.iter().map(|word| (*word).to_owned()).collect();
	checkout.extend([
		"--work-tree".to_owned(),
		prepared.fixture.display().to_string(),
		"checkout-index".into(),
		"-a".into(),
		"-f".into(),
	]);
	git(&bare, &checkout, Some(index))?;
	let _ = fs::remove_file(dir.join("fixture.index"));

	fs::write(&prepared.gold_patch, &task.gold_patch)?;
	fs::write(&prepared.test_patch, &task.test_patch)?;
	let scratch = dir.join("scratch");
	copy_tree(&prepared.fixture, &scratch)?;
	let test_patch = prepared.test_patch.display().to_string();
	for args in [
		vec!["init", "-q", "-b", "main"],
		vec!["config", "core.autocrlf", "false"],
		vec!["config", "user.name", "Jocasta Evals"],
		vec!["config", "user.email", "evals@example.org"],
		vec!["add", "-A"],
		vec!["commit", "-q", "--allow-empty", "-m", "fixture"],
		vec!["apply", "--whitespace=nowarn", test_patch.as_str()],
	] {
		git(&scratch, &args, None)?;
	}
	fs::create_dir_all(&prepared.hidden)?;
	for file in &task.hidden_test_files {
		let from = scratch.join(file);
		if !from.exists() {
			return Err(jocasta::shared::errors::Error::other(format!(
				"{}: the test patch deletes {file}; grading can only add or replace files",
				task.instance_id
			)));
		}
		let to = prepared.hidden.join(file);
		if let Some(parent) = to.parent() {
			fs::create_dir_all(parent)?;
		}
		fs::copy(&from, &to)?;
	}
	remove_tree(&scratch)?;
	let tag = match known_tag {
		Some(tag) => tag,
		None => nearest_tag(task, &dir)?,
	};
	let stamp = json!({ "base_commit": task.base_commit, "patches_sha256": patches_sha256, "tag": tag });
	fs::write(&marker, format!("{stamp}\n"))?;
	Ok(PreparedTask { tag, ..prepared })
}

/// Upstream's nearest tag at the base commit (`git describe --tags --abbrev=0`), from a throwaway bare repo that fetches
/// only commits and tags (`--filter=tree:0`; 1–9 MB for the Python tasks). The trial repo's fixture commit carries it,
/// so tools that read the version from git (setuptools-scm, hatch-vcs) see upstream's version, as a full clone does,
/// not a tagless `0.1.dev1` (which fails pytest's dev extras and tox's provisioning tests).
fn nearest_tag(task: &BenchTask, dir: &Path) -> Result<Option<String>> {
	let bare = dir.join("tags.git");
	fs::create_dir_all(&bare)?;
	let found = (|| -> Result<Option<String>> {
		git(&bare, &["init", "-q", "--bare"], None)?;
		git(
			&bare,
			&["fetch", "-q", "--filter=tree:0", &task.repo_url, &task.base_commit, "+refs/tags/*:refs/tags/*"],
			None,
		)?;
		Ok(git_read(&bare, &["describe", "--tags", "--abbrev=0", &task.base_commit])?.filter(|tag| !tag.is_empty()))
	})();
	remove_tree(&bare)?;
	found
}

/// A Python task's setup (0482): a venv at the trial root's `venv/` (made once), pip upgraded, then the task's setup.
/// cmd.exe resolves `python` per command, so once the venv exists every later `python` is the venv's
/// (`PYTHON_PATH_PREFIX`).
pub const PYTHON_SETUP_PREFIX: &str = "(if not exist ..\\venv\\Scripts\\python.exe python -m venv ..\\venv) && python -m pip install -q --upgrade pip && ";
const PYTHON_PATH_PREFIX: [&str; 1] = ["venv/Scripts"];
/// Hidden grading's Python setup (0574): a fresh venv at the trial root's `gvenv/` for every grading, so nothing the
/// agent installed or upgraded in `venv/` during the trial reaches the hidden tests. The old one is removed first, so
/// `python` is the system's again when the new one is made.
const PYTHON_GRADE_SETUP_PREFIX: &str = "(if exist ..\\gvenv rmdir /s /q ..\\gvenv) && python -m venv ..\\gvenv && python -m pip install -q --upgrade pip && ";
const PYTHON_GRADE_PATH_PREFIX: [&str; 1] = ["gvenv/Scripts"];

/// The task as the runner's case: the fixed prompt, the task's setup (wrapped in a venv for Python, a fresh grading
/// venv for its grading), no visible checks and the task's test command as the one hidden check, run on the clean
/// clone with `hidden/` overlaid; the fixture commit tagged with upstream's nearest tag. Node's and Go's setups run as
/// they are: their package and module caches are safe to share between trials.
pub fn task_case(task: &BenchTask, prepared: &PreparedTask) -> EvalCase {
	let python = task.language == TaskLanguage::Python;
	let owned = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
	EvalCase {
		id: task.instance_id.clone(),
		dir: prepared.dir.clone(),
		prompt: task_prompt(task),
		tags: vec![
			language_name(task.language).to_owned(),
			if task.set == TaskSet::Main { "main" } else { "spare" }.to_owned(),
		],
		checks: Vec::new(),
		hidden_checks: vec![crate::evals::runner::Check {
			command: Some(task.test_command.clone()),
			..Default::default()
		}],
		setup: Some(if python {
			format!("{PYTHON_SETUP_PREFIX}{}", task.setup_command)
		} else {
			task.setup_command.clone()
		}),
		fixture: Some(prepared.fixture.clone()),
		hidden: Some(prepared.hidden.clone()),
		fixture_tag: prepared.tag.clone(),
		path_prefix: python.then(|| owned(&PYTHON_PATH_PREFIX)),
		grade_setup: python.then(|| format!("{PYTHON_GRADE_SETUP_PREFIX}{}", task.setup_command)),
		grade_path_prefix: python.then(|| owned(&PYTHON_GRADE_PATH_PREFIX)),
	}
}

/// The transcript without the prompt (it is echoed back), so the issue's own text can't match a scan.
fn without_prompt(transcript: &str, prompt: &str) -> String {
	if prompt.is_empty() {
		return transcript.to_owned();
	}
	let escaped = serde_json::to_string(prompt).unwrap_or_default();
	let escaped = &escaped[1..escaped.len() - 1];
	let mut text = transcript.to_owned();
	if !escaped.is_empty() {
		text = text.replace(escaped, "");
	}
	text.replace(prompt, "")
}

/// Whether `needle` occurs in `text` where `fits(before, after)` holds for the characters around it.
fn occurs(text: &str, needle: &str, fits: impl Fn(Option<char>, Option<char>) -> bool) -> bool {
	text.match_indices(needle)
		.any(|(at, found)| fits(text[..at].chars().next_back(), text[at + found.len()..].chars().next()))
}

/// The leak scan (0482): the fix's PR, the task's issues, the PR's path and the merge commit (as `/commit/<sha>` and
/// its first 10 hex) anywhere in the transcript besides the prompt. Returns the matched patterns; empty when none.
pub fn scan_leaks(task: &BenchTask, transcript: &str, prompt: Option<&str>) -> Vec<String> {
	static PULL: LazyLock<Regex> = LazyLock::new(|| pattern(r"/pull/(\d+)"));
	let text = without_prompt(transcript, prompt.unwrap_or_default()).to_lowercase();
	let number = PULL.captures(&task.pr_url).map(|found| found[1].to_owned());
	let not_digit_after = |_: Option<char>, after: Option<char>| !after.is_some_and(|c| c.is_ascii_digit());
	let mut matched: Vec<String> = Vec::new();
	let mut check = |label: String, hit: bool| {
		if hit && !matched.contains(&label) {
			matched.push(label);
		}
	};
	for url in std::iter::once(&task.pr_url).chain(&task.issue_urls) {
		let needle = url.trim_end_matches('/').to_lowercase();
		check(url.clone(), occurs(&text, &needle, not_digit_after));
	}
	if let Some(number) = number {
		let needle = format!("github.com/{}/pull/{number}", task.repo).to_lowercase();
		check(format!("github.com/{}/pull/{number}", task.repo), occurs(&text, &needle, not_digit_after));
	}
	let commit = format!("/commit/{}", task.merge_commit);
	check(commit.clone(), text.contains(&commit.to_lowercase()));
	let short = &task.merge_commit[..10];
	check(
		short.to_owned(),
		occurs(&text, &short.to_lowercase(), |before, _| {
			!before.is_some_and(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
		}),
	);
	matched
}

// Status codes count only with an error's context: bare 401 or 429 are token counts, line numbers and sizes (EVL-2).
static AUTH: LazyLock<Regex> = LazyLock::new(|| {
	pattern(
		r"(?i)\b(?:status|error|code)[:= ]+40[13]\b|\b40[13] (?:unauthori[sz]ed|forbidden)|unauthori[sz]ed|forbidden|not logged in|not signed in|please (?:log|sign) ?in|/login\b|oauth|refresh(?:ing)? (?:the )?token|token refresh|invalid[_ ]grant|authentication[_ ](?:failed|error)|invalid api key",
	)
});
static RATE: LazyLock<Regex> = LazyLock::new(|| {
	pattern(
		r"(?i)rate[ _-]?limit|\b(?:status|error|code)[:= ]+(?:429|529)\b|\b(?:429|529) (?:too many requests|overloaded)|too many requests|overloaded|usage limit|quota exceeded",
	)
});

/// What [`classify_infra`] reads of a finished (or failed) trial.
#[derive(Default)]
pub struct InfraOutcome {
	pub error: Option<String>,
	pub row: Option<ResultRow>,
	pub stderr: String,
	pub transcript: String,
	pub prompt: String,
	pub account_full: bool,
}

pub struct Infra {
	pub kind: InfraKind,
	pub message: String,
}

/// The error a harness's own records say the trial ended on, or "" (EVL-2), as its adapter reads them: omp's last
/// assistant message (a `--mode json` `message_end`, or a session file's `message`) when its `stopReason` is `error`,
/// with its `errorMessage`; Claude Code's `result` with `is_error` (its text and `api_error_status`) and a
/// `rate_limit_event` whose status isn't `allowed`; opencode's `error` events. Never the run's free text: a tool
/// result's line numbers or a token count.
fn end_signal(transcript: &str, harness: Harness) -> String {
	(adapter(harness).end_signal)(transcript)
}

/// The last 4000 characters of a long text, led by an ellipsis.
fn cut(text: &str) -> String {
	let count = text.chars().count();
	if count > 4000 { format!("…{}", text.chars().skip(count - 4000).collect::<String>()) } else { text.to_owned() }
}

/// Whether a trial failed for infrastructure, not the task (0482): retried later instead of graded. A thrown error is
/// `setup` (the task's setup failed) or `error`. The rest reads only stderr and the harness's own end signal
/// ([`end_signal`]): a sign-in failure there ("not logged in", OAuth or token refresh, `status 401`) with the
/// harness ending on an error (non-zero, not timed out) or before any response is `auth`; a rate-limit or overloaded
/// error there without a tool call is `rate-limit`, and so is one after any number of tool calls when the harness
/// ended on it while an account was full (0543: `account_full`, the pacer's last reading); a harness that ended before
/// its first model response, not timed out, is `no-response`. `None`: the trial counts.
pub fn classify_infra(outcome: &InfraOutcome) -> Option<Infra> {
	if let Some(error) = &outcome.error {
		let kind = if error.contains(": setup `") { InfraKind::Setup } else { InfraKind::Error };
		return Some(Infra { kind, message: cut(error) });
	}
	let Some(row) = &outcome.row else {
		return Some(Infra { kind: InfraKind::Error, message: "no result".to_owned() });
	};
	let signal = end_signal(&without_prompt(&outcome.transcript, &outcome.prompt), row.harness);
	let end = [outcome.stderr.trim().to_owned(), signal]
		.into_iter()
		.filter(|part| !part.is_empty())
		.collect::<Vec<_>>()
		.join("\n");
	let timed_out = row.grade.contains_key("timeout");
	let exited_ok = row.grade.get(&format!("{} exited 0", row.harness)).and_then(Value::as_f64) == Some(1.0);
	let ended_on_error = !exited_ok && !timed_out;
	let message = cut(&end);
	let infra = |kind| Some(Infra { kind, message: message.clone() });
	if (ended_on_error || row.turns == 0) && AUTH.is_match(&end) {
		return infra(InfraKind::Auth);
	}
	if row.tool_calls == 0 && RATE.is_match(&end) {
		return infra(InfraKind::RateLimit);
	}
	if outcome.account_full && ended_on_error && RATE.is_match(&end) {
		return infra(InfraKind::RateLimit);
	}
	if row.turns == 0 && !timed_out {
		return infra(InfraKind::NoResponse);
	}
	None
}
