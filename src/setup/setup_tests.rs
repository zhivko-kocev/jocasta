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
use serde_json::{Map, Value, json};

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

type CheckFn = Arc<dyn Fn(PathBuf, PathBuf) -> BoxFuture<'static, Result<HerdrCheck>> + Send + Sync>;
type ReloadFn = Arc<dyn Fn() -> BoxFuture<'static, Result<RunOutput>> + Send + Sync>;

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

fn ok_check() -> CheckFn {
	Arc::new(|_, _| Box::pin(async { Ok(HerdrCheck { code: Some(0), output: "config: ok\n".to_owned() }) }))
}

fn reloader(code: i32, output: &str, log: Arc<Mutex<Vec<String>>>) -> ReloadFn {
	let output = output.to_owned();
	Arc::new(move || {
		log.lock().unwrap().push("reload".to_owned());
		let output = output.clone();
		Box::pin(async move { Ok(RunOutput { code, output }) })
	})
}

struct Harness {
	root: TempDir,
	paths: SetupPaths,
	mise: Option<PathBuf>,
	check: Option<CheckFn>,
	reload: Option<ReloadFn>,
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
			mise_config_dir: base.join("mise"),
			jocasta_command: vec![PathBuf::from("C:\\Jocasta\\jocasta.exe")],
			whiteboard: Some(WhiteboardRuntime {
				node: PathBuf::from("C:\\Jocasta\\node\\node.exe"),
				cli: PathBuf::from("C:\\Jocasta\\whiteboard\\cli.js"),
			}),
		};
		// herdr's own check isn't run against a stand-in `herdr.exe`.
		Self { root, paths, mise: None, check: Some(ok_check()), reload: None, git_version: None }
	}

	fn deps(&self) -> SetupDeps {
		let mut deps = SetupDeps::new(PathBuf::from("herdr.exe"));
		deps.mise = self.mise.clone();
		deps.check_herdr_config = self.check.clone();
		deps.reload_herdr = self.reload.clone();
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

	fn herdr_toml(&self) -> PathBuf {
		self.paths.jocasta_config_dir.join("herdr.toml")
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

	/// herdr's omp hook as the snapshot writes it.
	fn hook(&self) -> PathBuf {
		self.paths.omp_agent_dir.join("extensions").join(herdr::embed::OMP_HOOK_FILE)
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
		vec![
			self.omp_config(),
			self.keybindings(),
			self.lsp(),
			self.watchdog(),
			self.mise_skill(),
			self.global_mcp(),
			self.hook(),
			self.secrets(),
		]
	}

	fn plugin_dir(&self) -> PathBuf {
		self.paths.jocasta_config_dir.join("herdr-plugin")
	}

	fn marker(&self, name: &str) -> Option<Marker> {
		read_marker(&self.paths.jocasta_config_dir, name)
	}

	fn undo(&self, calendar_stored: bool) -> Vec<SetupUndo> {
		setup_undo(&self.paths, calendar_stored)
	}

	fn whiteboard_server(&self) -> Value {
		json!({
			"type": "stdio",
			"command": "C:\\Jocasta\\node\\node.exe",
			"args": ["C:\\Jocasta\\whiteboard\\cli.js", "mcp"],
			"env": {
				"DEV_REVIEW_HOME": self.paths.data_dir.join("whiteboard").display().to_string(),
				"DO_NOT_TRACK": "1",
				"DEV_FAST_REVIEW_TELEMETRY_DISABLED": "1",
			},
		})
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
	assert!(text.contains(&format!("omp MCP servers \"jocasta\", \"whiteboard\": {}", s(&h.boss_mcp()))));
	assert!(text.contains(&format!("herdr config for session `jocasta`: {}", s(&h.herdr_toml()))));
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
	assert!(!h.herdr_toml().exists());
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
	assert_eq!(
		read_json(&h.global_mcp()),
		json!({"mcpServers": {"jocasta": {
			"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp"], "timeout": 1_860_000,
		}}})
	);
	// omp's 30 s request timeout is raised past the tools' own bounds (0577): a thread's 30-minute `check` plus a minute,
	// the boss's calls more.
	assert_eq!(
		read_json(&h.boss_mcp()),
		json!({"mcpServers": {
			"jocasta": {
				"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp", "--boss"], "timeout": 2_100_000,
			},
			"whiteboard": h.whiteboard_server(),
		}})
	);
	// The snapshot as shipped: `config/omp/` file for file, and herdr's hook from the fork's own asset.
	assert_eq!(read(&h.omp_config()), asset_text("omp/config.yml"));
	assert_eq!(read(&h.keybindings()), asset_text("omp/keybindings.yml"));
	assert_eq!(read(&h.lsp()), asset_text("omp/lsp.json"));
	assert_eq!(read(&h.watchdog()), asset_text("omp/WATCHDOG.md"));
	assert_eq!(read(&h.mise_skill()), asset_text("omp/skills/mise/SKILL.md"));
	assert_eq!(read(&h.secrets()), asset_text("omp/secrets.yml"));
	assert_eq!(read(&h.hook()), herdr::embed::OMP_HOOK);
	// Nothing was replaced: no backups.
	assert_eq!(h.backups(), None);
	// herdr's config as shipped: Jocasta draws the HUD itself, beside herdr's clock.
	assert_eq!(read(&h.herdr_toml()), asset_text("herdr.toml"));
	assert_eq!(read_json(&h.fragment()), json!({"schemes": [&*TOKYO_NIGHT_SCHEME]}));
	let mut markers: Vec<String> = fs::read_dir(markers_dir(&h.paths.jocasta_config_dir))
		.unwrap()
		.map(|entry| entry.unwrap().file_name().into_string().unwrap())
		.collect();
	markers.sort();
	assert_eq!(
		markers,
		[
			"0002-boss-mcp.json",
			"0005-herdr-config.json",
			"0007-terminal-scheme.json",
			"0018-herdr-plugin-gone.json",
			"0019-omp-snapshot.json",
		]
	);
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
		"hunk = \"0.23.0\"",
		"gh = \"2.92.0\"",
	] {
		assert!(MISE_FRAGMENT.lines().any(|l| l == line), "{line}");
	}
	// His own config.toml in the same folder is never touched.
	assert!(!h.paths.mise_config_dir.join("config.toml").exists());
}

