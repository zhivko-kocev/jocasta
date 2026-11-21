//! Jocasta's eval runner (1.10) on his daily omp (0386): each trial runs `omp -p <prompt> --mode json` headless in a
//! fresh temp fixture repo on its own branch (0165), with his `~/.omp/agent` logins (or a `--profile`'s, 0482 E8) and a
//! per-trial `--config` overlay that turns memory off and carries the variant's settings. A variant may run Claude
//! Code's `claude -p` or opencode's `opencode run` instead (E4, 0537), plan first (E3) and add follow-up turns (E2). It
//! grades the repo's end state (0164), then the case's hidden checks on a clean clone of the result (E1), and writes the
//! claude-api skill's layout: `results.jsonl`, `transcripts/<case>-<rep>.jsonl` and `report.html` (0165, 0166).
//!
//! The repo, the setup and the grading are in `grading.rs`; their names are re-exported here, where `runner.ts` had them.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value, json};

pub use crate::evals::cases::load_all_cases;
#[cfg(test)]
pub use crate::evals::grading::grade_check;
use crate::evals::grading::{
	GRADING_LIMIT_MS, HIDDEN_TIMEOUT, SETUP_LIMIT_MS, SetupFailure, check_label, check_result, line_counts,
	make_trial_repo_checked,
};
pub use crate::evals::grading::{grade_hidden, make_trial_repo};
use crate::evals::harness::{
	A7_API_KEY_ENV, ResponseSource, TrialRecords, Turn, Usage, adapter, assistant_text, omp_session_text,
	read_prewalk_switch, save_sessions,
};
use crate::evals::layer::{BENCH_LAYER, BenchLayer, LayerSetup, LayerTrial};
use crate::evals::plan_review::{PlanReviewChoice, PlanReviewSummary, plan_review_extension, read_plan_review_log};
use crate::evals::report::render_report;
pub use crate::evals::report::{MEASUREMENT_NOTES, to_fixed};
use crate::evals::terminal::{TerminalRunner, TerminalTurn, herdr_terminal};
use crate::evals::variants::{Harness, Variant, deep_merge, default_variant, variant_args};
use crate::lib::output::{iso_now, json_text, read_version};
use crate::lib::sandbox::BENCH_PTY_SESSION;
use jocasta::frontier::protocol::js_number;
use jocasta::shared::env;
use jocasta::shared::errors::Result;
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::jsonl::append_jsonl;
use jocasta::shared::proctree::{end_processes, process_id_of, processes_in};
use jocasta::shared::prompt::without_leading_comment;

/// The overlay every trial starts from: eval sessions never enter his memory. A variant's settings layer over it.
pub fn base_overlay() -> Map<String, Value> {
	object(json!({ "memory": { "backend": "off" } }))
}

/// What an interactive plan-review turn adds to the overlay: plan mode at startup (`plan.defaultOnStartup`).
pub fn plan_review_overlay() -> Map<String, Value> {
	object(json!({ "plan": { "enabled": true, "defaultOnStartup": true } }))
}

fn object(value: Value) -> Map<String, Value> {
	match value {
		Value::Object(map) => map,
		_ => Map::new(),
	}
}

/// What A7's key becomes in its transcript and exports when an agent printed it (EVL-13).
const A7_KEY_MARK: &str = "<A7 key>";

/// An end-state check (0164): a command that exits 0, a file's content or absence, or whether git is clean.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Check {
	pub command: Option<String>,
	pub file: Option<String>,
	pub contains: Option<String>,
	pub absent: Option<bool>,
	pub git_clean: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvalCase {
	pub id: String,
	pub dir: PathBuf,
	pub prompt: String,
	pub tags: Vec<String>,
	pub checks: Vec<Check>,
	pub hidden_checks: Vec<Check>,
	pub setup: Option<String>,
	/// `fixture/` beside `case.yaml`: files committed into the trial's repo before the run.
	pub fixture: Option<PathBuf>,
	/// `hidden/` beside `case.yaml`: grader files copied in only for the hidden checks, never into the agent's checkout.
	pub hidden: Option<PathBuf>,
	/// A tag on the fixture commit (0482's task benchmark: upstream's nearest tag at the base commit), there before setup
	/// runs and moved onto the commit that folds setup's output in, so tools that read the version from git see upstream's.
	pub fixture_tag: Option<String>,
	/// Folders relative to the trial's root, put first on PATH for the setup, the harness and the checks (0482's task
	/// benchmark: a Python task's `venv/Scripts`, so every `python` is the trial's own).
	pub path_prefix: Option<Vec<String>>,
	/// Hidden grading's setup when it differs from `setup` (0574: a Python task's fresh grading venv).
	pub grade_setup: Option<String>,
	/// Hidden grading's `path_prefix` when it differs from the trial's (the grading venv's `Scripts`).
	pub grade_path_prefix: Option<Vec<String>>,
}

