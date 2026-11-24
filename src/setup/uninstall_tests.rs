//! `uninstall.ts`'s tests (`test/setup/uninstall.test.ts`). Everything runs under `temp_jocasta()` and `temp_dir`.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use super::*;
use crate::frontier::server::whiteboard::WhiteboardRuntime;
use crate::setup::markers::markers_dir;
use crate::setup::setup::{CalendarStore, HerdrCheck, RunOutput, SetupDeps, SetupPaths, run_setup, setup_undo};
use crate::shared::testing::{TempDir, TempJocasta, temp_dir, temp_jocasta};

type Calls = Arc<Mutex<Vec<String>>>;

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

fn names(dir: &Path) -> Vec<String> {
	let mut found: Vec<String> =
		fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
	found.sort();
	found
}

fn strings(list: &[&str]) -> Vec<String> {
	list.iter().map(|s| (*s).to_owned()).collect()
}

/// A terminal that answers from a list (none left: end of input) and records what it was told and asked.
struct Io {
	out: Mutex<Vec<String>>,
	asked: Mutex<Vec<String>>,
	answers: Mutex<VecDeque<String>>,
}

impl Io {
	fn new(answers: &[&str]) -> Self {
		Self {
			out: Mutex::new(Vec::new()),
			asked: Mutex::new(Vec::new()),
			answers: Mutex::new(answers.iter().map(|a| (*a).to_owned()).collect()),
		}
	}
}

impl SetupIo for Io {
	fn print(&self, line: &str) {
		self.out.lock().unwrap().push(line.to_owned());
	}

	fn ask<'a>(&'a self, question: &'a str) -> BoxFuture<'a, Option<String>> {
		self.asked.lock().unwrap().push(question.to_owned());
		let answer = self.answers.lock().unwrap().pop_front();
		Box::pin(async move { answer })
	}
}

struct Out {
	code: i32,
	text: String,
	asked: Vec<String>,
}

struct Calendar {
	stored: AtomicBool,
}

impl CalendarStore for Calendar {
	fn stored(&self) -> bool {
		self.stored.load(Ordering::SeqCst)
	}

	fn save(&self, _secret: &str) -> Result<()> {
		self.stored.store(true, Ordering::SeqCst);
		Ok(())
	}

	fn env_secret(&self) -> Option<String> {
		Some("https://calendar.example.org/secret.ics".to_owned())
	}
}

/// What one test works on: a machine set up by `jocasta setup`.
struct Machine {
	_j: TempJocasta,
	_dir: TempDir,
	root: PathBuf,
	setup: SetupPaths,
	calls: Calls,
}

/// His own `keybindings.yml` from before Jocasta, with CRLF line ends and a comment.
const HIS_KEYS: &str = "# mine\r\napp.model.cycle: [ctrl+k] # cycle\r\n";

/// `jocasta setup`, answering `y`, with herdr's config check standing in.
async fn setup_run(paths: &SetupPaths, mise: Option<&str>, calendar: bool) {
	let mut deps = SetupDeps::new(PathBuf::from("herdr.exe"));
	deps.mise = mise.map(PathBuf::from);
	deps.check_herdr_config =
		Some(Arc::new(|_, _| Box::pin(async { Ok(HerdrCheck { code: Some(0), output: "config: ok\n".to_owned() }) })));
	if calendar {
		deps.calendar = Some(Arc::new(Calendar { stored: AtomicBool::new(false) }));
	}
	run_setup(
		paths,
		&deps,
		&Io::new(&["y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y", "y"]),
	)
	.await
	.unwrap();
}

