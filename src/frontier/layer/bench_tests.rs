//! `bench.rs`'s tests, ported from the layer's case in `test/evals/arms.test.ts`.

use std::path::Path;

use super::*;
use crate::shared::bridge_wire::{MAX_LINE, VERSION, connect, line_of};
use crate::shared::line_rpc::LineReader;
use crate::shared::testing::temp_dir;
use tokio::io::AsyncWriteExt;

/// `TASK_PROMPT_PREFIX` of `evals/tasks.ts`: the prompt of every arm, before the issue's text.
const TASK_PROMPT_PREFIX: &str = "Fix the issue below in the repository in the current folder.\n\n";

fn inside(folder: &Path, path: &Path) -> bool {
	path.starts_with(folder)
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
	let at = args.iter().position(|arg| arg == flag).expect("the flag is given");
	&args[at + 1]
}

/// Runs on the real bridge the layer starts, the way a pane's omp reaches it.
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
	// 0675: the trial's omp loads the bridge extension a pane does, and finds the bridge and its own key in its env.
	assert_eq!(arg_after(&layer.args, "-e"), shipped_config_dir(None).join(BRIDGE_EXTENSION).display().to_string());
	let address = layer.env[BRIDGE_ENV].clone();
	let key = layer.env[PANE_KEY_ENV].clone();
	assert!(!address.is_empty() && !key.is_empty());
	// PRM-5: titled as New thread titles the brief, the checkout's folder as the project.
	let text = std::fs::read_to_string(arg_after(&layer.args, "--append-system-prompt")).unwrap();
	assert!(text.starts_with("# Ponytail, lazy senior dev mode\n"));
	assert!(text.contains(&format!("# Your task: {} (repo)", brief_title(&prompt))));
	assert!(!text.contains("benchmark"));
	// The pane's hello with its key is welcomed with a thread's tools, history_search among them.
	let (read, mut write) = tokio::io::split(connect(&address).await.unwrap());
	let mut read = LineReader::new(read, MAX_LINE);
	let hello = json!({ "t": "hello", "v": VERSION, "key": key, "pid": std::process::id(), "harness_version": "test" });
	write.write_all(line_of(&hello).as_bytes()).await.unwrap();
	let welcome: Value = serde_json::from_str(&read.next_line().await.unwrap().expect("a welcome")).unwrap();
	assert_eq!(welcome["t"], "welcome");
	let tools: Vec<&str> =
		welcome["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
	assert!(tools.contains(&"history_search"), "{tools:?}");
	// A trial has no past sessions: the search tool answers as for an empty history (0604).
	let call = json!({ "t": "call", "id": "1", "tool": "history_search", "args": { "query": "retry decision" } });
	write.write_all(line_of(&call).as_bytes()).await.unwrap();
	let reply: Value = loop {
		let line = read.next_line().await.unwrap().expect("a result");
		let message: Value = serde_json::from_str(&line).unwrap();
		if message["t"] == "result" {
			break message;
		}
	};
	assert_eq!((reply["id"].as_str(), reply["ok"].as_bool()), (Some("1"), Some(true)));
	assert_eq!(reply["text"], "Nothing in repo's history for that.");
	drop((read, write));
	layer.close().await;
	assert!(!data.exists());
}
