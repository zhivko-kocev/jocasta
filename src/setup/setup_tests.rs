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
use crate::setup::config_file::edit_config;
use crate::setup::markers::{fingerprint, markers_dir};
use crate::setup::mise::MISE_FRAGMENT;
use crate::setup::omp_settings::{omp_settings_text, omp_yaml};
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

fn run_ok(ran: Arc<Mutex<Vec<Vec<String>>>>, omp_agent_dir: PathBuf) -> RunProgram {
	// Stands in for herdr: writes the extension file the real install writes.
	Arc::new(move |argv| {
		ran.lock().unwrap().push(argv);
		let file = omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION);
		fs::create_dir_all(file.parent().unwrap()).unwrap();
		fs::write(file, "// herdr\n").unwrap();
		Box::pin(async { Ok(RunOutput { code: 0, output: String::new() }) })
	})
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
	ran: Arc<Mutex<Vec<Vec<String>>>>,
	run: RunProgram,
	mise: Option<PathBuf>,
	check: Option<CheckFn>,
	reload: Option<ReloadFn>,
	run_value: Option<Arc<dyn RunValueStore>>,
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
		let ran = Arc::new(Mutex::new(Vec::new()));
		let run = run_ok(ran.clone(), paths.omp_agent_dir.clone());
		// herdr's own check isn't run against a stand-in `herdr.exe`.
		Self {
			root,
			paths,
			ran,
			run,
			mise: None,
			check: Some(ok_check()),
			reload: None,
			run_value: None,
			git_version: None,
		}
	}

	fn ran(&self) -> Vec<Vec<String>> {
		self.ran.lock().unwrap().clone()
	}

	fn deps(&self, herdr: Option<&str>) -> SetupDeps {
		let mut deps = SetupDeps::new(herdr.map(PathBuf::from), self.run.clone());
		deps.mise = self.mise.clone();
		deps.check_herdr_config = self.check.clone();
		deps.reload_herdr = self.reload.clone();
		deps.run_value = self.run_value.clone();
		deps.git_version = self.git_version.clone();
		deps
	}

	async fn setup_with(&self, herdr: Option<&str>, answer: Option<&str>) -> Result<Out> {
		let io = TestIo::new(&[answer]);
		let code = run_setup(&self.paths, &self.deps(herdr), &io).await?;
		Ok(Out { code, text: io.text(), asked: io.asked() })
	}

	async fn setup(&self, answer: Option<&str>) -> Out {
		self.setup_with(Some("herdr.exe"), answer).await.unwrap()
	}

	async fn setup_without_herdr(&self, answer: Option<&str>) -> Out {
		self.setup_with(None, answer).await.unwrap()
	}

	async fn refresh(&self, file: &str, answer: Option<&str>) -> Result<Out> {
		let io = TestIo::new(&[answer]);
		let now = chrono::Local.with_ymd_and_hms(2026, 10, 23, 9, 5, 7).unwrap();
		let code = refresh_setup(&self.paths, &self.deps(Some("herdr.exe")), &io, file, Some(now)).await?;
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
const HERDR_LINE: &str = "herdr isn't installed: install it from https://herdr.dev, then run `jocasta setup` again.";

fn other(undo: Undo) -> SetupUndo {
	SetupUndo::Other(undo)
}

#[tokio::test]
async fn a_fresh_machine_the_diff_names_every_file_and_n_writes_nothing() {
	let h = Harness::new();
	put(
		&h.paths.jocasta_config_dir.join("settings.json"),
		&json!({"frontier": {"port": 47615}, "theme": "x"}).to_string(),
	);
	let Out { code, text, asked } = h.setup(Some("n")).await;
	assert_eq!(code, 0);
	assert_eq!(asked, ["Write these changes? [y/N] "]);
	assert!(text.contains(&format!("omp MCP server \"jocasta\": {}", s(&h.global_mcp()))));
	assert!(text.contains(&format!("omp MCP servers \"jocasta\", \"whiteboard\": {}", s(&h.boss_mcp()))));
	assert!(text.contains(&format!("omp settings: {}", s(&h.omp_config()))));
	let extension = h.paths.omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION);
	assert!(text.contains(&format!("+ {} (written by herdr)", s(&extension))));
	assert!(text.contains(&format!("herdr config for session `jocasta`: {}", s(&h.herdr_toml()))));
	assert!(text.contains(&format!("Jocasta settings: {}", s(&h.paths.jocasta_config_dir.join("jocasta.yaml")))));
	assert!(text.contains(&format!("Windows Terminal scheme Tokyo Night: {}", s(&h.fragment()))));
	assert!(text.contains(&format!("omp skill mise: {}", s(&h.mise_skill()))));
	assert!(text.contains(&format!(
		"omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation: {}",
		s(&h.keybindings())
	)));
	assert!(text.contains(&format!("the advisor's brief: {}", s(&h.watchdog()))));
	assert!(text.contains(&format!("omp language servers: idle ones stop after 5 minutes: {}", s(&h.lsp()))));
	assert!(
		text.contains(&format!(
			"omp's secrets.yml: your own strings to hide, an empty list to fill: {}",
			s(&h.secrets())
		))
	);
	// No mise on this machine: no mise file (0476).
	assert!(!text.contains(&s(&h.mise_file())));
	assert!(text.ends_with("Nothing written."));
	assert!(!h.root.path().join("omp").exists());
	assert!(!h.paths.data_dir.exists());
	assert!(!markers_dir(&h.paths.jocasta_config_dir).exists());
	assert!(!h.herdr_toml().exists());
	assert!(!h.fragment().exists());
	assert!(h.ran().is_empty());
	// End of input is a no too.
	assert!(h.setup(None).await.text.ends_with("Nothing written."));
}

