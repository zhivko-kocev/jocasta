//! Steps C and D of the settings trials (fork plan section 4), and the model study: real trials of the arms step B
//! found live, on Opus 5.5 at high over the base (A1, the advisor off), each trial capped by omp's `--max-cost`. His
//! subscription's session windows are the real limit (the pacer waits them out); the dollar ceiling only stops a
//! runaway (his word of 2026-11-18: no budget, the most optimal result).
//!
//! - `screen` (C): the base on every case [`BASE_REPS`] times; then successive halving: every arm on [`FIRST_CASES`]
//!   cases, the best half on the other 8, the best third on all 12 a second time. Arms go round-robin within a case,
//!   the order rotated each case. An arm that fails a case the base passed every time is out.
//! - `confirm` (D): the winners, C arms whose bootstrap interval over the cases puts their cost or their wall time
//!   below the base's; the best of each subsystem (the key's first part) combined, run 12 × [`CONFIRM_REPS`]
//!   interleaved with the base and judged by compare's keep rule. A combination no better than the best single arm
//!   gives way to adding the winners one at a time, each kept only when it improves.
//! - `models`: the pane's model and thinking level and the subagents' model ([`MODEL_ARMS`]), each on all 12 cases
//!   [`MODEL_REPS`] times interleaved with the base; reported, never adopted from a score (the roles are 0590's and
//!   0591's).
//!
//! Every trial is a child `jocasta-bench evals settings trial` in its own Job Object under the memory floor
//! (`evals/watch.rs`), with its row appended to `<run>/trials.jsonl`; a rerun skips the trials already there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jocasta::shared::errors::{Error, Result};
use jocasta::shared::jsonl::append_jsonl;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::cases::load_all_cases;
use super::compare::{Verdict, compare_rows};
use super::pacer::{Pacer, RealPacerDeps, create_pacer};
use super::replay::ReplayRow;
use super::run::launcher_of;
use super::runner::{ResultRow, TrialOptions, process_env, run_trial};
use super::settings::{Arm, Class, arm_delta, arm_name, base_variant, hold_omp_version, parse_arm, read_settings};
use super::variants::deep_merge;
use super::watch::{run_watched, wait_for_start_floor, wait_while_paused};
use crate::lib::args::REPO_ROOT;
use crate::lib::output::read_version;
use crate::lib::stats::{BootstrapOptions, Pair, paired_bootstrap};

/// The spend that stops every step, in USD: a brake against a runaway, not a budget.
pub const BUDGET: f64 = 600.0;
/// omp's own ceiling per trial (`--max-cost`): room for Fable 5.2 at xhigh, a stop for a trial that loops.
pub const TRIAL_CAP_USD: f64 = 3.0;
/// The most arms step C runs; the rest are logged as live, not afforded.
pub const MOST_ARMS: usize = 90;
const BASE_REPS: u64 = 5;
const CONFIRM_REPS: u64 = 5;
const MODEL_REPS: u64 = 3;
const FIRST_CASES: usize = 4;
const TRIAL_LIMIT: Duration = Duration::from_secs(15 * 60);

/// An arm's key for the pane's model (`--model`); the model study's own, never an omp setting.
const MODEL_KEY: &str = "model";
/// An arm's key for the pane's thinking level (`--thinking`, the last one winning over the base's `high`).
const THINKING_KEY: &str = "thinking";

/// The model study's arms over the base (Opus 5.5 at high, subagents on Sonnet 5.5 at medium): the Anthropic models
/// his two accounts reach, at the levels a pane or a subagent would run them.
pub const MODEL_ARMS: &[&str] = &[
	r#"thinking="low""#,
	r#"thinking="medium""#,
	r#"thinking="xhigh""#,
	r#"thinking="auto""#,
	r#"model="anthropic/claude-fable-5-2" & thinking="medium""#,
	r#"model="anthropic/claude-fable-5-2" & thinking="high""#,
	r#"model="anthropic/claude-fable-5-2" & thinking="xhigh""#,
	r#"model="anthropic/claude-sonnet-5-5" & thinking="medium""#,
	r#"model="anthropic/claude-sonnet-5-5" & thinking="high""#,
	r#"model="anthropic/claude-haiku-5-5" & thinking="high""#,
	r#"modelRoles.task="anthropic/claude-haiku-5-5:low""#,
	r#"modelRoles.task="anthropic/claude-opus-5-5:medium""#,
];

