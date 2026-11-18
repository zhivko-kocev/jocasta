//! `jocasta setup` (0387, 0392, 0393, 0396, 0401, 0430, 0440, 0466, 0467, 0476, 0479, 0594): what Jocasta adds to omp,
//! herdr, Windows Terminal and mise, and the calendar's secret in Windows Credential Manager. Each thing is a numbered
//! migration with a marker in `~/.jocasta/agent/setup/` (0466): it is pending until applied, and again when what Jocasta
//! ships for it changes, so one runs once per shipped version and a file he edits afterwards is left alone until then
//! (`--refresh` resets one, 0467). Every pending change is shown as a diff and written only after his `y`, in order; a
//! failed one stops the rest, and so does a file changed since its diff was shown (0584). Jocasta's entries in omp's MCP
//! configs are merged beside his other servers; omp's settings are merged into his `config.yml` key by key, keys he set
//! to another value kept and shown, and so are its keys in his `keybindings.yml` (only their lines edited) and
//! `lsp.json` (0479, 0584); his own advisor brief (`WATCHDOG.md`) is shown and kept, and his own `secrets.yml` is kept
//! and never shown (0594). Every file is written through a temp file and a rename, so a crash never leaves one torn.
//! Each marker names how to undo its migration (0468).
//!
//! The shipped `config/` is read from `assets_dir` (the install's, or the repo's in a source run, as
//! `shared::install::shipped_config_dir` says): the TypeScript embeds nothing of it either.
//!
//! The module is split by `#[path]` children: `setup_steps.rs` (what a step is and the helpers that build one) and
//! `setup_migrations.rs` (the numbered migrations).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Local};
use futures_util::future::BoxFuture;
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::frontier::server::whiteboard::{WhiteboardRuntime, whiteboard_runtime};
use crate::frontier::tui::TOKYO_NIGHT;
use crate::setup::markers::{Marker, Undo, read_marker, recorded_undo, write_marker};
use crate::shared::credentials::{CALENDAR_CREDENTIAL, CALENDAR_ENV, read_credential, write_credential};
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::herdr_session::herdr_program;
use crate::shared::install::{self_command, shipped_config_dir};
use crate::shared::log::{LogLevel, log};
use crate::shared::pathkey::{path_key, same_path};
use crate::shared::paths::{config_dir, data_dir, omp_agent_dir};
use crate::shared::process::{RunOptions, on_path, run_program};

#[path = "setup_migrations.rs"]
mod migrations;
#[path = "setup_steps.rs"]
mod steps;

use migrations::{HERDR_MISSING, MIGRATIONS, Migration, marker_name};
use steps::{SetupChange, Step};

/// Where setup reads and writes; every path injectable so tests never touch his homes.
#[derive(Debug, Clone)]
pub struct SetupPaths {
	/// omp's agent dir, `~/.omp/agent` (`PI_CODING_AGENT_DIR` overrides it, as in omp; `omp_agent_dir`).
	pub omp_agent_dir: PathBuf,
	/// Jocasta's config dir, `~/.jocasta/agent`.
	pub jocasta_config_dir: PathBuf,
	/// Jocasta's data dir; the boss pane runs in `<data>/home`.
	pub data_dir: PathBuf,
	/// The shipped `config/` (boss manual, templates, skills): beside `jocasta.exe`, or the repo's in a source run.
	pub assets_dir: PathBuf,
	/// `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments`; `None` off Windows.
	pub terminal_fragments_dir: Option<PathBuf>,
	/// Windows' own folders omp fences (0594): `%SystemRoot%` and `%ProgramFiles%`; `None` off Windows.
	pub system_folders: Option<Vec<String>>,
	/// mise's global config dir, `~/.config/mise` (`MISE_CONFIG_DIR` overrides it, as in mise).
	pub mise_config_dir: PathBuf,
	/// How omp starts this Jocasta: `[jocasta.exe]`; in a source run the running executable.
	pub jocasta_command: Vec<PathBuf>,
	/// whiteboard's CLI for `whiteboard mcp` (0393): beside `jocasta.exe`, or the built clone; `None` when absent.
	pub whiteboard: Option<WhiteboardRuntime>,
}

