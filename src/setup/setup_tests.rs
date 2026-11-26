//! `setup.ts`'s tests (`test/setup/setup.test.ts`) and `test/config/herdr-config.test.ts`.
//!
//! The TypeScript's `Bun.TOML.parse` has no twin here (no TOML crate): a written TOML file is checked by its lines, and
//! the HUD's command by reading its basic string back.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::TimeZone;
use futures_util::future::BoxFuture;
use regex::Regex;
use serde_json::{Value, json};

use super::*;
use crate::setup::markers::{fingerprint, markers_dir};
use crate::setup::mise::MISE_FRAGMENT;
use crate::setup::omp_settings::omp_settings_text;
use crate::shared::fsx::remove_tree;
use crate::shared::install::REPO;
use crate::shared::testing::{TempDir, temp_dir, temp_jocasta};
use crate::shared::yaml::parse_value;

fn assets() -> PathBuf {
	Path::new(REPO).join("config")
}

/// A terminal that answers from a list and records what it was told and asked.
struct TestIo {
	out: Mutex<Vec<String>>,
	asked: Mutex<Vec<String>>,
	secrets: Mutex<Vec<String>>,
	answers: Mutex<VecDeque<Option<String>>>,
	/// Runs when a question is asked, before the answer (a file changing while the diff waits).
	on_ask: Option<Box<dyn Fn() + Send + Sync>>,
	/// What `ask_secret` answers; without it the secret question goes through `ask`.
	secret_answer: Option<String>,
}

impl TestIo {
	fn new(answers: &[Option<&str>]) -> Self {
		Self {
			out: Mutex::new(Vec::new()),
			asked: Mutex::new(Vec::new()),
			secrets: Mutex::new(Vec::new()),
			answers: Mutex::new(answers.iter().map(|a| a.map(str::to_owned)).collect()),
			on_ask: None,
			secret_answer: None,
		}
	}

	fn text(&self) -> String {
		self.out.lock().unwrap().join("\n")
	}

	fn asked(&self) -> Vec<String> {
		self.asked.lock().unwrap().clone()
	}
}

impl SetupIo for TestIo {
	fn print(&self, line: &str) {
		self.out.lock().unwrap().push(line.to_owned());
	}

	fn ask<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		self.asked.lock().unwrap().push(question.to_owned());
		if let Some(hook) = &self.on_ask {
			hook();
		}
		let answer = self.answers.lock().unwrap().pop_front().flatten();
		Box::pin(async move { answer })
	}

	fn ask_secret<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		match &self.secret_answer {
			Some(answer) => {
				self.secrets.lock().unwrap().push(question.to_owned());
				let answer = answer.clone();
				Box::pin(async move { Some(answer) })
			}
			None => self.ask(question),
		}
	}
}

/// What one setup run printed.
struct Out {
	code: i32,
	text: String,
	asked: Vec<String>,
}

struct Harness {
	root: TempDir,
	paths: SetupPaths,
	mise: Option<PathBuf>,
	git_version: Option<Arc<dyn Fn() -> BoxFuture<'static, Option<String>> + Send + Sync>>,
}

impl Harness {
	fn new() -> Self {
		let root = temp_dir("setup");
		let base = root.path().to_path_buf();
		let paths = SetupPaths {
			omp_agent_dir: base.join("omp").join("agent"),
			jocasta_config_dir: base.join("jocasta").join("agent"),
			data_dir: base.join("data"),
			assets_dir: assets(),
			terminal_fragments_dir: Some(base.join("Fragments")),
			system_folders: None,
			sleep_prevention_off: false,
			mise_config_dir: base.join("mise"),
			jocasta_command: vec![PathBuf::from("C:\\Jocasta\\jocasta.exe")],
		};
		Self { root, paths, mise: None, git_version: None }
	}

	fn deps(&self) -> SetupDeps {
		let mut deps = SetupDeps::new();
		deps.mise = self.mise.clone();
		deps.git_version = self.git_version.clone();
		deps
	}

	async fn setup_with(&self, answer: Option<&str>) -> Result<Out> {
		let io = TestIo::new(&[answer]);
		let code = run_setup(&self.paths, &self.deps(), &io).await?;
		Ok(Out { code, text: io.text(), asked: io.asked() })
	}

	async fn setup(&self, answer: Option<&str>) -> Out {
		self.setup_with(answer).await.unwrap()
	}

	async fn refresh(&self, file: &str, answer: Option<&str>) -> Result<Out> {
		let io = TestIo::new(&[answer]);
		let now = chrono::Local.with_ymd_and_hms(2026, 10, 23, 9, 5, 7).unwrap();
		let code = refresh_setup(&self.paths, &self.deps(), &io, file, Some(now)).await?;
		Ok(Out { code, text: io.text(), asked: io.asked() })
	}

	/// A copy of the shipped config/ the test may change.
	fn own_assets(&mut self) -> PathBuf {
		let dir = self.root.path().join("assets");
		copy_dir(&assets(), &dir);
		self.paths.assets_dir = dir.clone();
		dir
	}

	fn global_mcp(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("mcp.json")
	}

	fn boss_mcp(&self) -> PathBuf {
		self.paths.data_dir.join("home").join(".omp").join("mcp.json")
	}

	fn omp_config(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("config.yml")
	}

	fn fragment(&self) -> PathBuf {
		self.root.path().join("Fragments").join("jocasta").join("jocasta.json")
	}

	fn mise_skill(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("skills").join("mise").join("SKILL.md")
	}

	fn mise_file(&self) -> PathBuf {
		self.paths.mise_config_dir.join("conf.d").join("jocasta.toml")
	}

	fn keybindings(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("keybindings.yml")
	}

	fn watchdog(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("WATCHDOG.md")
	}

	fn lsp(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("lsp.json")
	}

	fn secrets(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("secrets.yml")
	}

	/// herdr's omp hook as it was left in omp's `extensions/` folder (migration 22 takes it out).
	fn hook(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("extensions").join("herdr-omp-agent-state.ts")
	}

	/// The snapshot's backups of this run's replaced files: the one folder under `omp-backup/`.
	fn backups(&self) -> Option<PathBuf> {
		let root = markers_dir(&self.paths.jocasta_config_dir).join("omp-backup");
		let mut stamps: Vec<PathBuf> = fs::read_dir(root).ok()?.map(|entry| entry.unwrap().path()).collect();
		stamps.sort();
		stamps.pop()
	}

