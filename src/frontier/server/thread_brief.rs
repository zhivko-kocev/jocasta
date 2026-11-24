//! The thread brief (0678): `thread-<tid>`, built by code when a review is ready and again when the thread is stuck,
//! from what git, the check run and the ledger say. Facts, the phase table as a flow, the checks, the reviewer's notes
//! as cards, the changed files as a map and the diff. Nothing here asks a model.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{
	Brief, CheckRow, DiffFile, DiffHunk, DiffLine, Fact, FlowEdge, FlowNode, MapGroup, MapItem, MapLink, NodeState,
	Section,
};
use crate::frontier::brief::store;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::review_notes::{ReviewNotes, review_file};
use crate::frontier::server::task_file::CheckExit;
use crate::frontier::server::thread::{Thread, ThreadRef};
use crate::frontier::server::wording::{money, review_said};
use crate::frontier::server::worktrees::git_out;
use crate::shared::clock::now_ms;
use crate::shared::errors::Result;
use crate::shared::log::{LogLevel, log};

#[cfg(test)]
#[path = "thread_brief_tests.rs"]
mod tests;

/// The diff a brief shows holds at most this many lines; the rest is counted.
pub const DIFF_LINES: usize = 2_000;

/// Files one commit may link all-to-all in the map; a bigger commit links its files in a chain.
const LINK_ALL_UP_TO: usize = 12;

/// The most links a map carries.
const MAX_LINKS: usize = 200;

/// A thread brief's id: `thread-<task id>`; none for a thread with no task record.
pub fn brief_id(thread: &Thread) -> Option<String> {
	thread.task.as_ref().map(|task| format!("thread-{}", task.id))
}

/// What a brief shows that code read before building it: the checks' durations by command and a stuck gate's last
/// output lines.
#[derive(Debug, Clone, Default)]
pub struct BriefExtras {
	pub check_ms: Vec<(String, i64)>,
	pub gate_tail: Vec<String>,
}

/// Builds and stores `thread`'s brief; its id, or `None` when it has no task record or the build failed (said in the
/// log: a brief is never a reason to fail a review).
pub async fn refresh(c: &Arc<ServiceContext>, thread: &ThreadRef, extras: &BriefExtras) -> Option<String> {
	let t = thread.get();
	let id = brief_id(&t)?;
	match build(c, &t, &id, extras).await {
		Ok(brief) => match store::save(&brief) {
			Ok(()) => Some(id),
			Err(error) => {
				log(LogLevel::Warn, "brief", &format!("{id} wasn't stored: {error}"), None);
				None
			}
		},
		Err(error) => {
			log(LogLevel::Warn, "brief", &format!("{id} wasn't built: {error}"), None);
			None
		}
	}
}

async fn build(c: &Arc<ServiceContext>, t: &Thread, id: &str, extras: &BriefExtras) -> Result<Brief> {
	let trunk = c.places.trunk(&t.project).await.ok().flatten().filter(|trunk| !trunk.is_empty());
	let git = Git::read(&t.path, trunk.as_deref()).await;
	let spent = c.status.spend_of(&thread_ref_of(c, t)).await;
	let mut sections = vec![facts(t, trunk.as_deref(), &git, spent)];
	if t.stuck.as_deref().is_some_and(|stuck| stuck.starts_with("stuck:")) && !extras.gate_tail.is_empty() {
		sections.push(Section::Text {
			heading: Some("Last output of the gate".to_owned()),
			body: extras.gate_tail.join("\n"),
		});
	}
	sections.push(flow(t));
	if let Some(checks) = checks(t, extras) {
		sections.push(checks);
	}
	let notes = t.task.as_ref().map(|task| ReviewNotes::read(&review_file(&t.project.id, &task.id)));
	if let Some(notes) = notes.filter(|notes| !notes.notes.is_empty()) {
		sections.push(Section::Cards { heading: Some("Review notes".to_owned()), cards: notes.notes });
	}
	if !git.numstat.is_empty() {
		sections.push(map(&git));
	}
	if !git.diff.is_empty() {
		sections.push(Section::Diff { heading: Some("Diff".to_owned()), files: parse_diff(&git.diff, DIFF_LINES) });
	}
	let mut brief = Brief {
		id: id.to_owned(),
		kind: "thread".to_owned(),
		title: t.title.clone(),
		subtitle: format!("{} · {}", t.project.name, t.branch),
		generated_at_ms: now_ms(),
		sections,
	};
	lay_out(&mut brief).map_err(crate::shared::errors::Error::other)?;
	Ok(brief)
}

