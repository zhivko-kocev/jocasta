//! `herdr.rs`'s tests, ported from `test/herdr/herdr.test.ts`. The live smoke tests there are gone: Jocasta no longer
//! starts a herdr server of its own to talk to.

use std::sync::atomic::AtomicUsize;

use futures_util::FutureExt;
use serde_json::{Value, json};

use super::fake_herdr::{Answer, FakeHerdrServer};
use super::*;
use crate::shared::git::git;
use crate::shared::proctree::pid_alive;
use crate::shared::proctree::tests::{SleeperTree, settles, sleeper_tree};
use crate::shared::testing::{TempDir, temp_dir, temp_jocasta};

struct Setup {
	fake: FakeHerdrServer,
	herdr: Arc<dyn Herdr>,
}

impl Drop for Setup {
	fn drop(&mut self) {
		self.herdr.close();
		self.fake.stop();
	}
}

async fn setup() -> Setup {
	let fake = FakeHerdrServer::start(None).await;
	fake.set_handler("ping", move |_| Answer::result(json!({"type": "pong", "version": "0.9.4", "protocol": 22})));
	let herdr = connect_herdr(HerdrOptions {
		session: "unused".to_owned(),
		socket_path: Some(fake.path.clone()),
		watch_interval_ms: Some(20),
		..HerdrOptions::default()
	})
	.await
	.expect("connecting to the fake herdr");
	Setup { fake, herdr }
}

async fn until(check: impl Fn() -> bool, what: &str) {
	let held = wait_for(|| std::future::ready(Ok(check())), 5_000, 10).await.unwrap_or(false);
	assert!(held, "timed out waiting for {what}");
}

fn reply(value: Value) -> impl Fn(&Value) -> Answer + Send + Sync + 'static {
	move |_| Answer::result(value.clone())
}

fn ok() -> impl Fn(&Value) -> Answer + Send + Sync + 'static {
	|_| Answer::ok()
}

fn strings(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

fn env(pairs: &[(&str, &str)]) -> Env {
	pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

/// A pane as herdr lists it, `extra`'s keys over the usual ones.
fn pane(pane_id: &str, workspace_id: &str, agent_status: &str, extra: Value) -> Value {
	let mut pane = json!({
		"pane_id": pane_id,
		"tab_id": format!("{workspace_id}:t1"),
		"workspace_id": workspace_id,
		"terminal_id": format!("term-{pane_id}"),
		"focused": false,
		"revision": 1,
		"agent_status": agent_status,
		"state_labels": {},
		"tokens": {},
	});
	if let (Value::Object(map), Value::Object(extra)) = (&mut pane, extra) {
		map.extend(extra);
	}
	pane
}

fn layout(workspace_id: &str, tab_id: &str, pane_id: &str) -> Value {
	json!({"type": "layout_apply", "layout": {"workspace_id": workspace_id, "tab_id": tab_id, "root": {"type": "pane", "pane_id": pane_id}}})
}

fn pane_list(panes: Vec<Value>) -> Value {
	json!({"type": "pane_list", "panes": panes})
}

fn shape(requests: &[super::fake_herdr::FakeRequest]) -> Value {
	Value::Array(requests.iter().map(|r| json!({"method": r.method, "params": r.params})).collect())
}

#[test]
fn herdr_socket_path_follows_jocastas_data_dir_the_default_session_is_herdrs_default_as_a_named_pipe_on_windows() {
	let mut temp = temp_jocasta();
	let data = temp.root().join("elsewhere").display().to_string();
	temp.set_var("JOCASTA_DATA_DIR", Some(&data));
	let data = crate::shared::paths::data_dir().display().to_string();
	let (default, named) = if cfg!(windows) {
		let data = data.replace('/', "\\");
		(
			format!("\\\\.\\pipe\\{data}\\herdr\\herdr.sock"),
			format!("\\\\.\\pipe\\{data}\\herdr\\sessions\\s1\\herdr.sock"),
		)
	} else {
		(format!("{data}/herdr/herdr.sock"), format!("{data}/herdr/sessions/s1/herdr.sock"))
	};
	assert_eq!(herdr_socket_path("jocasta"), default);
	assert_eq!(herdr_socket_path("s1"), named);
	// The socket file herdr hands its plugin commands (`HERDR_SOCKET_PATH`) is a pipe name too.
	let file = if cfg!(windows) { named.trim_start_matches("\\\\.\\pipe\\").to_owned() } else { named.clone() };
	assert_eq!(herdr_pipe(&file), named);
}

#[tokio::test]
async fn connect_herdr_pings_once_and_reports_a_server_it_didnt_start() {
	let setup = setup().await;
	assert_eq!(setup.fake.methods(), ["ping"]);
	assert!(setup.herdr.started_server());
}

#[tokio::test]
async fn ensure_workspace_reuses_a_workspace_with_that_label_and_creates_a_missing_one_unfocused() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler(
		"workspace.list",
		reply(
			json!({"type": "workspace_list", "workspaces": [{"workspace_id": "w1", "label": "Jocasta", "number": 1}]}),
		),
	);
	fake.set_handler(
		"workspace.create",
		reply(json!({"type": "workspace_created", "workspace": {"workspace_id": "w2", "label": "demo"}, "tab": {}, "root_pane": {}})),
	);
	assert_eq!(herdr.ensure_workspace("Jocasta", Path::new("C:\\data\\home"), None, None).await.unwrap(), "w1");
	assert!(fake.last("workspace.create").is_none());
	assert_eq!(herdr.ensure_workspace("demo", Path::new("C:\\src\\demo"), None, None).await.unwrap(), "w2");
	assert_eq!(
		fake.last("workspace.create").unwrap().params,
		json!({"cwd": "C:\\src\\demo", "label": "demo", "focus": false})
	);
}

#[tokio::test]
async fn workspaces_take_each_workspaces_cwd_from_its_first_pane() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler(
		"workspace.list",
		reply(json!({"type": "workspace_list", "workspaces": [
			{"workspace_id": "w1", "label": "Jocasta"},
			{"workspace_id": "w2", "label": "empty"},
		]})),
	);
	fake.set_handler(
		"pane.list",
		reply(pane_list(vec![pane("w1:p1", "w1", "idle", json!({"cwd": "C:\\data\\home"}))])),
	);
	assert_eq!(
		herdr.workspaces().await.unwrap(),
		[
			WorkspaceInfo { id: "w1".into(), label: "Jocasta".into(), cwd: Some(PathBuf::from("C:\\data\\home")) },
			WorkspaceInfo { id: "w2".into(), label: "empty".into(), cwd: None },
		]
	);
}

