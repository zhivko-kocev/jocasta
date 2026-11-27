//! What the pull-request brief's buttons do (0680, 0681, 0707): `edit` stores his wording of a note, `reword` asks a
//! one-shot `@smol` helper for another wording (the answer is only a suggestion until he accepts it, which is an
//! `edit`), `verdict` switches a note between `post`, `question` and `skip` (questions and opinions come as `skip`), and
//! `publish` posts one checked note as a comment on the pull request. A comment is published only by his click, only
//! for a note whose quote was found at the head, whose verdict isn't `skip` and that isn't published yet; no vote is ever
//! cast. The notes live in the thread's `review.json`; the brief is built again from them after each action, so a
//! brief keeps working after its thread is gone.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::frontier::brief::model::{Brief, Card, Published, Verdict};
use crate::frontier::brief::store;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::pr_review::{Platform, PrTarget, journal_published, json_of, last_line, rebuilt, refs};
use crate::frontier::server::review_notes::{ReviewNotes, review_file};
use crate::frontier::server::task_file::{TaskRecord, project_tasks};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, run_program};

#[cfg(test)]
#[path = "pr_actions_tests.rs"]
mod tests;

/// The helper's whole system prompt (0680).
pub const REWORD_SYSTEM: &str =
	"Rewrite the review comment as instructed. Keep every technical claim. Answer with the comment text only.";

/// The helper may take this long.
const REWORD_MS: i64 = 60_000;

/// The longest comment or instruction a click may carry.
const MAX_TEXT: usize = 20_000;

/// An edit or a publish at a time: a second click on Publish finds the note published already.
static ACTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The newest review thread of brief `pr-<project id>-<n>`, open or closed.
fn task_of(id: &str) -> Result<TaskRecord> {
	let (project, number) = id
		.strip_prefix("pr-")
		.and_then(|rest| rest.rsplit_once('-'))
		.and_then(|(project, number)| Some((project, number.parse::<u64>().ok()?)))
		.ok_or_else(|| Error::user("That brief has no actions."))?;
	project_tasks(project)
		.into_iter()
		.filter(|task| task.pr.as_ref().is_some_and(|pr| pr.number == number))
		.max_by_key(|task| task.created_at)
		.ok_or_else(|| Error::user("That review's thread record is gone; its brief is a record, not a control."))
}

fn card_of<'a>(notes: &'a mut ReviewNotes, id: &str) -> Result<&'a mut Card> {
	notes.notes.iter_mut().find(|card| card.id == id).ok_or_else(|| Error::user(format!("The brief has no note {id}.")))
}

fn text_of(action: &Value, key: &str) -> String {
	action.get(key).and_then(Value::as_str).unwrap_or("").trim().to_owned()
}

/// The note as it reads now: his wording when he edited it.
fn current(card: &Card) -> &str {
	card.edited.as_deref().unwrap_or(&card.text)
}

/// Stores `notes`, builds the brief again from them and stores that.
fn save(task: &TaskRecord, notes: &ReviewNotes, id: &str) -> Result<Brief> {
	notes.write(&review_file(&task.project_id, &task.id))?;
	let stored = store::load(id).ok_or_else(|| Error::user("That brief is gone."))?;
	let brief = rebuilt(&stored, task, &task.project, notes)?;
	store::save(&brief)?;
	Ok(brief)
}

fn brief_json(brief: &Brief) -> Result<Value> {
	serde_json::to_value(brief).map_err(|error| Error::other(error.to_string()))
}