fn thread_ref_of(c: &Arc<ServiceContext>, t: &Thread) -> ThreadRef {
	c.threads.get(&t.pane_id).unwrap_or_else(|| crate::frontier::server::context::Shared::new(t.clone()))
}

/// What git says of the thread's branch against trunk.
#[derive(Debug, Default)]
struct Git {
	head: String,
	commits: Vec<String>,
	/// `(added, removed, path)` per changed file.
	numstat: Vec<(usize, usize, String)>,
	/// The files of each commit, newest first.
	commit_files: Vec<Vec<String>>,
	diff: String,
}

impl Git {
	async fn read(path: &Path, trunk: Option<&str>) -> Self {
		let ask = |args: Vec<String>| async move { git_out(path, &args).await.ok().flatten().unwrap_or_default() };
		let head = ask(vec!["rev-parse".into(), "HEAD".into()]).await;
		let Some(trunk) = trunk else { return Self { head, ..Self::default() } };
		let range = format!("{trunk}...HEAD");
		let log_range = format!("{trunk}..HEAD");
		let (log, numstat, names, diff) = tokio::join!(
			ask(vec!["log".into(), "--format=%h %s".into(), log_range.clone()]),
			ask(vec!["diff".into(), "--numstat".into(), range.clone()]),
			ask(vec!["log".into(), "--name-only".into(), "--format=%x00".into(), log_range]),
			ask(vec!["diff".into(), "--no-color".into(), "--no-ext-diff".into(), range]),
		);
		Self {
			head,
			commits: log.lines().map(str::to_owned).collect(),
			numstat: numstat
				.lines()
				.filter_map(|line| {
					let mut parts = line.splitn(3, '\t');
					let (added, removed, file) = (parts.next()?, parts.next()?, parts.next()?);
					// A binary file's counts are `-`.
					Some((added.parse().unwrap_or(0), removed.parse().unwrap_or(0), file.to_owned()))
				})
				.collect(),
			commit_files: names
				.split('\0')
				.map(|block| block.lines().filter(|line| !line.is_empty()).map(str::to_owned).collect::<Vec<_>>())
				.filter(|files| !files.is_empty())
				.collect(),
			diff,
		}
	}
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
	Fact { label: label.to_owned(), value: value.into(), url: None }
}

fn facts(t: &Thread, trunk: Option<&str>, git: &Git, spent: Option<f64>) -> Section {
	let mut rows = vec![fact("Project", t.project.name.clone()), fact("Branch", t.branch.clone())];
	if let Some(trunk) = trunk {
		rows.push(fact("Trunk", trunk));
	}
	if !git.head.is_empty() {
		rows.push(fact("Head", &git.head[..git.head.len().min(7)]));
	}
	let plural = if git.commits.len() == 1 { "" } else { "s" };
	rows.push(fact("Commits", format!("{} commit{plural}", git.commits.len())));
	let (added, removed): (usize, usize) = git.numstat.iter().fold((0, 0), |(a, r), (add, del, _)| (a + add, r + del));
	let files = git.numstat.len();
	let plural = if files == 1 { "" } else { "s" };
	rows.push(fact("Diff", format!("+{added}/−{removed} in {files} file{plural}")));
	if let Some(result) = t.review.as_ref().and_then(|review| review.result.as_ref()) {
		rows.push(fact("Review", review_said(result.notes, result.tests, result.blocking)));
	}
	let budget = t.task.as_ref().and_then(crate::frontier::server::pane_launch::thread_budget);
	if let Some(spent) = spent {
		rows.push(fact("Spent", money(spent)));
	}
	if let Some(budget) = budget {
		rows.push(fact("Budget", format!("${}", crate::frontier::protocol::js_number_text(budget))));
	}
	if let Some(stuck) = &t.stuck {
		rows.push(fact("Stopped", stuck.clone()));
	}
	Section::Facts { heading: None, rows }
}

