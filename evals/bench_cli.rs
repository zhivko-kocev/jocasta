//! The task benchmark's command line (`evals/bench.ts`'s `main`): its flags, the plan they make, the dry run and the
//! checks of the omp folder before a run (0587).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use super::args::{number_of, profile_name};
use super::bench::{A7_SPEND_LIMIT_USD, Arm, BenchOptions, run_bench};
use super::bench_validate::{GOLD_GRADES, run_validation};
use super::followup::{
	FOLLOWUP_ARMS, FOLLOWUP_BASELINES, FOLLOWUP_COMPARISONS, FOLLOWUP_REPS, FOLLOWUP_TASKS, pick_followup_tasks,
};
use super::harness::{A7_API_KEY_ENV, Turn};
use super::layer::LAYER_MODULE;
use super::pacer::{PACER_PROVIDER, PAUSE_AT, Pause, RealPacerDeps, create_pacer};
use super::report::js_text;
use super::run::{bench_tool, quote, variants_dir};
use super::runner::trial_args;
use super::tasks::{BenchTask, TaskSet, load_tasks, pick_pilot, plan_trials};
use super::variants::{Harness, Variant, load_variants};
use super::watch::wait_for_start_floor;
use crate::lib::args::{
	ArgSpec, BenchArgs, BenchUsageError, FlagValue, REPO_ROOT, flag, flag_or, list_flag, parse_bench_args, run_script,
};
use crate::lib::launch::tool_command;
use crate::lib::output::read_version;
use jocasta::shared::env;
use jocasta::shared::errors::Result;
use jocasta::shared::pathkey::{is_inside, resolve};
use jocasta::shared::paths::omp_agent_dir;

/// The main run's arms (0543): A0–A4 as 0482 has them, A6 = A3 + the harness-OS layer, A7 = opencode bare.
pub const DEFAULT_ARMS: [&str; 7] = ["a0", "a1", "a2", "a3", "a4", "a6", "a7"];

fn whole_number(text: Option<&str>, name: &str, fallback: u64, min: u64) -> Result<u64> {
	let Some(text) = text else { return Ok(fallback) };
	let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
	match text.parse::<u64>() {
		Ok(value) if digits && value >= min => Ok(value),
		_ => Err(BenchUsageError::err(format!("--{name} needs a whole number of {min} or more, not \"{text}\""))),
	}
}

/// The layer's own MCP server's name in a layered arm's extension root (`src/frontier/layer/bench.rs`).
const LAYER_SERVER: &str = "jocasta";

fn strings(value: Option<&Value>) -> Vec<String> {
	match value {
		Some(Value::Array(items)) => items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect(),
		_ => Vec::new(),
	}
}

struct McpFile {
	file: PathBuf,
	servers: Map<String, Value>,
	disabled: Vec<String>,
}

/// 0587 as 0652 left it: why the layered arms can't run with omp's folder `agent_dir` (the profile's with `--profile`)
/// as it is, or `None`. The servers his `mcp.json` turns on load in every omp arm alike, as his skills do (0652). For the
/// `layered` arms, a `jocasta` entry there, even one turned off, or `jocasta` in `disabledServers` hides the layer's own
/// server: omp keeps its folder's over an extension's (`discovery/omp-plugins.ts`). A file that's missing or isn't JSON
/// gives omp nothing.
pub fn mcp_problem(agent_dir: &Path, layered: &[String]) -> Option<String> {
	if layered.is_empty() {
		return None;
	}
	let files: Vec<McpFile> = ["mcp.json", ".mcp.json"]
		.iter()
		.filter_map(|name| {
			let file = agent_dir.join(name);
			let config: Value = serde_json::from_str(&fs::read_to_string(&file).ok()?).ok()?;
			let config = config.as_object()?;
			Some(McpFile {
				file,
				servers: config.get("mcpServers").and_then(Value::as_object).cloned().unwrap_or_default(),
				disabled: strings(config.get("disabledServers")),
			})
		})
		.collect();
	let entry = files.iter().find(|each| each.servers.contains_key(LAYER_SERVER));
	let denied = files.iter().find(|each| each.disabled.iter().any(|name| name == LAYER_SERVER));
	let why = if let Some(entry) = entry {
		format!("{} has a \"{LAYER_SERVER}\" entry, which hides the layer's even turned off", entry.file.display())
	} else {
		let denied = denied?;
		format!("{} names \"{LAYER_SERVER}\" in disabledServers", denied.file.display())
	};
	Some(format!(
		"{} would lose the layer's own MCP server \"{LAYER_SERVER}\": {why}. Remove it or use a --profile without it; no trial ran.",
		layered.join(", ")
	))
}

