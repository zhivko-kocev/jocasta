//! Tests of `whiteboard.rs`, with the stand-in for whiteboard's Desktop server this test executable plays.

use std::io::Write;

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::frontier::server::mise::tests::passthrough_shim;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// Stands in for `node <whiteboard>/dist/server/desktop-host.js` (0393, 0419): listens on
/// `DEV_FAST_REVIEW_SERVER_PORT`, writes `<DEV_REVIEW_HOME>/review-desktop/instances/stable.json`, says `ready`, serves
/// `/health`, and records its environment. It acts only when `FAKE_WHITEBOARD` is set; `FAKE_WHITEBOARD_FAIL` exits 3
/// at once.
#[test]
fn fake_whiteboard() {
	if env::var("FAKE_WHITEBOARD").is_none() {
		return;
	}
	if env::var("FAKE_WHITEBOARD_FAIL").is_some() {
		eprintln!("no room");
		std::process::exit(3);
	}
	let home = PathBuf::from(env::var("DEV_REVIEW_HOME").expect("DEV_REVIEW_HOME"));
	let port: u16 = env::var("DEV_FAST_REVIEW_SERVER_PORT").and_then(|port| port.parse().ok()).expect("the port");
	let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("a runtime");
	runtime.block_on(async move {
		let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.expect("listening");
		let discovery = json!({
			"version": 3,
			"url": format!("http://127.0.0.1:{port}"),
			"token": "wb-token",
			"serverPid": std::process::id(),
			"key": "stable",
		});
		let instances = home.join("review-desktop").join("instances");
		std::fs::create_dir_all(&instances).expect("the instances folder");
		std::fs::write(instances.join("stable.json"), discovery.to_string()).expect("the discovery file");
		let seen = json!({
			"argv": env::var("FAKE_ARGS").unwrap_or_default().replace('"', ""),
			"DO_NOT_TRACK": env::var("DO_NOT_TRACK"),
			"TELEMETRY": env::var("DEV_FAST_REVIEW_TELEMETRY_DISABLED"),
			"SERVER_DIR": env::var("DEV_REVIEW_SERVER_DIR"),
			"APP_PID": env::var("DEV_FAST_REVIEW_APP_PID"),
		});
		std::fs::write(home.join("env.json"), seen.to_string()).expect("the env record");
		let mut ready = json!({ "event": "ready" });
		ready.as_object_mut().expect("an object").extend(discovery.as_object().expect("an object").clone());
		println!("{ready}");
		std::io::stdout().flush().expect("flushing");
		// Until it is ended; a crashed test leaves it for two minutes at most.
		let serve = async {
			loop {
				let Ok((mut stream, _)) = listener.accept().await else { return };
				tokio::spawn(async move {
					let mut seen = Vec::new();
					let mut chunk = [0_u8; 1024];
					while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
						match stream.read(&mut chunk).await {
							Ok(0) | Err(_) => return,
							Ok(read) => seen.extend_from_slice(&chunk[..read]),
						}
					}
					let body = "{\"ok\":true}";
					let head = format!(
						"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
						body.len()
					);
					let _ = stream.write_all(head.as_bytes()).await;
				});
			}
		};
		let _ = tokio::time::timeout(Duration::from_secs(120), serve).await;
	});
}

/// A runtime whose `cli.js` sits beside `server/desktop-host.js`, as in whiteboard's `dist/`.
pub(crate) fn fake_runtime(temp: &TempJocasta) -> WhiteboardRuntime {
	let dist = temp.root().join("fake-whiteboard").join("dist");
	std::fs::create_dir_all(dist.join("server")).unwrap();
	std::fs::write(dist.join("server").join("desktop-host.js"), "").unwrap();
	let node = passthrough_shim(temp.root(), "node", "frontier::server::whiteboard::tests::fake_whiteboard");
	WhiteboardRuntime { node, cli: dist.join("cli.js") }
}

async fn health_ok(url: &str) -> bool {
	match reqwest::get(format!("{url}/health")).await {
		Ok(response) => response.status().is_success(),
		Err(_) => false,
	}
}

#[tokio::test]
async fn the_desktop_server_starts_with_telemetry_off_on_its_own_state_and_port_is_reused_while_healthy_and_stops_on_quit()
 {
	let mut temp = temp_jocasta();
	temp.set_var("FAKE_WHITEBOARD", Some("1"));
	let elsewhere = temp.root().join("elsewhere").display().to_string();
	temp.set_var("DEV_REVIEW_SERVER_DIR", Some(&elsewhere));
	let home = data_dir().join("whiteboard");
	let options = |temp: &TempJocasta| WhiteboardOptions {
		runtime: RuntimeChoice::Given(fake_runtime(temp)),
		home: Some(home.clone()),
		..WhiteboardOptions::default()
	};
	let whiteboard = start_whiteboard(options(&temp));
	let port: u16 = whiteboard.origin().rsplit(':').next().unwrap().parse().unwrap();
	let connection = whiteboard.ready().await.unwrap();
	assert_eq!(connection.url, format!("http://127.0.0.1:{port}"));
	assert_eq!(connection.token, "wb-token");
	let seen: Value = serde_json::from_str(&std::fs::read_to_string(home.join("env.json")).unwrap()).unwrap();
	let host = temp.root().join("fake-whiteboard").join("dist").join("server").join("desktop-host.js");
	assert_eq!(
		seen,
		json!({
			"argv": host.display().to_string(),
			"DO_NOT_TRACK": "1",
			"TELEMETRY": "1",
			"SERVER_DIR": null,
			"APP_PID": std::process::id().to_string(),
		})
	);
	assert!(health_ok(&connection.url).await);
	assert_eq!(whiteboard.check().await, ServiceStatus { state: ServiceState::Up, note: None });
	// A second service on the same state uses the running server.
	let second = start_whiteboard(options(&temp));
	assert_eq!(second.ready().await.unwrap(), connection);
	assert_eq!(second.origin(), connection.url);
	second.stop().await;
	assert!(health_ok(&connection.url).await);
	whiteboard.stop().await;
	assert!(!health_ok(&connection.url).await);
}