/// One trial's row in `trials.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
	pub arm: String,
	/// `base`, `r1`, `r2`, `r3`, `confirm`, `greedy`.
	pub round: String,
	pub case: String,
	pub rep: u64,
	pub passed: bool,
	pub cost: f64,
	pub tokens: f64,
	pub wall_ms: f64,
	#[serde(default)]
	pub peak_tree_bytes: Option<u64>,
	#[serde(default)]
	pub min_available_bytes: Option<u64>,
	/// Why the trial doesn't count: `memory` (the floor ended it), `crash` (the child left no row).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub infra: Option<String>,
	/// The child's own row, for compare's keep rule.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub row: Option<ResultRow>,
}

fn mean(values: &[f64]) -> f64 {
	if values.is_empty() { f64::NAN } else { values.iter().sum::<f64>() / values.len() as f64 }
}

/// The trials that count of `arm` (by name) in the given rounds, per case.
pub fn by_case<'a>(trials: &'a [Trial], arm: &str, rounds: &[&str]) -> BTreeMap<&'a str, Vec<&'a Trial>> {
	let mut cases: BTreeMap<&str, Vec<&Trial>> = BTreeMap::new();
	for trial in trials.iter().filter(|t| t.arm == arm && t.infra.is_none() && rounds.contains(&t.round.as_str())) {
		cases.entry(trial.case.as_str()).or_default().push(trial);
	}
	cases
}

/// The cases the base passed every time it ran them.
pub fn base_always_passes<'a>(base: &BTreeMap<&'a str, Vec<&Trial>>) -> HashSet<&'a str> {
	base.iter().filter(|(_, runs)| !runs.is_empty() && runs.iter().all(|t| t.passed)).map(|(case, _)| *case).collect()
}

/// How an arm stands against the base over the cases it ran.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Standing {
	pub arm: String,
	pub cases: usize,
	/// It failed a case the base always passed.
	pub out: bool,
	/// Mean over cases of the arm's mean ÷ the base's mean.
	pub cost_ratio: f64,
	pub wall_ratio: f64,
	/// The paired bootstrap's 95% interval of (arm − base) per case, in USD and ms.
	pub cost_interval: (f64, f64),
	pub wall_interval: (f64, f64),
}

impl Standing {
	/// The rank: the better of its two ratios; lower is better.
	pub fn score(&self) -> f64 {
		self.cost_ratio.min(self.wall_ratio)
	}

	/// A winner (step D): its interval excludes zero on the cheap or the fast side, at equal pass.
	pub fn wins(&self) -> bool {
		!self.out && (self.cost_interval.1 < 0.0 || self.wall_interval.1 < 0.0)
	}
}

/// `arm`'s standing against the base's trials of `base_rounds`.
pub fn standing(trials: &[Trial], arm: &str, rounds: &[&str], base_rounds: &[&str]) -> Standing {
	let base = by_case(trials, "base", base_rounds);
	let always = base_always_passes(&base);
	let own = by_case(trials, arm, rounds);
	let mut cost_ratios = Vec::new();
	let mut wall_ratios = Vec::new();
	let mut cost_pairs = Vec::new();
	let mut wall_pairs = Vec::new();
	let mut out = false;
	for (case, runs) in &own {
		if always.contains(case) && runs.iter().any(|t| !t.passed) {
			out = true;
		}
		let Some(base_runs) = base.get(case) else { continue };
		let (base_cost, base_wall) = (
			mean(&base_runs.iter().map(|t| t.cost).collect::<Vec<_>>()),
			mean(&base_runs.iter().map(|t| t.wall_ms).collect::<Vec<_>>()),
		);
		let (cost, wall) = (
			mean(&runs.iter().map(|t| t.cost).collect::<Vec<_>>()),
			mean(&runs.iter().map(|t| t.wall_ms).collect::<Vec<_>>()),
		);
		if base_cost > 0.0 {
			cost_ratios.push(cost / base_cost);
		}
		if base_wall > 0.0 {
			wall_ratios.push(wall / base_wall);
		}
		cost_pairs.push(Pair { a: base_cost, b: cost });
		wall_pairs.push(Pair { a: base_wall, b: wall });
	}
	let interval = |pairs: &[Pair]| {
		if pairs.len() < 2 {
			return (f64::NEG_INFINITY, f64::INFINITY);
		}
		let found = paired_bootstrap(pairs, BootstrapOptions::default());
		(found.ci95.low, found.ci95.high)
	};
	Standing {
		arm: arm.to_owned(),
		cases: own.len(),
		out,
		cost_ratio: mean(&cost_ratios),
		wall_ratio: mean(&wall_ratios),
		cost_interval: interval(&cost_pairs),
		wall_interval: interval(&wall_pairs),
	}
}

