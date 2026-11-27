//! What a brief window shows and sends, without a display (0721): the app id by kind, the labels each section turns
//! into, the actions its buttons send to the service, and how an answer patches one card.

use std::path::PathBuf;

use chrono::{Local, TimeZone};
use serde_json::{Value, json};

use crate::frontier::brief::model::{
	Anchor, Brief, Card, DiffFile, FlowNode, Mark, NodeState, RanProof, Section, SystemNode, Verdict, WorksState,
};
use crate::frontier::brief::store;

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

/// What a closed thread's brief says instead of its buttons.
pub const CLOSED: &str = "This thread is closed; this page is its record.";

/// What an empty section says.
pub const EMPTY: &str = "Nothing here.";

/// The window's app id (Linux) by the brief's kind: ultron places each kind's windows.
pub fn app_id(id: &str) -> &'static str {
	if id.starts_with("thread-") || id.starts_with("pr-") {
		"jocasta-review"
	} else if id.starts_with("show-") {
		"jocasta-show"
	} else {
		"jocasta-brief"
	}
}

/// `n` with its noun, `1 file`, `2 files`.
fn count(n: usize, noun: &str) -> String {
	if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// The diff's closed button: `Show code (2 files, +10 −3)`.
pub fn diff_summary(files: &[DiffFile]) -> String {
	let lines = files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines);
	let (added, removed) = lines.fold((0, 0), |(a, r), line| match line.kind.as_str() {
		"+" => (a + 1, r),
		"-" => (a, r + 1),
		_ => (a, r),
	});
	format!("Show code ({}, +{added} \u{2212}{removed})", count(files.len(), "file"))
}

/// A duration as he reads it: `340 ms`, `1.2 s`, `2 min 5 s`; none (or 0) is `not timed`.
pub fn duration(ms: Option<i64>) -> String {
	match ms {
		Some(ms) if ms >= 60_000 => format!("{} min {} s", ms / 60_000, ms % 60_000 / 1_000),
		Some(ms) if ms >= 1_000 => format!("{:.1} s", ms as f64 / 1_000.0),
		Some(ms) if ms > 0 => format!("{ms} ms"),
		_ => "not timed".to_owned(),
	}
}

/// An exit code as he reads it: `exit 0`, or `didn't run`.
pub fn exit(code: Option<i32>) -> String {
	code.map_or_else(|| "didn't run".to_owned(), |code| format!("exit {code}"))
}

/// A works row's state.
pub fn works_state(state: WorksState) -> &'static str {
	match state {
		WorksState::Passed => "passed",
		WorksState::Failed => "failed",
		WorksState::NotRun => "not run",
	}
}

/// A verdict as the switch shows it.
pub fn verdict(verdict: Verdict) -> &'static str {
	match verdict {
		Verdict::Post => "post",
		Verdict::Question => "question",
		Verdict::Skip => "skip",
	}
}

/// What a proof's disclosure holds: the command, what it had to give, how it ended and its output's tail.
pub fn proof_text(ran: &RanProof) -> String {
	let ended = ran.exit.map_or_else(|| "didn't end".to_owned(), |code| code.to_string());
	format!("$ {}\nexpect: {}\nexit: {ended}\n{}", ran.run, ran.expect, ran.tail.trim_end())
}

/// A card's words: his edit when he made one.
pub fn card_text(card: &Card) -> &str {
	card.edited.as_deref().filter(|text| !text.is_empty()).unwrap_or(&card.text)
}

/// Whether a card's Publish can be pressed: checked, kept, and not published yet.
pub fn publishable(card: &Card) -> bool {
	card.proof.ok && card.verdict != Verdict::Skip && card.published.is_none()
}

/// Whether a section has nothing to show.
pub fn is_empty(section: &Section) -> bool {
	match section {
		Section::Text { body, .. } => body.trim().is_empty(),
		Section::Facts { rows, .. } => rows.is_empty(),
		Section::Checks { rows, .. } => rows.is_empty(),
		Section::Flow { nodes, .. } => nodes.is_empty(),
		Section::Map { groups, .. } => groups.iter().all(|group| group.items.is_empty()),
		Section::Diff { files, .. } => files.is_empty(),
		Section::Cards { cards, .. } => cards.is_empty(),
		Section::Works { rows, .. } => rows.is_empty(),
		Section::Verdict { .. } => false,
		Section::System { nodes, brings, takes, no_change, .. } => {
			!no_change && nodes.is_empty() && brings.is_empty() && takes.is_empty()
		}
		Section::List { rows, .. } => rows.is_empty(),
	}
}

/// What a system says when the change leaves it as it was (0710).
pub const NO_SYSTEM_CHANGE: &str = "No change to the system.";

/// Where an anchor points: `src/a.rs:12`.
pub fn place(anchor: &Anchor) -> String {
	format!("{}:{}", anchor.file, anchor.line)
}