#[tokio::test]
async fn y_writes_every_file_marks_each_migration_runs_herdrs_omp_integration_and_a_second_run_has_nothing_to_do() {
	let h = Harness::new();
	put(
		&h.paths.jocasta_config_dir.join("settings.json"),
		&json!({
			"frontier": {"quietHours": {"from": "22:00", "to": "07:00"}},
			"pollers": {"github": {"enabled": true}},
		})
		.to_string(),
	);
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 0);
	assert!(text.contains(&format!("wrote {}", s(&h.global_mcp()))));
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
	assert_eq!(read(&h.omp_config()), asset_text("omp-config.yml"));
	assert_eq!(h.ran(), [strings(&["herdr.exe", "integration", "install", "omp"])]);
	// herdr's config as shipped: Jocasta draws the HUD itself, beside herdr's clock.
	assert_eq!(read(&h.herdr_toml()), asset_text("herdr.toml"));
	assert_eq!(
		yaml(&read(&h.paths.jocasta_config_dir.join("jocasta.yaml"))),
		json!({"frontier": {"quietHours": {"from": "22:00", "to": "07:00"}}, "pollers": {"github": {"enabled": true}}})
	);
	assert_eq!(read_json(&h.fragment()), json!({"schemes": [&*TOKYO_NIGHT_SCHEME]}));
	assert_eq!(read(&h.mise_skill()), asset_text("skills/mise/SKILL.md"));
	assert_eq!(read(&h.keybindings()), omp_yaml(&json!({"app.live.toggle": [], "app.stt.toggle": "f9"})));
	assert_eq!(read(&h.watchdog()), asset_text("WATCHDOG.md"));
	assert_eq!(read_json(&h.lsp()), json!({"idleTimeoutMs": 300_000}));
	assert_eq!(read(&h.secrets()), asset_text("secrets.yml"));
	let mut markers: Vec<String> = fs::read_dir(markers_dir(&h.paths.jocasta_config_dir))
		.unwrap()
		.map(|entry| entry.unwrap().file_name().into_string().unwrap())
		.collect();
	markers.sort();
	assert_eq!(
		markers,
		[
			"0001-mcp.json",
			"0002-boss-mcp.json",
			"0003-omp-settings.json",
			"0004-herdr-integration.json",
			"0005-herdr-config.json",
			"0006-jocasta-yaml.json",
			"0007-terminal-scheme.json",
			"0009-mise-skill.json",
			"0010-omp-keybindings.json",
			"0011-omp-watchdog.json",
			"0012-omp-lsp.json",
			"0015-omp-secrets.json",
			"0017-whiteboard-boss-only.json",
			"0018-herdr-plugin-gone.json",
		]
	);

	let again = h.setup(Some("y")).await;
	assert_eq!(again.text, IN_PLACE);
	assert_eq!(h.ran().len(), 1);
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
async fn an_existing_mcp_json_keeps_his_other_servers_and_keys_only_jocastas_entries_change() {
	let h = Harness::new();
	let his = json!({
		"mcpServers": {
			"github": {"type": "stdio", "command": "gh-mcp", "args": []},
			"jocasta": {"type": "stdio", "command": "old\\jocasta.exe", "args": ["mcp"]},
		},
		"disabledServers": ["github"],
	});
	put(&h.global_mcp(), &pretty(&his));
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("-       \"command\": \"old\\\\jocasta.exe\","));
	assert!(text.contains("+       \"command\": \"C:\\\\Jocasta\\\\jocasta.exe\","));
	assert!(text.contains("        \"command\": \"gh-mcp\","));
	assert_eq!(
		read_json(&h.global_mcp()),
		json!({
			"mcpServers": {
				"github": {"type": "stdio", "command": "gh-mcp", "args": []},
				"jocasta": {
					"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp"], "timeout": 1_860_000,
				},
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
	assert!(text.contains(&format!("omp MCP server \"jocasta\": {}", s(&h.global_mcp()))));
	assert!(text.contains(&format!("omp MCP server \"jocasta\": {}", s(&h.boss_mcp()))));
	assert!(read_json(&h.boss_mcp())["mcpServers"].get("whiteboard").is_none());
}

#[tokio::test]
async fn his_config_yml_gets_the_keys_jocasta_ships_merged_in_his_other_keys_untouched_and_his_own_values_kept_and_shown()
 {
	let mut h = Harness::new();
	let dir = h.own_assets();
	put(
		&dir.join("omp-config.yml"),
		"# Jocasta's\ncompaction:\n  thresholdPercent: 60\n  idleEnabled: true\nmemory:\n  backend: mnemopi\n",
	);
	put(&h.omp_config(), "theme:\n  dark: his-theme\ncompaction:\n  thresholdPercent: 45\n");
	let no = h.setup(Some("n")).await;
	assert!(
		no.text.contains(&format!(
			"omp settings Jocasta ships (yours kept where you changed them): {}",
			s(&h.omp_config())
		))
	);
	assert!(no.text.contains("  kept: compaction.thresholdPercent: yours 45 kept (Jocasta ships 60)"));
	assert!(has_line(&no.text, r"^\+ {3}idleEnabled: true$"));
	assert!(has_line(&no.text, r"^\+ memory:$"));
	assert_eq!(read(&h.omp_config()), "theme:\n  dark: his-theme\ncompaction:\n  thresholdPercent: 45\n");

	h.setup(Some("y")).await;
	assert_eq!(
		yaml(&read(&h.omp_config())),
		json!({
			"theme": {"dark": "his-theme"},
			"compaction": {"thresholdPercent": 45, "idleEnabled": true},
			"memory": {"backend": "mnemopi"},
		})
	);
	// Written as omp writes it, and marked: the same shipped file is never proposed again.
	assert_eq!(
		read(&h.omp_config()),
		omp_yaml(&json!({
			"theme": {"dark": "his-theme"},
			"compaction": {"thresholdPercent": 45, "idleEnabled": true},
			"memory": {"backend": "mnemopi"},
		}))
	);
	assert!(!h.setup(Some("y")).await.text.contains(&s(&h.omp_config())));

	// A newer shipped value replaces a key still at the value shipped before; his own stays.
	put(
		&dir.join("omp-config.yml"),
		"compaction:\n  thresholdPercent: 70\n  idleEnabled: false\nmemory:\n  backend: mnemopi\n",
	);
	let newer = h.setup(Some("y")).await;
	assert!(newer.text.contains("  kept: compaction.thresholdPercent: yours 45 kept (Jocasta ships 70)"));
	assert_eq!(
		yaml(&read(&h.omp_config())),
		json!({
			"theme": {"dark": "his-theme"},
			"compaction": {"thresholdPercent": 45, "idleEnabled": false},
			"memory": {"backend": "mnemopi"},
		})
	);
}

#[tokio::test]
async fn jocasta_yaml_is_never_overwritten_the_terminal_fragment_follows_the_shipped_scheme_after_y() {
	let h = Harness::new();
	let jocasta_yaml = h.paths.jocasta_config_dir.join("jocasta.yaml");
	put(&jocasta_yaml, "his own\n");
	put(&h.paths.jocasta_config_dir.join("settings.json"), &json!({"frontier": {"port": 1}}).to_string());
	put(&h.fragment(), "his own\n");
	let Out { text, .. } = h.setup(Some("y")).await;
	assert_eq!(read(&jocasta_yaml), "his own\n");
	assert!(!text.contains(&s(&jocasta_yaml)));
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
	let mut h = Harness::new();
	let ran = h.ran.clone();
	h.run = Arc::new(move |argv| {
		ran.lock().unwrap().push(argv);
		Box::pin(async { Ok(RunOutput { code: 3, output: "no omp found\n".to_owned() }) })
	});
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 1);
	assert!(
		text.contains(
			"jocasta setup: 0004-herdr-integration (herdr's omp integration (`herdr integration install omp`)) failed: herdr integration install omp exited 3: no omp found; the 7 changes after it wait."
		),
		"{text}"
	);
	assert!(h.global_mcp().exists());
	assert!(!h.herdr_toml().exists());
	assert!(h.marker("0003-omp-settings").is_some());
	assert!(h.marker("0004-herdr-integration").is_none());
}

#[tokio::test]
async fn without_herdr_setup_says_so_and_does_the_rest_a_broken_mcp_json_is_refused_before_anything_is_written() {
	let h = Harness::new();
	let Out { text, .. } = h.setup_without_herdr(Some("y")).await;
	assert!(text.starts_with("herdr isn't installed: install it from https://herdr.dev"));
	assert!(h.ran().is_empty());
	assert!(h.global_mcp().exists());
	assert!(h.marker("0004-herdr-integration").is_none());

	remove_tree(h.root.path()).unwrap();
	put(&h.global_mcp(), "{ not json");
	let error = h.setup_with(Some("herdr.exe"), Some("y")).await.err().expect("a broken mcp.json is refused");
	assert!(error.to_string().contains(&format!("{} is not JSON", s(&h.global_mcp()))));
	assert!(!h.boss_mcp().exists());
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

	let mut mcp = read_json(&h.global_mcp());
	mcp["mcpServers"]["jocasta"]["args"] = json!(["mcp", "--his"]);
	mcp["mcpServers"]["github"] = json!({"type": "stdio", "command": "gh-mcp"});
	put(&h.global_mcp(), &mcp.to_string());
	let reset = h.refresh("mcp.json", Some("y")).await.unwrap();
	assert!(reset.text.contains(&format!("{} already is the shipped version.", s(&h.boss_mcp()))));
	assert!(reset.text.contains(&format!("kept {}.bak.20261023-090507", s(&h.global_mcp()))));
	assert_eq!(
		read_json(&h.global_mcp())["mcpServers"],
		json!({
			"jocasta": {"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp"], "timeout": 1_860_000},
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
	let global = undo
		.iter()
		.find(|u| matches!(u, SetupUndo::Mcp(m) if m.path == s(&h.global_mcp())))
		.expect("the global mcp.json's undo");
	let mut writes = Map::new();
	writes.insert(
		"jocasta".to_owned(),
		json!({"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp"], "timeout": 1_860_000}),
	);
	writes.insert("whiteboard".to_owned(), h.whiteboard_server());
	assert_eq!(
		*global,
		SetupUndo::Mcp(McpUndo {
			path: s(&h.global_mcp()),
			servers: vec!["jocasta".to_owned()],
			writes,
			wrote: h.marker("0001-mcp").unwrap().fingerprint,
		})
	);
	assert!(undo.iter().any(|u| matches!(
		u,
		SetupUndo::Mcp(m) if m.path == s(&h.boss_mcp()) && m.servers.join(",") == "jocasta,whiteboard"
	)));
	let extension = h.paths.omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION);
	assert!(undo.contains(&other(Undo::HerdrIntegration { path: s(&extension) })));
	assert!(undo.contains(&other(Undo::File { path: s(&h.herdr_toml()) })));
	assert!(undo.contains(&other(Undo::Dir {
		path: s(h.fragment().parent().unwrap()),
		file: Some("jocasta.json".to_owned())
	})));
	assert!(undo.contains(&other(Undo::File { path: s(&h.mise_file()) })));
	assert!(
		undo.contains(&other(Undo::Dir {
			path: s(h.mise_skill().parent().unwrap()),
			file: Some("SKILL.md".to_owned())
		}))
	);
	// A file in a folder that goes whole isn't named again (0484).
	assert!(!undo.contains(&other(Undo::File { path: s(&h.mise_skill()) })));
	// The three omp files Jocasta made (0479): only its keys, and the brief while it is still Jocasta's text.
	let Value::Object(keys) = json!({"app.live.toggle": [], "app.stt.toggle": "f9"}) else { unreachable!() };
	assert!(undo.contains(&other(Undo::Keys { path: s(&h.keybindings()), keys, created: true })));
	assert!(
		undo.contains(&other(Undo::Text {
			path: s(&h.watchdog()),
			fingerprint: fingerprint(&asset_text("WATCHDOG.md"))
		}))
	);
	let Value::Object(lsp_keys) = json!({"idleTimeoutMs": 300_000}) else { unreachable!() };
	assert!(undo.contains(&other(Undo::Keys { path: s(&h.lsp()), keys: lsp_keys, created: true })));
	// omp's config.yml stays: its keys are his settings.
	let all: Vec<String> = undo.iter().map(|u| u.to_json().to_string()).collect();
	assert!(all.iter().all(|u| !u.contains("config.yml")));
	let unique: std::collections::HashSet<&String> = all.iter().collect();
	assert_eq!(unique.len(), undo.len());

	// Without the markers nothing is setup's to undo, whatever is there, and a stored calendar secret only with its
	// marker (B1).
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	assert!(h.undo(true).is_empty());
}

#[tokio::test]
async fn herdrs_omp_integration_that_was_there_before_setup_isnt_setups_to_undo_one_setup_installed_is() {
	let h = Harness::new();
	let extension = h.paths.omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION);
	let integration = other(Undo::HerdrIntegration { path: s(&extension) });
	put(&extension, "// another install's\n");
	h.setup(Some("y")).await;
	assert!(h.ran().is_empty());
	assert!(!h.undo(false).contains(&integration));

	fs::remove_file(&extension).unwrap();
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	h.setup(Some("y")).await;
	assert_eq!(h.ran(), [strings(&["herdr.exe", "integration", "install", "omp"])]);
	assert!(h.undo(false).contains(&integration));
}