/// The best `share` (rounded up) of the arms still in, by score.
pub fn best_share(standings: &[Standing], divisor: usize) -> Vec<String> {
	let mut alive: Vec<&Standing> = standings.iter().filter(|s| !s.out && !s.score().is_nan()).collect();
	alive.sort_by(|a, b| a.score().total_cmp(&b.score()));
	let keep = alive.len().div_ceil(divisor);
	alive.into_iter().take(keep).map(|s| s.arm.clone()).collect()
}

/// The arms in the order a case runs them: the list turned by the case's place, so none always goes first.
pub fn rotated<T: Clone>(items: &[T], by: usize) -> Vec<T> {
	if items.is_empty() {
		return Vec::new();
	}
	let at = by % items.len();
	items[at..].iter().chain(&items[..at]).cloned().collect()
}

/// How far step B saw an arm move the run: its request bytes' and wall time's distance from the base's means.
pub fn replay_effect(row: &ReplayRow, base_bytes: f64, base_wall: f64) -> f64 {
	let bytes = if base_bytes > 0.0 { (row.request_bytes as f64 / base_bytes - 1.0).abs() } else { 0.0 };
	let wall = if base_wall > 0.0 { (row.wall_ms as f64 / base_wall - 1.0).abs() } else { 0.0 };
	bytes + wall + if row.because.iter().any(|b| b == "exit") { 1.0 } else { 0.0 }
}

/// The live arms step B left, the [`MOST_ARMS`] largest effects first; the rest are returned apart. A row's class is
/// taken from step A's current `classes` (key → class), never from the row's copy, so a key classed out since the
/// replay ran is not tried.
pub fn live_arms(rows: &[ReplayRow], classes: &HashMap<String, Class>) -> (Vec<(String, Class)>, Vec<String>) {
	let bases: Vec<&ReplayRow> = rows.iter().filter(|r| r.verdict == "base").collect();
	let base_bytes = mean(&bases.iter().map(|r| r.request_bytes as f64).collect::<Vec<_>>());
	let base_wall = mean(&bases.iter().map(|r| r.wall_ms as f64).collect::<Vec<_>>());
	let class_of = |row: &ReplayRow| row.key.as_ref().and_then(|key| classes.get(key)).copied();
	let mut live: Vec<(&ReplayRow, Class, f64)> = rows
		.iter()
		.filter(|r| r.verdict == "live")
		.filter_map(|r| class_of(r).filter(|class| matches!(class, Class::C | Class::Q)).map(|class| (r, class)))
		.map(|(r, class)| (r, class, replay_effect(r, base_bytes, base_wall)))
		.collect();
	live.sort_by(|a, b| b.2.total_cmp(&a.2));
	let mut seen = HashSet::new();
	live.retain(|(r, _, _)| seen.insert(r.arm.clone()));
	let rest = live.iter().skip(MOST_ARMS).map(|(r, _, _)| r.arm.clone()).collect();
	let chosen = live.into_iter().take(MOST_ARMS).map(|(r, class, _)| (r.arm.clone(), class)).collect();
	(chosen, rest)
}

/// The run's trials so far.
pub fn read_trials(dir: &Path) -> Vec<Trial> {
	fs::read_to_string(dir.join("trials.jsonl"))
		.unwrap_or_default()
		.lines()
		.filter_map(|line| serde_json::from_str(line).ok())
		.collect()
}

