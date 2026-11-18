//! H1, the guard benchmark (0482, 0500, 0501, 0535, 0585, 0600, 0623), ported from `bench/h1-guard/*.ts` (0636): the
//! corpus builder, the evaluator, the live check with its ACP recorder, the git guard check and the omp verdicts script.
//! The oracle itself, `omp-verdicts.ts` (omp's own code, loaded by Bun), stays TypeScript: `oracle` is its client.

use std::process::ExitCode;

use clap::Subcommand;

use crate::lib::args::{ScriptArgs, finish_script};

pub mod acp_recorder;
pub mod build_corpus;
pub mod evaluate;
pub mod git_guard;
pub mod git_guard_check;
pub mod live_check;
pub mod omp_verdicts;
pub mod oracle;

/// H1's scripts, named by their files.
#[derive(Subcommand)]
pub enum H1Guard {
	/// The guard's verdicts on a labelled command corpus, scored per group and per way of writing.
	#[command(disable_help_flag = true)]
	Evaluate(ScriptArgs),
	/// omp 18.20.2's own verdict on every corpus line, compared with the reimplementation (nothing runs).
	#[command(name = "omp-verdicts", disable_help_flag = true)]
	OmpVerdicts(ScriptArgs),
	/// Measures omp's git guard on the VM's fixture (`--run`) or prints the plan (`--dry-run`).
	#[command(name = "git-guard-check", disable_help_flag = true)]
	GitGuardCheck(ScriptArgs),
	/// Real omp, told by a cheap model to run random corpus commands, against the in-process verdicts.
	#[command(name = "live-check", disable_help_flag = true)]
	LiveCheck(ScriptArgs),
	/// Derives the frozen corpus from the collected source lines.
	#[command(name = "build-corpus", disable_help_flag = true)]
	BuildCorpus(ScriptArgs),
}

/// Runs `jocasta-bench h1-guard <script> …`.
pub fn run(command: H1Guard) -> ExitCode {
	let ran = |result: jocasta::shared::errors::Result<()>| finish_script(result.map(|()| ExitCode::SUCCESS));
	match command {
		H1Guard::Evaluate(script) => ran(evaluate::main(&script.args)),
		H1Guard::OmpVerdicts(script) => ran(omp_verdicts::main(&script.args)),
		H1Guard::GitGuardCheck(script) => ran(git_guard_check::main(&script.args)),
		H1Guard::LiveCheck(script) => ran(live_check::main(&script.args)),
		H1Guard::BuildCorpus(script) => build_corpus::main(&script.args),
	}
}
