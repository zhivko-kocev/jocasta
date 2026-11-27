//! The reviewer's system view (0710): the thread tool `review_system` hands in what the change does to the system, a
//! Before and an After graph whose parts are anchored to the code, and what it brings and takes away, each row proven
//! by a run or anchored; or that it changes no part of the system. Before's anchors and Takes away's are checked at the
//! base commit, After's and Brings' at the head, with the note quote check. The union of both graphs, each part
//! marked kept, added or removed, is kept in the review's `review.json` and becomes the page's `system` section. A
//! review that hands in neither form is asked once at its turn end.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::frontier::brief::layout::lay_out_system;
use crate::frontier::brief::model::{Anchor, Mark, Section, SystemEdge, SystemNode, SystemRow};
use crate::frontier::server::bridge::Deliver;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::review_notes::{ReviewNotes, around, git_out_raw, head_of, reads, review_file};
use crate::frontier::server::steps::step_of;
use crate::frontier::server::thread::{Thread, ThreadRef};
use crate::frontier::server::works::prove;
use crate::frontier::server::worktrees::git_out;
use crate::shared::errors::{Error, Result};

#[cfg(test)]
#[path = "review_system_tests.rs"]
mod tests;

/// The tool's name.
pub const REVIEW_SYSTEM: &str = "review_system";

/// The most nodes one graph may have.
pub const MAX_NODES: usize = 12;

/// What a review that handed in neither form is told at its turn end, once.
pub const MISSING: &str = "review_system is missing: hand in the before and after graphs, or no_system_change";

/// What `review_system` handed in, as `review.json` keeps it: the union of the two graphs, marked, and the rows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemReview {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub nodes: Vec<SystemNode>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub edges: Vec<SystemEdge>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub brings: Vec<SystemRow>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub takes: Vec<SystemRow>,
	#[serde(default, skip_serializing_if = "std::ops::Not::not")]
	pub no_change: bool,
}

impl SystemReview {
	/// The page's `system` section.
	pub fn section(&self) -> Section {
		Section::System {
			heading: Some("System".to_owned()),
			nodes: self.nodes.clone(),
			edges: self.edges.clone(),
			brings: self.brings.clone(),
			takes: self.takes.clone(),
			no_change: self.no_change,
		}
	}
}

/// A node as the reviewer gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GivenNode {
	id: String,
	label: String,
	anchor: Anchor,
}

/// An edge as the reviewer gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GivenEdge {
	from: String,
	to: String,
	#[serde(default)]
	label: Option<String>,
	anchor: Anchor,
}

/// One graph as the reviewer gives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
	nodes: Vec<GivenNode>,
	#[serde(default)]
	edges: Vec<GivenEdge>,
}

/// A Brings or Takes away row as the reviewer gives it: a proof or an anchor.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GivenRow {
	text: String,
	#[serde(default)]
	proof: Option<Value>,
	#[serde(default)]
	anchor: Option<Anchor>,
}

/// `values[key]` read as `T`; the refusal names the key and what serde found wrong.
fn read<T: for<'de> Deserialize<'de>>(values: &Map<String, Value>, key: &str) -> Result<Option<T>> {
	match values.get(key) {
		None | Some(Value::Null) => Ok(None),
		Some(value) => {
			T::deserialize(value).map(Some).map_err(|error| Error::user(format!("{REVIEW_SYSTEM}: {key}: {error}.")))
		}
	}
}

fn blank(text: &str) -> bool {
	text.trim().is_empty()
}