/// Infra trials in a row that stop the run: the trials keep failing for a reason of the harness, not the arm's.
const INFRA_STREAK: u32 = 3;

/// What a trial counts toward the budget: its measured cost, or omp's whole cap when it left no row to measure (its
/// omp may have spent up to the cap before the child failed or the floor ended it).
pub fn charged(trial: &Trial) -> f64 {
	if trial.infra.is_some() { TRIAL_CAP_USD } else { trial.cost }
}

/// What runs the trials: the run folder, its rows, the spend so far and its ceiling.
struct Ladder {
	dir: PathBuf,
	trials: Vec<Trial>,
	done: HashSet<(String, String, String, u64)>,
	exe: String,
	/// Run here, outside the trial's time limit and memory watch: a full quota window waits as long as it must.
	pacer: Pacer,
	infra_streak: u32,
	/// No trial starts that could take the spend past it.
	ceiling: f64,
}

impl Ladder {
	fn open(dir: &Path, ceiling: f64) -> Result<Self> {
		let trials = read_trials(dir);
		let done = trials
			.iter()
			.filter(|t| t.infra.is_none())
			.map(|t| (t.arm.clone(), t.round.clone(), t.case.clone(), t.rep))
			.collect();
		let exe = std::env::current_exe()?.display().to_string();
		let pacer = create_pacer(Arc::new(RealPacerDeps), Arc::new(Mutex::new(Vec::new())));
		Ok(Self { dir: dir.to_owned(), trials, done, exe, pacer, infra_streak: 0, ceiling })
	}

	fn spent(&self) -> f64 {
		self.trials.iter().map(charged).sum()
	}

	/// One trial, unless it ran already. `Ok(false)` when the spend has reached the ceiling.
	async fn trial(&mut self, arm: &str, round: &str, case: &str, rep: u64) -> Result<bool> {
		if self.done.contains(&(arm.to_owned(), round.to_owned(), case.to_owned(), rep)) {
			return Ok(true);
		}
		for attempt in 0..2 {
			if self.spent() + TRIAL_CAP_USD > self.ceiling {
				return Ok(false);
			}
			wait_while_paused(&self.dir).await;
			self.pacer.pace().await;
			wait_for_start_floor().await;
			let out = self.dir.join("trial-row.json");
			let _ = fs::remove_file(&out);
			let argv: Vec<String> = [
				self.exe.as_str(),
				"evals",
				"settings",
				"trial",
				"--dir",
				&self.dir.display().to_string(),
				"--arm",
				arm,
				"--case",
				case,
				"--rep",
				&rep.to_string(),
				"--out",
				&out.display().to_string(),
			]
			.iter()
			.map(|s| (*s).to_owned())
			.collect();
			let watched = run_watched(&argv, &self.dir, &process_env(), TRIAL_LIMIT).await?;
			let row: Option<ResultRow> =
				fs::read_to_string(&out).ok().and_then(|text| serde_json::from_str(&text).ok());
			let infra = if watched.memory_floor {
				Some("memory".to_owned())
			} else if row.is_none() {
				Some("crash".to_owned())
			} else {
				None
			};
			let trial = Trial {
				arm: arm.to_owned(),
				round: round.to_owned(),
				case: case.to_owned(),
				rep,
				passed: row.as_ref().is_some_and(|r| r.passed),
				cost: row.as_ref().map_or(0.0, |r| r.total_usage.cost),
				tokens: row.as_ref().map_or(0.0, |r| r.total_usage.total_tokens),
				wall_ms: row.as_ref().map_or(watched.wall_ms as f64, |r| r.wall_ms),
				peak_tree_bytes: watched.peak_tree_bytes,
				min_available_bytes: watched.min_available_bytes,
				infra: infra.clone(),
				row: row.clone(),
			};
			append_jsonl(&self.dir.join("trials.jsonl"), &trial)?;
			println!(
				"${:.2} {round} {} {case} rep {rep}: {}{}",
				self.spent() + charged(&trial),
				arm,
				if trial.passed { "pass" } else { "FAIL" },
				infra.as_deref().map(|why| format!(" (infra: {why})")).unwrap_or_default()
			);
			if infra.is_some() && attempt == 0 {
				eprintln!("{}", watched.stderr.lines().rev().take(5).collect::<Vec<_>>().join("\n"));
			}
			self.trials.push(trial);
			if infra.is_none() {
				self.infra_streak = 0;
				self.done.insert((arm.to_owned(), round.to_owned(), case.to_owned(), rep));
				break;
			}
			self.infra_streak += 1;
			if self.infra_streak >= INFRA_STREAK {
				return Err(Error::other(format!(
					"{INFRA_STREAK} trials in a row left no row (the last: {}); stopping before they spend more",
					infra.unwrap_or_default()
				)));
			}
		}
		Ok(true)
	}

