//! Threads come back after a restart (0549, 0683, 0676).

use std::sync::Arc;

use serde_json::{Value, json};

use crate::frontier::server::close::CloseOutcome;
use crate::frontier::server::helpers::{
	Msg, ThreadOptions, call, demo_project, push_state, session_file, start_thread,
};
use crate::frontier::server::helpers_procs::with_server_on;
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::frontier::server::surface::{ASLEEP, AgentState};
use crate::shared::testing::temp_jocasta;

fn options() -> ServerOptions {
	ServerOptions { agent_wait_ms: Some(10_000), ..ServerOptions::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_each_thread_opens_again_asleep_one_that_never_had_a_turn_too_and_one_mid_turn_goes_on() {
	let j = temp_jocasta();
	demo_project(j.root(), "demo").await;
	let first = Arc::new(FakeSurface::new());
	let (session, quiet_path) = with_server_on(first, options(), |server| async move {
		// No model turn ever ("No model selected"): its omp is up and idle at once, with no session file at all, so the
		// stop saves nothing of it for `--restore`.
		let quiet = start_thread(&server, ThreadOptions { title: Some("Quiet one"), ..ThreadOptions::default() }).await;
		push_state(&server, &quiet.pane_id, AgentState::Idle, None).await;
		let busy = start_thread(&server, ThreadOptions::default()).await;
		let file = session_file(&busy.path, &[Msg::new("Working on it.", 0.1)]);
		push_state(&server, &busy.pane_id, AgentState::Working, file.to_str()).await;
		assert_eq!(server.shutdown(false).await, CloseOutcome::Stopped);
		(file, quiet.path)
	})
	.await;
	// The own shell keeps no tabs across a restart: the next run starts on an empty surface.
	let second = Arc::new(FakeSurface::new());
	let seen = second.clone();
	with_server_on(second, options(), |server| async move {
		let listed = call(&server, "thread_status", &json!({})).await.text;
		assert!(listed.contains("demo/Quiet one") && listed.contains("demo/Readme line"), "{listed}");
		let opened: Vec<Vec<Value>> = seen.of("ensureWorkspace");
		let quiet = opened.iter().find(|args| args[0] == json!("Quiet one")).expect("Quiet one opened again");
		assert_eq!(quiet[1], json!(quiet_path.display().to_string()));
		assert_eq!(quiet[2][0], json!(ASLEEP));
		// The one mid-turn went on with that turn; the quiet one stays asleep until a key or a send wakes it.
		let respawned = seen.of("respawn");
		assert_eq!(respawned.len(), 1, "{respawned:?}");
		let command: Vec<String> = serde_json::from_value(respawned[0][1]["command"].clone()).unwrap();
		let at = command.iter().position(|arg| arg == "--restore").expect("--restore");
		assert_eq!(command[at + 1], session.display().to_string());
	})
	.await;
}
