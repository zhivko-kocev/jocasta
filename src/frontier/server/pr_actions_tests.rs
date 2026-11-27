//! Tests of `pr_actions.rs`: the pull-request brief's `edit`, `polish`, `english`, `verdict`, `add`, `summary`,
//! `publish` and `submit` (0680, 0681, 0707, 0711, 0712). The gh, az and omp fakes are generated scripts (PowerShell on
//! Windows, `sh` elsewhere) that record their arguments next to themselves.

use std::fs;
use std::path::Path;

use super::*;
use crate::frontier::brief::model::{Kind, Proof, Severity};
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

/// A gh that appends each call's arguments (newlines written as `\n`) to `gh-calls.txt` and copies the file given after
/// `--input` to `gh-input.json`; an `api` call answers `HTTP 422` and exits 1 while a file `fail-api` exists beside it.
/// On Windows it is a PowerShell script: a `.cmd` can't be given an argument with a newline, and the fallback comment
/// has two.
fn fake_gh(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("gh.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nAdd-Content \"$here\\gh-calls.txt\" (($args -join ' ') -replace \"`r?`n\", '\\n')\r\n$i = [array]::IndexOf($args, '--input')\r\nif ($i -ge 0) { Copy-Item $args[$i + 1] \"$here\\gh-input.json\" }\r\nif ($args[0] -eq 'api' -and (Test-Path \"$here\\fail-api\")) { [Console]::Error.WriteLine('gh: Unprocessable Entity (HTTP 422)'); exit 1 }\r\n'https://github.com/acme/app/pull/7#discussion_r1'\r\n",
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
		"#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$@\" | tr '\\n' ' ' >> \"$dir/gh-calls.txt\"\necho >> \"$dir/gh-calls.txt\"\nprev=\nfor a in \"$@\"; do if [ \"$prev\" = --input ]; then cp \"$a\" \"$dir/gh-input.json\"; fi; prev=$a; done\nif [ \"$1\" = api ] && [ -e \"$dir/fail-api\" ]; then echo 'gh: Unprocessable Entity (HTTP 422)' >&2; exit 1; fi\necho https://github.com/acme/app/pull/7#discussion_r1\n",
	)
}

/// An omp that records its arguments in `omp-args.txt` and answers with a new wording; while a file `no-model` exists
/// beside it, it says what omp says of a role with no model, and exits 1.
fn fake_omp(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("omp.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nSet-Content \"$here\\omp-args.txt\" (($args -join ' ') -replace \"`r?`n\", '\\n')\r\nif (Test-Path \"$here\\no-model\") { [Console]::Error.WriteLine('Model \"@polish\" not found'); exit 1 }\r\n'Please use a guard clause here.'\r\n",
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
		"#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$@\" > \"$dir/omp-args.txt\"\nif [ -e \"$dir/no-model\" ]; then echo 'Model \"@polish\" not found' >&2; exit 1; fi\necho 'Please use a guard clause here.'\n",
	)
}

