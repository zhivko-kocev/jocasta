//! Tests of `pr_actions.rs`: the pull-request brief's `edit`, `reword` and `publish` (0680, 0681). The gh and omp fakes
//! are generated scripts (`.cmd` on Windows, `sh` elsewhere) that record their arguments next to themselves.

use std::fs;
use std::path::Path;

use super::*;
use crate::frontier::brief::model::{Proof, Severity};
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::pr_review::{Platform, PrTarget};
use crate::frontier::server::task_file::{NewTask, new_task, save_task};
use crate::shared::testing::{TempDir, temp_dir};

const HEAD: &str = "1111111111111111111111111111111111111111";

fn script(dir: &Path, name: &str, windows: &str, unix: &str) -> Vec<String> {
	let (file, text) = if cfg!(windows) { (format!("{name}.cmd"), windows) } else { (name.to_owned(), unix) };
	let path = dir.join(file);
	fs::write(&path, text).unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
	}
	vec![path.display().to_string()]
}

/// A gh that appends each call's arguments (newlines written as `\n`) to `gh-calls.txt`; an `api` call answers
/// `HTTP 422` and exits 1 while a file `fail-api` exists beside it. On Windows it is a PowerShell script: a `.cmd` can't
/// be given an argument with a newline, and the fallback comment has two.
fn fake_gh(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("gh.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nAdd-Content \"$here\\gh-calls.txt\" (($args -join ' ') -replace \"`r?`n\", '\\n')\r\nif ($args[0] -eq 'api' -and (Test-Path \"$here\\fail-api\")) { [Console]::Error.WriteLine('gh: Unprocessable Entity (HTTP 422)'); exit 1 }\r\n'https://github.com/acme/app/pull/7#discussion_r1'\r\n",
		)
		.unwrap();
		return ["powershell.exe", "-NoProfile", "-NonInteractive", "-File"]
			.map(str::to_owned)
			.into_iter()
			.chain([path.display().to_string()])
			.collect();
	}
	script(
		dir,
		"gh",
		"",
		"#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$@\" | tr '\\n' ' ' >> \"$dir/gh-calls.txt\"\necho >> \"$dir/gh-calls.txt\"\nif [ \"$1\" = api ] && [ -e \"$dir/fail-api\" ]; then echo 'gh: Unprocessable Entity (HTTP 422)' >&2; exit 1; fi\necho https://github.com/acme/app/pull/7#discussion_r1\n",
	)
}

/// An omp that records its arguments in `omp-args.txt` and answers with a new wording.
fn fake_omp(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("omp.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nSet-Content \"$here\\omp-args.txt\" (($args -join ' ') -replace \"`r?`n\", '\\n')\r\n'Please use a guard clause here.'\r\n",
		)
		.unwrap();
		return ["powershell.exe", "-NoProfile", "-NonInteractive", "-File"]
			.map(str::to_owned)
			.into_iter()
			.chain([path.display().to_string()])
			.collect();
	}
	script(
		dir,
		"omp",
		"",
		"#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/omp-args.txt\"\necho 'Please use a guard clause here.'\n",
	)
}

fn card(id: &str, ok: bool, verdict: Verdict) -> Card {
	Card {
		id: id.to_owned(),
		severity: Severity::P1,
		file: "src/a.rs".to_owned(),
		line: 12,
		quote: "let x = 1;".to_owned(),
		text: "This can overflow.".to_owned(),
		verdict,
		proof: Proof { sha: HEAD.to_owned(), ok },
		edited: None,
		published: None,
		ran: None,
	}
}

fn target() -> PrTarget {
	PrTarget {
		platform: Platform::Github,
		url: "https://github.com/acme/app/pull/7".to_owned(),
		number: 7,
		repo: "acme/app".to_owned(),
		org_url: None,
		project: None,
		title: "Add a thing".to_owned(),
		author: "client-a".to_owned(),
		base_branch: "main".to_owned(),
		head_sha: HEAD.to_owned(),
		base_sha: "2222222222222222222222222222222222222222".to_owned(),
		ci: "passing".to_owned(),
		threads: 0,
	}
}

struct World {
	t: TestContext,
	dir: TempDir,
	brief: String,
	task: TaskRecord,
}

/// A review thread of pull request 7 with `cards` as its notes, and its stored brief.
fn world(cards: Vec<Card>) -> World {
	let dir = temp_dir("pr-actions");
	let gh = fake_gh(dir.path());
	let omp = fake_omp(dir.path());
	let t = test_context(move |options| {
		options.gh = Some(gh);
		options.omp = Some(omp);
	});
	let project = format!("p-acme-{}", uuid::Uuid::new_v4().simple());
	let task = new_task(
		NewTask {
			project_id: project.clone(),
			project: "acme".to_owned(),
			title: "Review: Add a thing".to_owned(),
			pr: Some(target()),
			..NewTask::default()
		},
		None,
	);
	save_task(&task).unwrap();
	let notes = ReviewNotes { head: HEAD.to_owned(), notes: cards, ..ReviewNotes::default() };
	notes.write(&review_file(&task.project_id, &task.id)).unwrap();
	let brief = format!("pr-{project}-7");
	let first = rebuilt_from_nothing(&task, &notes, &brief);
	store::save(&first).unwrap();
	World { t, dir, brief, task }
}

fn rebuilt_from_nothing(task: &TaskRecord, notes: &ReviewNotes, id: &str) -> Brief {
	let empty = Brief {
		id: id.to_owned(),
		kind: "pr".to_owned(),
		title: String::new(),
		subtitle: String::new(),
		generated_at_ms: 1,
		sections: Vec::new(),
	};
	rebuilt(&empty, task, "acme", notes).unwrap()
}