#[tokio::test]
async fn keybindings_yml_and_lsp_json_get_only_jocastas_key_his_other_keys_and_his_own_value_for_it_kept() {
	let h = Harness::new();
	put(&h.keybindings(), "app.model.cycle:\n  - ctrl+k\n");
	put(&h.lsp(), &pretty(&json!({"servers": {"biome": {"disabled": true}}, "idleTimeoutMs": 60000})));
	let no = h.setup(Some("n")).await;
	assert!(no.text.contains(&format!(
		"omp key bindings: Ctrl+L no longer starts live voice; F9 toggles dictation: {}",
		s(&h.keybindings())
	)));
	assert!(has_line(&no.text, r"^\+ app\.live\.toggle: \[\]$"));
	assert!(
		no.text
			.contains(&format!("omp language servers: idle ones stop after 5 minutes (yours kept): {}", s(&h.lsp())))
	);
	assert!(no.text.contains("  kept: idleTimeoutMs: yours 60000 kept (Jocasta ships 300000)"));
	assert_eq!(read(&h.keybindings()), "app.model.cycle:\n  - ctrl+k\n");

	let yes = h.setup(Some("y")).await;
	assert!(yes.text.contains(&format!("kept {}", s(&h.lsp()))));
	assert_eq!(
		yaml(&read(&h.keybindings())),
		json!({"app.model.cycle": ["ctrl+k"], "app.live.toggle": [], "app.stt.toggle": "f9"})
	);
	assert_eq!(read_json(&h.lsp()), json!({"servers": {"biome": {"disabled": true}}, "idleTimeoutMs": 60000}));
	// Setup made neither file and added only its two keys: that is all uninstall may take out.
	let undo = h.undo(false);
	let Value::Object(keys) = json!({"app.live.toggle": [], "app.stt.toggle": "f9"}) else { unreachable!() };
	assert!(undo.contains(&other(Undo::Keys { path: s(&h.keybindings()), keys, created: false })));
	let all = undo.iter().map(|u| u.to_json().to_string()).collect::<String>();
	assert!(!all.contains(&serde_json::to_string(&s(&h.lsp())).unwrap()));
	assert!(!h.setup(Some("y")).await.text.contains(&s(&h.keybindings())));
}