/// `Harness` as the rows spell it.
mod harness_text {
	use super::Harness;
	use serde::{Deserialize, Deserializer, Serializer};

	pub fn serialize<S: Serializer>(harness: &Harness, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(harness.as_str())
	}

	pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Harness, D::Error> {
		match String::deserialize(deserializer)?.as_str() {
			"omp" => Ok(Harness::Omp),
			"claude" => Ok(Harness::Claude),
			"opencode" => Ok(Harness::Opencode),
			other => Err(serde::de::Error::custom(format!("unknown harness {other}"))),
		}
	}
}

/// `[P0]`–`[P3]` findings in the first follow-up's reply (A3's review turn), P0/P1 blocking (0453).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Findings {
	pub total: u64,
	pub blocking: u64,
}

/// `[P0]`…`[P3]` markers in a review's reply (0453's priorities); P0 and P1 are blocking.
pub fn count_findings(text: &str) -> Findings {
	let mut findings = Findings { total: 0, blocking: 0 };
	for (at, _) in text.match_indices("[P") {
		let mut rest = text[at + 2..].chars();
		let (Some(digit), Some(']')) = (rest.next(), rest.next()) else { continue };
		if let Some(priority) = digit.to_digit(10).filter(|priority| *priority <= 3) {
			findings.total += 1;
			if priority <= 1 {
				findings.blocking += 1;
			}
		}
	}
	findings
}

/// A number as `JSON.stringify` writes it.
fn whole_number<S: Serializer>(value: &f64, serializer: S) -> std::result::Result<S::Ok, S::Error> {
	js_number(value, serializer)
}

/// One `results.jsonl` row (0165, 0375, 0482). `grade` maps each check to 1 (passed) or 0; hidden checks are labelled
/// `hidden: …`. `usage` is the main session's, from its assistant `message_end` events; `total_usage` and `requests`
/// add the subagents omp's `task` tool ran, from each call's result details (see [`MEASUREMENT_NOTES`]). Over a
/// trial's follow-up turns everything sums.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
	pub case: String,
	pub rep: u64,
	/// The variant label (0375): names joined with "+", or "default".
	pub variant: String,
	#[serde(with = "harness_text")]
	pub harness: Harness,
	/// `<harness> --version` of the run, e.g. `omp/18.12.6`.
	pub harness_version: String,
	/// The trial's start, ISO: the analysis logs time of day (0482, [S16]).
	pub started_at: String,
	pub model: Option<String>,
	pub usage: Usage,
	pub total_usage: Usage,
	pub requests: u64,
	/// Assistant turns in the transcript, and the tool calls in them.
	pub turns: u64,
	pub tool_calls: u64,
	/// The prompt and its follow-ups that ran.
	pub prompts: u64,
	/// From the first harness spawn to the last one's close, pacing pauses excluded.
	#[serde(serialize_with = "whole_number")]
	pub wall_ms: f64,
	/// `git diff --numstat` against the fixture commit, untracked files included; binary files count 0.
	pub lines_added: u64,
	pub lines_removed: u64,
	pub grade: Map<String, Value>,
	/// Every hidden check passed on the clean clone; `None` when the case has none.
	pub hidden_passed: Option<bool>,
	pub findings: Option<Findings>,
	pub passed: bool,
	/// A plan-review trial's review (0567): requested, the choice given and taken, and why omp was shut down.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub plan_review: Option<PlanReviewSummary>,
	/// A `--prewalk` trial (0608): omp's prewalk switched the model, read from the session files.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub prewalk_switched: Option<bool>,
	/// A7's key was in the trial's output or exports; it was replaced there with `<A7 key>` (EVL-13).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub key_exposed: Option<bool>,
}

/// What a harness run ended with, for `on_harness_run`.
#[derive(Debug, Clone)]
pub struct HarnessRun {
	pub stderr: String,
}

/// Called after each harness run with its exit, so a caller can tell a sign-in failure from the task's (0482).
pub type OnHarnessRun = Arc<dyn Fn(&HarnessRun) + Send + Sync>;

