//! The weekly big-picture review of a project (0470, 0583): everything merged into trunk in the past 7 days, read by
//! Jocasta's drift agent in a headless omp, one at a time per project (SVC-12).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::Datelike;
use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use regex::Regex;

use crate::frontier::server::context::{Link, ServiceContext};
use crate::frontier::server::mise::{Env, Mise, MiseRunOptions, MiseTaskRun, mise_run_task};
use crate::frontier::server::overlay::ensure_overlay;
use crate::frontier::server::production::production_lines;
use crate::frontier::server::task_file::project_tasks;
use crate::frontier::server::worktrees::git_out;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::install::shipped_config_dir;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::project_dir;
use crate::shared::process::{RunOptions, run_program};
use crate::shared::proctree::{end_processes, process_id_of, processes_in};
use crate::shared::projects::Project;

/// How long the weekly big-picture review may run before it is stopped (0470).
const WEEKLY_REVIEW_MS: i64 = 30 * 60_000;
/// The most one big-picture review may spend, in US dollars (0608): omp's `--max-cost` stops sending model requests at
/// it and exits with `COST_CAP_EXIT_CODE` (omp 19.1.1 `print-mode.ts`).
const WEEKLY_REVIEW_MAX_COST_USD: u32 = 25;
/// How long the project's `scorecard` task may run before the review goes on without it (0616).
const SCORECARD_MS: i64 = 10 * 60_000;
/// The most characters of one scorecard run's output that are kept and shown (0616); the rest is cut.
pub const SCORECARD_MAX_CHARS: usize = 6000;
/// The exit code omp's print mode ends with when `--max-cost` was reached (omp 19.1.1 `PRINT_MODE_COST_CAP_EXIT_CODE`).
const COST_CAP_EXIT_CODE: i32 = 3;
/// git's empty tree: the start of a repo younger than the review's week.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// What a big-picture review covers: trunk a week ago → trunk now, with the count merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Week {
	pub trunk: String,
	pub now: String,
	pub from: String,
	pub range: String,
	pub merged: u32,
}

/// One review's result line (`None` without a week to review), or why it failed; the failure is its message, so every
/// caller of a shared run can have it.
type Review = std::result::Result<Option<String>, String>;
type SharedReview = Shared<BoxFuture<'static, Review>>;

/// What the scorecard adds to a review: the sentences for the agent's prompt, and the text that ends the report.
struct Scorecard {
	prompt: Vec<String>,
	report: String,
}

/// `weekly-review.ts`'s `WeeklyReviews`.
pub struct WeeklyReviews {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	/// Each project's big-picture review under way (SVC-12, 0577): one at a time per project.
	runs: Mutex<HashMap<String, SharedReview>>,
}

impl WeeklyReviews {
	pub fn new(link: Link) -> Self {
		Self { link, runs: Mutex::new(HashMap::new()) }
	}

	fn runs(&self) -> MutexGuard<'_, HashMap<String, SharedReview>> {
		self.runs.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Whether the project's big-picture review is running now.
	pub fn running(&self, project_id: &str) -> bool {
		self.runs().contains_key(project_id)
	}

	/// What a big-picture review would cover (0470): trunk a week ago → trunk now, with the count merged; `None` when
	/// trunk had fewer than two commits in the past 7 days (one change can't drift across changes, and the thread's own
	/// review saw it).
	pub async fn range(&self, project: &Project) -> Result<Option<Week>> {
		let c = self.link.ctx();
		let trunk = if project.git { c.places.trunk(project).await? } else { None };
		let Some(trunk) = trunk else { return Ok(None) };
		let main = project.main_path.as_path();
		let (now, count) = futures_util::future::try_join(
			git_out(main, &["rev-parse", "--short", trunk.as_str()]),
			git_out(main, &["rev-list", "--count", "--since=7.days.ago", trunk.as_str()]),
		)
		.await?;
		let merged = count.and_then(|count| count.parse::<u32>().ok()).unwrap_or(0);
		let Some(now) = now.filter(|now| !now.is_empty()).filter(|_| merged >= 2) else { return Ok(None) };
		// Trunk a week ago; a younger repo is reviewed from its start (git's empty tree).
		let from = git_out(main, &["rev-list", "-1", "--abbrev-commit", "--before=7.days.ago", trunk.as_str()])
			.await?
			.filter(|from| !from.is_empty())
			.unwrap_or_else(|| EMPTY_TREE.to_owned());
		Ok(Some(Week { trunk, range: format!("{from}..{now}"), now, from, merged }))
	}