#[tokio::test]
async fn a_server_that_exits_says_why_a_missing_build_says_how_to_build_it() {
	let mut temp = temp_jocasta();
	temp.set_var("FAKE_WHITEBOARD", Some("1"));
	temp.set_var("FAKE_WHITEBOARD_FAIL", Some("1"));
	let home = data_dir().join("whiteboard");
	let failed = start_whiteboard(WhiteboardOptions {
		runtime: RuntimeChoice::Given(fake_runtime(&temp)),
		home: Some(home.clone()),
		..WhiteboardOptions::default()
	});
	assert_eq!(failed.ready().await.unwrap_err().to_string(), "whiteboard's server exited 3: no room");
	assert_eq!(failed.check().await, ServiceStatus { state: ServiceState::Down, note: Some("not started".into()) });
	let missing = start_whiteboard(WhiteboardOptions {
		runtime: RuntimeChoice::Absent,
		home: Some(home),
		..WhiteboardOptions::default()
	});
	let error = missing.ready().await.unwrap_err().to_string();
	assert!(error.starts_with("whiteboard isn't built: run `mise run build`"), "{error}");
}

#[test]
fn the_canvas_builds_entry_stylesheets_and_wasm_come_from_vites_manifest_files_outside_assets_arent_served() {
	let temp = temp_jocasta();
	let dir = temp.root().join("canvas");
	std::fs::create_dir_all(dir.join(".vite")).unwrap();
	std::fs::create_dir_all(dir.join("assets")).unwrap();
	let manifest = json!({
		// 0.5.2's shape: the entry lists no wasm; a lazily loaded chunk and the wasm's own entry do (0615).
		"src/desktop-entry.tsx": {
			"file": "assets/canvas-1.js",
			"name": "canvas",
			"isEntry": true,
			"css": ["assets/canvas-1.css"],
			"assets": ["assets/stage-1.html", "assets/font.woff2"],
		},
		"elkjs-libavoid/index.mjs": { "file": "assets/dist-1.js", "name": "dist", "assets": ["assets/libavoid-1.wasm"] },
		"libavoid-js/libavoid.wasm": { "file": "assets/libavoid-1.wasm" },
	});
	std::fs::write(dir.join(".vite").join("manifest.json"), manifest.to_string()).unwrap();
	std::fs::write(dir.join("assets").join("canvas-1.js"), "export {}").unwrap();
	assert_eq!(
		canvas_files(&dir).unwrap(),
		CanvasFiles {
			script: format!("{CANVAS_PATH}assets/canvas-1.js"),
			styles: vec![format!("{CANVAS_PATH}assets/canvas-1.css")],
			wasm: format!("{CANVAS_PATH}assets/libavoid-1.wasm"),
		}
	);
	let served = canvas_asset(&dir, &format!("{CANVAS_PATH}assets/canvas-1.js")).unwrap();
	assert!(served.content_type.contains("text/javascript"));
	assert_eq!(served.path, dir.join("assets").join("canvas-1.js"));
	assert_eq!(canvas_asset(&dir, &format!("{CANVAS_PATH}.vite/manifest.json")), None);
	assert_eq!(canvas_asset(&dir, &format!("{CANVAS_PATH}assets/..%2F..%2Fsecret")), None);
}

#[test]
fn the_animation_stages_file_is_served_as_html_and_a_neighbouring_asset_is_not_the_stage() {
	let temp = temp_jocasta();
	let dir = temp.root().join("canvas-stage");
	std::fs::create_dir_all(dir.join("assets")).unwrap();
	std::fs::write(dir.join("assets").join("stage-1.html"), "<!doctype html>").unwrap();
	std::fs::write(dir.join("assets").join("canvas-1.js"), "export {}").unwrap();
	let stage = format!("{CANVAS_PATH}assets/stage-1.html");
	let script = format!("{CANVAS_PATH}assets/canvas-1.js");
	let served = canvas_asset(&dir, &stage).unwrap();
	assert_eq!(served.content_type, "text/html; charset=utf-8");
	assert_eq!(served.cache_control, "public, max-age=31536000, immutable");
	assert!(is_stage_file(&stage));
	assert!(!is_stage_file(&script));
	assert!(!is_stage_file("/"));
}

#[tokio::test]
async fn monacos_worker_is_the_embedded_bundle_with_the_one_content_type_header_and_nothing_to_install() {
	let worker = monaco_worker();
	assert_eq!(worker.status(), axum::http::StatusCode::OK);
	assert_eq!(worker.headers().len(), 1);
	assert_eq!(worker.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
	let body = axum::body::to_bytes(worker.into_body(), usize::MAX).await.unwrap();
	assert!(!body.is_empty());
	assert_eq!(&body[..], MONACO_WORKER);
}