impl Machine {
	async fn new() -> Self {
		let j = temp_jocasta();
		let dir = temp_dir("uninstall");
		let root = dir.path().to_path_buf();
		let setup = SetupPaths {
			omp_agent_dir: root.join("omp").join("agent"),
			jocasta_config_dir: root.join("jocasta").join("agent"),
			data_dir: root.join("local").join("jocasta").join("data"),
			assets_dir: Path::new(crate::shared::install::REPO).join("config"),
			terminal_fragments_dir: Some(root.join("Fragments")),
			system_folders: None,
			mise_config_dir: root.join("mise"),
			jocasta_command: vec![PathBuf::from("C:\\Jocasta\\jocasta.exe")],
			whiteboard: Some(WhiteboardRuntime {
				node: PathBuf::from("C:\\Jocasta\\node\\node.exe"),
				cli: PathBuf::from("C:\\Jocasta\\whiteboard\\cli.js"),
			}),
		};
		// A machine set up by `jocasta setup` over his own omp `mcp.json` and `keybindings.yml`, with the calendar's
		// secret stored.
		put(
			&setup.omp_agent_dir.join("mcp.json"),
			&json!({ "mcpServers": { "github": { "command": "gh-mcp" } } }).to_string(),
		);
		put(&setup.omp_agent_dir.join("keybindings.yml"), HIS_KEYS);
		setup_run(&setup, Some("mise.exe"), true).await;
		put(&setup.data_dir.join("catalog.db"), "db");
		put(&setup.mise_config_dir.join("config.jocasta-p1.override.toml"), "[tools]\n");
		put(&setup.mise_config_dir.join("config.toml"), "# his\n");
		Self { _j: j, _dir: dir, root, setup, calls: Arc::new(Mutex::new(Vec::new())) }
	}

	fn calls(&self) -> Vec<String> {
		self.calls.lock().unwrap().clone()
	}

	fn install(&self) -> InstallRecord {
		let dir = self.root.join("Programs").join("jocasta");
		put(&dir.join("jocasta.exe"), "exe");
		put(&dir.join("install.json"), &json!({ "dir": dir.to_string_lossy() }).to_string());
		InstallRecord {
			dir: dir.clone(),
			path: Some(dir),
			autostart: Some("Jocasta".to_owned()),
			app_data: Some(self.root.join("local").join("local.jocasta.frontier")),
			..InstallRecord::default()
		}
	}

	fn paths(&self, record: Option<InstallRecord>, running_from_install: bool) -> UninstallPaths {
		UninstallPaths {
			jocasta_config_dir: self.setup.jocasta_config_dir.clone(),
			data_dir: self.setup.data_dir.clone(),
			mise_config_dir: self.setup.mise_config_dir.clone(),
			install: record,
			running_from_install,
		}
	}

	fn deps(&self, state: ServiceState) -> UninstallDeps {
		let run_calls = self.calls.clone();
		let run: RunProgram = Arc::new(move |argv| {
			run_calls.lock().unwrap().push(argv.join(" "));
			Box::pin(async { Ok(RunOutput { code: 0, output: String::new() }) })
		});
		let (path_calls, run_value_calls, credential_calls, exit_calls) =
			(self.calls.clone(), self.calls.clone(), self.calls.clone(), self.calls.clone());
		UninstallDeps {
			service_state: Arc::new(move || Box::pin(async move { Ok(state) })),
			herdr: PathBuf::from("herdr.exe"),
			session: "jocasta-test".to_owned(),
			run,
			setup_undo: setup_undo(&self.setup, true),
			remove_path_entry: Arc::new(move |entry| {
				path_calls.lock().unwrap().push(format!("path {}", entry.display()));
				Ok(true)
			}),
			remove_run_value: Arc::new(move |name, launcher| {
				run_value_calls.lock().unwrap().push(format!("run {name} {}", launcher.display()));
				Ok(true)
			}),
			delete_credential: Arc::new(move |target| {
				credential_calls.lock().unwrap().push(format!("credential {target}"));
				true
			}),
			remove_after_exit: Arc::new(move |dir, entries| {
				exit_calls.lock().unwrap().push(format!("after exit {} {}", dir.display(), entries.join(",")));
				Ok(())
			}),
			processes_under: None,
		}
	}

	async fn uninstall(&self, answers: &[&str], paths: &UninstallPaths, deps: &UninstallDeps) -> Result<Out> {
		let io = Io::new(answers);
		let code = run_uninstall(paths, deps, &io).await?;
		let text = io.out.lock().unwrap().join("\n");
		let asked = io.asked.lock().unwrap().clone();
		Ok(Out { code, text, asked })
	}

	/// `uninstall(answers)` with the default paths and deps.
	async fn run(&self, answers: &[&str]) -> Out {
		self.uninstall(answers, &self.paths(None, false), &self.deps(ServiceState::Down)).await.unwrap()
	}
}

fn run_with(output: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static, calls: Calls) -> RunProgram {
	Arc::new(move |argv| {
		calls.lock().unwrap().push(argv.join(" "));
		let (code, output) = output(&argv);
		Box::pin(async move { Ok(RunOutput { code, output }) })
	})
}

