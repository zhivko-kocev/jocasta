//! `jocasta-bench`: the benchmark harness (bench/, H1–H9) and the eval runner (evals/), ported to Rust as the
//! package's second binary, built from the product's own modules through the `jocasta` library (0636). H1's oracle
//! stays TypeScript on Bun. Each documented script is one subcommand named by its path (`h2-toast`, `h1-guard
//! evaluate`, `evals compare`, …).

// `mod lib;` is the harness library's place beside `bench/lib/*.ts` (0629); the name is only special to the library target.
#![allow(special_module_name)]

use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand};

mod lib;

use lib::args::{ScriptArgs, finish_script, run_script};

#[path = "../evals/mod.rs"]
mod evals;

mod footprint;
#[path = "h1-guard/mod.rs"]
mod h1_guard;
mod h2_toast;
mod h3_recovery;
mod h4_isolation;
mod h5_startup;
mod h6_memory;
mod h7_longpath;
mod h8_parallel;
mod h9_footprint;
mod tokscale_parity;

/// The benchmark harness and the eval runner.
#[derive(Parser)]
#[command(name = "jocasta-bench", version, arg_required_else_help = true)]
struct Cli {
	#[command(subcommand)]
	command: Option<Command>,
}

/// One subcommand per documented script, named by its path.
#[derive(Subcommand)]
enum Command {
	/// H1, the guard benchmark.
	#[command(name = "h1-guard", subcommand_required = true, arg_required_else_help = true)]
	H1Guard {
		#[command(subcommand)]
		command: h1_guard::H1Guard,
	},
	/// The ledger's parser check against tokscale (the VM only).
	#[command(name = "tokscale-parity", disable_help_flag = true)]
	TokscaleParity(ScriptArgs),
	/// H2 (0482, 0552, 0576): the time from "needs you" to his notification, in a sandbox Jocasta.
	#[command(name = "h2-toast", disable_help_flag = true)]
	H2Toast(ScriptArgs),
	/// H3 (0482): crash recovery.
	#[command(name = "h3-recovery", disable_help_flag = true)]
	H3Recovery(ScriptArgs),
	/// H4 (0482, 0576): memory isolation between projects.
	#[command(name = "h4-isolation", disable_help_flag = true)]
	H4Isolation(ScriptArgs),
	/// H5 (0482): startup time.
	#[command(name = "h5-startup", disable_help_flag = true)]
	H5Startup(ScriptArgs),
	/// H6 (0482): memory.
	#[command(name = "h6-memory", disable_help_flag = true)]
	H6Memory(ScriptArgs),
	/// H7 (0482): long paths.
	#[command(name = "h7-longpath", disable_help_flag = true)]
	H7Longpath(ScriptArgs),
	/// H8 (0482): parallel isolation.
	#[command(name = "h8-parallel", disable_help_flag = true)]
	H8Parallel(ScriptArgs),
	/// H9 (0482): install footprint.
	#[command(name = "h9-footprint", disable_help_flag = true)]
	H9Footprint(ScriptArgs),
	/// The footprint budget (0674): Jocasta's own processes, memory, CPU, startup and binary against a budget that
	/// only goes down.
	#[command(disable_help_flag = true)]
	Footprint(ScriptArgs),
	/// Runs the evals on his daily omp; the subcommands are the other eval scripts.
	Evals(EvalsArgs),
}

/// `evals` runs the arms runner with its own flags (`--variant`, `--reps`, `--timeout`, `--profile`, `--dry-run`, parsed
/// as the scripts parse them, so their errors stay); its subcommands are the other eval scripts.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct EvalsArgs {
	#[command(subcommand)]
	command: Option<EvalsCommand>,
	/// The arms runner's flags.
	#[arg(allow_hyphen_values = true, trailing_var_arg = true, num_args = 0..)]
	run: Vec<String>,
}

#[derive(Subcommand)]
enum EvalsCommand {
	/// Applies the keep rule and the paired statistics to two run folders.
	Compare(ScriptArgs),
	/// The task benchmark.
	Bench(ScriptArgs),
	/// The keep rule over a follow-up study's results.
	#[command(disable_help_flag = true)]
	Keep(ScriptArgs),
	/// A5, his case study: `record` or `report`.
	A5(ScriptArgs),
	/// The settings trials (fork plan section 4): `enumerate`, `replay`, `screen`, `confirm` or `report`, each with
	/// `--dir <run>`.
	#[command(disable_help_flag = true)]
	Settings(ScriptArgs),
}

#[tokio::main]
async fn main() -> ExitCode {
	// A stand-in harness a test started (`evals/stand_in.rs`), before anything else of the real entry point.
	#[cfg(feature = "test-support")]
	{
		let argv: Vec<String> = std::env::args().skip(1).collect();
		if let Some(code) = evals::stand_in::run(&argv) {
			return ExitCode::from(u8::try_from(code).unwrap_or(1));
		}
	}
	jocasta::shared::env::use_real_environment();
	// The footprint bench's stub agent is this binary named `omp`.
	if let Some(code) = footprint::stub_agent() {
		return code;
	}
	match Cli::parse().command {
		Some(Command::H1Guard { command }) => h1_guard::run(command),
		Some(Command::TokscaleParity(script)) => finish_script(tokscale_parity::main(&script.args)),
		Some(Command::H2Toast(script)) => h2_toast::main(script.args).await,
		Some(Command::H3Recovery(script)) => h3_recovery::main(script.args).await,
		Some(Command::H4Isolation(script)) => h4_isolation::main(script.args).await,
		Some(Command::Footprint(script)) => footprint::main(script.args).await,
		Some(Command::H5Startup(script)) => h5_startup::main(script.args).await,
		Some(Command::H6Memory(script)) => h6_memory::main(script.args).await,
		Some(Command::H7Longpath(script)) => h7_longpath::main(script.args).await,
		Some(Command::H8Parallel(script)) => h8_parallel::main(script.args).await,
		Some(Command::H9Footprint(script)) => h9_footprint::main(script.args).await,
		Some(Command::Evals(evals)) => match evals.command {
			Some(EvalsCommand::Compare(script)) => evals::run::compare_main(&script.args),
			Some(EvalsCommand::Bench(script)) => evals::bench_cli::main(script.args).await,
			Some(EvalsCommand::Keep(script)) => run_script(evals::keep::run(&script.args)).await,
			Some(EvalsCommand::A5(script)) => evals::a5::main(script.args).await,
			Some(EvalsCommand::Settings(script)) => evals::settings::main(script.args).await,
			None => evals::run::main(evals.run).await,
		},
		None => {
			let _ = Cli::command().print_help();
			ExitCode::from(2)
		}
	}
}