/// What one trial needs: [`run_evals`] passes its run's options, the task benchmark (`evals/bench.rs`, 0482) its own.
#[derive(Clone, Default)]
pub struct TrialOptions {
	/// The run folder; transcripts go to its `transcripts/`.
	pub out_dir: PathBuf,
	/// Per-trial limit over all its turns (0318); a trial that reaches it is killed with its process tree and graded failed.
	pub timeout_ms: Option<u64>,
	/// The command that starts the variant's harness; the runner appends the arguments of [`trial_args`].
	pub launcher: Vec<String>,
	/// The run's variants, layered (0375); the default variant when unset.
	pub variant: Option<Variant>,
	/// omp's `--profile <name>` on every trial (0482 E8): its own logins, sessions and settings, apart from his daily omp.
	pub profile: Option<String>,
	/// The transcript's file name without `.jsonl`; `<case>-<rep>` when unset.
	pub transcript_name: Option<String>,
	pub on_harness_run: Option<OnHarnessRun>,
	/// Where the trial's root goes; the OS temp folder when unset. A short one keeps deep trees under 260 characters.
	pub work_dir: Option<PathBuf>,
	/// When set, the setup's and the hidden grading's commands are logged to `setup.log` and `grade.log` in it, and the
	/// per-file line counts behind `lines_added`/`lines_removed` to `lines.log` (0508).
	pub log_dir: Option<PathBuf>,
	/// When set, the harness's own records of the trial are saved there for the ledger (0538): omp's session files (omp
	/// runs with `--session-dir` in the trial root), Claude Code's transcripts and subagent transcripts, and opencode's
	/// `opencode export` of each session. Emptied first, so a retried attempt keeps only its own.
	pub sessions_dir: Option<PathBuf>,
	/// The bench herdr session whose panes give a plan-review trial's first turn its terminal (0567).
	pub pty_session: Option<String>,
	/// Runs that turn instead of a herdr pane (tests).
	pub run_in_terminal: Option<TerminalRunner>,
	/// The grading limit; [`GRADING_LIMIT_MS`] when unset.
	pub grading_timeout_ms: Option<u64>,
	/// The limit of the setup before the harness; [`SETUP_LIMIT_MS`] when unset.
	pub setup_timeout_ms: Option<u64>,
}

/// Called with each row as it is written.
pub type OnRow = Box<dyn Fn(&ResultRow) + Send + Sync>;

/// `RunOptions`: a run's cases, folder, reps and hooks.
pub struct RunOptions {
	pub trial: TrialOptions,
	/// Case folders, read in order: the repo's cases and then his own (0166, 0375).
	pub cases_dir: Vec<PathBuf>,
	/// The run folder, `<data>/evals/runs/<time>-<variant label>/` (0166, 0375).
	pub out_dir: PathBuf,
	pub reps: u64,
	/// Called with each row as it is written.
	pub on_row: Option<OnRow>,
	/// Awaited before each trial: the quota pacer (0482) holds the run here while a window is past its limit.
	pub pace: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
}

/// This process's environment as the programs it starts inherit it (the test overlay included).
pub fn process_env() -> HashMap<String, String> {
	env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect()
}

/// Runs every case `reps` times. Real model calls: `jocasta-bench evals` gates it behind `JOCASTA_EVALS_REAL` and off CI (0169).
pub async fn run_evals(options: RunOptions) -> Result<Vec<ResultRow>> {
	let cases = load_all_cases(&options.cases_dir)?;
	let harness_version = read_version(&options.trial.launcher)?;
	fs::create_dir_all(options.out_dir.join("transcripts"))?;
	let results_file = options.out_dir.join("results.jsonl");
	fs::write(&results_file, "")?;
	let trial = TrialOptions { out_dir: options.out_dir.clone(), ..options.trial.clone() };
	let mut rows: Vec<ResultRow> = Vec::new();
	for eval_case in &cases {
		for rep in 0..options.reps {
			if let Some(pace) = &options.pace {
				pace().await;
			}
			let row = run_trial(eval_case, rep, &harness_version, trial.clone()).await?;
			append_jsonl(&results_file, &row)?;
			if let Some(on_row) = &options.on_row {
				on_row(&row);
			}
			rows.push(row);
		}
	}
	let variant = options.trial.variant.clone().unwrap_or_else(default_variant);
	fs::write(
		options.out_dir.join("report.html"),
		render_report(&rows, &cases, options.reps, &variant.label, &harness_version),
	)?;
	Ok(rows)
}

/// One turn's harness arguments after the launcher, as the variant's harness builds them (its adapter's `args`); a
/// follow-up never plans first.
pub fn trial_args(variant: &Variant, turn: &Turn) -> Result<Vec<String>> {
	let planned = Variant { plan_first: variant.plan_first && !turn.follow_up, ..variant.clone() };
	(adapter(variant.harness).args)(&variant_args(&planned), turn)
}