	/// Every file of the snapshot, as omp's folder names it.
	fn snapshot_paths(&self) -> Vec<PathBuf> {
		vec![self.omp_config(), self.keybindings(), self.lsp(), self.watchdog(), self.mise_skill(), self.secrets()]
	}

	fn plugin_dir(&self) -> PathBuf {
		self.paths.jocasta_config_dir.join("herdr-plugin")
	}

	fn marker(&self, name: &str) -> Option<Marker> {
		read_marker(&self.paths.jocasta_config_dir, name)
	}

	fn undo(&self, calendar_stored: bool) -> Vec<SetupUndo> {
		setup_undo(&self.paths, |target| calendar_stored && target == CALENDAR_CREDENTIAL)
	}
}

fn copy_dir(from: &Path, to: &Path) {
	fs::create_dir_all(to).unwrap();
	for entry in fs::read_dir(from).unwrap() {
		let entry = entry.unwrap();
		let target = to.join(entry.file_name());
		if entry.file_type().unwrap().is_dir() {
			copy_dir(&entry.path(), &target);
		} else {
			fs::copy(entry.path(), target).unwrap();
		}
	}
}

fn put(path: &Path, text: &str) {
	fs::create_dir_all(path.parent().unwrap()).unwrap();
	fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
	fs::read_to_string(path).unwrap()
}

fn read_json(path: &Path) -> Value {
	serde_json::from_str(&read(path)).unwrap()
}

fn asset_text(relative: &str) -> String {
	read(&assets().join(relative))
}

fn pretty(value: &Value) -> String {
	format!("{}\n", serde_json::to_string_pretty(value).unwrap())
}

fn has_line(text: &str, pattern: &str) -> bool {
	Regex::new(&format!("(?m){pattern}")).unwrap().is_match(text)
}

fn yaml(text: &str) -> Value {
	parse_value(text).unwrap()
}

fn s(path: &Path) -> String {
	path.display().to_string()
}

fn strings(argv: &[&str]) -> Vec<String> {
	argv.iter().map(|part| (*part).to_owned()).collect()
}

const IN_PLACE: &str = "Nothing to set up: everything Jocasta adds is in place.";

fn other(undo: Undo) -> SetupUndo {
	SetupUndo::Other(undo)
}

#[tokio::test]
async fn a_fresh_machine_the_diff_names_every_file_and_n_writes_nothing() {
	let h = Harness::new();
	let Out { code, text, asked } = h.setup(Some("n")).await;
	assert_eq!(code, 0);
	assert_eq!(asked, ["Write these changes? [y/N] "]);
	assert!(text.contains(&format!("Windows Terminal scheme Tokyo Night: {}", s(&h.fragment()))));
	assert!(text.contains(&format!(
		"Jocasta's omp snapshot over your omp files, each replaced one kept first in {}",
		s(&markers_dir(&h.paths.jocasta_config_dir).join("omp-backup"))
	)));
	for path in h.snapshot_paths() {
		assert!(text.contains(&format!("\n  {} (new)\n", s(&path))), "{}", s(&path));
	}
	assert!(text.contains("\n  + app.stt.toggle: f9\n"));
	// No mise on this machine: no mise file (0476).
	assert!(!text.contains(&s(&h.mise_file())));
	assert!(text.ends_with("Nothing written."));
	assert!(!h.root.path().join("omp").exists());
	assert!(!h.paths.data_dir.exists());
	assert!(!markers_dir(&h.paths.jocasta_config_dir).exists());
	assert!(!h.fragment().exists());
	// End of input is a no too.
	assert!(h.setup(None).await.text.ends_with("Nothing written."));
}

#[tokio::test]
async fn y_writes_every_file_marks_each_migration_and_a_second_run_has_nothing_to_do() {
	let h = Harness::new();
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 0);
	assert!(text.contains(&format!("wrote {}", s(&h.paths.omp_agent_dir))));
	// No `mcp.json` anywhere any more (0675, 0679).
	assert!(!h.global_mcp().exists());
	assert!(!h.boss_mcp().exists());
	// The snapshot as shipped: `config/omp/` file for file.
	assert_eq!(read(&h.omp_config()), asset_text("omp/config.yml"));
	assert_eq!(read(&h.keybindings()), asset_text("omp/keybindings.yml"));
	assert_eq!(read(&h.lsp()), asset_text("omp/lsp.json"));
	assert_eq!(read(&h.watchdog()), asset_text("omp/WATCHDOG.md"));
	assert_eq!(read(&h.mise_skill()), asset_text("omp/skills/mise/SKILL.md"));
	assert_eq!(read(&h.secrets()), asset_text("omp/secrets.yml"));
	// Nothing was replaced: no backups.
	assert_eq!(h.backups(), None);
	assert_eq!(read_json(&h.fragment()), json!({"schemes": [&*TOKYO_NIGHT_SCHEME]}));
	let mut markers: Vec<String> = fs::read_dir(markers_dir(&h.paths.jocasta_config_dir))
		.unwrap()
		.map(|entry| entry.unwrap().file_name().into_string().unwrap())
		.collect();
	markers.sort();
	assert_eq!(markers, ["0007-terminal-scheme.json", "0018-herdr-plugin-gone.json", "0019-omp-snapshot.json"]);
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

#[tokio::test]
async fn with_mise_installed_setup_writes_jocastas_mise_file_exact_pins_and_the_private_paranoid_settings() {
	let mut h = Harness::new();
	h.mise = Some(PathBuf::from("mise.exe"));
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains(&format!("mise's pins and settings for Jocasta's tools: {}", s(&h.mise_file()))));
	assert_eq!(read(&h.mise_file()), MISE_FRAGMENT);
	for line in [
		"min_version = \"2026.10.12\"",
		"use_versions_host = false",
		"use_versions_host_track = false",
		"auto_install = false",
		"paranoid = [\"config\"]",
		"gh = \"2.92.0\"",
	] {
		assert!(MISE_FRAGMENT.lines().any(|l| l == line), "{line}");
	}
	// His own config.toml in the same folder is never touched.
	assert!(!h.paths.mise_config_dir.join("config.toml").exists());
}