/// `jocasta-bench evals bench …`; prints after `bench: ` on a bad flag (the harness's one usage-error line).
pub async fn main(argv: Vec<String>) -> ExitCode {
	let code = std::cell::Cell::new(0u8);
	let result = run_script(async {
		code.set(run(argv).await?);
		Ok(())
	})
	.await;
	if code.get() == 0 { result } else { ExitCode::from(code.get()) }
}

/// What a bench invocation chose, checked.
struct Plan {
	tasks: Vec<BenchTask>,
	arm_names: Vec<String>,
	variants: Vec<(String, Variant)>,
	layered: Vec<String>,
	reps: u64,
	seed: u32,
	workers: usize,
	timeout_minutes: f64,
	profile: Option<String>,
	pilot: bool,
	validate: bool,
	followup: bool,
	cache_dir: PathBuf,
	work_dir: Option<PathBuf>,
	out_dir: PathBuf,
}

impl Plan {
	fn variant(&self, name: &str) -> &Variant {
		&self.variants.iter().find(|(each, _)| each == name).expect("every arm's variant was loaded").1
	}
}

/// The flags `bench` takes besides `--out`, `--runs` and `--dry-run`.
fn bench_spec() -> ArgSpec {
	ArgSpec {
		default_runs: 3,
		valued: &[
			"--tasks",
			"--cache",
			"--arms",
			"--reps",
			"--set",
			"--study",
			"--only",
			"--workers",
			"--seed",
			"--timeout",
			"--profile",
			"--work",
		],
		booleans: &["--pilot", "--validate"],
		..ArgSpec::default()
	}
}