	/// One read-only big-picture review of a project (0470), one at a time per project (SVC-12): a second caller gets the
	/// running one's result, so neither ends the other's processes. Resolves the result line, or `None` when there is no
	/// week to review ([`WeeklyReviews::range`]).
	pub async fn review(&self, project: &Project) -> Result<Option<String>> {
		let shared = {
			let mut runs = self.runs();
			match runs.get(&project.id) {
				Some(running) => running.clone(),
				None => {
					let (link, owned) = (self.link.clone(), project.clone());
					let run: BoxFuture<'static, Review> = async move {
						let c = link.ctx();
						let done = c.weekly.run(&c, &owned).await.map_err(|error| error.to_string());
						c.weekly.runs().remove(&owned.id);
						done
					}
					.boxed();
					let shared = run.shared();
					runs.insert(project.id.clone(), shared.clone());
					shared
				}
			}
		};
		shared.await.map_err(Error::other)
	}

	/// The big-picture review itself: everything merged into trunk in the past 7 days, by Jocasta's drift agent (0583,
	/// written beside the project's lessons skill, found from the run's folder) against the project's rules and lessons,
	/// looking for design drift across the changes, with the week's production numbers (0621) and scorecard (0616) in
	/// its prompt. omp runs headless (`-p`) in `<data>/projects/<pid>/weekly` with the project's overlay, read-only
	/// tools and the main checkout added, so no session lands where the project pane resumes; `config/headless.yml`
	/// turns omp's async jobs and its advisor off, so the subagent's result comes back before print mode exits.
	async fn run(&self, c: &ServiceContext, project: &Project) -> Result<Option<String>> {
		let Some(week) = self.range(project).await? else { return Ok(None) };
		let main = project.main_path.as_path();
		let dir = project_dir(&project.id).join("weekly");
		fs::create_dir_all(&dir)?;
		let production = self.production(project)?;
		let scorecard = self.scorecard(c, project).await?;
		let overlay = ensure_overlay(&project.id).await?;
		let prompt = review_prompt(project, &week, &production, &scorecard);
		let argv: Vec<String> = c
			.panes
			.pane_omp()
			.iter()
			.filter(|arg| arg.as_str() != "--continue")
			.cloned()
			.chain(["--config".to_owned(), overlay.to_string_lossy().into_owned(), "--config".to_owned()])
			.chain([
				shipped_config_dir(None).join("headless.yml").to_string_lossy().into_owned(),
				"--max-cost".to_owned(),
				WEEKLY_REVIEW_MAX_COST_USD.to_string(),
				"-p".to_owned(),
				"--tools".to_owned(),
				// No `find`: its fuzzy search is off (0451, `find.enabled`), and omp refuses to start when asked for it.
				"read,grep,glob,task".to_owned(),
				"--add-dir".to_owned(),
				main.to_string_lossy().into_owned(),
				prompt,
			])
			.collect();
		// The service's own env with the panes' (omp needs its `PI_*`), then mise's.
		let mut base: Vec<(String, String)> = env::vars()
			.into_iter()
			.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
			.collect();
		base.extend(c.panes.pane_env().iter().cloned());
		let env: Env = c.panes.with_mise(&project.id, main, &base).await.into_iter().collect();
		let run = run_program(
			&argv,
			RunOptions {
				cwd: Some(dir.clone()),
				env: Some(env),
				timeout_ms: Some(c.options.weekly_review_ms.unwrap_or(WEEKLY_REVIEW_MS)),
				..RunOptions::default()
			},
		)
		.await?;
		// Workers omp started (a memory embedding server, the tiny model) can outlive print mode and keep this folder
		// busy, as panes' did (0486); whatever still works in this run's own folder is this run's.
		let found: Vec<_> = processes_in(&dir).into_iter().filter_map(|found| process_id_of(found.pid)).collect();
		end_processes(&found, &[]).await;
		let Some(drift) = drift_of(&run.stdout) else {
			let why = if run.timed_out || run.signal.is_some() {
				"it ran out of time".to_owned()
			} else if run.code == Some(COST_CAP_EXIT_CODE) {
				format!("it reached its ${WEEKLY_REVIEW_MAX_COST_USD} cost cap")
			} else {
				let said = if run.stderr.is_empty() { &run.stdout } else { &run.stderr };
				let last = said.trim().split('\n').next_back().unwrap_or_default();
				format!("omp exited {}: {last}", run.code.map_or_else(|| "null".to_owned(), |code| code.to_string()))
			};
			return Err(Error::other(format!("the big-picture review of {} gave no result ({why})", week.range)));
		};
		let plural = if week.merged == 1 { "" } else { "s" };
		keep_drift(&project.id, &drift);
		Ok(Some(format!("{} ({} commit{plural}): {drift}{}", week.range, week.merged, scorecard.report)))
	}

	/// The week's production numbers (0621), from the project's `thread.json` records with no model call, kept for the
	/// week (`<data>/projects/<pid>/production/<ISO week>.txt`). The prompt sentences for the agent, with this week's
	/// lines and the last kept week's.
	fn production(&self, project: &Project) -> Result<Vec<String>> {
		let now = chrono::Utc::now().timestamp_millis();
		let text = production_lines(&project_tasks(&project.id), now);
		let kept = keep_for_week(&project_dir(&project.id).join("production"), &text)?;
		Ok(vec![
			"The project's production numbers for the past 7 days, from Jocasta's thread records (lines joined with ` | `);"
				.to_owned(),
			format!(
				"give them to the drift subagent in its task as context, not as targets. This week: {}",
				one_line(&text)
			),
			match kept {
				None => "There is no earlier week of them.".to_owned(),
				Some((before, last)) => format!("Last week ({before}): {}", one_line(&last)),
			},
		])
	}

	/// The project's scorecard for this review (0616): its trusted mise task `scorecard`, run in the main checkout with
	/// the checks' env before the agent starts, its output cut to `SCORECARD_MAX_CHARS` and kept for the week
	/// (`<data>/projects/<pid>/scorecard/<ISO week>.txt`, the output as is). The prompt sentences for the agent, with this
	/// week's and the last kept week's output, and the text that ends the report: the lines that changed, or one line
	/// on why there is no scorecard. Both are empty for a project without the task; a failing, overlong or untrusted
	/// task costs the review nothing but the scorecard.
	async fn scorecard(&self, c: &ServiceContext, project: &Project) -> Result<Scorecard> {
		let options = &c.options;
		let check_env = c.panes.check_env();
		let mise = match &options.mise {
			Some(None) => Mise::Absent,
			Some(Some(argv)) => Mise::Argv(argv),
			None => Mise::OnPath,
		};
		let run = mise_run_task(
			&project.id,
			&project.main_path,
			"scorecard",
			MiseRunOptions { timeout_ms: options.scorecard_ms.unwrap_or(SCORECARD_MS), env: Some(&check_env), mise },
		)
		.await;
		let without = |why: &str| Scorecard { prompt: Vec::new(), report: format!(" · no scorecard: {why}") };
		let output = match run {
			None => return Ok(Scorecard { prompt: Vec::new(), report: String::new() }),
			Some(MiseTaskRun::Skipped(why)) => return Ok(without(&why)),
			Some(MiseTaskRun::Failed(failed)) => return Ok(without(&format!("the scorecard task {failed}"))),
			Some(MiseTaskRun::Output(output)) => output,
		};
		let text = capped(output.trim());
		if text.is_empty() {
			return Ok(without("the scorecard task printed nothing"));
		}
		let kept = keep_for_week(&project_dir(&project.id).join("scorecard"), &text)?;
		let (compare, report) = match &kept {
			None => (
				"There is no earlier week to compare.".to_owned(),
				" · scorecard: no earlier week to compare".to_owned(),
			),
			Some((before, last)) => {
				(format!("Last week ({before}): {}", one_line(last)), format!(" · scorecard: {}", changes(last, &text)))
			}
		};
		Ok(Scorecard {
			prompt: vec![
				"The project's scorecard is its mise task `scorecard`, run just now (lines joined with ` | `); give it to the"
					.to_owned(),
				"drift subagent in its task. These are measurements to read as questions, not targets: a number that moved is"
					.to_owned(),
				format!("worth a look at the code behind it. This week: {}", one_line(&text)),
				compare,
			],
			report,
		})
	}
}