	/// `arms` on `cases`, round-robin within each case, the order turned per case. `Ok(false)` once the spend stops it.
	async fn round(&mut self, arms: &[String], round: &str, cases: &[String], rep: u64) -> Result<bool> {
		for (at, case) in cases.iter().enumerate() {
			for arm in rotated(arms, at) {
				if !self.trial(&arm, round, case, rep).await? {
					return Ok(false);
				}
			}
		}
		Ok(true)
	}
}

/// The repo's cases only: his own (`~/.jocasta/agent/evals/cases`) never enter a run whose numbers are published.
fn repo_cases() -> Vec<PathBuf> {
	vec![REPO_ROOT.join("evals").join("cases")]
}

/// Every case's id, in order.
fn case_ids() -> Result<Vec<String>> {
	Ok(load_all_cases(&repo_cases())?.into_iter().map(|c| c.id).collect())
}

/// The [`FIRST_CASES`] cases every arm runs: of those the base always passed, the cheapest; then the rest by passes.
pub fn first_cases(trials: &[Trial], ids: &[String]) -> Vec<String> {
	let base = by_case(trials, "base", &["base"]);
	let mut ranked: Vec<(&String, usize, f64)> = ids
		.iter()
		.map(|id| {
			let runs = base.get(id.as_str()).cloned().unwrap_or_default();
			let passes = runs.iter().filter(|t| t.passed).count();
			(id, runs.len() - passes, mean(&runs.iter().map(|t| t.cost).collect::<Vec<_>>()))
		})
		.collect();
	ranked.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)));
	ranked.into_iter().take(FIRST_CASES).map(|(id, _, _)| id.clone()).collect()
}

/// `jocasta-bench evals settings screen --dir <run>` (step C).
pub async fn screen(dir: &Path) -> Result<()> {
	let rows: Vec<ReplayRow> = fs::read_to_string(dir.join("replay.jsonl"))?
		.lines()
		.filter_map(|line| serde_json::from_str(line).ok())
		.collect();
	let classes: HashMap<String, Class> = read_settings(dir)?.into_iter().map(|s| (s.key, s.class)).collect();
	let (chosen, rest) = live_arms(&rows, &classes);
	fs::write(dir.join("not-afforded.txt"), rest.iter().map(|arm| format!("{arm}\n")).collect::<String>())?;
	let arms: Vec<String> = chosen.iter().map(|(arm, _)| arm.clone()).collect();
	println!("{} live arms tried, {} live and not afforded", arms.len(), rest.len());
	let ids = case_ids()?;
	let mut ladder = Ladder::open(dir, BUDGET)?;
	for rep in 0..BASE_REPS {
		if !ladder.round(&["base".to_owned()], "base", &ids, rep).await? {
			return Err(Error::other("the budget ran out during the base"));
		}
	}
	let first = first_cases(&ladder.trials, &ids);
	let others: Vec<String> = ids.iter().filter(|id| !first.contains(id)).cloned().collect();
	let stands = |ladder: &Ladder, rounds: &[&str]| -> Vec<Standing> {
		arms.iter().map(|arm| standing(&ladder.trials, arm, rounds, &["base"])).collect()
	};
	let mut finished = ladder.round(&arms, "r1", &first, 0).await?;
	let half = best_share(&stands(&ladder, &["r1"]), 2);
	if finished {
		finished = ladder.round(&half, "r2", &others, 0).await?;
	}
	let third = best_share(&stands(&ladder, &["r1", "r2"]), 3);
	if finished {
		finished = ladder.round(&third, "r3", &ids, 1).await?;
	}
	let standings = stands(&ladder, &["r1", "r2", "r3"]);
	let summary = serde_json::json!({
		"spent": ladder.spent(),
		"finished": finished,
		"first_cases": first,
		"half": half,
		"third": third,
		"classes": chosen.iter().map(|(arm, class)| (arm.clone(), *class)).collect::<HashMap<_, _>>(),
		"standings": standings,
	});
	fs::write(dir.join("screen.json"), serde_json::to_string_pretty(&summary)?)?;
	println!("screen done: ${:.2} spent{}", ladder.spent(), if finished { "" } else { ", stopped by the budget" });
	Ok(())
}