/// Runs `action` (`edit`, `reword`, `verdict` or `publish`) on the pull-request brief `id`; the brief as it is after,
/// whole. A `reword` adds `suggestion: {card, text}` to it: nothing is stored until he accepts.
pub async fn act(c: &Arc<ServiceContext>, id: &str, action: &Value) -> Result<Value> {
	let task = task_of(id)?;
	let op = action.get("op").and_then(Value::as_str).unwrap_or("");
	let card_id = text_of(action, "card");
	match op {
		"reword" => {
			let instruction = text_of(action, "instruction");
			if instruction.is_empty() || instruction.chars().count() > MAX_TEXT {
				return Err(Error::user("reword: say how the comment should change."));
			}
			let notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			let card = notes
				.notes
				.iter()
				.find(|card| card.id == card_id)
				.ok_or_else(|| Error::user(format!("The brief has no note {card_id}.")))?;
			let suggestion = reword(c, card, &instruction).await?;
			let stored = store::load(id).ok_or_else(|| Error::user("That brief is gone."))?;
			let mut answer = brief_json(&stored)?;
			answer["suggestion"] = json!({ "card": card_id, "text": suggestion });
			Ok(answer)
		}
		"edit" => {
			let text = text_of(action, "text");
			if text.is_empty() || text.chars().count() > MAX_TEXT {
				return Err(Error::user("edit: the comment can't be empty."));
			}
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			let card = card_of(&mut notes, &card_id)?;
			if card.published.is_some() {
				return Err(Error::user("edit: that comment is published already."));
			}
			card.edited = (text != card.text).then_some(text);
			brief_json(&save(&task, &notes, id)?)
		}
		"verdict" => {
			let verdict = match text_of(action, "verdict").as_str() {
				"post" => Verdict::Post,
				"question" => Verdict::Question,
				"skip" => Verdict::Skip,
				_ => return Err(Error::user("verdict: it is post, question or skip.")),
			};
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			let card = card_of(&mut notes, &card_id)?;
			if card.published.is_some() {
				return Err(Error::user("verdict: that comment is published already."));
			}
			card.verdict = verdict;
			brief_json(&save(&task, &notes, id)?)
		}
		"publish" => {
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			let card = card_of(&mut notes, &card_id)?;
			let pr = task.pr.clone().ok_or_else(|| Error::user("That brief has no actions."))?;
			refuse_unless_publishable(card)?;
			let (url, platform) = publish(c, &pr, card).await?;
			card.published = Some(Published { url, at_ms: now_ms() });
			journal_published(c, &task, platform);
			brief_json(&save(&task, &notes, id)?)
		}
		other => Err(Error::user(format!("The brief has no action `{other}`."))),
	}
}

/// `publish`'s refusals: a note whose quote wasn't found at the head, one for him only, one already published.
fn refuse_unless_publishable(card: &Card) -> Result<()> {
	if card.published.is_some() {
		return Err(Error::user("publish: that comment is published already."));
	}
	if !card.proof.ok {
		return Err(Error::user("publish: that note's quote wasn't found at the head; it can't be published."));
	}
	if card.verdict == Verdict::Skip {
		return Err(Error::user("publish: that note is marked skip."));
	}
	Ok(())
}

/// The one-shot helper (0680): `omp -p --model @smol` with no tools, session, extensions, skills or rules and the fixed
/// system prompt. Its input is the instruction, the note's text, `file:line` and the quote, sealed as model text
/// (0681); its answer is the suggested comment.
async fn reword(c: &Arc<ServiceContext>, card: &Card, instruction: &str) -> Result<String> {
	let input = format!(
		"Instruction: {instruction}\n\nComment:\n{}\n\nLocation: {}:{}\n\nCode:\n{}\n",
		current(card),
		card.file,
		card.line,
		card.quote
	);
	let sealed = c.bridge.seal(&input);
	let mut argv = c.panes.omp().to_vec();
	argv.extend(
		[
			"-p",
			"--model",
			"@smol",
			"--no-tools",
			"--no-session",
			"--no-extensions",
			"--no-skills",
			"--no-rules",
			"--system-prompt",
			REWORD_SYSTEM,
		]
		.map(str::to_owned),
	);
	argv.push(sealed.as_str().to_owned());
	// The service's own env with the panes' (omp needs its `PI_*`).
	let mut env: HashMap<String, String> = crate::shared::env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect();
	env.extend(c.panes.pane_env().iter().cloned());
	let run = run_program(
		&argv,
		RunOptions {
			cwd: Some(crate::shared::paths::data_dir()),
			env: Some(env),
			timeout_ms: Some(REWORD_MS),
			..RunOptions::default()
		},
	)
	.await
	.map_err(|error| Error::user(format!("reword: omp didn't start: {error}")))?;
	let answer = run.stdout.trim().to_owned();
	if run.timed_out {
		return Err(Error::user("reword: the helper didn't answer in 60 s."));
	}
	if run.code != Some(0) || answer.is_empty() {
		let said = if run.stderr.trim().is_empty() { &run.stdout } else { &run.stderr };
		return Err(Error::user(format!("reword: the helper failed: {}", last_line(said))));
	}
	Ok(answer.chars().take(MAX_TEXT).collect())
}