fn plan_of(argv: &[String]) -> Result<(Plan, BenchArgs)> {
	if argv.iter().any(|arg| arg == "--runs") {
		return Err(BenchUsageError::err("unknown option: --runs (use --reps)"));
	}
	let args = parse_bench_args(argv, &bench_spec())?;
	let (Some(tasks_file), Some(cache_text), Some(out_dir)) =
		(flag(&args, "tasks"), flag(&args, "cache"), args.out.clone())
	else {
		return Err(BenchUsageError::err("--tasks <tasks.jsonl>, --cache <dir> and --out <runDir> are required"));
	};
	let cache_dir = resolve(Path::new(cache_text));
	if is_inside(&REPO_ROOT, &cache_dir) {
		return Err(BenchUsageError::err(format!("--cache must be outside the repo, not {}", cache_dir.display())));
	}
	let work_dir = flag(&args, "work").map(|text| resolve(Path::new(text)));
	if let Some(work_dir) = &work_dir
		&& is_inside(&REPO_ROOT, work_dir)
	{
		return Err(BenchUsageError::err(format!("--work must be outside the repo, not {}", work_dir.display())));
	}
	let pilot = args.flags.get("pilot") == Some(&FlagValue::Set);
	let validate = args.flags.get("validate") == Some(&FlagValue::Set);
	let study = flag_or(&args, "study", "main");
	if study != "main" && study != "followup" {
		return Err(BenchUsageError::err(format!("--study must be main or followup, not \"{study}\"")));
	}
	let followup = study == "followup";
	if followup {
		for name in ["arms", "reps", "set", "only"] {
			if flag(&args, name).is_some() {
				return Err(BenchUsageError::err(format!("--study followup fixes its own --{name}; drop it")));
			}
		}
		if pilot {
			return Err(BenchUsageError::err("--study followup has no pilot; drop --pilot"));
		}
	}
	if pilot && flag(&args, "reps").is_some() {
		return Err(BenchUsageError::err("--pilot runs 1 rep; drop --reps"));
	}
	let reps = if followup {
		FOLLOWUP_REPS
	} else if pilot {
		1
	} else {
		whole_number(flag(&args, "reps"), "reps", 3, 1)?
	};
	let workers = whole_number(flag(&args, "workers"), "workers", 2, 1)? as usize;
	let seed = whole_number(flag(&args, "seed"), "seed", 482, 0)? as u32;
	let timeout_text = flag_or(&args, "timeout", "90");
	let timeout_minutes = number_of(timeout_text);
	if !timeout_minutes.is_finite() || timeout_minutes <= 0.0 {
		return Err(BenchUsageError::err(format!(
			"--timeout needs a number of minutes above 0, not \"{timeout_text}\""
		)));
	}
	let set = flag_or(&args, "set", "main");
	if !matches!(set, "main" | "spare" | "all") {
		return Err(BenchUsageError::err(format!("--set must be main, spare or all, not \"{set}\"")));
	}
	let profile = flag(&args, "profile").map(profile_name).transpose()?;
	let arm_names: Vec<String> = if followup {
		FOLLOWUP_ARMS.iter().map(|name| (*name).to_owned()).collect()
	} else {
		list_flag(&args, "arms", &DEFAULT_ARMS)
	};
	let mut variants: Vec<(String, Variant)> = Vec::new();
	for name in &arm_names {
		variants.push((name.clone(), load_variants(&variants_dir(), std::slice::from_ref(name))?));
	}
	if arm_names.is_empty() {
		return Err(BenchUsageError::err("--arms needs at least one arm"));
	}
	let all = load_tasks(&resolve(Path::new(tasks_file)))?;
	let only = list_flag(&args, "only", &[]);
	for id in &only {
		if !all.iter().any(|task| task.instance_id == *id) {
			return Err(BenchUsageError::err(format!("no task {id} in {tasks_file}")));
		}
	}
	let wanted = |task: &BenchTask| {
		set == "all" || matches!((set, task.set), ("main", TaskSet::Main) | ("spare", TaskSet::Spare))
	};
	let mut tasks: Vec<BenchTask> = if only.is_empty() {
		all.iter().filter(|task| wanted(task)).cloned().collect()
	} else {
		all.iter().filter(|task| only.contains(&task.instance_id)).cloned().collect()
	};
	if followup {
		tasks = pick_followup_tasks(&all, seed)?;
	}
	if pilot {
		tasks = pick_pilot(&tasks, seed)?;
	}
	if tasks.is_empty() {
		return Err(BenchUsageError::err("no tasks selected"));
	}
	// Each layered arm needs Jocasta's layer; it is linked into this binary, so no checkout lacks it.
	let layered: Vec<String> =
		variants.iter().filter(|(_, variant)| variant.layer).map(|(name, _)| name.clone()).collect();
	let plan = Plan {
		tasks,
		arm_names,
		variants,
		layered,
		reps,
		seed,
		workers,
		timeout_minutes,
		profile,
		pilot,
		validate,
		followup,
		cache_dir,
		work_dir,
		out_dir,
	};
	Ok((plan, args))
}