/// The prompt the drift agent is given (0470, 0583, 0616, 0621).
fn review_prompt(project: &Project, week: &Week, production: &[String], scorecard: &Scorecard) -> String {
	let Week { trunk, now, from, range, .. } = week;
	let main = project.main_path.as_path();
	let slash = |path: &Path| path.to_string_lossy().replace('\\', "/");
	let rules: Vec<String> = ["AGENTS.md", "AGENTS.local.md"]
		.iter()
		.map(|name| main.join(name))
		.filter(|file| file.exists())
		.map(|file| slash(&file))
		.collect();
	let log_range = if from == EMPTY_TREE { now } else { range };
	let rules_said =
		if rules.is_empty() { String::new() } else { format!("the project's rules ({}) and ", rules.join(", ")) };
	let mut parts = vec![
		format!("Big-picture review of {}: everything merged into {trunk} in the past 7 days,", project.name),
		format!(
			"`git -C \"{}\" diff {range}` (its commits: `git -C \"{}\" log --oneline {log_range}`).",
			slash(main),
			slash(main)
		),
		"This review is read-only: change no file, commit nothing. Run the drift subagent on that range. In its task"
			.to_owned(),
		format!("give it {rules_said}its lessons in skill://lessons"),
		format!("and the design rubric ({}),", slash(&shipped_config_dir(None).join("design-rubric.md"))),
		"and ask it for design drift across these changes: each may be reasonable alone while together they pull the design"
			.to_owned(),
		"apart (a second way of doing what already has one, patterns that disagree, broken layering, a rule or lesson"
			.to_owned(),
		"contradicted). Not line-level bugs. Tell it: report only material findings; prefer one strong finding over"
			.to_owned(),
		"several weak ones.".to_owned(),
	];
	parts.extend(production.iter().cloned());
	parts.extend(scorecard.prompt.iter().cloned());
	parts.push("End your reply with one line on its own: `DRIFT: <one sentence: no drift found, or the".to_owned());
	parts.push("strongest finding and where>`.".to_owned());
	parts.join(" ")
}

