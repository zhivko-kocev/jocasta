//! How the service starts omp in a pane (0399, 0404, 0457, 0521, 0540, 0578): the boss pane with this run's boss key,
//! a project or thread pane again with its overlay, env and pane key, each checked to host on loopback only (0561);
//! the panes' env with mise's (0473, 0477) and hunk's (F-02, F-03); and Jocasta's hunk session daemon.
//!
//! What the TypeScript sets in the constructor (`paneOmp`, `paneEnv`, the boss key) is built on first use, from the
//! options the context holds: the context doesn't exist yet when the part is made. A pane's env is an ordered
//! [`Env`], so the argv and env herdr gets are in the TypeScript's order.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::frontier::server::boss::{EnsureBossOptions, PANE_KEY_ENV, boss_command, ensure_boss_pane, new_pane_key};
use crate::frontier::server::context::{Link, Shared};
use crate::frontier::server::events::BossPane;
use crate::frontier::server::herdr::{AgentState, Env, PaneInfo, RespawnOptions};
use crate::frontier::server::hunk::{HUNK_PORT, ensure_hunk_daemon, find_hunk, hunk_env};
use crate::frontier::server::mise::{Mise, mise_env, trust_copies};
use crate::frontier::server::overlay::ensure_overlay;
use crate::frontier::server::service::ServiceOptions;
use crate::frontier::server::task_file::{TaskRecord, launch_prompt};
use crate::frontier::server::thread::ThreadRef;
use crate::frontier::server::worktrees::thread_env;
use crate::shared::boss_key::BOSS_KEY_ENV;
use crate::shared::bridge_wire::{BRIDGE_ENV, BRIDGE_EXTENSION};
use crate::shared::clock::now_ms;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::write_atomically;
use crate::shared::install::{self_command, shipped_config_dir};
use crate::shared::layer::LAYER_ENV;
use crate::shared::log::{LogLevel, log};
use crate::shared::pane_sleep::PANE_SLEEP_ROLE;
use crate::shared::pathkey::{path_key, real_path};
use crate::shared::paths::data_dir;
#[cfg(windows)]
use crate::shared::process::on_path;
use crate::shared::proctree::{ProcessId, end_processes, process_start};
use crate::shared::projects::Project;
use crate::shared::prompt::without_leading_comment;
use crate::shared::push_hook::push_hook_env;

#[cfg(test)]
#[path = "pane_launch_tests.rs"]
mod tests;

/// What every thread's omp starts with besides its pane's flags (0541, K10): a cost warning in its pane at $50.
pub const THREAD_FLAGS: [&str; 2] = ["--warn-cost", "50"];

/// The boss pane's place among the folders [`OmpBackoff`] counts: no folder's key is empty.
const BOSS_AGAIN_KEY: &str = "";

/// The first wait before an omp that ended again in a Jocasta pane starts again; doubled with each ending (0521).
const OMP_AGAIN_MS: i64 = 10_000;
/// The longest wait before an omp that keeps ending in a Jocasta pane starts again (0521).
const OMP_AGAIN_MAX_MS: i64 = 5 * 60_000;
/// An omp that ran this long since Jocasta last started it again ends afresh: no wait (0521).
const OMP_AGAIN_RESET_MS: i64 = 10 * 60_000;

/// The shipped config folder's `file`, as the argv spells it.
fn shipped(file: &str) -> String {
	shipped_config_dir(None).join(file).display().to_string()
}