/// What a program run left: its exit code and combined output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
	pub code: i32,
	pub output: String,
}

/// Runs a program to completion; resolves its exit code and combined output.
pub type RunProgram = Arc<dyn Fn(Vec<String>) -> BoxFuture<'static, Result<RunOutput>> + Send + Sync>;

/// What `herdr config check` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrCheck {
	pub code: Option<i32>,
	pub output: String,
}

/// The calendar secret's store (0430): Windows Credential Manager target `jocasta/calendar-ical`.
pub trait CalendarStore: Send + Sync {
	/// Whether the secret is stored.
	fn stored(&self) -> bool;
	fn save(&self, secret: &str) -> Result<()>;
	/// `JOCASTA_CALENDAR_ICAL_SECRET`'s value, imported when the store has none.
	fn env_secret(&self) -> Option<String>;
}

/// The logon Run value the companion wrote (0367), read and removed (0557).
pub trait RunValueStore: Send + Sync {
	fn read(&self, name: &str) -> Option<String>;
	fn remove(&self, name: &str) -> bool;
}

pub struct SetupDeps {
	/// The herdr binary, or `None` when herdr isn't installed.
	pub herdr: Option<PathBuf>,
	/// The mise binary, or `None` when mise isn't installed: then setup writes no mise file (0476).
	pub mise: Option<PathBuf>,
	pub run: RunProgram,
	/// The calendar secret's store; `None` where there is none (off Windows).
	pub calendar: Option<Arc<dyn CalendarStore>>,
	/// `herdr config check` on a config file (herdr, path), for setup's check of the herdr.toml it wrote (0608);
	/// `run_program` by default.
	#[allow(clippy::type_complexity)]
	pub check_herdr_config:
		Option<Arc<dyn Fn(PathBuf, PathBuf) -> BoxFuture<'static, Result<HerdrCheck>> + Send + Sync>>,
	/// `git --version`'s output, or `None` when git can't run (0613); no function here skips the check.
	#[allow(clippy::type_complexity)]
	pub git_version: Option<Arc<dyn Fn() -> BoxFuture<'static, Option<String>> + Send + Sync>>,
	/// `herdr server reload-config` for Jocasta's session, after a herdr.toml refresh (0467).
	#[allow(clippy::type_complexity)]
	pub reload_herdr: Option<Arc<dyn Fn() -> BoxFuture<'static, Result<RunOutput>> + Send + Sync>>,
	/// The logon Run value the companion wrote (0367), read and removed (0557); `None` off Windows and in tests, so
	/// nothing touches his registry there.
	pub run_value: Option<Arc<dyn RunValueStore>>,
}

impl Clone for SetupDeps {
	fn clone(&self) -> Self {
		Self {
			herdr: self.herdr.clone(),
			mise: self.mise.clone(),
			run: self.run.clone(),
			calendar: self.calendar.clone(),
			check_herdr_config: self.check_herdr_config.clone(),
			git_version: self.git_version.clone(),
			reload_herdr: self.reload_herdr.clone(),
			run_value: self.run_value.clone(),
		}
	}
}

impl SetupDeps {
	/// Deps with the two required members and nothing else.
	pub fn new(herdr: Option<PathBuf>, run: RunProgram) -> Self {
		Self {
			herdr,
			mise: None,
			run,
			calendar: None,
			check_herdr_config: None,
			git_version: None,
			reload_herdr: None,
			run_value: None,
		}
	}
}

/// How setup talks to him.
pub trait SetupIo: Send + Sync {
	fn print(&self, line: &str);
	/// Asks one question; resolves his answer, or `None` at end of input.
	fn ask<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>>;
	/// Asks for a secret, kept off the screen and its scrollback (SEC-LOC-12); `ask` where there is none.
	fn ask_secret<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		self.ask(question)
	}
}

/// How an apply step is run: with his terminal for the questions and lines it has.
type Apply = Box<dyn for<'a> FnOnce(&'a dyn SetupIo) -> BoxFuture<'a, Result<()>> + Send>;

