//! H1 (0482, 0500, 0501, 0535, 0600, 0623): omp 18.20.2's own verdict on every corpus line, computed in-process from
//! omp's installed sources, with nothing executed. Ported from the script half of `omp-verdicts.ts` (0636); the oracle
//! half, which loads omp and judges through its `BashTool` approval and `resolveApproval`, stays TypeScript on Bun and
//! is reached through `oracle.rs`.
//!
//! ```text
//! jocasta-bench h1-guard omp-verdicts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir> [--omp <package dir>]
//!   [--any-version] [--dry-run]
//! ```
//!
//! How omp decides is in `omp-verdicts.ts`. `bash.gitGuard` isn't an approval: the embedded shell's `git` builtin
//! refuses at run time. The git guard has no check that doesn't run the command: with `--git-guard
//! <git-guard-check.jsonl>` the refusals measured in the VM (`git-guard-check` ran those lines through omp's embedded
//! shell) decide for the lines it covers, and `git_guard`'s reimplementation, applied on omp's own tokenizer's argv, for
//! the rest; each line names its source (`gitGuardSource`). A line omp's approval allows and the git guard refuses is
//! `deny` by `git-guard`; a line omp asks about stays `prompt`.
//!
//! Writes `omp-verdicts.jsonl` (id, command, expected, variant, base, category, verdict, mechanism, match, approval,
//! critical, gitGuard, gitGuardSource, reimplemented = the evaluator's verdict) and `omp-verdicts.json` (version,
//! settings, counts), and prints where omp and the reimplementation disagree, by expected class and by way of writing.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::evaluate::{CorpusEntry, VARIANTS, parse_guard, read_corpus};
use super::git_guard::{MeasuredGitGuardLine, parse_measured_git_guard};
use super::oracle::{OmpVerdict, Oracle};
use crate::lib::args::{ArgSpec, BenchUsageError, REPO_ROOT, flag, flag_or, parse_bench_args};
use crate::lib::output::{Conditions, append_row, conditions, print_plan, write_json};
use crate::lib::temp::mkdtemp;
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::guard::{GuardRule, Verdict, verdict};
use jocasta::shared::pathkey;

pub const PINNED_OMP: &str = "18.20.2";
const PACKAGE: &str = "@oh-my-pi/pi-coding-agent";

/// One row of `omp-verdicts.jsonl`.
#[derive(Debug, Clone, Serialize)]
pub struct VerdictLine {
	pub id: String,
	pub command: String,
	pub expected: Verdict,
	pub variant: String,
	pub base: String,
	pub category: String,
	#[serde(flatten)]
	pub omp: OmpVerdict,
	pub reimplemented: Verdict,
}

/// Where omp's verdict differs from the reimplementation's for one key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disagreement {
	pub key: String,
	pub n: usize,
	pub disagree: usize,
	/// `old→new` (reimplemented→omp) and how many lines moved that way, in first-seen order.
	pub moves: Map<String, Value>,
}

/// Where omp's verdict differs from the reimplementation's, grouped by `key_of`, in first-seen key order; each line is
/// `(omp's verdict, the reimplementation's)`.
pub fn disagreements(lines: &[(Verdict, Verdict)], key_of: impl Fn(usize) -> String) -> Vec<Disagreement> {
	let mut by_key: Vec<Disagreement> = Vec::new();
	for (index, &(omp, reimplemented)) in lines.iter().enumerate() {
		let key = key_of(index);
		let at = by_key.iter().position(|entry| entry.key == key).unwrap_or_else(|| {
			by_key.push(Disagreement { key, n: 0, disagree: 0, moves: Map::new() });
			by_key.len() - 1
		});
		let entry = &mut by_key[at];
		entry.n += 1;
		if omp == reimplemented {
			continue;
		}
		entry.disagree += 1;
		let slot = entry.moves.entry(format!("{}→{}", word(reimplemented), word(omp))).or_insert(json!(0));
		*slot = json!(slot.as_u64().unwrap_or(0) + 1);
	}
	by_key
}

fn word(verdict: Verdict) -> &'static str {
	match verdict {
		Verdict::Allow => "allow",
		Verdict::Deny => "deny",
		Verdict::Prompt => "prompt",
	}
}

fn render_disagreements(title: &str, rows: &[Disagreement]) -> String {
	let mut lines = vec![format!("| {title} | lines | disagree | reimplemented→omp |"), "|---|---|---|---|".to_owned()];
	for row in rows {
		let mut moves: Vec<(&String, u64)> =
			row.moves.iter().map(|(name, count)| (name, count.as_u64().unwrap_or(0))).collect();
		moves.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
		let listed = moves.iter().map(|(name, count)| format!("{name} {count}")).collect::<Vec<_>>().join(", ");
		let listed = if listed.is_empty() { "—".to_owned() } else { listed };
		lines.push(format!("| {} | {} | {} | {listed} |", row.key, row.n, row.disagree));
	}
	lines.join("\n")
}