/// A system's parts as flow nodes to draw (0710): added green, removed red, kept neutral, each with its file name and
/// line under its label, after its mark when it was added or removed.
pub fn system_nodes(nodes: &[SystemNode]) -> Vec<FlowNode> {
	nodes
		.iter()
		.map(|node| FlowNode {
			id: node.id.clone(),
			label: node.label.clone(),
			group: None,
			state: match node.mark {
				Mark::Kept => NodeState::Pending,
				Mark::Added => NodeState::Done,
				Mark::Removed => NodeState::Failed,
			},
			detail: Some({
				let file = node.anchor.file.rsplit(['/', '\\']).next().unwrap_or(&node.anchor.file);
				match node.mark {
					Mark::Kept => format!("{file}:{}", node.anchor.line),
					mark => format!("{} \u{b7} {file}:{}", mark.as_str(), node.anchor.line),
				}
			}),
			layer: node.layer,
			order: node.order,
		})
		.collect()
}

/// The paragraphs of a text section: blank lines part them.
pub fn paragraphs(body: &str) -> Vec<String> {
	body.split("\n\n").map(str::trim).filter(|para| !para.is_empty()).map(str::to_owned).collect()
}

/// When the brief was made, in his local time.
pub fn generated(at_ms: i64) -> String {
	Local.timestamp_millis_opt(at_ms).single().map_or_else(String::new, |at| at.format("%-d %b %Y, %H:%M").to_string())
}

/// A screenshot `review_try` kept for brief `id` (0706): `<data>/briefs/assets/<id>/<name>`, or `None` when the id
/// or the name could leave that folder or isn't an image.
pub fn screenshot(id: &str, name: &str) -> Option<PathBuf> {
	let plain = !name.is_empty()
		&& !name.starts_with('.')
		&& name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
	let image = matches!(name.rsplit_once('.').map(|(_, ext)| ext), Some("png" | "jpg" | "gif" | "webp"));
	(store::valid_id(id) && plain && image).then(|| store::brief_dir().join("assets").join(id).join(name))
}

/// What a button asks the service for (the ops of `brief_actions.rs` and `pr_actions.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
	/// Save his words for a card.
	Edit { card: String, text: String },
	/// Ask the `@polish` helper (`op` is `polish` or `english`) about his text for a card (0711).
	Help { card: String, text: String, op: &'static str },
	/// Switch a card's verdict.
	Verdict { card: String, verdict: Verdict },
	/// Publish a card.
	Publish { card: String },
	/// Finish the thread: `merge` or `pr`.
	Finish { mode: &'static str },
	/// Send the thread back with his words.
	Send { text: String },
}

impl Act {
	/// The action as the service takes it.
	pub fn to_json(&self) -> Value {
		match self {
			Self::Edit { card, text } => json!({ "op": "edit", "card": card, "text": text }),
			Self::Help { card, text, op } => json!({ "op": op, "card": card, "text": text }),
			Self::Verdict { card, verdict: v } => json!({ "op": "verdict", "card": card, "verdict": verdict(*v) }),
			Self::Publish { card } => json!({ "op": "publish", "card": card }),
			Self::Finish { mode } => json!({ "op": "finish", "mode": mode }),
			Self::Send { text } => json!({ "op": "send", "text": text }),
		}
	}

	/// The card it acts on; `None` for the thread's own buttons.
	pub fn card(&self) -> Option<&str> {
		match self {
			Self::Edit { card, .. } | Self::Help { card, .. } | Self::Verdict { card, .. } | Self::Publish { card } => {
				Some(card)
			}
			Self::Finish { .. } | Self::Send { .. } => None,
		}
	}

	/// What the window says while it waits.
	pub fn waiting(&self) -> &'static str {
		match self {
			Self::Finish { .. } => "Waiting for your Allow in Jocasta\u{2026}",
			Self::Send { .. } => "Sending\u{2026}",
			Self::Help { op: "english", .. } => "Putting it into English\u{2026}",
			Self::Help { .. } => "Polishing\u{2026}",
			Self::Publish { .. } => "Publishing\u{2026}",
			Self::Edit { .. } | Self::Verdict { .. } => "Saving\u{2026}",
		}
	}
}

/// The service's answer to an action: the brief after it and a reword's suggestion `(card, text)`.
pub fn answer_of(value: &Value) -> Result<(Brief, Option<(String, String)>), String> {
	let brief =
		serde_json::from_value(value.clone()).map_err(|error| format!("Jocasta's answer wasn't a brief: {error}"))?;
	let suggestion = value
		.get("suggestion")
		.and_then(|s| Some((s.get("card")?.as_str()?.to_owned(), s.get("text")?.as_str()?.to_owned())));
	Ok((brief, suggestion))
}

fn cards_mut(brief: &mut Brief) -> impl Iterator<Item = &mut Card> {
	brief.sections.iter_mut().flat_map(|section| match section {
		Section::Cards { cards, .. } => cards.as_mut_slice(),
		_ => &mut [],
	})
}

/// The card `id` of `brief`.
pub fn card<'a>(brief: &'a Brief, id: &str) -> Option<&'a Card> {
	brief
		.sections
		.iter()
		.flat_map(|section| match section {
			Section::Cards { cards, .. } => cards.as_slice(),
			_ => &[],
		})
		.find(|card| card.id == id)
}

/// Puts the answer's card `id` in place of the shown one and leaves every other card as shown; whether it was found.
pub fn patch_card(brief: &mut Brief, answer: &Brief, id: &str) -> bool {
	let Some(fresh) = card(answer, id).cloned() else { return false };
	match cards_mut(brief).find(|card| card.id == id) {
		Some(shown) => {
			*shown = fresh;
			true
		}
		None => false,
	}
}
