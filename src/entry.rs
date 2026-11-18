//! The `jocasta` command (0387): open the work surface, `setup [--refresh <file>]`, `uninstall`, `diagnose`,
//! `mcp [--boss]`, `project …`, `--version`, herdr's own words (the fork built in, its server running the Jocasta service
//! for bare `jocasta`), `push-hook`, git's pre-push hook in every pane (0613), and `pane-check`, the check a project
//! pane's omp extension asks for when a turn ends (0617). No Jocasta code runs inside omp (0386) but that one
//! extension; omp starts `jocasta mcp` as a stdio MCP server. clap parses the words (0637); this module runs the route.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::frontier::client::{ask_pane_check, ask_push};
use crate::frontier::server::boss::{EnsureBossOptions, LaunchedPanes, boss_command, ensure_boss_pane};
use crate::frontier::server::embedded::{SERVICE_ENV, register, serves};
use crate::frontier::server::herdr::{HerdrOptions, connect_herdr};
use crate::frontier::server::launch::{service_health, service_state};
use crate::frontier::server::pane_launch::pane_omp_program;
use crate::mcp::server::run_mcp_stdio;
use crate::setup::diagnose::{DiagnoseDeps, ServiceInfo, run_diagnose};
use crate::setup::setup::{
	RunOutput, RunProgram, SetupDeps, SetupIo, default_calendar_store, default_setup_paths, mise_config_dir,
	refresh_setup, run_setup, setup_check, setup_undo,
};
use crate::setup::uninstall::{
	UninstallDeps, UninstallPaths, processes_under, read_install_record, remove_after_exit, remove_run_value_for,
	remove_user_path_entry, run_uninstall,
};
use crate::shared::credentials::{CALENDAR_CREDENTIAL, delete_credential, read_credential, take_calendar_env};
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::git::git;
use crate::shared::herdr_session::{DEFAULT_SESSION, herdr_program, herdr_session, session_args};
use crate::shared::install::{install_dir, self_command, shipped_config_dir};
use crate::shared::log::logs_dir;
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::process::{RunOptions, on_path, run_program, run_program_sync};
use crate::shared::project_cli::project_command;
use crate::shared::push_hook::{PushHookDeps, PushReply, PushRequest, run_push_hook};
use crate::shared::route::{Route, route};
use crate::shared::service_file::{read_service_file, running_service};
use crate::shared::surface::{SurfaceDeps, after_surface, before_surface, last_logged_error};
use crate::shared::thread_cli::{CliDeps, CliWord, cli_request, run_cli_command};

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

/// `jocasta --version`'s line: Jocasta's, omp's and the built-in herdr's versions.
fn version_line() -> String {
	format!(
		"jocasta {} (omp {}, herdr {})",
		env!("CARGO_PKG_VERSION"),
		version_of(on_path("omp", None)),
		herdr::embed::version()
	)
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

async fn run_setup_route(refresh: Option<String>, calendar_env: Option<String>) -> Result<i32> {
	let herdr = herdr_program();
	let paths = default_setup_paths(&shipped_config_dir(None), self_command(None));
	let mut deps = SetupDeps::new(herdr.clone());
	deps.mise = on_path("mise", None);
	deps.calendar = default_calendar_store(calendar_env);
	deps.git_version = Some(Arc::new(|| {
		Box::pin(async {
			let out = run(vec!["git".to_owned(), "--version".to_owned()]).await.ok()?;
			(out.code == 0).then_some(out.output)
		})
	}));
	if let Some(herdr) = herdr {
		let session = herdr_session()?;
		deps.reload_herdr = Some(Arc::new(move || {
			let argv = std::iter::once(herdr.display().to_string())
				.chain(session_args(&session))
				.chain(["server".to_owned(), "reload-config".to_owned()])
				.collect();
			Box::pin(run(argv))
		}));
	}
	let io = ConsoleIo::new();
	match refresh {
		None => run_setup(&paths, &deps, &io).await,
		Some(file) => refresh_setup(&paths, &deps, &io, &file, None).await,
	}
}

async fn run_uninstall_route() -> Result<i32> {
	let session = herdr_session()?;
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
		herdr: herdr_program(),
		session,
		run: run_program_dep(),
		setup_undo: setup_undo(&setup, read_credential(CALENDAR_CREDENTIAL).is_some()),
		remove_path_entry: Arc::new(remove_user_path_entry),
		remove_run_value: Arc::new(remove_run_value_for),
		delete_credential: Arc::new(delete_credential),
		remove_after_exit: Arc::new(|dir, entries| remove_after_exit(dir, entries, None)),
		processes_under: Some(Arc::new(processes_under)),
	};
	run_uninstall(&paths, &deps, &ConsoleIo::new()).await
}