#[tokio::test]
async fn create_tab_applies_a_one_pane_layout_running_the_argv_command() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler(
		"layout.apply",
		reply(json!({"type": "layout_apply", "layout": {
			"workspace_id": "w2", "tab_id": "w2:t3", "zoomed": false, "focused_pane_id": "w2:p5",
			"root": {"type": "pane", "pane_id": "w2:p5"},
		}})),
	);
	let tab = TabSpec {
		title: "fix login".into(),
		cwd: PathBuf::from("C:\\src\\demo"),
		command: strings(&["omp", "--continue"]),
		env: Some(env(&[("A", "1")])),
	};
	assert_eq!(
		herdr.create_tab("w2", &tab).await.unwrap(),
		TabPane { tab_id: "w2:t3".into(), pane_id: "w2:p5".into() }
	);
	assert_eq!(
		fake.last("layout.apply").unwrap().params,
		json!({
			"workspace_id": "w2", "tab_label": "fix login", "focus": false,
			"root": {"type": "pane", "cwd": "C:\\src\\demo", "command": ["omp", "--continue"], "env": {"A": "1"}},
		})
	);
}

fn worktree_created(workspace_id: &str, label: &str, path: &str) -> Value {
	json!({
		"type": "worktree_created",
		"workspace": {"workspace_id": workspace_id, "label": label},
		"tab": {"tab_id": format!("{workspace_id}:t1")},
		"root_pane": pane(&format!("{workspace_id}:p1"), workspace_id, "unknown", json!({})),
		"worktree": {"path": path, "label": label},
	})
}

#[tokio::test]
async fn create_worktree_creates_the_checkout_then_replaces_its_shell_tab_with_one_running_the_command() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("worktree.create", reply(worktree_created("w4", "fix login", "C:\\wt\\demo\\fix-login")));
	fake.set_handler("layout.apply", reply(layout("w4", "w4:t2", "w4:p2")));
	let command = strings(&["omp", "--continue", "--config", "C:\\data\\projects\\demo\\omp.yml"]);
	let created = herdr
		.create_worktree(
			"w2",
			WorktreeOptions {
				repo: PathBuf::from("C:\\src\\demo"),
				branch: "fix-login".into(),
				path: PathBuf::from("C:\\wt\\demo\\fix-login"),
				title: "fix login".into(),
				command: command.clone(),
				..WorktreeOptions::default()
			},
		)
		.await
		.unwrap();
	assert_eq!(
		created,
		WorktreeCreated {
			workspace_id: "w4".into(),
			tab_id: "w4:t2".into(),
			pane_id: "w4:p2".into(),
			path: PathBuf::from("C:\\wt\\demo\\fix-login"),
		}
	);
	assert_eq!(fake.methods(), ["ping", "worktree.create", "pane.list", "layout.apply"]);
	assert_eq!(
		fake.last("worktree.create").unwrap().params,
		json!({
			"workspace_id": "w2", "branch": "fix-login", "path": "C:\\wt\\demo\\fix-login",
			"label": "fix login", "focus": false,
		})
	);
	assert_eq!(
		fake.last("layout.apply").unwrap().params,
		json!({
			"tab_id": "w4:t1", "tab_label": "fix login", "focus": false,
			"root": {"type": "pane", "cwd": "C:\\wt\\demo\\fix-login", "command": command},
		})
	);
}

#[tokio::test]
async fn create_worktree_passes_the_base_and_what_prepare_returns_joins_the_panes_env_before_the_pane_starts() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("worktree.create", reply(worktree_created("w4", "x", "C:\\wt\\x")));
	fake.set_handler("layout.apply", reply(layout("w4", "w4:t2", "w4:p2")));
	let prepared = Arc::new(Mutex::new(Vec::<String>::new()));
	let (seen, server) = (prepared.clone(), fake.clone());
	let prepare: Prepare = Box::new(move |path| {
		async move {
			// The checkout exists, the pane doesn't yet.
			let mut seen = locked(&seen);
			seen.push(path.display().to_string());
			seen.extend(server.methods());
			Ok(env(&[("B", "2")]))
		}
		.boxed()
	});
	herdr
		.create_worktree(
			"w2",
			WorktreeOptions {
				repo: PathBuf::from("C:\\src\\demo"),
				branch: "feature/x".into(),
				base: Some("main".into()),
				path: PathBuf::from("C:\\wt\\x"),
				title: "x".into(),
				command: strings(&["omp"]),
				env: Some(env(&[("A", "1")])),
				prepare: Some(prepare),
			},
		)
		.await
		.unwrap();
	assert_eq!(*locked(&prepared), ["C:\\wt\\x", "ping", "worktree.create"]);
	let created = fake.last("worktree.create").unwrap().params;
	assert_eq!((created["branch"].as_str(), created["base"].as_str()), (Some("feature/x"), Some("main")));
	assert_eq!(fake.last("layout.apply").unwrap().params["root"]["env"], json!({"A": "1", "B": "2"}));
}

fn split_handlers(fake: &FakeHerdrServer) {
	fake.set_handler("pane.get", reply(json!({"type": "pane_info", "pane": pane("w4:p2", "w4", "idle", json!({}))})));
	fake.set_handler("layout.apply", reply(layout("w4", "w4:t9", "w4:p7")));
	fake.set_handler(
		"pane.move",
		reply(
			json!({"type": "pane_move", "move_result": {"changed": true, "pane": pane("w4:p7", "w4", "unknown", json!({}))}}),
		),
	);
}

