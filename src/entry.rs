//! The `jocasta` command (0387): open the work surface, `setup [--refresh <file>]`, `uninstall`, `diagnose`,
//! `mcp [--boss]`, `project …`, `--version`, `service`, the Jocasta service bare `jocasta` starts (0552), `push-hook`,
//! git's pre-push hook in every pane (0613), and `pane-check`, the check a project pane's omp extension asks for when
//! a turn ends (0617). No Jocasta code runs inside omp (0386) but that one extension; omp starts `jocasta mcp` as a
//! stdio MCP server. clap parses the words (0637); this module runs the route it names.

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::frontier::ask::main::run_ask;
use crate::frontier::client::{ask_pane_check, ask_push, post_action};
use crate::frontier::protocol::PluginAction;
use crate::frontier::server::boss::{EnsureBossOptions, LaunchedPanes, boss_command, ensure_boss_pane};
use crate::frontier::server::herdr::{
	HerdrOptions, connect_herdr, find_herdr, herdr_pipe, herdr_session_answers, stop_herdr_session,
};
use crate::frontier::server::launch::{ask_shutdown, service_health, service_state};
use crate::frontier::server::plugin::apply_agent_view;
use crate::frontier::server::run::run_service;
use crate::frontier::today::main::run_today;
use crate::mcp::server::run_mcp_stdio;
use crate::setup::diagnose::{DiagnoseDeps, ServiceInfo, run_diagnose};
use crate::setup::run_value::{delete_run_value, read_run_value};
use crate::setup::setup::{
	RunOutput, RunProgram, RunValueStore, SetupDeps, SetupIo, default_calendar_store, default_setup_paths,
	mise_config_dir, refresh_setup, run_setup, setup_check, setup_undo,
};
use crate::setup::uninstall::{
	UninstallDeps, UninstallPaths, processes_under, read_install_record, remove_after_exit, remove_run_value_for,
	remove_user_path_entry, run_uninstall,
};
use crate::shared::credentials::{
	CALENDAR_CREDENTIAL, CALENDAR_ENV, delete_credential, read_credential, take_calendar_env,
};
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::git::git;
use crate::shared::herdr_session::herdr_session;
use crate::shared::install::{install_dir, self_command, shipped_config_dir};
use crate::shared::log::{LogLevel, log, logs_dir};
use crate::shared::paths::{config_dir, data_dir};
use crate::shared::process::{RunOptions, on_path, run_program, run_program_sync};
use crate::shared::project_cli::project_command;
use crate::shared::push_hook::{PushHookDeps, PushReply, PushRequest, run_push_hook};
use crate::shared::route::{PluginPart, Route, route};
use crate::shared::service_file::{ServiceFile, read_service_file, running_service};
use crate::shared::surface::{
	StopCheck, SurfaceDeps, attached_surface, last_logged_error, open_surface, service_stopped,
};
use crate::shared::thread_cli::{CliDeps, CliWord, cli_request, run_cli_command};

/// `<program> --version`'s version (`omp/18.10.3`, `herdr 0.9.5`), or "not found".
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

/// `jocasta --version`'s line: Jocasta's, omp's and herdr's versions (no Bun runtime in this build).
fn version_line() -> String {
	format!(
		"jocasta {} (omp {}, herdr {})",
		env!("CARGO_PKG_VERSION"),
		version_of(on_path("omp", None)),
		version_of(find_herdr())
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

/// The logon Run value the companion wrote (0367), through `reg`.
struct RegistryRunValue;

impl RunValueStore for RegistryRunValue {
	fn read(&self, name: &str) -> Option<String> {
		read_run_value(name)
	}

	fn remove(&self, name: &str) -> bool {
		delete_run_value(name)
	}
}

async fn run_setup_route(refresh: Option<String>, calendar_env: Option<String>) -> Result<i32> {
	let herdr = find_herdr();
	let paths = default_setup_paths(&shipped_config_dir(None), self_command(None));
	let mut deps = SetupDeps::new(herdr.clone(), run_program_dep());
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
			let argv = [herdr.display().to_string(), "--session".to_owned(), session.clone()]
				.into_iter()
				.chain(["server".to_owned(), "reload-config".to_owned()])
				.collect();
			Box::pin(run(argv))
		}));
	}
	if cfg!(windows) {
		deps.run_value = Some(Arc::new(RegistryRunValue));
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
	let herdr = find_herdr();
	let started = session.clone();
	let deps = UninstallDeps {
		service_state: Arc::new(|| Box::pin(async { Ok(service_state(read_service_file().as_ref(), None).await) })),
		herdr,
		session,
		run: run_program_dep(),
		setup_undo: setup_undo(&setup, read_credential(CALENDAR_CREDENTIAL).is_some()),
		remove_path_entry: Arc::new(remove_user_path_entry),
		remove_run_value: Arc::new(remove_run_value_for),
		delete_credential: Arc::new(delete_credential),
		remove_after_exit: Arc::new(|dir, entries| remove_after_exit(dir, entries, None)),
		processes_under: Some(Arc::new(processes_under)),
		// A headless server for `plugin unlink` (0558); the session's own step stops it.
		start_herdr: Some(Arc::new(move || {
			let options = HerdrOptions { session: started.clone(), ..HerdrOptions::default() };
			Box::pin(async move {
				let herdr = connect_herdr(options).await?;
				herdr.keep_down();
				herdr.close();
				Ok(())
			})
		})),
	};
	run_uninstall(&paths, &deps, &ConsoleIo::new()).await
}

