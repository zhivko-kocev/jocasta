//! Tests of `review_prompt.rs` (the prompt's cases of the old `hunk_tests.rs`, now naming `review_note`, 0678).

use std::path::PathBuf;

use super::*;

fn context(rules: &[&str]) -> ReviewContext {
	ReviewContext { rules: rules.iter().map(PathBuf::from).collect(), ..ReviewContext::default() }
}

#[test]
fn the_review_prompt_tells_the_reviewer_to_hand_findings_in_through_review_note_and_to_quote_the_code() {
	let prompt = review_prompt("main...feature/x", &context(&[]));
	assert!(prompt.contains("git diff main...feature/x"));
	assert!(prompt.contains("Hand every finding in with the `review_note` tool"));
	assert!(prompt.contains("copied exactly"));
	assert!(prompt.contains("`severity` (P0 breaks the task or the build, P1 is a likely bug, both block"));
	assert!(!prompt.to_lowercase().contains("hunk"));
	assert!(!prompt.contains("comment apply"));
}

#[test]
fn the_reviewer_gets_the_plan_the_rules_and_the_lessons_and_only_material_findings_with_priorities() {
	let gates = "checks: FAIL (tree 3f9a1c)\n  FAIL  test  mise run test";
	let prompt = review_prompt(
		"main...b",
		&ReviewContext {
			plan: Some(PathBuf::from("C:\\plans\\p.md")),
			rules: vec![PathBuf::from("C:\\wt\\AGENTS.md"), PathBuf::from("C:\\repo\\AGENTS.local.md")],
			gates: Some(gates.to_owned()),
			checked: true,
			..ReviewContext::default()
		},
	);
	assert!(prompt.contains("the approved plan (C:/plans/p.md)"));
	assert!(prompt.contains("the project's rules (C:/wt/AGENTS.md, C:/repo/AGENTS.local.md)"));
	assert!(prompt.contains("skill://lessons"));
	assert!(prompt.contains("report only material findings; prefer one strong finding over several weak ones"));
	// Jocasta ran the checks: the failing gates go in, the turn runs no tests of its own and ends on no TESTS line.
	assert!(prompt.contains(&format!("they failed; give the reviewer these gates too:\n\n{gates}\n\n")));
	assert!(!prompt.contains("as one bash command"));
	assert!(!prompt.contains("TESTS:"));
	// Checks that passed say nothing.
	let passed = review_prompt(
		"main...b",
		&ReviewContext { gates: Some(String::new()), checked: true, ..ReviewContext::default() },
	);
	assert!(passed.starts_with("Check this task's work. When the project has a `verify` skill"));
	// Without a mise check, the project's own way and the TESTS line; without a plan or rules, only the lessons.
	let plain = review_prompt("main...b", &ReviewContext::default());
	assert!(plain.contains("the way its AGENTS.md or package scripts say"));
	assert!(plain.contains("read skill://verify and run it too"));
	assert!(plain.contains("`TESTS: pass — <the test command you ran>`"));
	assert!(!plain.contains("approved plan"));
	assert!(plain.contains("give it its lessons in skill://lessons"));
}

#[test]
fn the_prompt_gives_the_brief_and_the_report_on_one_line_each() {
	let prompt = review_prompt(
		"main...b",
		&ReviewContext {
			brief: Some("  fix «the»\n   bug ".to_owned()),
			report: Some(Some("did  it\n".to_owned())),
			..ReviewContext::default()
		},
	);
	assert!(prompt.contains("his brief «fix \"the\" bug», to check the diff against what he asked for"));
	assert!(prompt.contains("the thread's own report «did it» (its claims, not proof)"));
	let none = review_prompt("main...b", &ReviewContext { report: Some(None), ..ReviewContext::default() });
	assert!(none.contains("the fact that the thread handed in no report"));
}

#[test]
fn the_review_prompt_gives_the_reviewer_the_shipped_design_rubric_and_a_cap_of_three_non_blocking_design_findings_0611()
{
	let prompt = review_prompt("main...b", &ReviewContext::default());
	assert!(
		prompt.contains(&format!("{}/design-rubric.md", shipped_config_dir(None).to_string_lossy().replace('\\', "/")))
	);
	assert!(prompt.contains("at most three design findings"));
	assert!(prompt.contains("the code this change could delete"));
	assert!(prompt.contains("A design finding is P2"));
	assert!(prompt.contains("never block"));
	assert!(shipped_config_dir(None).join("design-rubric.md").exists());
}

#[test]
fn the_facts_block_goes_after_the_lead_line() {
	let prompt = review_prompt(
		"main...b",
		&ReviewContext { facts: Some("Facts as of abc1234 (x):\n3 log lines".to_owned()), ..ReviewContext::default() },
	);
	assert!(prompt.starts_with("Check this task's work. \n\nFacts as of abc1234"));
}