#[tokio::test]
async fn split_beside_starts_the_command_in_a_temporary_tab_and_moves_its_pane_beside_the_target() {
	let Setup { fake, herdr, .. } = &setup().await;
	split_handlers(fake);
	let command = strings(&["hunk", "diff", "main...fix-login", "--watch"]);
	let id = herdr
		.split_beside(
			"w4:p2",
			&SplitOptions {
				command: command.clone(),
				cwd: PathBuf::from("C:\\wt\\demo\\fix-login"),
				ratio: Some(0.4),
				..SplitOptions::default()
			},
		)
		.await
		.unwrap();
	assert_eq!(id, "w4:p7");
	assert_eq!(
		fake.last("layout.apply").unwrap().params,
		json!({
			"workspace_id": "w4", "focus": false,
			"root": {"type": "pane", "cwd": "C:\\wt\\demo\\fix-login", "command": command},
		})
	);
	assert_eq!(
		fake.last("pane.move").unwrap().params,
		json!({
			"pane_id": "w4:p7",
			"destination": {"type": "tab", "tab_id": "w4:t1", "target_pane_id": "w4:p2", "split": "right", "ratio": 0.4},
			"focus": false,
		})
	);

	// A refused move (zoomed tab) closes the temporary tab and throws herdr's reason.
	fake.set_handler(
		"pane.move",
		reply(json!({"type": "pane_move", "move_result": {"changed": false, "reason": "zoomed_tab", "pane": pane("w4:p7", "w4", "unknown", json!({}))}})),
	);
	fake.set_handler("tab.close", ok());
	let refused = herdr
		.split_beside(
			"w4:p2",
			&SplitOptions {
				command: strings(&["hunk"]),
				cwd: PathBuf::from("C:\\wt"),
				direction: Some(Direction::Down),
				..SplitOptions::default()
			},
		)
		.await
		.unwrap_err();
	assert_eq!(refused.code, "zoomed_tab");
	assert_eq!(fake.last("tab.close").unwrap().params, json!({"tab_id": "w4:t9"}));
	assert_eq!(fake.last("pane.move").unwrap().params["destination"]["split"], "down");
}

#[tokio::test]
async fn split_beside_runs_the_command_with_its_env_and_names_the_new_pane() {
	let Setup { fake, herdr, .. } = &setup().await;
	split_handlers(fake);
	fake.set_handler("pane.rename", ok());
	herdr
		.split_beside(
			"w4:p2",
			&SplitOptions {
				command: strings(&["hunk"]),
				cwd: PathBuf::from("C:\\wt"),
				env: Some(env(&[("HUNK_MCP_PORT", "1")])),
				label: Some("review".into()),
				..SplitOptions::default()
			},
		)
		.await
		.unwrap();
	assert_eq!(
		fake.last("layout.apply").unwrap().params["root"],
		json!({"type": "pane", "cwd": "C:\\wt", "command": ["hunk"], "env": {"HUNK_MCP_PORT": "1"}})
	);
	assert_eq!(fake.last("pane.rename").unwrap().params, json!({"pane_id": "w4:p7", "label": "review"}));
}

#[tokio::test]
async fn prompt_and_send_text_strip_esc_and_other_control_characters_so_outside_text_cant_end_the_paste() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("agent.prompt", ok());
	fake.set_handler("pane.send_text", ok());
	herdr.prompt("w4:p2", "title\u{1b}[201~\r!echo pwned\u{7}\u{9b}2J\u{7f}\nnext\tline «ok» é").await.unwrap();
	herdr.send_text("w4:p2", "a\u{1b}b").await.unwrap();
	assert_eq!(fake.last("agent.prompt").unwrap().params["text"], "title[201~!echo pwned2J\nnext\tline «ok» é");
	assert_eq!(fake.last("pane.send_text").unwrap().params["text"], "ab");
	assert_eq!(pane_text("plain"), "plain");
}

#[tokio::test]
async fn snapshot_reads_workspaces_and_panes_once_each_close_workspace_and_ttld_metadata_send_herdrs_params() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler(
		"workspace.list",
		reply(json!({"type": "workspace_list", "workspaces": [{"workspace_id": "w1", "label": "Jocasta"}]})),
	);
	fake.set_handler("pane.list", reply(pane_list(vec![pane("w1:p1", "w1", "idle", json!({"cwd": "C:\\h"}))])));
	fake.set_handler("workspace.close", ok());
	fake.set_handler("pane.report_metadata", ok());
	fake.set_handler("workspace.report_metadata", ok());
	let before = fake.requests().len();
	let snapshot = herdr.snapshot().await.unwrap();
	assert_eq!(
		snapshot.workspaces,
		[WorkspaceInfo { id: "w1".into(), label: "Jocasta".into(), cwd: Some(PathBuf::from("C:\\h")) }]
	);
	assert_eq!(snapshot.panes.iter().map(|p| p.pane_id.as_str()).collect::<Vec<_>>(), ["w1:p1"]);
	let mut methods: Vec<String> = fake.requests()[before..].iter().map(|r| r.method.clone()).collect();
	methods.sort();
	assert_eq!(methods, ["pane.list", "workspace.list"]);
	herdr.close_workspace("w2").await.unwrap();
	assert_eq!(fake.last("workspace.close").unwrap().params, json!({"workspace_id": "w2"}));
	let meta = PaneMetadata {
		tokens: Some(vec![("cost".into(), Some("$1".into()))]),
		ttl_ms: Some(60_000),
		..PaneMetadata::default()
	};
	herdr.report_pane_metadata("w1:p1", &meta).await.unwrap();
	assert_eq!(
		fake.last("pane.report_metadata").unwrap().params,
		json!({"pane_id": "w1:p1", "source": "jocasta", "tokens": {"cost": "$1"}, "ttl_ms": 60_000})
	);
	herdr.report_workspace_metadata("w1", &vec![("running".into(), None)], Some(60_000)).await.unwrap();
	assert_eq!(
		fake.last("workspace.report_metadata").unwrap().params,
		json!({"workspace_id": "w1", "source": "jocasta", "tokens": {"running": null}, "ttl_ms": 60_000})
	);
}

