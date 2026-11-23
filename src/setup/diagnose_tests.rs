//! `diagnose.ts`'s tests (`test/setup/diagnose.test.ts`), on the in-process fake herdr.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::shared::install::REPO;
use crate::shared::testing::{TempDir, temp_dir};
use crate::shared::yaml::parse_value;

fn assets() -> PathBuf {
	Path::new(REPO).join("config")
}

struct Fixture {
	root: TempDir,
	herdr: Arc<FakeHerdr>,
}

impl Fixture {
	fn new() -> Self {
		Self { root: temp_dir("diagnose"), herdr: Arc::new(FakeHerdr::new()) }
	}

	fn data(&self) -> PathBuf {
		self.root.path().join("data")
	}

	fn logs(&self, files: &[(&str, usize)]) {
		let dir = self.data().join("logs");
		fs::create_dir_all(&dir).unwrap();
		for (day, count) in files {
			let lines: Vec<String> =
				(0..*count).map(|i| format!("{day}T10:00:{:02}Z WARN [service] {day} line {i}", i % 60)).collect();
			fs::write(dir.join(format!("jocasta-{day}.log")), format!("{}\n", lines.join("\n"))).unwrap();
		}
	}

	fn state(&self) -> String {
		fs::read_to_string(self.data().join("diagnose").join("state.md")).unwrap()
	}

	fn deps(&self) -> DiagnoseDeps {
		let herdr = self.herdr.clone();
		let data = self.data();
		DiagnoseDeps {
			data_dir: data.clone(),
			assets_dir: assets(),
			omp: vec!["omp".to_owned()],
			versions: "jocasta 0.1.0 (omp 18.12.6, herdr 0.9.5, bun 1.4.3)".to_owned(),
			session: "jocasta-test".to_owned(),
			service: Box::new(|| Box::pin(async { Ok(ServiceInfo { state: ServiceState::Down, file: None }) })),
			connect: Box::new(move || {
				let herdr: Arc<dyn Herdr> = herdr.clone();
				Box::pin(async move { Ok(herdr) })
			}),
			ensure_boss: Box::new(move |herdr| {
				let home = data.join("home");
				Box::pin(async move {
					let command = ["omp".to_owned(), "--continue".to_owned()];
					let id = herdr.ensure_workspace("Jocasta", &home, Some(&command), None).await?;
					Ok(herdr.panes().await?.into_iter().find(|pane| pane.workspace_id == id).unwrap())
				})
			}),
			setup_check: Box::new(|| {
				Box::pin(async {
					SetupCheck {
						waiting: Some(
							"jocasta setup has 2 changes waiting: run `jocasta setup` to see them.".to_owned(),
						),
						failed: None,
					}
				})
			}),
			now: Some(1_792_751_400_000),
		}
	}
}

fn run(deps: &DiagnoseDeps) -> (Result<i32>, Vec<String>) {
	let out = Mutex::new(Vec::new());
	let print = |line: &str| out.lock().unwrap().push(line.to_owned());
	let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
	let result = rt.block_on(run_diagnose(deps, &print));
	(result, out.into_inner().unwrap())
}

#[test]
fn recent_log_keeps_the_newest_lines_across_days_oldest_first_and_names_the_files() {
	let fixture = Fixture::new();
	fixture.logs(&[("2026-10-21", 10), ("2026-10-22", 300), ("2026-10-23", 200)]);
	let RecentLog { text, files } = recent_log(&fixture.data().join("logs"), None).unwrap();
	let lines: Vec<&str> = text.trim_end().split('\n').collect();
	assert_eq!(lines.len(), RECENT_LOG_LINES);
	assert_eq!(files, ["jocasta-2026-10-22.log", "jocasta-2026-10-23.log"]);
	assert!(lines[0].contains("2026-10-22 line 100"));
	assert!(lines.last().unwrap().contains("2026-10-23 line 199"));
	assert_eq!(
		recent_log(&fixture.root.path().join("none"), None).unwrap(),
		RecentLog { text: String::new(), files: Vec::new() }
	);
}

#[test]
fn jocasta_diagnose_opens_a_read_only_omp_pane_in_workspace_jocasta_with_the_state_and_the_log_attached_saying_they_go_to_the_model_provider()
 {
	let fixture = Fixture::new();
	fixture.logs(&[("2026-10-23", 3)]);
	let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
	rt.block_on(fixture.herdr.ensure_workspace("Jocasta", fixture.root.path(), Some(&["omp".to_owned()]), None))
		.unwrap();
	let mut deps = fixture.deps();
	deps.service = Box::new(|| {
		Box::pin(async {
			Ok(ServiceInfo {
				state: ServiceState::Busy,
				file: Some(ServiceFile { port: 47615, token: "secret".to_owned(), pid: 42 }),
			})
		})
	});
	let (code, out) = run(&deps);
	assert_eq!(code.unwrap(), 0);
	let dir = fixture.data().join("diagnose");
	let create = &fixture.herdr.of("createTab")[0];
	assert_eq!(create[0], json!(fixture.herdr.state().workspace_list[0].id));
	assert_eq!(create[1]["title"], "diagnose");
	assert_eq!(create[1]["cwd"], json!(dir.display().to_string()));
	assert_eq!(
		create[1]["command"],
		json!([
			"omp",
			"--config",
			dir.join("omp.yml").display().to_string(),
			"--tools",
			"read,grep,glob",
			format!("@{}", dir.join("state.md").display()),
			format!("@{}", dir.join("recent.log").display()),
			"Jocasta's state and recent log, attached, go to the model provider with this message. Diagnose Jocasta with the diagnose skill from them. Change nothing.",
		])
	);
	assert_eq!(fixture.herdr.of("focus").len(), 1);
	assert_eq!(
		out,
		[
			"Opened a read-only diagnose pane in workspace Jocasta (herdr session `jocasta-test`): run `jocasta` to see it."
		]
	);

	let state = fixture.state();
	assert!(state.contains("(2026-10-23T10:30:00.000Z)"));
	assert!(state.contains("- Versions: jocasta 0.1.0 (omp 18.12.6, herdr 0.9.5, bun 1.4.3)"));
	assert!(state.contains("- Service: busy (jocasta-service.json names pid 42, port 47615)"));
	assert!(state.contains("- herdr session `jocasta-test`: herdr 0.9.4 answers; 1 workspaces, 1 panes"));
	assert!(state.contains("- Setup: jocasta setup has 2 changes waiting"));
	assert!(state.contains("(from jocasta-2026-10-23.log)"));
	assert!(regex::Regex::new(r"(?m)^- Jocasta \(w_\d+\)").unwrap().is_match(&state));
	// The service's token never reaches the pane.
	assert!(!state.contains("secret"));
	assert!(fs::read_to_string(dir.join("recent.log")).unwrap().contains("2026-10-23 line 2"));
	let overlay = parse_value(&fs::read_to_string(dir.join("omp.yml")).unwrap()).unwrap();
	assert_eq!(
		overlay,
		json!({
			"plan": {"defaultOnStartup": true},
			"skills": {"customDirectories": [assets().join("skills").display().to_string()]},
		})
	);
}