/// A plan-review trial's extension and what it logs (0567).
struct PlanReview {
	choice: PlanReviewChoice,
	role: Option<String>,
	log: PathBuf,
	extension: PathBuf,
}

/// What the trial's pieces have set up when its turns start.
struct Prepared {
	variant: Variant,
	repo: PathBuf,
	fixture_commit: String,
	overlay: PathBuf,
	plan_review: Option<PlanReview>,
	prewalk: bool,
	session_dir: Option<PathBuf>,
	prompts: Vec<String>,
	harness_env: HashMap<String, String>,
	layer_args: Vec<String>,
}

/// What the harness's turns left.
struct Turns {
	outputs: Vec<String>,
	wall_ms: u64,
	exited_ok: bool,
	timed_out: bool,
}

/// A harness's output with its exit.
struct HarnessOutput {
	status: Option<i32>,
	stdout: String,
	stderr: String,
	timed_out: bool,
	/// From spawn to close.
	wall_ms: u64,
}

/// A7's key (EVL-13): wherever the agent printed it, it never reaches what the run keeps.
struct Secret {
	value: Option<String>,
	exposed: Arc<AtomicBool>,
}

impl Secret {
	fn scrub(&self, text: &str) -> String {
		match &self.value {
			Some(secret) if !secret.is_empty() && text.contains(secret.as_str()) => {
				self.exposed.store(true, Ordering::SeqCst);
				text.replace(secret.as_str(), A7_KEY_MARK)
			}
			_ => text.to_owned(),
		}
	}
}

/// What one trial holds from its start to its end.
struct TrialCtx<'a> {
	eval_case: &'a EvalCase,
	rep: u64,
	options: &'a TrialOptions,
	chosen: Variant,
	root: PathBuf,
	env: HashMap<String, String>,
	name: String,
	secret: Secret,
}

/// One trial: a fresh temp root with the overlay and the fixture repo, the harness's turns, the end-state checks, the
/// line counts and the hidden checks. A failed setup before the harness, one cut by [`SETUP_LIMIT_MS`], or a grading
/// setup that fails on the fixture commit too, fails (0482's infra); what the harness and the agent left running is
/// ended and the temp root removed either way.
pub async fn run_trial(
	eval_case: &EvalCase,
	rep: u64,
	harness_version: &str,
	options: TrialOptions,
) -> Result<ResultRow> {
	let chosen = options.variant.clone().unwrap_or_else(default_variant);
	let root = make_trial_root(options.work_dir.as_deref())?;
	let env = trial_env(eval_case, &root, &chosen.env);
	let secret_env = adapter(chosen.harness).api_key_env.and_then(env::var);
	let ctx = TrialCtx {
		eval_case,
		rep,
		options: &options,
		name: options.transcript_name.clone().unwrap_or_else(|| format!("{}-{rep}", eval_case.id)),
		chosen,
		root: root.clone(),
		env,
		secret: Secret { value: secret_env, exposed: Arc::new(AtomicBool::new(false)) },
	};
	let mut layer: Option<LayerSetup> = None;
	let outcome = ctx.run(&mut layer, harness_version, iso_now()).await;
	if let Some(layer) = layer.as_mut() {
		layer.close().await;
	}
	end_leftovers(&root).await;
	if let Err(error) = remove_tree(&root) {
		eprintln!("could not remove {} ({error}); delete it by hand", root.display());
	}
	outcome
}