/// Posts the note as a comment (the comment's page, and where); the CLI's last line when it refuses, with nothing stored.
async fn publish(c: &Arc<ServiceContext>, pr: &PrTarget, card: &Card) -> Result<(String, Platform)> {
	let text = current(card).to_owned();
	match pr.platform {
		Platform::Github => Ok((publish_github(c, pr, card, &text).await?, Platform::Github)),
		Platform::Azure => Ok((publish_azure(c, pr, card, &text).await?, Platform::Azure)),
	}
}

/// The first `http` word of a CLI's output, or `html_url` of its JSON.
fn url_in(out: &str, fallback: &str) -> String {
	if let Some(url) = serde_json::from_str::<Value>(out)
		.ok()
		.and_then(|value| value.get("html_url").and_then(Value::as_str).map(str::to_owned))
	{
		return url;
	}
	out.split_whitespace().find(|word| word.starts_with("http")).unwrap_or(fallback).to_owned()
}

async fn publish_github(c: &Arc<ServiceContext>, pr: &PrTarget, card: &Card, text: &str) -> Result<String> {
	let data = crate::shared::paths::data_dir();
	let route = format!("repos/{}/pulls/{}/comments", pr.repo, pr.number);
	let args: Vec<String> = [
		"api",
		&route,
		"-f",
		&format!("body={text}"),
		"-f",
		&format!("commit_id={}", pr.head_sha),
		"-f",
		&format!("path={}", card.file),
		"-F",
		&format!("line={}", card.line),
		"-f",
		"side=RIGHT",
	]
	.map(str::to_owned)
	.to_vec();
	let Some(run) = c.finisher.gh(&refs(&args), &data).await else {
		return Err(Error::user("publish: gh isn't installed; nothing was posted."));
	};
	if run.code == Some(0) {
		return Ok(url_in(&run.out, &pr.url));
	}
	// A line outside the diff: GitHub answers 422, so the comment goes on the pull request itself, with its place.
	if run.out.contains("HTTP 422") {
		let body = format!("{}:{}\n\n{text}", card.file, card.line);
		let Some(run) = c.finisher.gh(&["pr", "comment", &pr.url, "--body", &body], &data).await else {
			return Err(Error::user("publish: gh isn't installed; nothing was posted."));
		};
		if run.code == Some(0) {
			return Ok(url_in(&run.out, &pr.url));
		}
		return Err(Error::user(format!("publish: {}", last_line(&run.out))));
	}
	Err(Error::user(format!("publish: {}", last_line(&run.out))))
}

async fn publish_azure(c: &Arc<ServiceContext>, pr: &PrTarget, card: &Card, text: &str) -> Result<String> {
	let (Some(org), Some(project)) = (pr.org_url.as_deref(), pr.project.as_deref()) else {
		return Err(Error::user("publish: the pull request's organisation isn't recorded."));
	};
	let line = card.line;
	let body = json!({
		"comments": [{ "parentCommentId": 0, "content": text, "commentType": 1 }],
		"status": 1,
		"threadContext": {
			"filePath": format!("/{}", card.file),
			"rightFileStart": { "line": line, "offset": 1 },
			"rightFileEnd": { "line": line, "offset": 1 },
		},
	});
	let file = std::env::temp_dir().join(format!("jocasta-thread-{}-{}.json", std::process::id(), now_ms()));
	std::fs::write(&file, body.to_string())?;
	let args: Vec<String> = [
		"devops",
		"invoke",
		"--org",
		org,
		"--area",
		"git",
		"--resource",
		"pullRequestThreads",
		"--route-parameters",
		&format!("project={project}"),
		&format!("repositoryId={}", pr.repo),
		&format!("pullRequestId={}", pr.number),
		"--http-method",
		"POST",
		"--api-version",
		"7.1",
		"--in-file",
		&file.to_string_lossy(),
		"-o",
		"json",
	]
	.map(str::to_owned)
	.to_vec();
	let run = c.finisher.az(&refs(&args), &crate::shared::paths::data_dir()).await;
	let _ = std::fs::remove_file(&file);
	let answer = json_of_publish(run)?;
	Ok(match answer.get("id").and_then(Value::as_u64) {
		Some(id) => format!("{}?discussionId={id}", pr.url),
		None => pr.url.clone(),
	})
}

fn json_of_publish(run: Option<crate::frontier::server::finish::GhRun>) -> Result<Value> {
	let Some(run) = run else { return Err(Error::user("publish: az isn't installed; nothing was posted.")) };
	if run.code != Some(0) {
		return Err(Error::user(format!("publish: {}", last_line(&run.out))));
	}
	Ok(json_of(Some(run), "az devops invoke").unwrap_or(Value::Null))
}