#[tokio::test]
async fn his_keybindings_yaml_is_the_one_merged_an_old_keybindings_json_waits_for_omp_to_move_it() {
	let h = Harness::new();
	let yaml_file = h.paths.omp_agent_dir.join("keybindings.yaml");
	put(&yaml_file, "app.model.cycle: [ctrl+k]\n");
	h.setup(Some("y")).await;
	assert_eq!(
		yaml(&read(&yaml_file)),
		json!({"app.model.cycle": ["ctrl+k"], "app.live.toggle": [], "app.stt.toggle": "f9"})
	);
	assert!(!h.keybindings().exists());

	fs::remove_file(&yaml_file).unwrap();
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	put(&h.paths.omp_agent_dir.join("keybindings.json"), "{ \"app.model.cycle\": [\"ctrl+k\"] }\n");
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("omp's keybindings.json is in its old format: start omp once"));
	assert!(!h.keybindings().exists());
}

#[tokio::test]
async fn watchdog_md_written_when_missing_his_own_shown_and_kept_jocastas_older_text_follows_the_new_one() {
	let mut h = Harness::new();
	let dir = h.own_assets();
	put(&h.watchdog(), "# His brief\n");
	let his = h.setup(Some("y")).await;
	let kept_title = format!("the advisor's brief (yours kept; Jocasta's differs as shown): {}", s(&h.watchdog()));
	assert!(his.text.contains(&kept_title));
	assert!(has_line(&his.text, r"^- # His brief$"));
	assert!(has_line(&his.text, r"^\+ # Advisor brief \(Jocasta\)$"));
	assert!(
		his.text.contains("  kept: yours stays as it is, never overwritten (Jocasta's text is config/WATCHDOG.md)")
	);
	assert!(his.text.contains(&format!("kept {}", s(&h.watchdog()))));
	assert_eq!(read(&h.watchdog()), "# His brief\n");
	let undo_text = |h: &Harness| h.undo(false).iter().map(|u| u.to_json().to_string()).collect::<String>();
	assert!(!undo_text(&h).contains(&serde_json::to_string(&s(&h.watchdog())).unwrap()));
	// Shown once: the same shipped text never asks again.
	assert!(!h.setup(Some("y")).await.text.contains(&s(&h.watchdog())));
	// A newer shipped text is shown again, and his stays.
	put(&dir.join("WATCHDOG.md"), "# Advisor brief v2\n");
	assert!(h.setup(Some("y")).await.text.contains(&kept_title));
	assert_eq!(read(&h.watchdog()), "# His brief\n");

	// Missing: Jocasta's text is written; a newer one replaces it after `y` while he hasn't changed it.
	fs::remove_file(h.watchdog()).unwrap();
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	h.setup(Some("y")).await;
	assert_eq!(read(&h.watchdog()), "# Advisor brief v2\n");
	put(&dir.join("WATCHDOG.md"), "# Advisor brief v3\n");
	let newer = h.setup(Some("y")).await;
	assert!(newer.text.contains(&format!("the advisor's brief: {}", s(&h.watchdog()))));
	assert!(has_line(&newer.text, r"^- # Advisor brief v2$"));
	assert_eq!(read(&h.watchdog()), "# Advisor brief v3\n");
	assert!(
		h.undo(false)
			.contains(&other(Undo::Text { path: s(&h.watchdog()), fingerprint: fingerprint("# Advisor brief v3\n") }))
	);
}