/// A change, or a migration already in place, with the marker written once it is applied.
struct PlannedChange {
	/// The marker's name: `<NNNN>-<slug>` for a migration, `calendar` for the calendar's secret.
	name: String,
	/// The migration's slug (empty for the calendar's secret).
	slug: String,
	/// The file it is about.
	path: String,
	/// `None` when the target already is what Jocasta ships: only the marker is written.
	change: Option<SetupChange>,
	/// The marker to write once applied; its `applied_at` is set then.
	marker: Marker,
}

struct SetupPlan {
	/// Pending changes, in migration order.
	changes: Vec<PlannedChange>,
	/// Migrations already in place without a current marker: marked when setup runs.
	settled: Vec<PlannedChange>,
	/// Things setup can't do now, said before the diff.
	notes: Vec<String>,
}

/// Credential Manager as the calendar's store, with the env value taken at start.
struct CredentialStore {
	from_env: Option<String>,
}

impl CalendarStore for CredentialStore {
	fn stored(&self) -> bool {
		read_credential(CALENDAR_CREDENTIAL).is_some()
	}

	fn save(&self, secret: &str) -> Result<()> {
		write_credential(CALENDAR_CREDENTIAL, secret)
	}

	fn env_secret(&self) -> Option<String> {
		self.from_env.clone()
	}
}

/// Credential Manager as the calendar's store, with the env value taken at start; `None` off Windows.
pub fn default_calendar_store(from_env: Option<String>) -> Option<Arc<dyn CalendarStore>> {
	cfg!(windows).then(|| Arc::new(CredentialStore { from_env }) as Arc<dyn CalendarStore>)
}

fn homedir() -> PathBuf {
	std::env::home_dir().unwrap_or_default()
}

/// mise's global config dir as mise resolves it: `MISE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/mise`, else `~/.config/mise`.
pub fn mise_config_dir() -> PathBuf {
	if let Some(dir) = env::var_nonempty("MISE_CONFIG_DIR") {
		return PathBuf::from(dir);
	}
	env::var_nonempty("XDG_CONFIG_HOME").map_or_else(|| homedir().join(".config"), PathBuf::from).join("mise")
}

/// The real paths on this machine.
pub fn default_setup_paths(assets_dir: &Path, jocasta_command: Vec<PathBuf>) -> SetupPaths {
	let local =
		env::var_nonempty("LOCALAPPDATA").map_or_else(|| homedir().join("AppData").join("Local"), PathBuf::from);
	SetupPaths {
		omp_agent_dir: omp_agent_dir(None),
		jocasta_config_dir: config_dir(),
		data_dir: data_dir(),
		assets_dir: assets_dir.to_path_buf(),
		terminal_fragments_dir: cfg!(windows)
			.then(|| local.join("Microsoft").join("Windows Terminal").join("Fragments")),
		system_folders: cfg!(windows).then(|| {
			vec![
				env::var_nonempty("SystemRoot").unwrap_or_else(|| "C:\\Windows".to_owned()),
				env::var_nonempty("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".to_owned()),
			]
		}),
		mise_config_dir: mise_config_dir(),
		jocasta_command,
		whiteboard: whiteboard_runtime(),
	}
}

/// The file herdr's omp integration writes (`herdr:src/integration/targets.rs:89-106`).
pub const HERDR_OMP_EXTENSION: &str = "herdr-omp-agent-state.ts";