/// The winners of the screen: C arms that ran every case, are not out and win; the best of each subsystem, best first.
pub fn subsystem_winners(standings: &[Standing], classes: &HashMap<String, Class>, all_cases: usize) -> Vec<Standing> {
	let mut best: BTreeMap<String, Standing> = BTreeMap::new();
	for s in standings.iter().filter(|s| s.cases >= all_cases && s.wins() && classes.get(&s.arm) == Some(&Class::C)) {
		let subsystem = s.arm.split(['.', '=']).next().unwrap_or("").to_owned();
		if best.get(&subsystem).is_none_or(|held| s.score() < held.score()) {
			best.insert(subsystem, s.clone());
		}
	}
	let mut winners: Vec<Standing> = best.into_values().collect();
	winners.sort_by(|a, b| a.score().total_cmp(&b.score()));
	winners
}

/// Several arms as one.
fn combined(arms: &[String]) -> Result<String> {
	let mut all: Arm = Vec::new();
	for arm in arms {
		all.extend(parse_arm(arm)?);
	}
	Ok(arm_name(&all))
}

/// `jocasta-bench evals settings confirm --dir <run>` (step D).
pub async fn confirm(dir: &Path) -> Result<()> {
	let screen: Value = serde_json::from_str(&fs::read_to_string(dir.join("screen.json"))?)?;
	let standings: Vec<Standing> = screen["standings"]
		.as_array()
		.into_iter()
		.flatten()
		.filter_map(|s| {
			let pair = |v: &Value| (v[0].as_f64().unwrap_or(f64::NEG_INFINITY), v[1].as_f64().unwrap_or(f64::INFINITY));
			Some(Standing {
				arm: s["arm"].as_str()?.to_owned(),
				cases: s["cases"].as_u64()? as usize,
				out: s["out"].as_bool()?,
				cost_ratio: s["cost_ratio"].as_f64().unwrap_or(f64::NAN),
				wall_ratio: s["wall_ratio"].as_f64().unwrap_or(f64::NAN),
				cost_interval: pair(&s["cost_interval"]),
				wall_interval: pair(&s["wall_interval"]),
			})
		})
		.collect();
	let classes: HashMap<String, Class> = serde_json::from_value(screen["classes"].clone())?;
	let ids = case_ids()?;
	let winners = subsystem_winners(&standings, &classes, ids.len());
	let mut ladder = Ladder::open(dir, BUDGET)?;
	let mut result = serde_json::json!({ "winners": winners });
	if winners.is_empty() {
		result["adopted"] = Value::Array(Vec::new());
		fs::write(dir.join("confirm.json"), serde_json::to_string_pretty(&result)?)?;
		println!("no winner: nothing adopted");
		return Ok(());
	}
	let names: Vec<String> = winners.iter().map(|w| w.arm.clone()).collect();
	let combo = combined(&names)?;
	// Interleaved: per rep and case, the base and the combination, which goes first turning with the case.
	let pair = ["base".to_owned(), combo.clone()];
	let mut finished = true;
	'confirm: for rep in 0..CONFIRM_REPS {
		for (at, case) in ids.iter().enumerate() {
			for arm in rotated(&pair, at + rep as usize) {
				if !ladder.trial(&arm, "confirm", case, rep).await? {
					finished = false;
					break 'confirm;
				}
			}
		}
	}
	let rows_of = |ladder: &Ladder, arm: &str, round: &str| -> Vec<ResultRow> {
		ladder
			.trials
			.iter()
			.filter(|t| t.arm == arm && t.round == round && t.infra.is_none())
			.filter_map(|t| t.row.clone())
			.collect()
	};
	let verdict = compare_rows(&rows_of(&ladder, "base", "confirm"), &rows_of(&ladder, &combo, "confirm"));
	let combo_standing = standing(&ladder.trials, &combo, &["confirm"], &["confirm"]);
	let mut adopted = vec![combo.clone()];
	let mut kept = verdict.verdict == Verdict::Keep;
	if names.len() > 1 && combo_standing.score() >= winners[0].score() {
		// No better than the best single arm: add the winners one at a time onto it. A step is taken only when it
		// beats the base on its own (no case the base always passes lost) and scores better than the set before it.
		let mut current = names[0].clone();
		let mut current_score = winners[0].score();
		for name in &names[1..] {
			let candidate = combined(&[current.clone(), name.clone()])?;
			if !ladder.round(std::slice::from_ref(&candidate), "greedy", &ids, 0).await? {
				finished = false;
				break;
			}
			let step = standing(&ladder.trials, &candidate, &["greedy"], &["confirm", "base"]);
			if step.wins() && step.score() < current_score {
				current = candidate;
				current_score = step.score();
			}
		}
		// The best single winner beat the base on all cases in the screen; each step taken beat it again.
		adopted = vec![current];
		kept = true;
	}
	result["combination"] = Value::String(combo);
	result["keep_rule"] = serde_json::json!({
		"verdict": format!("{:?}", verdict.verdict),
		"rate_base": verdict.rate_a,
		"rate_combination": verdict.rate_b,
		"tokens_ratio": verdict.mean_ratios.tokens,
		"wall_ratio": verdict.mean_ratios.wall_ms,
	});
	result["combination_standing"] = serde_json::to_value(&combo_standing)?;
	result["adopted"] = if kept { serde_json::to_value(&adopted)? } else { Value::Array(Vec::new()) };
	result["spent"] = Value::from(ladder.spent());
	result["finished"] = Value::Bool(finished);
	fs::write(dir.join("confirm.json"), serde_json::to_string_pretty(&result)?)?;
	println!("confirm done: ${:.2} spent; adopted {}", ladder.spent(), result["adopted"]);
	Ok(())
}