#[tokio::test]
async fn watch_a_state_pushed_while_the_subscriptions_reconcile_read_is_in_flight_isnt_overwritten_by_that_read() {
	let Setup { fake, herdr, .. } = &setup().await;
	let lists = Arc::new(AtomicUsize::new(0));
	let (count, server) = (lists.clone(), fake.clone());
	fake.set_handler("pane.list", move |_| {
		// The reconcile read (the second list): herdr pushes `working` before answering with its older `idle`.
		if count.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
			server.push(
				"pane.agent_status_changed",
				&json!({"pane_id": "w2:p1", "workspace_id": "w2", "agent_status": "working"}),
			);
		}
		Answer::result(pane_list(vec![pane("w2:p1", "w2", "idle", json!({}))]))
	});
	let seen = Arc::new(Mutex::new(Vec::<AgentState>::new()));
	let sink = seen.clone();
	let stop = herdr.watch(Box::new(|| strings(&["w2:p1"])), Box::new(move |event| locked(&sink).push(event.state)));
	until(|| !locked(&seen).is_empty() && lists.load(Ordering::SeqCst) >= 2, "the subscription and its reconcile read")
		.await;
	herdr.panes().await.unwrap();
	stop();
	assert_eq!(*locked(&seen), [AgentState::Working]);
}

#[tokio::test]
async fn prompt_send_text_stop_focus_and_metadata_send_herdrs_methods_and_params() {
	let Setup { fake, herdr, .. } = &setup().await;
	for method in [
		"agent.prompt",
		"pane.send_text",
		"pane.send_keys",
		"pane.focus",
		"pane.report_metadata",
		"workspace.report_metadata",
	] {
		fake.set_handler(method, ok());
	}
	herdr.prompt("w4:p2", "add a README line").await.unwrap();
	herdr.send_text("w4:p2", "/handoff").await.unwrap();
	herdr.stop("w4:p2").await.unwrap();
	herdr.focus("w4:p2").await.unwrap();
	herdr
		.report_pane_metadata(
			"w4:p2",
			&PaneMetadata {
				tokens: Some(vec![("model".into(), Some("opus".into())), ("cost".into(), Some("$0.42".into()))]),
				state_labels: Some(vec![(AgentState::Done, "review ready".into())]),
				ttl_ms: None,
			},
		)
		.await
		.unwrap();
	herdr.report_workspace_metadata("w4", &vec![("running".into(), Some("2".into()))], None).await.unwrap();
	assert_eq!(
		shape(&fake.requests()[1..]),
		json!([
			{"method": "agent.prompt", "params": {"target": "w4:p2", "text": "add a README line"}},
			{"method": "pane.send_text", "params": {"pane_id": "w4:p2", "text": "/handoff"}},
			{"method": "pane.send_keys", "params": {"pane_id": "w4:p2", "keys": ["esc"]}},
			{"method": "pane.focus", "params": {"pane_id": "w4:p2"}},
			{"method": "pane.report_metadata", "params": {
				"pane_id": "w4:p2", "source": "jocasta",
				"tokens": {"model": "opus", "cost": "$0.42"},
				"state_labels": {"done": "review ready"},
			}},
			{"method": "workspace.report_metadata", "params": {
				"workspace_id": "w4", "source": "jocasta", "tokens": {"running": "2"},
			}},
		])
	);
}

#[tokio::test]
async fn each_request_uses_its_own_connection_an_error_response_rejects_with_herdrs_code() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("agent.prompt", |_| {
		Answer::error("agent_blocked", "agent w4:p2 is blocked and requires interactive input")
	});
	fake.set_handler("pane.list", reply(pane_list(vec![])));
	let blocked = herdr.prompt("w4:p2", "go").await.unwrap_err();
	assert_eq!(blocked.code, "agent_blocked");
	assert!(herdr.panes().await.unwrap().is_empty());
	assert!(herdr.panes().await.unwrap().is_empty());
	assert_eq!(fake.connections(), 4);
}

#[tokio::test]
async fn a_read_herdr_closes_before_answering_is_asked_again_once_a_second_drop_fails() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("pane.list", reply(pane_list(vec![pane("w1:p1", "w1", "idle", json!({}))])));
	fake.drop_next("pane.list", 1);
	let ids: Vec<String> = herdr.panes().await.unwrap().into_iter().map(|p| p.pane_id).collect();
	assert_eq!(ids, ["w1:p1"]);
	assert_eq!(fake.methods().iter().filter(|m| *m == "pane.list").count(), 2);
	fake.drop_next("pane.list", 2);
	assert_eq!(herdr.panes().await.unwrap_err().code, "transport");
}

#[tokio::test]
async fn a_change_herdr_closes_before_answering_is_never_sent_again_herdr_may_have_made_it() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("agent.prompt", ok());
	fake.drop_next("agent.prompt", 1);
	let error = herdr.prompt("w4:p2", "go").await.unwrap_err();
	assert!(error.message.contains("closed the connection before answering agent.prompt"), "{error}");
	assert_eq!(fake.methods().iter().filter(|m| *m == "agent.prompt").count(), 1);
}

