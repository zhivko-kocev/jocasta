//! What the pull-request brief's buttons do (0680, 0681, 0707, 0711, 0712): `edit` stores his wording of a note,
//! `polish` and `english` ask a one-shot `@polish` helper to fix the language of his text or put it into English (the
//! answer is only a suggestion until he accepts it, which is an `edit` or a `summary`), `verdict` switches a note between
//! `post`, `question` and `skip` (questions and opinions come as `skip`), `add` makes a note of his own, `summary` keeps
//! his words for the whole review, `publish` posts one checked note as a comment on the pull request, and `submit` sends
//! the review with his verdict: on GitHub one review with his summary and every kept note, on Azure DevOps each kept note
//! and the summary as comments and then his vote. Nothing is published but by his click, only for a note whose quote was
//! found at the head, whose verdict isn't `skip` and that isn't published yet. The notes live in the thread's
//! `review.json`; the brief is built again from them after each action, so a brief keeps working after its thread is
//! gone.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::frontier::brief::model::{Brief, Card, Kind, Proof, Published, Severity, Submitted, Verdict};
use crate::frontier::brief::store;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::pr_review::{Platform, PrTarget, journal_published, json_of, last_line, rebuilt, refs};
use crate::frontier::server::review_notes::{ReviewNotes, git_out_raw, review_file};
use crate::frontier::server::task_file::{TaskRecord, project_tasks};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, run_program};

#[cfg(test)]
#[path = "pr_actions_tests.rs"]
mod tests;

/// Polish's whole system prompt (0711).
pub const POLISH_SYSTEM: &str = "Fix the grammar, spelling and phrasing of this review comment. Keep its language, its meaning, every technical claim and every identifier exactly. Do not add or remove points. Answer with the comment text only.";

/// To English's whole system prompt (0711).
pub const ENGLISH_SYSTEM: &str = "Translate this review comment into clear English. Keep every technical claim and identifier exactly. Do not add or remove points. Answer with the comment text only.";

/// The omp role both helpers run on (0711).
const POLISH_ROLE: &str = "@polish";

/// What `card` names when the text is the review's summary.
pub const SUMMARY: &str = "summary";

/// The helper may take this long.
const HELPER_MS: i64 = 60_000;

/// The longest comment or summary a click may carry.
const MAX_TEXT: usize = 20_000;

/// One action at a time: a second click on Publish or Submit finds it done already.
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