async fn run(argv: Vec<String>) -> Result<u8> {
	let (plan, args) = plan_of(&argv)?;
	// 0587 as 0652 left it: omp's folder doesn't hide a layered arm's own MCP server (dry run too).
	let omp_arms: Vec<String> =
		plan.arm_names.iter().filter(|name| plan.variant(name).harness == Harness::Omp).cloned().collect();
	if !plan.validate && !omp_arms.is_empty() {
		let layered: Vec<String> = omp_arms.iter().filter(|name| plan.layered.contains(name)).cloned().collect();
		if let Some(problem) = mcp_problem(&omp_agent_dir(plan.profile.as_deref()), &layered) {
			return Err(BenchUsageError::err(problem));
		}
	}
	if args.dry_run {
		print_dry_run(&plan)?;
		return Ok(0);
	}
	let log = |line: &str| println!("{line}");
	if plan.validate {
		let failed =
			run_validation(&plan.tasks, &plan.cache_dir, &plan.out_dir, &log, plan.work_dir.as_deref()).await?;
		if !failed.is_empty() {
			return Ok(1);
		}
		return Ok(0);
	}
	if env::var_nonempty("JOCASTA_EVALS_REAL").is_none() {
		return Err(BenchUsageError::err(
			"the benchmark runs real models and bills usage; set JOCASTA_EVALS_REAL=1 to run it",
		));
	}
	if env::var_nonempty("CI").is_some() {
		return Err(BenchUsageError::err("real-model runs never run in CI (0169)"));
	}
	let a7: Vec<String> =
		plan.arm_names.iter().filter(|name| plan.variant(name).harness == Harness::Opencode).cloned().collect();
	if !a7.is_empty() && env::var_nonempty(A7_API_KEY_ENV).is_none() {
		return Err(BenchUsageError::err(format!(
			"{} needs A7's Anthropic API key in {A7_API_KEY_ENV} (0537)",
			a7.join(", ")
		)));
	}
	let mut arms: Vec<Arm> = Vec::new();
	for name in &plan.arm_names {
		let variant = plan.variant(name).clone();
		let launcher = tool_command(bench_tool(variant.harness))?;
		let version = read_version(&launcher).map_err(|error| BenchUsageError::err(format!("arm {name}: {error}")))?;
		arms.push(Arm { name: name.clone(), variant, launcher, harness_version: version });
	}
	let pauses: Arc<Mutex<Vec<Pause>>> = Arc::default();
	let pacer = Arc::new(create_pacer(Arc::new(RealPacerDeps), pauses.clone()));
	let full = pacer.clone();
	let mut options =
		BenchOptions::new(plan.tasks.clone(), plan.cache_dir.clone(), plan.out_dir.clone(), arms, plan.reps);
	options.seed = plan.seed;
	options.workers = plan.workers;
	options.timeout_ms = Some((plan.timeout_minutes * 60_000.0) as u64);
	options.profile = plan.profile.clone();
	options.pilot = plan.pilot;
	options.work_dir = plan.work_dir.clone();
	// The start floor rides with the pacer so the tests' runs never wait on the machine's memory.
	options.pace = Some(Arc::new(move || {
		let pacer = pacer.clone();
		Box::pin(async move {
			pacer.pace().await;
			wait_for_start_floor().await;
		})
	}));
	options.account_full = Some(Arc::new(move || full.account_full()));
	options.pauses = Some(pauses);
	// 0574: a harness that updated itself mid-run stops its arm.
	options.version_of = Some(Arc::new(|arm| read_version(&arm.launcher)));
	let code = run_bench(options).await?;
	println!("results: {}", plan.out_dir.join("results.jsonl").display());
	Ok(code)
}

/// One arm's lines of the dry run: each turn's command and what the arm adds.
fn print_arm(plan: &Plan, name: &str) -> Result<()> {
	let variant = plan.variant(name);
	let turns: Vec<String> = std::iter::once("<prompt>".to_owned())
		.chain((0..variant.followups.len()).map(|i| format!("<follow-up {}>", i + 1)))
		.collect();
	for (i, prompt) in turns.into_iter().enumerate() {
		let turn = Turn {
			prompt,
			follow_up: i > 0,
			overlay: "<trial>/eval-run.yml".to_owned(),
			profile: plan.profile.clone(),
			session_dir: Some("<trial>/sessions".to_owned()),
			session: Some("<session>".to_owned()),
			extension: (variant.plan_review.is_some() && i == 0).then(|| "<trial>/plan-review.js".to_owned()),
		};
		let args = trial_args(variant, &turn)?;
		println!(
			"  arm {name} turn {}: {} {}",
			i + 1,
			variant.harness,
			args.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join(" ")
		);
	}
	if let Some(choice) = variant.plan_review {
		println!(
			"  arm {name} plan review (0567): turn 1 runs omp interactive in a pty, plan mode at startup; the extension answers \"{}\"{} and shuts omp down after the execution",
			if choice == super::plan_review::PlanReviewChoice::Execute {
				"Approve and execute"
			} else {
				"Approve and keep context"
			},
			variant
				.plan_review_role
				.as_ref()
				.map_or_else(String::new, |role| format!(" with the work on the {role} role"))
		);
	}
	if variant.layer {
		println!("  arm {name} layer: {LAYER_MODULE} adds its flags, settings and env");
	}
	if variant.harness == Harness::Opencode {
		println!(
			"  arm {name}: Anthropic API key from {A7_API_KEY_ENV} (as ANTHROPIC_API_KEY, opencode only), stops at ${} priced spend (0537)",
			js_text(A7_SPEND_LIMIT_USD)
		);
	}
	Ok(())
}

