//! The boss pane (0392, 0404, 0424): workspace "Jocasta", first in the sidebar's list (Alt+1), with one omp pane in
//! `<data>/home`. Its manual is `<data>/home/.omp/APPEND_SYSTEM.md`, which omp loads for every session in that folder
//! when no `--append-system-prompt` is given (omp 19.2.0 `pi-coding-agent/src/main.ts:1526-1549`, `:1692`), so a pane
//! he starts by hand still has it; the procedures it rarely needs are skills in
//! `<data>/home/.omp/skills/` (0465). Both the service and bare `jocasta` ensure it.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde_json::{Map, Value};

use super::surface::{AgentState, Env, PaneInfo, RespawnOptions, Surface, SurfaceSnapshot, TabSpec, WorkspaceInfo};
use crate::shared::bridge_wire::BRIDGE_EXTENSION;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, same_token, write_atomically};
use crate::shared::install::shipped_config_dir;
use crate::shared::pathkey::same_dir;
use crate::shared::paths::{config_dir as jocasta_config_dir, data_dir};

/// The boss's workspace label (0392, 0404).
pub const BOSS_WORKSPACE: &str = "Jocasta";

/// Whether `pane` has an agent in it (a name that is not empty).
fn has_agent(pane: &PaneInfo) -> bool {
	pane.agent.as_deref().is_some_and(|agent| !agent.is_empty())
}

/// The boss pane's folder, `<data>/home`.
pub fn boss_home() -> PathBuf {
	data_dir().join("home")
}

/// The boss pane's command: omp resuming its session line with the boss's own settings (0424), his own
/// `~/.jocasta/agent/boss.yml` when it exists (0697), and the guard last (0450). `config_dir` is the shipped config
/// folder (the install's, or the repo's in a source run, by default).
pub fn boss_command(omp: &[String], config_dir: Option<&Path>) -> Vec<String> {
	let config_dir = config_dir.map_or_else(|| shipped_config_dir(None), Path::to_path_buf);
	let mut command = omp.to_vec();
	command.extend(["--continue".to_owned(), "--config".to_owned(), config_dir.join("boss.yml").display().to_string()]);
	let personal = jocasta_config_dir().join("boss.yml");
	if personal.is_file() {
		command.extend(["--config".to_owned(), personal.display().to_string()]);
	}
	command.extend(["--config".to_owned(), config_dir.join("guard.yml").display().to_string()]);
	// The bridge extension (0675): the boss's tools, and its state for the service.
	command.extend(["-e".to_owned(), config_dir.join(BRIDGE_EXTENSION).display().to_string()]);
	command
}

/// Writes `text` to `file` when it differs.
fn install_text(file: &Path, text: &str) -> Result<()> {
	if fs::read_to_string(file).is_ok_and(|existing| existing == text) {
		return Ok(());
	}
	write_atomically(file, text, None)
}

/// The shipped manual as `<home>/.omp/APPEND_SYSTEM.md`, and each shipped boss skill `config/boss-skills/<name>/` as
/// `<home>/.omp/skills/<name>/` (0465); the shipped TDD lesson goes with `save-lesson`, which hands it to a project (0461).
fn install_boss_files(home: &Path, config_dir: &Path) -> Result<()> {
	let omp = home.join(".omp");
	install_text(&omp.join("APPEND_SYSTEM.md"), &fs::read_to_string(config_dir.join("boss-manual.md"))?)?;
	let skills = config_dir.join("boss-skills");
	for skill in fs::read_dir(&skills)? {
		let skill = skill?;
		if !skill.file_type()?.is_dir() {
			continue;
		}
		let name = skill.file_name();
		for file in fs::read_dir(skills.join(&name))? {
			let file = file?.file_name();
			let text = fs::read_to_string(skills.join(&name).join(&file))?;
			install_text(&omp.join("skills").join(&name).join(&file), &text)?;
		}
	}
	let tdd = fs::read_to_string(config_dir.join("lessons").join("tdd.md"))?;
	install_text(&omp.join("skills").join("save-lesson").join("tdd.md"), &tdd)
}

