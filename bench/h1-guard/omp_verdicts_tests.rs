//! `test/bench/h1-guard.test.ts`'s cases for `omp-verdicts.ts`'s script half (`disagreements`, `judgeCorpus`), and one
//! case through the real oracle (Bun and omp 19.1.1), skipped with its reason where either is missing, as the
//! TypeScript's omp-dependent cases were.

use super::*;
use crate::h1_guard::evaluate::parse_corpus;
use jocasta::shared::testing::temp_dir;

#[test]
fn disagreements_count_reimplemented_omp_moves_per_key() {
	use Verdict::{Allow, Deny, Prompt};
	let lines = [(Deny, Allow), (Deny, Deny), (Deny, Allow), (Prompt, Allow)];
	let keys = ["a", "a", "b", "a"];
	let found = disagreements(&lines, |i| keys[i].to_owned());
	let moves = |pairs: &[(&str, u64)]| -> Map<String, Value> {
		pairs.iter().map(|(name, count)| ((*name).to_owned(), json!(count))).collect()
	};
	assert_eq!(
		found,
		vec![
			Disagreement {
				key: "a".to_owned(),
				n: 3,
				disagree: 2,
				moves: moves(&[("allow→deny", 1), ("allow→prompt", 1)])
			},
			Disagreement { key: "b".to_owned(), n: 1, disagree: 1, moves: moves(&[("allow→deny", 1)]) },
		]
	);
}

fn stub_verdict(check: Option<&MeasuredGitGuardLine>) -> OmpVerdict {
	OmpVerdict {
		verdict: Verdict::Allow,
		mechanism: "mode-default".to_owned(),
		pattern: None,
		approval: Verdict::Allow,
		critical: false,
		git_guard: None,
		git_guard_source: if check.is_none() { "reimplemented" } else { "measured" },
	}
}

#[test]
fn measured_git_guard_lines_replace_the_reimplementation_in_omp_s_verdicts() {
	let jsonl = [
		json!({ "id": "c1", "command": "git stash", "measured": true, "refusal": "git stash is blocked" }),
		json!({ "id": "c2", "command": "git status", "measured": true, "refusal": null }),
		json!({ "id": "c3", "command": "git log", "measured": false, "refusal": null }),
	]
	.iter()
	.map(Value::to_string)
	.collect::<Vec<_>>()
	.join("\n");
	let measured: HashMap<String, MeasuredGitGuardLine> =
		parse_measured_git_guard(&jsonl, "g.jsonl").unwrap().into_iter().collect();
	let corpus = parse_corpus(
		&[
			json!({ "id": "c1", "command": "git stash", "expected": "allow", "group": "g" }),
			json!({ "id": "c9", "command": "git log", "expected": "allow", "group": "g" }),
		]
		.iter()
		.map(Value::to_string)
		.collect::<Vec<_>>()
		.join("\n"),
		"corpus",
	)
	.unwrap();
	let mut seen: Vec<Option<Option<String>>> = Vec::new();
	let mut judge = |_: &str, check: Option<&MeasuredGitGuardLine>| {
		seen.push(check.map(|line| line.refusal.clone()));
		Ok(stub_verdict(check))
	};
	let lines = judge_corpus(&corpus, &mut judge, &|_| Verdict::Allow, &measured).unwrap();
	assert_eq!(seen, vec![Some(Some("git stash is blocked".to_owned())), None]);
	assert_eq!(lines.iter().map(|line| line.omp.git_guard_source).collect::<Vec<_>>(), ["measured", "reimplemented"]);
	let wrong: HashMap<String, MeasuredGitGuardLine> = HashMap::from([(
		"c1".to_owned(),
		MeasuredGitGuardLine { command: "git stash list".to_owned(), refusal: None },
	)]);
	let mut judge = |_: &str, check: Option<&MeasuredGitGuardLine>| Ok(stub_verdict(check));
	let error = judge_corpus(&corpus, &mut judge, &|_| Verdict::Allow, &wrong).unwrap_err();
	assert!(error.to_string().contains("c1's command differs"));
}

#[test]
fn a_row_serializes_as_the_typescript_wrote_it() {
	let line = VerdictLine {
		id: "g1".to_owned(),
		command: "git status".to_owned(),
		expected: Verdict::Allow,
		variant: String::new(),
		base: String::new(),
		category: String::new(),
		omp: stub_verdict(None),
		reimplemented: Verdict::Allow,
	};
	assert_eq!(
		serde_json::to_string(&line).unwrap(),
		r#"{"id":"g1","command":"git status","expected":"allow","variant":"","base":"","category":"","verdict":"allow","mechanism":"mode-default","match":null,"approval":"allow","critical":false,"gitGuard":null,"gitGuardSource":"reimplemented","reimplemented":"allow"}"#
	);
}

/// An oracle with omp loaded, or why there is none.
fn open_oracle() -> std::result::Result<Oracle, String> {
	let mut oracle = Oracle::start().map_err(|error| error.to_string())?;
	if oracle.find().map_err(|error| error.to_string())?.is_none() {
		return Err("omp isn't installed (not on PATH, not in Bun's global install)".to_owned());
	}
	oracle.load(None, false).map_err(|error| error.to_string())?;
	Ok(oracle)
}

#[test]
fn the_oracle_judges_through_omp_s_own_approval_and_the_git_guard_refuses_what_omp_allows() {
	let mut oracle = match open_oracle() {
		Ok(oracle) => oracle,
		Err(why) => {
			println!("the oracle: skipped, {why}");
			return;
		}
	};
	let dir = temp_dir("jocasta-h1-");
	let guard = dir.path().join("guard.yml");
	std::fs::write(
		&guard,
		"bash:\n  gitGuard: true\n  patterns:\n    - { match: \"*git *push* --force\", approval: deny }\n    - { match: \"echo ask*\", approval: prompt }\n",
	)
	.unwrap();
	let init = oracle.judge_init(&[guard.as_path()], dir.path(), Some(dir.path())).unwrap();
	assert!(init.summary.git_guard);
	let forced = oracle.judge("git push origin feature --force", None).unwrap();
	assert_eq!((forced.verdict, forced.mechanism.as_str()), (Verdict::Deny, "guard-pattern"));
	assert_eq!(forced.pattern.as_deref(), Some("*git *push* --force"));
	let asked = oracle.judge("echo ask me", None).unwrap();
	assert_eq!((asked.verdict, asked.approval), (Verdict::Prompt, Verdict::Prompt));
	let stash = oracle.judge("git stash", None).unwrap();
	assert_eq!((stash.verdict, stash.mechanism.as_str(), stash.approval), (Verdict::Deny, "git-guard", Verdict::Allow));
	assert_eq!((stash.git_guard.as_deref(), stash.git_guard_source), (Some("git stash"), "reimplemented"));
	let status = oracle.judge("git status", None).unwrap();
	assert_eq!((status.verdict, status.git_guard), (Verdict::Allow, None));
	let measured =
		MeasuredGitGuardLine { command: "git status".to_owned(), refusal: Some("git status is blocked".to_owned()) };
	let measured = oracle.judge("git status", Some(&measured)).unwrap();
	assert_eq!((measured.verdict, measured.git_guard_source), (Verdict::Deny, "measured"));
}