/// herdr's Tokyo Night as a Windows Terminal scheme, so pane content matches the chrome (0401), from the one palette
/// file (0474). A fragment scheme must define all 16 colours.
pub static TOKYO_NIGHT_SCHEME: LazyLock<Value> = LazyLock::new(|| {
	use ratatui::style::Color;
	let hex = |colour: Color| match colour {
		Color::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
		_ => String::new(),
	};
	let p = &TOKYO_NIGHT;
	json!({
		"name": "Tokyo Night",
		"background": hex(p.bg0),
		"foreground": hex(p.fg),
		"cursorColor": hex(p.fg),
		"selectionBackground": hex(p.bg3),
		"black": hex(p.bg2),
		"red": hex(p.red),
		"green": hex(p.green),
		"yellow": hex(p.yellow),
		"blue": hex(p.blue),
		"purple": hex(p.purple),
		"cyan": hex(p.cyan),
		"white": hex(p.fg2),
		"brightBlack": hex(p.bright_black),
		"brightRed": hex(p.red),
		"brightGreen": hex(p.green),
		"brightYellow": hex(p.yellow),
		"brightBlue": hex(p.blue),
		"brightPurple": hex(p.purple),
		"brightCyan": hex(p.cyan),
		"brightWhite": hex(p.fg),
	})
});

/// An omp `mcp.json` undo with what setup wrote there (0468): each server goes only while it still is that. A marker
/// names only the servers and a fingerprint of the entries written, so the entries setup writes today stand in for them.
#[derive(Debug, Clone, PartialEq)]
pub struct McpUndo {
	pub path: String,
	pub servers: Vec<String>,
	/// The entries setup writes there today, by name.
	pub writes: Map<String, Value>,
	/// The marker's fingerprint of the entries it wrote: present entries that still match it are setup's too.
	pub wrote: String,
}

/// One thing setup added, as uninstall undoes it.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupUndo {
	/// Every undo but an MCP one.
	Other(Undo),
	Mcp(McpUndo),
}

impl SetupUndo {
	/// The undo as JSON, keys in the TypeScript's order (the MCP one carries `writes` and `wrote` after its own).
	pub fn to_json(&self) -> Value {
		match self {
			Self::Other(undo) => undo.to_json(),
			Self::Mcp(mcp) => json!({
				"kind": "mcp", "path": mcp.path, "servers": mcp.servers, "writes": mcp.writes, "wrote": mcp.wrote,
			}),
		}
	}
}

/// How to undo what setup added on this machine (0468): only what its markers recorded (a step without its marker
/// isn't setup's to undo), the calendar's secret while it is stored. Reads only.
pub fn setup_undo(paths: &SetupPaths, calendar_stored: bool) -> Vec<SetupUndo> {
	let mut written: Vec<(String, Map<String, Value>, String)> = Vec::new();
	for migration in &MIGRATIONS {
		let marker = read_marker(&paths.jocasta_config_dir, &marker_name(migration));
		let Some(servers) = migration.servers else { continue };
		let Some(marker) = marker.filter(|m| m.undo.iter().any(|u| matches!(u, Undo::Mcp { .. }))) else { continue };
		// A marker from before 0607 names whiteboard too: its entry is the one migration 1 wrote then.
		let mut writes = servers(paths);
		if let Some(legacy) = migration.legacy_servers {
			writes.extend(legacy(paths));
		}
		for undo in &marker.undo {
			if let Undo::Mcp { path, .. } = undo {
				let key = path_key(Path::new(path));
				written.retain(|(k, ..)| *k != key);
				written.push((key, writes.clone(), marker.fingerprint.clone()));
			}
		}
	}
	let mut undo: Vec<SetupUndo> = Vec::new();
	for item in recorded_undo(&paths.jocasta_config_dir) {
		match item {
			Undo::Credential { .. } if !calendar_stored => {}
			Undo::Mcp { path, servers } => {
				let found = written.iter().find(|(key, ..)| *key == path_key(Path::new(&path)));
				let (writes, wrote) = found.map_or((Map::new(), String::new()), |(_, w, f)| (w.clone(), f.clone()));
				undo.push(SetupUndo::Mcp(McpUndo { path, servers, writes, wrote }));
			}
			other => undo.push(SetupUndo::Other(other)),
		}
	}
	// A file in a folder that goes whole goes with it; folders go after files, so one setup may not have made goes
	// only once it is empty (a marker from before 0484).
	let folders: Vec<PathBuf> = undo
		.iter()
		.filter_map(|item| match item {
			SetupUndo::Other(Undo::Dir { path, file: Some(_) }) => Some(PathBuf::from(path)),
			_ => None,
		})
		.collect();
	let in_folder = |item: &SetupUndo| match item {
		SetupUndo::Other(Undo::File { path }) => {
			Path::new(path).parent().is_some_and(|dir| folders.iter().any(|folder| same_path(dir, folder)))
		}
		_ => false,
	};
	let mut seen: HashSet<String> = HashSet::new();
	let unique: Vec<SetupUndo> = undo
		.into_iter()
		.filter(|item| {
			let key = item.to_json().to_string().to_lowercase();
			!in_folder(item) && seen.insert(key)
		})
		.collect();
	let is_dir = |item: &SetupUndo| matches!(item, SetupUndo::Other(Undo::Dir { .. }));
	let (dirs, rest): (Vec<_>, Vec<_>) = unique.into_iter().partition(is_dir);
	[rest, dirs].concat()
}