#[tokio::test]
async fn a_running_jocasta_is_refused_and_n_removes_nothing() {
	let m = Machine::new().await;
	let refused = m.uninstall(&["y"], &m.paths(None, false), &m.deps(ServiceState::Up)).await;
	assert!(refused.err().is_some_and(|error| error.is_user()));
	let Out { code, text, asked } = m.run(&["n"]).await;
	assert_eq!(code, 0);
	assert_eq!(asked, ["Remove these? [y/N] "]);
	let agent = &m.setup.omp_agent_dir;
	assert!(text.contains("herdr session `jocasta-test` (stopped, its panes closed, then deleted)"));
	assert!(text.contains(&format!(
		"{}: yours from before Jocasta's omp snapshot back (while it is the snapshot's)",
		agent.join("keybindings.yml").display()
	)));
	assert!(text.contains(&format!("{} (while it is still the snapshot's)", agent.join("config.yml").display())));
	assert!(text.contains(&format!(
		"Jocasta's mise config for a project: {}",
		m.setup.mise_config_dir.join("config.jocasta-p1.override.toml").display()
	)));
	assert!(text.contains("Windows Credential Manager: jocasta/calendar-ical"));
	assert!(text.ends_with("Nothing removed."));
	assert!(m.calls().is_empty());
	assert!(m.setup.jocasta_config_dir.join("herdr.toml").exists());
}

#[tokio::test]
async fn y_removes_what_install_and_setup_added_and_keeps_his_own_the_data_folder_only_on_its_own_y() {
	let m = Machine::new().await;
	let record = m.install();
	let app_data = record.app_data.clone().unwrap();
	put(&app_data.join("stable").join("WebView2").join("x"), "x");
	let Out { code, text, asked } = m.uninstall_with(&["y", "n"], &record).await;
	assert_eq!(code, 0);
	assert_eq!(
		asked,
		[
			"Remove these? [y/N] ".to_owned(),
			format!("Also remove Jocasta's data folder {} (history, models, logs)? [y/N] ", m.setup.data_dir.display()),
		]
	);
	assert!(text.ends_with("Jocasta is uninstalled."));
	assert_eq!(
		m.calls(),
		[
			"herdr.exe session stop jocasta-test".to_owned(),
			"herdr.exe session delete jocasta-test".to_owned(),
			"credential jocasta/calendar-ical".to_owned(),
			format!("run Jocasta {}", record.dir.join("app").join("bin").join("launcher.exe").display()),
			format!("path {}", record.dir.display()),
		]
	);
	// His own mcp.json and keybindings are back byte for byte and his mise config stays; Jocasta's are gone.
	let agent = &m.setup.omp_agent_dir;
	assert_eq!(
		read(&agent.join("mcp.json")),
		json!({ "mcpServers": { "github": { "command": "gh-mcp" } } }).to_string()
	);
	assert_eq!(read(&agent.join("keybindings.yml")), HIS_KEYS);
	assert!(m.setup.mise_config_dir.join("config.toml").exists());
	let gone = [
		m.setup.data_dir.join("home").join(".omp").join("mcp.json"),
		agent.join("skills").join("mise"),
		m.setup.mise_config_dir.join("conf.d").join("jocasta.toml"),
		// The snapshot's files that had nothing of his before them.
		agent.join("config.yml"),
		agent.join("WATCHDOG.md"),
		agent.join("lsp.json"),
		agent.join("secrets.yml"),
		agent.join("extensions"),
		m.setup.mise_config_dir.join("config.jocasta-p1.override.toml"),
		m.root.join("Fragments").join("jocasta"),
		m.setup.jocasta_config_dir.clone(),
		app_data,
		record.dir.clone(),
	];
	for path in gone {
		assert!(!path.exists(), "{} should be gone", path.display());
	}
	assert!(m.setup.data_dir.join("catalog.db").exists());
}

impl Machine {
	/// `uninstall(answers, uninstallPaths(record))` with the default deps.
	async fn uninstall_with(&self, answers: &[&str], record: &InstallRecord) -> Out {
		self.uninstall(answers, &self.paths(Some(record.clone()), false), &self.deps(ServiceState::Down)).await.unwrap()
	}
}