#[tokio::test]
async fn panes_map_herdrs_pane_list_with_the_omp_session_file_when_herdr_has_one() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler(
		"pane.list",
		reply(pane_list(vec![
			pane(
				"w1:p1",
				"w1",
				"working",
				json!({
					"cwd": "C:\\data\\home", "agent": "omp", "label": "boss",
					"agent_session": {"source": "herdr:omp", "agent": "omp", "kind": "path", "value": "C:\\s\\a.jsonl"},
				}),
			),
			pane("w2:p1", "w2", "something-new", json!({"agent_session": {"kind": "id", "value": "abc"}})),
		])),
	);
	assert_eq!(
		herdr.panes().await.unwrap(),
		[
			PaneInfo {
				pane_id: "w1:p1".into(),
				tab_id: "w1:t1".into(),
				workspace_id: "w1".into(),
				cwd: Some(PathBuf::from("C:\\data\\home")),
				agent_state: AgentState::Working,
				agent: Some("omp".into()),
				session_path: Some(PathBuf::from("C:\\s\\a.jsonl")),
				label: Some("boss".into()),
				terminal_id: Some("term-w1:p1".into()),
				focused: false,
			},
			PaneInfo {
				pane_id: "w2:p1".into(),
				tab_id: "w2:t1".into(),
				workspace_id: "w2".into(),
				agent_state: AgentState::Unknown,
				terminal_id: Some("term-w2:p1".into()),
				..PaneInfo::default()
			},
		]
	);
	assert_eq!(fake.last("pane.list").unwrap().params, json!({}));
}

#[tokio::test]
async fn move_workspace_and_respawn_send_workspace_move_and_a_layout_over_the_panes_tab() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("workspace.move", ok());
	fake.set_handler(
		"pane.get",
		reply(json!({"type": "pane_info", "pane": pane("w1:p1", "w1", "unknown", json!({}))})),
	);
	fake.set_handler("layout.apply", reply(layout("w1", "w1:t1", "w1:p2")));
	herdr.move_workspace("w1", 0).await.unwrap();
	let respawned = herdr
		.respawn(
			"w1:p1",
			&RespawnOptions {
				cwd: PathBuf::from("C:\\data\\home"),
				command: strings(&["omp", "--continue"]),
				env: None,
			},
		)
		.await
		.unwrap();
	assert_eq!(respawned, TabPane { tab_id: "w1:t1".into(), pane_id: "w1:p2".into() });
	assert_eq!(
		shape(&fake.requests()[1..]),
		json!([
			{"method": "workspace.move", "params": {"workspace_id": "w1", "insert_index": 0}},
			{"method": "pane.get", "params": {"pane_id": "w1:p1"}},
			{"method": "pane.list", "params": {}},
			{"method": "layout.apply", "params": {
				"tab_id": "w1:t1", "focus": false,
				"root": {"type": "pane", "cwd": "C:\\data\\home", "command": ["omp", "--continue"]},
			}},
		])
	);
}

#[tokio::test]
async fn watch_subscribes_per_pane_reports_the_current_state_then_changes_follows_the_pane_set_and_resubscribes_after_events_lost()
 {
	let Setup { fake, herdr, .. } = &setup().await;
	let states = Arc::new(Mutex::new(std::collections::BTreeMap::from([
		("w2:p1".to_owned(), "idle".to_owned()),
		("w3:p1".to_owned(), "working".to_owned()),
	])));
	let listed = states.clone();
	fake.set_handler("pane.list", move |_| {
		let panes = locked(&listed)
			.iter()
			.map(|(id, state)| pane(id, id.split(':').next().unwrap_or(""), state, json!({})))
			.collect();
		Answer::result(pane_list(panes))
	});
	let wanted = Arc::new(Mutex::new(strings(&["w2:p1", "w9:p9"])));
	let seen = Arc::new(Mutex::new(Vec::<WatchEvent>::new()));
	let (ask, sink) = (wanted.clone(), seen.clone());
	let stop = herdr.watch(Box::new(move || locked(&ask).clone()), Box::new(move |event| locked(&sink).push(event)));
	let event =
		|pane: &str, workspace: &str, state| WatchEvent { pane_id: pane.into(), workspace_id: workspace.into(), state };

	until(|| locked(&seen).len() == 1, "the initial state").await;
	assert_eq!(locked(&seen)[0], event("w2:p1", "w2", AgentState::Idle));
	// A pane herdr doesn't know is left out of the subscription (herdr would reject the whole request).
	assert_eq!(
		fake.last("events.subscribe").unwrap().params,
		json!({"subscriptions": [{"type": "pane.agent_status_changed", "pane_id": "w2:p1"}]})
	);

	let working = json!({"pane_id": "w2:p1", "workspace_id": "w2", "agent_status": "working"});
	fake.push("pane.agent_status_changed", &working);
	fake.push("pane.agent_status_changed", &working);
	until(|| locked(&seen).len() == 2, "the pushed change").await;
	assert_eq!(locked(&seen)[1], event("w2:p1", "w2", AgentState::Working));

	*locked(&wanted) = strings(&["w2:p1", "w3:p1"]);
	until(|| locked(&seen).iter().any(|e| e.pane_id == "w3:p1"), "the new pane").await;
	assert_eq!(
		fake.last("events.subscribe").unwrap().params,
		json!({"subscriptions": [
			{"type": "pane.agent_status_changed", "pane_id": "w2:p1"},
			{"type": "pane.agent_status_changed", "pane_id": "w3:p1"},
		]})
	);

	let subscribes = fake.methods().iter().filter(|m| *m == "events.subscribe").count();
	locked(&states).insert("w2:p1".to_owned(), "blocked".to_owned());
	fake.lose_events();
	until(|| locked(&seen).iter().any(|e| e.state == AgentState::Blocked), "the state read after resubscribing").await;
	assert_eq!(fake.methods().iter().filter(|m| *m == "events.subscribe").count(), subscribes + 1);

	stop();
	until(|| fake.subscriptions_closed(), "the subscription to close").await;
}

