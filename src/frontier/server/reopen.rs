//! Threads come back after a restart (0549, 0683, 0676): the own shell keeps no tabs across one, so at the start each
//! thread pane the last run recorded in `panes.json`, whose worktree is still there, opens again in a workspace of its
//! own, asleep, and is found as a thread like any other. A key in it, a `thread_send`, a review fix or a finish starts
//! its omp from the session it kept (`--continue` without one: a thread that never had a turn). The threads that were
//! mid-turn when Jocasta stopped go on with that turn (`--restore`) once they are found.

use std::sync::Arc;

use crate::frontier::server::boss::{LeftPane, PANE_KEY_ENV, new_pane_key};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::task_file::find_task;
use crate::shared::errors::{Error, Result};
use crate::shared::log::{LogLevel, log};
use crate::shared::pathkey::same_dir;

/// The records the last run left, taken out of `panes.json` before this run gives any pane an id (the own shell's ids
/// start again at every run). Read before the boss pane is made.
pub async fn left_panes(c: &ServiceContext) -> Vec<LeftPane> {
	let taken = match c.herdr.panes().await {
		Ok(panes) => c.launched.take_gone(&panes),
		Err(error) => Err(Error::from(error)),
	};
	taken.unwrap_or_else(|error| {
		c.warn(format!("the last run's panes weren't read: {error}"));
		Vec::new()
	})
}

/// Opens each left thread pane again, asleep; returns the new panes of those that were mid-turn.
pub async fn reopen_threads(c: &ServiceContext, left: Vec<LeftPane>) -> Vec<String> {
	let mut mid_turn = Vec::new();
	for pane in left {
		match reopen(c, &pane).await {
			Ok(Some(pane_id)) if !pane.asleep && pane.restore.is_some() => mid_turn.push(pane_id),
			Ok(_) => {}
			Err(error) => c.warn(format!("the thread in {} didn't open again: {error}", pane.cwd.display())),
		}
	}
	mid_turn
}

/// `pane` in a workspace of its own, asleep, with the session it kept; `None` when it isn't a thread's (the boss's,
/// a project's main checkout) or its worktree is gone.
async fn reopen(c: &ServiceContext, pane: &LeftPane) -> Result<Option<String>> {
	if !pane.cwd.is_dir() {
		return Ok(None);
	}
	let Some(checkout) = c.places.checkout(&pane.cwd)? else { return Ok(None) };
	if same_dir(&checkout.top, &checkout.main) {
		return Ok(None);
	}
	let Some(project) = c.places.project_at(&pane.cwd)? else { return Ok(None) };
	if !same_dir(&project.main_path, &checkout.main) {
		return Ok(None);
	}
	let title = find_task(&project.id, &checkout.top)
		.map(|task| task.title)
		.filter(|title| !title.is_empty())
		.or_else(|| checkout.branch.clone())
		.unwrap_or_else(|| checkout.top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
	// A workspace's label is the thread's title when it is found; a label already taken would be that workspace.
	let taken: Vec<String> = c.herdr.workspaces().await?.into_iter().map(|w| w.label).collect();
	let label = (1..)
		.map(|n| if n == 1 { title.clone() } else { format!("{title} {n}") })
		.find(|label| !taken.contains(label))
		.unwrap_or(title);
	let key = new_pane_key();
	let command = c.panes.sleeper_command(&format!("{}/{label} · asleep", project.name));
	let env = vec![(PANE_KEY_ENV.to_owned(), key.clone())];
	let workspace = c.herdr.ensure_workspace(&label, &pane.cwd, Some(&command), Some(&env)).await?;
	let found = c.herdr.panes().await?.into_iter().find(|p| p.workspace_id == workspace);
	let pane_id = found.ok_or_else(|| Error::other(format!("workspace {label} has no pane")))?.pane_id;
	c.launched.add_asleep(&pane_id, &pane.cwd.display().to_string(), Some(&key), pane.restore.as_deref())?;
	let how = if pane.asleep || pane.restore.is_none() { "asleep" } else { "to go on with its turn" };
	log(LogLevel::Info, "start", &format!("{}/{label}: open again, {how}", project.name), None);
	Ok(Some(pane_id))
}

/// The reopened threads that were mid-turn go on with that turn (`--restore`, 0549), once they are found.
pub async fn resume_turns(c: &Arc<ServiceContext>, panes: &[String]) {
	for pane_id in panes {
		let Some(thread) = c.threads.get(pane_id) else { continue };
		if let Err(error) = c.sleep.wake_thread(&thread).await {
			c.warn(format!("a thread's turn didn't go on: {error}"));
		}
	}
}

#[cfg(test)]
#[path = "reopen_tests.rs"]
mod tests;