#[tokio::test]
async fn a_snapshot_file_he_changed_since_stays_with_his_file_from_before_jocasta_beside_it() {
	let m = Machine::new().await;
	let agent = m.setup.omp_agent_dir.clone();
	let keys = agent.join("keybindings.yml");
	put(&keys, "# his changed keys\n");
	put(&agent.join("WATCHDOG.md"), "# His brief\n");
	let Out { code, text, .. } = m.run(&["y", "n"]).await;
	assert_eq!(code, 0);
	let before = agent.join("keybindings.yml.before-jocasta");
	assert!(text.contains(&format!(
		"kept {}: changed since setup; yours from before is {}",
		keys.display(),
		before.display()
	)));
	assert!(text.contains(&format!("kept {}: changed since setup", agent.join("WATCHDOG.md").display())));
	assert_eq!(read(&keys), "# his changed keys\n");
	assert_eq!(read(&before), HIS_KEYS);
	assert_eq!(read(&agent.join("WATCHDOG.md")), "# His brief\n");
	assert!(!agent.join("WATCHDOG.md.before-jocasta").exists());
	// The snapshot's files he left alone are gone.
	assert!(!agent.join("config.yml").exists());
}

#[tokio::test]
async fn the_data_folder_goes_on_its_own_y_and_a_running_installs_folder_goes_after_exit() {
	let m = Machine::new().await;
	let record = m.install();
	let Out { text, .. } =
		m.uninstall(&["y", "y"], &m.paths(Some(record.clone()), true), &m.deps(ServiceState::Down)).await.unwrap();
	assert!(!m.setup.data_dir.exists());
	// `<LOCALAPPDATA>\jocasta` held only the data folder.
	assert!(!m.setup.data_dir.parent().unwrap().exists());
	assert_eq!(m.calls().last().unwrap(), &format!("after exit {} jocasta.exe", record.dir.display()));
	assert!(record.dir.exists());
	assert!(text.contains(&format!("{} goes when this command exits.", record.dir.display())));
}

#[tokio::test]
async fn a_step_that_fails_is_said_and_the_rest_still_go_the_exit_code_says_so() {
	let m = Machine::new().await;
	let mut failing = m.deps(ServiceState::Down);
	failing.run = run_with(
		|argv| {
			if argv.iter().any(|word| word == "delete") {
				(1, "session is running\n".to_owned())
			} else {
				(0, String::new())
			}
		},
		m.calls.clone(),
	);
	let Out { code, text, .. } = m.uninstall(&["y", "n"], &m.paths(None, false), &failing).await.unwrap();
	assert_eq!(code, 1);
	assert!(text.contains("couldn't remove herdr session `jocasta-test`"));
	assert!(text.contains("herdr session delete jocasta-test exited 1: session is running"));
	assert!(text.ends_with("Done, except 1 thing above; remove it by hand."));
	assert!(!m.setup.jocasta_config_dir.exists());
}

#[tokio::test]
async fn without_setups_markers_nothing_outside_jocastas_own_folders_is_listed_or_touched() {
	let m = Machine::new().await;
	fs::remove_dir_all(markers_dir(&m.setup.jocasta_config_dir)).unwrap();
	let mcp = m.setup.omp_agent_dir.join("mcp.json");
	let his = read(&mcp);
	let listed: Vec<String> =
		m.run(&["n"]).await.text.split('\n').filter(|line| line.starts_with("- ")).map(str::to_owned).collect();
	assert_eq!(
		listed,
		[
			"- herdr session `jocasta-test` (stopped, its panes closed, then deleted)".to_owned(),
			format!(
				"- Jocasta's mise config for a project: {}",
				m.setup.mise_config_dir.join("config.jocasta-p1.override.toml").display()
			),
			format!("- Jocasta's config folder: {}", m.setup.jocasta_config_dir.display()),
		]
	);
	let Out { code, .. } = m.run(&["y", "n"]).await;
	assert_eq!(code, 0);
	assert_eq!(m.calls(), ["herdr.exe session stop jocasta-test", "herdr.exe session delete jocasta-test"]);
	assert_eq!(read(&mcp), his);
	let agent = &m.setup.omp_agent_dir;
	for kept in [
		agent.join("extensions").join(herdr::embed::OMP_HOOK_FILE),
		agent.join("keybindings.yml"),
		agent.join("skills").join("mise").join("SKILL.md"),
		m.setup.mise_config_dir.join("conf.d").join("jocasta.toml"),
		m.root.join("Fragments").join("jocasta").join("jocasta.json"),
		m.setup.data_dir.join("home").join(".omp").join("mcp.json"),
	] {
		assert!(kept.exists(), "{} should stay", kept.display());
	}
}

