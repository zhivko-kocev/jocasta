//! Runs Jocasta's evals (1.10) on his daily omp (0386): the repo's cases in `evals/cases/`, then his own in
//! `~/.jocasta/agent/evals/cases/`, each trial a real `omp -p <prompt> --mode json` run (or `claude -p` for a Claude
//! Code variant, 0482) that bills model usage, so it needs `JOCASTA_EVALS_REAL=1` and never runs in CI (0163, 0166,
//! 0169). The harness is resolved on PATH. 3 reps and a 10-minute limit per trial by default; `--reps <n>` and
//! `--timeout <minutes>` change them (0318). `--variant <name>`, repeatable, layers `evals/variants/<name>.yaml` (0375;
//! the arms A0–A4 of 0482 are `a0`…`a4`). `--profile <name>` runs every omp trial under that omp profile (0482 E8).
//! Before each trial the quota pacer reads `omp usage --json` and waits while every Anthropic account has its 5h or 7d
//! window at 85% or more (0482, 0543).
//! `--dry-run` prints the plan and runs nothing. Results go to `<data>/evals/runs/<time>-<label>/` (0166, 0375).
//! `jocasta-bench evals compare <runDirA> <runDirB>` applies the keep rule and the paired statistics (0375, 0482).
//! Exits 1 when a trial fails, 2 on a bad flag, variant or environment (0140).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use super::args::{RunSettings, parse_run_args, take_variant_flags};
use super::compare::compare_run_dirs;
use super::harness::Turn;
use super::pacer::{PAUSE_AT, RealPacerDeps, create_pacer};
use super::report::js_text;
use super::runner::{RunOptions, TrialOptions, load_all_cases, run_evals, trial_args};
use super::variants::{Harness, Variant, load_variants};
use crate::lib::args::REPO_ROOT;
use crate::lib::launch::{BenchTool, tool_command_with};
use crate::lib::output::iso_now;
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::normalize;
use jocasta::shared::paths::{config_dir, data_dir};
use jocasta::shared::process::on_path;

/// The eval scripts' error line: a refusal is `jocasta: <message>` with its code; any other error prints its message
/// and exits 1 (the TypeScript's uncaught error printed Bun's stack trace).
pub fn report_error(error: &Error) -> ExitCode {
	match error {
		Error::User { message, exit_code } => {
			eprintln!("jocasta: {message}");
			ExitCode::from(*exit_code)
		}
		other => {
			eprintln!("{other}");
			ExitCode::from(1)
		}
	}
}

/// The bench tool a variant's harness is.
pub fn bench_tool(harness: Harness) -> BenchTool {
	match harness {
		Harness::Omp => BenchTool::Omp,
		Harness::Claude => BenchTool::Claude,
		Harness::Opencode => BenchTool::Opencode,
	}
}

/// `evals/variants/` of the checkout.
pub fn variants_dir() -> PathBuf {
	REPO_ROOT.join("evals").join("variants")
}

fn cases_dirs() -> Vec<PathBuf> {
	vec![REPO_ROOT.join("evals").join("cases"), config_dir().join("evals").join("cases")]
}

/// `jocasta-bench evals compare <runDirA> <runDirB>`.
pub fn compare_main(argv: &[String]) -> ExitCode {
	let outcome = match argv {
		[a, b] => compare_run_dirs(Path::new(a), Path::new(b)),
		_ => Err(Error::user("usage: jocasta-bench evals compare <runDirA> <runDirB>")),
	};
	match outcome {
		Ok(text) => {
			println!("{text}");
			ExitCode::SUCCESS
		}
		Err(error) => report_error(&error),
	}
}

pub fn quote(arg: &str) -> String {
	if arg.chars().any(char::is_whitespace) { serde_json::to_string(arg).unwrap_or_default() } else { arg.to_owned() }
}

