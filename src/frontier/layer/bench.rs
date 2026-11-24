//! The harness-OS layer for the benchmark's A6 arm (0543, 0567), built from Jocasta's own parts so a trial gets what a
//! thread gets, without a Jocasta service or herdr (the Rust port of `bench.ts`):
//!
//! - the task file's brief and Done-when appended to the system prompt ([`task_prompt`], 0540), after the variant's own
//!   prompt file (omp keeps only the last `--append-system-prompt`), titled as New thread titles a brief, with the
//!   checkout's folder as the project (0577);
//! - the bridge extension (0675) as the trial's only extension (`-e <config>/bridge-extension.ts`), connected to a bridge
//!   that answers from this process: `report_result` (0539), `check` (0542, 0546, 0571) and
//!   `history_search` (0604), which has no past sessions to search in a trial and answers as for an empty history;
//! - the check runner on the trial's checkout, the project's own mise check as the product decides it.
//!
//! The ledger is the bench's own, from the trial's saved sessions (`src/shared/ledger.rs`). The layer's data (checks,
//! the bridge's name) lives in a per-trial folder under the OS temp folder and goes on [`LayerSetup::close`].
//!
//! The types are here, not in `evals/layer.rs` as the TypeScript had them: the library can't import from the bench's
//! binary, which uses them from this module.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};

use crate::frontier::server::boss::PANE_KEY_ENV;
use crate::frontier::server::bridge::{Bridge, BridgeService, PaneAgent, PaneEvent, Resolved, Role};
use crate::frontier::server::checks::{CheckRunner, CheckRunnerOptions, CheckSpec, checks_dir, gate_reply, short_tree};
use crate::frontier::server::history::{HISTORY_SEARCH, history_answer};
use crate::frontier::server::mise::{Env, Mise, MiseCheck, MiseCheckTask, mise_check, skipped_said};
use crate::frontier::server::report::{REPORT_RESULT, parse_report, report_line};
use crate::frontier::server::task_file::{PromptFields, task_prompt};
use crate::frontier::server::tool_specs::{ToolSpec, tools_for};
use crate::frontier::server::tools::{Caller, ToolReply};
use crate::frontier::server::wording::brief_title;
use crate::shared::bridge_wire::{BRIDGE_ENV, BRIDGE_EXTENSION};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Context, Error, Result};
use crate::shared::fsx::{free_port, remove_tree};
use crate::shared::install::shipped_config_dir;
use crate::shared::jsonl::append_jsonl;
use crate::shared::process::AbortSignal;
use crate::shared::push_hook::{PushAnswer, PushRequest};

/// The project id the layer's parts know every trial by.
const PROJECT_ID: &str = "bench";

/// One trial as the layer sees it.
#[derive(Debug, Clone)]
pub struct LayerTrial {
	/// The trial's root (outside the repo): the layer's prompt file and omp extension go there.
	pub root: PathBuf,
	/// The trial's checkout, where the harness runs.
	pub repo: PathBuf,
	/// The fixture commit the task starts from.
	pub base_commit: String,
	/// The task prompt every arm gets.
	pub prompt: String,
	/// The variant's `--append-system-prompt` file (Ponytail), absolute: omp keeps only the last one given, so the
	/// layer's own prompt file starts with it, as a thread's launch prompt does (0540).
	pub append_system_prompt: Option<PathBuf>,
	pub env: Env,
}

/// What stops what `set_up` started and removes its data.
type Closer = Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>;

/// What the layer adds to a trial's omp, and how it is taken down.
pub struct LayerSetup {
	/// omp flags added after the variant's (e.g. `--append-system-prompt <task.md>`, the MCP endpoint's config).
	pub args: Vec<String>,
	/// omp settings deep-merged into the trial's `--config` overlay.
	pub settings: Map<String, Value>,
	pub env: HashMap<String, String>,
	closer: Option<Closer>,
}

impl LayerSetup {
	/// Stops what `set_up` started and removes its data; awaited after the last turn, also when the trial fails. A
	/// second call does nothing.
	pub async fn close(&mut self) {
		if let Some(closer) = self.closer.take() {
			closer().await;
		}
	}
}

