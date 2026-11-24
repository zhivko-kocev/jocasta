//! The weekly brief (0680): `weekly-<yyyy>-W<ww>`, built by code when the weekly review is due and again as each project's
//! drift review answers. Production numbers (0621) and the ledger's cost per project, the journal's numbers for the week
//! (0688), what each check cost, the week's stuck and over-budget threads, leftover worktrees, and the scorecard and
//! drift review each project kept. Nothing here asks a model.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{Brief, Fact, ListRow, Section};
use crate::frontier::brief::store;
use crate::frontier::server::check_costs::{CheckCost, CheckCosts};
use crate::frontier::server::checks::checks_dir;
use crate::frontier::server::context::ServiceContext;
use crate::frontier::server::day_briefs::weekly_id_of;
use crate::frontier::server::journal::JOURNAL_FILE;
use crate::frontier::server::production::{WeeklyNumbers, production_lines, weekly_numbers};
use crate::frontier::server::task_file::{TaskRecord, project_tasks};
use crate::shared::day::{DayRef, day_label, hhmm, weekday};
use crate::shared::errors::{Error, Result};
use crate::shared::paths::{data_dir, project_dir};
use crate::shared::projects::Project;

const WEEK_MS: i64 = 7 * 24 * 3_600_000;

/// What one project adds: its production lines and its spend over the week.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectWeek {
	pub name: String,
	/// 0621's lines, joined with ` · `.
	pub production: String,
	/// The ledger's cost of the threads open at any time in the week; none when none had one read.
	pub cost: Option<f64>,
	/// The kept scorecard's first line and file, when this week's was kept.
	pub scorecard: Option<(String, String)>,
	/// The drift review's one line, when this week's came.
	pub drift: Option<String>,
}

/// What the weekly brief is built from.
pub struct WeeklyInput {
	pub id: String,
	pub monday: String,
	pub now: i64,
	pub projects: Vec<ProjectWeek>,
	pub numbers: WeeklyNumbers,
	pub check_costs: Vec<Fact>,
	pub trouble: Vec<ListRow>,
	pub leftovers: Vec<ListRow>,
}

/// The brief, its flows laid out (it has none).
pub fn build(input: &WeeklyInput) -> Brief {
	let none = |rows: Vec<ListRow>, said: &str| {
		if rows.is_empty() { vec![ListRow { text: said.to_owned(), url: None, note: None }] } else { rows }
	};
	let mut production: Vec<Fact> = Vec::new();
	for project in &input.projects {
		production.push(Fact { label: project.name.clone(), value: project.production.clone(), url: None });
		let cost = project.cost.map_or_else(|| "not known".to_owned(), |cost| format!("${cost:.2}"));
		production.push(Fact { label: format!("{} cost", project.name), value: cost, url: None });
	}
	if production.is_empty() {
		production.push(Fact { label: "Projects".to_owned(), value: "no thread records yet".to_owned(), url: None });
	}
	let journal = input.numbers.rows().into_iter().map(|(label, value)| Fact { label, value, url: None }).collect();
	let costs = if input.check_costs.is_empty() {
		vec![Fact { label: "Checks".to_owned(), value: "no cost recorded yet".to_owned(), url: None }]
	} else {
		input.check_costs.clone()
	};
	let mut kept: Vec<ListRow> = Vec::new();
	for project in &input.projects {
		if let Some((first, file)) = &project.scorecard {
			kept.push(ListRow {
				text: format!("Scorecard, {}: {first}", project.name),
				url: None,
				note: Some(file.clone()),
			});
		}
		if let Some(drift) = &project.drift {
			kept.push(ListRow { text: format!("Drift review, {}: {drift}", project.name), url: None, note: None });
		}
	}
	let sections = vec![
		Section::Facts { heading: Some("Production and cost".to_owned()), rows: production },
		Section::Facts { heading: Some("The week in the journal".to_owned()), rows: journal },
		Section::Facts { heading: Some("What each check cost".to_owned()), rows: costs },
		Section::List {
			heading: Some("Stuck and over budget".to_owned()),
			rows: none(input.trouble.clone(), "No thread was stuck or over budget."),
		},
		Section::List {
			heading: Some("Leftover worktrees".to_owned()),
			rows: none(input.leftovers.clone(), "No worktree is left on disk."),
		},
		Section::List {
			heading: Some("Scorecard and drift review".to_owned()),
			rows: none(kept, "No scorecard or drift review is kept for this week yet."),
		},
	];
	Brief {
		id: input.id.clone(),
		kind: "weekly".to_owned(),
		title: "Weekly brief".to_owned(),
		subtitle: format!("Week of {}", day_label(DayRef::Day(&input.monday), true)),
		generated_at_ms: input.now,
		sections,
	}
}

