//! The brief's model (0678): what a page shows, as serde types. Field names are the page's own (snake_case).

use serde::{Deserialize, Serialize};

/// One brief: a title and sections, top to bottom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Brief {
	pub id: String,
	/// `thread`, `pr`, `morning`, `weekly` or `show`: the page offers actions by it.
	pub kind: String,
	pub title: String,
	#[serde(default)]
	pub subtitle: String,
	pub generated_at_ms: i64,
	pub sections: Vec<Section>,
}

/// A section of a brief, tagged by `type`. Any section may carry a `heading`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Section {
	/// Plain paragraphs.
	Text {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		body: String,
	},
	Facts {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		rows: Vec<Fact>,
	},
	Checks {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		rows: Vec<CheckRow>,
	},
	Flow {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		nodes: Vec<FlowNode>,
		#[serde(default)]
		edges: Vec<FlowEdge>,
	},
	Map {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		groups: Vec<MapGroup>,
		#[serde(default)]
		links: Vec<MapLink>,
	},
	Diff {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		files: Vec<DiffFile>,
	},
	Cards {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		cards: Vec<Card>,
	},
	List {
		#[serde(default, skip_serializing_if = "Option::is_none")]
		heading: Option<String>,
		rows: Vec<ListRow>,
	},
}

impl Section {
	/// The section's type name as the page and the errors say it.
	pub fn type_name(&self) -> &'static str {
		match self {
			Self::Text { .. } => "text",
			Self::Facts { .. } => "facts",
			Self::Checks { .. } => "checks",
			Self::Flow { .. } => "flow",
			Self::Map { .. } => "map",
			Self::Diff { .. } => "diff",
			Self::Cards { .. } => "cards",
			Self::List { .. } => "list",
		}
	}
}

/// One labelled value of a `facts` section, linking to `url` when it has one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
	pub label: String,
	pub value: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
}

/// One check of a `checks` section: its command, exit code and duration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckRow {
	pub command: String,
	/// `None`: it didn't run.
	pub exit: Option<i32>,
	pub ms: i64,
}

/// Where a flow node stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
	Done,
	Active,
	Failed,
	Pending,
}

/// One box of a `flow`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
	pub id: String,
	pub label: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub group: Option<String>,
	pub state: NodeState,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
	/// Longest path from a source, set by `layout::lay_out` (a brief as given carries none).
	#[serde(default)]
	pub layer: usize,
	/// Position within the layer, in input order, set by `layout::lay_out`.
	#[serde(default)]
	pub order: usize,
}

/// One arrow of a `flow`, from one node id to another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
	pub from: String,
	pub to: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub label: Option<String>,
}

/// A column of a `map`: a label and its items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapGroup {
	pub label: String,
	pub items: Vec<MapItem>,
}

/// One file of a `map`, with the lines it added and removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapItem {
	pub id: String,
	pub label: String,
	pub added: usize,
	pub removed: usize,
}

/// An arc of a `map` between two item ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapLink {
	pub a: String,
	pub b: String,
}

/// One file of a `diff` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffFile {
	pub path: String,
	pub hunks: Vec<DiffHunk>,
}

/// One hunk of a diff file: its header and lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
	pub header: String,
	pub lines: Vec<DiffLine>,
}

/// One line of a diff hunk with its numbers on either side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
	/// `+`, `-` or a space.
	pub kind: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub old: Option<usize>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub new: Option<usize>,
	pub text: String,
}

/// A review note's weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
	P0,
	P1,
	P2,
}

impl Severity {
	/// The severity as the page and the tool say it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::P0 => "P0",
			Self::P1 => "P1",
			Self::P2 => "P2",
		}
	}

	/// P0 and P1 block a review (0453).
	pub fn blocking(self) -> bool {
		matches!(self, Self::P0 | Self::P1)
	}
}

/// What the reviewer says to do with a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
	Post,
	Question,
	Skip,
}

/// Whether a card's quote was found in the file at the head it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
	pub sha: String,
	pub ok: bool,
}

/// Where and when a card was published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Published {
	pub url: String,
	pub at_ms: i64,
}

/// One review note, machine-checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
	pub id: String,
	pub severity: Severity,
	pub file: String,
	pub line: usize,
	pub quote: String,
	pub text: String,
	pub verdict: Verdict,
	pub proof: Proof,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub edited: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub published: Option<Published>,
}

/// One row of a `list` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListRow {
	pub text: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub note: Option<String>,
}