/// The last `DRIFT:` line of the agent's reply, trimmed.
fn drift_of(out: &str) -> Option<String> {
	let pattern = Regex::new(r"(?im)^[\s*_`>-]*DRIFT:\s*(.+?)[`*_\s]*$").ok()?;
	let last = pattern.captures_iter(out).last()?;
	let drift = last.get(1)?.as_str().trim();
	(!drift.is_empty()).then(|| drift.to_owned())
}

/// Keeps `text` as this ISO week's `<week>.txt` in `dir`, replacing an earlier run of the same week, and gives back the
/// last earlier week's name and text, when there is one.
fn keep_for_week(dir: &Path, text: &str) -> Result<Option<(String, String)>> {
	let week = iso_week();
	fs::create_dir_all(dir)?;
	let mut names = Vec::new();
	for entry in fs::read_dir(dir)? {
		let name = entry?.file_name().to_string_lossy().into_owned();
		let name = name.strip_suffix(".txt").unwrap_or(&name).to_owned();
		if name < week {
			names.push(name);
		}
	}
	names.sort();
	let last = match names.pop() {
		Some(before) => Some((before.clone(), fs::read_to_string(dir.join(format!("{before}.txt")))?)),
		None => None,
	};
	write_atomically(&dir.join(format!("{week}.txt")), text, None)?;
	Ok(last)
}

/// Keeps the drift agent's one line as this ISO week's `<week>.txt` in the project's `drift` folder, for the weekly brief
/// (0680) to show; a failed write is logged, never the review's failure.
fn keep_drift(project_id: &str, drift: &str) {
	let dir = project_dir(project_id).join("drift");
	let kept = fs::create_dir_all(&dir)
		.map_err(Error::from)
		.and_then(|()| write_atomically(&dir.join(format!("{}.txt", iso_week())), format!("{drift}\n"), None));
	if let Err(error) = kept {
		log(LogLevel::Warn, "weekly", &format!("the drift review of {project_id} wasn't kept: {error}"), None);
	}
}

/// Lines as one line for the prompt (` | ` between them): omp's argv carries no newline.
fn one_line(text: &str) -> String {
	text.split('\n').map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" | ")
}

/// `text` cut to `SCORECARD_MAX_CHARS` (counted in UTF-16 units, as the TypeScript's `length` does), with a last line
/// that says it was cut.
fn capped(text: &str) -> String {
	if text.encode_utf16().count() <= SCORECARD_MAX_CHARS {
		return text.to_owned();
	}
	let mut units = 0;
	let kept: String = text
		.chars()
		.take_while(|ch| {
			units += ch.len_utf16();
			units <= SCORECARD_MAX_CHARS
		})
		.collect();
	format!("{kept}\n[cut at {SCORECARD_MAX_CHARS} characters]")
}

/// The ISO 8601 week of now in local time, as `2026-W46`: the key of a week's kept scorecard and production numbers.
fn iso_week() -> String {
	let week = chrono::Local::now().date_naive().iso_week();
	format!("{}-W{:02}", week.year(), week.week())
}

/// The lines of `now` that differ from `before`, matched by their label (the text before the first `: `): `label: was →
/// is` for a line both weeks have, the line itself for a new one; "no line changed" when none did.
fn changes(before: &str, now: &str) -> String {
	let label = |line: &str| line.split(": ").next().unwrap_or(line).to_owned();
	let was: HashMap<String, &str> = before.split('\n').map(|line| (label(line), line)).collect();
	let mut moved = Vec::new();
	for line in now.split('\n') {
		let name = label(line);
		let earlier = was.get(&name).copied();
		if earlier == Some(line) {
			continue;
		}
		moved.push(match earlier {
			None => line.to_owned(),
			Some(earlier) => format!("{earlier} → {}", line.get(name.len() + 2..).unwrap_or_default()),
		});
	}
	if moved.is_empty() {
		"no line changed since last week".to_owned()
	} else {
		format!("changed since last week: {}", moved.join("; "))
	}
}

#[cfg(test)]
#[path = "weekly_review_tests.rs"]
mod tests;
