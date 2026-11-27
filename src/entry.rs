//! The `jocasta` command (0387): open the work surface, `setup [--refresh <file>]`, `uninstall`, `diagnose`,
//! `project …`, `--version`, herdr's own words (the fork built in, its server running the Jocasta service for bare
//! `jocasta`), and `push-hook`, git's pre-push hook in every pane (0613). No Jocasta code runs inside omp (0386) but the
//! bridge extension (0675). clap parses the words (0637); this module runs the route.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::frontier::client::ask_push;
use crate::frontier::server::embedded::run_embedded;
use crate::frontier::server::launch::{service_health, service_state};
use crate::frontier::server::pane_launch::pane_omp_program;
use crate::setup::diagnose::{DiagnoseDeps, ServiceInfo, run_diagnose};
use crate::setup::setup::{
	Asking, RunOutput, RunProgram, SetupDeps, SetupIo, default_calendar_store, default_secret_store,
	default_setup_paths, mise_config_dir, refresh_setup, run_setup, setup_check, setup_undo,
};
use crate::setup::uninstall::{
	UninstallDeps, UninstallPaths, processes_under, read_install_record, remove_after_exit, remove_run_value_for,
	remove_user_path_entry, run_uninstall,
};
use crate::shared::credentials::{delete_credential, read_credential, take_calendar_env, take_source_envs};
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::git::git;
use crate::shared::install::{install_dir, self_command, shipped_config_dir};
use crate::shared::log::logs_dir;
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::process::{RunOptions, on_path, run_program, run_program_sync};
use crate::shared::project_cli::project_command;
use crate::shared::push_hook::{PushHookDeps, PushReply, PushRequest, run_push_hook};
use crate::shared::route::{Route, route};
use crate::shared::service_file::{read_service_file, running_service};
use crate::shared::surface::{SurfaceDeps, after_surface, before_surface, last_logged_error};
use crate::shared::thread_cli::{CliDeps, CliWord, cli_request, run_cli_command, run_status_json};

/// `<program> --version`'s version (`omp/18.10.3`), or "not found".
fn version_of(program: Option<PathBuf>) -> String {
	const NOT_FOUND: &str = "not found";
	let Some(program) = program else { return NOT_FOUND.to_owned() };
	let argv = [program.into_os_string(), "--version".into()];
	let Ok(out) = run_program_sync(&argv, RunOptions { timeout_ms: Some(10_000), ..RunOptions::default() }) else {
		return NOT_FOUND.to_owned();
	};
	let text = out.stdout.trim();
	if out.code != Some(0) || text.is_empty() {
		return NOT_FOUND.to_owned();
	}
	text.rsplit(|c: char| c.is_whitespace() || c == '/').next().unwrap_or(text).to_owned()
}

/// `jocasta --version`'s line: Jocasta's and omp's versions.
fn version_line() -> String {
	format!("jocasta {} (omp {})", env!("CARGO_PKG_VERSION"), version_of(on_path("omp", None)))
}

/// A line to stdout; a closed pipe is no reason to fail.
fn out(line: &str) {
	let _ = writeln!(std::io::stdout(), "{line}");
}

/// A line to stderr.
fn err(line: &str) {
	let _ = writeln!(std::io::stderr(), "{line}");
}

/// Runs a program to completion with its output captured.
async fn run(argv: Vec<String>) -> Result<RunOutput> {
	let result = run_program(&argv, RunOptions::default()).await?;
	Ok(RunOutput { code: result.code.unwrap_or(1), output: format!("{}{}", result.stdout, result.stderr) })
}

fn run_program_dep() -> RunProgram {
	Arc::new(|argv| Box::pin(run(argv)))
}

/// Lines to the console and questions from it: one reader for every question a command asks. The reader ends with
/// the command: nothing keeps the process alive in a terminal after its last line.
struct ConsoleIo {
	lines: tokio::sync::Mutex<tokio::io::Lines<BufReader<tokio::io::Stdin>>>,
}

impl ConsoleIo {
	fn new() -> Self {
		Self { lines: tokio::sync::Mutex::new(BufReader::new(tokio::io::stdin()).lines()) }
	}

	fn write(text: &str) {
		let mut stdout = std::io::stdout();
		let _ = write!(stdout, "{text}");
		let _ = stdout.flush();
	}