/// The variable that carries a launched pane's own key into its environment (0578, SEC-NET-6): `jocasta mcp` sends it,
/// and the service knows the pane by it, never by a pane id the caller states.
pub const PANE_KEY_ENV: &str = "JOCASTA_PANE_KEY";

/// A new pane key: 32 random bytes, hex (0578).
pub fn new_pane_key() -> String {
	let mut bytes = [0_u8; 32];
	getrandom::fill(&mut bytes).expect("the OS has random bytes");
	bytes.iter().fold(String::with_capacity(64), |mut hex, byte| {
		let _ = write!(hex, "{byte:02x}");
		hex
	})
}

#[derive(Default)]
struct Launched {
	/// Each recorded pane's entry (`cwd`, `terminal`, `restore`, `key`), in the order `panes.json` keeps them.
	entries: Vec<(String, Map<String, Value>)>,
	/// The terminal each recorded pane's omp was seen running in (0521). Not kept in `panes.json`: an omp that ended
	/// while no service watched (it stopped) isn't one this run saw end.
	ran: HashMap<String, String>,
}

impl Launched {
	fn entry(&self, pane_id: &str) -> Option<&Map<String, Value>> {
		self.entries.iter().find(|(id, _)| id == pane_id).map(|(_, entry)| entry)
	}
}

fn text<'a>(entry: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
	entry.get(field).and_then(Value::as_str)
}

/// `<data>/panes.json`: the panes Jocasta started omp in (the boss, project and thread panes), each with its folder, its
/// pane key (0578: kept, so a service restart knows the panes still working) and the surface's terminal behind it once
/// seen. The own shell keeps no session across a restart (0683), so after one a recorded pane has no terminal: such a
/// pane, in the same folder and without omp, runs omp again with its own flags (its `--config`). A pane that kept its
/// terminal runs omp again only once omp was seen running in that terminal and has ended there (0521, `ended`); no pane
/// Jocasta didn't start is ever touched. A thread pane that was mid-turn when Jocasta stopped everything keeps its
/// session file as `restore` (0549): its omp comes back with `--restore` and goes on with that turn.
///
/// Its methods take `&self` (the record sits behind a lock never held across an await), so one record is shared as
/// `Arc<LaunchedPanes>`.
pub struct LaunchedPanes {
	file: PathBuf,
	state: Mutex<Launched>,
}