#[tokio::test]
async fn migration_20_takes_only_the_jocasta_entry_out_of_each_mcp_json_and_no_entry_means_no_step() {
	// No `jocasta` entry anywhere: no step, no marker.
	let h = Harness::new();
	h.setup(Some("y")).await;
	assert!(h.marker("0020-jocasta-mcp-gone").is_none());
	// An earlier build's entries in both files: only `jocasta` goes, his other entries and keys stay.
	let jocasta = json!({"type": "stdio", "command": "old\\jocasta.exe", "args": ["mcp"]});
	let boss = json!({"mcpServers": {
		"jocasta": jocasta.clone(),
		"other": {"type": "stdio", "command": "other-mcp", "args": []},
	}});
	put(&h.boss_mcp(), &pretty(&boss));
	put(&h.global_mcp(), &pretty(&json!({"mcpServers": {"jocasta": jocasta}, "disabledServers": ["x"]})));
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("The jocasta MCP server is gone"), "{text}");
	assert!(text.contains(&format!("mcpServers.jocasta in {}", s(&h.boss_mcp()))), "{text}");
	assert!(text.contains(&format!("mcpServers.jocasta in {}", s(&h.global_mcp()))), "{text}");
	assert_eq!(
		read_json(&h.boss_mcp()),
		json!({"mcpServers": {
			"other": {"type": "stdio", "command": "other-mcp", "args": []},
		}})
	);
	assert_eq!(read_json(&h.global_mcp()), json!({"mcpServers": {}, "disabledServers": ["x"]}));
	assert!(h.marker("0020-jocasta-mcp-gone").is_some());
	// Done: a second run has nothing to do.
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

#[tokio::test]
async fn migration_21_takes_the_installed_whiteboard_and_hunk_and_the_boss_whiteboard_server_out_only_from_an_install()
{
	// No install marker and no entry: no step, no marker.
	let mut h = Harness::new();
	let dir = h.own_assets();
	h.setup(Some("y")).await;
	assert!(h.marker("0021-whiteboard-hunk-gone").is_none());
	// A source run's folder (no marker) is never cleaned, but the boss's entry still goes.
	let install = dir.parent().unwrap().to_path_buf();
	for name in ["whiteboard", "hunk", "node"] {
		put(&install.join(name).join("x.txt"), "x");
	}
	put(&h.boss_mcp(), &pretty(&json!({"mcpServers": {"whiteboard": {"command": "node"}, "other": {"command": "o"}}})));
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("mcpServers.whiteboard in "), "{text}");
	assert!(install.join("whiteboard").exists());
	assert_eq!(read_json(&h.boss_mcp()), json!({"mcpServers": {"other": {"command": "o"}}}));
	assert!(h.marker("0021-whiteboard-hunk-gone").is_some());
	// An install: the folders go, his other files stay.
	let mut h = Harness::new();
	let dir = h.own_assets();
	let install = dir.parent().unwrap().to_path_buf();
	put(&install.join("install.json"), &json!({"dir": install.display().to_string()}).to_string());
	for name in ["whiteboard", "whiteboard-canvas", "hunk", "node"] {
		put(&install.join(name).join("x.txt"), "x");
	}
	put(&install.join("keep").join("y.txt"), "y");
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("Whiteboard and hunk are gone"), "{text}");
	for name in ["whiteboard", "whiteboard-canvas", "hunk", "node"] {
		assert!(!install.join(name).exists(), "{name}");
	}
	assert!(install.join("keep").join("y.txt").exists());
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

/// Files of omp's that the snapshot doesn't have: his logins, sessions, memory and history.
fn his_omp_state(h: &Harness) -> Vec<(PathBuf, &'static str)> {
	let dir = &h.paths.omp_agent_dir;
	vec![
		(dir.join("agent.db"), "his logins\n"),
		(dir.join("sessions").join("--d--code--").join("one.jsonl"), "{\"type\":\"session\"}\n"),
		(dir.join("memories").join("notes.md"), "what he remembers\n"),
		(dir.join("history.db"), "his prompts\n"),
		(dir.join("extensions").join("his.ts"), "// his own extension\n"),
	]
}