impl TrialCtx<'_> {
	fn log_file(&self, phase: &str) -> Option<PathBuf> {
		self.options.log_dir.as_ref().map(|dir| dir.join(format!("{phase}.log")))
	}

	async fn run(
		&self,
		layer: &mut Option<LayerSetup>,
		harness_version: &str,
		started_at: String,
	) -> Result<ResultRow> {
		let prepared = self.prepare(layer).await?;
		let turns = self.run_turns(&prepared).await?;
		// EVL-12: language servers, watchers and dev servers the harness or the agent left go before grading, and a
		// git index lock a killed process left with them, so grading sees the agent's files, not a held repo.
		end_leftovers(&self.root).await;
		remove_tree(&prepared.repo.join(".git").join("index.lock"))?;
		self.grade(&prepared, &turns, harness_version, started_at).await
	}

	/// The repo, the layer, the overlay and the plan-review extension.
	async fn prepare(&self, layer: &mut Option<LayerSetup>) -> Result<Prepared> {
		let adapter = adapter(self.chosen.harness);
		let variant = with_prompt_copy(&self.chosen, &self.root)?;
		// Outside the repo, so it never shows in a git_clean check or the line counts. JSON is YAML.
		let overlay = self.root.join("eval-run.yml");
		let setup_limit_ms = self.options.setup_timeout_ms.unwrap_or(SETUP_LIMIT_MS);
		let made = make_trial_repo_checked(
			self.eval_case,
			self.rep,
			&self.root,
			&self.env,
			self.log_file("setup").as_deref(),
			Some(setup_limit_ms),
		)
		.await;
		let repo = match made {
			Ok(repo) => repo,
			Err(failure) => {
				if matches!(failure, SetupFailure::Killed(_)) {
					eprintln!(
						"{}: setup killed after {} min",
						self.name,
						((setup_limit_ms as f64 / 60_000.0) + 0.5).floor()
					);
				}
				return Err(failure.into());
			}
		};
		// 0567: plan first answered in omp's interactive plan review; the first turn runs in a terminal.
		let planned = adapter.session_files.then_some(variant.plan_review).flatten();
		let plan_review = planned.map(|choice| PlanReview {
			choice,
			role: variant.plan_review_role.clone(),
			log: self.root.join("plan-review.jsonl"),
			extension: self.root.join("plan-review.js"),
		});
		// 0608: whether prewalk's switch fired is read from the session files, so they are kept for such a trial.
		let prewalk = adapter.session_files && variant.args.iter().any(|arg| arg == "--prewalk");
		if variant.layer {
			// A6 (0543): Jocasta's layer sets the trial up.
			*layer = Some(
				BENCH_LAYER
					.set_up(LayerTrial {
						root: self.root.clone(),
						repo: repo.repo.clone(),
						base_commit: repo.fixture_commit.clone(),
						prompt: self.eval_case.prompt.clone(),
						append_system_prompt: variant.append_system_prompt.as_ref().map(PathBuf::from),
						env: self.env.iter().map(|(name, value)| (name.clone(), value.clone())).collect(),
					})
					.await?,
			);
		}
		let layer_settings = layer.as_ref().map(|layer| layer.settings.clone()).unwrap_or_default();
		let mut settings = base_overlay();
		for delta in [
			variant.settings.clone().unwrap_or_default(),
			if plan_review.is_some() { plan_review_overlay() } else { Map::new() },
			layer_settings,
		] {
			settings = deep_merge(&settings, &delta);
		}
		fs::write(&overlay, format!("{}\n", json_text(&Value::Object(settings), true)))?;
		if let Some(review) = &plan_review {
			let extension =
				plan_review_extension(review.choice, &review.log.display().to_string(), review.role.as_deref());
			fs::write(&review.extension, extension)?;
		}
		let prompts = std::iter::once(self.eval_case.prompt.clone())
			.chain(variant.followups.iter().map(|text| text.replace("{base}", &repo.fixture_commit)))
			.collect();
		// omp writes its session files into the trial root (the ledger's and a plan-review trial's record); opencode's
		// exports are needed for its usage either way.
		let wants_sessions = self.options.sessions_dir.is_some() || plan_review.is_some() || prewalk;
		let session_dir = (wants_sessions && adapter.session_files).then(|| self.root.join("sessions"));
		let mut harness_env = self.env.clone();
		match (adapter.api_key_env, &self.secret.value) {
			(Some(_), Some(secret)) => {
				harness_env.insert("ANTHROPIC_API_KEY".to_owned(), secret.clone());
			}
			(Some(_), None) => {}
			(None, _) => {
				if let Some(layer) = layer.as_ref() {
					harness_env.extend(layer.env.clone());
				}
			}
		}
		let layer_args = layer.as_ref().map(|layer| layer.args.clone()).unwrap_or_default();
		Ok(Prepared {
			variant,
			repo: repo.repo,
			fixture_commit: repo.fixture_commit,
			overlay,
			plan_review,
			prewalk,
			session_dir,
			prompts,
			harness_env,
			layer_args,
		})
	}

	/// The harness's turns: the prompt, then each follow-up, until one fails or the trial's limit.
	async fn run_turns(&self, prepared: &Prepared) -> Result<Turns> {
		let adapter = adapter(prepared.variant.harness);
		let (command, launcher_args) = match self.options.launcher.split_first() {
			Some((command, rest)) => (command.clone(), rest.to_vec()),
			None => (String::new(), Vec::new()),
		};
		let mut turns = Turns { outputs: Vec::new(), wall_ms: 0, exited_ok: true, timed_out: false };
		for (i, prompt) in prepared.prompts.iter().enumerate() {
			let left = self.options.timeout_ms.map(|limit| limit.saturating_sub(turns.wall_ms).max(1));
			let review_turn = i == 0 && prepared.plan_review.is_some();
			let turn = Turn {
				prompt: prompt.clone(),
				follow_up: i > 0,
				overlay: prepared.overlay.display().to_string(),
				profile: self.options.profile.clone(),
				session_dir: prepared.session_dir.as_ref().map(|dir| dir.display().to_string()),
				session: (adapter.session)(&turns.outputs.join("\n")),
				extension: prepared
					.plan_review
					.as_ref()
					.filter(|_| review_turn)
					.map(|review| review.extension.display().to_string()),
			};
			let mut args = launcher_args.clone();
			args.extend(trial_args(&prepared.variant, &turn)?);
			args.extend(prepared.layer_args.iter().cloned());
			let mut run = match prepared.plan_review.as_ref().filter(|_| review_turn) {
				Some(review) => {
					let turn = TerminalTurn {
						command: command.clone(),
						args,
						cwd: prepared.repo.clone(),
						env: prepared.harness_env.clone(),
						timeout_ms: left,
					};
					self.run_review_turn(review, turn).await?
				}
				None => run_harness(&command, &args, &prepared.repo, &prepared.harness_env, left).await,
			};
			run.stdout = self.secret.scrub(&run.stdout);
			run.stderr = self.secret.scrub(&run.stderr);
			if let Some(on_run) = &self.options.on_harness_run {
				on_run(&HarnessRun { stderr: run.stderr.clone() });
			}
			turns.outputs.push(run.stdout.clone());
			turns.wall_ms += run.wall_ms;
			if run.timed_out {
				turns.timed_out = true;
				eprintln!(
					"{}: killed after {} ms",
					self.name,
					self.options.timeout_ms.map_or_else(|| "undefined".to_owned(), |limit| limit.to_string())
				);
			} else if run.status != Some(0) {
				let status = run.status.map_or_else(|| "null".to_owned(), |status| status.to_string());
				eprintln!("{}: {} exited {status}\n{}", self.name, prepared.variant.harness, run.stderr);
			}
			if run.status != Some(0) || run.timed_out {
				turns.exited_ok = false;
				break;
			}
		}
		Ok(turns)
	}

	/// The first turn of a plan-review trial: omp interactive in a terminal, with the extension that answers the review.
	async fn run_review_turn(&self, review: &PlanReview, turn: TerminalTurn) -> Result<HarnessOutput> {
		let terminal = match &self.options.run_in_terminal {
			Some(terminal) => terminal.clone(),
			None => herdr_terminal(
				self.options.pty_session.as_deref().unwrap_or(BENCH_PTY_SESSION),
				crate::evals::terminal::TerminalDeps {
					workspace_cwd: Some(self.options.work_dir.clone().unwrap_or_else(std::env::temp_dir)),
					..Default::default()
				},
			)?,
		};
		let ended = terminal(turn).await?;
		let log = fs::read_to_string(&review.log).unwrap_or_default();
		// herdr reports no exit code: the turn counts as ended well when the extension shut omp down.
		let ok =
			!ended.timed_out && read_plan_review_log(&log, review.choice, review.role.as_deref()).shutdown.is_some();
		Ok(HarnessOutput {
			status: if ended.timed_out { None } else { Some(i32::from(!ok)) },
			stdout: String::new(),
			stderr: if ok {
				String::new()
			} else {
				"omp ended without the plan-review extension's shutdown".to_owned()
			},
			timed_out: ended.timed_out,
			wall_ms: ended.wall_ms,
		})
	}

	/// The transcript and saved sessions, the end-state checks, the line counts and the hidden checks, as the row.
	async fn grade(
		&self,
		prepared: &Prepared,
		turns: &Turns,
		harness_version: &str,
		started_at: String,
	) -> Result<ResultRow> {
		let variant = &prepared.variant;
		let adapter = adapter(variant.harness);
		let plan_review = prepared.plan_review.as_ref();
		// Each turn's output ends its last line, so one turn's JSON never runs into the next. A plan-review trial's
		// first turn had a terminal, not JSON: its transcript is the session files omp wrote, every turn included.
		let transcript =
			match (plan_review, &prepared.session_dir) {
				(Some(_), Some(dir)) => omp_session_text(dir)?,
				_ => turns
					.outputs
					.iter()
					.map(|output| {
						if output.is_empty() || output.ends_with('\n') { output.clone() } else { format!("{output}\n") }
					})
					.collect(),
			};
		fs::write(self.options.out_dir.join("transcripts").join(format!("{}.jsonl", self.name)), &transcript)?;
		let saved_sessions =
			self.options.sessions_dir.clone().or_else(|| adapter.records_dir.map(|dir| self.root.join(dir)));
		if let Some(saved) = &saved_sessions {
			let secret = Secret { value: self.secret.value.clone(), exposed: self.secret.exposed.clone() };
			save_sessions(
				variant.harness,
				saved,
				&TrialRecords {
					transcript: transcript.clone(),
					session_dir: prepared.session_dir.clone(),
					launcher: self.options.launcher.clone(),
					env: prepared.harness_env.clone(),
					cwd: prepared.repo.clone(),
					scrub: Some(Box::new(move |text| secret.scrub(text))),
				},
			)?;
		}
		let (grade, lines, hidden_passed) = self.grade_trial(prepared, turns).await?;
		let response = (adapter.response)(&ResponseSource {
			transcript: &transcript,
			saved_sessions: saved_sessions.as_deref(),
			session_dir: prepared.session_dir.as_deref(),
			plan_review: plan_review.is_some(),
		})?;
		let passed = grade.values().all(|score| score.as_f64() == Some(1.0));
		let findings = turns.outputs.get(1).map(|review| count_findings(&assistant_text(review, variant.harness)));
		let plan_review_summary = plan_review.map(|review| {
			let log = fs::read_to_string(&review.log).unwrap_or_default();
			read_plan_review_log(&log, review.choice, review.role.as_deref())
		});
		Ok(ResultRow {
			case: self.eval_case.id.clone(),
			rep: self.rep,
			variant: variant.label.clone(),
			harness: variant.harness,
			harness_version: harness_version.to_owned(),
			started_at,
			model: response.model,
			usage: response.usage,
			total_usage: response.total_usage,
			requests: response.requests,
			turns: response.turns,
			tool_calls: response.tool_calls,
			prompts: turns.outputs.len() as u64,
			wall_ms: turns.wall_ms as f64,
			lines_added: lines.0,
			lines_removed: lines.1,
			grade,
			hidden_passed,
			// Only when a review turn ran: a trial whose first turn failed has no review to count.
			findings,
			passed,
			plan_review: plan_review_summary,
			prewalk_switched: match (prepared.prewalk, &prepared.session_dir) {
				(true, Some(dir)) => Some(read_prewalk_switch(dir)?),
				_ => None,
			},
			key_exposed: self.secret.exposed.load(Ordering::SeqCst).then_some(true),
		})
	}

	/// The grade: the harness's exit, the end-state checks, the line counts and the hidden checks.
	async fn grade_trial(
		&self,
		prepared: &Prepared,
		turns: &Turns,
	) -> Result<(Map<String, Value>, (u64, u64), Option<bool>)> {
		let harness = prepared.variant.harness;
		let grading_limit_ms = self.options.grading_timeout_ms.unwrap_or(GRADING_LIMIT_MS);
		let mut grade = Map::new();
		grade.insert(format!("{harness} exited 0"), Value::from(u8::from(turns.exited_ok)));
		if turns.timed_out {
			grade.insert("timeout".to_owned(), Value::from(0));
		}
		let killed = || {
			eprintln!(
				"{}: grading killed after {} min",
				self.name,
				((grading_limit_ms as f64 / 60_000.0) + 0.5).floor()
			);
		};
		let checks_by = Instant::now() + std::time::Duration::from_millis(grading_limit_ms);
		for check in &self.eval_case.checks {
			let result = check_result(check, &prepared.repo, Some(&self.env), None, checks_by).await?;
			if result.timed_out {
				killed();
			}
			grade.insert(check_label(check), Value::from(u8::from(result.passed)));
		}
		// After grading, so staging can't change a git_clean check.
		let lines = line_counts(&prepared.repo, &prepared.fixture_commit, self.log_file("lines").as_deref())?;
		let mut hidden_passed = None;
		if !self.eval_case.hidden_checks.is_empty() {
			let grading_case = EvalCase {
				path_prefix: self.eval_case.grade_path_prefix.clone().or_else(|| self.eval_case.path_prefix.clone()),
				..self.eval_case.clone()
			};
			let hidden = grade_hidden(
				self.eval_case,
				&prepared.repo,
				&prepared.fixture_commit,
				&self.root.join("grade"),
				&trial_env(&grading_case, &self.root, &self.chosen.env),
				self.log_file("grade").as_deref(),
				Some(grading_limit_ms),
			)
			.await?;
			if hidden.contains_key(HIDDEN_TIMEOUT) {
				killed();
			}
			hidden_passed = Some(hidden.values().all(|score| score.as_f64() == Some(1.0)));
			grade.extend(hidden);
		}
		Ok((grade, lines, hidden_passed))
	}
}