/// One graph's own checks: at most 12 nodes, ids and labels given, ids unique, each edge between two of its nodes.
fn check_graph(graph: &Graph, side: &str) -> Result<()> {
	if graph.nodes.len() > MAX_NODES {
		return Err(Error::user(format!(
			"{REVIEW_SYSTEM}: {side} has {} nodes; at most {MAX_NODES} per graph: name the parts the change touches.",
			graph.nodes.len()
		)));
	}
	let mut ids = HashSet::new();
	for node in &graph.nodes {
		if blank(&node.id) || blank(&node.label) {
			return Err(Error::user(format!("{REVIEW_SYSTEM}: every {side} node needs an id and a label.")));
		}
		if !ids.insert(node.id.as_str()) {
			return Err(Error::user(format!("{REVIEW_SYSTEM}: {side} has two nodes with the id `{}`.", node.id)));
		}
	}
	for edge in &graph.edges {
		for end in [&edge.from, &edge.to] {
			if !ids.contains(end.as_str()) {
				return Err(Error::user(format!(
					"{REVIEW_SYSTEM}: the {side} edge {} → {} names `{end}`, which is no {side} node.",
					edge.from, edge.to
				)));
			}
		}
	}
	Ok(())
}

/// The two graphs as one, each part marked: in both `kept` (After's label and anchor), only in After `added`, only in
/// Before `removed`; laid out once.
fn union(before: &Graph, after: &Graph) -> Result<(Vec<SystemNode>, Vec<SystemEdge>)> {
	check_graph(before, "before")?;
	check_graph(after, "after")?;
	if before.nodes.is_empty() && after.nodes.is_empty() {
		return Err(Error::user(format!("{REVIEW_SYSTEM}: both graphs are empty; hand in no_system_change instead.")));
	}
	let was: HashSet<&str> = before.nodes.iter().map(|node| node.id.as_str()).collect();
	let is: HashSet<&str> = after.nodes.iter().map(|node| node.id.as_str()).collect();
	let mark = |id: &str| if was.contains(id) { Mark::Kept } else { Mark::Added };
	let mut nodes: Vec<SystemNode> = after
		.nodes
		.iter()
		.map(|node| SystemNode {
			id: node.id.clone(),
			label: node.label.clone(),
			mark: mark(&node.id),
			anchor: node.anchor.clone(),
			layer: 0,
			order: 0,
		})
		.collect();
	nodes.extend(before.nodes.iter().filter(|node| !is.contains(node.id.as_str())).map(|node| SystemNode {
		id: node.id.clone(),
		label: node.label.clone(),
		mark: Mark::Removed,
		anchor: node.anchor.clone(),
		layer: 0,
		order: 0,
	}));
	let key = |edge: &GivenEdge| (edge.from.clone(), edge.to.clone());
	let was_edges: HashSet<(String, String)> = before.edges.iter().map(key).collect();
	let is_edges: HashSet<(String, String)> = after.edges.iter().map(key).collect();
	let mut edges: Vec<SystemEdge> = after
		.edges
		.iter()
		.map(|edge| SystemEdge {
			from: edge.from.clone(),
			to: edge.to.clone(),
			label: edge.label.clone().filter(|label| !blank(label)),
			mark: if was_edges.contains(&key(edge)) { Mark::Kept } else { Mark::Added },
		})
		.collect();
	edges.extend(before.edges.iter().filter(|edge| !is_edges.contains(&key(edge))).map(|edge| SystemEdge {
		from: edge.from.clone(),
		to: edge.to.clone(),
		label: edge.label.clone().filter(|label| !blank(label)),
		mark: Mark::Removed,
	}));
	lay_out_system(&mut nodes, &edges).map_err(|error| Error::user(format!("{REVIEW_SYSTEM}: {error}.")))?;
	Ok((nodes, edges))
}

/// The commit Before's anchors are read at: a pull request's base, or the merge base of a thread's review range.
async fn base_of(t: &Thread, head: &str) -> Result<String> {
	if let Some(pr) = t.task.as_ref().and_then(|task| task.pr.as_ref()) {
		return Ok(pr.base_sha.clone());
	}
	let trunk = t
		.review
		.as_ref()
		.and_then(|review| review.running.as_ref())
		.and_then(|running| running.range.split_once("...").map(|(trunk, _)| trunk.to_owned()))
		.ok_or_else(|| {
			Error::user(format!("{REVIEW_SYSTEM}: this thread isn't in a review, so its base isn't known."))
		})?;
	git_out(&t.path, &["merge-base", &trunk, head])
		.await?
		.filter(|base| !base.is_empty())
		.ok_or_else(|| Error::user(format!("{REVIEW_SYSTEM}: {trunk} and {head} have no merge base to read.")))
}