#[tokio::test]
async fn stop_server_sends_server_stop_waits_for_the_socket_to_go_and_nothing_starts_the_server_again() {
	let Setup { fake, herdr, .. } = &setup().await;
	// herdr goes right after it took the request (the answer may or may not get out).
	let server = fake.clone();
	fake.set_handler("server.stop", move |_| {
		let server = server.clone();
		tokio::spawn(async move { server.stop() });
		Answer::ok()
	});
	herdr.stop_server().await.unwrap();
	assert!(fake.methods().contains(&"server.stop".to_owned()));
	// The server stays down: nothing starts it again.
	assert_eq!(herdr.workspaces().await.unwrap_err().code, "unreachable");
	// Stopping again: nothing runs, nothing is sent.
	herdr.stop_server().await.unwrap();
}

#[tokio::test]
async fn stop_server_is_done_once_herdr_says_it_is_shutting_down_though_its_socket_still_answers() {
	let Setup { fake, herdr, .. } = &setup().await;
	// Jocasta's own server: its socket answers until the process ends, and every request but `ping` says it is going.
	let going = Arc::new(AtomicBool::new(false));
	let stopped = Arc::clone(&going);
	fake.set_handler("server.stop", move |_| {
		stopped.store(true, Ordering::SeqCst);
		Answer::ok()
	});
	let asked = Arc::clone(&going);
	fake.set_handler("pane.list", move |_| {
		if asked.load(Ordering::SeqCst) {
			Answer::error("server_unavailable", "server is shutting down")
		} else {
			reply(pane_list(Vec::new()))(&json!({}))
		}
	});
	let started = std::time::Instant::now();
	herdr.stop_server().await.unwrap();
	assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
}

/// A temp folder with a real sleeper tree working in it, killed after the test whatever happens.
struct TreeIn {
	tree: SleeperTree,
	_dir: TempDir,
}

async fn tree_in(name: &str) -> TreeIn {
	let dir = temp_dir(&format!("jocasta-herdr-{name}"));
	let tree = sleeper_tree(dir.path(), false).await;
	TreeIn { tree, _dir: dir }
}

/// herdr's `pane.process_info` answer for a pane whose process is `pid`.
fn process_info(pane_id: &str, pid: u32) -> Answer {
	Answer::result(json!({
		"type": "pane_process_info",
		"process_info": {"pane_id": pane_id, "shell_pid": pid, "foreground_processes": [{"pid": pid, "name": "bun.exe"}]},
	}))
}

/// `pane.process_info` answers for the panes named, each with its tree's root.
fn process_handler(fake: &FakeHerdrServer, roots: Vec<(&'static str, u32)>) {
	fake.set_handler("pane.process_info", move |params| {
		let asked = params["pane_id"].as_str().unwrap_or_default();
		let (pane_id, root) =
			roots.iter().find(|(id, _)| *id == asked).copied().unwrap_or_else(|| *roots.last().unwrap());
		process_info(pane_id, root)
	});
}

fn process_info_asks(fake: &FakeHerdrServer) -> Vec<Value> {
	fake.requests().into_iter().filter(|r| r.method == "pane.process_info").map(|r| r.params).collect()
}

fn all_alive(pids: &[u32]) -> bool {
	pids.iter().all(|pid| pid_alive(*pid))
}

#[tokio::test]
async fn close_workspace_ends_every_process_its_panes_ran_with_all_they_started_other_workspaces_stay() {
	let Setup { fake, herdr, .. } = &setup().await;
	let (closing, staying) = (tree_in("closing").await, tree_in("staying").await);
	fake.set_handler(
		"pane.list",
		reply(pane_list(vec![pane("w2:p1", "w2", "working", json!({})), pane("w1:p1", "w1", "idle", json!({}))])),
	);
	process_handler(fake, vec![("w2:p1", closing.tree.root), ("w1:p1", staying.tree.root)]);
	let alive_at_close = Arc::new(AtomicBool::new(false));
	let (flag, pids) = (alive_at_close.clone(), closing.tree.all.clone());
	fake.set_handler("workspace.close", move |_| {
		flag.store(all_alive(&pids), Ordering::SeqCst);
		Answer::ok()
	});
	herdr.close_workspace("w2").await.unwrap();
	assert!(alive_at_close.load(Ordering::SeqCst));
	assert!(closing.tree.all.iter().all(|pid| !pid_alive(*pid)));
	assert!(all_alive(&staying.tree.all));
	assert_eq!(process_info_asks(fake), [json!({"pane_id": "w2:p1"})]);
}

#[tokio::test]
async fn ensure_workspace_ends_the_default_shell_its_commands_layout_replaced_another_tabs_processes_stay() {
	// herdr 0.9.5 on Windows drops the replaced pane but can leave its shell running, listed nowhere after.
	let Setup { fake, herdr, .. } = &setup().await;
	let (shell, other_tab) = (tree_in("shell").await, tree_in("other-tab").await);
	fake.set_handler("workspace.list", reply(json!({"type": "workspace_list", "workspaces": []})));
	fake.set_handler(
		"workspace.create",
		reply(json!({
			"type": "workspace_created",
			"workspace": {"workspace_id": "w2", "label": "demo"},
			"tab": {"tab_id": "w2:t1"},
			"root_pane": pane("w2:p1", "w2", "unknown", json!({})),
		})),
	);
	fake.set_handler(
		"pane.list",
		reply(pane_list(vec![
			pane("w2:p1", "w2", "unknown", json!({})),
			pane("w2:p9", "w2", "idle", json!({"tab_id": "w2:t2"})),
		])),
	);
	process_handler(fake, vec![("w2:p1", shell.tree.root), ("w2:p9", other_tab.tree.root)]);
	let alive_at_apply = Arc::new(AtomicBool::new(false));
	let (flag, pids) = (alive_at_apply.clone(), shell.tree.all.clone());
	fake.set_handler("layout.apply", move |_| {
		flag.store(all_alive(&pids), Ordering::SeqCst);
		Answer::result(layout("w2", "w2:t1", "w2:p2"))
	});
	let command = strings(&["omp", "--continue"]);
	let id = herdr.ensure_workspace("demo", Path::new("C:\\src\\demo"), Some(&command), None).await.unwrap();
	assert_eq!(id, "w2");
	assert!(alive_at_apply.load(Ordering::SeqCst));
	assert!(shell.tree.all.iter().all(|pid| !pid_alive(*pid)));
	assert!(all_alive(&other_tab.tree.all));
	assert_eq!(process_info_asks(fake), [json!({"pane_id": "w2:p1"})]);
}

#[tokio::test]
async fn stop_server_ends_every_panes_processes_with_all_they_started_once_the_server_is_gone() {
	let Setup { fake, herdr, .. } = &setup().await;
	let (first, second) = (tree_in("first").await, tree_in("second").await);
	fake.set_handler(
		"pane.list",
		reply(pane_list(vec![pane("w1:p1", "w1", "idle", json!({})), pane("w2:p1", "w2", "working", json!({}))])),
	);
	process_handler(fake, vec![("w1:p1", first.tree.root), ("w2:p1", second.tree.root)]);
	let alive_at_stop = Arc::new(AtomicBool::new(false));
	let (flag, server) = (alive_at_stop.clone(), fake.clone());
	let pids: Vec<u32> = first.tree.all.iter().chain(&second.tree.all).copied().collect();
	let watched = pids.clone();
	fake.set_handler("server.stop", move |_| {
		flag.store(all_alive(&watched), Ordering::SeqCst);
		let server = server.clone();
		tokio::spawn(async move { server.stop() });
		Answer::ok()
	});
	herdr.stop_server().await.unwrap();
	assert!(alive_at_stop.load(Ordering::SeqCst));
	assert!(pids.iter().all(|pid| !pid_alive(*pid)));
}

#[tokio::test]
async fn pane_root_names_the_process_herdr_started_for_a_pane_or_none_when_herdr_names_none() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("pane.process_info", |params| {
		if params["pane_id"] == "w1:p1" {
			process_info("w1:p1", 4242)
		} else {
			Answer::result(
				json!({"type": "pane_process_info", "process_info": {"pane_id": params["pane_id"], "shell_pid": null}}),
			)
		}
	});
	assert_eq!(herdr.pane_root("w1:p1", None).await.unwrap(), Some(4242));
	assert_eq!(herdr.pane_root("w2:p1", None).await.unwrap(), None);
}

