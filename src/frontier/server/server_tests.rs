//! Tests of `server.rs`: the cases of `test/frontier/server/server.test.ts` that need a running server (the pure ones
//! are in `page_routes_tests.rs`, `routes_tests.rs` and `close_tests.rs`), and the server-level cases of
//! `pane-hook-env.test.ts`. The pieces of `shutdown.test.ts`, `popups.test.ts` and
//! `weekly-scorecard.test.ts` are in their own files beside it.

use std::fs;
use std::net::TcpListener;

use serde_json::{Value, json};

use super::*;
use crate::frontier::protocol::{ServerMessage, token_cookie};
use crate::frontier::server::helpers::{
	Page, boss_pane, http, start_test_server, until, with_server, with_test_defaults,
};
use crate::frontier::server::launch::{ServiceState, service_state};
use crate::frontier::server::surface::AgentState;
use crate::frontier::server::surface::fake_surface::FakeSurface;
use crate::shared::fsx::free_port;
use crate::shared::service_file::{read_service_file, service_file_path};
use crate::shared::testing::temp_jocasta;

fn bound(port: u16) -> TcpListener {
	TcpListener::bind(("127.0.0.1", port)).expect("the port is free again")
}

async fn status(request: reqwest::RequestBuilder) -> u16 {
	request.send().await.expect("an answer").status().as_u16()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_takes_the_token_as_a_bearer_only_a_pages_one_time_code_becomes_its_own_per_port_cookie_never_the_token()
 {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let base = format!("http://127.0.0.1:{}", server.port);
		let client = http();
		let get = |path: &str| client.get(format!("{base}{path}"));
		assert_eq!(status(get("/")).await, 403);
		assert_eq!(status(get("/health")).await, 403);
		assert_eq!(status(get("/ws").header("upgrade", "websocket")).await, 403);
		assert_eq!(status(get(&format!("/?t={}", "0".repeat(64)))).await, 403);
		// A token of the right length in characters but not in bytes is refused, not an error (SE-3).
		assert_eq!(status(get(&format!("/?t={}", "%C3%A9".repeat(64)))).await, 403);
		// The API token is no page code, and no cookie.
		assert_eq!(status(get(&format!("/?t={}", server.token))).await, 403);
		let token_cookie_header = format!("{}={}", token_cookie(server.port), server.token);
		assert_eq!(status(get("/health").header("cookie", &token_cookie_header)).await, 403);
		assert_eq!(status(get("/health").header("authorization", "Bearer wrong")).await, 403);

		let url = server.server.page_url("/brief/show-1");
		let shape = regex_lite_code(&url, &base);
		assert!(shape, "{url}");
		assert!(!url.contains(&server.token));
		let login = client.get(&url).send().await.unwrap();
		assert_eq!(login.status().as_u16(), 302);
		assert_eq!(login.headers()["location"], "/brief/show-1");
		let set_cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
		assert!(set_cookie.starts_with(&format!("{}=", token_cookie(server.port))), "{set_cookie}");
		assert!(set_cookie.contains("HttpOnly"));
		assert!(!set_cookie.contains(&server.token));
		// The code is single use.
		assert_eq!(status(client.get(&url)).await, 403);
		let more = client.get(format!("{}&x=1", server.server.page_url("/brief/show-1"))).send().await.unwrap();
		assert_eq!(more.headers()["location"], "/brief/show-1?x=1");
		let cookie = set_cookie.split(';').next().unwrap().to_owned();

		// The page cookie opens the pages, nothing of the API.
		assert_eq!(status(get("/brief/show-9.json").header("cookie", &cookie)).await, 404);
		assert_eq!(status(get("/brief/show-9.json")).await, 403);
		// A brief's action from another origin is refused (0678).
		let evil = client
			.post(format!("{base}/brief/show-9/act"))
			.header("cookie", &cookie)
			.header("origin", "http://evil.test")
			.header("content-type", "application/json")
			.body("{}");
		assert_eq!(status(evil).await, 403);
		assert_eq!(status(get("/health").header("cookie", &cookie)).await, 403);
		assert_eq!(status(get("/ws").header("cookie", &cookie).header("upgrade", "websocket")).await, 403);

		let health: Value = get("/health").bearer_auth(&server.token).send().await.unwrap().json().await.unwrap();
		assert_eq!(health["pid"], std::process::id());
		assert_eq!(health["layer"], "on");
		assert_eq!(health["hosts"], "on");

		// A cross-site page can't open the socket.
		let cross_site = get("/ws").bearer_auth(&server.token).header("origin", "http://example.org");
		assert_eq!(status(cross_site.header("upgrade", "websocket")).await, 403);

		let page = Page::open(&server, "").await;
		let snapshot = page.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. })).await;
		let ServerMessage::Snapshot { hud, .. } = snapshot else { unreachable!() };
		assert_eq!((hud.running, hud.blocked, hud.spend_today), (0, 0, Some(0.0)));
		assert_ne!(hud.quiet, Some(true));
		page.close();
	})
	.await;
}