/// A note he would send: not `skip`, checked at the head, not published yet.
fn kept(card: &Card) -> bool {
	card.verdict != Verdict::Skip && card.proof.ok && card.published.is_none()
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

/// Runs `action` on the pull-request brief `id`; the brief as it is after, whole. A `polish` or `english` adds
/// `suggestion: {card, text}` to it: nothing is stored until he accepts.
pub async fn act(c: &Arc<ServiceContext>, id: &str, action: &Value) -> Result<Value> {
	let task = task_of(id)?;
	let op = action.get("op").and_then(Value::as_str).unwrap_or("");
	let card_id = text_of(action, "card");
	match op {
		"polish" | "english" => {
			let text = text_of(action, "text");
			if text.is_empty() || text.chars().count() > MAX_TEXT {
				return Err(Error::user(format!("{op}: there is no text to work on.")));
			}
			if card_id != SUMMARY {
				let notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
				if !notes.notes.iter().any(|card| card.id == card_id) {
					return Err(Error::user(format!("The brief has no note {card_id}.")));
				}
			}
			let suggestion = helper(c, op, &text).await?;
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
		"add" => {
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			let card = his_card(c, &task, action, notes.notes.len() + 1).await?;
			notes.notes.push(card);
			brief_json(&save(&task, &notes, id)?)
		}
		"summary" => {
			let text = text_of(action, "text");
			if text.chars().count() > MAX_TEXT {
				return Err(Error::user("summary: that is longer than a review takes."));
			}
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			if notes.submitted.is_some() {
				return Err(Error::user("summary: this review was submitted already."));
			}
			notes.summary = text;
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
			journal_published(c, &task, platform, "comment");
			brief_json(&save(&task, &notes, id)?)
		}
		"submit" => {
			let verdict = text_of(action, "verdict");
			if !matches!(verdict.as_str(), "approve" | "request_changes" | "comment") {
				return Err(Error::user("submit: the verdict is approve, request_changes or comment."));
			}
			let _one = ACTING.lock().await;
			let mut notes = ReviewNotes::read(&review_file(&task.project_id, &task.id));
			if notes.submitted.is_some() {
				return Err(Error::user("submit: this review was submitted already."));
			}
			let pr = task.pr.clone().ok_or_else(|| Error::user("That brief has no actions."))?;
			let outcome = match pr.platform {
				Platform::Github => submit_github(c, &pr, &mut notes, &verdict).await,
				Platform::Azure => submit_azure(c, &pr, &mut notes, &verdict).await,
			};
			// Azure's comments go one by one: those that went out stay published even when a later step failed.
			let url = match outcome {
				Ok(url) => url,
				Err(error) => {
					save(&task, &notes, id)?;
					return Err(error);
				}
			};
			notes.submitted = Some(Submitted { verdict, url, at_ms: now_ms() });
			journal_published(c, &task, pr.platform, "review");
			brief_json(&save(&task, &notes, id)?)
		}
		other => Err(Error::user(format!("The brief has no action `{other}`."))),
	}
}

/// `add`'s note (0711): his text, kind `yours`, P2, to post, with no proof. With a file it needs a line the head has,
/// and quotes that line; without one it is about the pull request as a whole.
async fn his_card(c: &Arc<ServiceContext>, task: &TaskRecord, action: &Value, number: usize) -> Result<Card> {
	let text = text_of(action, "text");
	if text.is_empty() || text.chars().count() > MAX_TEXT {
		return Err(Error::user("add: the comment can't be empty."));
	}
	let pr = task.pr.as_ref().ok_or_else(|| Error::user("That brief has no actions."))?;
	let file = text_of(action, "file");
	let line = match action.get("line") {
		None | Some(Value::Null) => None,
		Some(Value::Number(number)) => number.as_u64(),
		Some(Value::String(digits)) if digits.trim().is_empty() => None,
		Some(Value::String(digits)) => digits.trim().parse::<u64>().ok(),
		Some(_) => Some(0),
	};
	let (line, quote) = match (file.is_empty(), line) {
		(true, None) => (0, String::new()),
		(true, Some(_)) => return Err(Error::user("add: a line needs its file.")),
		(false, None | Some(0)) => return Err(Error::user("add: give the line in that file (1 or more).")),
		(false, Some(line)) => {
			let line = usize::try_from(line).map_err(|_| Error::user("add: that line isn't in the file."))?;
			let repo = c.places.find(&task.project_id)?.main_path;
			let short = &pr.head_sha[..pr.head_sha.len().min(7)];
			let Some(content) = git_out_raw(&repo, &pr.head_sha, &file).await? else {
				return Err(Error::user(format!("add: {file} isn't in the pull request's head {short}.")));
			};
			let Some(code) = content.lines().nth(line - 1) else {
				return Err(Error::user(format!("add: {file} at {short} has {} lines.", content.lines().count())));
			};
			(line, code.trim().to_owned())
		}
	};
	Ok(Card {
		id: format!("c{number}"),
		kind: Kind::Yours,
		severity: Severity::P2,
		file,
		line,
		quote,
		text,
		verdict: Verdict::Post,
		proof: Proof { sha: pr.head_sha.clone(), ok: true },
		edited: None,
		published: None,
		ran: None,
	})
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

/// The one-shot helper (0711): `omp -p --model @polish` with no tools, session, extensions, skills or rules and the
/// fixed system prompt of `op` (`polish` or `english`). Its input is his text only, sealed as model text (0681); its
/// answer is the suggested text.
async fn helper(c: &Arc<ServiceContext>, op: &str, text: &str) -> Result<String> {
	let (system, name) = if op == "english" { (ENGLISH_SYSTEM, "To English") } else { (POLISH_SYSTEM, "Polish") };
	let sealed = c.bridge.seal(text);
	let mut argv = c.panes.omp().to_vec();
	argv.extend(
		[
			"-p",
			"--model",
			POLISH_ROLE,
			"--no-tools",
			"--no-session",
			"--no-extensions",
			"--no-skills",
			"--no-rules",
			"--system-prompt",
			system,
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
			timeout_ms: Some(HELPER_MS),
			..RunOptions::default()
		},
	)
	.await
	.map_err(|error| Error::user(format!("{op}: omp didn't start: {error}")))?;
	let answer = run.stdout.trim().to_owned();
	if run.timed_out {
		return Err(Error::user(format!("{op}: the helper didn't answer in 60 s.")));
	}
	if run.code != Some(0) || answer.is_empty() {
		let said = if run.stderr.trim().is_empty() { &run.stdout } else { &run.stderr };
		// omp names the role it couldn't resolve: no model answers for it (0711).
		if said.contains(POLISH_ROLE) {
			return Err(Error::user(format!("{name} isn't available: no model answers for the polish role.")));
		}
		return Err(Error::user(format!("{op}: the helper failed: {}", last_line(said))));
	}
	Ok(answer.chars().take(MAX_TEXT).collect())
}

/// Posts the note as a comment (the comment's page, and where); the CLI's last line when it refuses, with nothing stored.
async fn publish(c: &Arc<ServiceContext>, pr: &PrTarget, card: &Card) -> Result<(String, Platform)> {
	let text = current(card).to_owned();
	let place = (!card.file.is_empty()).then_some((card.file.as_str(), card.line));
	match pr.platform {
		Platform::Github => Ok((publish_github(c, pr, card, &text).await?, Platform::Github)),
		Platform::Azure => Ok((publish_azure(c, pr, place, &text).await?, Platform::Azure)),
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

/// A comment on the pull request itself (`gh pr comment`).
async fn comment_github(c: &Arc<ServiceContext>, pr: &PrTarget, body: &str) -> Result<String> {
	let data = crate::shared::paths::data_dir();
	let Some(run) = c.finisher.gh(&["pr", "comment", &pr.url, "--body", body], &data).await else {
		return Err(Error::user("publish: gh isn't installed; nothing was posted."));
	};
	if run.code == Some(0) {
		return Ok(url_in(&run.out, &pr.url));
	}
	Err(Error::user(format!("publish: {}", last_line(&run.out))))
}

async fn publish_github(c: &Arc<ServiceContext>, pr: &PrTarget, card: &Card, text: &str) -> Result<String> {
	// A note about the pull request as a whole (his own, 0711) is a comment on it.
	if card.file.is_empty() {
		return comment_github(c, pr, text).await;
	}
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
		return comment_github(c, pr, &format!("{}:{}\n\n{text}", card.file, card.line)).await;
	}
	Err(Error::user(format!("publish: {}", last_line(&run.out))))
}

/// A comment thread on an Azure DevOps pull request: on `place` (file and line) when given, else on the pull request.
async fn publish_azure(
	c: &Arc<ServiceContext>,
	pr: &PrTarget,
	place: Option<(&str, usize)>,
	text: &str,
) -> Result<String> {
	let (Some(org), Some(project)) = (pr.org_url.as_deref(), pr.project.as_deref()) else {
		return Err(Error::user("publish: the pull request's organisation isn't recorded."));
	};
	let mut body = json!({
		"comments": [{ "parentCommentId": 0, "content": text, "commentType": 1 }],
		"status": 1,
	});
	if let Some((file, line)) = place {
		body["threadContext"] = json!({
			"filePath": format!("/{file}"),
			"rightFileStart": { "line": line, "offset": 1 },
			"rightFileEnd": { "line": line, "offset": 1 },
		});
	}
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

/// GitHub's review (0712): one `POST repos/<o>/<r>/pulls/<n>/reviews` with the event, the head, his summary as the body
/// (a kept note without a file joins it) and every kept note with a file and line as a line comment. The notes it sent
/// are marked published with the review's link; the review's link.
async fn submit_github(
	c: &Arc<ServiceContext>,
	pr: &PrTarget,
	notes: &mut ReviewNotes,
	verdict: &str,
) -> Result<String> {
	let event = match verdict {
		"approve" => "APPROVE",
		"request_changes" => "REQUEST_CHANGES",
		_ => "COMMENT",
	};
	let mut body: Vec<&str> = Vec::new();
	if !notes.summary.trim().is_empty() {
		body.push(notes.summary.trim());
	}
	let mut comments = Vec::new();
	for card in notes.notes.iter().filter(|card| kept(card)) {
		if card.file.is_empty() {
			body.push(current(card));
		} else {
			comments.push(json!({ "path": card.file, "line": card.line, "side": "RIGHT", "body": current(card) }));
		}
	}
	let body = body.join("\n\n");
	if event != "APPROVE" && body.is_empty() {
		return Err(Error::user("submit: write a summary first; GitHub needs one for this verdict."));
	}
	let payload = json!({ "commit_id": pr.head_sha, "event": event, "body": body, "comments": comments });
	let file = std::env::temp_dir().join(format!("jocasta-review-{}-{}.json", std::process::id(), now_ms()));
	std::fs::write(&file, payload.to_string())?;
	let route = format!("repos/{}/pulls/{}/reviews", pr.repo, pr.number);
	let input = file.to_string_lossy().into_owned();
	let run =
		c.finisher.gh(&["api", &route, "--method", "POST", "--input", &input], &crate::shared::paths::data_dir()).await;
	let _ = std::fs::remove_file(&file);
	let Some(run) = run else { return Err(Error::user("submit: gh isn't installed; nothing was sent.")) };
	if run.code != Some(0) {
		let outside = if run.out.contains("HTTP 422") {
			" A comment on a line outside the diff can be published on its own (it goes on the pull request), then submit again."
		} else {
			""
		};
		return Err(Error::user(format!("submit: {}; nothing was sent.{outside}", last_line(&run.out))));
	}
	let url = url_in(&run.out, &pr.url);
	let at_ms = now_ms();
	for card in notes.notes.iter_mut().filter(|card| kept(card)) {
		card.published = Some(Published { url: url.clone(), at_ms });
	}
	Ok(url)
}

/// Azure DevOps' review (0712): each kept note published as it is one by one, the summary as a comment on the pull
/// request, then his vote (`approve`, `wait-for-author`; none for a comment). The pull request's link.
async fn submit_azure(
	c: &Arc<ServiceContext>,
	pr: &PrTarget,
	notes: &mut ReviewNotes,
	verdict: &str,
) -> Result<String> {
	for card in notes.notes.iter_mut().filter(|card| kept(card)) {
		let place = (!card.file.is_empty()).then_some((card.file.as_str(), card.line));
		let url = publish_azure(c, pr, place, current(card)).await?;
		card.published = Some(Published { url, at_ms: now_ms() });
	}
	if !notes.summary.trim().is_empty() {
		publish_azure(c, pr, None, notes.summary.trim()).await?;
	}
	let vote = match verdict {
		"approve" => "approve",
		"request_changes" => "wait-for-author",
		_ => return Ok(pr.url.clone()),
	};
	let Some(org) = pr.org_url.as_deref() else {
		return Err(Error::user("submit: the pull request's organisation isn't recorded."));
	};
	let id = pr.number.to_string();
	let args = ["repos", "pr", "set-vote", "--id", &id, "--vote", vote, "--org", org, "-o", "json"];
	let run = c.finisher.az(&args, &crate::shared::paths::data_dir()).await;
	let Some(run) = run else { return Err(Error::user("submit: az isn't installed; the vote wasn't cast.")) };
	if run.code != Some(0) {
		return Err(Error::user(format!("submit: {}; the vote wasn't cast.", last_line(&run.out))));
	}
	Ok(pr.url.clone())
}