fn locked(mutex: &Mutex<Launched>) -> MutexGuard<'_, Launched> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl LaunchedPanes {
	/// The record in `file` (`<data>/panes.json` by default).
	pub fn new(file: Option<&Path>) -> Self {
		let file = file.map_or_else(|| data_dir().join("panes.json"), Path::to_path_buf);
		let mut entries = Vec::new();
		// Only well-formed entries: a hand-edited or torn file must not throw on every refresh (CO-19).
		for (pane_id, entry) in read_json_object(&file).unwrap_or_default() {
			let Some(fields) = entry.as_object() else { continue };
			if text(fields, "cwd").is_none() {
				continue;
			}
			if fields.get("terminal").is_some_and(|terminal| !terminal.is_string()) {
				continue;
			}
			let mut kept = Map::new();
			for field in ["cwd", "terminal", "restore", "key", "asleep"] {
				if let Some(value) = text(fields, field) {
					kept.insert(field.to_owned(), Value::String(value.to_owned()));
				}
			}
			entries.push((pane_id, kept));
		}
		Self { file, state: Mutex::new(Launched { entries, ran: HashMap::new() }) }
	}

	pub fn has(&self, pane_id: &str) -> bool {
		locked(&self.state).entry(pane_id).is_some()
	}

	/// The folder Jocasta started omp in for `pane_id`, or `None` for a pane it didn't start.
	pub fn cwd_of(&self, pane_id: &str) -> Option<String> {
		locked(&self.state).entry(pane_id).and_then(|entry| text(entry, "cwd")).map(str::to_owned)
	}

	/// The panes Jocasta started omp in.
	pub fn panes(&self) -> Vec<String> {
		locked(&self.state).entries.iter().map(|(id, _)| id.clone()).collect()
	}

	/// Jocasta started omp in `pane_id`, in `cwd`, with pane key `key`: an omp not yet seen running (0521, 0578).
	pub fn add(&self, pane_id: &str, cwd: &str, key: Option<&str>) -> Result<()> {
		self.put(pane_id, cwd, key, None)
	}

	/// `pane_id` is a sleeping pane (0676, drawn by the shell since 0689) instead of omp, in `cwd`, with pane key `key`;
	/// `session` is the omp session file to come back to (kept as `restore`, so a wake resumes it; none: `--continue`).
	/// The record is persisted, so a sleeper restored after a restart is a sleeper again.
	pub fn add_asleep(&self, pane_id: &str, cwd: &str, key: Option<&str>, session: Option<&str>) -> Result<()> {
		self.put(pane_id, cwd, key, Some(session.unwrap_or_default()))
	}

	/// The record of `pane_id`, made again: a sleeper (`asleep`: its session, maybe empty) or an omp.
	fn put(&self, pane_id: &str, cwd: &str, key: Option<&str>, asleep: Option<&str>) -> Result<()> {
		let mut entry = Map::new();
		entry.insert("cwd".to_owned(), Value::String(cwd.to_owned()));
		if let Some(key) = key.filter(|key| !key.is_empty()) {
			entry.insert("key".to_owned(), Value::String(key.to_owned()));
		}
		if let Some(session) = asleep {
			entry.insert("asleep".to_owned(), Value::String("true".to_owned()));
			if !session.is_empty() {
				entry.insert("restore".to_owned(), Value::String(session.to_owned()));
			}
		}
		let mut state = locked(&self.state);
		match state.entries.iter_mut().find(|(id, _)| id == pane_id) {
			Some(slot) => slot.1 = entry,
			None => state.entries.push((pane_id.to_owned(), entry)),
		}
		state.ran.remove(pane_id);
		self.save(&state)
	}

	/// Whether `pane_id` runs the sleeper (0676): its omp is ended, and a wake starts it again.
	pub fn asleep(&self, pane_id: &str) -> bool {
		locked(&self.state).entry(pane_id).is_some_and(|entry| text(entry, "asleep") == Some("true"))
	}

	/// The panes that run the sleeper.
	pub fn sleepers(&self) -> Vec<String> {
		locked(&self.state)
			.entries
			.iter()
			.filter(|(_, entry)| text(entry, "asleep") == Some("true"))
			.map(|(id, _)| id.clone())
			.collect()
	}

	/// The key `pane_id` was started with (0578); `None` for a pane started before panes had keys, or not Jocasta's.
	pub fn key_of(&self, pane_id: &str) -> Option<String> {
		locked(&self.state).entry(pane_id).and_then(|entry| text(entry, "key")).map(str::to_owned)
	}

	/// The pane started with `key` (0578), compared in constant time; `None` for any other key.
	pub fn pane_of_key(&self, key: Option<&str>) -> Option<String> {
		let key = key.filter(|key| !key.is_empty())?;
		let state = locked(&self.state);
		let found = state.entries.iter().find(|(_, entry)| same_token(Some(key), text(entry, "key").unwrap_or("")));
		found.map(|(pane_id, _)| pane_id.clone())
	}

	/// Drops the record of `pane_id`, a pane that was replaced by a respawn (it has a new id).
	pub fn forget(&self, pane_id: &str) -> Result<()> {
		let mut state = locked(&self.state);
		state.entries.retain(|(id, _)| id != pane_id);
		state.ran.remove(pane_id);
		self.save(&state)
	}

	/// `pane_id`'s omp was mid-turn in session file `session` as Jocasta stopped (0549): its next start restores it.
	pub fn pause(&self, pane_id: &str, session: &str) -> Result<()> {
		let mut state = locked(&self.state);
		let Some(slot) = state.entries.iter_mut().find(|(id, _)| id == pane_id) else { return Ok(()) };
		slot.1.insert("restore".to_owned(), Value::String(session.to_owned()));
		self.save(&state)
	}

	/// The session `pane_id`'s omp was mid-turn in when Jocasta last stopped, or `None`.
	pub fn paused(&self, pane_id: &str) -> Option<String> {
		locked(&self.state).entry(pane_id).and_then(|entry| text(entry, "restore")).map(str::to_owned)
	}

	/// Notes each recorded pane's terminal the first time it is seen, and the terminal its omp is seen running in
	/// (0521); forgets panes herdr no longer has.
	pub fn seen(&self, panes: &[PaneInfo]) -> Result<()> {
		// herdr answering with no panes at all is a server still restoring, not every pane closed.
		if panes.is_empty() {
			return Ok(());
		}
		let mut guard = locked(&self.state);
		let state = &mut *guard;
		let mut changed = false;
		let mut gone = Vec::new();
		for (pane_id, entry) in &mut state.entries {
			let Some(pane) = panes.iter().find(|p| &p.pane_id == pane_id) else {
				gone.push(pane_id.clone());
				continue;
			};
			let terminal = pane.terminal_id.as_deref().filter(|terminal| !terminal.is_empty());
			if let (None, Some(terminal)) = (entry.get("terminal"), terminal) {
				entry.insert("terminal".to_owned(), Value::String(terminal.to_owned()));
				changed = true;
			}
			if let (true, Some(terminal)) = (has_agent(pane), terminal)
				&& text(entry, "terminal") == Some(terminal)
			{
				state.ran.insert(pane_id.clone(), terminal.to_owned());
			}
		}
		for pane_id in &gone {
			state.ran.remove(pane_id);
		}
		if !gone.is_empty() {
			state.entries.retain(|(id, _)| !gone.contains(id));
			changed = true;
		}
		if changed { self.save(state) } else { Ok(()) }
	}

	/// A pane Jocasta started that herdr restored as a shell: same folder, a new terminal, no omp.
	pub fn restored(&self, pane: &PaneInfo) -> bool {
		let state = locked(&self.state);
		let Some(entry) = state.entry(&pane.pane_id) else { return false };
		let (Some(terminal), Some(now)) = (text(entry, "terminal"), pane.terminal_id.as_deref()) else { return false };
		if now.is_empty() || terminal == now {
			return false;
		}
		shell_in(pane, text(entry, "cwd").unwrap_or(""))
	}

	/// A pane Jocasta started whose omp ended while the pane stayed open (0521): omp was seen running in its terminal,
	/// which it keeps, and it is a shell in the same folder now. Never a pane whose omp hasn't come up yet.
	pub fn ended(&self, pane: &PaneInfo) -> bool {
		let state = locked(&self.state);
		let Some(entry) = state.entry(&pane.pane_id) else { return false };
		let (Some(terminal), Some(now)) = (text(entry, "terminal"), pane.terminal_id.as_deref()) else { return false };
		if now.is_empty() || terminal != now {
			return false;
		}
		if state.ran.get(&pane.pane_id).map(String::as_str) != Some(now) {
			return false;
		}
		shell_in(pane, text(entry, "cwd").unwrap_or(""))
	}

	fn save(&self, state: &Launched) -> Result<()> {
		let entries: Map<String, Value> =
			state.entries.iter().map(|(id, entry)| (id.clone(), Value::Object(entry.clone()))).collect();
		write_atomically(&self.file, format!("{}\n", serde_json::to_string_pretty(&entries)?), None)
	}
}