impl World {
	async fn act(&self, action: Value) -> Result<Value> {
		act(&self.t.ctx, &self.brief, &action).await
	}

	fn notes(&self) -> ReviewNotes {
		ReviewNotes::read(&review_file(&self.task.project_id, &self.task.id))
	}

	fn gh_calls(&self) -> Vec<String> {
		fs::read_to_string(self.dir.path().join("gh-calls.txt"))
			.unwrap_or_default()
			.lines()
			.map(|line| line.replace('"', ""))
			.collect()
	}
}

#[tokio::test]
async fn a_note_whose_quote_wasnt_found_is_refused_and_a_good_one_is_posted_once() {
	let w = world(vec![card("n1", false, Verdict::Post), card("n2", true, Verdict::Post)]);
	let refused = w.act(json!({ "op": "publish", "card": "n1" })).await.unwrap_err().to_string();
	assert!(refused.contains("quote wasn't found"), "{refused}");
	assert!(w.gh_calls().is_empty(), "nothing ran for the refused note");

	let answer = w.act(json!({ "op": "publish", "card": "n2" })).await.unwrap();
	let calls = w.gh_calls();
	assert_eq!(calls.len(), 1, "{calls:?}");
	assert_eq!(
		calls[0].split_whitespace().take(3).collect::<Vec<_>>(),
		["api", "repos/acme/app/pulls/7/comments", "-f"]
	);
	for part in
		[format!("commit_id={HEAD}"), "path=src/a.rs".to_owned(), "-F line=12".to_owned(), "-f side=RIGHT".to_owned()]
	{
		assert!(calls[0].contains(&part), "{part} in {}", calls[0]);
	}
	assert!(calls[0].contains("body=This can overflow."), "{}", calls[0]);
	let published = w.notes().notes.iter().find(|n| n.id == "n2").and_then(|n| n.published.clone());
	assert!(published.is_some_and(|p| p.url.contains("github.com")));
	assert!(answer.to_string().contains("published"));

	let again = w.act(json!({ "op": "publish", "card": "n2" })).await.unwrap_err().to_string();
	assert!(again.contains("published already"), "{again}");
	assert_eq!(w.gh_calls().len(), 1, "no second post");
}

#[tokio::test]
async fn a_skipped_note_is_not_published() {
	let w = world(vec![card("n1", true, Verdict::Skip)]);
	let refused = w.act(json!({ "op": "publish", "card": "n1" })).await.unwrap_err().to_string();
	assert!(refused.contains("skip"), "{refused}");
	assert!(w.gh_calls().is_empty());
}

#[tokio::test]
async fn a_line_outside_the_diff_goes_on_the_pull_request_itself() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	fs::write(w.dir.path().join("fail-api"), "").unwrap();
	w.act(json!({ "op": "publish", "card": "n1" })).await.unwrap();
	let calls = w.gh_calls();
	assert_eq!(calls.len(), 2, "{calls:?}");
	assert!(calls[1].starts_with("pr comment https://github.com/acme/app/pull/7 --body"), "{}", calls[1]);
	assert!(w.notes().notes[0].published.is_some());
}

#[tokio::test]
async fn an_edit_is_stored_until_the_note_is_published() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	let answer = w.act(json!({ "op": "edit", "card": "n1", "text": "Use checked_add." })).await.unwrap();
	assert_eq!(w.notes().notes[0].edited.as_deref(), Some("Use checked_add."));
	assert!(answer.to_string().contains("Use checked_add."));
	let empty = w.act(json!({ "op": "edit", "card": "n1", "text": "  " })).await.unwrap_err().to_string();
	assert!(empty.contains("can't be empty"), "{empty}");
	w.act(json!({ "op": "publish", "card": "n1" })).await.unwrap();
	assert!(w.gh_calls()[0].contains("body=Use checked_add."), "his wording is the one posted");
	let late = w.act(json!({ "op": "edit", "card": "n1", "text": "Other." })).await.unwrap_err().to_string();
	assert!(late.contains("published already"), "{late}");
}

#[tokio::test]
async fn a_reword_is_a_suggestion_and_stores_nothing() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	let answer = w.act(json!({ "op": "reword", "card": "n1", "instruction": "shorter" })).await.unwrap();
	assert_eq!(answer["suggestion"]["card"], "n1");
	assert_eq!(answer["suggestion"]["text"], "Please use a guard clause here.");
	assert_eq!(w.notes().notes[0].edited, None, "only his Accept stores it");
	let argv = fs::read_to_string(w.dir.path().join("omp-args.txt")).unwrap();
	for flag in ["-p", "--model @smol", "--no-tools", "--no-session", "--no-extensions", "--no-skills", "--no-rules"] {
		assert!(argv.contains(flag), "{flag} in {argv}");
	}
	assert!(argv.contains("--system-prompt"), "{argv}");
	assert!(argv.contains("Rewrite the review comment as instructed."), "{argv}");
	// Its input is the instruction, the note, its place and the quote: nothing else.
	assert!(
		argv.contains("Instruction: shorter") && argv.contains("src/a.rs:12") && argv.contains("let x = 1;"),
		"{argv}"
	);
}

#[tokio::test]
async fn an_unknown_op_or_note_is_refused() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	assert!(w.act(json!({ "op": "vote", "card": "n1" })).await.unwrap_err().to_string().contains("no action"));
	assert!(w.act(json!({ "op": "publish", "card": "zz" })).await.unwrap_err().to_string().contains("no note zz"));
}
