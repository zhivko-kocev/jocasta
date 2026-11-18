//! `bench.rs`'s tests, ported from the layer's case in `test/evals/arms.test.ts`.

use std::path::Path;

use super::*;
use crate::shared::testing::temp_dir;

/// `TASK_PROMPT_PREFIX` of `evals/tasks.ts`: the prompt of every arm, before the issue's text.
const TASK_PROMPT_PREFIX: &str = "Fix the issue below in the repository in the current folder.\n\n";

fn inside(folder: &Path, path: &Path) -> bool {
	path.starts_with(folder)
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
	let at = args.iter().position(|arg| arg == flag).expect("the flag is given");
	&args[at + 1]
}

/// Waits on a real server, the way the service's own e2e tests do.
#[tokio::test(flavor = "multi_thread")]
async fn a6s_layer_keeps_its_data_under_the_os_temp_folder_not_in_a_trial_root_outside_the_profile_and_removes_it() {
	// EVL-1: the TypeScript put the trial root beside the checkout (on Windows outside the user's profile, D:\ on his
	// PC and in CI); the sandbox root is the folder `temp_dir` gives, which is not under the layer's data either.
	let root = temp_dir("arms");
	let repo = root.path().join("repo");
	std::fs::create_dir(&repo).unwrap();
	let ponytail = root.path().join("system-prompt.md");
	std::fs::write(&ponytail, "# Ponytail, lazy senior dev mode\n").unwrap();
	let prompt = format!("{TASK_PROMPT_PREFIX}The retry test is flaky.");
	let trial = LayerTrial {
		root: root.path().to_path_buf(),
		repo,
		base_commit: "0".repeat(40),
		prompt: prompt.clone(),
		append_system_prompt: Some(ponytail),
		env: Env::new(),
	};
	let mut layer = BENCH_LAYER.set_up(trial).await.unwrap();
	let data = std::path::PathBuf::from(&layer.env["JOCASTA_DATA_DIR"]);
	assert!(inside(&std::env::temp_dir(), &data));
	assert!(!inside(root.path(), &data));
	assert!(data.join("jocasta-service.json").exists());
	let plugin = arg_after(&layer.args, "-e").to_owned();
	// 0577: the check's 30 minutes fit omp's limit; JOCASTA_THREAD lists report_result and check.
	let mcp: Value =
		serde_json::from_str(&std::fs::read_to_string(Path::new(&plugin).join(".mcp.json")).unwrap()).unwrap();
	let server = &mcp["mcpServers"]["jocasta"];
	assert_eq!(server["timeout"], 1_860_000);
	assert_eq!(server["env"]["JOCASTA_DATA_DIR"], data.display().to_string());
	let thread = server["env"]["JOCASTA_THREAD"].as_str().unwrap();
	assert!(!thread.is_empty() && thread.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
	// PRM-5: titled as New thread titles the brief, the checkout's folder as the project.
	let text = std::fs::read_to_string(arg_after(&layer.args, "--append-system-prompt")).unwrap();
	assert!(text.starts_with("# Ponytail, lazy senior dev mode\n"));
	assert!(text.contains(&format!("# Your task: {} (repo)", brief_title(&prompt))));
	assert!(!text.contains("benchmark"));
	// A trial has no past sessions: the search tool answers as for an empty history (0604).
	let service: Value =
		serde_json::from_str(&std::fs::read_to_string(data.join("jocasta-service.json")).unwrap()).unwrap();
	let reply: Value = reqwest::Client::new()
		.post(format!("http://127.0.0.1:{}/api/mcp/history_search", service["port"]))
		.bearer_auth(service["token"].as_str().unwrap())
		.body(r#"{"args":{"query":"retry decision"}}"#)
		.send()
		.await
		.unwrap()
		.json()
		.await
		.unwrap();
	assert_eq!(reply, json!({ "text": "Nothing in repo's history for that." }));
	layer.close().await;
	assert!(!data.exists());
}
