//! `diagnose.ts`'s tests (`test/setup/diagnose.test.ts`), with omp's run recorded.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::shared::errors::Error;
use crate::shared::install::REPO;
use crate::shared::testing::{TempDir, temp_dir, temp_jocasta};
use crate::shared::yaml::parse_value;

fn assets() -> PathBuf {
	Path::new(REPO).join("config")
}

/// What omp was asked to run: its argv and folder.
type Runs = Arc<Mutex<Vec<(Vec<String>, PathBuf)>>>;

struct Fixture {
	root: TempDir,
	runs: Runs,
}

impl Fixture {
	fn new() -> Self {
		Self { root: temp_dir("diagnose"), runs: Runs::default() }
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
		let runs = self.runs.clone();
		DiagnoseDeps {
			data_dir: self.data(),
			assets_dir: assets(),
			omp: vec!["omp".to_owned()],
			versions: "jocasta 0.1.0 (omp 18.12.6, bun 1.4.3)".to_owned(),
			service: Box::new(|| Box::pin(async { Ok(ServiceInfo { state: ServiceState::Down, file: None }) })),
			run_omp: Box::new(move |argv, dir| {
				runs.lock().unwrap().push((argv, dir));
				Box::pin(async { Ok(0) })
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
fn jocasta_diagnose_writes_the_state_and_the_log_and_runs_a_read_only_omp_in_the_diagnose_folder_saying_they_go_to_the_model_provider()
 {
	let _j = temp_jocasta();
	let fixture = Fixture::new();
	fixture.logs(&[("2026-10-23", 3)]);
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
	let runs = fixture.runs.lock().unwrap().clone();
	assert_eq!(runs.len(), 1);
	assert_eq!(runs[0].1, dir);
	assert_eq!(
		runs[0].0,
		[
			"omp".to_owned(),
			"--config".to_owned(),
			dir.join("omp.yml").display().to_string(),
			"--tools".to_owned(),
			"read,grep,glob".to_owned(),
			format!("@{}", dir.join("state.md").display()),
			format!("@{}", dir.join("recent.log").display()),
			"Jocasta's state and recent log, attached, go to the model provider with this message. Diagnose Jocasta with the diagnose skill from them. Change nothing.".to_owned(),
		]
	);
	// No phone configured: nothing printed.
	assert!(out.is_empty(), "{out:?}");

	let state = fixture.state();
	assert!(state.contains("(2026-10-23T10:30:00.000Z)"));
	assert!(state.contains("- Versions: jocasta 0.1.0 (omp 18.12.6, bun 1.4.3)"));
	assert!(state.contains("- Service: busy (jocasta-service.json names pid 42, port 47615)"));
	assert!(state.contains("- Setup: jocasta setup has 2 changes waiting"));
	assert!(state.contains("(from jocasta-2026-10-23.log)"));
	// The service's token never reaches the model.
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
fn diagnose_resolves_omps_exit_code_and_a_failed_run_is_its_error() {
	let fixture = Fixture::new();
	let mut deps = fixture.deps();
	deps.run_omp = Box::new(|_, _| Box::pin(async { Ok(3) }));
	assert_eq!(run(&deps).0.unwrap(), 3);
	deps.run_omp = Box::new(|_, _| Box::pin(async { Err(Error::other("omp isn't installed")) }));
	assert_eq!(run(&deps).0.unwrap_err().to_string(), "omp isn't installed");
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