fn node(id: &str, label: &str, state: NodeState, detail: Option<String>) -> FlowNode {
	FlowNode {
		id: id.to_owned(),
		label: label.to_owned(),
		group: None,
		state,
		detail: detail.filter(|detail| !detail.is_empty()),
		layer: 0,
		order: 0,
	}
}

fn passed(checks: &[CheckExit]) -> bool {
	checks.iter().all(|check| check.exit == Some(0))
}

/// The thread's phase table as a flow: the task, then each review's checks and review with the fix round that
/// followed a failed one, then his call. The current step is `active`; a failed gate or review is `failed`.
fn flow(t: &Thread) -> Section {
	let reviews = t.task.as_ref().map(|task| task.reviews.as_slice()).unwrap_or_default();
	let stuck = t.stuck.clone();
	let mut nodes = Vec::new();
	if reviews.is_empty() {
		let state = if stuck.is_some() { NodeState::Failed } else { NodeState::Active };
		nodes.push(node("task", "Task", state, stuck.clone()));
	} else {
		nodes.push(node("task", "Task", NodeState::Done, None));
	}
	for (index, review) in reviews.iter().enumerate() {
		let n = index + 1;
		let checks_state = if passed(&review.checks) { NodeState::Done } else { NodeState::Failed };
		let gates = review.checks.len();
		nodes.push(node(&format!("check{n}"), "Checks", checks_state, Some(format!("{gates} run"))));
		let review_state = if review.passed { NodeState::Done } else { NodeState::Failed };
		nodes.push(node(&format!("review{n}"), "Review", review_state, Some(review.said.clone())));
		if index + 1 < reviews.len() {
			nodes.push(node(&format!("fix{n}"), &format!("Fix round {n}"), NodeState::Done, None));
		}
	}
	if !reviews.is_empty() {
		match stuck {
			Some(stuck) => nodes.push(node("stuck", "Stopped", NodeState::Failed, Some(stuck))),
			None => nodes.push(node("you", "Your call", NodeState::Active, None)),
		}
	}
	let edges = nodes
		.windows(2)
		.map(|pair| FlowEdge { from: pair[0].id.clone(), to: pair[1].id.clone(), label: None })
		.collect();
	Section::Flow { heading: Some("Where it stands".to_owned()), nodes, edges }
}

/// The last review's checks with their exits and durations.
fn checks(t: &Thread, extras: &BriefExtras) -> Option<Section> {
	let result = t.review.as_ref()?.result.as_ref()?;
	if result.checks.is_empty() {
		return None;
	}
	let rows = result
		.checks
		.iter()
		.map(|check| CheckRow {
			command: check.command.clone(),
			exit: check.exit,
			ms: extras.check_ms.iter().find(|(command, _)| *command == check.command).map_or(0, |(_, ms)| *ms),
		})
		.collect();
	Some(Section::Checks { heading: Some("Checks".to_owned()), rows })
}

/// The changed files grouped by their top folder, with a link between two files changed in the same commit.
fn map(git: &Git) -> Section {
	let mut groups: BTreeMap<String, Vec<MapItem>> = BTreeMap::new();
	for (added, removed, path) in &git.numstat {
		let group = path.split_once('/').map_or("(root)", |(top, _)| top).to_owned();
		groups.entry(group).or_default().push(MapItem {
			id: path.clone(),
			label: path.rsplit('/').next().unwrap_or(path).to_owned(),
			added: *added,
			removed: *removed,
		});
	}
	let changed: HashSet<&str> = git.numstat.iter().map(|(_, _, path)| path.as_str()).collect();
	let mut seen: HashSet<(String, String)> = HashSet::new();
	let mut links = Vec::new();
	'commits: for files in &git.commit_files {
		let files: Vec<&String> = files.iter().filter(|file| changed.contains(file.as_str())).collect();
		let pairs: Vec<(&String, &String)> = if files.len() <= LINK_ALL_UP_TO {
			files.iter().enumerate().flat_map(|(i, a)| files[i + 1..].iter().map(move |b| (*a, *b))).collect()
		} else {
			files.windows(2).map(|pair| (pair[0], pair[1])).collect()
		};
		for (a, b) in pairs {
			let key = if a <= b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
			if seen.insert(key) {
				links.push(MapLink { a: a.clone(), b: b.clone() });
				if links.len() >= MAX_LINKS {
					break 'commits;
				}
			}
		}
	}
	Section::Map {
		heading: Some("Changed files".to_owned()),
		groups: groups.into_iter().map(|(label, items)| MapGroup { label, items }).collect(),
		links,
	}
}

