//! Works first (0706): before anyone judges a change, code shows whether it works. A pull request's check runs in the
//! clean room before its review's first turn, and a failure ends the review with one P0 card; the reviewer's
//! `review_try` steps run here, by Jocasta, and a failed step ends the review the same way; Finish runs the clean-room
//! check on a thread's head before its question. Every outcome is a row of the review's `works` section.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::frontier::brief::model::{RanProof, WorksRow, WorksState};
use crate::frontier::brief::store::{brief_dir, valid_id};
use crate::frontier::server::allow::{AskError, Confirmation};
use crate::frontier::server::cleanroom::{self, Check, NO_TASK_SAID, tail};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::mise::{Found, mise_env, mise_task_among};
use crate::frontier::server::pr_review::{self, PrTarget};
use crate::frontier::server::review_flow::mise_of;
use crate::frontier::server::review_notes::{REVIEW_NOTE, ReviewNotes, head_of, review_file};
use crate::frontier::server::steps::{self, Place, Step};
use crate::frontier::server::thread::{Thread, ThreadRef};
use crate::frontier::server::thread_brief::{self, BriefExtras};
use crate::frontier::server::thread_start::guard_said;
use crate::frontier::server::wording::Door;
use crate::shared::errors::{Error, Result};
use crate::shared::guard::Verdict;
use crate::shared::projects::Project;
use crate::shared::projects_file::{read_projects_file, set_pr_run};

#[cfg(test)]
#[path = "works_tests.rs"]
mod tests;

/// The tool's name.
pub const REVIEW_TRY: &str = "review_try";

/// The largest screenshot `review_try` copies.
const MAX_SHOT_BYTES: u64 = 10 * 1024 * 1024;

/// What a review is told once a check or step failed.
const ENDS_HERE: &str = "The review ends here: hand in nothing more and end your turn.";

/// Where a thread's steps run, owned.
enum Where {
	CleanRoom { project: Project, commit: String },
	Here { path: PathBuf, env: HashMap<String, String> },
}

impl Where {
	fn place(&self) -> Place<'_> {
		match self {
			Self::CleanRoom { project, commit } => Place::CleanRoom { project, commit },
			Self::Here { path, env } => Place::Here { path, env },
		}
	}

	fn said(&self) -> &'static str {
		match self {
			Self::CleanRoom { .. } => "in the clean room",
			Self::Here { .. } => "on this machine",
		}
	}
}

/// The environment a step on his machine gets: the panes' check environment (Jocasta's own keys left out) with the
/// project's mise env.
async fn here_env(c: &ServiceContext, project: &Project, path: &Path) -> HashMap<String, String> {
	let mut env = c.panes.check_env();
	let own = mise_env(&project.id, path, Some(&env), mise_of(c)).await;
	env.extend(own);
	env
}

/// [`here_env`] for a pull request he allowed to run here: its worktree's mise config is trusted for that process only.
async fn pr_env(c: &ServiceContext, project: &Project, path: &Path) -> HashMap<String, String> {
	let mut env = here_env(c, project, path).await;
	env.insert("MISE_TRUSTED_CONFIG_PATHS".to_owned(), path.to_string_lossy().into_owned());
	env
}

fn row(label: String, state: WorksState, ms: Option<i64>, tail: String) -> WorksRow {
	WorksRow { label, state, ms, tail, screenshots: Vec::new() }
}

/// His answer to running `pr` on this machine: kept in `projects.yaml` (`prRun`), else asked once in the Allow box. A
/// Deny he chose is kept too; no answer is kept for no answer.
async fn pr_run_allowed(c: &ServiceContext, project: &Project, pr: &PrTarget, why: &str) -> bool {
	let answered = read_projects_file(None).ok().and_then(|file| file.pr_run_answer(&project.main_path, &pr.author));
	if let Some(allow) = answered {
		return allow;
	}
	let ask = Confirmation {
		message: format!("Run PR #{} by {} on this machine?", pr.number, pr.author),
		detail: format!(
			"The clean room isn't possible ({why}). Allow runs this pull request's checks and its reviewer's tries in the review's worktree, as you, with your environment; Deny reviews it without running it. The answer is kept for {}'s pull requests in {} (projects.yaml, prRun).",
			pr.author, project.name
		),
	};
	let (allow, keep) = match c.allow.ask(&ask, "it wasn't run", Door::Key).await {
		Ok(()) => (true, true),
		Err(AskError::Denied(denied)) => (false, denied.chose),
		Err(AskError::Refused(_)) => (false, false),
	};
	if keep && let Err(error) = set_pr_run(&project.main_path.to_string_lossy(), &pr.author, allow, None) {
		c.warn(Error::other(format!(
			"projects.yaml: his answer for {}'s pull requests wasn't kept: {error}",
			pr.author
		)));
	}
	allow
}