/// What checks a call's anchors and runs its rows' proofs: files read at a commit, each once.
struct Checker<'a> {
	c: &'a ServiceContext,
	thread: &'a ThreadRef,
	path: &'a Path,
	read: HashMap<(String, String), Option<String>>,
}

impl Checker<'_> {
	/// Refuses `anchor` (of `what`, read at `sha`) unless the file holds its quote at its line.
	async fn check(&mut self, sha: &str, anchor: &Anchor, what: &str) -> Result<()> {
		let key = (sha.to_owned(), anchor.file.clone());
		if !self.read.contains_key(&key) {
			let text = git_out_raw(self.path, sha, &anchor.file).await?;
			self.read.insert(key.clone(), text);
		}
		let short = &sha[..sha.len().min(7)];
		let Some(text) = self.read.get(&key).and_then(Option::as_ref) else {
			return Err(Error::user(format!("{REVIEW_SYSTEM}: {what}: {} isn't in the commit {short}.", anchor.file)));
		};
		if !reads(text, anchor.line, &anchor.quote) {
			return Err(Error::user(format!(
				"{REVIEW_SYSTEM}: {what}: {}:{} at {short} doesn't read that. {}",
				anchor.file,
				anchor.line,
				around(text, anchor.line)
			)));
		}
		Ok(())
	}

	/// The rows of `list` (`brings` or `takes`) checked: an anchor read at `sha`, or a proof run like a `review_try`
	/// step.
	async fn rows(&mut self, list: &[GivenRow], sha: &str, list_name: &str) -> Result<Vec<SystemRow>> {
		let mut out = Vec::with_capacity(list.len());
		for (i, row) in list.iter().enumerate() {
			let what = format!("{list_name} row {}", i + 1);
			if blank(&row.text) {
				return Err(Error::user(format!("{REVIEW_SYSTEM}: {what} has no text.")));
			}
			match (&row.proof, &row.anchor) {
				(Some(proof), None) => {
					let step = step_of(proof, REVIEW_SYSTEM, &format!("{what}'s proof"))?;
					let ran = prove(self.c, self.thread, &step, REVIEW_SYSTEM, "anchor the row to the code instead")
						.await
						.map_err(|error| Error::user(format!("{error} ({what})")))?;
					out.push(SystemRow { text: row.text.trim().to_owned(), ran: Some(ran), anchor: None });
				}
				(None, Some(anchor)) => {
					self.check(sha, anchor, &what).await?;
					out.push(SystemRow { text: row.text.trim().to_owned(), ran: None, anchor: Some(anchor.clone()) });
				}
				_ => {
					return Err(Error::user(format!(
						"{REVIEW_SYSTEM}: {what} needs a proof {{run, expect}} or an anchor {{file, line, quote}}, one of them."
					)));
				}
			}
		}
		Ok(out)
	}
}

/// What the call handed in: the no-change form, or the two graphs and the rows read and checked against each other.
#[derive(Debug)]
enum Given {
	NoChange,
	Graphs { before: Graph, after: Graph, brings: Vec<GivenRow>, takes: Vec<GivenRow> },
}

