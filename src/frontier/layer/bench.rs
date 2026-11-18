//! The harness-OS layer for the benchmark's A6 arm (0543, 0567), built from Jocasta's own parts so a trial gets what a
//! thread gets, without a Jocasta service or herdr (the Rust port of `bench.ts`):
//!
//! - the task file's brief and Done-when appended to the system prompt ([`task_prompt`], 0540), after the variant's own
//!   prompt file (omp keeps only the last `--append-system-prompt`), titled as New thread titles a brief, with the
//!   checkout's folder as the project (0577);
//! - `jocasta mcp` as the trial's MCP server "jocasta", through an omp extension root (`-e <root>/layer/omp`, whose
//!   `.mcp.json` omp reads), answering from this process: `report_result` (0539), `check` (0542, 0546, 0571) and
//!   `history_search` (0604), which has no past sessions to search in a trial and answers as for an empty history;
//! - the check runner on the trial's checkout, the project's own mise check as the product decides it.
//!
//! The ledger is the bench's own, from the trial's saved sessions (`src/shared/ledger.rs`). The layer's data (checks,
//! `jocasta-service.json`) lives in a per-trial folder under the OS temp folder and goes on [`LayerSetup::close`].
//!
//! The types are here, not in `evals/layer.rs` as the TypeScript had them: the library can't import from the bench's
//! binary, which uses them from this module.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};
use url::Url;

use crate::frontier::server::checks::{CheckRunner, CheckRunnerOptions, CheckSpec, checks_dir, gate_reply, short_tree};
use crate::frontier::server::door_routes::decode_uri_component;
use crate::frontier::server::history::{HISTORY_SEARCH, history_answer};
use crate::frontier::server::mise::{Env, Mise, MiseCheck, MiseCheckTask, mise_check, skipped_said};
use crate::frontier::server::report::{REPORT_RESULT, parse_report, report_line};
use crate::frontier::server::routes::{Route, Served, ServerHandle, has_bearer, json_response, listen, text_response};
use crate::frontier::server::task_file::{PromptFields, task_prompt};
use crate::frontier::server::wording::{branch_slug, brief_title};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Context, Error, Result};
use crate::shared::fsx::remove_tree;
use crate::shared::jsonl::append_jsonl;
use crate::shared::service_file::{ServiceFile, write_service_file};

/// The project id the layer's parts know every trial by.
const PROJECT_ID: &str = "bench";

/// omp's time limit for the layer's MCP calls (0577): `check` runs up to 30 minutes, plus one, as setup writes on his
/// global `jocasta` server.
const MCP_TIMEOUT_MS: u64 = 1_860_000;

/// Where `jocasta mcp` calls its tools.
const MCP_PATH: &str = "/api/mcp/";

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
	server: Option<ServerHandle>,
}