/// The trial's layer (the TypeScript's `BenchLayer`).
pub trait BenchLayer: Send + Sync {
	fn set_up(&self, trial: LayerTrial) -> BoxFuture<'_, Result<LayerSetup>>;
}

/// Jocasta's layer, the TypeScript's `benchLayer`.
pub struct JocastaBenchLayer;

/// Jocasta's layer for a trial.
pub const BENCH_LAYER: JocastaBenchLayer = JocastaBenchLayer;

impl BenchLayer for JocastaBenchLayer {
	fn set_up(&self, trial: LayerTrial) -> BoxFuture<'_, Result<LayerSetup>> {
		Box::pin(set_up(trial))
	}
}

/// What one trial's layer keeps while it runs.
struct Trial {
	repo: PathBuf,
	/// The checkout's folder name: the project's name in replies, as a thread's project is named.
	project: String,
	env: Env,
	checks: CheckRunner,
	reports: PathBuf,
}

/// What `set_up` has started so far, for `close` to take down.
struct Running {
	data: PathBuf,
	trial: Option<Arc<Trial>>,
	bridge: Option<(Bridge, Arc<dyn BridgeService>)>,
}

impl Running {
	async fn close(self) {
		if let Some((bridge, _)) = &self.bridge {
			bridge.stop();
		}
		if let Some(trial) = &self.trial {
			trial.checks.stop().await;
		}
		let _ = remove_tree(&self.data);
	}
}

/// `N` random bytes as lowercase hex (the TypeScript's `randomBytes(N).toString("hex")`).
fn random_hex<const N: usize>() -> Result<String> {
	let mut bytes = [0_u8; N];
	getrandom::fill(&mut bytes).map_err(|error| Error::other(format!("no random bytes: {error}")))?;
	Ok(bytes.iter().fold(String::new(), |mut text, byte| {
		let _ = write!(text, "{byte:02x}");
		text
	}))
}

/// A new folder for one trial's layer data, under the OS temp folder (inside the user's profile on Windows).
fn layer_data_dir() -> Result<PathBuf> {
	let dir = std::env::temp_dir().join(format!("jl{}", random_hex::<4>()?));
	std::fs::create_dir(&dir).context(format!("making {}", dir.display()))?;
	Ok(dir)
}

/// `error` as the answer's text.
fn said(error: impl std::fmt::Display) -> String {
	error.to_string()
}

async fn check(trial: &Trial) -> std::result::Result<String, String> {
	let none = |why: &str| format!("No checks: {why}, and the task has no done_when.");
	let task = match mise_check(PROJECT_ID, &trial.repo, Some(&trial.env), Mise::OnPath).await {
		None => return Ok(none("the project has no trusted mise verify or test task")),
		Some(MiseCheck::Skipped(why)) => return Ok(none(&skipped_said(&why))),
		Some(MiseCheck::Task(MiseCheckTask::Verify)) => "verify",
		Some(MiseCheck::Task(MiseCheckTask::Test)) => "test",
	};
	let spec = CheckSpec { name: task.to_owned(), command: format!("mise run -c {task}"), mise: true };
	let run = trial.checks.run(&trial.repo, &[spec], &trial.env, None).await.map_err(said)?;
	let Some(run) = run else { return Err("check: the checks couldn't run here.".to_owned()) };
	let reply = gate_reply(&run);
	Ok(if reply.is_empty() { format!("checks: PASS (tree {})", short_tree(&run.tree)) } else { reply })
}

async fn answer(trial: &Trial, tool: &str, args: Map<String, Value>) -> std::result::Result<String, String> {
	match tool {
		REPORT_RESULT => {
			let report = Value::Object(args);
			let parsed = parse_report(&report)?;
			append_jsonl(&trial.reports, &json!({ "at": now_ms(), "value": report })).map_err(said)?;
			Ok(format!("Report stored: {}", report_line(&parsed)))
		}
		"check" => check(trial).await,
		// A trial has no past sessions: the tool answers as it does for a project with nothing in its history (0604).
		HISTORY_SEARCH => Ok(history_answer(&trial.project, &[])),
		_ => Err(format!("no tool {tool}.")),
	}
}