/// Ends every process whose working folder is in `root` (EVL-12): what a harness or an agent started detached outlives
/// the harness, holds the trial's files and piles up over a run.
async fn end_leftovers(root: &Path) {
	let ids: Vec<_> = processes_in(root).iter().filter_map(|found| process_id_of(found.pid)).collect();
	end_processes(&ids, &[]).await;
}

/// The variant with its `--append-system-prompt` file copied into the trial root without a leading `<!-- … -->` block
/// (0577): Ponytail's license and file notes stay in the shipped file and out of every arm's system prompt.
fn with_prompt_copy(variant: &Variant, root: &Path) -> Result<Variant> {
	let Some(source) = &variant.append_system_prompt else { return Ok(variant.clone()) };
	let file = root.join("system-prompt.md");
	fs::write(&file, without_leading_comment(&String::from_utf8_lossy(&fs::read(source)?)))?;
	Ok(Variant { append_system_prompt: Some(file.display().to_string()), ..variant.clone() })
}

/// A trial's root: a short folder (`t` and 6 characters) in `work_dir` (default the OS temp folder), so deep dependency
/// trees stay under Windows' 260 characters. Nothing in it names the case: a benchmark task's id ends in its fix's PR
/// number, and agents list the root (0508); the trial's `setup.log` names the root.
pub fn make_trial_root(work_dir: Option<&Path>) -> Result<PathBuf> {
	let parent = work_dir.map_or_else(std::env::temp_dir, Path::to_path_buf);
	fs::create_dir_all(&parent)?;
	loop {
		let suffix = uuid::Uuid::new_v4().simple().to_string();
		let root = parent.join(format!("t{}", &suffix[..6]));
		match fs::create_dir(&root) {
			Ok(()) => return Ok(root),
			Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
			Err(error) => return Err(error.into()),
		}
	}
}