/// `pane` runs no agent, in folder `cwd`.
fn shell_in(pane: &PaneInfo, cwd: &str) -> bool {
	if has_agent(pane) || pane.agent_state != AgentState::Unknown {
		return false;
	}
	pane.cwd.as_deref().is_some_and(|dir| !dir.as_os_str().is_empty() && same_dir(dir, Path::new(cwd)))
}

/// Whether a found boss pane's omp must start again.
pub type BossStale<'a> = Box<dyn Fn(&PaneInfo) -> bool + Send + Sync + 'a>;

/// Called with the boss pane's id each time its omp is started.
pub type BossStarted<'a> = Box<dyn Fn(&str) + Send + Sync + 'a>;

pub struct EnsureBossOptions<'a> {
	/// The boss pane's argv.
	pub command: Vec<String>,
	/// The boss pane's own variables (this run's boss key, 0484), whenever its omp is started.
	pub env: Env,
	/// The panes Jocasta started: the boss pane joins them; a restored one runs omp again.
	pub launched: &'a LaunchedPanes,
	/// The service's start, or a boss pane whose omp ended in it (0521): a boss pane with no agent in it runs omp even
	/// when `launched` has no record of it (herdr restored workspace "Jocasta" from its own saved session, or a pane from
	/// an earlier build). Not on every refresh: a just-started omp reports nothing for a moment.
	pub adopt: bool,
	/// A found boss pane whose omp must start again (it lacks this run's `env`, 0484).
	pub stale: Option<BossStale<'a>>,
	/// Called with the boss pane's id each time its omp is started here, with `env`.
	pub started: Option<BossStarted<'a>>,
	pub home: Option<PathBuf>,
	pub config_dir: Option<PathBuf>,
}