/// What `String.prototype.trim` strips from the start: white space and the byte order mark.
fn js_trim_start(text: &str) -> &str {
	text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `{...env, [name]: value}`: a name set again keeps its place and takes the new value.
fn set_env(env: &mut Env, name: &str, value: String) {
	match env.iter_mut().find(|(have, _)| have == name) {
		Some(slot) => slot.1 = value,
		None => env.push((name.to_owned(), value)),
	}
}

/// The process's own environment, as `process.env` holds it.
fn process_env() -> HashMap<String, String> {
	env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect()
}

/// What a thread's omp loads after the project's overlay (0590): `config/heavy.yml` for a heavy thread, which makes its
/// `default` role his `slow` one; nothing for another. Its first launch and every relaunch take it from here.
pub fn after_overlay(task: Option<&TaskRecord>) -> Vec<String> {
	if task.is_some_and(|task| task.heavy == Some(true)) {
		vec!["--config".to_owned(), shipped("heavy.yml")]
	} else {
		Vec::new()
	}
}

/// Whether `name` is one of the keys mise's env must not override in a pane (0484): herdr's, Jocasta's and omp's
/// (`PI_*`), in any case.
fn is_jocasta_key(name: &str) -> bool {
	let upper = name.to_uppercase();
	["HERDR_", "JOCASTA_", "PI_"].iter().any(|prefix| upper.starts_with(prefix))
}

/// mise's env without the keys it must not override in a pane (0484): herdr's, Jocasta's and omp's (`PI_*`).
pub fn without_jocasta_keys(env: impl IntoIterator<Item = (String, String)>) -> Env {
	env.into_iter().filter(|(name, _)| !is_jocasta_key(name)).collect()
}

/// The pane prompt `file` as omp should append it (0577, PRM-6): without its leading `<!-- … -->` comment (Ponytail's
/// license and file notes), as a copy in `<data>` written when it differs; `file` itself when it has no such comment or
/// can't be read.
fn stripped_prompt(file: &Path) -> PathBuf {
	let Ok(bytes) = std::fs::read(file) else { return file.to_path_buf() };
	let text = String::from_utf8_lossy(&bytes).into_owned();
	let stripped = js_trim_start(&without_leading_comment(&text)).to_owned();
	if stripped == text {
		return file.to_path_buf();
	}
	let copy = data_dir().join(file.file_name().unwrap_or_default());
	let written: Result<()> = (|| {
		let held = std::fs::read(&copy).ok().map(|held| String::from_utf8_lossy(&held).into_owned());
		if held.as_deref() != Some(stripped.as_str()) {
			write_atomically(&copy, &stripped, None)?;
		}
		Ok(())
	})();
	match written {
		Ok(()) => copy,
		Err(error) => {
			log(LogLevel::Warn, "service", &format!("{} wasn't written: {error}", copy.display()), None);
			file.to_path_buf()
		}
	}
}

/// The wait before a pane's omp starts again after `count` endings or failures in a row (0521): 10 s doubling to 5 min.
fn again_wait(count: u32) -> i64 {
	OMP_AGAIN_MS.saturating_mul(2_i64.saturating_pow(count.saturating_sub(1))).min(OMP_AGAIN_MAX_MS)
}

/// `GIT_CONFIG_*` for git's pre-push hook (0613) in the order the TypeScript builds it: the count, then each key and
/// its value.
fn hook_env() -> Env {
	let command: Vec<String> = self_command(None).iter().map(|word| word.display().to_string()).collect();
	let hook: BTreeMap<String, String> = push_hook_env(&command, None);
	let count = hook.get("GIT_CONFIG_COUNT").cloned().unwrap_or_default();
	let total: usize = count.parse().unwrap_or(0);
	let mut ordered: Env = vec![("GIT_CONFIG_COUNT".to_owned(), count)];
	for n in total.saturating_sub(3)..total {
		for name in [format!("GIT_CONFIG_KEY_{n}"), format!("GIT_CONFIG_VALUE_{n}")] {
			if let Some(value) = hook.get(&name) {
				ordered.push((name, value.clone()));
			}
		}
	}
	ordered
}

/// What the part derives from the service's options.
struct Config {
	omp: Vec<String>,
	pane_omp: Vec<String>,
	/// The panes' shipped prompt file (Ponytail), or none; a thread's own launch prompt starts with it (0540).
	pane_prompt: Option<PathBuf>,
	pane_env: Env,
	hook_env: Env,
	hunk_port: u16,
	boss_key: String,
}

/// omp as the panes start it, when the service is given none. On Windows herdr's panes take their PATH from the
/// machine's registry (portable-pty's base environment), not from the service, so the omp the service finds on its own
/// PATH, the one it asks `omp config` and `omp usage`, is named by its full path and every pane runs that same omp.
/// Elsewhere a pane inherits the service's PATH and `omp` finds it already.
pub fn pane_omp_program() -> Vec<String> {
	#[cfg(windows)]
	if let Some(found) = on_path("omp", None) {
		return vec![found.display().to_string()];
	}
	vec!["omp".to_owned()]
}

impl Config {
	fn build(options: &ServiceOptions) -> Self {
		let omp = options.omp.clone().unwrap_or_else(pane_omp_program);
		let boss_key = options.boss_key.clone().unwrap_or_else(|| {
			let mut bytes = [0_u8; 32];
			getrandom::fill(&mut bytes).expect("the OS has random bytes");
			bytes.iter().map(|byte| format!("{byte:02x}")).collect()
		});
		let ponytail = shipped_config_dir(None).join("ponytail.md");
		let pane_prompt = match &options.pane_prompt {
			Some(prompt) => prompt.clone(),
			None => ponytail.exists().then_some(ponytail),
		};
		// The guard (0450) rides on every pane, before the project's overlay; the appended prompt without its leading
		// comment (0577, PRM-6).
		let mut pane_omp = omp.clone();
		pane_omp.push("--continue".to_owned());
		if let Some(prompt) = &pane_prompt {
			pane_omp.extend(["--append-system-prompt".to_owned(), stripped_prompt(prompt).display().to_string()]);
		}
		pane_omp.extend(["--config".to_owned(), shipped("guard.yml")]);
		// The one extension every pane loads (0675); `-e` adds to the extensions his own settings list.
		pane_omp.extend(["-e".to_owned(), shipped(BRIDGE_EXTENSION)]);
		let hunk_port = options.hunk_port.unwrap_or(HUNK_PORT);
		let hook_env = hook_env();
		let mut pane_env = hunk_env(hunk_port);
		for (name, value) in &hook_env {
			set_env(&mut pane_env, name, value.clone());
		}
		Self { omp, pane_omp, pane_prompt, pane_env, hook_env, hunk_port, boss_key }
	}
}

/// What changes while the service runs.
#[derive(Default)]
struct State {
	/// The boss pane started with this run's key; another boss pane runs omp started before this run.
	boss_keyed: Option<String>,
	/// The boss's omp started without its `jocasta` tools (beta day P9), said once as a warning notification.
	boss_notice: Option<String>,
	/// The hunk session daemon this service started (B4): Jocasta's close ends it.
	hunk_daemon: Option<ProcessId>,
}

/// How a pane's omp is started again ([`PaneLauncher::respawn`]).
pub struct Respawn<'a> {
	pub cwd: &'a Path,
	pub project_id: &'a str,
	/// The pane's env as it is, without mise's; the hunk variables and mise's when absent.
	pub env: Option<Env>,
	/// A session file: omp goes on with the turn that session's last process left unfinished (0549).
	pub restore: Option<&'a Path>,
	/// The thread of a thread pane; none for a project pane.
	pub thread: Option<&'a ThreadRef>,
}

