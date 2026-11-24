//! The day's briefs, built by code (0680): the morning brief on the first refresh tick after 06:00 local each day and on
//! his `b` key, and the rows of `today` that open them. The brief itself is `brief/morning.rs`; the weekly one is
//! `weekly_brief.rs`. Nothing here asks a model, and nothing opens by itself: the rows wait for his Enter.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, Timelike};

use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{FlowEdge, FlowNode, NodeState};
use crate::frontier::brief::morning::{self, MorningInput};
use crate::frontier::brief::store;
use crate::frontier::protocol::{BriefLink, PrCi, ThreadState, Today};
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::day::local;
use crate::shared::errors::{Error, Result};

/// The morning brief is built from this hour (local) on.
const MORNING_HOUR: u32 = 6;

/// A local day as `YYYY-MM-DD`.
fn date_of(at: i64) -> String {
	local(at).format("%Y-%m-%d").to_string()
}

/// `morning-<yyyy-mm-dd>` for the local day of `at`.
pub fn morning_id(at: i64) -> String {
	morning::brief_id(&date_of(at))
}

/// `weekly-<yyyy>-W<ww>` (ISO week) for the week that began on `monday` (`YYYY-MM-DD`); `None` when it isn't a date.
pub fn weekly_id_of(monday: &str) -> Option<String> {
	let week = NaiveDate::parse_from_str(monday, "%Y-%m-%d").ok()?.iso_week();
	Some(format!("weekly-{}-W{:02}", week.year(), week.week()))
}

/// The weekly brief's id for the week `at` is in.
pub fn weekly_id(at: i64) -> String {
	let week = local(at).date_naive().iso_week();
	format!("weekly-{}-W{:02}", week.year(), week.week())
}

/// The rows of `today`'s BRIEFS at `now`: the day's morning brief and the week's weekly brief, each once it is stored,
/// with the link `link.open` opens. None without a page server.
pub fn links(c: &ServiceContext, now: i64) -> Vec<BriefLink> {
	let Some(briefs) = c.options.briefs.as_ref() else { return Vec::new() };
	[("morning brief", morning_id(now)), ("weekly brief", weekly_id(now))]
		.into_iter()
		.filter(|(_, id)| store::exists(id))
		.map(|(label, id)| BriefLink { label: label.to_owned(), url: briefs.url(&id) })
		.collect()
}

/// Builds the morning brief on the first tick after 06:00 of a day that has none (0680); nothing opens.
pub async fn morning_if_due(c: &Arc<ServiceContext>) -> Result<()> {
	if !c.options.auto_briefs_on() {
		return Ok(());
	}
	let now = c.day.now_ms();
	if local(now).hour() < MORNING_HOUR || store::exists(&morning_id(now)) {
		return Ok(());
	}
	build_morning(c).await?;
	c.day.push_today()
}

/// Builds and stores today's morning brief again from what the day holds now; its id.
pub async fn build_morning(c: &Arc<ServiceContext>) -> Result<String> {
	let now = c.day.now_ms();
	let date = date_of(now);
	let today = c.day.today()?;
	let dashboard = c.day.dashboard();
	let (accounts, capacity) = c.day.quota_accounts();
	let work = work_graph(c, &today);
	let mut brief = morning::build(&MorningInput {
		date: &date,
		now,
		today: &today,
		dashboard: &dashboard,
		accounts: &accounts,
		capacity: &capacity,
		work,
	});
	lay_out(&mut brief).map_err(Error::other)?;
	store::save(&brief)?;
	Ok(brief.id)
}

/// Builds the morning brief for his `b` key, announces it in `today` and opens it; the answer he reads.
pub async fn open_morning(c: &Arc<ServiceContext>) -> Result<String> {
	let id = build_morning(c).await?;
	c.day.push_today()?;
	match c.options.briefs.as_ref() {
		Some(briefs) => {
			briefs.open(&id);
			Ok("Morning brief opened.".to_owned())
		}
		None => Ok("Morning brief built.".to_owned()),
	}
}

fn node(id: &str, label: &str, state: NodeState, detail: Option<String>) -> FlowNode {
	FlowNode { id: id.to_owned(), label: label.to_owned(), group: None, state, detail, layer: 0, order: 0 }
}