impl<'a> EnsureBossOptions<'a> {
	/// Options with `command` and `launched` and everything else off.
	pub fn new(command: Vec<String>, launched: &'a LaunchedPanes) -> Self {
		Self {
			command,
			env: Env::new(),
			launched,
			adopt: false,
			stale: None,
			started: None,
			home: None,
			config_dir: None,
		}
	}
}

async fn boss_workspaces(herdr: &dyn Surface) -> Result<Vec<WorkspaceInfo>> {
	let all = herdr.workspaces().await?;
	Ok(all.into_iter().filter(|w| w.label == BOSS_WORKSPACE).collect())
}

/// The boss's pane in `workspace_id`: one Jocasta started, else one running omp, else the workspace's only pane.
async fn in_boss(herdr: &dyn Surface, workspace_id: &str, launched: &LaunchedPanes) -> Result<Option<PaneInfo>> {
	let panes: Vec<PaneInfo> = herdr.panes().await?.into_iter().filter(|p| p.workspace_id == workspace_id).collect();
	let found = panes
		.iter()
		.find(|p| launched.has(&p.pane_id))
		.or_else(|| panes.iter().find(|p| p.agent.as_deref() == Some("omp")))
		.or_else(|| if panes.len() == 1 { panes.first() } else { None });
	Ok(found.cloned())
}

async fn pane_by_id(herdr: &dyn Surface, pane_id: &str) -> Result<Option<PaneInfo>> {
	Ok(herdr.panes().await?.into_iter().find(|p| p.pane_id == pane_id))
}

/// Where and how the boss's omp starts this time.
struct Start {
	home: PathBuf,
	env: Env,
	key: String,
}

fn record_start(options: &EnsureBossOptions<'_>, start: &Start, pane_id: &str) -> Result<()> {
	options.launched.add(pane_id, &start.home.display().to_string(), Some(&start.key))?;
	if let Some(started) = &options.started {
		started(pane_id);
	}
	Ok(())
}

/// Starts the boss: a tab in workspace `workspace_id` when there is one, else the workspace (CR-10: the oldest
/// "Jocasta" stays, any other is closed). Returns the workspace and its pane.
async fn make_boss(
	herdr: &dyn Surface,
	options: &EnsureBossOptions<'_>,
	start: &Start,
	workspace_id: Option<String>,
) -> Result<(String, PaneInfo)> {
	let (workspace_id, pane) = if let Some(workspace_id) = workspace_id {
		let tab = TabSpec {
			title: BOSS_WORKSPACE.to_owned(),
			cwd: start.home.clone(),
			command: options.command.clone(),
			env: Some(start.env.clone()),
		};
		let tab = herdr.create_tab(&workspace_id, &tab).await?;
		let pane = pane_by_id(herdr, &tab.pane_id).await?;
		(workspace_id, pane)
	} else {
		let made =
			herdr.ensure_workspace(BOSS_WORKSPACE, &start.home, Some(&options.command), Some(&start.env)).await?;
		let mut all: Vec<String> = boss_workspaces(herdr).await?.into_iter().map(|w| w.id).collect();
		all.sort_by(|a, b| by_creation(a, b));
		let workspace_id = all.first().cloned().unwrap_or(made);
		for other in all.iter().filter(|other| **other != workspace_id) {
			herdr.close_workspace(other).await?;
		}
		let pane = in_boss(herdr, &workspace_id, options.launched).await?;
		(workspace_id, pane)
	};
	let pane = pane.ok_or_else(|| Error::other(format!("herdr made workspace {BOSS_WORKSPACE} without a pane")))?;
	record_start(options, start, &pane.pane_id)?;
	Ok((workspace_id, pane))
}