fn dry_run(settings: &RunSettings, variant: &Variant) -> Result<()> {
	let cases = load_all_cases(&cases_dirs())?;
	println!("evals dry run (0482): nothing runs and no model is called.");
	println!(
		"  variant {}, harness {}, {} cases × {} reps",
		variant.label,
		variant.harness,
		cases.len(),
		settings.reps
	);
	println!(
		"  limit {} min per trial; profile {}",
		settings.timeout_minutes.map_or_else(|| "none".to_owned(), js_text),
		settings.profile.as_deref().unwrap_or("his default")
	);
	println!(
		"  quota pacer: omp usage --json before each trial, waits while every account has its 5h or 7d window at {}% or more",
		js_text(PAUSE_AT * 100.0)
	);
	for case in &cases {
		println!("  case {}: {} checks, {} hidden", case.id, case.checks.len(), case.hidden_checks.len());
	}
	if let Some(first) = cases.first() {
		let prompts = std::iter::once(first.prompt.clone()).chain(variant.followups.iter().cloned());
		for (i, prompt) in prompts.enumerate() {
			let turn = Turn {
				prompt,
				follow_up: i > 0,
				overlay: "<trial>/eval-run.yml".to_owned(),
				profile: settings.profile.clone(),
				..Turn::default()
			};
			let args = trial_args(variant, &turn)?;
			println!(
				"  turn {}: {} {}",
				i + 1,
				variant.harness,
				args.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join(" ")
			);
		}
	}
	let results = normalize(&data_dir().join("evals").join("runs").join(format!("<time>-{}", variant.label)));
	println!("  results: {}", results.display());
	Ok(())
}

/// What a real run needs besides the settings: the checked environment and the harness's launcher.
pub fn launcher_of(variant: &Variant) -> Result<Vec<String>> {
	if env::var_nonempty("JOCASTA_EVALS_REAL").is_none() {
		return Err(Error::user(
			"evals run omp against real models and bill usage; set JOCASTA_EVALS_REAL=1 to run them",
		));
	}
	if env::var_nonempty("CI").is_some() {
		return Err(Error::user("real-model evals never run in CI (0169)"));
	}
	// The bench's launcher (EVL-14): npm's `.cmd` shim is started through its `.exe`, or `cmd.exe /d /c`.
	let name = variant.harness.as_str();
	if on_path(name, None).is_none() {
		return Err(Error::user(format!("{name} is not on PATH; install {name} or add it to PATH to run the evals")));
	}
	tool_command_with(bench_tool(variant.harness), &|name| on_path(name, None))
}

/// `jocasta-bench evals [--variant <name>]… [--reps <n>] [--timeout <minutes>] [--profile <name>] [--dry-run]`.
pub async fn main(argv: Vec<String>) -> ExitCode {
	match run(&argv).await {
		Ok(code) => code,
		Err(error) => report_error(&error),
	}
}

async fn run(argv: &[String]) -> Result<ExitCode> {
	let flags = take_variant_flags(argv)?;
	let settings = parse_run_args(&flags.rest)?;
	let variant = load_variants(&variants_dir(), &flags.variants)?;
	if settings.profile.is_some() && variant.harness != Harness::Omp {
		return Err(Error::user("--profile is omp's; a Claude Code variant can't take it"));
	}
	if settings.dry_run {
		dry_run(&settings, &variant)?;
		return Ok(ExitCode::SUCCESS);
	}
	let launcher = launcher_of(&variant)?;
	let out_dir =
		data_dir().join("evals").join("runs").join(format!("{}-{}", iso_now().replace([':', '.'], "-"), variant.label));
	let pacer = Arc::new(create_pacer(Arc::new(RealPacerDeps), Arc::new(Mutex::new(Vec::new()))));
	let options = RunOptions {
		trial: TrialOptions {
			out_dir: out_dir.clone(),
			timeout_ms: settings.timeout_minutes.map(|minutes| (minutes * 60_000.0) as u64),
			launcher,
			variant: Some(variant),
			profile: settings.profile.clone(),
			..TrialOptions::default()
		},
		cases_dir: cases_dirs(),
		out_dir: out_dir.clone(),
		reps: settings.reps,
		on_row: Some(Box::new(|row| {
			println!("{} {} rep {}", if row.passed { "pass" } else { "FAIL" }, row.case, row.rep);
		})),
		pace: Some(Arc::new(move || {
			let pacer = pacer.clone();
			Box::pin(async move { pacer.pace().await })
		})),
	};
	let rows = run_evals(options).await?;
	let passed = rows.iter().filter(|row| row.passed).count();
	println!("{passed}/{} passed; report: {}", rows.len(), normalize(&out_dir.join("report.html")).display());
	Ok(if passed < rows.len() { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