	async fn next(&self) -> Option<String> {
		self.lines.lock().await.next_line().await.ok().flatten()
	}
}

impl SetupIo for ConsoleIo {
	fn print(&self, line: &str) {
		out(line);
	}

	fn ask<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		Box::pin(async move {
			Self::write(question);
			self.next().await
		})
	}

	/// A secret (the calendar's address, SEC-LOC-12): typed in concealed text, then its rows erased and the question
	/// shown again, so neither the screen nor the scrollback keeps it.
	fn ask_secret<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		Box::pin(async move {
			if !std::io::stdout().is_terminal() {
				return self.ask(question).await;
			}
			Self::write(&format!("{question}\x1b[8m"));
			let answer = self.next().await;
			let columns = ratatui::crossterm::terminal::size().map_or(80, |(columns, _)| usize::from(columns).max(1));
			let typed = question.encode_utf16().count() + answer.as_deref().map_or(0, |a| a.encode_utf16().count());
			let rows = typed.div_ceil(columns).max(1);
			Self::write(&format!("\x1b[28m{}{question}\n", "\x1b[1A\r\x1b[2K".repeat(rows)));
			answer
		})
	}
}

async fn run_setup_route(refresh: Option<String>, asking: Asking, calendar_env: Option<String>) -> Result<i32> {
	let paths = default_setup_paths(&shipped_config_dir(None), self_command(None));
	let mut deps = SetupDeps::new();
	deps.mise = on_path("mise", None);
	deps.calendar = default_calendar_store(calendar_env);
	deps.secrets = default_secret_store();
	deps.git_version = Some(Arc::new(|| {
		Box::pin(async {
			let out = run(vec!["git".to_owned(), "--version".to_owned()]).await.ok()?;
			(out.code == 0).then_some(out.output)
		})
	}));
	deps.docker = Some(run_program_dep());
	let io = ConsoleIo::new();
	match refresh {
		None => run_setup(&paths, &deps, &io, asking).await,
		Some(file) => refresh_setup(&paths, &deps, &io, &file, None).await,
	}
}

async fn run_uninstall_route() -> Result<i32> {
	// Only the install this command runs from: a source run removes no install.
	let install = install_dir(None).and_then(|dir| {
		let local = env::var("LOCALAPPDATA").map(PathBuf::from);
		read_install_record(&dir, local.as_deref(), None)
	});
	let paths = UninstallPaths {
		jocasta_config_dir: config_dir(),
		data_dir: data_dir(),
		mise_config_dir: mise_config_dir(),
		// Windows holds the running jocasta.exe: its files go after this exits. POSIX removes them at once.
		running_from_install: install.is_some() && cfg!(windows),
		install,
	};
	let setup = default_setup_paths(&shipped_config_dir(None), self_command(None));
	let deps = UninstallDeps {
		service_state: Arc::new(|| Box::pin(async { Ok(service_state(read_service_file().as_ref(), None).await) })),
		setup_undo: setup_undo(&setup, |target| read_credential(target).is_some()),
		remove_path_entry: Arc::new(remove_user_path_entry),
		remove_run_value: Arc::new(remove_run_value_for),
		delete_credential: Arc::new(delete_credential),
		remove_after_exit: Arc::new(|dir, entries| remove_after_exit(dir, entries, None)),
		processes_under: Some(Arc::new(processes_under)),
	};
	run_uninstall(&paths, &deps, &ConsoleIo::new()).await
}

async fn run_diagnose_route() -> Result<i32> {
	let deps = DiagnoseDeps {
		data_dir: data_dir(),
		assets_dir: shipped_config_dir(None),
		omp: pane_omp_program(),
		versions: version_line(),
		service: Box::new(|| {
			Box::pin(async {
				let file = read_service_file();
				Ok(ServiceInfo { state: service_state(file.as_ref(), None).await, file })
			})
		}),
		run_omp: Box::new(|command, dir| Box::pin(open_diagnose(command, dir))),
		setup_check: Box::new(|| Box::pin(setup_check())),
		now: None,
	};
	run_diagnose(&deps, &out).await
}

