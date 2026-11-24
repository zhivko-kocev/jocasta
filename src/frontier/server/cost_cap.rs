//! A thread's cost cap (0686): omp stops the thread's session tree at `--max-cost`, and this is the second check, from
//! the ledger, after every turn's end. A thread at or over its budget is stuck (`over budget: $5.10 of $5`), a NEEDS YOU
//! event says so once, and he is asked: raise the budget and carry on, or stop the thread. The boss and the project
//! panes have no cap.

use std::sync::Arc;

use serde_json::json;

use crate::frontier::protocol::ToastPriority;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::pane_launch::thread_budget;
use crate::frontier::server::popups::{over_budget_question, raise_label};
use crate::frontier::server::thread::subject;
use crate::frontier::server::wording::money;
use crate::shared::errors::Result;
use crate::shared::settings::read_max_cost_usd;

/// A turn of `pane_id` ended: when it is a thread at or over its budget, the cap trips (once) and he is asked.
pub async fn after_turn(c: &Arc<ServiceContext>, pane_id: &str) -> Result<()> {
	let Some(thread) = c.threads.get(pane_id) else { return Ok(()) };
	let Some(budget) = thread.with(|t| t.task.as_ref().and_then(thread_budget)) else { return Ok(()) };
	let Some(spent) = c.status.spend_of(&thread).await else { return Ok(()) };
	if spent < budget || thread.with(|t| std::mem::replace(&mut t.budget_asked, true)) {
		return Ok(());
	}
	let said = format!("over budget: {} of ${}", money(spent), crate::frontier::protocol::js_number_text(budget));
	thread.with(|t| {
		t.stuck = Some(said.clone());
		c.journal.thread("over_budget", t, json!({ "spent": spent, "budget": budget }));
	});
	c.events.thread(&thread, &said, ToastPriority::Urgent, None).await;
	let Some(ask) = c.options.ask.clone() else { return Ok(()) };
	let setting = read_max_cost_usd();
	let raise = if setting > 0.0 { setting } else { budget };
	let popup = ask(&over_budget_question(&thread.with(|t| subject(t)), &said, raise), None, false);
	let choice = popup.answer().await;
	if choice == raise_label(raise) {
		thread.with(|t| {
			t.stuck = None;
			t.budget_asked = false;
			if let Some(task) = t.task.as_mut() {
				task.budget = Some(budget + raise);
			}
		});
		c.status.write_task(&thread);
		return c.lifecycle.restart(&thread).await;
	}
	c.lifecycle.abort_turn(pane_id).await
}