#[tokio::test]
async fn the_bosss_mcp_json_keeps_his_other_servers_and_keys_only_jocastas_entries_change() {
	let h = Harness::new();
	let his = json!({
		"mcpServers": {
			"github": {"type": "stdio", "command": "gh-mcp", "args": []},
			"jocasta": {"type": "stdio", "command": "old\\jocasta.exe", "args": ["mcp", "--boss"]},
		},
		"disabledServers": ["github"],
	});
	put(&h.boss_mcp(), &pretty(&his));
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("-       \"command\": \"old\\\\jocasta.exe\","));
	assert!(text.contains("+       \"command\": \"C:\\\\Jocasta\\\\jocasta.exe\","));
	assert!(text.contains("        \"command\": \"gh-mcp\","));
	assert_eq!(
		read_json(&h.boss_mcp()),
		json!({
			"mcpServers": {
				"github": {"type": "stdio", "command": "gh-mcp", "args": []},
				"jocasta": {
					"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp", "--boss"], "timeout": 2_100_000,
				},
				"whiteboard": h.whiteboard_server(),
			},
			"disabledServers": ["github"],
		})
	);
}

#[tokio::test]
async fn without_whiteboards_build_setup_says_so_and_adds_only_the_jocasta_server() {
	let mut h = Harness::new();
	h.paths.whiteboard = None;
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.starts_with("whiteboard isn't built: run `mise run build`"));
	assert!(text.contains(&format!("omp MCP server \"jocasta\": {}", s(&h.boss_mcp()))));
	assert!(read_json(&h.boss_mcp())["mcpServers"].get("whiteboard").is_none());
	assert!(read_json(&h.global_mcp())["mcpServers"].get("whiteboard").is_none());
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
async fn a_herdr_toml_that_differs_from_the_shipped_template_shows_its_diff_and_is_replaced_only_after_y() {
	let h = Harness::new();
	let wanted = asset_text("herdr.toml");
	let old = wanted.replacen("agent_panel_sort = \"priority\"", "agent_panel_sort = \"recent\"", 1);
	put(&h.herdr_toml(), &old);
	let no = h.setup(Some("n")).await;
	assert!(no.text.contains(&format!("Jocasta's herdr config, to the shipped template: {}", s(&h.herdr_toml()))));
	assert!(has_line(&no.text, r#"^- .*agent_panel_sort = "recent""#));
	assert!(has_line(&no.text, r#"^\+ .*agent_panel_sort = "priority""#));
	assert_eq!(read(&h.herdr_toml()), old);
	let yes = h.setup(Some("y")).await;
	assert!(yes.text.contains(&format!("wrote {}", s(&h.herdr_toml()))));
	assert!(yes.text.contains("Done: run `jocasta` to open Jocasta."));
	assert_eq!(read(&h.herdr_toml()), wanted);
	assert!(!h.setup(Some("y")).await.text.contains(&s(&h.herdr_toml())));
}

#[tokio::test]
async fn a_migration_runs_once_per_shipped_version_his_later_edit_stays_until_jocasta_ships_a_new_one() {
	let mut h = Harness::new();
	let dir = h.own_assets();
	h.setup(Some("y")).await;
	let edited = format!("{}# his line\n", read(&h.herdr_toml()));
	put(&h.herdr_toml(), &edited);
	assert_eq!(h.setup(Some("y")).await.text, IN_PLACE);
	assert_eq!(read(&h.herdr_toml()), edited);

	put(&dir.join("herdr.toml"), &format!("{}# shipped later\n", read(&dir.join("herdr.toml"))));
	let newer = h.setup(Some("y")).await;
	assert!(newer.text.contains(&format!("Jocasta's herdr config, to the shipped template: {}", s(&h.herdr_toml()))));
	assert!(newer.text.contains("- # his line"));
	assert!(read(&h.herdr_toml()).ends_with("# shipped later\n"));
	assert_eq!(h.marker("0005-herdr-config").unwrap().fingerprint, fingerprint(&read(&h.herdr_toml())));
}

#[tokio::test]
async fn a_failed_migration_stops_the_rest_and_says_why_the_ones_before_it_are_marked() {
	let h = Harness::new();
	let mut io = TestIo::new(&[Some("y")]);
	// herdr.toml appears while the diff that named it new waits for his answer.
	let herdr_toml = h.herdr_toml();
	io.on_ask = Some(Box::new(move || put(&herdr_toml, "# his\n")));
	let code = run_setup(&h.paths, &h.deps(), &io).await.unwrap();
	assert_eq!(code, 1);
	let text = io.text();
	assert!(
		text.contains(&format!(
			"jocasta setup: 0005-herdr-config (herdr config for session `jocasta`) failed: {} changed since the diff; run jocasta setup again; the 2 changes after it wait.",
			s(&h.herdr_toml())
		)),
		"{text}"
	);
	assert_eq!(read(&h.herdr_toml()), "# his\n");
	assert!(h.boss_mcp().exists());
	assert!(h.marker("0002-boss-mcp").is_some());
	assert!(h.marker("0005-herdr-config").is_none());
	assert!(!h.global_mcp().exists());
	assert!(h.marker("0019-omp-snapshot").is_none());
}

#[tokio::test]
async fn a_broken_mcp_json_is_refused_before_anything_is_written() {
	let h = Harness::new();
	put(&h.boss_mcp(), "{ not json");
	let error = h.setup_with(Some("y")).await.err().expect("a broken mcp.json is refused");
	assert!(error.to_string().contains(&format!("{} is not JSON", s(&h.boss_mcp()))));
	assert!(!h.global_mcp().exists());
}

#[tokio::test]
async fn refresh_herdr_toml_shows_the_diff_keeps_a_dated_backup_writes_the_shipped_file_and_reloads_herdr() {
	let mut h = Harness::new();
	h.setup(Some("y")).await;
	let shipped = read(&h.herdr_toml());
	put(&h.herdr_toml(), &format!("{shipped}# his line\n"));
	let reloads = Arc::new(Mutex::new(Vec::new()));
	h.reload = Some(reloader(0, "", reloads.clone()));
	let no = h.refresh("herdr.toml", Some("n")).await.unwrap();
	assert!(no.text.contains("- # his line"));
	assert!(no.text.ends_with("Nothing written."));
	assert!(reloads.lock().unwrap().is_empty());

	let yes = h.refresh("herdr.toml", Some("y")).await.unwrap();
	let backup = format!("{}.bak.20261023-090507", s(&h.herdr_toml()));
	assert_eq!(yes.code, 0);
	assert!(yes.text.contains(&format!("kept {backup}")));
	assert!(yes.text.ends_with("herdr reloaded its config."));
	assert_eq!(read(Path::new(&backup)), format!("{shipped}# his line\n"));
	assert_eq!(read(&h.herdr_toml()), shipped);
	assert_eq!(*reloads.lock().unwrap(), ["reload"]);
	assert_eq!(
		h.refresh(&s(&h.herdr_toml()), Some("y")).await.unwrap().text,
		format!("{} already is the shipped version.", s(&h.herdr_toml()))
	);
}

#[tokio::test]
async fn refresh_says_to_restart_herdr_when_it_cant_reload_resets_jocastas_mcp_entries_and_refuses_other_files() {
	let mut h = Harness::new();
	h.setup(Some("y")).await;
	put(&h.herdr_toml(), "# his\n");
	h.reload = Some(reloader(1, "error: no running server for session jocasta\n", Arc::default()));
	assert!(h.refresh("herdr.toml", Some("y")).await.unwrap().text.ends_with(
		"herdr didn't reload its config (error: no running server for session jocasta): restart herdr to use it (close Jocasta, then run `jocasta`)."
	));

	let mut mcp = read_json(&h.boss_mcp());
	mcp["mcpServers"]["jocasta"]["args"] = json!(["mcp", "--his"]);
	mcp["mcpServers"]["github"] = json!({"type": "stdio", "command": "gh-mcp"});
	put(&h.boss_mcp(), &mcp.to_string());
	let reset = h.refresh("mcp.json", Some("y")).await.unwrap();
	assert!(reset.text.contains(&format!("kept {}.bak.20261023-090507", s(&h.boss_mcp()))));
	assert_eq!(
		read_json(&h.boss_mcp())["mcpServers"],
		json!({
			"jocasta": {
				"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp", "--boss"], "timeout": 2_100_000,
			},
			"whiteboard": h.whiteboard_server(),
			"github": {"type": "stdio", "command": "gh-mcp"},
		})
	);

	let refused = h.refresh("config.yml", Some("y")).await.err().unwrap();
	assert!(refused.is_user());
	assert!(refused.to_string().contains("isn't a file setup refreshes"));
}

#[tokio::test]
async fn refresh_jocasta_json_puts_back_the_terminal_scheme_he_changed_after_setup_keeping_his_copy_herdr_isnt_reloaded()
 {
	let mut h = Harness::new();
	h.setup(Some("y")).await;
	let shipped = read(&h.fragment());
	put(&h.fragment(), "his own\n");
	let reloads = Arc::new(Mutex::new(Vec::new()));
	h.reload = Some(reloader(0, "", reloads.clone()));
	let yes = h.refresh("JOCASTA.JSON", Some("y")).await.unwrap();
	let backup = format!("{}.bak.20261023-090507", s(&h.fragment()));
	assert_eq!(yes.code, 0);
	assert!(yes.text.contains(&format!("Windows Terminal scheme Tokyo Night: {}", s(&h.fragment()))));
	assert!(yes.text.contains(&format!("kept {backup}")));
	assert_eq!(read(Path::new(&backup)), "his own\n");
	assert_eq!(read(&h.fragment()), shipped);
	assert!(reloads.lock().unwrap().is_empty());
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
		assert!(!h.global_mcp().exists());
	}
	h.setup(Some(" YES ")).await;
	assert!(h.global_mcp().exists());
	put(&h.herdr_toml(), "# his\n");
	assert!(
		h.refresh("herdr.toml", Some("Y"))
			.await
			.unwrap()
			.text
			.contains(&format!("kept {}.bak.20261023-090507", s(&h.herdr_toml())))
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
	let mut writes = Map::new();
	writes.insert(
		"jocasta".to_owned(),
		json!({"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp", "--boss"], "timeout": 2_100_000}),
	);
	writes.insert("whiteboard".to_owned(), h.whiteboard_server());
	assert!(undo.contains(&SetupUndo::Mcp(McpUndo {
		path: s(&h.boss_mcp()),
		servers: strings(&["jocasta", "whiteboard"]),
		writes,
		wrote: h.marker("0002-boss-mcp").unwrap().fingerprint,
	})));
	assert!(undo.contains(&other(Undo::File { path: s(&h.herdr_toml()) })));
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
	// omp's default is off: without it the snapshot would turn his advisor off in every pane (0619, 0620).
	assert_eq!(config["advisor"], json!({"enabled": true, "reviewInterval": 3}));
	// His fallbacks (0590), dictation and speech kept on the machine (0556).
	let chains = &config["retry"]["fallbackChains"];
	assert_eq!(chains["plan"], json!(["anthropic/claude-opus-5-5:max", "anthropic/claude-opus-5:max"]));
	assert_eq!(chains["slow"], chains["plan"]);
	assert_eq!(chains["dictation"], json!([]));
	assert_eq!(chains["speech"], json!([]));
	assert_eq!(config["modelRoles"]["default"], "anthropic/claude-opus-5-5:high");
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
	assert!(!h.keybindings().exists() && !h.global_mcp().exists() && !h.hook().exists());
	assert_eq!(h.backups(), None);
	assert!(h.marker("0019-omp-snapshot").is_none());
}

#[tokio::test]
async fn refresh_herdr_toml_works_beside_a_malformed_mcp_json_it_doesnt_reset() {
	let h = Harness::new();
	h.setup(Some("y")).await;
	put(&h.herdr_toml(), "# his\n");
	put(&h.boss_mcp(), "{ broken");
	let Out { code, text, .. } = h.refresh("herdr.toml", Some("y")).await.unwrap();
	assert_eq!(code, 0);
	assert!(text.contains(&format!("kept {}.bak.20261023-090507", s(&h.herdr_toml()))));
	assert_ne!(read(&h.herdr_toml()), "# his\n");
}

#[tokio::test]
async fn setup_reloads_herdr_after_writing_herdr_toml_and_says_so() {
	let mut h = Harness::new();
	let reloads = Arc::new(Mutex::new(Vec::new()));
	h.reload = Some(reloader(0, "", reloads.clone()));
	let first = h.setup(Some("y")).await;
	// Once for the config, with the line.
	assert_eq!(*reloads.lock().unwrap(), ["reload"]);
	assert!(first.text.contains("herdr reloaded its config.\nDone: run `jocasta` to open Jocasta."));
	reloads.lock().unwrap().clear();
	let dir = h.own_assets();
	put(&dir.join("herdr.toml"), &format!("{}# shipped later\n", read(&dir.join("herdr.toml"))));
	h.reload = Some(reloader(1, "error: no running server for session jocasta\n", reloads));
	assert!(h.setup(Some("y")).await.text.contains(
		"herdr didn't reload its config (error: no running server for session jocasta): restart herdr to use it (close Jocasta, then run `jocasta`)."
	));
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
	let mut deps = SetupDeps::new(PathBuf::from("herdr.exe"));
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
	let mut deps = SetupDeps::new(PathBuf::from("herdr.exe"));
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

#[tokio::test]
async fn after_writing_herdr_toml_setup_runs_herdr_config_check_on_it_and_prints_each_line_it_reports() {
	let mut h = Harness::new();
	let checked: Arc<Mutex<Vec<(PathBuf, PathBuf)>>> = Arc::default();
	let seen = checked.clone();
	h.check = Some(Arc::new(move |herdr, path| {
		seen.lock().unwrap().push((herdr, path));
		Box::pin(async {
			Ok(HerdrCheck {
				code: Some(1),
				output: "config: issues found\nunknown config key made_up_key; ignoring key\n".to_owned(),
			})
		})
	}));
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 0);
	assert_eq!(*checked.lock().unwrap(), [(PathBuf::from("herdr.exe"), h.herdr_toml())]);
	assert!(text.contains(&format!("herdr config check ({}):", s(&h.herdr_toml()))));
	assert!(text.contains("  unknown config key made_up_key; ignoring key"));
	// The file was written before the check ran, and nothing else is stopped by what it reports.
	assert!(h.herdr_toml().exists());
	assert!(text.contains("Done: run `jocasta` to open Jocasta."));
	// Up to date: nothing written, no check.
	checked.lock().unwrap().clear();
	h.setup(Some("y")).await;
	assert!(checked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_herdr_config_check_that_cant_run_is_said_and_doesnt_stop_setup() {
	let mut h = Harness::new();
	h.check = Some(Arc::new(|_, _| Box::pin(async { Err(Error::other("spawn failed")) })));
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 0);
	assert!(text.contains("herdr config check didn't run: spawn failed"));
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