pub struct PaneLauncher {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	config: OnceLock<Config>,
	state: Mutex<State>,
}

impl PaneLauncher {
	pub fn new(link: Link) -> Self {
		Self { link, config: OnceLock::new(), state: Mutex::new(State::default()) }
	}

	fn config(&self) -> &Config {
		self.config.get_or_init(|| Config::build(&self.link.ctx().options))
	}

	fn state(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// How omp starts: `["omp"]` by default.
	pub fn omp(&self) -> &[String] {
		&self.config().omp
	}

	/// How every project and thread pane starts, before its overlay: omp resuming, with the appended prompt.
	pub fn pane_omp(&self) -> &[String] {
		&self.config().pane_omp
	}

	/// What every project and thread pane starts with: Jocasta's hunk port and no update notice (F-02, F-03), and git's
	/// pre-push hook through its configuration.
	pub fn pane_env(&self) -> &Env {
		&self.config().pane_env
	}

	/// The loopback port of Jocasta's hunk session daemon.
	pub fn hunk_port(&self) -> u16 {
		self.config().hunk_port
	}

	/// The shipped bridge extension every pane loads (0675).
	pub fn bridge_extension(&self) -> String {
		shipped(BRIDGE_EXTENSION)
	}

	/// `env` with the bridge's address (`JOCASTA_BRIDGE`, 0675), which the extension every pane loads connects to; `env`
	/// alone while the bridge isn't listening.
	pub fn bridged(&self, mut env: Env) -> Env {
		if let Some(address) = self.link.ctx().bridge.address() {
			set_env(&mut env, BRIDGE_ENV, address);
		}
		env
	}

	/// This run's boss key (0484): only the boss pane's environment gets it, and boss tools require it.
	pub fn boss_key(&self) -> &str {
		&self.config().boss_key
	}

	/// The id of the boss pane started with this run's key; another boss pane runs omp started before this run.
	pub fn boss_keyed(&self) -> Option<String> {
		self.state().boss_keyed.clone()
	}

	/// The boss's omp started without its `jocasta` tools (beta day P9): said once as a warning notification.
	pub fn boss_notice(&self) -> Option<String> {
		self.state().boss_notice.clone()
	}

	pub fn set_boss_notice(&self, notice: Option<String>) {
		self.state().boss_notice = notice;
	}

	/// Workspace "Jocasta", first, with its boss pane in `<data>/home`; made again when it was closed (0404). At start,
	/// or when its omp ended in it (0521), (`adopt`) a boss pane with no omp in it runs omp, even one Jocasta has no
	/// record of starting. The boss pane's omp starts with this run's boss key in its environment (0484); one already
	/// running without it (started before this run, or by bare `jocasta`) starts again with it, resuming its session,
	/// once it isn't working or asking him.
	pub async fn ensure_boss(&self, adopt: bool) -> Result<()> {
		let c = self.link.ctx();
		let command = c.options.boss_command.clone().unwrap_or_else(|| boss_command(self.omp(), None));
		let mut options = EnsureBossOptions::new(command, &c.launched);
		let mut boss_env: Env = vec![(BOSS_KEY_ENV.to_owned(), self.boss_key().to_owned())];
		for (name, value) in &self.config().hook_env {
			set_env(&mut boss_env, name, value.clone());
		}
		options.env = self.bridged(boss_env);
		options.adopt = adopt;
		let link = self.link.clone();
		options.stale = Some(Box::new(move |pane| link.ctx().panes.unkeyed_boss(pane)));
		let link = self.link.clone();
		options.started = Some(Box::new(move |pane_id| {
			link.ctx().panes.state().boss_keyed = Some(pane_id.to_owned());
		}));
		let pane = ensure_boss_pane(&*c.herdr, options).await?;
		let same = c.boss().filter(|boss| boss.with(|boss| boss.pane_id == pane.pane_id));
		let mut boss = same.map(|boss| boss.get()).unwrap_or_else(|| BossPane {
			pane_id: pane.pane_id.clone(),
			workspace_id: pane.workspace_id.clone(),
			state: pane.agent_state,
			session_path: None,
			event_at: None,
			prompted_at: None,
		});
		boss.workspace_id = pane.workspace_id.clone();
		if let Some(path) = pane.session_path.as_ref().filter(|path| !path.as_os_str().is_empty()) {
			boss.session_path = Some(path.display().to_string());
		}
		c.set_boss(Some(Shared::new(boss)));
		Ok(())
	}

	/// A boss pane whose omp started without this run's key and is idle now, so it can start again with it (0484).
	pub fn unkeyed_boss(&self, pane: &PaneInfo) -> bool {
		self.state().boss_keyed.as_deref() != Some(pane.pane_id.as_str())
			&& pane.agent.as_deref() == Some("omp")
			&& matches!(pane.agent_state, AgentState::Idle | AgentState::Done)
	}

	/// A project or thread pane herdr restored as a shell, or whose omp ended in it (0521), runs omp again in its folder
	/// with the project's overlay (0399, 0400), resuming its session, and its env: a thread's own variables (0457) or,
	/// for a project pane, the hunk variables, both with mise's (0473, 0477); returns the new pane. With `restore` (a
	/// session file), omp goes on with the turn that session's last process left unfinished (0549). A thread's omp
	/// appends its task's brief and done-when again (0540), and a heavy thread's loads `heavy.yml` after the overlay
	/// again (0590). Each start gets its own pane key (0578), kept with the pane.
	pub async fn respawn(&self, pane_id: &str, how: Respawn<'_>) -> Result<String> {
		let c = self.link.ctx();
		let Respawn { cwd, project_id, env, restore, thread } = how;
		let overlay = ensure_overlay(project_id).await?;
		let task = thread.and_then(|thread| thread.with(|thread| thread.task.clone()));
		let pane = if thread.is_some() {
			let flags: Vec<String> = self.pane_omp().iter().cloned().chain(THREAD_FLAGS.map(str::to_owned)).collect();
			self.task_prompted(&flags, task.as_ref())?
		} else {
			self.pane_omp().to_vec()
		};
		let omp = match restore {
			Some(session) => {
				let mut omp: Vec<String> = pane.into_iter().filter(|arg| arg != "--continue").collect();
				omp.extend(["--restore".to_owned(), session.display().to_string()]);
				omp
			}
			None => pane,
		};
		let mut started = omp;
		started.extend(["--config".to_owned(), overlay.display().to_string()]);
		started.extend(after_overlay(task.as_ref()));
		let command = started;
		let key = new_pane_key();
		let mut own = match env {
			Some(env) => env,
			None => self.with_mise(project_id, cwd, self.pane_env()).await,
		};
		set_env(&mut own, PANE_KEY_ENV, key.clone());
		let pane_env = self.bridged(own);
		let opts = RespawnOptions { cwd: cwd.to_path_buf(), command, env: Some(pane_env) };
		let respawned = c.herdr.respawn(pane_id, &opts).await?;
		c.launched.add(&respawned.pane_id, &cwd.display().to_string(), Some(&key))?;
		if respawned.pane_id != pane_id {
			c.launched.forget(pane_id)?;
		}
		Ok(respawned.pane_id)
	}

	/// A sleeping pane's argv (0676): this Jocasta's `pane-sleep` role showing `text`.
	pub fn sleeper_command(&self, text: &str) -> Vec<String> {
		let mut command: Vec<String> = self_command(None).iter().map(|word| word.display().to_string()).collect();
		command.extend([PANE_SLEEP_ROLE.to_owned(), text.to_owned()]);
		command
	}

	/// The pane `pane_id` (its tab replaced, as every respawn does) now runs the sleeper showing `text`, in `cwd`;
	/// returns the new pane. The sleeper's own pane key lets it ask for its wake; `session` is the omp session the wake
	/// brings back (none: `--continue`). The record is kept as a sleeper's (0676).
	pub async fn respawn_asleep(&self, pane_id: &str, cwd: &Path, text: &str, session: Option<&str>) -> Result<String> {
		let c = self.link.ctx();
		let key = new_pane_key();
		let opts = RespawnOptions {
			cwd: cwd.to_path_buf(),
			command: self.sleeper_command(text),
			env: Some(vec![(PANE_KEY_ENV.to_owned(), key.clone())]),
		};
		let respawned = c.herdr.respawn(pane_id, &opts).await?;
		c.launched.add_asleep(&respawned.pane_id, &cwd.display().to_string(), Some(&key), session)?;
		if respawned.pane_id != pane_id {
			c.launched.forget(pane_id)?;
		}
		Ok(respawned.pane_id)
	}

	/// A thread pane's omp argv with its task's launch prompt (0540): `prompt.md` (the panes' shipped prompt, then the
	/// task file's Brief, Done-when and When you finish sections; the Brief alone with the layer off, 0576) as the one
	/// `--append-system-prompt` omp takes, in the shipped prompt's place (before the guard's `--config`). Unchanged
	/// without a recorded brief.
	pub fn task_prompted(&self, command: &[String], task: Option<&TaskRecord>) -> Result<Vec<String>> {
		let Some(task) = task.filter(|task| !js_trim_start(&task.brief).is_empty()) else {
			return Ok(command.to_vec());
		};
		let c = self.link.ctx();
		let file = launch_prompt(task, self.config().pane_prompt.as_deref(), c.layer)?.display().to_string();
		if let Some(at) = command.iter().position(|arg| arg == "--append-system-prompt") {
			return Ok(command
				.iter()
				.enumerate()
				.map(|(i, arg)| if i == at + 1 { file.clone() } else { arg.clone() })
				.collect());
		}
		// `indexOf` gives -1 without a `--config`, and `slice(0, -1)` then cuts before the last word.
		let guard = command.iter().position(|arg| arg == "--config").unwrap_or(command.len().saturating_sub(1));
		let mut argv = command[..guard].to_vec();
		argv.extend(["--append-system-prompt".to_owned(), file]);
		argv.extend_from_slice(&command[guard..]);
		Ok(argv)
	}

	/// `env` with mise's for a pane of `project_id` in `cwd` (0473, 0477); `env` alone without mise.
	pub async fn with_mise(&self, project_id: &str, cwd: &Path, env: &Env) -> Env {
		let c = self.link.ctx();
		let Some(mise) = mise_of(&c.options) else { return env.clone() };
		let mut base = process_env();
		base.extend(env.iter().cloned());
		let mut found: Vec<(String, String)> = mise_env(project_id, cwd, Some(&base), mise).await.into_iter().collect();
		// mise's env comes as a map: its keys are added in name order.
		found.sort();
		let mut merged = env.clone();
		for (name, value) in without_jocasta_keys(found) {
			set_env(&mut merged, &name, value);
		}
		merged
	}

	/// The env the task's checks run with (0542, 0559, SVC-16): the service's own without Jocasta's, herdr's and omp's
	/// keys, and the hunk variables its panes get (not the push hook: a check pushes nothing).
	pub fn check_env(&self) -> HashMap<String, String> {
		let mut checks: HashMap<String, String> =
			process_env().into_iter().filter(|(name, _)| !is_jocasta_key(name)).collect();
		checks.extend(hunk_env(self.hunk_port()));
		checks
	}

	/// A thread pane's env (0457, 0473): the hunk variables, the thread's own, then mise's, once the worktree's unchanged
	/// copies of the main checkout's trusted mise configs are trusted (0596); with the layer off, the layer's switch too,
	/// so its `jocasta mcp` lists no thread tools (0576).
	pub async fn thread_pane_env(&self, project: &Project, path: &Path) -> Result<Env> {
		let c = self.link.ctx();
		let open: Vec<PathBuf> = c.threads.of_project(&project.id).iter().map(|t| t.with(|t| t.path.clone())).collect();
		let own = thread_env(&base_name(&project.main_path), &base_name(path), c.numbers.of(path, &open)?);
		let mut env = self.pane_env().clone();
		for (name, value) in own {
			set_env(&mut env, &name, value);
		}
		if !c.layer {
			set_env(&mut env, LAYER_ENV, "off".to_owned());
		}
		if let Some(mise) = mise_of(&c.options) {
			let mut base = process_env();
			base.extend(env.iter().cloned());
			trust_copies(&project.id, &project.main_path, path, Some(&base), mise).await;
		}
		Ok(self.bridged(self.with_mise(&project.id, path, &env).await))
	}

	/// hunk's argv: the test seam's, else the shipped or npm binary; none when there is none.
	pub fn hunk(&self) -> Option<Vec<String>> {
		match &self.link.ctx().options.hunk {
			Some(hunk) => hunk.clone(),
			None => find_hunk().map(|found| vec![found.display().to_string()]),
		}
	}

	/// Jocasta's hunk session daemon, started when it isn't up; one started here is remembered by pid and start.
	pub async fn ensure_hunk_daemon(&self, hunk: &[String]) {
		let remember = |pid: u32| {
			if let Some(start) = process_start(pid) {
				self.state().hunk_daemon = Some(ProcessId { pid, start });
			}
		};
		if let Err(error) = ensure_hunk_daemon(hunk, self.hunk_port(), Some(&remember)).await {
			self.link.ctx().warn(error);
		}
	}

	/// Ends the hunk session daemon this service started, the same process by pid and start time (0434: Jocasta's
	/// close stops everything it runs; 0486). Run once herdr's panes are gone, so no hunk pane starts another.
	pub async fn end_hunk_daemon(&self) {
		let daemon = self.state().hunk_daemon.take();
		if let Some(daemon) = daemon.filter(|daemon| process_start(daemon.pid) == Some(daemon.start)) {
			end_processes(&[daemon], &[]).await;
		}
	}
}

/// The folder or file name of `path`, as `path.basename` gives it.
fn base_name(path: &Path) -> String {
	path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The mise the options name; none when they name none (`mise: null`).
fn mise_of(options: &ServiceOptions) -> Option<Mise<'_>> {
	match &options.mise {
		None => Some(Mise::OnPath),
		Some(None) => None,
		Some(Some(argv)) => Some(Mise::Argv(argv)),
	}
}

/// How many endings in a row, the last start, and whether the wait for the next was said.
struct Again {
	count: u32,
	at: i64,
	said: bool,
}

/// How many failures in a row and the last try.
struct Failed {
	count: u32,
	at: i64,
}

#[derive(Default)]
struct BackoffState {
	/// omps Jocasta started again after they ended in an open pane (0521), by folder (a pane started again may get a
	/// new id).
	omp_again: HashMap<String, Again>,
	/// Panes whose respawn failed (SVC-3), by folder.
	failures: HashMap<String, Failed>,
	/// The last failure said per folder, so each is said once per reason.
	said: HashMap<String, String>,
}

/// When a Jocasta pane's omp starts again (0521, SVC-3): an omp that ended while its pane stayed open, at once the
/// first time and after a doubling wait for each ending in a row; a respawn that failed, after the same wait for each
/// failure in a row, its failure said once per pane and reason.
pub struct OmpBackoff {
	/// The service's context, reached only through here (SVC-15).
	pub link: Link,
	state: Mutex<BackoffState>,
}

impl OmpBackoff {
	pub fn new(link: Link) -> Self {
		Self { link, state: Mutex::new(BackoffState::default()) }
	}

