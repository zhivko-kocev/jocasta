//! A project's lessons (0459): `<data>/projects/<pid>/lessons`, outside client repos, which every pane of the project
//! sees as the omp skill `lessons` through its overlay's `skills.customDirectories`. `SKILL.md` is the index; each
//! lesson is its own file beside it, written by the boss's `lesson` tool only on his word.
//!
//! Beside it, `<data>/projects/<pid>/.omp/agents/drift.md` is the weekly big-picture review's `drift` agent (0583):
//! omp finds project agents in the nearest `.omp/agents` above its working folder, and the weekly review runs in
//! `<data>/projects/<pid>/weekly`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::shared::errors::Result;
use crate::shared::fsx::write_atomically;
use crate::shared::install::shipped_config_dir;
use crate::shared::paths::{lessons_dir, project_dir};

static LESSONS_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^##\s+Lessons\s*$").expect("a valid pattern"));
static LESSON_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- \[.*\]\(.*\)").expect("a valid pattern"));

/// `text` without its trailing whitespace, split at `\r?\n`.
fn lines_of(text: &str) -> Vec<&str> {
	text.trim_end().split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect()
}

/// The index with the shipped header (everything up to its `## Lessons` heading) and the lesson lines `current` holds
/// below its own heading, so a shipped edit of the header reaches every project (0583).
fn refreshed_index(shipped: &str, current: &str) -> String {
	let shipped_lines = lines_of(shipped);
	let header: Vec<&str> = match shipped_lines.iter().position(|line| LESSONS_HEADING.is_match(line)) {
		Some(heading) => shipped_lines[..=heading].to_vec(),
		None => shipped_lines.iter().copied().chain(["", "## Lessons"]).collect(),
	};
	let current_lines = lines_of(current);
	let lessons: Vec<&str> = match current_lines.iter().position(|line| LESSONS_HEADING.is_match(line)) {
		Some(at) => current_lines[at + 1..].to_vec(),
		None => current_lines.iter().copied().filter(|line| LESSON_LINE.is_match(line)).collect(),
	};
	format!("{}\n", header.into_iter().chain(lessons).collect::<Vec<_>>().join("\n"))
}

/// Writes `file` with `text` unless it already holds exactly that.
fn write_when_changed(file: &Path, text: &str) -> Result<()> {
	if fs::read_to_string(file).is_ok_and(|existing| existing == text) {
		return Ok(());
	}
	write_atomically(file, text, None)
}

/// The project's lessons folder: its `SKILL.md` index with the shipped header from `config/lessons/SKILL.md` and the
/// project's lesson lines, and the shipped `drift` agent from `config/agents/drift.md` beside it. `config_dir` is the
/// shipped config folder (the install's, or the repo's in a source run, by default).
pub fn ensure_lessons_skill(project_id: &str, config_dir: Option<&Path>) -> Result<PathBuf> {
	let config_dir = config_dir.map_or_else(|| shipped_config_dir(None), Path::to_path_buf);
	let dir = lessons_dir(project_id);
	let index = dir.join("SKILL.md");
	let shipped = fs::read_to_string(config_dir.join("lessons").join("SKILL.md"))?;
	let text = if index.exists() { refreshed_index(&shipped, &fs::read_to_string(&index)?) } else { shipped };
	write_when_changed(&index, &text)?;
	write_when_changed(
		&project_dir(project_id).join(".omp").join("agents").join("drift.md"),
		&fs::read_to_string(config_dir.join("agents").join("drift.md"))?,
	)?;
	Ok(dir)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_jocasta;
	use crate::shared::yaml::parse_value;

	/// A config folder with a lessons index header and a drift agent.
	fn config(root: &Path, header: &str) -> PathBuf {
		let dir = root.join("config");
		fs::create_dir_all(dir.join("lessons")).unwrap();
		fs::create_dir_all(dir.join("agents")).unwrap();
		fs::write(dir.join("lessons").join("SKILL.md"), format!("---\nname: lessons\n---\n\n{header}\n\n## Lessons\n"))
			.unwrap();
		fs::write(dir.join("agents").join("drift.md"), "---\nname: drift\ndescription: d\n---\nbody\n").unwrap();
		dir
	}

	#[test]
	fn a_new_project_gets_the_shipped_index_and_the_drift_agent_where_omp_finds_project_agents() {
		let env = temp_jocasta();
		let dir = config(env.root(), "Only on his word.");
		assert_eq!(ensure_lessons_skill("p-1", Some(&dir)).unwrap(), lessons_dir("p-1"));
		assert_eq!(
			fs::read_to_string(lessons_dir("p-1").join("SKILL.md")).unwrap(),
			fs::read_to_string(dir.join("lessons").join("SKILL.md")).unwrap()
		);
		// The weekly review runs in <project>/weekly; omp walks up from there to the nearest `.omp/agents`.
		assert_eq!(
			fs::read_to_string(project_dir("p-1").join(".omp").join("agents").join("drift.md")).unwrap(),
			fs::read_to_string(dir.join("agents").join("drift.md")).unwrap()
		);
	}

	#[test]
	fn a_shipped_header_change_reaches_an_existing_index_and_its_lessons_stay() {
		let env = temp_jocasta();
		ensure_lessons_skill("p-1", Some(&config(env.root(), "Only when he says so."))).unwrap();
		let index = lessons_dir("p-1").join("SKILL.md");
		let lines = "- [Retry with backoff](retry-with-backoff.md) — adding an HTTP call\n- [No sync git](no-sync-git.md) — timers\n";
		fs::write(&index, format!("{}{lines}", fs::read_to_string(&index).unwrap())).unwrap();
		ensure_lessons_skill("p-1", Some(&config(env.root(), "Only on his word."))).unwrap();
		assert_eq!(
			fs::read_to_string(&index).unwrap(),
			format!("---\nname: lessons\n---\n\nOnly on his word.\n\n## Lessons\n{lines}")
		);
	}

	#[test]
	fn an_index_without_its_heading_keeps_only_its_lesson_lines() {
		let env = temp_jocasta();
		let dir = config(env.root(), "Header.");
		let index = lessons_dir("p-2").join("SKILL.md");
		fs::create_dir_all(index.parent().unwrap()).unwrap();
		fs::write(&index, "old text\r\n- [A](a.md) — x\r\nnoise\r\n").unwrap();
		ensure_lessons_skill("p-2", Some(&dir)).unwrap();
		assert_eq!(
			fs::read_to_string(&index).unwrap(),
			"---\nname: lessons\n---\n\nHeader.\n\n## Lessons\n- [A](a.md) — x\n"
		);
	}

	#[test]
	fn the_shipped_drift_agent_is_named_drift_and_reviews_for_drift_not_bugs() {
		let text = fs::read_to_string(shipped_config_dir(None).join("agents").join("drift.md")).unwrap();
		let front = parse_value(text.split("---").nth(1).unwrap_or("")).unwrap();
		assert_eq!(front["name"], "drift");
		assert_eq!(
			front["description"],
			"Review a week of merged changes for design drift: a second way of doing what already has one, patterns that disagree, broken layering, a rule or lesson contradicted. Not bugs."
		);
	}
}