/// Runs omp again in a boss pane herdr restored as a shell, or whose omp is stale.
async fn respawn_boss(
	herdr: &dyn Surface,
	options: &EnsureBossOptions<'_>,
	start: &Start,
	pane: &PaneInfo,
) -> Result<PaneInfo> {
	let opts =
		RespawnOptions { cwd: start.home.clone(), command: options.command.clone(), env: Some(start.env.clone()) };
	let respawned = herdr.respawn(&pane.pane_id, &opts).await?;
	let pane = pane_by_id(herdr, &respawned.pane_id)
		.await?
		.ok_or_else(|| Error::other(format!("herdr's respawned boss pane {} is gone", respawned.pane_id)))?;
	record_start(options, start, &pane.pane_id)?;
	Ok(pane)
}

/// Workspace "Jocasta" with its boss pane, made when missing and moved first when it isn't (0404); a boss pane herdr
/// restored as a shell runs omp again. Bare `jocasta` and the service may both make it at once (herdr's list and
/// create aren't one step): after making it, the oldest "Jocasta" stays and any other is closed (CR-10). The boss pane
/// is one Jocasta started or one running omp; another pane is adopted only when it is the workspace's only one, so his
/// own shell tab there is never taken over (CO-21).
pub async fn ensure_boss_pane(herdr: &dyn Surface, options: EnsureBossOptions<'_>) -> Result<PaneInfo> {
	let home = options.home.clone().unwrap_or_else(boss_home);
	let config_dir = options.config_dir.clone().unwrap_or_else(|| shipped_config_dir(None));
	install_boss_files(&home, &config_dir)?;
	let mut workspace_id = boss_workspaces(herdr).await?.first().map(|w| w.id.clone());
	let mut pane = match &workspace_id {
		Some(id) => in_boss(herdr, id, options.launched).await?,
		None => None,
	};
	// Each start of the boss's omp gets its own pane key (0578), kept with the pane.
	let key = new_pane_key();
	let mut env = options.env.clone();
	match env.iter_mut().find(|(name, _)| name == PANE_KEY_ENV) {
		Some(slot) => slot.1.clone_from(&key),
		None => env.push((PANE_KEY_ENV.to_owned(), key.clone())),
	}
	let start = Start { home, env, key };
	pane = match pane {
		None => {
			let (id, made) = make_boss(herdr, &options, &start, workspace_id.take()).await?;
			workspace_id = Some(id);
			Some(made)
		}
		Some(found) => {
			let again = options.launched.restored(&found)
				|| (options.adopt && !has_agent(&found) && found.agent_state == AgentState::Unknown)
				|| options.stale.as_ref().is_some_and(|stale| stale(&found));
			if again { Some(respawn_boss(herdr, &options, &start, &found).await?) } else { Some(found) }
		}
	};
	if let (Some(id), Some(pane)) = (workspace_id, pane) {
		if herdr.workspaces().await?.first().map(|w| w.id.as_str()) != Some(id.as_str()) {
			herdr.move_workspace(&id, 0).await?;
		}
		return Ok(pane);
	}
	Err(Error::other(format!("herdr has no workspace {BOSS_WORKSPACE}")))
}