/// What the works-first check of a pull request came to: go on with the review (and the line its brief gains), or the
/// review ended with what failed.
pub enum WorksFirst {
	Go(String),
	Ended(String),
}

/// A pull request's check before its review's first turn (0706): in the clean room on the head; on his machine when
/// the clean room isn't possible and he allows it; else not run. A failure ends the review: its notes get the failed
/// row and the one P0 card.
pub async fn works_first(c: &Arc<ServiceContext>, thread: &ThreadRef) -> Result<WorksFirst> {
	let t = thread.get();
	let (Some(task), Some(pr)) = (t.task.as_ref(), t.task.as_ref().and_then(|task| task.pr.as_ref())) else {
		return Ok(WorksFirst::Go(String::new()));
	};
	if !c.options.clean_room {
		return Ok(WorksFirst::Go(String::new()));
	}
	let file = review_file(&t.project.id, &task.id);
	let mut notes = ReviewNotes::of_head(&file, &pr.head_sha);
	let outcome = match cleanroom::check(&t.project, &pr.head_sha).await {
		Check::Ran { task, ran } => Some((format!("`mise run {task}` in the clean room"), task, ran)),
		Check::NotPossible(why) if why == NO_TASK_SAID => {
			notes.works.push(row(format!("check not run: {why}"), WorksState::NotRun, None, String::new()));
			None
		}
		Check::NotPossible(why) if pr_run_allowed(c, &t.project, pr, &why).await => host_check(c, &t, &mut notes).await,
		Check::NotPossible(why) => {
			let label =
				format!("check not run: the clean room isn't possible ({why}) and running it here wasn't allowed");
			notes.works.push(row(label, WorksState::NotRun, None, String::new()));
			None
		}
	};
	let said = match outcome {
		Some((label, _, ran)) if ran.exit == Some(0) => {
			notes.works.push(row(label.clone(), WorksState::Passed, Some(ran.ms), tail(&ran.output)));
			format!("Jocasta ran {label} on the head: it passed.")
		}
		Some((label, task, ran)) => {
			let step = Step { run: format!("mise run {task}"), expect: "exit:0".to_owned() };
			let proof = steps::proof(&step, &ran);
			notes.failed(row(label.clone(), WorksState::Failed, Some(ran.ms), tail(&ran.output)), proof);
			notes.write(&file)?;
			return Ok(WorksFirst::Ended(label));
		}
		None => notes.works.last().map(|row| format!("Jocasta's {}.", row.label)).unwrap_or_default(),
	};
	notes.write(&file)?;
	Ok(WorksFirst::Go(said))
}

/// The pull request's check on his machine, in its worktree (he allowed it): its label, task and run; `None` with a
/// not-run row when the project has neither task or it couldn't start.
async fn host_check(
	c: &ServiceContext,
	t: &Thread,
	notes: &mut ReviewNotes,
) -> Option<(String, String, cleanroom::Ran)> {
	let env = pr_env(c, &t.project, &t.path).await;
	let task = match mise_task_among(&t.project.id, &t.path, &["verify", "test"], Some(&env), mise_of(c)).await {
		Some(Found::Task(task)) => task,
		Some(Found::Skipped(why)) => {
			notes.works.push(row(format!("check not run: {why}"), WorksState::NotRun, None, String::new()));
			return None;
		}
		None => {
			let label = format!("check not run: {NO_TASK_SAID}");
			notes.works.push(row(label, WorksState::NotRun, None, String::new()));
			return None;
		}
	};
	let place = Place::Here { path: &t.path, env: &env };
	match steps::run(&place, &format!("mise run {task}")).await {
		Ok(ran) => Some((format!("`mise run {task}` on this machine"), task, ran)),
		Err(error) => {
			notes.works.push(row(format!("check not run: {error}"), WorksState::NotRun, None, String::new()));
			None
		}
	}
}

/// Where `review_try`'s steps and a finding's proof run for thread `t`: the clean room for a pull request (his machine
/// when he allowed it), the worktree for his own thread. `tool` and `otherwise` word the refusal.
async fn where_for(c: &ServiceContext, t: &Thread, tool: &str, otherwise: &str) -> Result<Where> {
	let Some(pr) = t.task.as_ref().and_then(|task| task.pr.as_ref()) else {
		return Ok(Where::Here { path: t.path.clone(), env: here_env(c, &t.project, &t.path).await });
	};
	let why = if c.options.clean_room {
		match cleanroom::off_for(&t.project) {
			Some(why) => Some(why),
			None => cleanroom::unready().await,
		}
	} else {
		Some("clean-room runs are off in this run".to_owned())
	};
	let Some(why) = why else {
		return Ok(Where::CleanRoom { project: t.project.clone(), commit: pr.head_sha.clone() });
	};
	if pr_run_allowed(c, &t.project, pr, &why).await {
		return Ok(Where::Here { path: t.path.clone(), env: pr_env(c, &t.project, &t.path).await });
	}
	Err(Error::user(format!(
		"{tool}: this pull request runs only in the clean room, which isn't possible ({why}), and running it on this machine wasn't allowed; {otherwise}."
	)))
}