/// Whether he answered yes (`y` or `yes`, any case).
pub fn answered_yes(answer: Option<&str>) -> bool {
	answer.is_some_and(|answer| matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// The marker as JavaScript's `new Date().toISOString()` writes the time.
fn now_iso() -> String {
	chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn mark(paths: &SetupPaths, planned: &PlannedChange) -> Result<()> {
	write_marker(&paths.jocasta_config_dir, &planned.name, &Marker { applied_at: now_iso(), ..planned.marker.clone() })
}

fn print_change(io: &dyn SetupIo, change: &SetupChange) {
	io.print("");
	io.print(&format!("{}: {}", change.title, change.path));
	for line in &change.diff {
		io.print(line);
	}
	for line in &change.kept {
		io.print(&format!("  kept: {line}"));
	}
}

/// `migrations`' steps now; `force` plans those with a current marker too (`--refresh`).
fn plan_migrations(
	paths: &SetupPaths,
	deps: &SetupDeps,
	notes: &mut Vec<String>,
	force: bool,
	migrations: &[&Migration],
) -> Result<Vec<PlannedChange>> {
	let mut planned = Vec::new();
	for migration in migrations {
		let name = marker_name(migration);
		let marker = read_marker(&paths.jocasta_config_dir, &name);
		let current = marker.as_ref().map(|m| m.fingerprint.clone());
		let mut context = migrations::PlanContext { paths, deps, notes, marker };
		let Some(step) = (migration.plan)(&mut context)? else { continue };
		if !force && current.as_deref() == Some(step.fingerprint.as_str()) {
			continue;
		}
		let Step { path, fingerprint, change, undo, shipped } = step;
		planned.push(PlannedChange {
			name,
			slug: migration.slug.to_owned(),
			path,
			change,
			marker: Marker { fingerprint, undo, shipped, applied_at: String::new() },
		});
	}
	Ok(planned)
}

fn calendar_marker() -> Marker {
	Marker {
		fingerprint: CALENDAR_CREDENTIAL.to_owned(),
		undo: vec![Undo::Credential { target: CALENDAR_CREDENTIAL.to_owned() }],
		shipped: None,
		applied_at: String::new(),
	}
}

/// Every change setup would make now, and what it can't do. Reads only.
fn plan_setup(paths: &SetupPaths, deps: &SetupDeps) -> Result<SetupPlan> {
	let mut notes = Vec::new();
	let all: Vec<&Migration> = MIGRATIONS.iter().collect();
	let planned = plan_migrations(paths, deps, &mut notes, false, &all)?;
	let (mut changes, settled): (Vec<_>, Vec<_>) = planned.into_iter().partition(|p| p.change.is_some());
	if let Some(calendar) =
		deps.calendar.as_ref().filter(|c| c.env_secret().is_some_and(|s| !s.is_empty()) && !c.stored())
	{
		let secret = calendar.env_secret().unwrap_or_default();
		let path = format!("Credential Manager: {CALENDAR_CREDENTIAL}");
		let store = calendar.clone();
		changes.push(PlannedChange {
			name: "calendar".to_owned(),
			slug: String::new(),
			path: path.clone(),
			change: Some(SetupChange {
				title: format!("the calendar's secret address from {CALENDAR_ENV} into Windows Credential Manager"),
				path,
				diff: vec![format!("+ {CALENDAR_CREDENTIAL} (the address itself isn't shown)")],
				kept: Vec::new(),
				keeps_only: false,
				apply: Box::new(move |_| Box::pin(async move { store.save(&secret) })),
			}),
			marker: calendar_marker(),
		});
	}
	let mut unique = Vec::new();
	for note in notes {
		if !unique.contains(&note) {
			unique.push(note);
		}
	}
	Ok(SetupPlan { changes, settled, notes: unique })
}

/// Applies `changes` in order, each marked once written; the first failure stops the rest and says why. Resolves
/// whether all were applied.
async fn apply_in_order(paths: &SetupPaths, changes: Vec<PlannedChange>, io: &dyn SetupIo) -> Result<bool> {
	let total = changes.len();
	for (index, mut planned) in changes.into_iter().enumerate() {
		let Some(change) = planned.change.take() else { continue };
		let SetupChange { title, path, keeps_only, apply, .. } = change;
		if let Err(error) = apply(io).await {
			let waiting = total - index - 1;
			let after = match waiting {
				0 => String::new(),
				1 => "; the 1 change after it wait".to_owned(),
				n => format!("; the {n} changes after it wait"),
			};
			io.print(&format!("jocasta setup: {} ({title}) failed: {error}{after}.", planned.name));
			return Ok(false);
		}
		mark(paths, &planned)?;
		io.print(&format!("{} {path}", if keeps_only { "kept" } else { "wrote" }));
	}
	Ok(true)
}

/// herdr's config reloaded after a herdr.toml change (0467, 0584), or what to do when it can't be.
async fn reload_herdr_saying(deps: &SetupDeps, io: &dyn SetupIo) -> Result<()> {
	let reload = match &deps.reload_herdr {
		Some(reload) => reload().await?,
		None => RunOutput { code: 1, output: "herdr isn't installed".to_owned() },
	};
	if reload.code == 0 {
		io.print("herdr reloaded its config.");
		return Ok(());
	}
	let first = reload.output.trim().split('\n').next().unwrap_or_default().trim_end_matches('\r');
	let why = if first.is_empty() { format!("exit {}", reload.code) } else { first.to_owned() };
	io.print(&format!(
		"herdr didn't reload its config ({why}): restart herdr to use it (close Jocasta, then run `jocasta`)."
	));
	Ok(())
}

/// The one note when git is missing or older than 2.54 (0613): an older git ignores the pre-push hook Jocasta gives
/// every pane through its environment, in silence, so pushes to main, master and develop would go unasked. `None`
/// when git is new enough.
fn git_version_note(output: Option<&str>) -> Option<String> {
	static VERSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)\.(\d+)").expect("a fixed pattern"));
	let found = output.and_then(|output| VERSION.captures(output));
	let number = |at: usize| found.as_ref().and_then(|f| f[at].parse::<f64>().ok());
	if let (Some(major), Some(minor)) = (number(1), number(2))
		&& (major > 2.0 || (major == 2.0 && minor >= 54.0))
	{
		return None;
	}
	let has = match &found {
		Some(found) => format!("git {} is older than 2.54", &found[0]),
		None => "git isn't installed".to_owned(),
	};
	Some(format!(
		"{has}: Jocasta's check of pushes to main, master and develop needs git 2.54 or newer, so a pane's pushes there go unasked until you update git."
	))
}

/// Shows the plan, asks once, and writes everything only after `y`; then asks for a missing calendar address. herdr
/// reloads a herdr.toml setup wrote; with herdr missing, setup ends by saying so rather than "Done" (0584).
pub async fn run_setup(paths: &SetupPaths, deps: &SetupDeps, io: &dyn SetupIo) -> Result<i32> {
	let SetupPlan { changes, settled, notes } = plan_setup(paths, deps)?;
	for note in &notes {
		io.print(note);
	}
	if let Some(git) = &deps.git_version
		&& let Some(note) = git_version_note(git().await.as_deref())
	{
		io.print(&note);
	}
	let herdr_missing = notes.iter().any(|note| note == HERDR_MISSING);
	if changes.is_empty() {
		for planned in &settled {
			mark(paths, planned)?;
		}
		if !herdr_missing {
			io.print("Nothing to set up: everything Jocasta adds is in place.");
		}
	} else {
		io.print("jocasta setup will make these changes:");
		for planned in &changes {
			if let Some(change) = &planned.change {
				print_change(io, change);
			}
		}
		io.print("");
		if !answered_yes(io.ask("Write these changes? [y/N] ").await.as_deref()) {
			io.print("Nothing written.");
			return Ok(0);
		}
		let wrote_config = changes.iter().any(|p| p.slug == "herdr-config");
		if !apply_in_order(paths, changes, io).await? {
			return Ok(1);
		}
		for planned in &settled {
			mark(paths, planned)?;
		}
		if deps.reload_herdr.is_some() && wrote_config {
			reload_herdr_saying(deps, io).await?;
		}
		io.print(if herdr_missing { HERDR_MISSING } else { "Done: run `jocasta` to open Jocasta." });
	}
	ask_calendar(paths, deps.calendar.as_deref(), io).await?;
	Ok(0)
}

/// The calendar's secret iCal address (0430), asked once per setup when neither Credential Manager nor the env var has
/// it, off the screen as he types (SEC-LOC-12); Enter skips (the calendar stays off).
async fn ask_calendar(paths: &SetupPaths, calendar: Option<&dyn CalendarStore>, io: &dyn SetupIo) -> Result<()> {
	let Some(calendar) = calendar else { return Ok(()) };
	if calendar.env_secret().is_some_and(|s| !s.is_empty()) || calendar.stored() {
		return Ok(());
	}
	let question = "Your calendar's secret iCal address, for today's meetings (Enter to skip): ";
	let answer = io.ask_secret(question).await.map(|answer| answer.trim().to_owned()).unwrap_or_default();
	if answer.is_empty() {
		io.print("No calendar: run `jocasta setup` again to add it.");
		return Ok(());
	}
	static ADDRESS: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"(?i)^(https?|webcal)://\S+$").expect("a fixed pattern"));
	if !ADDRESS.is_match(&answer) {
		io.print("That isn't an http(s) or webcal address; nothing saved. Run `jocasta setup` again to add it.");
		return Ok(());
	}
	calendar.save(&answer)?;
	write_marker(&paths.jocasta_config_dir, "calendar", &Marker { applied_at: now_iso(), ..calendar_marker() })?;
	io.print(&format!("saved in Windows Credential Manager as {CALENDAR_CREDENTIAL}"));
	Ok(())
}