	fn state(&self) -> MutexGuard<'_, BackoffState> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// A pane Jocasta started whose omp ended while the pane stayed open (0521) and that starts omp again now: at once
	/// the first time, then after a wait that doubles with each ending in a row (10 s first, at most 5 minutes),
	/// so an omp that keeps dying doesn't loop; each wait is said once into Jocasta's log. An omp that ran 10
	/// minutes since its last start ends afresh. Jocasta's own endings never get here: a finished or closed thread's
	/// pane is gone or no longer a thread's, Stop everything stops herdr with its panes, and a stopping service doesn't
	/// refresh.
	pub fn omp_ended(&self, pane: &PaneInfo) -> bool {
		self.omp_ended_at(pane, now_ms())
	}

	fn omp_ended_at(&self, pane: &PaneInfo, now: i64) -> bool {
		let Some(cwd) = pane.cwd.as_deref().filter(|cwd| !cwd.as_os_str().is_empty()) else { return false };
		// A pane being put to sleep or woken, or asleep, ends its omp on purpose: that is no crash (0676).
		let c = self.link.ctx();
		if c.sleep.busy(&pane.pane_id) || c.launched.asleep(&pane.pane_id) {
			return false;
		}
		if !self.link.ctx().launched.ended(pane) {
			return false;
		}
		self.again_at(path_key(&real_path(cwd)), now, |times, seconds| {
			format!(
				"omp ended in pane {} ({}) {times} times in a row; it starts again in {seconds} s.",
				pane.pane_id,
				cwd.display()
			)
		})
	}