#[tokio::test]
async fn the_snapshot_replaces_his_omp_files_after_y_each_kept_first_and_leaves_every_other_file_byte_for_byte() {
	let h = Harness::new();
	let state = his_omp_state(&h);
	for (path, text) in &state {
		put(path, text);
	}
	put(&h.omp_config(), "theme:\n  dark: his-theme\n");
	put(&h.keybindings(), "app.model.cycle: [ctrl+k]\n");
	// Already the snapshot's: no diff, no backup.
	put(&h.lsp(), &asset_text("omp/lsp.json"));

	let no = h.setup(Some("n")).await;
	assert!(no.text.contains(&format!("\n  {}\n", s(&h.omp_config()))));
	assert!(has_line(&no.text, r"^  -   dark: his-theme$"));
	assert!(has_line(&no.text, r"^  - app\.model\.cycle: \[ctrl\+k\]$"));
	assert!(!no.text.contains(&format!("\n  {}", s(&h.lsp()))));
	assert_eq!(read(&h.omp_config()), "theme:\n  dark: his-theme\n");
	assert_eq!(h.backups(), None);

	h.setup(Some("y")).await;
	assert_eq!(read(&h.omp_config()), asset_text("omp/config.yml"));
	assert_eq!(read(&h.keybindings()), asset_text("omp/keybindings.yml"));
	let backups = h.backups().expect("the replaced files kept");
	assert_eq!(read(&backups.join("config.yml")), "theme:\n  dark: his-theme\n");
	assert_eq!(read(&backups.join("keybindings.yml")), "app.model.cycle: [ctrl+k]\n");
	assert!(!backups.join("lsp.json").exists());
	assert!(!backups.join("WATCHDOG.md").exists());
	for (path, text) in &state {
		assert_eq!(read(path), *text, "{}", s(path));
	}
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

#[tokio::test]
async fn his_edit_after_the_snapshot_stays_until_a_newer_snapshot_which_keeps_his_first_file_for_uninstall() {
	let mut h = Harness::new();
	let dir = h.own_assets();
	put(&h.omp_config(), "theme:\n  dark: his-theme\n");
	h.setup(Some("y")).await;
	let first = h.backups().unwrap();
	// omp saves a setting he changed in `/settings`: the same snapshot never comes back over it.
	let edited = format!("{}# his line\n", read(&h.omp_config()));
	put(&h.omp_config(), &edited);
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
	assert_eq!(read(&h.omp_config()), edited);

	// A newer snapshot shows his edit going and keeps it, but uninstall still restores the file from before Jocasta.
	put(
		&dir.join("omp").join("config.yml"),
		&format!("{}# shipped later\n", read(&dir.join("omp").join("config.yml"))),
	);
	let newer = h.setup(Some("y")).await;
	assert!(has_line(&newer.text, r"^  - # his line$"));
	assert!(read(&h.omp_config()).ends_with("# shipped later\n"));
	let second = h.backups().unwrap();
	assert_ne!(first, second);
	assert_eq!(read(&second.join("config.yml")), edited);
	let restore = h
		.marker("0019-omp-snapshot")
		.unwrap()
		.undo
		.into_iter()
		.find(|undo| matches!(undo, Undo::Restore { path, .. } if *path == s(&h.omp_config())))
		.unwrap();
	assert_eq!(
		restore,
		Undo::Restore {
			path: s(&h.omp_config()),
			backup: Some(s(&first.join("config.yml"))),
			fingerprint: fingerprint(&read(&h.omp_config())),
		}
	);
}

#[tokio::test]
async fn secrets_yml_is_written_only_when_missing_his_own_is_never_shown_backed_up_or_undone() {
	// omp warns at every start about a file that isn't a YAML list (`secrets/index.ts:299-301`).
	assert_eq!(yaml(&asset_text("omp/secrets.yml")), json!([]));
	let mut h = Harness::new();
	let dir = h.own_assets();
	let mine = "- type: plain\n  content: his-name-q7z\n";
	put(&h.secrets(), mine);
	let his = h.setup(Some("y")).await;
	assert!(!his.text.contains("his-name-q7z"));
	assert!(!his.text.contains(&s(&h.secrets())));
	assert_eq!(read(&h.secrets()), mine);
	assert_eq!(h.backups(), None);
	let undo_text = |h: &Harness| h.undo(false).iter().map(|u| u.to_json().to_string()).collect::<String>();
	assert!(!undo_text(&h).contains(&serde_json::to_string(&s(&h.secrets())).unwrap()));
	// A newer shipped text leaves his alone, unshown.
	put(&dir.join("omp").join("secrets.yml"), "# v2\n[]\n");
	let newer = h.setup(Some("y")).await;
	assert!(!newer.text.contains("his-name-q7z"));
	assert_eq!(read(&h.secrets()), mine);
}

#[tokio::test]
async fn the_terminal_fragment_follows_the_shipped_scheme_after_y() {
	let h = Harness::new();
	put(&h.fragment(), "his own\n");
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains(&format!("Windows Terminal scheme Tokyo Night: {}", s(&h.fragment()))));
	assert!(text.contains("- his own"));
	assert_eq!(read_json(&h.fragment()), json!({"schemes": [&*TOKYO_NIGHT_SCHEME]}));
}

#[test]
fn the_terminal_scheme_comes_from_the_one_palette_file_byte_for_byte_what_setup_wrote_before() {
	assert_eq!(
		*TOKYO_NIGHT_SCHEME,
		json!({
			"name": "Tokyo Night",
			"background": "#1A1B26",
			"foreground": "#C0CAF5",
			"cursorColor": "#C0CAF5",
			"selectionBackground": "#2D3650",
			"black": "#24283B",
			"red": "#F7768E",
			"green": "#9ECE6A",
			"yellow": "#E0AF68",
			"blue": "#7AA2F7",
			"purple": "#BB9AF7",
			"cyan": "#7DCFFF",
			"white": "#A9B1D6",
			"brightBlack": "#414868",
			"brightRed": "#F7768E",
			"brightGreen": "#9ECE6A",
			"brightYellow": "#E0AF68",
			"brightBlue": "#7AA2F7",
			"brightPurple": "#BB9AF7",
			"brightCyan": "#7DCFFF",
			"brightWhite": "#C0CAF5",
		})
	);
	// Keys in the order setup wrote them.
	let keys: Vec<&String> = TOKYO_NIGHT_SCHEME.as_object().unwrap().keys().collect();
	assert_eq!(keys[..5], ["name", "background", "foreground", "cursorColor", "selectionBackground"]);
}

#[tokio::test]
async fn a_failed_migration_stops_the_rest_and_says_why_the_ones_before_it_are_marked() {
	let h = Harness::new();
	let mut io = TestIo::new(&[Some("y")]);
	// The scheme's file appears while the diff that named it new waits for his answer.
	let fragment = h.fragment();
	io.on_ask = Some(Box::new(move || put(&fragment, "his own\n")));
	let code = run_setup(&h.paths, &h.deps(), &io).await.unwrap();
	assert_eq!(code, 1);
	let text = io.text();
	assert!(
		text.contains(&format!(
			"jocasta setup: 0007-terminal-scheme (Windows Terminal scheme Tokyo Night) failed: {} changed since the diff; run jocasta setup again; the 1 change after it wait.",
			s(&h.fragment())
		)),
		"{text}"
	);
	assert_eq!(read(&h.fragment()), "his own\n");
	assert!(h.marker("0007-terminal-scheme").is_none());
	assert!(h.marker("0019-omp-snapshot").is_none());
}

#[tokio::test]
async fn refresh_refuses_a_file_setup_doesnt_refresh() {
	let h = Harness::new();
	h.setup(Some("y")).await;
	let refused = h.refresh("config.yml", Some("y")).await.err().unwrap();
	assert!(refused.is_user());
	assert!(refused.to_string().contains("isn't a file setup refreshes"));
}