/// `jocasta-bench evals settings models --dir <run>`: every [`MODEL_ARMS`] arm and the base on all 12 cases,
/// [`MODEL_REPS`] times, interleaved per case with the order turning, so time of day and the cache weigh on all alike.
pub async fn models(dir: &Path) -> Result<()> {
	let ids = case_ids()?;
	let mut ladder = Ladder::open(dir, BUDGET)?;
	let arms: Vec<String> = std::iter::once("base").chain(MODEL_ARMS.iter().copied()).map(str::to_owned).collect();
	let mut finished = true;
	'study: for rep in 0..MODEL_REPS {
		for (at, case) in ids.iter().enumerate() {
			for arm in rotated(&arms, at + rep as usize) {
				if !ladder.trial(&arm, "models", case, rep).await? {
					finished = false;
					break 'study;
				}
			}
		}
	}
	let study: Vec<Value> = arms[1..]
		.iter()
		.map(|arm| {
			let runs: Vec<&Trial> =
				ladder.trials.iter().filter(|t| &t.arm == arm && t.round == "models" && t.infra.is_none()).collect();
			serde_json::json!({
				"standing": standing(&ladder.trials, arm, &["models"], &["models"]),
				"passed": runs.iter().filter(|t| t.passed).count(),
				"runs": runs.len(),
			})
		})
		.collect();
	let base_runs: Vec<&Trial> =
		ladder.trials.iter().filter(|t| t.arm == "base" && t.round == "models" && t.infra.is_none()).collect();
	let result = serde_json::json!({
		"spent": ladder.spent(),
		"finished": finished,
		"base": { "passed": base_runs.iter().filter(|t| t.passed).count(), "runs": base_runs.len() },
		"arms": study,
	});
	fs::write(dir.join("models.json"), serde_json::to_string_pretty(&result)?)?;
	println!("models done: ${:.2} spent", ladder.spent());
	Ok(())
}