#[tokio::test]
async fn an_mcp_entry_that_isnt_what_setup_wrote_is_kept_and_said() {
	let m = Machine::new().await;
	let boss = m.setup.data_dir.join("home").join(".omp").join("mcp.json");
	let mut config = read_json(&boss);
	config["mcpServers"]["whiteboard"]["args"] = json!(["mcp", "--his"]);
	put(&boss, &config.to_string());
	let Out { code, text, .. } = m.run(&["y", "n"]).await;
	assert_eq!(code, 0);
	let shown = boss.display();
	assert!(
		text.contains(&format!("- MCP server \"whiteboard\" in {shown} (each while it is still what setup wrote)"))
	);
	assert!(text.contains(&format!(
		"{shown} has none of Jocasta's MCP servers left; kept \"whiteboard\" (not what setup wrote)"
	)));
	assert_eq!(read_json(&boss), config);
}

#[tokio::test]
async fn an_mcp_entry_still_as_its_marker_recorded_goes_after_jocasta_moved() {
	let mut m = Machine::new().await;
	// Whiteboard moved since setup ran: the boss's entry differs from today's but still is what setup wrote.
	m.setup.whiteboard = Some(WhiteboardRuntime {
		node: PathBuf::from("D:\\Moved\\node.exe"),
		cli: PathBuf::from("D:\\Moved\\whiteboard\\cli.js"),
	});
	let boss = m.setup.data_dir.join("home").join(".omp").join("mcp.json");
	let Out { code, text, .. } = m.run(&["y", "n"]).await;
	assert_eq!(code, 0);
	assert!(text.contains(&format!("removed {}", boss.display())));
	assert!(!boss.exists());
}

#[tokio::test]
async fn his_private_names_list_stays_in_jocastas_config_folder_everything_else_there_goes() {
	let m = Machine::new().await;
	let list = m.setup.jocasta_config_dir.join("private-names.txt");
	put(&list, "example-pattern\n");
	let Out { code, text, .. } = m.run(&["y", "n"]).await;
	assert_eq!(code, 0);
	assert!(text.contains(&format!("(kept: {}, yours, not Jocasta's)", list.display())));
	assert_eq!(names(&m.setup.jocasta_config_dir), ["private-names.txt"]);
	assert_eq!(read(&list), "example-pattern\n");
}

#[tokio::test]
async fn install_json_todays_names_what_the_installer_added_an_older_windows_one_gets_todays() {
	let m = Machine::new().await;
	let dir = m.root.join("inst");
	put(&dir.join("install.json"), &json!({ "dir": dir.to_string_lossy() }).to_string());
	let local = Path::new("C:\\L");
	assert_eq!(
		read_install_record(&dir, Some(local), Some(true)),
		Some(InstallRecord {
			dir: dir.clone(),
			path: Some(dir.clone()),
			autostart: Some("Jocasta".to_owned()),
			app_data: Some(local.join("local.jocasta.frontier")),
			..InstallRecord::default()
		})
	);
	assert_eq!(
		read_install_record(&dir, None, Some(false)),
		Some(InstallRecord { dir: dir.clone(), ..InstallRecord::default() })
	);
	put(&dir.join("install.json"), &json!({ "dir": dir.to_string_lossy(), "rc": "/home/u/.bashrc" }).to_string());
	assert_eq!(
		read_install_record(&dir, None, Some(false)),
		Some(InstallRecord {
			dir: dir.clone(),
			rc: Some(PathBuf::from("/home/u/.bashrc")),
			..InstallRecord::default()
		})
	);
	assert_eq!(read_install_record(&m.root.join("none"), None, None), None);
}

#[tokio::test]
async fn install_json_is_trusted_only_for_its_own_folder_and_the_window_data_only_directly_in_localappdata() {
	let m = Machine::new().await;
	let dir = m.root.join("inst");
	put(&dir.join("install.json"), &json!({ "dir": m.root.join("elsewhere").to_string_lossy() }).to_string());
	assert_eq!(read_install_record(&dir, Some(Path::new("C:\\L")), Some(true)), None);
	let local = m.root.join("local");
	let only_dir = Some(InstallRecord { dir: dir.clone(), ..InstallRecord::default() });
	put(
		&dir.join("install.json"),
		&json!({ "dir": dir.to_string_lossy(), "appData": m.root.join("Documents").to_string_lossy() }).to_string(),
	);
	assert_eq!(read_install_record(&dir, Some(&local), Some(true)), only_dir);
	let window = local.join("local.jocasta.frontier");
	put(
		&dir.join("install.json"),
		&json!({ "dir": dir.to_string_lossy(), "appData": window.to_string_lossy() }).to_string(),
	);
	assert_eq!(
		read_install_record(&dir, Some(&local), Some(true)),
		Some(InstallRecord { dir: dir.clone(), app_data: Some(window), ..InstallRecord::default() })
	);
}