/// What `judge_corpus` asks for one line: omp's verdict, with the measured git guard outcome when there is one.
pub type Judge<'a> = &'a mut dyn FnMut(&str, Option<&MeasuredGitGuardLine>) -> Result<OmpVerdict>;

/// Every corpus line through `judge`, with the reimplementation's verdict beside it.
pub fn judge_corpus(
	corpus: &[CorpusEntry],
	judge: Judge<'_>,
	reimplemented: &dyn Fn(&str) -> Verdict,
	measured: &HashMap<String, MeasuredGitGuardLine>,
) -> Result<Vec<VerdictLine>> {
	let mut lines = Vec::with_capacity(corpus.len());
	for (i, entry) in corpus.iter().enumerate() {
		let Some(id) = &entry.id else {
			return Err(BenchUsageError::err(format!("corpus line {} has no id", i + 1)));
		};
		let check = measured.get(id);
		if let Some(check) = check
			&& check.command != entry.command
		{
			return Err(BenchUsageError::err(format!("git guard check: {id}'s command differs from the corpus's")));
		}
		lines.push(VerdictLine {
			id: id.clone(),
			command: entry.command.clone(),
			expected: entry.expected,
			variant: entry.variant.clone().unwrap_or_default(),
			base: entry.base.clone().unwrap_or_default(),
			category: entry.category.clone().unwrap_or_default(),
			omp: judge(&entry.command, check)?,
			reimplemented: reimplemented(&entry.command),
		});
	}
	Ok(lines)
}

const USAGE: &str = "usage: jocasta-bench h1-guard omp-verdicts --corpus <corpus.jsonl> --guard <guard.yml> --out <dir> [--git-guard <git-guard-check.jsonl>] [--omp <package dir>] [--any-version] [--dry-run]";

/// The script: `jocasta-bench h1-guard omp-verdicts …`.
pub fn main(argv: &[String]) -> Result<()> {
	if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
		println!("{USAGE}");
		return Ok(());
	}
	let spec = ArgSpec {
		valued: &["--corpus", "--guard", "--omp", "--git-guard"],
		booleans: &["--any-version"],
		..ArgSpec::new(1)
	};
	let args = parse_bench_args(argv, &spec)?;
	let corpus_path = flag(&args, "corpus").ok_or_else(|| BenchUsageError::err("--corpus <path.jsonl> is required"))?;
	let default_guard = REPO_ROOT.join("config").join("guard.yml");
	let guard_path = pathkey::resolve(Path::new(flag_or(&args, "guard", &default_guard.to_string_lossy())));
	let git_guard_path = flag(&args, "git-guard");
	let mut needed = vec![Path::new(corpus_path), guard_path.as_path()];
	needed.extend(git_guard_path.map(Path::new));
	if let Some(missing) = needed.into_iter().find(|path| !path.exists()) {
		return Err(BenchUsageError::err(format!("{} does not exist", missing.display())));
	}
	let measured: HashMap<String, MeasuredGitGuardLine> = match git_guard_path {
		None => HashMap::new(),
		Some(path) => parse_measured_git_guard(&String::from_utf8_lossy(&fs::read(path)?), path)?.into_iter().collect(),
	};
	let any_version = args.flags.contains_key("any-version");
	let mut oracle = Oracle::start()?;
	let omp_dir = match flag(&args, "omp") {
		Some(text) => pathkey::resolve(Path::new(text)),
		None => oracle
			.find()?
			.map(Into::into)
			.ok_or_else(|| BenchUsageError::err(format!("omp's package not found; pass --omp <dir of {PACKAGE}>")))?,
	};
	let (corpus, corpus_sha256) = read_corpus(Path::new(corpus_path))?;
	let rules = parse_guard(&String::from_utf8_lossy(&fs::read(&guard_path)?), &guard_path.to_string_lossy())?;
	if args.dry_run {
		let overridden = if any_version { ", overridden by --any-version" } else { "" };
		let git_guard = match git_guard_path {
			None => "reimplemented (git_guard.rs)".to_owned(),
			Some(path) => format!(
				"measured for the {} lines in {path}, reimplemented (git_guard.rs) for the rest",
				measured.len()
			),
		};
		print_plan(
			"H1 omp verdicts",
			&[
				format!("import omp from {} (refusing anything but {PINNED_OMP}{overridden})", omp_dir.display()),
				format!(
					"load {} as --config over omp's defaults (empty agent dir and cwd); build BashTool on a session stub that throws",
					guard_path.display()
				),
				format!(
					"ask omp's resolveApproval for each of {} commands; git guard {git_guard}; nothing executes",
					corpus.len()
				),
				"compare with the reimplementation (evaluate.rs) by expected class and by way of writing".to_owned(),
			],
			args.out.as_deref(),
		);
		return Ok(());
	}
	let out = args.out.as_deref().ok_or_else(|| BenchUsageError::err("--out <dir> is required (outside the repo)"))?;
	if out.join("omp-verdicts.jsonl").exists() {
		return Err(BenchUsageError::err(format!(
			"{} exists; results are never overwritten",
			out.join("omp-verdicts.jsonl").display()
		)));
	}
	let started = conditions(&[] as &[&str]);
	let omp = oracle.load(Some(&omp_dir), any_version)?;
	println!("omp {} from {}", omp.version, omp.dir);
	let empty = mkdtemp("h1-omp-")?;
	let run = Run {
		corpus: &corpus,
		rules: &rules,
		corpus_sha256: &corpus_sha256,
		guard_path: &guard_path,
		git_guard_path,
		measured: &measured,
		out,
	};
	let done = run.record(&mut oracle, &empty, started, &omp.version);
	remove_tree(&empty)?;
	done
}