#[tokio::test]
async fn secrets_yml_a_list_omp_reads_written_when_missing_his_own_kept_and_never_shown_jocastas_older_text_follows() {
	// omp warns at every start about a file that isn't a YAML list (`secrets/index.ts:299-301`).
	assert_eq!(yaml(&asset_text("secrets.yml")), json!([]));
	let mut h = Harness::new();
	let dir = h.own_assets();
	let mine = "- type: plain\n  content: his-name-q7z\n";
	put(&h.secrets(), mine);
	let his = h.setup(Some("y")).await;
	assert!(!his.text.contains("his-name-q7z"));
	assert!(!his.text.contains(&s(&h.secrets())));
	assert_eq!(read(&h.secrets()), mine);
	let secrets_json = serde_json::to_string(&s(&h.secrets())).unwrap();
	let undo_text = |h: &Harness| h.undo(false).iter().map(|u| u.to_json().to_string()).collect::<String>();
	assert!(!undo_text(&h).contains(&secrets_json));
	// A newer shipped text leaves his alone, unshown.
	put(&dir.join("secrets.yml"), "# v2\n[]\n");
	let newer = h.setup(Some("y")).await;
	assert!(!newer.text.contains("his-name-q7z"));
	assert_eq!(read(&h.secrets()), mine);

	// Missing: Jocasta's text is written; a newer one replaces it after `y` while he hasn't changed it.
	fs::remove_file(h.secrets()).unwrap();
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	h.setup(Some("y")).await;
	assert_eq!(read(&h.secrets()), "# v2\n[]\n");
	put(&dir.join("secrets.yml"), "# v3\n[]\n");
	let follows = h.setup(Some("y")).await;
	assert!(has_line(&follows.text, r"^- # v2$"));
	assert_eq!(read(&h.secrets()), "# v3\n[]\n");
	assert!(
		h.undo(false).contains(&other(Undo::Text { path: s(&h.secrets()), fingerprint: fingerprint("# v3\n[]\n") }))
	);
}

