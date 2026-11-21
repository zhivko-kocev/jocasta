//! H1 (0482, amended by 0500 and 0501): the guard's verdicts on a labelled command corpus, scored per group and per
//! way of writing. Only an evaluator: the corpus is a separate frozen file.
//!
//! ```text
//! jocasta-bench h1-guard evaluate --corpus <corpus.jsonl> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>]
//!   [--resamples 10000] [--out <dir>] [--dry-run]
//! ```
//!
//! Corpus lines: `{"id", "command", "expected": "deny"|"prompt"|"allow", "group", "variant", "base", ...}`. With
//! `--verdicts`, it scores omp 19.1.0's own verdicts (0535, 0623), written by `omp-verdicts.ts` (the matcher of
//! record). Without it, it scores its own reimplementation below, which reads `guard.yml`'s rules only and misses omp's
//! quote-aware segmenting, the git guard, the critical patterns and the allow-rule compound rule (0501); it stays for
//! the unit tests and as the comparison column. Prints per group and per way of writing (0501): deny recall, must-deny
//! asked, must-ask stopped, false deny, false ask and ask rate, each with a bootstrap 95% CI over commands. With
//! `--out`, writes `h1-guard.json` (summary) and `h1-guard.jsonl` (one verdict per command). The `eval` tool and
//! scripts are outside the guard's reach (`config/guard.yml:10`); this measures pattern recall only, not containment.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::lib::args::{ArgSpec, BenchUsageError, REPO_ROOT, flag, flag_or, parse_bench_args};
use crate::lib::jsonl::strict_json_lines;
use crate::lib::output::{append_row, conditions, print_plan, write_json};
use crate::lib::stats::{BootstrapOptions, Rate, rate};
use jocasta::shared::errors::Result;
pub use jocasta::shared::guard::{GuardRule, Verdict};
use jocasta::shared::guard::{parse_guard as parse_guard_text, verdict};
use jocasta::shared::pathkey::{is_inside, resolve};

const VERDICTS: [Verdict; 3] = [Verdict::Deny, Verdict::Prompt, Verdict::Allow];

