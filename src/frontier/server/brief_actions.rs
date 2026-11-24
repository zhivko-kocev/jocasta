//! What a brief's buttons do (0678): the thread brief's `finish` and `send` run through the same code paths, and the
//! same Allow gate, as the boss's `thread_finish` and `thread_send`; the answer is the whole updated brief.

use std::sync::Arc;

use serde_json::Value;

use crate::frontier::brief::model::Brief;
use crate::frontier::brief::store;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::thread::ThreadRef;
use crate::frontier::server::thread_brief::{BriefExtras, refresh};
use crate::frontier::server::tools::Tools;
use crate::frontier::server::wording::Door;
use crate::shared::errors::{Error, Result};

/// The open thread whose brief is `thread-<task id>`.
fn thread_of(c: &Arc<ServiceContext>, id: &str) -> Result<ThreadRef> {
	let task = id.strip_prefix("thread-").ok_or_else(|| Error::user("That brief has no actions."))?;
	c.threads
		.all()
		.into_iter()
		.find(|thread| thread.with(|t| t.task.as_ref().is_some_and(|record| record.id == task)))
		.ok_or_else(|| Error::user("That thread isn't open any more; its brief is a record, not a control."))
}

/// Runs `action` (`{"op":"finish","mode":"merge"|"pr"}` or `{"op":"send","text":…}`) on the thread of brief `id`; the
/// brief as it is after.
pub async fn act(c: &Arc<ServiceContext>, tools: &Tools, id: &str, action: &Value) -> Result<Brief> {
	let thread = thread_of(c, id)?;
	let op = action.get("op").and_then(Value::as_str).unwrap_or("");
	match op {
		"finish" => {
			let mode = action.get("mode").and_then(Value::as_str).unwrap_or("");
			if !matches!(mode, "merge" | "pr") {
				return Err(Error::user("finish: the mode is merge or pr."));
			}
			c.finisher.finish(&thread, mode, Door::Key).await?;
		}
		"send" => {
			let text = action.get("text").and_then(Value::as_str).unwrap_or("").trim().to_owned();
			if text.is_empty() {
				return Err(Error::user("send: there is nothing to send."));
			}
			tools.send(&thread, &text, Door::Key).await?;
		}
		other => return Err(Error::user(format!("The brief has no action `{other}`."))),
	}
	// The thread may be gone after a finish: its brief stays as it was then.
	if c.threads.includes(&thread) {
		refresh(c, &thread, &BriefExtras::default()).await;
	}
	store::load(id).ok_or_else(|| Error::user("That brief is gone."))
}

/// The id of the stored brief a link is (`http://127.0.0.1:<port>/brief/<id>`, as the Done toast carries), when it is
/// one: opening it mints a fresh launch code.
pub fn brief_of_link(c: &Arc<ServiceContext>, url: &str) -> Option<String> {
	let briefs = c.options.briefs.as_ref()?;
	let id = url.rsplit_once("/brief/")?.1;
	(briefs.url(id) == url && store::load(id).is_some()).then(|| id.to_owned())
}