/// What the layer's bridge answers in this process for the trial's omp: the tools of a thread's pane (0675), with the
/// results the service's tools give.
struct TrialService {
	trial: Arc<Trial>,
	key: String,
}

impl BridgeService for TrialService {
	fn resolve(&self, key: &str) -> Option<Resolved> {
		(key == self.key).then(|| Resolved {
			pane_id: PROJECT_ID.to_owned(),
			role: Role::Thread,
			caller: Caller::default(),
		})
	}

	fn tools(&self, role: Role) -> Vec<Value> {
		tools_for(role, true).iter().map(ToolSpec::to_value).collect()
	}

	fn call(&self, _who: Resolved, tool: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move {
			match answer(&self.trial, &tool, args).await {
				Ok(text) => ToolReply { text, is_error: None },
				Err(text) => ToolReply { text, is_error: Some(true) },
			}
		})
	}

	fn cli(&self, command: String, _args: Map<String, Value>) -> BoxFuture<'_, ToolReply> {
		Box::pin(async move { ToolReply { text: format!("no command {command}."), is_error: Some(true) } })
	}

	fn push(&self, _request: PushRequest) -> BoxFuture<'_, PushAnswer> {
		Box::pin(async { PushAnswer { allow: false, reason: Some("a trial pushes nothing".to_owned()) } })
	}

	fn stop(&self, _pane_key: String, _signal: AbortSignal) -> BoxFuture<'_, String> {
		Box::pin(async { String::new() })
	}

	fn event(&self, _who: &Resolved, _event: &PaneEvent, _agent: &PaneAgent) {}
}

async fn start(trial: &LayerTrial, running: &mut Running, layer: &Path) -> Result<LayerSetup> {
	let state = Arc::new(Trial {
		repo: trial.repo.clone(),
		project: trial.repo.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
		env: trial.env.clone(),
		checks: CheckRunner::new(CheckRunnerOptions {
			dir: checks_dir(&running.data),
			..CheckRunnerOptions::default()
		}),
		reports: layer.join("reports.jsonl"),
	});
	running.trial = Some(state.clone());
	// The trial's omp loads the same extension a pane does, and its bridge answers from this process (0675). The
	// bridge's name is made from a port number nothing needs to listen on.
	let key = random_hex::<24>()?;
	let service: Arc<dyn BridgeService> = Arc::new(TrialService { trial: state.clone(), key: key.clone() });
	let bridge = Bridge::new();
	bridge.listen(free_port()?, &random_hex::<24>()?, &service)?;
	let address = bridge.address().unwrap_or_default();
	running.bridge = Some((bridge, service));
	let title = brief_title(&trial.prompt);
	let prompt = layer.join("prompt.md");
	let fields = PromptFields { title: &title, project: &state.project, brief: &trial.prompt, done_when: &[] };
	std::fs::write(&prompt, task_prompt(&fields, trial.append_system_prompt.as_deref(), true))?;
	let env = HashMap::from([
		("JOCASTA_DATA_DIR".to_owned(), running.data.display().to_string()),
		(BRIDGE_ENV.to_owned(), address),
		(PANE_KEY_ENV.to_owned(), key),
	]);
	Ok(LayerSetup {
		args: vec![
			"--append-system-prompt".to_owned(),
			prompt.display().to_string(),
			"-e".to_owned(),
			shipped_config_dir(None).join(BRIDGE_EXTENSION).display().to_string(),
		],
		settings: Map::new(),
		env,
		closer: None,
	})
}

async fn set_up(trial: LayerTrial) -> Result<LayerSetup> {
	let layer = trial.root.join("layer");
	std::fs::create_dir_all(&layer)?;
	let mut running = Running { data: layer_data_dir()?, trial: None, bridge: None };
	match start(&trial, &mut running, &layer).await {
		Ok(mut setup) => {
			setup.closer = Some(Box::new(move || Box::pin(running.close())));
			Ok(setup)
		}
		Err(error) => {
			running.close().await;
			Err(error)
		}
	}
}

#[cfg(test)]
#[path = "bench_tests.rs"]
mod tests;