/// How a PR and its CI read as nodes: the PR's state, the CI's label and state.
fn pr_states(ci: PrCi) -> (NodeState, &'static str, NodeState) {
	match ci {
		PrCi::Merged => (NodeState::Done, "merged", NodeState::Done),
		PrCi::Closed => (NodeState::Failed, "closed", NodeState::Pending),
		PrCi::Passing => (NodeState::Active, "passing", NodeState::Done),
		PrCi::Failing => (NodeState::Active, "failing", NodeState::Failed),
		PrCi::Pending => (NodeState::Active, "pending", NodeState::Active),
	}
}

/// The boxes and arrows of the work graph, each id once.
#[derive(Default)]
struct Graph {
	nodes: Vec<FlowNode>,
	edges: Vec<FlowEdge>,
	known: HashSet<String>,
}

impl Graph {
	fn add(&mut self, made: FlowNode) {
		if self.known.insert(made.id.clone()) {
			self.nodes.push(made);
		}
	}

	fn arrow(&mut self, from: &str, to: &str) {
		self.edges.push(FlowEdge { from: from.to_owned(), to: to.to_owned(), label: None });
	}

	/// `from` continues to the PR at `url` and to its CI, as `thread_facts` read them (`None`: not read yet).
	fn pr_chain(&mut self, from: &str, url: &str, number: Option<u64>, ci: Option<PrCi>) {
		let (pr_id, ci_id) = (format!("pr:{url}"), format!("ci:{url}"));
		let (pr_state, ci_word, ci_state) =
			ci.map_or((NodeState::Pending, "not read yet", NodeState::Pending), pr_states);
		if !self.known.contains(&pr_id) {
			let label = number.map_or_else(|| "PR".to_owned(), |number| format!("PR #{number}"));
			self.add(node(&pr_id, &label, pr_state, None));
			self.add(node(&ci_id, &format!("CI {ci_word}"), ci_state, None));
			self.arrow(&pr_id, &ci_id);
		}
		self.arrow(from, &pr_id);
	}
}

/// The work graph (0680): every open thread and every start that waits is a box, an arrow runs from each thing a start
/// waits on, and a thread or landed work with a PR continues to the PR and its CI, with the states `thread_facts` read.
fn work_graph(c: &Arc<ServiceContext>, today: &Today) -> (Vec<FlowNode>, Vec<FlowEdge>) {
	let mut graph = Graph::default();
	for thread in c.threads.all() {
		let t = thread.get();
		let id = t.task.as_ref().map_or_else(|| format!("pane:{}", t.pane_id), |task| task.id.clone());
		let (state, detail) = match (&t.stuck, t.shown) {
			(Some(stuck), _) => (NodeState::Failed, Some(stuck.clone())),
			(None, ThreadState::Working) => (NodeState::Active, Some("working".to_owned())),
			(None, ThreadState::Blocked) => (NodeState::Active, Some("waits on you".to_owned())),
			(None, ThreadState::Done) => (NodeState::Done, Some("review ready".to_owned())),
			(None, ThreadState::Idle) => (NodeState::Pending, Some("idle".to_owned())),
		};
		graph.add(node(&id, &format!("{}/{}", t.project.name, t.title), state, detail));
		if let Some(pr) = t.task.as_ref().and_then(|task| task.pr.as_ref()) {
			let facts = c.thread_facts.pr_facts(&pr.url);
			graph.pr_chain(&id, &pr.url, Some(pr.number), facts.map(|facts| facts.ci));
		}
	}
	for item in c.pending.list(None) {
		let state = if item.blocked { NodeState::Failed } else { NodeState::Pending };
		graph.add(node(&item.id, &item.title, state, Some(item.line())));
		for after in &item.after {
			// What it waits on may be gone from the open threads; it stays a box with the name it had.
			let state = if after.met { NodeState::Done } else { NodeState::Pending };
			graph.add(node(&after.thread, &after.name, state, None));
			graph.arrow(&after.thread, &item.id);
		}
	}
	for (index, landed) in today.landed.iter().enumerate() {
		let Some(facts) = landed.pr.as_ref() else { continue };
		let id = format!("landed:{index}");
		graph.add(node(&id, &landed.title, NodeState::Done, Some(landed.line.clone())));
		graph.pr_chain(&id, &format!("landed-{index}"), Some(facts.number), Some(facts.ci));
	}
	(graph.nodes, graph.edges)
}

#[cfg(test)]
#[path = "day_briefs_tests.rs"]
mod tests;