fn given(values: &Map<String, Value>) -> Result<Given> {
	if let Some(extra) =
		values.keys().find(|key| !matches!(key.as_str(), "before" | "after" | "brings" | "takes" | "no_system_change"))
	{
		return Err(Error::user(format!("{REVIEW_SYSTEM}: unknown argument {extra}.")));
	}
	if values.contains_key("no_system_change") {
		if values.get("no_system_change") != Some(&Value::Bool(true)) || values.len() > 1 {
			return Err(Error::user(format!(
				"{REVIEW_SYSTEM}: no_system_change is true and comes alone; otherwise hand in before and after."
			)));
		}
		return Ok(Given::NoChange);
	}
	let (Some(before), Some(after)) = (read::<Graph>(values, "before")?, read::<Graph>(values, "after")?) else {
		return Err(Error::user(format!(
			"{REVIEW_SYSTEM}: hand in before and after ({{nodes, edges}} each) with brings and takes, or no_system_change: true."
		)));
	};
	let brings = read::<Vec<GivenRow>>(values, "brings")?.unwrap_or_default();
	let takes = read::<Vec<GivenRow>>(values, "takes")?.unwrap_or_default();
	Ok(Given::Graphs { before, after, brings, takes })
}

/// `review_system` (0710): the system view handed in, checked and stored; a second call replaces the first.
pub async fn review_system(c: &Arc<ServiceContext>, thread: &ThreadRef, values: &Map<String, Value>) -> Result<String> {
	let t = thread.get();
	let Some(task) = t.task.clone() else {
		return Err(Error::user(format!("{REVIEW_SYSTEM}: this thread has no task file to keep its review in.")));
	};
	let head = head_of(&t.path).await?;
	let file = review_file(&t.project.id, &task.id);
	if let Some(ended) = ReviewNotes::of_head(&file, &head).ended() {
		return Err(Error::user(format!(
			"{REVIEW_SYSTEM}: the review ended: {ended} failed. Hand in nothing more and end your turn."
		)));
	}
	let (system, said) = match given(values)? {
		Given::NoChange => {
			(SystemReview { no_change: true, ..SystemReview::default() }, "no change to the system".to_owned())
		}
		Given::Graphs { before, after, brings, takes } => {
			let (nodes, edges) = union(&before, &after)?;
			let base = base_of(&t, &head).await?;
			let mut checker = Checker { c, thread, path: &t.path, read: HashMap::new() };
			for (graph, sha, side) in [(&before, &base, "before"), (&after, &head, "after")] {
				for node in &graph.nodes {
					checker.check(sha, &node.anchor, &format!("{side} node {}", node.id)).await?;
				}
				for edge in &graph.edges {
					checker.check(sha, &edge.anchor, &format!("{side} edge {} → {}", edge.from, edge.to)).await?;
				}
			}
			let brings = checker.rows(&brings, &head, "brings").await?;
			let takes = checker.rows(&takes, &base, "takes").await?;
			let count = |mark: Mark| nodes.iter().filter(|node| node.mark == mark).count();
			let said = format!(
				"{} added, {} removed, {} kept; {} brings, {} takes away",
				count(Mark::Added),
				count(Mark::Removed),
				count(Mark::Kept),
				brings.len(),
				takes.len()
			);
			(SystemReview { nodes, edges, brings, takes, no_change: false }, said)
		}
	};
	// A row's proof can take minutes: the notes are read again before the view joins them.
	let mut notes = ReviewNotes::of_head(&file, &head);
	notes.system = Some(system);
	notes.write(&file)?;
	Ok(format!("{REVIEW_SYSTEM}: stored ({said})."))
}

/// At a review's turn end (0710): when it handed in no system view, it is asked once with [`MISSING`] and its turn
/// end waits for the next; whether it was asked.
pub async fn ask_once(c: &ServiceContext, thread: &ThreadRef) -> bool {
	if !c.options.ask_system {
		return false;
	}
	let t = thread.get();
	let Some(task) = t.task.as_ref() else { return false };
	let file = review_file(&t.project.id, &task.id);
	let mut notes = ReviewNotes::read(&file);
	if notes.system.is_some() || notes.system_asked || notes.ended().is_some() {
		return false;
	}
	notes.system_asked = true;
	if notes.write(&file).is_err() {
		return false;
	}
	c.bridge.prompt(&t.pane_id, c.bridge.seal(MISSING), Deliver::FollowUp).await.is_ok()
}