/// Whether `url` is `<base>/?t=` and 64 lowercase hex digits.
fn regex_lite_code(url: &str, base: &str) -> bool {
	url.strip_prefix(&format!("{base}/brief/show-1?t=")).is_some_and(|code| {
		code.len() == 64 && code.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
	})
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browsers_page_from_an_earlier_run_gets_a_page_saying_so_and_what_to_do_with_no_token_in_it() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let response = http()
			.get(format!("http://127.0.0.1:{}/?review=r-1", server.port))
			.header("accept", "text/html,application/xhtml+xml")
			.header("cookie", format!("{}=stale", token_cookie(server.port)))
			.send()
			.await
			.unwrap();
		assert_eq!(response.status().as_u16(), 403);
		assert!(response.headers()["content-type"].to_str().unwrap().contains("text/html"));
		let html = response.text().await.unwrap();
		assert!(html.contains(
			"This Jocasta page is from an earlier run. Ask the boss to show it again; it opens a fresh page."
		));
		assert!(!html.contains(&server.token));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_that_fails_after_the_server_listens_stops_it_the_port_is_free_again() {
	let _j = temp_jocasta();
	let port = free_port().unwrap();
	// jocasta-service.json can't be written: a folder holds its name.
	fs::create_dir_all(service_file_path(None)).unwrap();
	fs::create_dir_all(crate::shared::paths::data_dir().join("omp-sessions")).unwrap();
	let herdr = Arc::new(FakeSurface::new());
	let options = ServerOptions { port: Some(port), ..ServerOptions::default() };
	assert!(start_server(with_test_defaults(options, &herdr)).await.is_err());
	drop(bound(port));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_removed_mcp_cli_and_push_routes_answer_404_only_health_and_shutdown_remain() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let client = http();
		for path in ["api/mcp/today", "api/cli", "api/push", "api/pane-check"] {
			let request = client.post(format!("http://127.0.0.1:{}/{path}", server.port)).bearer_auth(&server.token);
			assert_eq!(status(request).await, 404, "{path}");
		}
		let health = client.get(format!("http://127.0.0.1:{}/health", server.port)).bearer_auth(&server.token);
		assert_eq!(status(health).await, 200);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_its_own_host_is_answered_dns_rebinding_a_containers_host_docker_internal_no_longer() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let base = format!("http://127.0.0.1:{}", server.port);
		let client = http();
		for host in [format!("evil.test:{}", server.port), format!("host.docker.internal:{}", server.port)] {
			let request = client.get(format!("{base}/health")).bearer_auth(&server.token).header("host", host);
			assert_eq!(status(request).await, 403);
		}
		let local = client
			.get(format!("{base}/health"))
			.bearer_auth(&server.token)
			.header("host", format!("localhost:{}", server.port));
		assert_eq!(status(local).await, 200);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn jocasta_service_json_names_the_server_every_client_gets_the_state_feed_shutdown_stops_it() {
	let _j = temp_jocasta();
	let server = start_test_server(ServerOptions::default()).await;
	let file = read_service_file().expect("the service file");
	assert_eq!((file.port, file.token.as_str(), file.pid), (server.port, server.token.as_str(), std::process::id()));
	assert_eq!(service_state(Some(&file), None).await, ServiceState::Up);

	let page = Page::open(&server, "").await;
	let second = Page::open(&server, "").await;
	page.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. })).await;
	second.wait_for(|message| matches!(message, ServerMessage::Snapshot { .. })).await;
	assert_eq!(server.shutdown(true).await, CloseOutcome::Stopped);
	server.stopped().await;
	assert!(!service_file_path(None).exists());
	page.close();
	second.close();
	server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn right_ctrls_toggle_goes_to_the_boss_pane_as_omps_dictation_key() {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let boss = server.herdr.state().pane_list[0].pane_id.clone();
		server.service.toggle_dictation().await.unwrap();
		assert_eq!(server.herdr.of("sendKeys"), vec![vec![json!(boss), json!(["f9"])]]);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn service_port_is_tried_first_when_busy_the_service_takes_a_free_port() {
	let _j = temp_jocasta();
	let blocker = bound(0);
	let busy = blocker.local_addr().unwrap().port();
	with_server(ServerOptions { port: Some(busy), ..ServerOptions::default() }, |server| async move {
		assert_ne!(server.port, busy);
		assert_eq!(read_service_file().map(|file| file.port), Some(server.port));
	})
	.await;
	drop(blocker);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_boss_pane_is_made_first_in_workspace_jocasta_in_data_home_once_with_boss_yml_and_its_manual_as_append_system_md()
 {
	let _j = temp_jocasta();
	with_server(ServerOptions::default(), |server| async move {
		let made = server.herdr.of("ensureWorkspace");
		assert_eq!(made.len(), 1);
		let label = made[0][0].as_str().unwrap();
		let cwd = std::path::PathBuf::from(made[0][1].as_str().unwrap());
		let command: Vec<String> = serde_json::from_value(made[0][2].clone()).unwrap();
		assert_eq!(label, "Jocasta");
		assert!(cwd.ends_with("home"), "{}", cwd.display());
		assert_eq!(command[..3], ["omp", "--continue", "--config"]);
		let boss_yml = std::path::PathBuf::from(&command[3]);
		assert!(boss_yml.ends_with("boss.yml"));
		assert!(boss_yml.exists());
		let manual = cwd.join(".omp").join("APPEND_SYSTEM.md");
		let shipped = boss_yml.parent().unwrap().join("boss-manual.md");
		assert_eq!(fs::read_to_string(manual).unwrap(), fs::read_to_string(shipped).unwrap());
		assert_eq!(server.herdr.state().workspace_list[0].label, "Jocasta");
		assert_eq!(server.service.boss_state(), AgentState::Unknown);
		assert!(boss_pane(&server.herdr).is_some());
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_boss_pane_starts_with_the_pre_push_hook_in_gits_environment_and_its_own_pane_key() {
	let mut j = temp_jocasta();
	j.set_var("GIT_CONFIG_COUNT", None);
	with_server(ServerOptions::default(), |server| async move {
		until(|| boss_pane(&server.herdr).is_some()).await;
		let boss = boss_pane(&server.herdr).unwrap();
		let env = server.herdr.env_of(&boss).expect("the boss pane's env");
		let of = |name: &str| env.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
		assert_eq!(of("GIT_CONFIG_COUNT").as_deref(), Some("3"));
		assert_eq!(of("GIT_CONFIG_VALUE_2").as_deref(), Some("true"));
		assert!(of("JOCASTA_PANE_KEY").is_some_and(|key| !key.is_empty()));
	})
	.await;
}