/// The ways of writing in the corpus (0501), in report order.
pub const VARIANTS: [&str; 10] = [
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorpusEntry {
	pub command: String,
	pub expected: Verdict,
	pub group: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub id: Option<String>,
	/// How the command is written (0501): `as-written` or one of the rewrites.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub variant: Option<String>,
	/// The collected command this line was made from.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub base: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub category: Option<String>,
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
	Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A verdict named by a JSON value, or `None`.
fn verdict_of(value: Option<&Value>) -> Option<Verdict> {
	let text = value?.as_str()?;
	VERDICTS
		.into_iter()
		.find(|candidate| serde_json::to_value(candidate).ok().as_ref().and_then(Value::as_str) == Some(text))
}

/// Reads a guard file's `bash.patterns`; a malformed file is a usage error (exit 2).
pub fn parse_guard(text: &str, file: &str) -> Result<Vec<GuardRule>> {
	parse_guard_text(text, file).map_err(|error| BenchUsageError::err(error.to_string()))
}

/// Reads the corpus JSONL; a malformed line is a usage error with its line number.
pub fn parse_corpus(text: &str, file: &str) -> Result<Vec<CorpusEntry>> {
	let mut entries = Vec::new();
	for line in strict_json_lines(text, file, false) {
		let line = line?;
		let row = &line.row;
		let command = row.get("command").and_then(Value::as_str);
		let group = row.get("group").and_then(Value::as_str).filter(|group| !group.is_empty());
		let (Some(command), Some(expected), Some(group)) = (command, verdict_of(row.get("expected")), group) else {
			return Err(BenchUsageError::err(format!(
				"{}: needs command, expected (deny|prompt|allow) and group",
				line.where_
			)));
		};
		let optional = |field: &str| row.get(field).and_then(Value::as_str).map(str::to_owned);
		entries.push(CorpusEntry {
			command: command.to_owned(),
			expected,
			group: group.to_owned(),
			id: optional("id"),
			variant: optional("variant"),
			base: optional("base"),
			category: optional("category"),
		});
	}
	Ok(entries)
}

/// The corpus at `path`, parsed, with the SHA-256 of its text that every H1 script's summary records as
/// `corpusSha256` (BEN-19: one read and hash instead of four).
pub fn read_corpus(path: &Path) -> Result<(Vec<CorpusEntry>, String)> {
	let bytes = fs::read(path)?;
	let text = String::from_utf8_lossy(&bytes);
	let entries = parse_corpus(&text, &path.to_string_lossy())?;
	Ok((entries, sha256_hex(text.as_bytes())))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
	pub deny: usize,
	pub prompt: usize,
	pub allow: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
	pub counts: Counts,
	/// Must-deny judged deny.
	pub deny_recall: Rate,
	/// Must-deny judged prompt (stopped, but only by asking).
	pub deny_asked: Rate,
	/// Must-ask judged prompt.
	pub ask_recall: Rate,
	/// Must-ask judged deny or prompt.
	pub ask_stopped: Rate,
	/// Must-allow judged deny.
	pub false_deny: Rate,
	/// Must-allow judged prompt.
	pub false_ask: Rate,
	/// Every command judged prompt.
	pub ask_rate: Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupReport {
	pub group: String,
	#[serde(flatten)]
	pub metrics: Metrics,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantReport {
	pub variant: String,
	#[serde(flatten)]
	pub metrics: Metrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Judged {
	#[serde(flatten)]
	pub entry: CorpusEntry,
	pub verdict: Verdict,
	/// The guard pattern that decided, if one did.
	pub rule: Option<String>,
	/// What decided, with `--verdicts` (`omp-verdicts.ts`): guard-pattern, git-guard, critical-pattern, mode-default…
	#[serde(skip_serializing_if = "Option::is_none")]
	pub mechanism: Option<String>,
	/// The reimplementation's verdict beside omp's, with `--verdicts`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reimplemented: Option<Verdict>,
}

pub fn judge(corpus: &[CorpusEntry], rules: &[GuardRule]) -> Vec<Judged> {
	corpus
		.iter()
		.map(|entry| {
			let decided = verdict(&entry.command, rules);
			Judged {
				entry: entry.clone(),
				verdict: decided.verdict,
				rule: decided.rule,
				mechanism: None,
				reimplemented: None,
			}
		})
		.collect()
}

/// One row of `omp-verdicts.jsonl` as scoring reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmpVerdictRow {
	pub id: String,
	pub command: String,
	pub verdict: Verdict,
	pub mechanism: String,
	pub r#match: Option<String>,
}

/// Reads `omp-verdicts.jsonl`; a malformed line is a usage error with its line number.
pub fn parse_verdicts(text: &str, file: &str) -> Result<Vec<OmpVerdictRow>> {
	let mut rows = Vec::new();
	for line in strict_json_lines(text, file, false) {
		let line = line?;
		let row = &line.row;
		let text_of = |field: &str| row.get(field).and_then(Value::as_str);
		let (Some(id), Some(command), Some(decided), Some(mechanism)) =
			(text_of("id"), text_of("command"), verdict_of(row.get("verdict")), text_of("mechanism"))
		else {
			return Err(BenchUsageError::err(format!(
				"{}: needs id, command, verdict (deny|prompt|allow) and mechanism",
				line.where_
			)));
		};
		rows.push(OmpVerdictRow {
			id: id.to_owned(),
			command: command.to_owned(),
			verdict: decided,
			mechanism: mechanism.to_owned(),
			r#match: text_of("match").map(str::to_owned),
		});
	}
	Ok(rows)
}

/// omp's verdicts joined to the corpus by id, with the reimplementation's verdict beside each. Every corpus line needs
/// exactly one verdict for the same command.
pub fn judge_from_verdicts(
	corpus: &[CorpusEntry],
	verdicts: &[OmpVerdictRow],
	rules: &[GuardRule],
) -> Result<Vec<Judged>> {
	let mut by_id: Vec<(&str, &OmpVerdictRow)> = Vec::new();
	let mut seen = HashSet::new();
	for row in verdicts {
		if !seen.insert(row.id.as_str()) {
			return Err(BenchUsageError::err(format!("verdicts: {} appears twice", row.id)));
		}
		by_id.push((row.id.as_str(), row));
	}
	if by_id.len() != corpus.len() {
		return Err(BenchUsageError::err(format!("verdicts: {} rows for {} corpus lines", by_id.len(), corpus.len())));
	}
	corpus
		.iter()
		.map(|entry| {
			let row =
				entry.id.as_deref().and_then(|id| by_id.iter().find(|(known, _)| *known == id)).map(|(_, row)| *row);
			let Some(row) = row else {
				return Err(BenchUsageError::err(format!(
					"verdicts: no verdict for corpus line {}",
					entry.id.as_deref().unwrap_or("(no id)")
				)));
			};
			if row.command != entry.command {
				return Err(BenchUsageError::err(format!("verdicts: {}'s command differs from the corpus's", row.id)));
			}
			Ok(Judged {
				entry: entry.clone(),
				verdict: row.verdict,
				rule: if row.mechanism == "guard-pattern" { row.r#match.clone() } else { None },
				mechanism: Some(row.mechanism.clone()),
				reimplemented: Some(verdict(&entry.command, rules).verdict),
			})
		})
		.collect()
}

/// The rates with bootstrap CIs over `rows`.
pub fn metrics(rows: &[&Judged], resamples: Option<usize>) -> Metrics {
	let options = BootstrapOptions { resamples, ..BootstrapOptions::default() };
	let of = |expected: Verdict| -> Vec<&Judged> {
		rows.iter().copied().filter(|row| row.entry.expected == expected).collect()
	};
	let rate_of = |expected: Verdict, hit: &dyn Fn(Verdict) -> bool| {
		let outcomes: Vec<bool> = of(expected).iter().map(|row| hit(row.verdict)).collect();
		rate(&outcomes, options)
	};
	let all: Vec<bool> = rows.iter().map(|row| row.verdict == Verdict::Prompt).collect();
	Metrics {
		counts: Counts {
			deny: of(Verdict::Deny).len(),
			prompt: of(Verdict::Prompt).len(),
			allow: of(Verdict::Allow).len(),
		},
		deny_recall: rate_of(Verdict::Deny, &|v| v == Verdict::Deny),
		deny_asked: rate_of(Verdict::Deny, &|v| v == Verdict::Prompt),
		ask_recall: rate_of(Verdict::Prompt, &|v| v == Verdict::Prompt),
		ask_stopped: rate_of(Verdict::Prompt, &|v| v != Verdict::Allow),
		false_deny: rate_of(Verdict::Allow, &|v| v == Verdict::Deny),
		false_ask: rate_of(Verdict::Allow, &|v| v == Verdict::Prompt),
		ask_rate: rate(&all, options),
	}
}

/// Per group (plus `all`), the rates with bootstrap CIs.
pub fn report(judged: &[Judged], resamples: Option<usize>) -> Vec<GroupReport> {
	let mut groups: Vec<&str> =
		judged.iter().map(|row| row.entry.group.as_str()).collect::<HashSet<_>>().into_iter().collect();
	groups.sort_unstable();
	groups.push("all");
	groups
		.into_iter()
		.map(|group| {
			let rows: Vec<&Judged> = judged.iter().filter(|row| group == "all" || row.entry.group == group).collect();
			GroupReport { group: group.to_owned(), metrics: metrics(&rows, resamples) }
		})
		.collect()
}

/// The lines of one way of writing (0501). `as-written` is every line whose command is one of the collected commands
/// as written: the builder keeps one line per command, so a collected command that equals another's rewrite (the
/// corpus's `rm -rf node_modules`, also `rm -Rf node_modules` lowercased) sits under that rewrite's variant and counts
/// in both rows.
pub fn variant_rows<'a>(judged: &'a [Judged], variant: &str) -> Vec<&'a Judged> {
	if variant != "as-written" {
		return judged.iter().filter(|row| row.entry.variant.as_deref() == Some(variant)).collect();
	}
	let bases: HashSet<&str> = judged.iter().filter_map(|row| row.entry.base.as_deref()).collect();
	judged
		.iter()
		.filter(|row| row.entry.variant.as_deref() == Some("as-written") || bases.contains(row.entry.command.as_str()))
		.collect()
}

/// Per way of writing, in `VARIANTS` order then any others sorted; variants without lines are left out.
pub fn variant_report(judged: &[Judged], resamples: Option<usize>) -> Vec<VariantReport> {
	let present: HashSet<&str> = judged.iter().filter_map(|row| row.entry.variant.as_deref()).collect();
	let mut others: Vec<&str> = present.iter().copied().filter(|variant| !VARIANTS.contains(variant)).collect();
	others.sort_unstable();
	VARIANTS
		.into_iter()
		.filter(|variant| present.contains(variant))
		.chain(others)
		.map(|variant| VariantReport {
			variant: variant.to_owned(),
			metrics: metrics(&variant_rows(judged, variant), resamples),
		})
		.collect()
}

/// `x` with one decimal as JavaScript's `toFixed(1)` writes it: from the exact decimal expansion, a tie rounds up.
fn to_fixed_1(x: f64) -> String {
	let exact = format!("{x:.60}");
	let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, "0"));
	let digits = fraction.as_bytes();
	let tenths = u64::from(digits[0] - b'0') + u64::from(digits.get(1).is_some_and(|digit| *digit >= b'5'));
	let scaled = whole.parse::<u64>().unwrap_or(0) * 10 + tenths;
	format!("{}.{}", scaled / 10, scaled % 10)
}

fn percent(value: &Rate) -> String {
	if value.n == 0 {
		return "—".to_owned();
	}
	format!(
		"{}% [{}, {}] ({}/{})",
		to_fixed_1(100.0 * value.rate),
		to_fixed_1(100.0 * value.ci95.low),
		to_fixed_1(100.0 * value.ci95.high),
		value.hits,
		value.n
	)
}

fn metric_cells(m: &Metrics) -> String {
	format!(
		"{}/{}/{} | {} | {} | {} | {} | {} | {} |",
		m.counts.deny,
		m.counts.prompt,
		m.counts.allow,
		percent(&m.deny_recall),
		percent(&m.deny_asked),
		percent(&m.ask_stopped),
		percent(&m.false_deny),
		percent(&m.false_ask),
		percent(&m.ask_rate)
	)
}

const HEADER_CELLS: &str =
	"deny/ask/allow | Deny recall | Must-deny asked | Must-ask stopped | False deny | False ask | Ask rate |";

pub fn render_report(reports: &[GroupReport], variants: &[VariantReport]) -> String {
	let mut lines = vec![format!("| Group | {HEADER_CELLS}"), "|---|---|---|---|---|---|---|---|".to_owned()];
	lines.extend(reports.iter().map(|r| format!("| {} | {}", r.group, metric_cells(&r.metrics))));
	if !variants.is_empty() {
		lines.push(String::new());
		lines.push(format!("| Way of writing | {HEADER_CELLS}"));
		lines.push("|---|---|---|---|---|---|---|---|".to_owned());
		lines.extend(variants.iter().map(|r| format!("| {} | {}", r.variant, metric_cells(&r.metrics))));
	}
	lines.push(String::new());
	lines.push(
		"Pattern recall only: omp's `eval` tool, scripts and interpreters run outside the guard (config/guard.yml:10-12)."
			.to_owned(),
	);
	lines.join("\n")
}

const USAGE: &str = "usage: jocasta-bench h1-guard evaluate --corpus <corpus.jsonl> [--guard config/guard.yml] [--verdicts <omp-verdicts.jsonl>] [--resamples 10000] [--out <dir>] [--dry-run]";

/// The script: `jocasta-bench h1-guard evaluate …`.
pub fn main(argv: &[String]) -> Result<()> {
	if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
		println!("{USAGE}");
		return Ok(());
	}
	let spec = ArgSpec { valued: &["--corpus", "--guard", "--resamples", "--verdicts"], ..ArgSpec::new(1) };
	let args = parse_bench_args(argv, &spec)?;
	let corpus_path = flag(&args, "corpus").ok_or_else(|| BenchUsageError::err("--corpus <path.jsonl> is required"))?;
	let default_guard = REPO_ROOT.join("config").join("guard.yml");
	let guard_path = resolve(Path::new(flag_or(&args, "guard", &default_guard.to_string_lossy())));
	let verdicts_path = flag(&args, "verdicts").map(|text| resolve(Path::new(text)));
	let resamples_text = flag_or(&args, "resamples", "10000");
	let whole = !resamples_text.is_empty() && resamples_text.chars().all(|c| c.is_ascii_digit());
	let resamples = if whole { resamples_text.parse::<usize>().unwrap_or(usize::MAX) } else { 0 };
	if resamples < 1 {
		return Err(BenchUsageError::err(format!("--resamples needs a whole number, not \"{resamples_text}\"")));
	}
	let mut needed = vec![Path::new(corpus_path), guard_path.as_path()];
	needed.extend(verdicts_path.as_deref());
	if let Some(missing) = needed.into_iter().find(|path| !path.exists()) {
		return Err(BenchUsageError::err(format!("{} does not exist", missing.display())));
	}
	let (corpus, corpus_sha256) = read_corpus(Path::new(corpus_path))?;
	let rules = parse_guard(&String::from_utf8_lossy(&fs::read(&guard_path)?), &guard_path.to_string_lossy())?;
	let matcher = match &verdicts_path {
		None => "reimplementation (evaluate.ts: guard.yml's rules only, raw-text segments)".to_owned(),
		Some(path) => format!("omp's own verdicts ({})", path.display()),
	};
	if args.dry_run {
		let lines = plan_lines(&corpus, &rules, &guard_path, verdicts_path.as_deref(), resamples);
		print_plan("H1 guard", &lines, args.out.as_deref());
		return Ok(());
	}
	let started = conditions(&[] as &[&str]);
	let judged = match &verdicts_path {
		None => judge(&corpus, &rules),
		Some(path) => {
			let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
			judge_from_verdicts(&corpus, &parse_verdicts(&text, &path.to_string_lossy())?, &rules)?
		}
	};
	let reports = report(&judged, Some(resamples));
	let variants = variant_report(&judged, Some(resamples));
	let as_written_note = as_written_note(&judged);
	println!("matcher: {matcher}");
	println!("{}", render_report(&reports, &variants));
	let Some(out) = args.out.as_deref() else { return Ok(()) };
	for row in &judged {
		append_row(out, "h1-guard", row)?;
	}
	let mut summary = json!({
		"benchmark": "H1",
		"conditions": started.finished(),
		"matcher": matcher,
		"guard": guard_label(&guard_path),
		"corpusSha256": corpus_sha256,
	});
	if let (Some(path), Some(map)) = (&verdicts_path, summary.as_object_mut()) {
		map.insert("verdictsSha256".to_owned(), json!(sha256_hex(&fs::read(path)?)));
	}
	if let Some(map) = summary.as_object_mut() {
		map.insert("rules".to_owned(), json!(rules.len()));
		map.insert("notes".to_owned(), json!({ "asWritten": as_written_note }));
		map.insert("commands".to_owned(), json!(corpus.len()));
		map.insert("reports".to_owned(), serde_json::to_value(&reports)?);
		map.insert("variants".to_owned(), serde_json::to_value(&variants)?);
	}
	let file = write_json(out, "h1-guard", &summary)?;
	println!("wrote {}", file.display());
	Ok(())
}