#[tokio::test]
async fn refresh_jocasta_json_puts_back_the_terminal_scheme_he_changed_after_setup_keeping_his_copy() {
	let h = Harness::new();
	h.setup(Some("y")).await;
	let shipped = read(&h.fragment());
	put(&h.fragment(), "his own\n");
	let yes = h.refresh("JOCASTA.JSON", Some("y")).await.unwrap();
	let backup = format!("{}.bak.20261023-090507", s(&h.fragment()));
	assert_eq!(yes.code, 0);
	assert!(yes.text.contains(&format!("Windows Terminal scheme Tokyo Night: {}", s(&h.fragment()))));
	assert!(yes.text.contains(&format!("kept {backup}")));
	assert_eq!(read(Path::new(&backup)), "his own\n");
	assert_eq!(read(&h.fragment()), shipped);
	assert_eq!(
		h.refresh(&s(&h.fragment()), Some("y")).await.unwrap().text,
		format!("{} already is the shipped version.", s(&h.fragment()))
	);
}

#[tokio::test]
async fn setup_and_refresh_write_on_y_or_yes_in_any_case_and_spacing_any_other_answer_writes_nothing() {
	let h = Harness::new();
	for answer in [Some("yep"), Some("ja"), Some(""), None] {
		let Out { code, text, .. } = h.setup(answer).await;
		assert_eq!(code, 0);
		assert!(text.ends_with("Nothing written."));
		assert!(!h.fragment().exists());
	}
	h.setup(Some(" YES ")).await;
	assert!(h.fragment().exists());
	put(&h.fragment(), "his own\n");
	assert!(
		h.refresh("jocasta.json", Some("Y"))
			.await
			.unwrap()
			.text
			.contains(&format!("kept {}.bak.20261023-090507", s(&h.fragment())))
	);
}

#[test]
fn backup_stamp_is_local_yyyymmdd_hhmmss() {
	let at = chrono::Local.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
	assert_eq!(backup_stamp(Some(at)), "20260102-030405");
}

#[tokio::test]
async fn setup_undo_names_what_setup_added_from_its_markers_only_a_step_without_its_marker_isnt_undone() {
	let mut h = Harness::new();
	h.mise = Some(PathBuf::from("mise.exe"));
	h.setup(Some("y")).await;
	let undo = h.undo(false);
	assert!(undo.contains(&other(Undo::Dir {
		path: s(h.fragment().parent().unwrap()),
		file: Some("jocasta.json".to_owned())
	})));
	assert!(undo.contains(&other(Undo::File { path: s(&h.mise_file()) })));
	// Every file the snapshot made goes while it is still the snapshot's text; none had one before it.
	for path in h.snapshot_paths() {
		let restore = Undo::Restore { path: s(&path), backup: None, fingerprint: fingerprint(&read(&path)) };
		assert!(undo.contains(&other(restore)), "{}", s(&path));
	}
	let all: Vec<String> = undo.iter().map(|u| u.to_json().to_string()).collect();
	let unique: std::collections::HashSet<&String> = all.iter().collect();
	assert_eq!(unique.len(), undo.len());

	// A marker of a migration this build no longer has is ignored.
	let old = Undo::Mcp { path: s(&h.global_mcp()), servers: strings(&["jocasta"]) };
	write_marker(
		&h.paths.jocasta_config_dir,
		"0001-mcp",
		&Marker { fingerprint: "f".to_owned(), undo: vec![old], applied_at: "2026-11-01T00:00:00.000Z".to_owned() },
	)
	.unwrap();
	assert_eq!(h.undo(false), undo);

	// Without the markers nothing is setup's to undo, whatever is there, and a stored calendar secret only with its
	// marker (B1).
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	assert!(h.undo(true).is_empty());
}

#[test]
fn the_shipped_config_yml_carries_the_decided_values_that_his_own_file_held_where_omp_defaults_differ() {
	let config = yaml(&asset_text("omp/config.yml"));
	// omp's default is off: without it the snapshot would turn his advisor off in every pane (0619, 0620); it reviews at
	// the agent's end and wakes it only for a blocker (0695).
	assert_eq!(config["advisor"], json!({"enabled": true, "reviewMode": "agent-end", "continueSeverity": "blocker"}));
	// His fallbacks (0590), dictation and speech kept on the machine (0556).
	let chains = &config["retry"]["fallbackChains"];
	assert_eq!(chains["plan"], json!(["anthropic/claude-opus-5-5:max", "anthropic/claude-opus-5:max"]));
	assert_eq!(chains["slow"], chains["plan"]);
	assert_eq!(chains["dictation"], json!([]));
	assert_eq!(chains["speech"], json!([]));
	assert_eq!(config["modelRoles"]["default"], "anthropic/claude-opus-5-5:medium");
	// Commit messages on the local model first, Haiku without it (0696).
	assert_eq!(config["modelRoles"]["commit"], "ollama/qwen3:4b, anthropic/claude-haiku-5-5:low");
}

#[tokio::test]
async fn on_windows_the_shipped_fence_also_names_windows_own_folders_and_a_second_run_has_nothing_to_do() {
	let mut h = Harness::new();
	let shipped = asset_text("omp/config.yml");
	let fence = |text: &str| -> Vec<Value> { yaml(text)["tools"]["restrictedFolders"].as_array().unwrap().clone() };
	assert!(!fence(&shipped).is_empty());
	assert_eq!(omp_settings_text(&shipped, None).unwrap(), shipped);
	assert_eq!(
		omp_settings_text("tools:\n  restrictedFolders: []\n", Some(&["C:\\W".to_owned()])).unwrap(),
		"tools:\n  restrictedFolders: [\"C:\\\\W\"]\n"
	);
	assert!(
		omp_settings_text("tools: {}\n", Some(&["C:\\W".to_owned()]))
			.unwrap_err()
			.to_string()
			.contains("no one-line tools.restrictedFolders list")
	);

	h.paths.system_folders = Some(vec!["C:\\WINDOWS".to_owned(), "D:\\Program Files".to_owned()]);
	h.setup(Some("y")).await;
	let mut expected = fence(&shipped);
	expected.extend([json!("C:\\WINDOWS"), json!("D:\\Program Files")]);
	assert_eq!(fence(&read(&h.omp_config())), expected);
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
	assert!(yaml(&read(&h.omp_config())).get("power").is_none());
}