#[tokio::test]
async fn a_watch_stopped_while_herdr_answers_its_subscribe_leaves_no_subscription_open() {
	let Setup { fake, herdr, .. } = &setup().await;
	fake.set_handler("pane.list", reply(pane_list(vec![pane("w1:p1", "w1", "idle", json!({}))])));
	let stop: Arc<Mutex<Option<StopWatch>>> = Arc::new(Mutex::new(None));
	let stopper = stop.clone();
	fake.set_handler("events.subscribe", move |_| {
		// The stop lands while herdr takes the request, before its answer arrives.
		if let Some(stop) = locked(&stopper).as_ref() {
			stop();
		}
		Answer::result(json!({"type": "subscription_started"}))
	});
	*locked(&stop) = Some(herdr.watch(Box::new(|| strings(&["w1:p1"])), Box::new(|_| {})));
	until(|| fake.subscription_count() == 1, "the subscribe").await;
	assert!(settles(|| fake.subscriptions_closed()).await);
}

#[tokio::test]
async fn connecting_never_starts_a_server_no_answer_is_unreachable() {
	let name = format!("jocasta-herdr-none-{}", std::process::id());
	let socket_path = if cfg!(windows) {
		format!("\\\\.\\pipe\\{name}")
	} else {
		std::env::temp_dir().join(format!("{name}.sock")).display().to_string()
	};
	// Nothing listens there, and nothing starts a server.
	let Err(error) = connect_herdr(HerdrOptions {
		session: "unused".to_owned(),
		socket_path: Some(socket_path),
		..HerdrOptions::default()
	})
	.await
	else {
		panic!("connected to nothing");
	};
	assert_eq!(error.code, "unreachable");
}

// The in-memory `FakeHerdr` (`test/frontier/server/fake-herdr.ts`) has no test of its own in the TypeScript; these keep
// its behaviour, which the service's tests rely on, from drifting.

fn in_memory() -> Arc<fake_herdr::FakeHerdr> {
	Arc::new(fake_herdr::FakeHerdr::new())
}

#[tokio::test]
async fn the_in_memory_fake_lists_then_creates_so_two_calls_at_once_both_create() {
	let fake = in_memory();
	let cwd = PathBuf::from("C:\\src\\demo");
	let (first, second) =
		tokio::join!(fake.ensure_workspace("demo", &cwd, None, None), fake.ensure_workspace("demo", &cwd, None, None));
	assert_ne!(first.unwrap(), second.unwrap());
	assert_eq!(fake.state().workspace_list.len(), 2);
	let again = fake.ensure_workspace("demo", &cwd, None, None).await.unwrap();
	assert_eq!(fake.state().workspace_list.first().map(|w| w.id.clone()), Some(again));
	assert_eq!(fake.of("ensureWorkspace").len(), 3);
}

#[tokio::test]
async fn the_in_memory_fake_reports_state_changes_to_a_watch_restores_panes_as_shells_and_forgets_a_stopped_watch() {
	let fake = in_memory();
	let command = strings(&["omp"]);
	let id = fake
		.ensure_workspace("demo", Path::new("C:\\src\\demo"), Some(&command), Some(&env(&[("K", "V")])))
		.await
		.unwrap();
	let pane_id = fake.panes().await.unwrap()[0].pane_id.clone();
	assert_eq!(fake.env_of(&pane_id), Some(env(&[("K", "V")])));
	let seen = Arc::new(Mutex::new(Vec::<WatchEvent>::new()));
	let sink = seen.clone();
	let stop = fake.watch(Box::new(Vec::new), Box::new(move |event| locked(&sink).push(event)));
	fake.set_state(&pane_id, AgentState::Idle, Some("C:\\s\\a.jsonl"), true);
	fake.set_state(&pane_id, AgentState::Working, None, false);
	assert_eq!(*locked(&seen), [WatchEvent { pane_id: pane_id.clone(), workspace_id: id, state: AgentState::Idle }]);
	let pane = fake.panes().await.unwrap().remove(0);
	assert_eq!((pane.agent_state, pane.agent.as_deref()), (AgentState::Working, Some("omp")));
	assert_eq!(pane.session_path, Some(PathBuf::from("C:\\s\\a.jsonl")));
	fake.restore();
	let pane = fake.panes().await.unwrap().remove(0);
	assert_eq!(
		(pane.agent_state, pane.agent, pane.session_path, pane.label.as_deref()),
		(AgentState::Unknown, None, None, Some("shell"))
	);
	stop();
	fake.set_state(&pane_id, AgentState::Done, None, true);
	assert_eq!(locked(&seen).len(), 1);
}