/// The guard's path as the summary names it: relative to the repo when it lies inside, else as given.
fn guard_label(guard: &Path) -> String {
	if is_inside(&REPO_ROOT, guard)
		&& let Ok(relative) = resolve(guard).strip_prefix(resolve(&REPO_ROOT))
	{
		return relative.to_string_lossy().into_owned();
	}
	guard.to_string_lossy().into_owned()
}

fn as_written_note(judged: &[Judged]) -> String {
	let as_written = variant_rows(judged, "as-written");
	let elsewhere: Vec<&&Judged> =
		as_written.iter().filter(|row| row.entry.variant.as_deref() != Some("as-written")).collect();
	let listed = elsewhere
		.iter()
		.map(|row| {
			format!(
				"{} ({}, tagged {})",
				row.entry.id.as_deref().unwrap_or("?"),
				row.entry.command,
				row.entry.variant.as_deref().unwrap_or("?")
			)
		})
		.collect::<Vec<_>>()
		.join(", ");
	format!(
		"the as-written row is the {} lines whose command is one of the collected commands as written; {} carry the as-written tag, and {} {} filed under another rewrite because the builder keeps one line per command (0501); such a line counts in both rows",
		as_written.len(),
		as_written.len() - elsewhere.len(),
		if listed.is_empty() { "none" } else { &listed },
		if elsewhere.len() == 1 { "is" } else { "are" }
	)
}

/// The dry run's plan.
fn plan_lines(
	corpus: &[CorpusEntry],
	rules: &[GuardRule],
	guard_path: &Path,
	verdicts_path: Option<&Path>,
	resamples: usize,
) -> Vec<String> {
	let mut groups: Vec<&str> =
		corpus.iter().map(|entry| entry.group.as_str()).collect::<HashSet<_>>().into_iter().collect();
	groups.sort_unstable();
	let listed = if groups.is_empty() { "none".to_owned() } else { groups.join(", ") };
	let matcher_line = match verdicts_path {
		None => "matcher: the reimplementation (guard.yml's rules only; `*` wildcard, anchored, case-sensitive, whole command or any raw-text segment at && || ; |); omp's own verdicts need --verdicts".to_owned(),
		Some(path) => format!("matcher: omp's own verdicts from {}, the reimplementation beside them", path.display()),
	};
	vec![
		format!(
			"judge {} commands (groups: {listed}) against {} rules in {}",
			corpus.len(),
			rules.len(),
			guard_path.display()
		),
		matcher_line,
		format!(
			"report deny recall, must-ask stopped, false deny, false ask and ask rate per group and per way of writing with {resamples}-resample bootstrap 95% CIs"
		),
	]
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;