	/// A boss pane that closed (his close, or an omp that ended and took the pane with it) opens again: at once the
	/// first time, then after the same doubling wait as [`Self::omp_ended`], so a boss whose omp can't start doesn't
	/// loop on the refreshes its own panes' events wake (0521).
	pub fn boss_closed(&self) -> bool {
		self.boss_closed_at(now_ms())
	}

	fn boss_closed_at(&self, now: i64) -> bool {
		self.again_at(BOSS_AGAIN_KEY.to_owned(), now, |times, seconds| {
			format!("The boss pane closed {times} times in a row; it opens again in {seconds} s.")
		})
	}

	/// Whether what `key` names starts again now under 0521's doubling wait; a wait is said once, by `said` (the times
	/// in a row, the seconds left).
	fn again_at(&self, key: String, now: i64, said: impl FnOnce(u32, i64) -> String) -> bool {
		let waiting = {
			let mut state = self.state();
			let last = state.omp_again.get_mut(&key);
			let count = last.as_ref().map_or(0, |last| if now - last.at < OMP_AGAIN_RESET_MS { last.count } else { 0 });
			let wait = if count == 0 { 0 } else { again_wait(count) };
			match last.filter(|last| now - last.at < wait) {
				// Still waiting: the wait is said once.
				Some(last) => {
					let first = !std::mem::replace(&mut last.said, true);
					let seconds = (wait - (now - last.at) + 999) / 1000;
					Some(first.then(|| said(count + 1, seconds)))
				}
				None => {
					state.omp_again.insert(key, Again { count: count + 1, at: now, said: false });
					None
				}
			}
		};
		let Some(message) = waiting else { return true };
		if let Some(message) = message {
			self.link.ctx().warn(message);
		}
		false
	}