#[test]
fn without_workspace_jocasta_diagnose_makes_it_first_a_herdr_that_cant_start_is_a_plain_error() {
	let fixture = Fixture::new();
	let (code, _) = run(&fixture.deps());
	code.unwrap();
	assert_eq!(fixture.herdr.of("ensureWorkspace").len(), 1);
	let labels: Vec<String> = fixture.herdr.state().workspace_list.iter().map(|w| w.label.clone()).collect();
	assert_eq!(labels, ["Jocasta"]);
	assert_eq!(fixture.herdr.of("createTab").len(), 1);

	let mut failing = fixture.deps();
	failing.connect = Box::new(|| Box::pin(async { Err(Error::other("the server exited at once")) }));
	let (code, _) = run(&failing);
	let error = code.unwrap_err();
	assert!(error.is_user());
	assert!(
		error
			.to_string()
			.starts_with("herdr session `jocasta-test` doesn't answer and couldn't start (the server exited at once)")
	);
}

#[test]
fn state_md_says_setup_couldnt_check_rather_than_nothing_waiting_and_nothing_waiting_when_that_is_so() {
	let fixture = Fixture::new();
	let mut deps = fixture.deps();
	deps.setup_check = Box::new(|| {
		Box::pin(async {
			SetupCheck {
				waiting: None,
				failed: Some("C:\\omp\\lsp.json is not JSON (bad); fix it and run setup again".to_owned()),
			}
		})
	});
	run(&deps).0.unwrap();
	assert!(
		fixture
			.state()
			.contains("- Setup: couldn't check (C:\\omp\\lsp.json is not JSON (bad); fix it and run setup again)")
	);
	deps.setup_check = Box::new(|| Box::pin(async { SetupCheck::default() }));
	run(&deps).0.unwrap();
	assert!(fixture.state().contains("- Setup: nothing waiting"));
}

/// A `SKILL.md`'s front matter, `^---\n([\s\S]*?)\n---\n`, read as YAML.
fn front_matter(skill: &str) -> Value {
	let body = skill.strip_prefix("---\n").expect("front matter");
	let end = body.find("\n---\n").expect("front matter end");
	parse_value(&body[..end]).unwrap()
}

#[test]
fn the_shipped_diagnose_skill_is_one_omp_finds_in_the_overlays_folder_name_and_description() {
	let skill = fs::read_to_string(assets().join("skills").join("diagnose").join("SKILL.md")).unwrap();
	let front = front_matter(&skill);
	assert_eq!(front["name"], "diagnose");
	assert!(front["description"].is_string());
	let mise = fs::read_to_string(assets().join("omp").join("skills").join("mise").join("SKILL.md")).unwrap();
	assert_eq!(front_matter(&mise)["name"], "mise");
	assert!(diagnose_overlay(Path::new("C:\\x\\skills")).contains("- \"C:\\\\x\\\\skills\""));
}

#[test]
fn the_phone_line_names_the_server_the_mode_and_the_main_topic_never_the_token_and_is_absent_with_the_phone_off() {
	use crate::frontier::server::phone::{TOPICS_FILE, topics};
	use crate::shared::settings::NtfySettings;
	let fixture = Fixture::new();
	let data = fixture.data();
	let ntfy = |own_server: bool, token: Option<&str>| NtfySettings {
		server: Some("https://ntfy.example.org".to_owned()),
		own_server,
		token: token.map(str::to_owned),
	};
	assert_eq!(phone_line(&NtfySettings::default(), &data), None);
	let before = phone_line(&ntfy(false, None), &data).unwrap();
	assert_eq!(
		before,
		"phone: https://ntfy.example.org, public server, the topics are made when Jocasta starts (a secret: don't paste this output)"
	);
	let made = topics(&data);
	assert!(data.join(TOPICS_FILE).exists());
	let own = phone_line(&ntfy(true, Some("tk_example")), &data).unwrap();
	assert!(own.contains("own server") && own.contains(&format!("main topic {}", made.main)));
	assert!(!own.contains("tk_example") && !own.contains(&made.reply));
	// An own server without a token is public.
	assert!(phone_line(&ntfy(true, None), &data).unwrap().contains("public server"));
}