/// 0472: the read-only omp opens as a tab of the running Jocasta's boss workspace, which the service makes (it reaches
/// the shell in its own process over the bridge pipe). With no Jocasta running it runs in this terminal, and says so.
async fn open_diagnose(command: Vec<String>, dir: PathBuf) -> Result<i32> {
	if let Some(service) = running_service() {
		let message = serde_json::json!({
			"t": "cli",
			"command": "diagnose tab",
			"args": { "command": command, "cwd": dir.display().to_string() },
		});
		let answer = match crate::shared::bridge_wire::CliPeer::open(service.port, &service.token).await {
			Ok(mut peer) => peer.request(message).await,
			Err(error) => Err(error),
		};
		if let Ok(answer) = answer {
			let text = ["text", "error"].iter().find_map(|key| answer.get(key).and_then(serde_json::Value::as_str));
			let done = answer.get("ok") == Some(&serde_json::Value::Bool(true));
			out(text.unwrap_or("Jocasta's service answered without a result"));
			if done {
				out("Switch to Jocasta's window to read it.");
			}
			return Ok(i32::from(!done));
		}
	}
	out("Jocasta isn't running: starting the read-only diagnose session of omp in this terminal instead.");
	let mut argv = command.into_iter();
	let program = argv.next().ok_or_else(|| Error::other("no omp command to run"))?;
	let status = tokio::process::Command::new(program)
		.args(argv)
		.current_dir(dir)
		.status()
		.await
		.map_err(|error| Error::other(format!("omp didn't start: {error}")))?;
	Ok(status.code().unwrap_or(1))
}

/// What git runs before a push (0613): the exit code is its verdict, the reason goes to stderr.
struct PushHook {
	cwd: PathBuf,
	pane_key: Option<String>,
}

impl PushHookDeps for PushHook {
	fn cwd(&self) -> &Path {
		&self.cwd
	}

	fn pane_key(&self) -> Option<&str> {
		self.pane_key.as_deref()
	}

	fn trunk(&self, remote: &str) -> Option<String> {
		let head = git(&self.cwd, &["symbolic-ref", "--short", &format!("refs/remotes/{remote}/HEAD")]).ok()??;
		head.strip_prefix(&format!("{remote}/")).map(str::to_owned)
	}

	async fn ask(&self, request: &PushRequest) -> PushReply {
		ask_push(request).await
	}

	fn say(&self, line: &str) {
		err(line);
	}
}

async fn run_push_hook_route(args: &[String]) -> Result<i32> {
	let stdin = std::io::read_to_string(std::io::stdin()).unwrap_or_default();
	let hook = PushHook { cwd: std::env::current_dir()?, pane_key: env::var_nonempty("JOCASTA_PANE_KEY") };
	Ok(run_push_hook(args, &stdin, &hook).await)
}

/// `thread` and `worktree` go through the running service, which asks him first for the changes (0544).
struct ServiceCli;

impl CliDeps for ServiceCli {
	fn service(&self) -> Option<(u16, String)> {
		running_service().map(|service| (service.port, service.token))
	}

	fn print(&self, line: &str) {
		out(line);
	}

	fn print_error(&self, line: &str) {
		err(line);
	}
}

/// Bare `jocasta` (0392, 0552, 0683): what the work surface needs from this machine.
struct Surface;

impl SurfaceDeps for Surface {
	async fn service_health(&self) -> crate::frontier::server::launch::ServiceHealth {
		service_health(read_service_file().as_ref(), None).await
	}

	fn last_error(&self, since: i64) -> Option<String> {
		last_logged_error(&logs_dir(), since)
	}

	fn print(&self, line: &str) {
		err(line);
	}
}

/// Runs `future` on a runtime of its own that is gone before the service's runtime is built.
fn on_runtime<T>(future: impl Future<Output = T>) -> std::io::Result<T> {
	let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
	let result = runtime.block_on(future);
	runtime.shutdown_background();
	Ok(result)
}