/// A step on his machine is judged by the shipped guard like `done_when` (0540): deny refuses it, prompt asks him.
async fn guarded(c: &ServiceContext, t: &Thread, steps: &[Step], tool: &str) -> Result<()> {
	for step in steps {
		let judged = guard_said(&step.run)?;
		let rule = judged.rule.unwrap_or_default();
		match judged.verdict {
			Verdict::Deny => {
				return Err(Error::user(format!(
					"{tool}: `{}` is denied by Jocasta's guard (`{}`); nothing ran.",
					step.run, rule
				)));
			}
			Verdict::Prompt => {
				let ask = Confirmation {
					message: format!("Run `{}` for {}'s review?", step.run, t.title),
					detail: format!(
						"The reviewer of {} in {} asks Jocasta to run this command in the thread's worktree; Jocasta's guard asks you first (`{}`).",
						t.title, t.project.name, rule
					),
				};
				c.allow.ask(&ask, "nothing ran", Door::Key).await.map_err(Error::from)?;
			}
			Verdict::Allow => {}
		}
	}
	Ok(())
}

/// A screenshot's image type by its first bytes: `png`, `jpg`, `gif` or `webp`.
fn image_kind(head: &[u8]) -> Option<&'static str> {
	if head.starts_with(b"\x89PNG\r\n\x1a\n") {
		Some("png")
	} else if head.starts_with(&[0xff, 0xd8, 0xff]) {
		Some("jpg")
	} else if head.starts_with(b"GIF8") {
		Some("gif")
	} else if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
		Some("webp")
	} else {
		None
	}
}

/// Copies the screenshots to `<data>/briefs/assets/<brief id>/<row>-<n>.<ext>`: their names, and a line for each one
/// refused (not a file, larger than 10 MB, or not a PNG, JPEG, GIF or WebP image).
fn copy_shots(given: &[String], base: &Path, brief: &str, row: usize) -> (Vec<String>, Vec<String>) {
	let (mut names, mut refused) = (Vec::new(), Vec::new());
	if given.is_empty() || !valid_id(brief) {
		return (names, refused);
	}
	let dir = brief_dir().join("assets").join(brief);
	for (n, shot) in given.iter().enumerate() {
		let path = base.join(shot);
		let kind = std::fs::metadata(&path)
			.ok()
			.filter(|meta| meta.is_file() && meta.len() <= MAX_SHOT_BYTES)
			.and_then(|_| std::fs::read(&path).ok())
			.and_then(|bytes| image_kind(&bytes).map(|kind| (kind, bytes)));
		let Some((kind, bytes)) = kind else {
			refused.push(format!("screenshot {shot} skipped: not a PNG, JPEG, GIF or WebP file of at most 10 MB"));
			continue;
		};
		let name = format!("{row}-{}.{kind}", n + 1);
		match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(&name), bytes)) {
			Ok(()) => names.push(name),
			Err(error) => refused.push(format!("screenshot {shot} wasn't kept: {error}")),
		}
	}
	(names, refused)
}