	/// Whether a pane in `cwd` whose respawn failed has waited out 0521's doubling wait (SVC-3); true for any other.
	pub fn due(&self, cwd: Option<&Path>) -> bool {
		self.due_at(cwd, now_ms())
	}

	fn due_at(&self, cwd: Option<&Path>, now: i64) -> bool {
		let Some(cwd) = cwd.filter(|cwd| !cwd.as_os_str().is_empty()) else { return true };
		let key = path_key(&real_path(cwd));
		self.state().failures.get(&key).is_none_or(|failed| now - failed.at >= again_wait(failed.count))
	}

	/// Runs one pane's respawn: a failure is said once per pane and reason and waits before the next try (SVC-3).
	pub async fn respawning<T>(&self, cwd: &Path, run: impl Future<Output = Result<T>>) {
		let key = path_key(&real_path(cwd));
		let outcome = run.await;
		self.settled(&key, cwd, outcome.err(), now_ms());
	}

	fn settled(&self, key: &str, cwd: &Path, error: Option<Error>, now: i64) {
		let Some(error) = error else {
			let mut state = self.state();
			state.failures.remove(key);
			state.said.remove(key);
			return;
		};
		let message = format!("omp didn't start again in {}: {error}", cwd.display());
		let fresh = {
			let mut state = self.state();
			let count = state.failures.get(key).map_or(0, |last| last.count) + 1;
			state.failures.insert(key.to_owned(), Failed { count, at: now });
			let fresh = state.said.get(key) != Some(&message);
			if fresh {
				state.said.insert(key.to_owned(), message.clone());
			}
			fresh
		};
		if fresh {
			self.link.ctx().warn(message);
		}
	}
}