/// Bare `jocasta` (0683): Jocasta's own shell and the service in this one process, until its window closes. The
/// calendar's and the sources' secrets are taken out of the environment before any thread starts, so no pane inherits
/// them (SEC-5, 0430).
fn run_surface() -> ExitCode {
	let since = crate::shared::clock::now_ms();
	match on_runtime(before_surface(&Surface)) {
		Ok(Ok(())) => {}
		Ok(Err(error)) => return ExitCode::from(fail(&error)),
		Err(error) => {
			err(&format!("jocasta: {error}"));
			return ExitCode::from(1);
		}
	}
	take_calendar_env();
	take_source_envs();
	let code = match run_embedded() {
		Ok(()) => 0,
		Err(error) => {
			err(&format!("jocasta: {error}"));
			1
		}
	};
	ExitCode::from(u8::try_from(surface_ended(since, code)).unwrap_or(1))
}

/// Once the shell ended with `code`: why Jocasta stopped, if it stopped while starting (0582); the exit code.
fn surface_ended(since: i64, code: i32) -> i32 {
	on_runtime(after_surface(&Surface, since, code)).unwrap_or(code)
}

/// Runs what `target` names; resolves the exit code.
async fn run_route(target: Route, calendar_env: Option<String>) -> Result<i32> {
	match target {
		Route::Version => {
			out(&version_line());
			Ok(0)
		}
		Route::Project { args } => {
			for line in project_command(&args).await? {
				out(&line);
			}
			Ok(0)
		}
		Route::Thread { args } => run_service_cli(CliWord::Thread, &args).await,
		Route::Worktree { args } => run_service_cli(CliWord::Worktree, &args).await,
		Route::PushHook { args } => run_push_hook_route(&args).await,
		Route::Setup { refresh, yes } => {
			run_setup_route(refresh, if yes { Asking::Never } else { Asking::Ask }, calendar_env).await
		}
		Route::Uninstall => run_uninstall_route().await,
		Route::Diagnose => run_diagnose_route().await,
		Route::Status => Ok(run_status_json(&ServiceCli).await),
		Route::BriefOpen { spec } => {
			out(&crate::frontier::view::open_spec(&spec).await?);
			Ok(0)
		}
		Route::Surface | Route::View { .. } | Route::Board { .. } => {
			unreachable!("`main` runs these before Jocasta's runtime exists")
		}
	}
}

async fn run_service_cli(word: CliWord, args: &[String]) -> Result<i32> {
	Ok(run_cli_command(&cli_request(word, args)?, &ServiceCli).await)
}

/// Loud one-line failures (0101): DB failures exit 1; refusals exit with their code, 2 for an invocation error (0140).
/// A usage is printed as it is, its lines aligned (0581). Any other fault exits 1 with its message.
fn fail(error: &Error) -> u8 {
	match error {
		Error::User { message, exit_code } => {
			err(&if message.starts_with("usage: ") { message.clone() } else { format!("jocasta: {message}") });
			*exit_code
		}
		other => {
			err(&format!("jocasta: {other}"));
			1
		}
	}
}

/// The `jocasta` command: routes argv, runs the route and returns the process's exit code.
pub fn main() -> ExitCode {
	crate::shared::install::resolve_at_start();
	let args: Vec<String> = std::env::args_os().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect();
	let target = match route(&args) {
		Ok(target) => target,
		Err(error) => {
			let _ = error.print();
			return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
		}
	};
	if target == Route::Surface {
		return run_surface();
	}
	// The calendar's secret never reaches herdr's server, omp or anything else this command starts (SEC-5, 0430); taken
	// before any thread starts.
	let calendar_env = take_calendar_env();
	take_source_envs();
	// A brief's window (0721): iced runs its own event loop and runtime on this thread.
	if let Route::View { id } = &target {
		return crate::frontier::view::run(id);
	}
	// The board (0718): `jocasta board` starts the window on its own and returns at once; `--window` is that window,
	// iced on this thread like a brief's.
	if let Route::Board { window } = target {
		return if window { crate::frontier::view::board::run() } else { crate::frontier::view::board::open() };
	}
	let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
		Ok(runtime) => runtime,
		Err(error) => {
			err(&format!("jocasta: {error}"));
			return ExitCode::from(1);
		}
	};
	let code = match runtime.block_on(run_route(target, calendar_env)) {
		Ok(code) => u8::try_from(code).unwrap_or(1),
		Err(error) => fail(&error),
	};
	// A console read still waiting for input must not keep the command running once it is done.
	runtime.shutdown_background();
	ExitCode::from(code)
}