#[tokio::test]
async fn on_windows_the_shipped_fence_also_names_windows_own_folders_and_a_second_run_has_nothing_to_do() {
	let mut h = Harness::new();
	let shipped = asset_text("omp-config.yml");
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
async fn a_file_changed_after_the_diff_is_refused_never_overwritten_the_changes_after_it_wait() {
	let h = Harness::new();
	put(&h.omp_config(), "modelRoles:\n  default: anthropic/model-a\n");
	let mut io = TestIo::new(&[Some("y")]);
	// omp saves a setting while the diff waits for his answer.
	let config = h.omp_config();
	io.on_ask = Some(Box::new(move || put(&config, "modelRoles:\n  default: anthropic/model-b\n")));
	let code = run_setup(&h.paths, &h.deps(Some("herdr.exe")), &io).await.unwrap();
	assert_eq!(code, 1);
	let failed =
		io.out.lock().unwrap().iter().find(|l| l.starts_with("jocasta setup: 0003-omp-settings")).cloned().unwrap();
	assert!(failed.starts_with(&format!(
		"jocasta setup: 0003-omp-settings (omp settings Jocasta ships) failed: {} changed since the diff; run jocasta setup again; the ",
		s(&h.omp_config())
	)));
	assert!(Regex::new(r"; the \d+ changes after it wait\.$").unwrap().is_match(&failed));
	assert_eq!(read(&h.omp_config()), "modelRoles:\n  default: anthropic/model-b\n");
	assert!(h.marker("0003-omp-settings").is_none());
	// A file that appeared after the diff named it new is refused too.
	remove_tree(h.root.path()).unwrap();
	let mut fresh = TestIo::new(&[Some("y")]);
	let mcp = h.global_mcp();
	fresh.on_ask = Some(Box::new(move || {
		put(&mcp, "{ \"mcpServers\": { \"github\": { \"type\": \"stdio\", \"command\": \"gh-mcp\" } } }\n");
	}));
	assert_eq!(run_setup(&h.paths, &h.deps(Some("herdr.exe")), &fresh).await.unwrap(), 1);
	assert!(fresh.text().contains(&format!("{} changed since the diff; run jocasta setup again", s(&h.global_mcp()))));
	assert_eq!(read_json(&h.global_mcp())["mcpServers"], json!({"github": {"type": "stdio", "command": "gh-mcp"}}));
}

#[tokio::test]
async fn his_keybindings_yml_keeps_his_comments_and_formatting_only_jocastas_key_lines_change() {
	let h = Harness::new();
	let his =
		"# my bindings, tuned over months\napp.model.cycle: [ctrl+k] # cycle models\n\napp.live.toggle:\n  - ctrl+l\n";
	put(&h.keybindings(), his);
	// Jocasta's earlier value for a key it claimed follows the new one; here his own `ctrl+l` stays and is shown.
	let no = h.setup(Some("n")).await;
	assert!(no.text.contains("  kept: app.live.toggle: yours"));
	assert!(!no.text.contains("- # my bindings, tuned over months"));
	h.setup(Some("y")).await;
	assert_eq!(read(&h.keybindings()), format!("{his}app.stt.toggle: f9\n"));

	let map = |value: Value| -> Map<String, Value> {
		let Value::Object(map) = value else { unreachable!() };
		map
	};
	// A key line is replaced in place when its value changes; the lines around it stay.
	assert_eq!(
		edit_config(
			&h.keybindings(),
			"# mine\napp.live.toggle: [] # off\napp.stt.toggle: f9\n# end\n",
			&map(json!({"app.live.toggle": [], "app.stt.toggle": "f8"})),
			&["app.stt.toggle"],
			&[]
		)
		.as_deref(),
		Some("# mine\napp.live.toggle: [] # off\napp.stt.toggle: f8\n# end\n")
	);
	// A CRLF file keeps its line ends; lines that can't give the keys are never rewritten whole: `None`.
	assert_eq!(
		edit_config(&h.keybindings(), "a: 1\r\n", &map(json!({"a": 1, "b": 2})), &["b"], &[]).as_deref(),
		Some("a: 1\r\nb: 2\r\n")
	);
	assert_eq!(edit_config(&h.keybindings(), "{a: 1}\n", &map(json!({"a": 1, "b": 2})), &["b"], &[]), None);
	// His comments alone left read as an empty file.
	assert_eq!(edit_config(&h.keybindings(), "# mine\nb: 2\n", &Map::new(), &[], &["b"]).as_deref(), Some("# mine\n"));
}

#[tokio::test]
async fn a_keybindings_yml_whose_lines_cant_be_edited_stays_byte_for_byte_and_its_migration_fails_saying_why() {
	let h = Harness::new();
	let his = "{app.model.cycle: [ctrl+k]} # cycle models, flow style\n";
	put(&h.keybindings(), his);
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 1);
	assert!(text.contains(&format!(
		"failed: {} can't be edited line by line, so setup left it as it is; set app.live.toggle: [], app.stt.toggle: \"f9\" in it by hand, then run jocasta setup again",
		s(&h.keybindings())
	)));
	assert_eq!(read(&h.keybindings()), his);
}

#[tokio::test]
async fn a_merge_that_keeps_only_his_values_says_kept_and_config_yml_stays_byte_for_byte() {
	let mut h = Harness::new();
	let dir = h.own_assets();
	put(&dir.join("omp-config.yml"), "compaction:\n  thresholdPercent: 60\n");
	put(&h.omp_config(), "# his\ncompaction:\n  thresholdPercent: 45\n");
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains(&format!("kept {}", s(&h.omp_config()))));
	assert!(!text.contains(&format!("wrote {}", s(&h.omp_config()))));
	assert_eq!(read(&h.omp_config()), "# his\ncompaction:\n  thresholdPercent: 45\n");
}

#[tokio::test]
async fn refresh_herdr_toml_works_beside_a_malformed_lsp_json_it_doesnt_reset() {
	let h = Harness::new();
	h.setup(Some("y")).await;
	put(&h.herdr_toml(), "# his\n");
	put(&h.lsp(), "{ broken");
	let Out { code, text, .. } = h.refresh("herdr.toml", Some("y")).await.unwrap();
	assert_eq!(code, 0);
	assert!(text.contains(&format!("kept {}.bak.20261023-090507", s(&h.herdr_toml()))));
	assert_ne!(read(&h.herdr_toml()), "# his\n");
}