/// `YYYYMMDD-HHMMSS`, local time, for a backup's name (`now` defaults to the current time).
pub fn backup_stamp(now: Option<DateTime<Local>>) -> String {
	now.unwrap_or_else(Local::now).format("%Y%m%d-%H%M%S").to_string()
}

/// `jocasta setup --refresh <file>` (0467): resets one Jocasta-owned file (`herdr.toml`, the Terminal fragment
/// `jocasta.json`, Jocasta's entries in omp's `mcp.json`, by name or full path) to the shipped version after showing the
/// diff and his `y`, keeping the old one as `<name>.bak.<YYYYMMDD-HHMMSS>` beside it; herdr reloads its config after a
/// herdr.toml refresh.
pub async fn refresh_setup(
	paths: &SetupPaths,
	deps: &SetupDeps,
	io: &dyn SetupIo,
	file: &str,
	now: Option<DateTime<Local>>,
) -> Result<i32> {
	let by_path = file.contains(['/', '\\']);
	let name = Path::new(file).file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
	// Only the files it resets are read: another of his files that doesn't parse can't stop it (SET-13).
	let refreshable: Vec<&Migration> =
		MIGRATIONS.iter().filter(|m| m.refresh.is_some_and(|refresh| by_path || refresh == name)).collect();
	let chosen: Vec<PlannedChange> = plan_migrations(paths, deps, &mut Vec::new(), true, &refreshable)?
		.into_iter()
		.filter(|p| !by_path || same_path(Path::new(&p.path), Path::new(file)))
		.collect();
	if chosen.is_empty() {
		return Err(Error::user_with_code(
			format!(
				"setup --refresh: {file} isn't a file setup refreshes; use herdr.toml, jocasta.json (the Windows Terminal scheme) or mcp.json (Jocasta's MCP entries)"
			),
			2,
		));
	}
	for planned in chosen.iter().filter(|p| p.change.is_none()) {
		io.print(&format!("{} already is the shipped version.", planned.path));
	}
	let changes: Vec<PlannedChange> = chosen.into_iter().filter(|p| p.change.is_some()).collect();
	if changes.is_empty() {
		return Ok(0);
	}
	io.print("jocasta setup --refresh will reset these to the shipped version, keeping a backup beside each:");
	for planned in &changes {
		if let Some(change) = &planned.change {
			print_change(io, change);
		}
	}
	io.print("");
	if !answered_yes(io.ask("Refresh? [y/N] ").await.as_deref()) {
		io.print("Nothing written.");
		return Ok(0);
	}
	let stamp = backup_stamp(now);
	for planned in &changes {
		if Path::new(&planned.path).exists() {
			let backup = format!("{}.bak.{stamp}", planned.path);
			std::fs::copy(&planned.path, &backup)?;
			io.print(&format!("kept {backup}"));
		}
	}
	let wrote_config = changes.iter().any(|p| p.slug == "herdr-config");
	if !apply_in_order(paths, changes, io).await? {
		return Ok(1);
	}
	if wrote_config {
		reload_herdr_saying(deps, io).await?;
	}
	Ok(0)
}