/// The journal's `stuck` and `over_budget` lines of the window as rows, oldest first: `acme-api/Retry: stuck: quick
/// failed 3×` and when it happened. `names` maps `<project id>/<task id>` to the project's and the thread's names.
pub fn trouble_rows<S: AsRef<str>>(
	lines: impl IntoIterator<Item = S>,
	from_ms: i64,
	to_ms: i64,
	names: &HashMap<String, (String, String)>,
) -> Vec<ListRow> {
	let mut rows = Vec::new();
	for line in lines {
		let Ok(value) = serde_json::from_str::<Value>(line.as_ref()) else { continue };
		let Some(at) = value.get("at").and_then(Value::as_i64).filter(|at| (from_ms..to_ms).contains(at)) else {
			continue;
		};
		let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default();
		let what = match text("kind") {
			"stuck" => format!("stuck: {} failed 3×", text("gate")),
			"over_budget" => {
				let number = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or_default();
				format!("over budget: ${:.2} of ${}", number("spent"), number("budget"))
			}
			_ => continue,
		};
		let key = format!("{}/{}", text("project"), text("thread"));
		let who = names.get(&key).map_or_else(|| key.clone(), |(project, title)| format!("{project}/{title}"));
		rows.push(ListRow {
			text: format!("{who}: {what}"),
			url: None,
			note: Some(format!("{} {}", weekday(at), hhmm(at))),
		});
	}
	rows
}

/// A project's week: 0621's production lines, the cost of the threads open in the window, what the project kept.
fn project_week(project: &Project, records: &[TaskRecord], iso_week: &str, now: i64) -> ProjectWeek {
	let open_in_week = |record: &&TaskRecord| {
		record.created_at < now && record.close.as_ref().is_none_or(|close| close.at >= now - WEEK_MS)
	};
	let spends: Vec<f64> = records.iter().filter(open_in_week).filter_map(|record| record.spend).collect();
	let kept =
		|dir: &str| std::fs::read_to_string(project_dir(&project.id).join(dir).join(format!("{iso_week}.txt"))).ok();
	let scorecard = kept("scorecard").map(|text| {
		let first = text.lines().next().unwrap_or_default().to_owned();
		let file = project_dir(&project.id).join("scorecard").join(format!("{iso_week}.txt"));
		(first, file.display().to_string())
	});
	ProjectWeek {
		name: project.name.clone(),
		production: production_lines(records, now).lines().collect::<Vec<_>>().join(" · "),
		cost: (!spends.is_empty()).then(|| spends.iter().sum()),
		scorecard,
		drift: kept("drift").map(|text| text.trim().to_owned()).filter(|text| !text.is_empty()),
	}
}

/// `<project folder name> · <command>` for a check-cost key (`<project folder> :: <command>`).
fn check_label(key: &str) -> String {
	let (dir, command) = key.split_once(" :: ").unwrap_or(("", key));
	let folder = Path::new(dir).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
	if folder.is_empty() { command.to_owned() } else { format!("{folder} · {command}") }
}

/// The most check-cost rows the brief carries.
const CHECK_ROWS: usize = 30;

/// What each check cost as rows, the biggest peak first: its peak memory, the cores it used and how often it ran.
pub fn check_rows(all: Vec<(String, CheckCost)>) -> Vec<Fact> {
	all.into_iter()
		.take(CHECK_ROWS)
		.map(|(key, cost)| Fact {
			label: check_label(&key),
			value: format!(
				"peak {} MB · {:.1} cores · {} run{}",
				cost.peak_rss_bytes / 1_048_576,
				cost.avg_cores,
				cost.runs,
				if cost.runs == 1 { "" } else { "s" }
			),
			url: None,
		})
		.collect()
}

/// Builds the weekly brief of the week that began on `monday`; a failure is said in the log and nowhere else, since a
/// brief is never a reason to fail the review.
pub async fn build_weekly(c: &Arc<ServiceContext>, monday: &str) {
	if !c.options.auto_briefs_on() {
		return;
	}
	if let Err(error) = refresh(c, monday).await {
		c.warn(format!("the weekly brief wasn't built: {error}"));
	}
}

async fn refresh(c: &Arc<ServiceContext>, monday: &str) -> Result<()> {
	let id = weekly_id_of(monday).ok_or_else(|| Error::other(format!("`{monday}` isn't a date")))?;
	let iso_week = id.trim_start_matches("weekly-").to_owned();
	let now = c.day.now_ms();
	let (from, to) = (now - WEEK_MS, now + 1);
	let journal = std::fs::read_to_string(data_dir().join(JOURNAL_FILE)).unwrap_or_default();
	let mut projects = Vec::new();
	let mut names: HashMap<String, (String, String)> = HashMap::new();
	for project in c.places.listed() {
		let records = project_tasks(&project.id);
		for record in &records {
			names.insert(format!("{}/{}", project.id, record.id), (project.name.clone(), record.title.clone()));
		}
		if !records.is_empty() {
			projects.push(project_week(&project, &records, &iso_week, now));
		}
	}
	let leftovers = c
		.status
		.leftovers()
		.await?
		.into_iter()
		.map(|leftover| {
			let folder =
				leftover.folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
			let branch = leftover.branch.map(|branch| format!("{branch} · ")).unwrap_or_default();
			ListRow {
				text: format!("{}/{folder}", leftover.project),
				url: None,
				note: Some(format!("{branch}{} ahead · {} changed", leftover.ahead, leftover.dirty)),
			}
		})
		.collect();
	let input = WeeklyInput {
		id,
		monday: monday.to_owned(),
		now,
		projects,
		numbers: weekly_numbers(journal.lines(), from, to),
		check_costs: check_rows(CheckCosts::load(checks_dir(&data_dir()).join("costs.json")).all()),
		trouble: trouble_rows(journal.lines(), from, to, &names),
		leftovers,
	};
	let mut brief = build(&input);
	lay_out(&mut brief).map_err(Error::other)?;
	store::save(&brief)?;
	// The dock's row appears with it.
	c.day.push_today()
}

#[cfg(test)]
#[path = "weekly_brief_tests.rs"]
mod tests;