async fn run_diagnose_route() -> Result<i32> {
	let session = herdr_session()?;
	let connecting = HerdrOptions { session: session.clone(), ..HerdrOptions::default() };
	let deps = DiagnoseDeps {
		data_dir: data_dir(),
		assets_dir: shipped_config_dir(None),
		omp: vec!["omp".to_owned()],
		versions: version_line(),
		session,
		service: Box::new(|| {
			Box::pin(async {
				let file = read_service_file();
				Ok(ServiceInfo { state: service_state(file.as_ref(), None).await, file })
			})
		}),
		connect: Box::new(move || {
			let options = connecting.clone();
			Box::pin(async move { Ok(connect_herdr(options).await?) })
		}),
		ensure_boss: Box::new(|herdr| {
			Box::pin(async move {
				let launched = LaunchedPanes::new(None);
				let assets = shipped_config_dir(None);
				let mut options = EnsureBossOptions::new(boss_command(&pane_omp_program(), Some(&assets)), &launched);
				options.config_dir = Some(assets);
				ensure_boss_pane(&*herdr, options).await
			})
		}),
		setup_check: Box::new(|| Box::pin(setup_check())),
		now: None,
	};
	run_diagnose(&deps, &out).await
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

/// Bare `jocasta` (0392, 0552; fork plan phase 4): what the work surface needs from this machine.
struct Surface {
	herdr_config: PathBuf,
}

impl SurfaceDeps for Surface {
	async fn service_health(&self) -> crate::frontier::server::launch::ServiceHealth {
		service_health(read_service_file().as_ref(), None).await
	}

	fn last_error(&self, since: i64) -> Option<String> {
		last_logged_error(&logs_dir(), since)
	}

	fn herdr_config(&self) -> &Path {
		&self.herdr_config
	}

	fn print(&self, line: &str) {
		err(line);
	}
}

/// Runs `future` on a runtime of its own that is gone before herdr's entry builds its own.
fn on_runtime<T>(future: impl Future<Output = T>) -> std::io::Result<T> {
	let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
	let result = runtime.block_on(future);
	runtime.shutdown_background();
	Ok(result)
}

/// Bare `jocasta`: herdr's client on Jocasta's session, whose server, launched here when none runs, runs the service
/// (`JOCASTA_SERVICE=1`). herdr's socket overrides a pane passed down are dropped, so a `jocasta` run inside a pane
/// opens Jocasta's session, not that pane's; `JOCASTA_HERDR_SESSION` picks a throwaway one for live checks (0441).
/// The calendar's secret stays for that server, which takes it before it starts anything (SEC-5, 0430).
fn run_surface() -> ExitCode {
	let session = match herdr_session() {
		Ok(session) => session,
		Err(error) => return ExitCode::from(fail(&error)),
	};
	// SAFETY: before any thread starts, as `std::env::set_var` and `remove_var` require.
	unsafe {
		for name in ["HERDR_SOCKET_PATH", "HERDR_CLIENT_SOCKET_PATH", "HERDR_SESSION"] {
			std::env::remove_var(name);
		}
		if session != DEFAULT_SESSION {
			std::env::set_var("HERDR_SESSION", &session);
		}
		std::env::set_var(SERVICE_ENV, "1");
	}
	let surface = Surface { herdr_config: config_dir().join("herdr.toml") };
	let since = crate::shared::clock::now_ms();
	match on_runtime(before_surface(&surface)) {
		Ok(Ok(())) => {}
		Ok(Err(error)) => return ExitCode::from(fail(&error)),
		Err(error) => {
			err(&format!("jocasta: {error}"));
			return ExitCode::from(1);
		}
	}
	// herdr's client ends the process itself when its server stops under it: Jocasta's word comes first there too.
	herdr::embed::set_exit_hook(Box::new(move |code| surface_ended(since, code)));
	let code = i32::from(run_herdr(&[]) != ExitCode::SUCCESS);
	ExitCode::from(u8::try_from(surface_ended(since, code)).unwrap_or(1))
}

/// Once herdr's client ended with `code`: why Jocasta stopped, if it stopped while starting (0582); the exit code.
fn surface_ended(since: i64, code: i32) -> i32 {
	let surface = Surface { herdr_config: config_dir().join("herdr.toml") };
	on_runtime(after_surface(&surface, since, code)).unwrap_or(code)
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
		Route::Mcp { boss } => {
			// stdout carries only MCP messages.
			run_mcp_stdio(boss).await;
			Ok(0)
		}
		Route::PaneCheck => {
			// What a project pane's omp extension runs when a turn ends (0617): the service's text for the agent, or
			// nothing.
			let text = ask_pane_check(env::var("JOCASTA_PANE_KEY").as_deref()).await;
			let mut stdout = std::io::stdout();
			let _ = stdout.write_all(text.as_bytes());
			let _ = stdout.flush();
			Ok(0)
		}
		Route::PushHook { args } => run_push_hook_route(&args).await,
		Route::Setup { refresh } => run_setup_route(refresh, calendar_env).await,
		Route::Uninstall => run_uninstall_route().await,
		Route::Diagnose => run_diagnose_route().await,
		Route::Surface | Route::Herdr => unreachable!("`main` runs these before Jocasta's runtime exists"),
	}
}