/// A unified `git diff` as files, hunks and numbered lines, cut at `cap` lines of the diff: what is left is counted in
/// one last line, `… <n> more lines`.
pub fn parse_diff(diff: &str, cap: usize) -> Vec<DiffFile> {
	let all: Vec<&str> = diff.lines().collect();
	let more = all.len().saturating_sub(cap);
	let mut files: Vec<DiffFile> = Vec::new();
	let (mut old, mut new) = (0_usize, 0_usize);
	for line in &all[..all.len().min(cap)] {
		if let Some(rest) = line.strip_prefix("diff --git ") {
			let path = rest.rsplit_once(" b/").map_or(rest, |(_, path)| path).to_owned();
			files.push(DiffFile { path, hunks: Vec::new() });
		} else if let Some(header) = line.strip_prefix("@@") {
			let Some(file) = files.last_mut() else { continue };
			(old, new) = hunk_starts(header);
			file.hunks.push(DiffHunk { header: (*line).to_owned(), lines: Vec::new() });
		} else if let Some(file) = files.last_mut() {
			let Some(hunk) = file.hunks.last_mut() else {
				// Before the first hunk: a binary file's note, a mode change or a rename.
				if line.starts_with("Binary files") || line.starts_with("rename ") || line.starts_with("new file") {
					file.hunks.push(DiffHunk {
						header: String::new(),
						lines: vec![DiffLine { kind: " ".to_owned(), old: None, new: None, text: (*line).to_owned() }],
					});
				}
				continue;
			};
			let (kind, text) = line.split_at(line.chars().next().map_or(0, char::len_utf8));
			match kind {
				"+" => {
					hunk.lines.push(DiffLine { kind: "+".into(), old: None, new: Some(new), text: text.to_owned() });
					new += 1;
				}
				"-" => {
					hunk.lines.push(DiffLine { kind: "-".into(), old: Some(old), new: None, text: text.to_owned() });
					old += 1;
				}
				" " => {
					hunk.lines.push(DiffLine {
						kind: " ".into(),
						old: Some(old),
						new: Some(new),
						text: text.to_owned(),
					});
					old += 1;
					new += 1;
				}
				// `\ No newline at end of file`.
				_ => hunk.lines.push(DiffLine { kind: " ".into(), old: None, new: None, text: (*line).to_owned() }),
			}
		}
	}
	if more > 0 {
		let note = DiffLine { kind: " ".into(), old: None, new: None, text: format!("… {more} more lines") };
		match files.last_mut() {
			Some(file) => match file.hunks.last_mut() {
				Some(hunk) => hunk.lines.push(note),
				None => file.hunks.push(DiffHunk { header: String::new(), lines: vec![note] }),
			},
			None => files.push(DiffFile {
				path: "…".to_owned(),
				hunks: vec![DiffHunk { header: String::new(), lines: vec![note] }],
			}),
		}
	}
	files
}

/// The old and new first line numbers of a hunk header's text after the opening `@@` (` -12,5 +14,7 @@ context`).
fn hunk_starts(header: &str) -> (usize, usize) {
	let number = |sign: char| {
		header
			.split_whitespace()
			.find_map(|word| word.strip_prefix(sign))
			.and_then(|range| range.split(',').next())
			.and_then(|first| first.parse::<usize>().ok())
			.unwrap_or(1)
	};
	(number('-'), number('+'))
}