#[tokio::test]
async fn install_jsons_entries_are_kept_as_plain_names_only_a_list_with_a_path_in_it_is_no_list() {
	let m = Machine::new().await;
	let dir = m.root.join("inst");
	let rc = |rc: &str| Some(PathBuf::from(rc));
	put(
		&dir.join("install.json"),
		&json!({ "dir": dir.to_string_lossy(), "entries": ["jocasta", "config"], "rc": "/home/u/.bashrc" }).to_string(),
	);
	assert_eq!(
		read_install_record(&dir, None, Some(false)),
		Some(InstallRecord {
			dir: dir.clone(),
			entries: Some(strings(&["jocasta", "config"])),
			rc: rc("/home/u/.bashrc"),
			..InstallRecord::default()
		})
	);
	for bad in ["..", "../x", "a/b", "C:\\x", ""] {
		put(
			&dir.join("install.json"),
			&json!({ "dir": dir.to_string_lossy(), "entries": ["config", bad], "rc": "/r" }).to_string(),
		);
		assert_eq!(
			read_install_record(&dir, None, Some(false)),
			Some(InstallRecord { dir: dir.clone(), rc: rc("/r"), ..InstallRecord::default() })
		);
	}
}

#[tokio::test]
async fn uninstall_removes_only_the_installs_listed_entries_a_folder_holding_his_files_stays_with_them() {
	let m = Machine::new().await;
	let dir = m.root.join("bin");
	for name in [
		"jocasta",
		"config/boss.md",
		"node/node",
		"his-tool",
		"notes/his.txt",
		"jocasta.old-20261109",
		"conpty/conpty.dll",
	] {
		put(&dir.join(name), "x");
	}
	// A pane's ConPTY host an upgrade moved out of `conpty\` (0661), and a file of his that only looks like one.
	put(&dir.join("conpty-x64-OpenConsole.exe.old-20261123"), "x");
	put(&dir.join("conpty-notes.txt"), "x");
	put(
		&dir.join("install.json"),
		&json!({ "dir": dir.to_string_lossy(), "entries": ["jocasta", "config", "node", "conpty"] }).to_string(),
	);
	let record = read_install_record(&dir, None, Some(false)).unwrap();
	assert_eq!(
		install_contents(&record, Some(false)).unwrap(),
		InstallContents {
			ours: strings(&[
				"config",
				"conpty",
				"conpty-x64-OpenConsole.exe.old-20261123",
				"jocasta",
				"jocasta.old-20261109",
				"node"
			]),
			others: strings(&["conpty-notes.txt", "his-tool", "notes"]),
		}
	);
	let Out { code, text, .. } = m.uninstall_with(&["y", "n"], &record).await;
	assert_eq!(code, 0);
	assert!(
		text.contains(&format!(
			"removed Jocasta's files from {}; kept the folder (it holds other files)",
			dir.display()
		))
	);
	assert_eq!(names(&dir), ["conpty-notes.txt", "his-tool", "notes"]);

	// Only Jocasta's entries: the folder goes with them.
	let own = m.root.join("own");
	put(&own.join("jocasta"), "x");
	put(
		&own.join("install.json"),
		&json!({ "dir": own.to_string_lossy(), "entries": ["jocasta", "hunk"] }).to_string(),
	);
	let again = m.uninstall_with(&["y", "n"], &read_install_record(&own, None, Some(false)).unwrap()).await;
	assert!(again.text.contains(&format!("removed {}", own.display())));
	assert!(!own.exists());
}

#[tokio::test]
async fn an_install_from_before_the_list_goes_whole_only_when_every_entry_is_a_releases_else_it_all_stays() {
	let m = Machine::new().await;
	let record = m.install();
	put(&record.dir.join("his-tool.exe"), "his");
	let Out { code, text, .. } = m.uninstall_with(&["y", "n"], &record).await;
	assert_eq!(code, 1);
	let dir = record.dir.display();
	assert!(text.contains(&format!(
		"couldn't remove {dir}: kept {dir}: it holds files that aren't Jocasta's (his-tool.exe); remove Jocasta's files from it by hand"
	)));
	assert_eq!(names(&record.dir), ["his-tool.exe", "install.json", "jocasta.exe"]);
}