#[tokio::test]
async fn on_linux_setup_writes_power_sleep_prevention_off_and_a_second_run_has_nothing_to_do_0696() {
	let mut h = Harness::new();
	h.paths.sleep_prevention_off = true;
	let first = h.setup(Some("y")).await;
	assert!(first.text.contains("sleepPrevention"), "{}", first.text);
	assert_eq!(yaml(&read(&h.omp_config()))["power"], json!({ "sleepPrevention": "off" }));
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

#[tokio::test]
async fn setup_toast_one_line_when_setup_has_changes_waiting_none_when_everything_is_in_place() {
	let mut jocasta = temp_jocasta();
	let root = jocasta.root().to_path_buf();
	jocasta.set_var("MISE_CONFIG_DIR", Some(&s(&root.join("mise"))));
	jocasta.set_var("LOCALAPPDATA", Some(&s(&root.join("local"))));
	let toast = setup_toast().await.expect("changes are waiting");
	assert!(
		Regex::new(r"^jocasta setup has \d+ changes waiting: run `jocasta setup` to see them\.$")
			.unwrap()
			.is_match(&toast)
	);
	assert!(!omp_agent_dir(None).exists());
}

#[tokio::test]
async fn the_old_herdr_plugins_folder_and_migration_14s_marker_go_after_y_and_uninstall_never_looks_for_the_plugin() {
	let h = Harness::new();
	let marker = markers_dir(&h.paths.jocasta_config_dir).join("0014-herdr-plugin.json");
	put(&h.plugin_dir().join("herdr-plugin.toml"), "id = \"jocasta\"\n");
	let undo = json!({"kind": "herdr-plugin", "id": "jocasta", "path": s(&h.plugin_dir())});
	put(&marker, &json!({"fingerprint": "f", "undo": [undo], "appliedAt": "2026-11-10T00:00:00Z"}).to_string());
	let no = h.setup(Some("n")).await;
	assert!(no.text.contains("the `jocasta` herdr plugin is gone"), "{}", no.text);
	assert!(no.text.contains(&format!("- {}", s(&h.plugin_dir()))));
	assert!(no.text.contains(&format!("- {}", s(&marker))));
	assert!(h.plugin_dir().exists() && marker.exists());
	h.setup(Some("y")).await;
	assert!(!h.plugin_dir().exists() && !marker.exists());
	assert!(h.marker("0018-herdr-plugin-gone").is_some());
	assert!(h.undo(false).iter().all(|item| !item.to_json().to_string().contains("herdr-plugin")));
}

#[tokio::test]
async fn migration_22_takes_herdrs_omp_hook_and_config_out_after_y_and_with_neither_there_is_no_step() {
	let h = Harness::new();
	// Neither file: no step, no marker.
	h.setup(Some("y")).await;
	assert!(h.marker("0022-herdr-gone").is_none());
	let config = h.paths.jocasta_config_dir.join("herdr.toml");
	put(&h.hook(), "// herdr's omp hook\n");
	put(&config, "# herdr's config\n");
	let no = h.setup(Some("n")).await;
	assert!(no.text.contains("herdr is gone: its omp hook and its config leave"), "{}", no.text);
	assert!(no.text.contains(&format!("- {}", s(&h.hook()))));
	assert!(no.text.contains(&format!("- {}", s(&config))));
	assert!(h.hook().exists() && config.exists());
	h.setup(Some("y")).await;
	assert!(!h.hook().exists() && !config.exists());
	assert!(h.marker("0022-herdr-gone").is_some());
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
}

#[tokio::test]
async fn a_snapshot_file_changed_after_the_diff_is_refused_and_no_file_of_the_snapshot_is_written() {
	let h = Harness::new();
	put(&h.omp_config(), "modelRoles:\n  default: anthropic/model-a\n");
	let mut io = TestIo::new(&[Some("y")]);
	// omp saves a setting while the diff waits for his answer.
	let config = h.omp_config();
	io.on_ask = Some(Box::new(move || put(&config, "modelRoles:\n  default: anthropic/model-b\n")));
	let code = run_setup(&h.paths, &h.deps(), &io).await.unwrap();
	assert_eq!(code, 1);
	let failed =
		io.out.lock().unwrap().iter().find(|l| l.starts_with("jocasta setup: 0019-omp-snapshot")).cloned().unwrap();
	assert!(failed.starts_with("jocasta setup: 0019-omp-snapshot (Jocasta's omp snapshot over your omp files"));
	assert!(
		failed.ends_with(&format!("failed: {} changed since the diff; run jocasta setup again.", s(&h.omp_config())))
	);
	assert_eq!(read(&h.omp_config()), "modelRoles:\n  default: anthropic/model-b\n");
	assert!(!h.keybindings().exists() && !h.global_mcp().exists());
	assert_eq!(h.backups(), None);
	assert!(h.marker("0019-omp-snapshot").is_none());
}

/// A Credential Manager stand-in for the calendar's secret (0430).
struct Calendar {
	saved: Mutex<Vec<String>>,
	from_env: Option<String>,
}

impl Calendar {
	fn new(from_env: Option<&str>) -> Arc<Self> {
		Arc::new(Self { saved: Mutex::new(Vec::new()), from_env: from_env.map(str::to_owned) })
	}

	fn saved(&self) -> Vec<String> {
		self.saved.lock().unwrap().clone()
	}
}

impl CalendarStore for Calendar {
	fn stored(&self) -> bool {
		!self.saved.lock().unwrap().is_empty()
	}

	fn save(&self, secret: &str) -> Result<()> {
		self.saved.lock().unwrap().push(secret.to_owned());
		Ok(())
	}

	fn env_secret(&self) -> Option<String> {
		self.from_env.clone()
	}
}

async fn setup_with_calendar(h: &Harness, calendar: &Arc<Calendar>, answers: &[Option<&str>]) -> (String, Vec<String>) {
	let io = TestIo::new(answers);
	let mut deps = SetupDeps::new();
	deps.calendar = Some(calendar.clone());
	run_setup(&h.paths, &deps, &io).await.unwrap();
	(io.text(), io.asked())
}

#[tokio::test]
async fn the_calendars_env_secret_moves_into_credential_manager_with_the_other_changes_never_shown() {
	let h = Harness::new();
	let calendar = Calendar::new(Some("https://calendar.example/secret-token.ics"));
	let (text, asked) = setup_with_calendar(&h, &calendar, &[Some("y")]).await;
	assert!(text.contains("jocasta/calendar-ical"));
	assert!(!text.contains("secret-token"));
	assert_eq!(calendar.saved(), ["https://calendar.example/secret-token.ics"]);
	assert_eq!(asked.len(), 1);
	assert_eq!(h.marker("calendar").unwrap().undo, [Undo::Credential { target: "jocasta/calendar-ical".to_owned() }]);
	// Stored now: a second run neither proposes nor asks.
	let (again, _) = setup_with_calendar(&h, &calendar, &[Some("y")]).await;
	assert!(!again.contains("jocasta/calendar-ical"));
	assert_eq!(calendar.saved().len(), 1);
}

#[tokio::test]
async fn the_calendars_secret_address_is_asked_for_as_a_secret_off_the_screen_where_the_console_can() {
	let h = Harness::new();
	let calendar = Calendar::new(None);
	let mut io = TestIo::new(&[Some("y")]);
	io.secret_answer = Some("https://calendar.example/secret.ics".to_owned());
	let mut deps = SetupDeps::new();
	deps.calendar = Some(calendar.clone());
	run_setup(&h.paths, &deps, &io).await.unwrap();
	assert_eq!(
		*io.secrets.lock().unwrap(),
		["Your calendar's secret iCal address, for today's meetings (Enter to skip): "]
	);
	assert_eq!(io.asked(), ["Write these changes? [y/N] "]);
	assert_eq!(calendar.saved(), ["https://calendar.example/secret.ics"]);
}

#[tokio::test]
async fn with_no_calendar_secret_anywhere_setup_asks_once_enter_skips_an_address_is_saved() {
	let h = Harness::new();
	let skipped = Calendar::new(None);
	let (_, asked) = setup_with_calendar(&h, &skipped, &[Some("y"), Some("")]).await;
	assert!(asked.last().unwrap().contains("iCal address"));
	assert!(skipped.saved().is_empty());
	let given = Calendar::new(None);
	setup_with_calendar(&h, &given, &[Some("y"), Some("not a url")]).await;
	assert!(given.saved().is_empty());
	setup_with_calendar(&h, &given, &[Some("webcal://calendar.example/secret.ics")]).await;
	assert_eq!(given.saved(), ["webcal://calendar.example/secret.ics"]);
	assert!(h.marker("calendar").is_some());
}

/// A fake store for the mail and Jira secrets (0680).
#[derive(Default)]
struct Secrets {
	saved: Mutex<Vec<(String, String)>>,
	in_env: Vec<&'static str>,
}

impl SecretStore for Secrets {
	fn stored(&self, target: &str) -> bool {
		self.saved.lock().unwrap().iter().any(|(saved, _)| saved == target)
	}

	fn env_holds(&self, target: &str) -> bool {
		self.in_env.contains(&target)
	}

	fn save(&self, target: &str, secret: &str) -> Result<()> {
		self.saved.lock().unwrap().push((target.to_owned(), secret.to_owned()));
		Ok(())
	}
}

fn write_yaml(text: &str) {
	let file = crate::shared::settings::settings_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	fs::write(file, text).unwrap();
}

#[tokio::test]
async fn setup_asks_for_the_mail_and_jira_secrets_only_where_their_pollers_are_set_up_and_marks_what_it_saves() {
	let _j = temp_jocasta();
	let h = Harness::new();
	let run = async |store: &Arc<Secrets>, secret: &str| {
		let mut io = TestIo::new(&[Some("y")]);
		io.secret_answer = Some(secret.to_owned());
		let mut deps = SetupDeps::new();
		deps.secrets = Some(store.clone());
		run_setup(&h.paths, &deps, &io).await.unwrap();
		(io.text(), io.secrets.lock().unwrap().clone())
	};
	// Neither source is set up: nothing is asked.
	let store = Arc::new(Secrets::default());
	let (_, asked) = run(&store, "never used").await;
	assert!(asked.is_empty());
	// Both are: each secret is asked for, off the screen, and saved under its own target with a marker.
	write_yaml(
		"pollers:\n  gmail:\n    address: someone@example.org\n  jira:\n    site: https://example.atlassian.net\n    email: someone@example.org\n",
	);
	let (text, asked) = run(&store, "app-password-1").await;
	assert_eq!(
		asked,
		[
			"Your Gmail app password, for the unread mail in today (Enter to skip): ",
			"Your Jira API token, for your open issues in today (Enter to skip): ",
		]
	);
	assert_eq!(
		*store.saved.lock().unwrap(),
		[
			("jocasta/gmail-app-password".to_owned(), "app-password-1".to_owned()),
			("jocasta/jira-token".to_owned(), "app-password-1".to_owned()),
		]
	);
	assert!(!text.contains("app-password-1"));
	assert_eq!(h.marker("gmail").unwrap().undo, [Undo::Credential { target: "jocasta/gmail-app-password".to_owned() }]);
	assert_eq!(h.marker("jira").unwrap().undo, [Undo::Credential { target: "jocasta/jira-token".to_owned() }]);
	// Stored: a second run asks nothing. In the env: neither is asked.
	let (_, asked) = run(&store, "again").await;
	assert!(asked.is_empty());
	let env =
		Arc::new(Secrets { in_env: vec!["jocasta/gmail-app-password", "jocasta/jira-token"], ..Secrets::default() });
	let (_, asked) = run(&env, "again").await;
	assert!(asked.is_empty());
}

#[tokio::test]
async fn uninstall_lists_the_mail_and_jira_credentials_whoever_saved_them_while_they_are_stored() {
	let h = Harness::new();
	let stored = |target: &str| target == "jocasta/gmail-app-password" || target == "jocasta/jira-token";
	let undo = setup_undo(&h.paths, stored);
	for target in ["jocasta/gmail-app-password", "jocasta/jira-token"] {
		assert!(
			undo.iter().any(|item| matches!(item, SetupUndo::Other(Undo::Credential { target: t }) if t == target)),
			"{target}"
		);
	}
	assert!(
		!setup_undo(&h.paths, |_| false).iter().any(|item| matches!(item, SetupUndo::Other(Undo::Credential { .. })))
	);
}

#[tokio::test]
async fn setup_prints_one_note_when_git_is_older_than_2_54_or_missing_none_for_2_54_or_newer_and_goes_on_either_way() {
	let mut h = Harness::new();
	let mut note_for = async |version: Option<&str>| {
		let version = version.map(str::to_owned);
		h.git_version = Some(Arc::new(move || {
			let version = version.clone();
			Box::pin(async move { version })
		}));
		let Out { code, text, .. } = h.setup(Some("n")).await;
		let notes: Vec<String> =
			text.split('\n').filter(|l| l.contains("needs git 2.54 or newer")).map(str::to_owned).collect();
		(code, notes, text.contains("jocasta setup will make these changes"))
	};
	let short_version = Regex::new(r"^git version (\d+\.\d+).*").unwrap();
	for old in ["git version 2.53.0.windows.1", "git version 2.39.5", "git version 1.9.1"] {
		let (code, notes, listed) = note_for(Some(old)).await;
		let short = short_version.replace(old, "$1").into_owned();
		assert_eq!(
			notes,
			[format!(
				"git {short} is older than 2.54: Jocasta's check of pushes to main, master and develop needs git 2.54 or newer, so a pane's pushes there go unasked until you update git."
			)]
		);
		assert!(code == 0 && listed);
	}
	let (_, notes, listed) = note_for(None).await;
	assert_eq!(
		notes,
		[
			"git isn't installed: Jocasta's check of pushes to main, master and develop needs git 2.54 or newer, so a pane's pushes there go unasked until you update git."
		]
	);
	assert!(listed);
	for current in ["git version 2.54.0.windows.1", "git version 2.60.1", "git version 3.0.0"] {
		assert!(note_for(Some(current)).await.1.is_empty());
	}
}

/// What setup did on a machine with the old graph's compose file.
struct GraphRun {
	code: i32,
	text: String,
	asked: Vec<String>,
	docker: Vec<Vec<String>>,
	compose: PathBuf,
}

/// Runs setup on a machine with the old graph's compose file; `docker info` succeeds as `engine_runs` says, and every
/// other `docker` call succeeds.
async fn setup_with_graph(engine_runs: bool, answers: &[Option<&str>]) -> (Harness, GraphRun) {
	let h = Harness::new();
	let compose = h.paths.jocasta_config_dir.join("compose.yaml");
	put(&compose, "name: jocasta-demo\nservices: {}\n");
	put(&h.paths.jocasta_config_dir.join("containers").join("graphiti").join("Dockerfile"), "FROM scratch\n");
	let calls = Arc::new(Mutex::new(Vec::new()));
	let mut deps = h.deps();
	let log = calls.clone();
	deps.docker = Some(Arc::new(move |argv: Vec<String>| {
		let code = if argv[1] == "info" && !engine_runs { 1 } else { 0 };
		log.lock().unwrap().push(argv);
		Box::pin(async move { Ok(RunOutput { code, output: String::new() }) })
	}));
	let io = TestIo::new(answers);
	let code = run_setup(&h.paths, &deps, &io).await.unwrap();
	let docker = calls.lock().unwrap().clone();
	(h, GraphRun { code, text: io.text(), asked: io.asked(), docker, compose })
}

fn graph_gone(h: &Harness, run: &GraphRun) -> bool {
	!run.compose.exists() && !h.paths.jocasta_config_dir.join("containers").exists()
}

fn docker_argv(args: &[&str], compose: &Path) -> Vec<String> {
	let mut argv: Vec<String> = ["docker", "compose", "-f"].map(str::to_owned).to_vec();
	argv.push(compose.display().to_string());
	argv.extend(args.iter().map(|arg| (*arg).to_owned()));
	argv
}

const LATER_COMMAND: &str = "docker compose -p jocasta-demo down --rmi all --volumes";

#[tokio::test]
async fn an_install_with_the_old_graph_takes_its_containers_down_and_its_images_and_volume_go_only_on_his_yes() {
	let (h, kept) = setup_with_graph(true, &[Some("y"), Some("n")]).await;
	assert_eq!(kept.code, 0);
	assert_eq!(kept.docker, [vec!["docker".to_owned(), "info".to_owned()], docker_argv(&["down"], &kept.compose)]);
	assert_eq!(
		kept.asked.last().map(String::as_str),
		Some("Also remove the old graph's images and volume (Graphiti and FalkorDB)? [y/N] ")
	);
	assert!(graph_gone(&h, &kept));
	assert!(kept.text.contains(LATER_COMMAND));
	assert!(h.marker("0016-graphiti-removed").is_some());

	let (h, gone) = setup_with_graph(true, &[Some("y"), Some("y")]).await;
	assert_eq!(
		gone.docker,
		[
			vec!["docker".to_owned(), "info".to_owned()],
			docker_argv(&["down"], &gone.compose),
			docker_argv(&["down", "--rmi", "all", "--volumes"], &gone.compose),
		]
	);
	assert!(graph_gone(&h, &gone));
	assert!(!gone.text.contains("docker compose -p"));
}

#[tokio::test]
async fn an_install_with_the_old_graph_while_docker_isnt_running_asks_nothing_more_removes_the_files_and_names_the_command()
 {
	let (h, stopped) = setup_with_graph(false, &[Some("y")]).await;
	assert_eq!(stopped.asked, ["Write these changes? [y/N] "]);
	assert_eq!(stopped.docker, [vec!["docker".to_owned(), "info".to_owned()]]);
	assert!(graph_gone(&h, &stopped));
	assert!(stopped.text.contains(LATER_COMMAND));
}

/// On Linux the fence also names the keyring, GnuPG, gh's token, `/etc` and `/boot` (0701); `/usr` stays out. omp's
/// sleep inhibitor is off there (0696), and only there.
#[test]
fn the_os_values_linux_fences_five_folders_and_turns_the_sleep_inhibitor_off_0701_0696() {
	let paths = default_setup_paths(Path::new(REPO), Vec::new());
	assert_eq!(paths.sleep_prevention_off, cfg!(target_os = "linux"));
	if cfg!(target_os = "linux") {
		assert_eq!(
			paths.system_folders.unwrap(),
			["~/.local/share/keyrings", "~/.gnupg", "~/.config/gh", "/etc", "/boot"]
		);
	}
}
