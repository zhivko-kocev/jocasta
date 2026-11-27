//! The `show` spec (0714): what the boss's `show` tool and `jocasta brief open` (0716) both read. One reader checks it
//! all before anything is stored: strict keys (an unknown one is named), the label and diff limits, map ids and links
//! like a flow's nodes and edges, and each flow laid out. A `diff` or `map` section may instead name a thread; the
//! service fills it from that thread's branch (`thread_brief::reference`). Every error is a line starting `show: `.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::frontier::brief::layout::lay_out_flow;
use crate::frontier::brief::model::{
	Brief, DiffFile, Fact, FlowEdge, FlowNode, ListRow, MapGroup, MapLink, NodeState, Section,
};
use crate::frontier::brief::store::next_show_id;
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;

/// The section types a spec may hold (0679, 0714). `checks`, `works`, `system`, `cards` and `verdict` say what Jocasta
/// ran or checked, so only its code fills them.
pub const SECTIONS: [&str; 6] = ["text", "facts", "flow", "map", "diff", "list"];
/// The longest node label, in characters.
pub const NODE_LABEL: usize = 80;
/// The longest edge label, in characters.
pub const EDGE_LABEL: usize = 24;
/// The longest fact label, in characters.
pub const FACT_LABEL: usize = 40;
/// The most diff lines a spec may give, all its diffs together.
pub const DIFF_LINES: usize = 2_000;

/// A spec as read: its title and its sections, top to bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
	pub title: String,
	pub sections: Vec<Part>,
}

/// One section of a spec: given in full, or a thread's diff or map for the service to fill.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
	Given(Section),
	Thread { of: Reference, heading: Option<String>, thread: String },
}

/// What a reference section shows of its thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
	Diff,
	Map,
}

impl Reference {
	/// The section's type name.
	pub fn type_name(self) -> &'static str {
		match self {
			Self::Diff => "diff",
			Self::Map => "map",
		}
	}
}

impl Spec {
	/// The threads its reference sections name, with their 1-based section numbers.
	pub fn references(&self) -> impl Iterator<Item = (usize, &str)> {
		self.sections.iter().enumerate().filter_map(|(index, part)| match part {
			Part::Thread { thread, .. } => Some((index + 1, thread.as_str())),
			Part::Given(_) => None,
		})
	}
}

/// The `show-<n>` brief of a spec whose sections are all filled.
pub fn show_brief(title: String, sections: Vec<Section>) -> Brief {
	Brief {
		id: next_show_id(),
		kind: "show".to_owned(),
		title,
		subtitle: String::new(),
		generated_at_ms: now_ms(),
		sections,
	}
}

/// A flow node as a spec gives it: no layer or order (the layout sets them).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
	id: String,
	label: String,
	#[serde(default)]
	group: Option<String>,
	state: NodeState,
	#[serde(default)]
	detail: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Given {
	Text {
		#[serde(default)]
		heading: Option<String>,
		body: String,
	},
	Facts {
		#[serde(default)]
		heading: Option<String>,
		rows: Vec<Fact>,
	},
	Flow {
		#[serde(default)]
		heading: Option<String>,
		nodes: Vec<Node>,
		#[serde(default)]
		edges: Vec<FlowEdge>,
	},
	Map {
		#[serde(default)]
		heading: Option<String>,
		#[serde(default)]
		groups: Option<Vec<MapGroup>>,
		#[serde(default)]
		links: Option<Vec<MapLink>>,
		#[serde(default)]
		thread: Option<String>,
	},
	Diff {
		#[serde(default)]
		heading: Option<String>,
		#[serde(default)]
		files: Option<Vec<DiffFile>>,
		#[serde(default)]
		thread: Option<String>,
	},
	List {
		#[serde(default)]
		heading: Option<String>,
		rows: Vec<ListRow>,
	},
}

fn refused(text: String) -> Error {
	Error::user(format!("show: {text}"))
}