async fn run_diagnose_route() -> Result<i32> {
	let session = herdr_session()?;
	let herdr_config = config_dir().join("herdr.toml");
	let connecting = HerdrOptions {
		session: session.clone(),
		config_path: herdr_config.exists().then_some(herdr_config),
		..HerdrOptions::default()
	};
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
				let mut options = EnsureBossOptions::new(boss_command(&["omp".to_owned()], Some(&assets)), &launched);
				options.config_dir = Some(assets);
				ensure_boss_pane(&*herdr, options).await
			})
		}),
		setup_check: Box::new(|| Box::pin(setup_check())),
		now: None,
	};
	run_diagnose(&deps, &out).await
}

/// What the `jocasta` herdr plugin runs (0553): its panes, its startup hook and its actions.
async fn run_plugin(part: PluginPart) -> Result<i32> {
	match part {
		PluginPart::Today => run_today().await,
		PluginPart::Ask => run_ask().await,
		PluginPart::Startup => {
			// herdr names its socket file for its plugin commands; on Windows that is a pipe name.
			let socket = env::var_nonempty("HERDR_SOCKET_PATH").map(|file| herdr_pipe(&file));
			// Only a running server: the hook never starts a second one (so there is nothing to revive either).
			let options = HerdrOptions {
				session: herdr_session()?,
				start: Some(false),
				socket_path: socket,
				..HerdrOptions::default()
			};
			let herdr = connect_herdr(options).await?;
			herdr.keep_down();
			let line = if apply_agent_view(&*herdr).await? {
				"Agents view set."
			} else {
				"No boss workspace yet; the service sets the view."
			};
			out(line);
			herdr.close();
			Ok(0)
		}
		PluginPart::Action(action) => Ok(run_plugin_action(action).await),
	}
}