/// The trial's environment: `extra` (a variant's env) over this process's, with the case's [`EvalCase::path_prefix`]
/// folders under `root` first on PATH (Windows names it `Path`).
pub fn trial_env(eval_case: &EvalCase, root: &Path, extra: &HashMap<String, String>) -> HashMap<String, String> {
	let mut trial = process_env();
	trial.extend(extra.iter().map(|(name, value)| (name.clone(), value.clone())));
	trial.remove(A7_API_KEY_ENV);
	trial.remove("ANTHROPIC_API_KEY");
	let Some(prefix) = eval_case.path_prefix.as_ref().filter(|prefix| !prefix.is_empty()) else { return trial };
	let key = trial.keys().find(|name| name.to_uppercase() == "PATH").cloned().unwrap_or_else(|| "PATH".to_owned());
	let mut parts: Vec<String> = prefix
		.iter()
		.map(|dir| {
			dir.split('/')
				.filter(|part| !part.is_empty())
				.fold(root.to_path_buf(), |path, part| path.join(part))
				.display()
				.to_string()
		})
		.collect();
	parts.extend(trial.get(&key).cloned());
	parts.retain(|part| !part.is_empty());
	trial.insert(key, parts.join(if cfg!(windows) { ";" } else { ":" }));
	trial
}

/// Runs one trial's harness; past `timeout_ms` the whole process tree is ended (0318). Stdin is closed: omp's print
/// mode reads piped stdin to its end.
async fn run_harness(
	command: &str,
	args: &[String],
	cwd: &Path,
	env: &HashMap<String, String>,
	timeout_ms: Option<u64>,
) -> HarnessOutput {
	let started = Instant::now();
	let mut argv = vec![command.to_owned()];
	argv.extend(args.iter().cloned());
	let options = jocasta::shared::process::RunOptions {
		cwd: Some(cwd.to_path_buf()),
		env: Some(env.clone()),
		timeout_ms: timeout_ms.map(|limit| limit as i64),
		..Default::default()
	};
	let run = jocasta::shared::process::run_program(&argv, options).await;
	let wall_ms = (started.elapsed().as_secs_f64() * 1000.0).round() as u64;
	match run {
		Ok(run) => HarnessOutput {
			status: run.code,
			stdout: run.stdout,
			stderr: run.stderr,
			timed_out: run.timed_out,
			wall_ms,
		},
		Err(error) => {
			HarnessOutput { status: None, stdout: String::new(), stderr: error.to_string(), timed_out: false, wall_ms }
		}
	}
}