impl Running {
	async fn close(self) {
		if let Some(server) = &self.server {
			server.stop(true).await;
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

/// The `{text, isError?}` reply the tools answer with.
fn reply(text: &str, is_error: bool, status: StatusCode) -> Response {
	let body = if is_error { json!({ "text": text, "isError": true }) } else { json!({ "text": text }) };
	json_response(status, &body)
}

/// The layer's endpoint: `POST /api/mcp/<tool>` with the token as the bearer, answering in this process what the
/// service's tools answer there.
struct McpRoute {
	token: String,
	trial: Arc<Trial>,
}

impl Route for McpRoute {
	fn matches(&self, _path: &str) -> bool {
		true
	}

	fn handle(
		&self,
		request: Request,
		url: Url,
		_server: ServerHandle,
	) -> crate::frontier::server::routes::BoxFuture<Response> {
		let trial = self.trial.clone();
		let authorized = has_bearer(&request, &self.token);
		Box::pin(async move {
			if !authorized {
				return reply("invalid token", true, StatusCode::UNAUTHORIZED);
			}
			let Some(encoded) = url.path().strip_prefix(MCP_PATH).filter(|_| request.method() == Method::POST) else {
				return reply("not found", true, StatusCode::NOT_FOUND);
			};
			let Some(tool) = decode_uri_component(encoded) else {
				return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
			};
			let outcome = match read_args(request).await {
				Ok(args) => answer(&trial, &tool, args).await,
				Err(message) => Err(message),
			};
			match outcome {
				Ok(text) => reply(&text, false, StatusCode::OK),
				Err(message) => reply(&message, true, StatusCode::OK),
			}
		})
	}
}

/// The request's `{args}` when it is an object, else `{}`; the error is what reading the body said.
async fn read_args(request: Request) -> std::result::Result<Map<String, Value>, String> {
	let bytes = axum::body::to_bytes(request.into_body(), 128 * 1024 * 1024).await.map_err(said)?;
	match serde_json::from_slice::<Value>(&bytes).map_err(said)? {
		Value::Null => Err("null is not an object (evaluating 'body.args')".to_owned()),
		Value::Object(body) => Ok(body.get("args").and_then(Value::as_object).cloned().unwrap_or_default()),
		_ => Ok(Map::new()),
	}
}

/// The `jocasta` executable `jocasta mcp` runs from: the one beside this process's own executable (the install's, or a
/// cargo build's `target/<profile>/`; a test executable sits in that folder's `deps/`).
fn jocasta_exe() -> PathBuf {
	let exe = std::env::current_exe().unwrap_or_default();
	let mut dir = exe.parent().unwrap_or(Path::new("")).to_path_buf();
	if dir.file_name().is_some_and(|name| name == "deps") {
		dir.pop();
	}
	dir.join(format!("jocasta{}", std::env::consts::EXE_SUFFIX))
}

fn mcp_json(env: &Value) -> Result<String> {
	let config = json!({
		"mcpServers": {
			"jocasta": {
				"type": "stdio",
				"command": jocasta_exe().display().to_string(),
				"args": ["mcp"],
				"env": env,
				"timeout": MCP_TIMEOUT_MS,
			}
		}
	});
	Ok(format!("{}\n", serde_json::to_string_pretty(&config)?))
}

async fn start(trial: &LayerTrial, running: &mut Running, layer: &Path, plugin: &Path) -> Result<LayerSetup> {
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
	let token = random_hex::<24>()?;
	let served = Served {
		routes: vec![Box::new(McpRoute { token: token.clone(), trial: state.clone() })],
		page: Box::new(|_, _, _| Box::pin(async { text_response(StatusCode::NOT_FOUND, "not found") })),
	};
	let server = listen(0, served)?;
	let port = server.port();
	running.server = Some(server);
	// `jocasta mcp` finds this endpoint as it finds the service: `<data>/jocasta-service.json`, its process alive.
	write_service_file(&ServiceFile { port, token, pid: std::process::id() }, Some(&running.data))?;
	let title = brief_title(&trial.prompt);
	// `JOCASTA_THREAD` as a thread pane has it, so `jocasta mcp` lists report_result and check (0577, C6).
	let slug = branch_slug(&title);
	let env = json!({
		"JOCASTA_DATA_DIR": running.data.display().to_string(),
		"JOCASTA_THREAD": if slug.is_empty() { PROJECT_ID } else { slug.as_str() },
	});
	std::fs::write(plugin.join(".mcp.json"), mcp_json(&env)?)?;
	let prompt = layer.join("prompt.md");
	let fields = PromptFields { title: &title, project: &state.project, brief: &trial.prompt, done_when: &[] };
	std::fs::write(&prompt, task_prompt(&fields, trial.append_system_prompt.as_deref(), true))?;
	let env = env
		.as_object()
		.map(|object| {
			object.iter().filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned()))).collect()
		})
		.unwrap_or_default();
	Ok(LayerSetup {
		args: vec![
			"--append-system-prompt".to_owned(),
			prompt.display().to_string(),
			"-e".to_owned(),
			plugin.display().to_string(),
		],
		settings: Map::new(),
		env,
		closer: None,
	})
}

async fn set_up(trial: LayerTrial) -> Result<LayerSetup> {
	let layer = trial.root.join("layer");
	let plugin = layer.join("omp");
	std::fs::create_dir_all(&plugin)?;
	let mut running = Running { data: layer_data_dir()?, trial: None, server: None };
	match start(&trial, &mut running, &layer, &plugin).await {
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