/// Reads and checks a spec: `{title, sections}`.
pub fn read(spec: &Map<String, Value>) -> Result<Spec> {
	if let Some(key) = spec.keys().find(|key| !matches!(key.as_str(), "title" | "sections")) {
		return Err(refused(format!("unknown field `{key}`; a spec has title and sections.")));
	}
	let title = spec.get("title").and_then(Value::as_str).map(str::trim).unwrap_or_default();
	if title.is_empty() {
		return Err(refused("title is required.".to_owned()));
	}
	let Some(Value::Array(given)) = spec.get("sections") else {
		return Err(refused("sections is required: an array of sections, each with a `type`.".to_owned()));
	};
	if given.is_empty() {
		return Err(refused("sections is empty; give at least one section.".to_owned()));
	}
	let mut diff_lines = 0;
	let mut sections = Vec::with_capacity(given.len());
	for (index, section) in given.iter().enumerate() {
		let number = index + 1;
		let kind = section.get("type").and_then(Value::as_str).unwrap_or("");
		if !SECTIONS.contains(&kind) {
			return Err(refused(format!("section {number} has type `{kind}`; the types are {}.", SECTIONS.join(", "))));
		}
		let parsed: Given = serde_json::from_value(section.clone())
			.map_err(|error| refused(format!("section {number} ({kind}) isn't well formed: {error}")))?;
		let part =
			checked(parsed, &mut diff_lines).map_err(|why| refused(format!("section {number} ({kind}): {why}")))?;
		sections.push(part);
	}
	if diff_lines > DIFF_LINES {
		return Err(refused(format!("the diffs have {diff_lines} lines; at most {DIFF_LINES} are drawn.")));
	}
	Ok(Spec { title: title.to_owned(), sections })
}

/// Longer than `limit` characters.
fn over(text: &str, limit: usize) -> bool {
	text.chars().count() > limit
}

/// One section checked and turned into the model's; `diff_lines` counts the diff lines given so far.
fn checked(given: Given, diff_lines: &mut usize) -> std::result::Result<Part, String> {
	Ok(Part::Given(match given {
		Given::Text { heading, body } => Section::Text { heading, body },
		Given::Facts { heading, rows } => {
			if let Some(row) = rows.iter().find(|row| over(&row.label, FACT_LABEL)) {
				return Err(format!("the fact label `{}` is longer than {FACT_LABEL} characters", row.label));
			}
			Section::Facts { heading, rows }
		}
		Given::Flow { heading, nodes, edges } => {
			if let Some(node) = nodes.iter().find(|node| over(&node.label, NODE_LABEL)) {
				return Err(format!("node `{}` has a label longer than {NODE_LABEL} characters", node.id));
			}
			if let Some(edge) = edges.iter().find(|edge| edge.label.as_deref().is_some_and(|l| over(l, EDGE_LABEL))) {
				return Err(format!(
					"the edge {} → {} has a label longer than {EDGE_LABEL} characters",
					edge.from, edge.to
				));
			}
			let mut nodes: Vec<FlowNode> = nodes
				.into_iter()
				.map(|n| FlowNode {
					id: n.id,
					label: n.label,
					group: n.group,
					state: n.state,
					detail: n.detail,
					layer: 0,
					order: 0,
				})
				.collect();
			lay_out_flow(&mut nodes, &edges)?;
			Section::Flow { heading, nodes, edges }
		}
		Given::Map { heading, groups, links, thread } => match (groups, thread) {
			(None, Some(thread)) if links.is_none() => return Ok(Part::Thread { of: Reference::Map, heading, thread }),
			(Some(groups), None) => {
				let links = links.unwrap_or_default();
				map_checked(&groups, &links)?;
				Section::Map { heading, groups, links }
			}
			(None, None) => return Err("give groups, or a thread to fill it from".to_owned()),
			_ => return Err("give groups and links, or a thread, not both".to_owned()),
		},
		Given::Diff { heading, files, thread } => match (files, thread) {
			(None, Some(thread)) => return Ok(Part::Thread { of: Reference::Diff, heading, thread }),
			(Some(mut files), None) => {
				for line in files.iter_mut().flat_map(|file| &mut file.hunks).flat_map(|hunk| &mut hunk.lines) {
					match line.kind.as_str() {
						"+" | "-" | " " => {}
						"space" => line.kind = " ".to_owned(),
						other => return Err(format!("a line's kind is `{other}`; it is +, -, or a space (`space`)")),
					}
					*diff_lines += 1;
				}
				Section::Diff { heading, files }
			}
			(None, None) => return Err("give files, or a thread to fill it from".to_owned()),
			(Some(_), Some(_)) => return Err("give files or a thread, not both".to_owned()),
		},
		Given::List { heading, rows } => Section::List { heading, rows },
	}))
}

/// A map's item ids are unique and each link joins two of them, as a flow's nodes and edges.
fn map_checked(groups: &[MapGroup], links: &[MapLink]) -> std::result::Result<(), String> {
	let mut ids: HashSet<&str> = HashSet::new();
	for item in groups.iter().flat_map(|group| &group.items) {
		if !ids.insert(item.id.as_str()) {
			return Err(format!("the map has two items with the id `{}`", item.id));
		}
	}
	for link in links {
		if let Some(end) = [&link.a, &link.b].into_iter().find(|end| !ids.contains(end.as_str())) {
			return Err(format!("the link {} – {} names `{end}`, which is no item of the map", link.a, link.b));
		}
	}
	Ok(())
}