/// `jocasta-bench evals settings trial --dir <run> --arm <arm> --case <id> --rep <n> --out <file>`: one trial of the
/// base with the arm over it, its row written to `--out`. The ladder starts it; it isn't for his hand.
pub async fn trial(argv: &[String]) -> Result<()> {
	let flag = |name: &str| {
		argv.iter()
			.position(|arg| arg == name)
			.and_then(|at| argv.get(at + 1))
			.cloned()
			.ok_or_else(|| Error::user(format!("trial needs {name}")))
	};
	let dir = PathBuf::from(flag("--dir")?);
	hold_omp_version(&dir)?;
	let arm = parse_arm(&flag("--arm")?)?;
	let case_id = flag("--case")?;
	let rep: u64 = flag("--rep")?.parse().map_err(|_| Error::user("--rep needs a number"))?;
	let out = PathBuf::from(flag("--out")?);
	let mut variant = base_variant()?;
	for (key, value) in &arm {
		match (key.as_str(), value.as_str()) {
			(MODEL_KEY, Some(model)) => variant.model = Some(model.to_owned()),
			(THINKING_KEY, Some(level)) => variant.args.extend(["--thinking".to_owned(), level.to_owned()]),
			_ => {
				variant.settings =
					Some(deep_merge(&variant.settings.take().unwrap_or_default(), &arm_delta(key, value)));
			}
		}
	}
	variant.args.extend(["--max-cost".to_owned(), format!("{TRIAL_CAP_USD:.2}")]);
	variant.label = arm_name(&arm);
	let eval_case = load_all_cases(&repo_cases())?
		.into_iter()
		.find(|c| c.id == case_id)
		.ok_or_else(|| Error::user(format!("no case {case_id}")))?;
	let launcher = launcher_of(&variant)?;
	let version = read_version(&launcher)?;
	// The arm's own name in file names: a short hash of it, listed beside the run's rows in `arms.txt`.
	let short: String = Sha256::digest(arm_name(&arm)).iter().take(5).map(|byte| format!("{byte:02x}")).collect();
	let name = format!("{short}-{case_id}-{rep}");
	let options = TrialOptions {
		out_dir: dir.join("runs"),
		timeout_ms: Some(10 * 60_000),
		launcher,
		variant: Some(variant),
		transcript_name: Some(name.clone()),
		// Its sessions go to the trial's own folder and are kept here, never into his session store.
		sessions_dir: Some(dir.join("sessions").join(&name)),
		..TrialOptions::default()
	};
	fs::OpenOptions::new().create(true).append(true).open(dir.join("arms.txt")).and_then(|mut file| {
		std::io::Write::write_all(&mut file, format!("{short} {}\n", arm_name(&arm)).as_bytes())
	})?;
	fs::create_dir_all(options.out_dir.join("transcripts"))?;
	let row = run_trial(&eval_case, rep, &version, options).await?;
	fs::write(&out, serde_json::to_string(&row)?)?;
	Ok(())
}

/// Summary of the run for `docs/research/settings-trials-2.md`: counts per class and per step-B verdict.
pub fn counts(dir: &Path) -> Result<Value> {
	let settings = read_settings(dir)?;
	let mut classes: BTreeMap<String, usize> = BTreeMap::new();
	for s in &settings {
		*classes.entry(format!("{:?}", s.class)).or_default() += 1;
	}
	let rows: Vec<ReplayRow> = fs::read_to_string(dir.join("replay.jsonl"))
		.unwrap_or_default()
		.lines()
		.filter_map(|line| serde_json::from_str(line).ok())
		.collect();
	let mut verdicts: BTreeMap<String, usize> = BTreeMap::new();
	for r in rows.iter().filter(|r| r.verdict != "base") {
		*verdicts.entry(r.verdict.clone()).or_default() += 1;
	}
	Ok(serde_json::json!({ "settings": settings.len(), "classes": classes, "replay": verdicts }))
}