#[tokio::test]
async fn omp_sessions_running_programs_from_the_install_are_named_and_refused_before_anything_is_asked() {
	let m = Machine::new().await;
	let record = m.install();
	let mut d = m.deps(ServiceState::Down);
	d.processes_under = Some(Arc::new(|dir| {
		let dir = dir.to_string_lossy();
		vec![
			InstallProcess {
				pid: 4120,
				path: format!("{dir}\\jocasta.exe"),
				command_line: format!("\"{dir}\\jocasta.exe\" mcp"),
			},
			InstallProcess {
				pid: 5332,
				path: format!("{dir}\\node\\node.exe"),
				command_line: format!("\"{dir}\\node\\node.exe\" \"{dir}\\whiteboard\\dist\\cli.js\" mcp"),
			},
		]
	}));
	let refused = m.uninstall(&["y"], &m.paths(Some(record.clone()), false), &d).await;
	assert_eq!(
		refused.err().unwrap().to_string(),
		format!(
			"omp sessions still run programs from {}: jocasta.exe mcp (pid 4120), node.exe whiteboard mcp (pid 5332). Close them, then run jocasta uninstall again.",
			record.dir.display()
		)
	);
	assert!(m.calls().is_empty());
	assert!(record.dir.join("jocasta.exe").exists());
}

#[test]
fn describe_process_names_the_program_and_its_words_a_path_inside_the_install_as_its_entry() {
	let dir = "C:\\Users\\x\\AppData\\Local\\Programs\\jocasta";
	let hunk = InstallProcess {
		pid: 7,
		path: format!("{dir}\\hunk\\hunk.exe"),
		command_line: "hunk.exe diff --watch".to_owned(),
	};
	assert_eq!(describe_process(dir, &hunk), "hunk.exe diff --watch (pid 7)");
	let jocasta = InstallProcess {
		pid: 8,
		path: format!("{dir}\\jocasta.exe"),
		command_line: format!("{dir}\\jocasta.exe mcp --boss C:\\elsewhere\\x.js"),
	};
	assert_eq!(describe_process(dir, &jocasta), "jocasta.exe mcp --boss C:\\elsewhere\\x.js (pid 8)");
}

#[cfg(windows)]
#[tokio::test]
async fn remove_after_exit_removes_the_listed_entries_once_the_process_has_exited_then_install_json_and_the_folder() {
	let m = Machine::new().await;
	let dir = m.root.join("Programs").join("jocasta");
	put(&dir.join("jocasta.exe"), "exe");
	put(&dir.join("config").join("boss.md"), "x");
	put(&dir.join("install.json"), &json!({ "dir": dir.to_string_lossy() }).to_string());
	let mut waiter = std::process::Command::new("ping")
		.args(["-n", "2", "127.0.0.1"])
		.stdout(std::process::Stdio::null())
		.spawn()
		.unwrap();
	remove_after_exit(&dir, &strings(&["jocasta.exe", "config"]), Some(waiter.id())).unwrap();
	waiter.wait().unwrap();
	// The remover is a detached PowerShell with nothing to await: its effect on the folder is the signal.
	for _ in 0..80 {
		if !dir.exists() {
			break;
		}
		tokio::time::sleep(std::time::Duration::from_millis(250)).await;
	}
	assert!(!dir.exists());
}

#[tokio::test]
async fn a_recorded_folder_that_no_longer_holds_jocastas_marker_is_kept_and_said() {
	let m = Machine::new().await;
	let record = m.install();
	fs::remove_file(record.dir.join("install.json")).unwrap();
	// Window data without WebView2's folder in it.
	let app_data = record.app_data.clone().unwrap();
	put(&app_data.join("his.txt"), "his");
	let Out { code, text, .. } = m.uninstall_with(&["y", "n"], &record).await;
	assert_eq!(code, 1);
	let dir = record.dir.display();
	assert!(text.contains(&format!("couldn't remove {dir}: kept {dir}: it no longer holds install.json naming it")));
	assert!(text.contains(&format!("kept {}: it no longer holds the companion's WebView2 data", app_data.display())));
	assert!(record.dir.join("jocasta.exe").exists());
	assert!(app_data.join("his.txt").exists());

	// Run from such an install, its folder isn't handed over for removal either.
	m.calls.lock().unwrap().clear();
	let again =
		m.uninstall(&["y", "n"], &m.paths(Some(record.clone()), true), &m.deps(ServiceState::Down)).await.unwrap();
	assert_eq!(again.code, 1);
	assert!(!m.calls().iter().any(|call| call.starts_with("after exit")));
}

