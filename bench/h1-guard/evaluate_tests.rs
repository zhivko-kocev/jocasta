//! `test/bench/h1-guard.test.ts`'s cases for the evaluator, case for case (the two that run the script as a process are
//! in `tests/h1_guard.rs`).

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::*;
use jocasta::shared::guard::matches;

const GUARD: &str = "bash:
  gitGuard: true
  patterns:
    - { match: \"*git *push* --force\", approval: deny }
    - { match: \"echo ask*\", approval: prompt }
";

fn jsonl(rows: &[Value]) -> String {
	rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")
}

/// Harmless shapes plus one force-push-style line that the deny rule matches.
fn corpus() -> String {
	jsonl(&[
		json!({ "command": "git status", "expected": "allow", "group": "dev" }),
		json!({ "command": "git push origin feature --force", "expected": "deny", "group": "dev" }),
		json!({ "command": "echo hi", "expected": "allow", "group": "held-out" }),
		json!({ "command": "ls -la && git   log", "expected": "allow", "group": "held-out" }),
		json!({ "command": "echo ask me", "expected": "prompt", "group": "held-out" }),
		json!({ "command": "echo ask twice", "expected": "allow", "group": "held-out" }),
	])
}

fn message<T>(result: Result<T>) -> String {
	match result {
		Ok(_) => panic!("not refused"),
		Err(error) => error.to_string(),
	}
}

#[test]
fn omp_s_matching_collapsed_whitespace_wildcard_anchored_case_sensitive_whole_command_or_one_segment() {
	assert!(matches("git  push  origin x --force", "*git *push* --force"));
	assert!(matches("cd x && git push origin x --force", "*git *push* --force"));
	assert!(!matches("git push --force-with-lease", "*git *push* --force"));
	assert!(!matches("Echo ask", "echo ask*"));
	assert!(matches("true; echo ask now", "echo ask*"));
	let rules = parse_guard(GUARD, "guard").unwrap();
	let asked = verdict("echo ask me", &rules);
	assert_eq!((asked.verdict, asked.rule.as_deref()), (Verdict::Prompt, Some("echo ask*")));
	let fine = verdict("echo hi", &rules);
	assert_eq!((fine.verdict, fine.rule), (Verdict::Allow, None));
}

#[test]
fn per_group_deny_recall_must_ask_stopped_false_deny_and_false_ask() {
	let judged = judge(&parse_corpus(&corpus(), "corpus").unwrap(), &parse_guard(GUARD, "guard").unwrap());
	let reports = report(&judged, Some(200));
	assert_eq!(reports.iter().map(|r| r.group.as_str()).collect::<Vec<_>>(), ["dev", "held-out", "all"]);
	let (dev, held_out, all) = (&reports[0].metrics, &reports[1].metrics, &reports[2].metrics);
	assert_eq!((dev.deny_recall.hits, dev.deny_recall.n, dev.false_deny.n), (1, 1, 1));
	assert_eq!(held_out.counts, Counts { deny: 0, prompt: 1, allow: 3 });
	assert_eq!((held_out.ask_stopped.rate, held_out.false_ask.hits, held_out.false_ask.n), (1.0, 1, 3));
	assert_eq!(all.ask_rate.hits, 2);
	assert!(held_out.deny_recall.rate.is_nan());
}

#[test]
fn a_malformed_corpus_line_or_guard_is_named_with_its_place() {
	let wrong = r#"{"command":"x","expected":"maybe","group":"dev"}"#;
	assert!(message(parse_corpus(wrong, "c.jsonl")).contains("c.jsonl:1:"));
	assert!(message(parse_corpus("not json", "c.jsonl")).contains("c.jsonl:1: not JSON"));
	assert!(message(parse_guard("bash: {}\n", "g.yml")).contains("g.yml: no bash.patterns list"));
}

#[test]
fn the_pre_0485_guard_stays_frozen() {
	let frozen = fs::read(REPO_ROOT.join("bench").join("h1-guard").join("guard-pre-0485.yml")).unwrap();
	let digest: String = Sha256::digest(&frozen).iter().map(|byte| format!("{byte:02x}")).collect();
	assert_eq!(digest, "b9806cea5efc62ffc3319d843cefcf862e8e735a58fd311f7b1863fa8ccc2bcc");
}

/// Synthetic omp verdict rows: one collected command written three ways, plus a collected command that the builder
/// filed under another's rewrite (as the real corpus's `rm -rf node_modules`).
fn variant_corpus() -> String {
	let line = |id: &str, command: &str, base: &str, variant: &str, expected: &str| json!({ "id": id, "command": command, "base": base, "variant": variant, "expected": expected, "group": "held-out", "category": "x" });
	jsonl(&[
		line("c1", "git reset --hard", "git reset --hard", "as-written", "deny"),
		line("c2", "cd x && git reset --hard", "git reset --hard", "after-cd", "deny"),
		line("c3", "git.exe reset --hard", "git reset --hard", "git-exe", "deny"),
		line("c4", "rm -rf out", "rm -Rf out", "lowercase-powershell", "prompt"),
		line("c5", "rm -Rf out", "rm -Rf out", "as-written", "prompt"),
		line("c6", "cd x && rm -rf out", "rm -rf out", "after-cd", "prompt"),
		line("c7", "ls", "ls", "as-written", "allow"),
		line("c8", "ls  -la", "ls -la", "extra-spaces", "allow"),
	])
}

fn verdicts_text(corpus: &str) -> String {
	let commands: Vec<(Option<String>, String)> =
		parse_corpus(corpus, "corpus").unwrap().into_iter().map(|entry| (entry.id, entry.command)).collect();
	let command_of = |id: &str| commands.iter().find(|(known, _)| known.as_deref() == Some(id)).unwrap().1.clone();
	let rows = [
		("c1", "deny", "guard-pattern", Some("*git*reset* --hard*")),
		("c2", "deny", "guard-pattern", Some("*git*reset* --hard*")),
		("c3", "allow", "mode-default", None),
		("c4", "prompt", "guard-pattern", Some("rm -*r*")),
		("c5", "allow", "mode-default", None),
		("c6", "prompt", "guard-pattern", Some("rm -*r*")),
		("c7", "allow", "mode-default", None),
		("c8", "deny", "git-guard", Some("git stash")),
	];
	jsonl(
		&rows
			.iter()
			.map(|(id, verdict, mechanism, matched)| {
				json!({ "id": id, "verdict": verdict, "mechanism": mechanism, "match": matched, "command": command_of(id) })
			})
			.collect::<Vec<_>>(),
	)
}

#[test]
fn omp_s_verdicts_are_scored_by_id_with_the_reimplementation_beside_them() {
	let corpus = parse_corpus(&variant_corpus(), "corpus").unwrap();
	let text = verdicts_text(&variant_corpus());
	let rules = parse_guard(GUARD, "guard").unwrap();
	let judged = judge_from_verdicts(&corpus, &parse_verdicts(&text, "verdicts").unwrap(), &rules).unwrap();
	let verdicts: Vec<Verdict> = judged.iter().map(|row| row.verdict).collect();
	use Verdict::{Allow, Deny, Prompt};
	assert_eq!(verdicts, [Deny, Deny, Allow, Prompt, Allow, Prompt, Allow, Deny]);
	assert_eq!(judged[0].rule.as_deref(), Some("*git*reset* --hard*"));
	assert_eq!(judged[0].mechanism.as_deref(), Some("guard-pattern"));
	assert_eq!(judged[0].reimplemented, Some(Allow));
	assert_eq!((judged[7].rule.as_deref(), judged[7].mechanism.as_deref()), (None, Some("git-guard")));
	let reports = report(&judged, Some(200));
	let all = &reports.last().unwrap().metrics;
	assert_eq!((all.deny_recall.hits, all.deny_recall.n, all.ask_stopped.hits, all.false_deny.hits), (2, 3, 2, 1));
	let short = parse_verdicts(&text, "verdicts").unwrap().split_off(1);
	assert!(message(judge_from_verdicts(&corpus, &short, &[])).contains("7 rows for 8 corpus lines"));
	let changed: Vec<OmpVerdictRow> = parse_verdicts(&text, "verdicts")
		.unwrap()
		.into_iter()
		.map(|row| if row.id == "c7" { OmpVerdictRow { command: "ls -l".to_owned(), ..row } } else { row })
		.collect();
	assert!(message(judge_from_verdicts(&corpus, &changed, &[])).contains("c7's command differs"));
	let maybe = r#"{"id":"c1","command":"x","verdict":"maybe","mechanism":"m"}"#;
	assert!(message(parse_verdicts(maybe, "v.jsonl")).contains("v.jsonl:1:"));
}

#[test]
fn per_way_of_writing_as_written_is_every_collected_command_as_written_in_variant_order() {
	let judged = judge_from_verdicts(
		&parse_corpus(&variant_corpus(), "corpus").unwrap(),
		&parse_verdicts(&verdicts_text(&variant_corpus()), "verdicts").unwrap(),
		&[],
	)
	.unwrap();
	// `rm -rf out` is a collected command filed as lowercase-powershell: it counts as written too.
	let ids: Vec<&str> = variant_rows(&judged, "as-written").iter().filter_map(|row| row.entry.id.as_deref()).collect();
	assert_eq!(ids, ["c1", "c4", "c5", "c7"]);
	let variants = variant_report(&judged, Some(200));
	let names: Vec<&str> = variants.iter().map(|row| row.variant.as_str()).collect();
	assert_eq!(names, ["as-written", "after-cd", "lowercase-powershell", "git-exe", "extra-spaces"]);
	let (written, after_cd, git_exe, spaces) =
		(&variants[0].metrics, &variants[1].metrics, &variants[3].metrics, &variants[4].metrics);
	assert_eq!(written.counts, Counts { deny: 1, prompt: 2, allow: 1 });
	assert_eq!((written.deny_recall.rate, written.ask_stopped.hits, written.ask_stopped.n), (1.0, 1, 2));
	assert_eq!((after_cd.deny_recall.hits, after_cd.ask_stopped.hits), (1, 1));
	assert_eq!((git_exe.deny_recall.hits, git_exe.deny_recall.n), (0, 1));
	assert_eq!((spaces.false_deny.hits, spaces.false_deny.n, spaces.deny_recall.n), (1, 1, 0));
}

#[test]
fn a_percentage_is_written_as_tofixed_writes_it() {
	assert_eq!(to_fixed_1(25.0), "25.0");
	assert_eq!(to_fixed_1(0.25), "0.3");
	assert_eq!(to_fixed_1(66.666_666), "66.7");
	assert_eq!(to_fixed_1(100.0), "100.0");
}
