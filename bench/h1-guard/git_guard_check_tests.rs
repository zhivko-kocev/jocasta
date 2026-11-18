//! `test/bench/h1-guard.test.ts`'s cases for the git guard check, case for case (the half of the measured-lines case
//! that feeds `judgeCorpus` stays with `omp-verdicts.ts`, which stays TypeScript).

use regex::Regex;
use serde_json::json;

use super::*;
use crate::h1_guard::git_guard::{MeasuredGitGuardLine, parse_measured_git_guard};

/// A stand-in for omp's tokenizer: segments at `&&` and `;`, double quotes kept together and removed.
fn tokenize(command: &str) -> Result<Vec<Vec<String>>> {
	let split = Regex::new(r"&&|;").unwrap();
	let word = Regex::new(r#""[^"]*"|\S+"#).unwrap();
	Ok(split
		.split(command)
		.map(|segment| {
			word.find_iter(segment)
				.map(|found| {
					let text = found.as_str();
					let text = text.strip_prefix('"').unwrap_or(text);
					text.strip_suffix('"').unwrap_or(text).to_owned()
				})
				.collect::<Vec<String>>()
		})
		.filter(|segment| !segment.is_empty())
		.collect())
}

fn only(command: &str) -> Option<Vec<String>> {
	git_only_argv(command, &tokenize).unwrap()
}

fn words(argv: &[&str]) -> Option<Vec<String>> {
	Some(argv.iter().map(|word| (*word).to_owned()).collect())
}

#[test]
fn the_git_guard_check_picks_lines_made_only_of_git_any_spelling_cd_and_echo() {
	assert_eq!(only("cd C:\\work\\demo && git reset --hard"), words(&["git", "reset", "--hard"]));
	assert_eq!(only("echo start; git stash; echo done"), words(&["git", "stash"]));
	assert_eq!(only("pwsh -c \"git stash pop\""), words(&["git", "stash", "pop"]));
	assert_eq!(only("cmd /c \"git status\""), words(&["git", "status"]));
	let full = only("\"C:\\Program Files\\Git\\cmd\\git.exe\" push origin :dev");
	assert_eq!(full.as_ref().map(|argv| argv[0].as_str()), Some("C:\\Program Files\\Git\\cmd\\git.exe"));
	assert_eq!(only("git.exe clean -f"), words(&["git.exe", "clean", "-f"]));
	assert_eq!(only("ls && git status"), None);
	assert_eq!(only("echo hi"), None);
	assert_eq!(only("cmd /c \"rd /s /q build\""), None);
	assert_eq!(git_subcommand(&["git", "-C", "C:\\work\\demo", "reset", "--hard"]), "reset");
	assert_eq!(
		refusal_in("x\ngit stash is blocked by workspace policy — do not work around it.\nmore").as_deref(),
		Some("git stash is blocked by workspace policy — do not work around it.")
	);
	assert_eq!(refusal_in("Saved working directory"), None);
}

#[test]
fn a_line_omp_s_shell_stops_before_git_is_unmeasured_git_s_own_failures_and_the_guard_s_refusal_are_not() {
	let cd = "error: cd: i/o error: The system cannot find the file specified. (os error 2)";
	assert_eq!(shell_stopped(&format!("{cd}\n")).as_deref(), Some(cd));
	assert_eq!(shell_stopped("error: command not found: pwsh\n").as_deref(), Some("error: command not found: pwsh"));
	assert_eq!(shell_stopped("error: pathspec 'nope' did not match any file(s) known to git\n"), None);
	assert_eq!(shell_stopped("git stash is blocked by workspace policy — do not work around it.\n"), None);
	assert_eq!(shell_stopped("start\nerror: cd: i/o error\n"), None);
}

#[test]
fn the_git_guard_check_runs_only_on_the_vm_with_c_work_holding_nothing_but_the_fixture() {
	let refused = |machine: &str, entries: Option<&[&str]>| {
		let entries: Option<Vec<String>> = entries.map(|names| names.iter().map(|name| (*name).to_owned()).collect());
		require_clean_machine(machine, entries.as_deref()).map_err(|error| error.to_string())
	};
	assert!(refused("HIS-PC", None).unwrap_err().contains("only on the VM JOCASTA-CLEAN"));
	assert!(
		refused("jocasta-clean", Some(&["demo", "projects"]))
			.unwrap_err()
			.contains("holds more than the fixture (projects)")
	);
	assert!(refused("JOCASTA-CLEAN", Some(&["demo", "demo-origin.git"])).is_ok());
	assert!(refused("JOCASTA-CLEAN", None).is_ok());
}

#[test]
fn measured_git_guard_lines_agreement_per_key_and_they_replace_the_reimplementation_in_omp_s_verdicts() {
	let row = |id: &str, subcommand: &str, refusal: Option<&str>, reimplemented: Option<&str>| CheckRow {
		id: id.to_owned(),
		command: format!("git {subcommand}"),
		variant: (if id.starts_with('x') { "git-exe" } else { "as-written" }).to_owned(),
		subcommand: subcommand.to_owned(),
		measured: true,
		refusal: refusal.map(str::to_owned),
		reimplemented: reimplemented.map(str::to_owned),
		agree: refusal.is_some() == reimplemented.is_some(),
	};
	let rows = [
		row("a1", "stash", Some("git stash is blocked"), Some("git stash")),
		row("x1", "stash", Some("git stash is blocked"), None),
		row("a2", "reset", None, None),
		CheckRow { measured: false, ..row("a3", "reset", None, None) },
	];
	assert_eq!(
		agreement(&rows, |r| r.subcommand.clone()),
		[
			Agreement {
				key: "stash".into(),
				n: 2,
				agree: 1,
				disagreements: vec![Disagreement {
					id: "x1".into(),
					command: "git stash".into(),
					measured: Some("git stash is blocked".into()),
					reimplemented: None,
				}],
			},
			Agreement { key: "reset".into(), n: 1, agree: 1, disagreements: vec![] },
		]
	);
	let text = [
		json!({ "id": "c1", "command": "git stash", "measured": true, "refusal": "git stash is blocked" }),
		json!({ "id": "c2", "command": "git status", "measured": true, "refusal": null }),
		json!({ "id": "c3", "command": "git log", "measured": false, "refusal": null }),
	]
	.iter()
	.map(serde_json::Value::to_string)
	.collect::<Vec<_>>()
	.join("\n");
	let measured = parse_measured_git_guard(&text, "git-guard-check").unwrap();
	assert_eq!(measured.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
	assert_eq!(
		measured[0].1,
		MeasuredGitGuardLine { command: "git stash".into(), refusal: Some("git stash is blocked".into()) }
	);
	let missing = r#"{"id":"c1","command":"x","measured":true}"#;
	assert!(parse_measured_git_guard(missing, "g.jsonl").unwrap_err().to_string().contains("g.jsonl:1:"));
}