/// herdr's workspace ids in the order herdr made them: its ids are `w<number>` (a number that only grows), so both
/// racers of CR-10 keep the same one.
fn by_creation(a: &str, b: &str) -> Ordering {
	fn chunks(text: &str) -> Vec<(bool, &str)> {
		let mut out = Vec::new();
		let mut start = 0;
		let mut kind = None;
		for (at, c) in text.char_indices() {
			let digit = c.is_ascii_digit();
			if kind.is_some_and(|k| k != digit) {
				out.push((!digit, &text[start..at]));
				start = at;
			}
			kind = Some(digit);
		}
		if let Some(digit) = kind {
			out.push((digit, &text[start..]));
		}
		out
	}
	let (left, right) = (chunks(a), chunks(b));
	for (x, y) in left.iter().zip(&right) {
		let order = match (x.0, y.0) {
			(true, true) => {
				let (xd, yd) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
				xd.len().cmp(&yd.len()).then_with(|| xd.cmp(yd))
			}
			(false, false) => x.1.to_lowercase().cmp(&y.1.to_lowercase()),
			(true, false) => Ordering::Less,
			(false, true) => Ordering::Greater,
		};
		if order != Ordering::Equal {
			return order;
		}
	}
	left.len().cmp(&right.len())
}

/// Whether a session whose server Jocasta just started is new (0495): it has no "Jocasta" workspace and at most one
/// other, herdr's default, made when his client attached to the empty session first. A session herdr restored from its
/// saved state lists the "Jocasta" workspace it had.
pub fn fresh_session(workspaces: &[WorkspaceInfo]) -> bool {
	workspaces.len() <= 1 && !workspaces.iter().any(|w| w.label == BOSS_WORKSPACE)
}

/// What `close_herdr_default` found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultClosed {
	Wait,
	None,
	Closed,
}

/// herdr's default workspace in a session Jocasta created (0495): herdr makes it only while a session has no workspace,
/// so it is the one made before "Jocasta". It closes while untouched: its one pane a shell (no agent) in his home
/// folder (links followed: macOS reports `/tmp` as `/private/tmp`) whose screen (`pane.read`) is one prompt with
/// nothing typed after it. `Wait` while there is nothing to judge yet ("Jocasta" not made yet, or a shell that hasn't
/// drawn its prompt); `None` when there is no default to close ("Jocasta" came first) or it was touched. `home` is his
/// home folder by default.
pub async fn close_herdr_default(
	herdr: &dyn Surface,
	snapshot: &SurfaceSnapshot,
	home: Option<&Path>,
) -> Result<DefaultClosed> {
	let home = home.map_or_else(|| std::env::home_dir().unwrap_or_default(), Path::to_path_buf);
	let boss = snapshot.workspaces.iter().find(|w| w.label == BOSS_WORKSPACE);
	let first = snapshot.workspaces.iter().min_by(|a, b| by_creation(&a.id, &b.id));
	let (Some(boss), Some(first)) = (boss, first) else { return Ok(DefaultClosed::Wait) };
	if first.id == boss.id {
		return Ok(DefaultClosed::None);
	}
	let panes: Vec<&PaneInfo> = snapshot.panes.iter().filter(|p| p.workspace_id == first.id).collect();
	let [pane] = panes[..] else { return Ok(DefaultClosed::None) };
	let Some(cwd) = pane.cwd.as_deref().filter(|cwd| !cwd.as_os_str().is_empty()) else {
		return Ok(DefaultClosed::None);
	};
	if has_agent(pane) || pane.agent_state != AgentState::Unknown || !same_dir(cwd, &home) {
		return Ok(DefaultClosed::None);
	}
	let screen = herdr.screen(&pane.pane_id).await?;
	let lines: Vec<&str> = screen.split('\n').map(str::trim_end).filter(|line| !line.is_empty()).collect();
	let [line] = lines[..] else {
		return Ok(if lines.is_empty() { DefaultClosed::Wait } else { DefaultClosed::None });
	};
	if !line.ends_with(['>', '$', '#', '%']) {
		return Ok(DefaultClosed::None);
	}
	herdr.close_workspace(&first.id).await?;
	Ok(DefaultClosed::Closed)
}

#[cfg(test)]
#[path = "boss_tests.rs"]
mod tests;