/// What a real run reads and where it writes.
struct Run<'a> {
	corpus: &'a [CorpusEntry],
	rules: &'a [GuardRule],
	corpus_sha256: &'a str,
	guard_path: &'a Path,
	git_guard_path: Option<&'a str>,
	measured: &'a HashMap<String, MeasuredGitGuardLine>,
	out: &'a Path,
}

impl Run<'_> {
	/// Every line through omp, the rows, what the run prints and the summary.
	fn record(&self, oracle: &mut Oracle, empty: &Path, started: Conditions, omp_version: &str) -> Result<()> {
		let init = oracle.judge_init(&[self.guard_path], empty, Some(empty))?;
		let settings = init.summary.json();
		println!("settings: {settings}");
		let mut judge = |command: &str, check: Option<&MeasuredGitGuardLine>| oracle.judge(command, check);
		let rules = self.rules;
		let lines = judge_corpus(self.corpus, &mut judge, &|command| verdict(command, rules).verdict, self.measured)?;
		for line in &lines {
			append_row(self.out, "omp-verdicts", line)?;
		}
		let pairs: Vec<(Verdict, Verdict)> = lines.iter().map(|line| (line.omp.verdict, line.reimplemented)).collect();
		let by_expected = disagreements(&pairs, |i| word(lines[i].expected).to_owned());
		let mut by_variant = disagreements(&pairs, |i| lines[i].variant.clone());
		let order = |key: &str| VARIANTS.iter().position(|variant| *variant == key).map_or(-1, |at| at as i64);
		by_variant.sort_by_key(|entry| order(&entry.key));
		let mut mechanisms = Map::new();
		let mut sources = Map::new();
		for line in &lines {
			count(&mut mechanisms, &line.omp.mechanism);
			count(&mut sources, line.omp.git_guard_source);
		}
		println!("git guard sources: {}", Value::Object(sources.clone()));
		println!("{}", render_disagreements("Expected", &by_expected));
		println!();
		println!("{}", render_disagreements("Way of writing", &by_variant));
		println!("mechanisms: {}", Value::Object(mechanisms.clone()));
		let mut finished = started.finished();
		finished.versions.insert("omp".to_owned(), json!(omp_version));
		let reimplemented = if sources.contains_key("reimplemented") {
			json!(["git guard (git_guard.rs) where gitGuardSource is reimplemented"])
		} else {
			json!([])
		};
		let file = write_json(
			self.out,
			"omp-verdicts",
			&json!({
				"benchmark": "H1",
				"conditions": finished,
				"omp": { "version": omp_version, "sources": "src/ of the installed package" },
				"guard": self.guard_path.to_string_lossy(),
				"corpusSha256": self.corpus_sha256,
				"settings": settings,
				"gitGuardCheck": self
					.git_guard_path
					.map(|path| pathkey::resolve(Path::new(path)).to_string_lossy().into_owned()),
				"gitGuardSources": sources,
				"reimplemented": reimplemented,
				"commands": lines.len(),
				"mechanisms": mechanisms,
				"disagreements": { "byExpected": by_expected, "byVariant": by_variant },
			}),
		)?;
		println!("wrote {}", file.display());
		Ok(())
	}
}

fn count(counts: &mut Map<String, Value>, key: &str) {
	let slot = counts.entry(key.to_owned()).or_insert(json!(0));
	*slot = json!(slot.as_u64().unwrap_or(0) + 1);
}

#[cfg(test)]
#[path = "omp_verdicts_tests.rs"]
mod tests;