#[tokio::test]
async fn a_drive_root_or_his_home_folder_is_never_removed_whole() {
	let m = Machine::new().await;
	let home = std::env::home_dir().unwrap();
	assert_eq!(unsafe_folder(&std::path::absolute(Path::new("/")).unwrap()), Some("it is a drive root"));
	let holds_home = Some("it is your home folder or holds it");
	assert_eq!(unsafe_folder(&home), holds_home);
	assert_eq!(unsafe_folder(home.parent().unwrap()), holds_home);
	assert_eq!(unsafe_folder(&home.join(".jocasta").join("agent")), None);
	assert_eq!(unsafe_folder(&m.root), None);
}

#[tokio::test]
async fn a_folder_setup_didnt_make_stays_only_setups_file_in_it_goes() {
	let m = Machine::new().await;
	// Fresh setup on a machine where his skills/mise folder already held his own file.
	fs::remove_dir_all(&m.setup.jocasta_config_dir).unwrap();
	let skill = m.setup.omp_agent_dir.join("skills").join("mise");
	fs::remove_dir_all(&skill).unwrap();
	put(&skill.join("notes.md"), "his");
	setup_run(&m.setup, Some("mise.exe"), false).await;
	let undo = setup_undo(&m.setup, false);
	let file = skill.join("SKILL.md").to_string_lossy().into_owned();
	assert!(
		undo.iter().any(|u| matches!(u, SetupUndo::Other(Undo::Restore { path, backup: None, .. }) if *path == file))
	);
	let skill_text = skill.to_string_lossy();
	assert!(!undo.iter().any(|u| matches!(u, SetupUndo::Other(Undo::Dir { path, .. }) if *path == skill_text)));
	let mut d = m.deps(ServiceState::Down);
	d.setup_undo = undo;
	let Out { code, .. } = m.uninstall(&["y", "n"], &m.paths(None, false), &d).await.unwrap();
	assert_eq!(code, 0);
	assert!(!skill.join("SKILL.md").exists());
	assert_eq!(read(&skill.join("notes.md")), "his");
}

#[tokio::test]
async fn a_folder_setup_recorded_before_0484_goes_only_when_empty_one_it_made_goes_only_with_its_file_in_it() {
	let m = Machine::new().await;
	let legacy = m.root.join("legacy");
	put(&legacy.join("his.txt"), "his");
	let made = m.root.join("made");
	put(&made.join("other.txt"), "x");
	let mut d = m.deps(ServiceState::Down);
	d.setup_undo = vec![
		SetupUndo::Other(Undo::Dir { path: legacy.to_string_lossy().into_owned(), file: None }),
		SetupUndo::Other(Undo::Dir {
			path: made.to_string_lossy().into_owned(),
			file: Some("jocasta.json".to_owned()),
		}),
	];
	let Out { code, text, .. } = m.uninstall(&["y", "n"], &m.paths(None, false), &d).await.unwrap();
	assert_eq!(code, 1);
	assert!(text.contains(&format!("kept {}: setup may not have made it, and it isn't empty", legacy.display())));
	assert!(text.contains(&format!("kept {}: it no longer holds jocasta.json", made.display())));
	assert!(legacy.join("his.txt").exists());
	assert!(made.join("other.txt").exists());
}

#[tokio::test]
async fn remove_rc_line_takes_install_shs_path_line_out_of_the_rc_file_and_nothing_else() {
	let m = Machine::new().await;
	let rc = m.root.join(".bashrc");
	put(&rc, &format!("alias ll='ls -l'\n\n{}\n", rc_path_line(Path::new("/opt/jocasta"))));
	assert!(remove_rc_line(&rc, Path::new("/opt/jocasta")).unwrap());
	assert_eq!(read(&rc), "alias ll='ls -l'\n");
	assert!(!remove_rc_line(&rc, Path::new("/opt/jocasta")).unwrap());
}

#[test]
fn base64_pads_like_node() {
	assert_eq!(base64(b""), "");
	assert_eq!(base64(b"f"), "Zg==");
	assert_eq!(base64(b"fo"), "Zm8=");
	assert_eq!(base64(b"foo"), "Zm9v");
}