#[tokio::test]
async fn the_in_memory_fake_makes_a_real_linked_worktree_and_a_workspace_and_pane_for_it() {
	let repo = temp_dir("herdr-fake-repo");
	let identity = ["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	let git_in = |args: &[&str]| {
		let mut all = identity.to_vec();
		all.extend(args);
		git(repo.path(), &all).unwrap().expect("git succeeded")
	};
	git_in(&["init", "-q", "-b", "main"]);
	git_in(&["commit", "--allow-empty", "-q", "-m", "init"]);
	git_in(&["branch", "spare"]);
	let fake = in_memory();
	fake.state().omp_up_on_create = Some(AgentState::Idle);
	let path = repo.path().join("wt").join("fix-login");
	let prepare: Prepare = Box::new(|path| {
		async move {
			assert!(path.exists(), "the checkout exists before prepare runs");
			Ok(env(&[("B", "2")]))
		}
		.boxed()
	});
	let created = fake
		.create_worktree(
			"w1",
			WorktreeOptions {
				repo: repo.path().to_path_buf(),
				branch: "fix-login".into(),
				base: Some("main".into()),
				path: path.clone(),
				title: "fix login".into(),
				command: strings(&["omp"]),
				env: Some(env(&[("A", "1")])),
				prepare: Some(prepare),
			},
		)
		.await
		.unwrap();
	assert_eq!(created.path, path);
	assert!(path.join(".git").exists());
	assert_eq!(fake.env_of(&created.pane_id), Some(env(&[("A", "1"), ("B", "2")])));
	assert_eq!(fake.state().workspace_list.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["fix login"]);
	assert_eq!(fake.panes().await.unwrap()[0].agent_state, AgentState::Idle);
	assert_eq!(fake.of("createWorktree")[0][1]["env"], json!({"A": "1", "B": "2"}));
	// An existing branch is checked out as it is.
	let other = repo.path().join("wt").join("again");
	let failing = WorktreeOptions {
		repo: repo.path().to_path_buf(),
		branch: "spare".into(),
		path: other,
		title: "again".into(),
		command: strings(&["omp"]),
		..WorktreeOptions::default()
	};
	fake.state().create_worktree_fails = Some(HerdrError::new("boom", "layout failed"));
	assert_eq!(fake.create_worktree("w1", failing).await.unwrap_err().code, "boom");
	assert_eq!(fake.state().workspace_list.len(), 2);
}

#[tokio::test]
async fn the_in_memory_fake_respawns_in_the_same_tab_closes_workspaces_with_their_panes_and_stops_its_server() {
	let fake = in_memory();
	let cwd = Path::new("C:\\src\\demo");
	let first = fake.ensure_workspace("one", cwd, None, None).await.unwrap();
	let second = fake.ensure_workspace("two", cwd, None, None).await.unwrap();
	fake.move_workspace(&second, 0).await.unwrap();
	assert_eq!(fake.workspaces().await.unwrap().iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["two", "one"]);
	let old = fake.panes().await.unwrap().into_iter().find(|p| p.workspace_id == first).unwrap();
	let respawned = fake
		.respawn(&old.pane_id, &RespawnOptions { cwd: cwd.to_path_buf(), command: strings(&["omp"]), env: None })
		.await
		.unwrap();
	assert_eq!(respawned.tab_id, old.tab_id);
	assert_ne!(respawned.pane_id, old.pane_id);
	let beside = fake
		.split_beside(
			&respawned.pane_id,
			&SplitOptions {
				command: strings(&["hunk"]),
				cwd: cwd.to_path_buf(),
				label: Some("review".into()),
				..SplitOptions::default()
			},
		)
		.await
		.unwrap();
	let split = fake.panes().await.unwrap().into_iter().find(|p| p.pane_id == beside).unwrap();
	assert_eq!((split.tab_id, split.label.as_deref()), (old.tab_id, Some("review")));
	let popup = PluginPaneOptions {
		plugin_id: "jocasta".into(),
		entrypoint: "popup".into(),
		placement: Placement::Popup,
		target_pane_id: None,
		direction: None,
		cwd: None,
		env: Some(env(&[("P", "1")])),
		focus: None,
	};
	assert_eq!(fake.open_plugin_pane(&popup).await.unwrap(), None);
	assert_eq!(fake.state().popup, Some(env(&[("P", "1")])));
	assert!(fake.close_popup().await.unwrap());
	assert!(!fake.close_popup().await.unwrap());
	fake.state().roots.insert(respawned.pane_id.clone(), 7);
	assert_eq!(fake.pane_root(&respawned.pane_id, None).await.unwrap(), Some(7));
	fake.close_workspace(&first).await.unwrap();
	assert!(fake.panes().await.unwrap().iter().all(|p| p.workspace_id != first));
	fake.stop_server().await.unwrap();
	assert!(fake.snapshot().await.unwrap().workspaces.is_empty());
	assert_eq!(fake.methods().last().map(String::as_str), Some("stopServer"));
	assert_eq!(
		fake.notify(&Note { title: "t".into(), body: None, sound: Sound::None }).await.unwrap(),
		NotificationResult::Shown
	);
}