async fn run_service_cli(word: CliWord, args: &[String]) -> Result<i32> {
	Ok(run_cli_command(&cli_request(word, args)?, &ServiceCli).await)
}

/// A word of herdr's: herdr's own entry, which reads the command line itself and builds its own runtime (its server,
/// its client, its socket API's commands), in Jocasta's dirs: its sessions, sockets and plugins under
/// `<data>/herdr`, their state under `<data>/herdr/state`, its config Jocasta's `herdr.toml` (`HERDR_CONFIG_PATH`
/// still wins). Its exits inside (usage errors, a busy socket) are its own. A server bare `jocasta` launched runs the
/// Jocasta service: the calendar's secret is taken for it and `JOCASTA_SERVICE` dropped before anything starts, so
/// no pane inherits either.
fn run_herdr(args: &[String]) -> ExitCode {
	let herdr_dir = data_dir().join("herdr");
	herdr::embed::set_dirs(herdr::embed::Dirs {
		state: herdr_dir.join("state"),
		config: herdr_dir,
		config_file: config_dir().join("herdr.toml"),
	});
	// His client draws the dock and his questions (fork plan phase 5); a run that is no client never asks.
	crate::frontier::shell::register();
	if serves(args, env::var(SERVICE_ENV).as_deref()) {
		take_calendar_env();
		// SAFETY: before any thread starts, as `std::env::remove_var` requires.
		unsafe { std::env::remove_var(SERVICE_ENV) };
		register();
	}
	match herdr::main() {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			err(&format!("jocasta: {error}"));
			ExitCode::from(1)
		}
	}
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
	let args: Vec<String> = std::env::args_os().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect();
	let target = match route(&args) {
		Ok(target) => target,
		Err(error) => {
			let _ = error.print();
			return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
		}
	};
	match target {
		Route::Herdr => return run_herdr(&args),
		Route::Surface => return run_surface(),
		_ => {}
	}
	// The calendar's secret never reaches herdr's server, omp or anything else this command starts (SEC-5, 0430); taken
	// before any thread starts.
	let calendar_env = take_calendar_env();
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