/// `review_try` (0706): the reviewer's steps, run and judged by Jocasta. All passing is a passed row; the first that
/// fails is a failed row and the review's one P0 card, and the review ends.
pub async fn review_try(
	c: &Arc<ServiceContext>,
	thread: &ThreadRef,
	values: &serde_json::Map<String, Value>,
) -> Result<String> {
	let t = thread.get();
	let Some(task) = t.task.clone() else {
		return Err(Error::user(format!("{REVIEW_TRY}: this thread has no task file to keep its review in.")));
	};
	let what = values
		.get("what")
		.and_then(Value::as_str)
		.map(str::trim)
		.filter(|what| !what.is_empty())
		.ok_or_else(|| Error::user(format!("{REVIEW_TRY}: what is required: what you are trying, in a few words.")))?
		.to_owned();
	let steps = steps::steps_of(values.get("steps"), REVIEW_TRY)?;
	let shots: Vec<String> = match values.get("screenshots") {
		None | Some(Value::Null) => Vec::new(),
		Some(Value::Array(items)) if items.iter().all(Value::is_string) => {
			items.iter().filter_map(Value::as_str).map(str::to_owned).collect()
		}
		Some(_) => return Err(Error::user(format!("{REVIEW_TRY}: screenshots is a list of file paths."))),
	};
	let head = head_of(&t.path).await?;
	let file = review_file(&t.project.id, &task.id);
	if let Some(ended) = ReviewNotes::of_head(&file, &head).ended() {
		return Err(Error::user(format!("{REVIEW_TRY}: the review ended: {ended} failed. {ENDS_HERE}")));
	}
	let place = where_for(c, &t, REVIEW_TRY, "review it by reading").await?;
	if matches!(place, Where::Here { .. }) {
		guarded(c, &t, &steps, REVIEW_TRY).await?;
	}
	let started = Instant::now();
	let mut last = String::new();
	for (i, step) in steps.iter().enumerate() {
		let ran = steps::run(&place.place(), &step.run)
			.await
			.map_err(|error| Error::user(format!("{REVIEW_TRY}: step {} couldn't run: {error}", i + 1)))?;
		if !steps::gives(&ran, &step.expect) {
			let label = format!("{what} ({})", place.said());
			let mut notes = ReviewNotes::of_head(&file, &head);
			notes.failed(row(label, WorksState::Failed, Some(ran.ms), tail(&ran.output)), steps::proof(step, &ran));
			notes.write(&file)?;
			return Err(Error::user(format!(
				"{REVIEW_TRY}: step {} didn't give \"{}\": {}\n{ENDS_HERE}",
				i + 1,
				step.expect,
				steps::said(&ran)
			)));
		}
		last = tail(&ran.output);
	}
	let ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
	let mut notes = ReviewNotes::of_head(&file, &head);
	let brief = match &task.pr {
		Some(_) => pr_review::brief_id(&task),
		None => thread_brief::brief_id(&t),
	}
	.unwrap_or_default();
	let (names, refused) = copy_shots(&shots, &t.path, &brief, notes.works.len() + 1);
	let mut passed = row(format!("{what} ({})", place.said()), WorksState::Passed, Some(ms), last);
	passed.screenshots = names;
	notes.works.push(passed);
	notes.write(&file)?;
	let plural = if steps.len() == 1 { "" } else { "s" };
	let mut said = format!("{REVIEW_TRY}: {what} works ({} step{plural} passed {}).", steps.len(), place.said());
	for line in refused {
		said.push_str(&format!("\n{line}"));
	}
	Ok(said)
}

/// A finding's proof (0707), run once where the review's steps run and under the same guard: what the card keeps when
/// the run gives its expectation, the refusal otherwise.
pub async fn prove(c: &ServiceContext, thread: &ThreadRef, step: &Step) -> Result<RanProof> {
	let t = thread.get();
	let place = where_for(c, &t, REVIEW_NOTE, "hand it in as a question or an opinion").await?;
	if matches!(place, Where::Here { .. }) {
		guarded(c, &t, std::slice::from_ref(step), REVIEW_NOTE).await?;
	}
	let ran = steps::run(&place.place(), &step.run)
		.await
		.map_err(|error| Error::user(format!("{REVIEW_NOTE}: the proof couldn't run: {error}")))?;
	if !steps::gives(&ran, &step.expect) {
		return Err(Error::user(format!(
			"{REVIEW_NOTE}: the proof didn't reproduce: expected \"{}\", got {}",
			step.expect,
			steps::said(&ran)
		)));
	}
	Ok(steps::proof(step, &ran))
}

/// Finish's clean-room check on the thread's head (0706), before its question: a failing check refuses Finish; one
/// that isn't possible lets it go on, and the thread's page says why. The outcome replaces the page's earlier
/// clean-room row.
pub async fn finish_check(c: &Arc<ServiceContext>, thread: &ThreadRef) -> Result<()> {
	if !c.options.clean_room {
		return Ok(());
	}
	let t = thread.get();
	let Some(task) = t.task.as_ref() else { return Ok(()) };
	let head = head_of(&t.path).await?;
	let (row, failed) = match cleanroom::check(&t.project, &head).await {
		Check::Ran { task, ran } => {
			let label = format!("`mise run {task}` in the clean room");
			let state = if ran.exit == Some(0) { WorksState::Passed } else { WorksState::Failed };
			let failed = (state == WorksState::Failed).then(|| tail(&ran.output));
			(row(label, state, Some(ran.ms), tail(&ran.output)), failed)
		}
		Check::NotPossible(why) => {
			(row(format!("clean-room run not possible: {why}"), WorksState::NotRun, None, String::new()), None)
		}
	};
	let file = review_file(&t.project.id, &task.id);
	let mut notes = ReviewNotes::of_head(&file, &head);
	notes.works.retain(|old| !old.label.ends_with("in the clean room") && !old.label.starts_with("clean-room run"));
	notes.works.push(row);
	notes.write(&file)?;
	thread_brief::refresh(c, thread, &BriefExtras::default()).await;
	match failed {
		Some(tail) => Err(Error::user(format!("Finish: the clean-room check fails: {tail}"))),
		None => Ok(()),
	}
}