fn print_dry_run(plan: &Plan) -> Result<()> {
	println!(
		"task benchmark (0482, 0543{}) dry run: nothing is fetched, prepared or run, and no model is called.",
		if plan.followup { ": the follow-up study" } else { "" }
	);
	println!(
		"  tasks: {}{}{}",
		plan.tasks.len(),
		if plan.pilot {
			format!(" (pilot, seed {}: 2 Python, 2 TypeScript/JavaScript, 1 Go)", plan.seed)
		} else {
			String::new()
		},
		if plan.followup {
			format!(" ({FOLLOWUP_TASKS} main tasks, seed {}, languages in the main set's proportions)", plan.seed)
		} else {
			String::new()
		}
	);
	for task in &plan.tasks {
		let language = serde_json::to_value(task.language).ok().and_then(|v| v.as_str().map(str::to_owned));
		let set = serde_json::to_value(task.set).ok().and_then(|v| v.as_str().map(str::to_owned));
		println!("    {} ({}, {})", task.instance_id, language.unwrap_or_default(), set.unwrap_or_default());
	}
	if plan.validate {
		println!(
			"  validate: per task, the hidden tests on the base commit (must fail), then with gold.patch {GOLD_GRADES} times (must pass)"
		);
	} else {
		let order = plan_trials(&plan.tasks, &plan.arm_names, plan.reps, plan.seed);
		println!(
			"  trials: {} = {} tasks × {} arms × {} rep{}, seed {}, {} at once, {} min limit each",
			order.len(),
			plan.tasks.len(),
			plan.arm_names.len(),
			plan.reps,
			if plan.reps == 1 { "" } else { "s" },
			plan.seed,
			plan.workers,
			js_text(plan.timeout_minutes)
		);
		if plan.followup {
			println!(
				"  comparisons, each paired with its baseline ({}) by task and rep, kept by 0543's rule (`jocasta-bench evals keep`):",
				FOLLOWUP_BASELINES.join(" or ")
			);
			for c in &FOLLOWUP_COMPARISONS {
				println!("    {} vs {}: {} ({})", c.arm, c.baseline, c.change, c.records);
			}
			println!("    not compared: pitchfork (0560), keep-alive pings (no idle gap in a trial)");
		}
		println!("  order (first {}):", order.len().min(15));
		for trial in order.iter().take(15) {
			println!("    {}. {} {} rep {}", trial.index + 1, trial.task, trial.arm, trial.rep);
		}
		for name in &plan.arm_names {
			print_arm(plan, name)?;
		}
		println!(
			"  quota pacer: omp usage --json before each trial, waits only while every {PACER_PROVIDER} account has its 5h or 7d window at {}% or more; a trial that ended on a rate limit while an account is full is an infra error, retried",
			js_text(PAUSE_AT * 100.0)
		);
		println!(
			"  ledger: each trial's sessions saved to <out>/sessions/, priced with bench/prices/ (LiteLLM, pinned), rows in ledger.jsonl, totals in each results row, spend per attempt in spend.jsonl"
		);
		println!("  profile: {} (omp arms)", plan.profile.as_deref().unwrap_or("his default"));
		if !plan.followup && !plan.pilot {
			println!(
				"  follow-up study (after the main run, --study followup): {FOLLOWUP_TASKS} tasks × {FOLLOWUP_REPS} reps × {} arms ({}) = {} trials",
				FOLLOWUP_ARMS.len(),
				FOLLOWUP_ARMS.join(", "),
				FOLLOWUP_TASKS as u64 * FOLLOWUP_REPS * FOLLOWUP_ARMS.len() as u64
			);
		}
	}
	println!("  cache: {}", plan.cache_dir.display());
	println!(
		"  trial roots: {}",
		plan.work_dir.as_ref().map_or_else(
			|| format!("{} (the OS temp folder; --work sets a short root)", std::env::temp_dir().display()),
			|dir| dir.display().to_string()
		)
	);
	println!("  results: {}", plan.out_dir.display());
	Ok(())
}