/// Setup's start check: its pending changes as one line, or why it couldn't check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupCheck {
	/// The toast's text; `None` when nothing waits.
	pub waiting: Option<String>,
	/// Why the check failed (a file setup can't read).
	pub failed: Option<String>,
}

/// Setup's pending changes on this machine, read only (0466); a failure is returned, never thrown (SET-14).
pub async fn setup_check() -> SetupCheck {
	let read_only: RunProgram =
		Arc::new(|_| Box::pin(async { Ok(RunOutput { code: 1, output: "read only".to_owned() }) }));
	let mut deps = SetupDeps::new(herdr_program(), read_only);
	deps.mise = on_path("mise", None);
	deps.calendar = default_calendar_store(None);
	let paths = default_setup_paths(&shipped_config_dir(None), self_command(None));
	match plan_setup(&paths, &deps) {
		Ok(plan) => match plan.changes.len() {
			0 => SetupCheck::default(),
			n => SetupCheck {
				waiting: Some(format!(
					"jocasta setup has {n} change{} waiting: run `jocasta setup` to see {}.",
					if n > 1 { "s" } else { "" },
					if n > 1 { "them" } else { "it" }
				)),
				failed: None,
			},
		},
		Err(error) => SetupCheck { waiting: None, failed: Some(error.to_string()) },
	}
}

/// The service's start check (0466): setup's pending changes as one toast's text; `None` when none. Never fails:
/// a problem goes to Jocasta's log.
pub async fn setup_toast() -> Option<String> {
	let SetupCheck { waiting, failed } = setup_check().await;
	if let Some(failed) = failed {
		log(LogLevel::Warn, "setup", &format!("checking for setup changes: {failed}"), None);
	}
	waiting
}

/// `herdr config check` on `path`, with the output of both streams.
async fn default_herdr_check(herdr: &Path, path: &Path) -> Result<HerdrCheck> {
	let mut vars: std::collections::HashMap<String, String> = env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect();
	vars.insert("HERDR_CONFIG_PATH".to_owned(), path.display().to_string());
	let argv = [herdr.as_os_str(), "config".as_ref(), "check".as_ref()];
	let result =
		run_program(&argv, RunOptions { env: Some(vars), timeout_ms: Some(30_000), ..RunOptions::default() }).await?;
	Ok(HerdrCheck { code: result.code, output: format!("{}{}", result.stdout, result.stderr) })
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