/// An az that appends each call's arguments to `az-calls.txt` and answers a new comment thread's JSON.
fn fake_az(dir: &Path) -> Vec<String> {
	if cfg!(windows) {
		let path = dir.join("az.ps1");
		fs::write(
			&path,
			"$here = Split-Path $MyInvocation.MyCommand.Path\r\nAdd-Content \"$here\\az-calls.txt\" ($args -join ' ')\r\n'{\"id\": 5}'\r\n",
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
		"az",
		"",
		"#!/bin/sh\ndir=$(dirname \"$0\")\necho \"$@\" >> \"$dir/az-calls.txt\"\necho '{\"id\": 5}'\n",
	)
}

fn card(id: &str, ok: bool, verdict: Verdict) -> Card {
	Card {
		id: id.to_owned(),
		kind: Kind::Finding,
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

/// A review thread of pull request 7 on GitHub with `cards` as its notes, and its stored brief.
fn world(cards: Vec<Card>) -> World {
	world_on(cards, target())
}

/// A review thread of pull request 7 at `pr` with `cards` as its notes, and its stored brief.
fn world_on(cards: Vec<Card>, pr: PrTarget) -> World {
	let dir = temp_dir("pr-actions");
	let gh = fake_gh(dir.path());
	let omp = fake_omp(dir.path());
	let az = fake_az(dir.path());
	let t = test_context(move |options| {
		options.gh = Some(gh);
		options.omp = Some(omp);
		options.az = Some(az);
	});
	let project = format!("p-acme-{}", uuid::Uuid::new_v4().simple());
	let task = new_task(
		NewTask {
			project_id: project.clone(),
			project: "acme".to_owned(),
			title: "Review: Add a thing".to_owned(),
			pr: Some(pr),
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
async fn a_question_waits_as_skip_until_he_switches_its_verdict_then_publishes_0707() {
	let w = world(vec![Card { kind: Kind::Question, ..card("n1", true, Verdict::Skip) }]);
	let bad = w.act(json!({ "op": "verdict", "card": "n1", "verdict": "maybe" })).await.unwrap_err().to_string();
	assert!(bad.contains("post, question or skip"), "{bad}");
	w.act(json!({ "op": "verdict", "card": "n1", "verdict": "question" })).await.unwrap();
	assert_eq!(w.notes().notes[0].verdict, Verdict::Question);
	w.act(json!({ "op": "publish", "card": "n1" })).await.unwrap();
	let late = w.act(json!({ "op": "verdict", "card": "n1", "verdict": "skip" })).await.unwrap_err().to_string();
	assert!(late.contains("published already"), "{late}");
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
async fn polish_and_to_english_are_suggestions_on_his_text_only_and_store_nothing_0711() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	let answer = w.act(json!({ "op": "polish", "card": "n1", "text": "this can overflows" })).await.unwrap();
	assert_eq!(answer["suggestion"]["card"], "n1");
	assert_eq!(answer["suggestion"]["text"], "Please use a guard clause here.");
	assert_eq!(w.notes().notes[0].edited, None, "only his Accept stores it");
	let argv = fs::read_to_string(w.dir.path().join("omp-args.txt")).unwrap();
	for flag in ["-p", "--model @polish", "--no-tools", "--no-session", "--no-extensions", "--no-skills", "--no-rules"]
	{
		assert!(argv.contains(flag), "{flag} in {argv}");
	}
	assert!(argv.contains(&format!("--system-prompt {POLISH_SYSTEM}")), "{argv}");
	// Its input is his text: not the note's place or quote.
	assert!(argv.trim_end().ends_with("this can overflows"), "{argv}");
	assert!(!argv.contains("src/a.rs") && !argv.contains("let x = 1;"), "{argv}");
	w.act(json!({ "op": "english", "card": "summary", "text": "Добро е." })).await.unwrap();
	let argv = fs::read_to_string(w.dir.path().join("omp-args.txt")).unwrap();
	assert!(argv.contains(&format!("--system-prompt {ENGLISH_SYSTEM}")), "{argv}");
	let empty = w.act(json!({ "op": "polish", "card": "n1", "text": " " })).await.unwrap_err().to_string();
	assert!(empty.contains("no text"), "{empty}");
	let stray = w.act(json!({ "op": "polish", "card": "zz", "text": "x" })).await.unwrap_err().to_string();
	assert!(stray.contains("no note zz"), "{stray}");
}

#[tokio::test]
async fn without_a_model_for_the_polish_role_the_card_says_so_0711() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	fs::write(w.dir.path().join("no-model"), "").unwrap();
	let polish = w.act(json!({ "op": "polish", "card": "n1", "text": "x" })).await.unwrap_err().to_string();
	assert_eq!(polish, "Polish isn't available: no model answers for the polish role.");
	let english = w.act(json!({ "op": "english", "card": "n1", "text": "x" })).await.unwrap_err().to_string();
	assert_eq!(english, "To English isn't available: no model answers for the polish role.");
}

#[tokio::test]
async fn his_own_comment_is_a_note_marked_yours_and_his_summary_is_kept_0711() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	w.act(json!({ "op": "add", "text": "Thanks, this reads well." })).await.unwrap();
	let his = w.notes().notes[1].clone();
	assert_eq!((his.id.as_str(), his.kind, his.verdict, his.file.as_str()), ("c2", Kind::Yours, Verdict::Post, ""));
	assert!(his.ran.is_none() && his.proof.ok);
	let no_file = w.act(json!({ "op": "add", "text": "x", "line": 3 })).await.unwrap_err().to_string();
	assert!(no_file.contains("a line needs its file"), "{no_file}");
	let no_line = w.act(json!({ "op": "add", "text": "x", "file": "src/a.rs" })).await.unwrap_err().to_string();
	assert!(no_line.contains("give the line"), "{no_line}");
	let empty = w.act(json!({ "op": "add", "text": "" })).await.unwrap_err().to_string();
	assert!(empty.contains("can't be empty"), "{empty}");
	// Published on its own, it is a comment on the pull request.
	w.act(json!({ "op": "publish", "card": "c2" })).await.unwrap();
	assert!(
		w.gh_calls()[0].starts_with("pr comment https://github.com/acme/app/pull/7 --body Thanks"),
		"{:?}",
		w.gh_calls()
	);
	let answer = w.act(json!({ "op": "summary", "text": "Looks good overall." })).await.unwrap();
	assert_eq!(w.notes().summary, "Looks good overall.");
	assert!(answer.to_string().contains("Looks good overall."), "the verdict box carries it");
}

#[tokio::test]
async fn submit_sends_one_github_review_with_his_summary_and_every_kept_note_once_0712() {
	let skipped = Card { kind: Kind::Opinion, ..card("n2", true, Verdict::Skip) };
	let w = world(vec![card("n1", true, Verdict::Post), skipped]);
	let wordless = w.act(json!({ "op": "submit", "verdict": "request_changes" })).await.unwrap_err().to_string();
	assert!(wordless.contains("write a summary first") && w.gh_calls().is_empty(), "{wordless}");
	w.act(json!({ "op": "add", "text": "One more thing." })).await.unwrap();
	w.act(json!({ "op": "summary", "text": "Please fix the overflow." })).await.unwrap();
	let calls_before = w.gh_calls().len();
	w.act(json!({ "op": "submit", "verdict": "request_changes" })).await.unwrap();
	let calls = w.gh_calls();
	assert_eq!(calls.len(), calls_before + 1, "{calls:?}");
	assert!(calls.last().unwrap().starts_with("api repos/acme/app/pulls/7/reviews --method POST --input"));
	let sent: Value = serde_json::from_str(&fs::read_to_string(w.dir.path().join("gh-input.json")).unwrap()).unwrap();
	assert_eq!(sent["event"], "REQUEST_CHANGES");
	assert_eq!(sent["commit_id"], HEAD);
	assert_eq!(sent["body"], "Please fix the overflow.\n\nOne more thing.", "a note without a file joins the body");
	assert_eq!(
		sent["comments"],
		json!([{ "path": "src/a.rs", "line": 12, "side": "RIGHT", "body": "This can overflow." }])
	);
	let notes = w.notes();
	assert!(notes.notes[0].published.is_some() && notes.notes[2].published.is_some());
	assert!(notes.notes[1].published.is_none(), "a skipped opinion stays his");
	assert_eq!(notes.submitted.as_ref().map(|s| s.verdict.as_str()), Some("request_changes"));
	let again = w.act(json!({ "op": "submit", "verdict": "approve" })).await.unwrap_err().to_string();
	assert!(again.contains("submitted already"), "{again}");
}

#[tokio::test]
async fn a_github_review_the_cli_refuses_stores_nothing_0712() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	fs::write(w.dir.path().join("fail-api"), "").unwrap();
	let refused = w.act(json!({ "op": "submit", "verdict": "approve" })).await.unwrap_err().to_string();
	assert!(refused.contains("nothing was sent") && refused.contains("outside the diff"), "{refused}");
	assert!(w.notes().submitted.is_none() && w.notes().notes[0].published.is_none());
}

#[tokio::test]
async fn on_azure_devops_submit_publishes_each_kept_note_and_the_summary_then_votes_0712() {
	let azure = PrTarget {
		platform: Platform::Azure,
		url: "https://dev.azure.com/example-org/acme/_git/app/pullrequest/7".to_owned(),
		repo: "app".to_owned(),
		org_url: Some("https://dev.azure.com/example-org".to_owned()),
		project: Some("acme".to_owned()),
		..target()
	};
	let w = world_on(vec![card("n1", true, Verdict::Post), card("n2", true, Verdict::Skip)], azure);
	w.act(json!({ "op": "summary", "text": "Fine by me." })).await.unwrap();
	w.act(json!({ "op": "submit", "verdict": "approve" })).await.unwrap();
	let calls: Vec<String> =
		fs::read_to_string(w.dir.path().join("az-calls.txt")).unwrap().lines().map(str::to_owned).collect();
	assert_eq!(calls.len(), 3, "the note, the summary, the vote: {calls:?}");
	assert!(calls[0].starts_with("devops invoke") && calls[1].starts_with("devops invoke"));
	assert!(calls[2].starts_with("repos pr set-vote --id 7 --vote approve --org https://dev.azure.com/example-org"));
	assert!(w.notes().notes[0].published.as_ref().is_some_and(|p| p.url.ends_with("?discussionId=5")));
	assert!(w.notes().notes[1].published.is_none());
}

#[tokio::test]
async fn a_comment_verdict_on_azure_devops_casts_no_vote_0712() {
	let azure = PrTarget {
		platform: Platform::Azure,
		org_url: Some("https://dev.azure.com/example-org".to_owned()),
		project: Some("acme".to_owned()),
		..target()
	};
	let w = world_on(vec![card("n1", true, Verdict::Post)], azure);
	w.act(json!({ "op": "submit", "verdict": "comment" })).await.unwrap();
	let calls = fs::read_to_string(w.dir.path().join("az-calls.txt")).unwrap();
	assert!(!calls.contains("set-vote"), "{calls}");
}

#[tokio::test]
async fn an_unknown_op_or_note_is_refused() {
	let w = world(vec![card("n1", true, Verdict::Post)]);
	assert!(w.act(json!({ "op": "vote", "card": "n1" })).await.unwrap_err().to_string().contains("no action"));
	assert!(w.act(json!({ "op": "publish", "card": "zz" })).await.unwrap_err().to_string().contains("no note zz"));
}