/// The focused pane from herdr's invocation context (`focused_pane_id`), else the pane it ran in.
async fn run_plugin_action(action: PluginAction) -> i32 {
	let mut pane_id = env::var("HERDR_PANE_ID");
	let context = env::var("HERDR_PLUGIN_CONTEXT_JSON").unwrap_or_else(|| "{}".to_owned());
	// No context: the pane it ran in.
	if let Ok(serde_json::Value::Object(context)) = serde_json::from_str(&context)
		&& let Some(focused) = context.get("focused_pane_id").and_then(|focused| focused.as_str())
		&& !focused.is_empty()
	{
		pane_id = Some(focused.to_owned());
	}
	match post_action(action, pane_id.as_deref()).await {
		Some(refused) => {
			err(&format!("jocasta: {refused}"));
			1
		}
		None => 0,
	}
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

/// Bare `jocasta` (0392, 0552): what the work surface needs from this machine.
struct Surface {
	session: String,
	herdr: Option<PathBuf>,
	herdr_config: PathBuf,
	/// The calendar's secret, taken from the environment at start: the service takes it for itself; nothing else this
	/// command starts gets it.
	calendar_env: Option<String>,
}

impl Surface {
	fn herdr_options(&self, timeout_ms: i64) -> HerdrOptions {
		HerdrOptions { session: self.session.clone(), timeout_ms: Some(timeout_ms), ..HerdrOptions::default() }
	}
}

impl SurfaceDeps for Surface {
	async fn service_health(&self) -> crate::frontier::server::launch::ServiceHealth {
		service_health(read_service_file().as_ref(), None).await
	}

	/// A plain process outside this terminal (0552): closing the terminal mustn't end it before it has stopped
	/// everything.
	async fn start_service(&self) -> i32 {
		let command = self_command(None);
		let Some((program, args)) = command.split_first() else { return 1 };
		let mut service = tokio::process::Command::new(program);
		service
			.args(args)
			.arg("service")
			.stdin(std::process::Stdio::null())
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::null());
		env::apply_to_tokio(&mut service);
		if let Some(secret) = &self.calendar_env {
			service.env(CALENDAR_ENV, secret);
		}
		#[cfg(windows)]
		// DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP, as Node's `detached` starts it (no console window either).
		service.creation_flags(0x0000_0208);
		#[cfg(unix)]
		service.process_group(0);
		match service.spawn() {
			Ok(mut child) => child.wait().await.map_or(1, |status| status.code().unwrap_or(1)),
			Err(_) => 1,
		}
	}

	fn last_error(&self, since: i64) -> Option<String> {
		last_logged_error(&logs_dir(), since)
	}

	fn herdr(&self) -> Option<&Path> {
		self.herdr.as_deref()
	}

	fn herdr_config(&self) -> &Path {
		&self.herdr_config
	}

	async fn attach(&self, argv: &[String], env: &HashMap<String, String>) -> Result<i32> {
		let Some((program, args)) = argv.split_first() else { return Err(Error::other("no program to attach")) };
		let mut client = tokio::process::Command::new(program);
		client.args(args);
		crate::shared::env::apply_to_tokio(&mut client);
		client.envs(env);
		let status = client.status().await?;
		Ok(status.code().unwrap_or(1))
	}

	fn print(&self, line: &str) {
		err(line);
	}

	fn enter_surface(&self) -> Box<dyn Fn() -> bool> {
		match attached_surface(&data_dir().join("surfaces"), None, None, None) {
			Ok(attached) => Box::new(move || attached.leave()),
			Err(error) => {
				log(LogLevel::Warn, "surface", &format!("couldn't mark this terminal as attached: {error}"), None);
				Box::new(|| true)
			}
		}
	}

	fn on_terminal_close(&self, handler: Box<dyn Fn() + Send + Sync>) -> Box<dyn FnOnce()> {
		let listener = tokio::spawn(async move {
			#[cfg(windows)]
			let Ok(mut closed) = tokio::signal::windows::ctrl_close() else { return };
			#[cfg(unix)]
			let Ok(mut closed) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) else {
				return;
			};
			while closed.recv().await.is_some() {
				handler();
			}
		});
		Box::new(move || listener.abort())
	}

	async fn tell_service_closed(&self) -> bool {
		match read_service_file() {
			Some(file) => ask_shutdown(&file).await,
			None => false,
		}
	}

	/// A closing console leaves seconds (LC-7): herdr gets 3 of them.
	async fn stop_herdr(&self) -> Result<()> {
		stop_herdr_session(&self.herdr_options(3_000)).await?;
		Ok(())
	}

	fn exit(&self) {
		std::process::exit(0);
	}

	fn watch_service_stop(&self) -> StopCheck {
		let before = read_service_file();
		Box::new(move || {
			Box::pin(async move {
				let now: Option<ServiceFile> = read_service_file();
				service_stopped(before.as_ref(), now.as_ref(), async |file: &ServiceFile| {
					service_state(Some(file), None).await
				})
				.await
			})
		})
	}

	/// 0518: a ping that never starts herdr; the service starts a dead server again (0431).
	async fn herdr_up(&self) -> bool {
		herdr_session_answers(&self.herdr_options(1_000)).await
	}
}

async fn run_surface(calendar_env: Option<String>) -> Result<i32> {
	let surface = Surface {
		session: herdr_session()?,
		herdr: find_herdr(),
		herdr_config: config_dir().join("herdr.toml"),
		calendar_env,
	};
	open_surface(&surface).await
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
		// Ends here: no stray handle keeps a stopped or failed service holding the relay port.
		Route::Service => Ok(run_service().await),
		Route::Plugin { part } => run_plugin(part).await,
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
		Route::Surface => run_surface(calendar_env).await,
		Route::Herdr => unreachable!("`main` runs herdr's words before Jocasta's runtime exists"),
	}
}

async fn run_service_cli(word: CliWord, args: &[String]) -> Result<i32> {
	Ok(run_cli_command(&cli_request(word, args)?, &ServiceCli).await)
}

/// A word of herdr's: herdr's own entry, which reads the command line itself and builds its own runtime (its server,
/// its client, its socket API's commands), in Jocasta's dirs: its sessions, sockets and plugins under
/// `<data>/herdr`, their state under `<data>/herdr/state`, its config Jocasta's `herdr.toml` (`HERDR_CONFIG_PATH`
/// still wins). Its exits inside (usage errors, a busy socket) are its own.
fn run_herdr() -> ExitCode {
	let herdr_dir = data_dir().join("herdr");
	herdr::embed::set_dirs(herdr::embed::Dirs {
		state: herdr_dir.join("state"),
		config: herdr_dir,
		config_file: config_dir().join("herdr.toml"),
	});
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
	if target == Route::Herdr {
		return run_herdr();
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