#[tokio::test]
async fn without_herdr_setup_ends_by_saying_so_never_done_even_with_its_integration_already_there() {
	let h = Harness::new();
	put(&h.paths.omp_agent_dir.join("extensions").join(HERDR_OMP_EXTENSION), "// herdr\n");
	let Out { text, .. } = h.setup_without_herdr(Some("y")).await;
	assert!(text.starts_with(HERDR_LINE));
	assert!(text.ends_with(HERDR_LINE));
	assert!(!text.contains("Done: run `jocasta`"));
	assert_eq!(text.split(HERDR_LINE).count(), 3);
	// Nothing else waits on herdr: the herdr config is written without it, and nothing is said done.
	assert_eq!(h.setup_without_herdr(Some("y")).await.text, "Nothing to set up: everything Jocasta adds is in place.");
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

/// A registry stand-in holding Run values.
struct Values(Mutex<Vec<(String, String)>>);

impl RunValueStore for Values {
	fn read(&self, name: &str) -> Option<String> {
		self.0.lock().unwrap().iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
	}

	fn remove(&self, name: &str) -> bool {
		let mut values = self.0.lock().unwrap();
		let before = values.len();
		values.retain(|(n, _)| n != name);
		values.len() < before
	}
}

#[tokio::test]
async fn the_companions_logon_run_value_goes_another_programs_value_of_that_name_stays() {
	let mut h = Harness::new();
	let values = Arc::new(Values(Mutex::new(vec![(
		"Jocasta".to_owned(),
		"\"C:\\Jocasta\\app\\bin\\launcher.exe\"".to_owned(),
	)])));
	h.run_value = Some(values.clone());
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains("Jocasta no longer starts at logon: its Run value goes"));
	assert!(values.0.lock().unwrap().is_empty());

	*values.0.lock().unwrap() = vec![("Jocasta".to_owned(), "\"C:\\Other\\program.exe\"".to_owned())];
	remove_tree(&markers_dir(&h.paths.jocasta_config_dir)).unwrap();
	assert!(!h.setup(Some("y")).await.text.contains("Run value"));
	assert_eq!(*values.0.lock().unwrap(), [("Jocasta".to_owned(), "\"C:\\Other\\program.exe\"".to_owned())]);
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
	let mut deps = SetupDeps::new(Some(PathBuf::from("herdr.exe")), h.run.clone());
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
	let mut deps = SetupDeps::new(Some(PathBuf::from("herdr.exe")), h.run.clone());
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

/// Runs setup on a machine with the old graph's compose file; `docker` answers as `engine_runs` says.
async fn setup_with_graph(
	h: &mut Harness,
	engine_runs: bool,
	answers: &[Option<&str>],
) -> (i32, String, Vec<String>, Vec<Vec<String>>, PathBuf) {
	let compose = h.paths.jocasta_config_dir.join("compose.yaml");
	put(&compose, "name: jocasta-demo\nservices: {}\n");
	put(&h.paths.jocasta_config_dir.join("containers").join("graphiti").join("Dockerfile"), "FROM scratch\n");
	let docker: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
	let herdr = h.run.clone();
	let seen = docker.clone();
	h.run = Arc::new(move |argv| {
		if argv[0] != "docker" {
			return herdr(argv);
		}
		let code = i32::from(argv[1] == "info" && !engine_runs);
		seen.lock().unwrap().push(argv);
		Box::pin(async move { Ok(RunOutput { code, output: String::new() }) })
	});
	let io = TestIo::new(answers);
	let code = run_setup(&h.paths, &h.deps(Some("herdr.exe")), &io).await.unwrap();
	let docker = docker.lock().unwrap().clone();
	(code, io.text(), io.asked(), docker, compose)
}

fn graph_gone(h: &Harness, compose: &Path) -> bool {
	!compose.exists() && !h.paths.jocasta_config_dir.join("containers").exists()
}

fn docker(compose: &Path, args: &[&str]) -> Vec<String> {
	[vec!["docker".to_owned(), "compose".to_owned(), "-f".to_owned(), s(compose)], strings(args)].concat()
}

#[tokio::test]
async fn an_install_with_the_old_graph_its_containers_come_down_the_images_and_volume_go_only_on_his_yes() {
	let mut h = Harness::new();
	let (code, text, asked, ran, compose) = setup_with_graph(&mut h, true, &[Some("y"), Some("n")]).await;
	assert_eq!(code, 0);
	assert_eq!(ran, [strings(&["docker", "info"]), docker(&compose, &["down"])]);
	assert_eq!(asked.last().unwrap(), "Also remove the old graph's images and volume (Graphiti and FalkorDB)? [y/N] ");
	assert!(graph_gone(&h, &compose));
	assert!(text.contains("docker compose -p jocasta-demo down --rmi all --volumes"));
}

#[tokio::test]
async fn an_install_with_the_old_graph_yes_removes_its_images_and_volume_too_with_nothing_left_to_say() {
	let mut h = Harness::new();
	let (_, text, _, ran, compose) = setup_with_graph(&mut h, true, &[Some("y"), Some("y")]).await;
	assert_eq!(
		ran,
		[
			strings(&["docker", "info"]),
			docker(&compose, &["down"]),
			docker(&compose, &["down", "--rmi", "all", "--volumes"]),
		]
	);
	assert!(graph_gone(&h, &compose));
	assert!(!text.contains("docker compose -p"));
}

#[tokio::test]
async fn an_install_with_the_old_graph_while_docker_isnt_running_no_question_the_files_go_the_later_command_is_named() {
	let mut h = Harness::new();
	let (_, text, asked, ran, compose) = setup_with_graph(&mut h, false, &[Some("y")]).await;
	assert_eq!(asked, ["Write these changes? [y/N] "]);
	assert_eq!(ran, [strings(&["docker", "info"])]);
	assert!(graph_gone(&h, &compose));
	assert!(text.contains("docker compose -p jocasta-demo down --rmi all --volumes"));
}

#[tokio::test]
async fn a_machine_that_never_had_the_old_graph_is_not_asked_about_it_and_runs_no_docker() {
	let mut h = Harness::new();
	let docker: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
	let herdr = h.run.clone();
	let seen = docker.clone();
	h.run = Arc::new(move |argv| {
		if argv[0] == "docker" {
			seen.lock().unwrap().push(argv.clone());
		}
		herdr(argv)
	});
	let Out { asked, .. } = h.setup(Some("y")).await;
	assert_eq!(asked, ["Write these changes? [y/N] "]);
	assert!(docker.lock().unwrap().is_empty());
}

fn jocasta_server() -> Value {
	json!({"type": "stdio", "command": "C:\\Jocasta\\jocasta.exe", "args": ["mcp"], "timeout": 1_860_000})
}

/// The user-level mcp.json and migration 1's marker as an install from before 0607 left them.
fn install_before_0607(h: &Harness, whiteboard: Value) {
	let github = json!({"type": "stdio", "command": "gh-mcp"});
	put(
		&h.global_mcp(),
		&pretty(&json!({"mcpServers": {"github": github, "jocasta": jocasta_server(), "whiteboard": whiteboard}})),
	);
	write_marker(
		&h.paths.jocasta_config_dir,
		"0001-mcp",
		&Marker {
			fingerprint: fingerprint(
				&json!({"jocasta": jocasta_server(), "whiteboard": h.whiteboard_server()}).to_string(),
			),
			applied_at: "2026-11-01T00:00:00.000Z".to_owned(),
			undo: vec![Undo::Mcp { path: s(&h.global_mcp()), servers: strings(&["jocasta", "whiteboard"]) }],
			shipped: None,
		},
	)
	.unwrap();
}

#[tokio::test]
async fn a_fresh_install_writes_whiteboard_into_the_bosss_mcp_json_only() {
	let h = Harness::new();
	h.setup(Some("y")).await;
	assert!(read_json(&h.global_mcp())["mcpServers"].get("whiteboard").is_none());
	assert_eq!(read_json(&h.boss_mcp())["mcpServers"]["whiteboard"], h.whiteboard_server());
	assert!(h.marker("0017-whiteboard-boss-only").is_some());
	assert!(h.setup(Some("y")).await.text.contains("Nothing to set up"));
}

#[tokio::test]
async fn migration_17_moves_whiteboard_from_the_user_level_file_to_the_bosss_on_an_old_install() {
	let h = Harness::new();
	install_before_0607(&h, h.whiteboard_server());
	let Out { code, text, .. } = h.setup(Some("y")).await;
	assert_eq!(code, 0);
	assert!(text.contains(&format!("\"whiteboard\" leaves your MCP config: {}", s(&h.global_mcp()))));
	// His other servers stay; whiteboard is in the boss's file now.
	assert_eq!(
		read_json(&h.global_mcp())["mcpServers"],
		json!({"github": {"type": "stdio", "command": "gh-mcp"}, "jocasta": jocasta_server()})
	);
	assert_eq!(read_json(&h.boss_mcp())["mcpServers"]["whiteboard"], h.whiteboard_server());
	assert_eq!(
		h.marker("0001-mcp").unwrap().undo,
		[Undo::Mcp { path: s(&h.global_mcp()), servers: strings(&["jocasta"]) }]
	);
	assert!(h.setup(Some("y")).await.text.contains("Nothing to set up"));
}

#[tokio::test]
async fn migration_17_leaves_a_whiteboard_entry_he_changed_and_names_it() {
	let h = Harness::new();
	let his = json!({"type": "stdio", "command": "C:\\Programs\\other\\node.exe", "args": ["cli.js", "mcp"]});
	install_before_0607(&h, his.clone());
	let Out { text, .. } = h.setup(Some("y")).await;
	assert!(text.contains(&format!("\"whiteboard\" in {} is yours", s(&h.global_mcp()))));
	assert_eq!(read_json(&h.global_mcp())["mcpServers"]["whiteboard"], his);
	assert_eq!(read_json(&h.boss_mcp())["mcpServers"]["whiteboard"], h.whiteboard_server());
	// Said once: the next run has nothing to say.
	assert!(h.setup(Some("y")).await.text.contains("Nothing to set up"));
}

#[test]
fn setup_undo_on_an_install_where_migration_17_never_ran_still_owns_the_whiteboard_entry_it_wrote() {
	let h = Harness::new();
	install_before_0607(&h, h.whiteboard_server());
	let undo = h.undo(false);
	let Some(SetupUndo::Mcp(global)) =
		undo.into_iter().find(|u| matches!(u, SetupUndo::Mcp(m) if m.path == s(&h.global_mcp())))
	else {
		panic!("the global mcp.json's undo");
	};
	assert_eq!(global.servers, ["jocasta", "whiteboard"]);
	assert_eq!(global.writes["whiteboard"], h.whiteboard_server());
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
async fn with_herdr_missing_setup_runs_no_herdr_config_check_and_says_herdr_is_missing() {
	let mut h = Harness::new();
	let checks = Arc::new(Mutex::new(0));
	let seen = checks.clone();
	h.check = Some(Arc::new(move |_, _| {
		*seen.lock().unwrap() += 1;
		Box::pin(async { Ok(HerdrCheck { code: Some(0), output: String::new() }) })
	}));
	let Out { text, .. } = h.setup_without_herdr(Some("y")).await;
	assert_eq!(*checks.lock().unwrap(), 0);
	assert!(text.contains("herdr isn't installed"));
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
