//! whiteboard's runtime in Jocasta (0393, 0411, 0417, 0419, 0552, 0615): its Desktop server (`dist/server/desktop-host.js`,
//! the server whiteboard's own Desktop runs, under plain Node) runs as the service's child with telemetry off and its
//! state in `<data>/whiteboard`; the service's canvas page (`src/frontier/canvas/`, opened in his browser) mounts its
//! canvas (`@dev.fast/review-canvas`) and talks to that server directly on loopback with the server's token. The
//! Desktop server serves no page of its own, only its API. The Desktop server rather than `whiteboard server start`,
//! because only it relays the canvas's verbs (`/control`) that let an agent open a review here. Agents author through
//! `whiteboard mcp` (set up by `jocasta setup` with `DEV_REVIEW_HOME` only), which finds the same server as whiteboard's
//! Desktop instance `stable` (`<home>/review-desktop/instances/stable.json`).
//!
//! Installed, everything sits beside `jocasta.exe`: `node/node.exe` (Node 24, pinned in scripts/build.ts),
//! `whiteboard/` (whiteboard's runtime package as npm installs it, CLI at `dist/cli.js`, the Desktop server at
//! `dist/server/desktop-host.js`) and `whiteboard-canvas/` (the canvas build).
//! Monaco's worker (0411) is embedded in the binary (0639). A source run uses `node` on PATH and the clone in
//! `forks/whiteboard` after its build.

use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderValue, header};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::watch;

use crate::frontier::protocol::ServiceState;
use crate::frontier::server::assets::MONACO_WORKER;
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::free_port;
use crate::shared::install::{REPO, install_dir};
use crate::shared::paths::data_dir;
use crate::shared::process::on_path;
use crate::shared::proctree::kill_tree;

/// whiteboard's telemetry off (its README; 0393).
pub const WHITEBOARD_ENV: [(&str, &str); 2] = [("DO_NOT_TRACK", "1"), ("DEV_FAST_REVIEW_TELEMETRY_DISABLED", "1")];

/// The pinned whiteboard release (0393, 0417; plan appendix B): the canvas's `appVersion`.
pub const WHITEBOARD_VERSION: &str = "0.5.2";

/// The repo's clone of whiteboard v0.5.2 (0417, 0615), relative to the repo root.
fn whiteboard_fork() -> PathBuf {
	Path::new("forks").join("whiteboard")
}

/// Where the page loads the canvas and Monaco's worker from (served by `server.rs`).
pub const CANVAS_PATH: &str = "/whiteboard/canvas/";
pub const WORKER_PATH: &str = "/whiteboard/editor.worker.js";
/// The page's `GET` for the server's url and token and the canvas's files.
pub const CONNECTION_PATH: &str = "/whiteboard/connection";

/// How to run whiteboard's CLI: a Node 24 binary and `cli.js`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhiteboardRuntime {
	pub node: PathBuf,
	pub cli: PathBuf,
}

/// whiteboard's state dir, `<data>/whiteboard` (0393).
fn whiteboard_home() -> PathBuf {
	data_dir().join("whiteboard")
}

/// A path as Node's `path.normalize` leaves it: `.` and `..` folded, one kind of separator.
fn normalize(path: &Path) -> PathBuf {
	let mut out = PathBuf::new();
	for part in path.components() {
		match part {
			Component::CurDir => {}
			Component::ParentDir => {
				if !out.pop() {
					out.push("..");
				}
			}
			other => out.push(other),
		}
	}
	out
}

/// whiteboard's CLI: in the install (`<install dir>/whiteboard/dist/cli.js` with `<install dir>/node/node.exe`), or the
/// repo's built clone with `node` on PATH. `None` when none is there (a source run before the clone is built).
pub fn whiteboard_runtime() -> Option<WhiteboardRuntime> {
	if let Some(dir) = install_dir(None) {
		let cli = dir.join("whiteboard").join("dist").join("cli.js");
		let node = dir.join("node").join(if cfg!(windows) { "node.exe" } else { "node" });
		if cli.exists() && node.exists() {
			return Some(WhiteboardRuntime { node: normalize(&node), cli: normalize(&cli) });
		}
	}
	let cli = Path::new(REPO).join(whiteboard_fork()).join("packages").join("review").join("dist").join("cli.js");
	cli.exists()
		.then(|| WhiteboardRuntime { node: on_path("node", None).unwrap_or_else(|| PathBuf::from("node")), cli })
}

/// What the page needs: the server's loopback origin and token, and the canvas's built files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhiteboardConnection {
	pub url: String,
	pub token: String,
	/// The server's pid (the acceptance check lists its sockets).
	pub pid: u32,
}

/// A service's state as the dashboard's services row says it, without the service's name (`ServiceHealth` minus `name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
	pub state: ServiceState,
	pub note: Option<String>,
}

/// Which whiteboard CLI [`start_whiteboard`] runs: the TypeScript's `runtime` option, whose default is
/// [`whiteboard_runtime`] and `null` none (test seam).
#[derive(Debug, Clone, Default)]
pub enum RuntimeChoice {
	/// Found with [`whiteboard_runtime`].
	#[default]
	Found,
	/// There is none.
	Absent,
	/// This one.
	Given(WhiteboardRuntime),
}

#[derive(Debug, Clone, Default)]
pub struct WhiteboardOptions {
	/// whiteboard's CLI.
	pub runtime: RuntimeChoice,
	/// whiteboard's state dir; `<data>/whiteboard` by default.
	pub home: Option<PathBuf>,
	/// How long the server may take to come up (ms); 30 s by default.
	pub timeout_ms: Option<i64>,
	/// The loopback port; a free one by default.
	pub port: Option<u16>,
}

struct Discovery {
	url: String,
	token: String,
	server_pid: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryFile {
	url: Option<Value>,
	token: Option<Value>,
	server_pid: Option<Value>,
}

/// whiteboard's Desktop instance `stable` on this state: where `whiteboard mcp` finds it too.
fn read_discovery(home: &Path) -> Option<Discovery> {
	let text = std::fs::read_to_string(home.join("review-desktop").join("instances").join("stable.json")).ok()?;
	let value: DiscoveryFile = serde_json::from_str(&text).ok()?;
	let url = url::Url::parse(value.url?.as_str()?).ok()?;
	if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
		return None;
	}
	let pid = value.server_pid?.as_f64()?;
	Some(Discovery {
		url: url.origin().ascii_serialization(),
		token: value.token?.as_str()?.to_owned(),
		server_pid: (pid >= 0.0 && pid <= f64::from(u32::MAX)).then_some(pid as u32)?,
	})
}

/// The recorded server answers `/health` with its token: one this service didn't start may still be running.
async fn healthy(discovery: &Discovery) -> bool {
	let Ok(client) = reqwest::Client::builder().timeout(Duration::from_millis(1_500)).build() else { return false };
	client
		.get(format!("{}/health", discovery.url))
		.header("x-review-token", &discovery.token)
		.send()
		.await
		.is_ok_and(|response| response.status().is_success())
}

/// What the supervisor settles `ready` with: the connection, or why the server couldn't start.
type Ready = Option<std::result::Result<WhiteboardConnection, String>>;

/// The server this service started: its pid while it runs, and a flag set when it has exited.
#[derive(Default)]
struct ChildState {
	pid: Option<u32>,
}

/// A whiteboard server this service started, or found.
pub struct Whiteboard {
	origin: Arc<Mutex<String>>,
	ready: watch::Receiver<Ready>,
	home: PathBuf,
	child: Arc<Mutex<ChildState>>,
	exited: watch::Receiver<bool>,
}

/// Settles `ready` once: the first outcome stands.
fn settle(ready: &watch::Sender<Ready>, outcome: std::result::Result<WhiteboardConnection, String>) {
	ready.send_if_modified(|current| {
		if current.is_some() {
			return false;
		}
		*current = Some(outcome);
		true
	});
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Whiteboard {
	/// The server's loopback origin, known before it listens (the page's CSP names it, 0393): the port this service
	/// chose, or the running server's when one was already up.
	pub fn origin(&self) -> String {
		locked(&self.origin).clone()
	}

	/// Resolves once the server answers; fails with why it couldn't start.
	pub async fn ready(&self) -> Result<WhiteboardConnection> {
		let mut ready = self.ready.clone();
		let settled = ready
			.wait_for(Option::is_some)
			.await
			.map_err(|_| Error::other("whiteboard's server start was abandoned"))?
			.clone();
		settled.unwrap_or(Err(String::new())).map_err(Error::other)
	}

	/// Whether the server is running and answers now, with why not (the dashboard's services row).
	pub async fn check(&self) -> ServiceStatus {
		let status = |state, note: Option<&str>| ServiceStatus { state, note: note.map(str::to_owned) };
		let settled = self.ready.borrow().clone();
		match settled {
			None => status(ServiceState::Starting, None),
			Some(Err(_)) => status(ServiceState::Down, Some("not started")),
			Some(Ok(_)) => match read_discovery(&self.home) {
				Some(discovery) if healthy(&discovery).await => status(ServiceState::Up, None),
				_ => status(ServiceState::Down, Some("not answering")),
			},
		}
	}

	/// Stops the server this service started (Quit, 0357): its whole process tree, since Node may sit behind a shim.
	pub async fn stop(&self) {
		let Some(pid) = locked(&self.child).pid else { return };
		let mut exited = self.exited.clone();
		kill_tree(pid).await;
		let _ = tokio::time::timeout(Duration::from_secs(10), exited.wait_for(|done| *done)).await;
	}
}

/// Keeps the last 4000 characters of `text` (the TypeScript's `slice(-4000)`).
fn tail(text: String) -> String {
	let count = text.chars().count();
	if count <= 4_000 { text } else { text.chars().skip(count - 4_000).collect() }
}

/// The environment of whiteboard's Desktop server: the process's own, telemetry off, its state and port.
fn server_command(runtime: &WhiteboardRuntime, home: &Path, port: u16) -> tokio::process::Command {
	let host = runtime.cli.parent().unwrap_or(Path::new("")).join("server").join("desktop-host.js");
	let mut command = tokio::process::Command::new(&runtime.node);
	command.arg(host).current_dir(home);
	env::apply_to_tokio(&mut command);
	for (name, value) in WHITEBOARD_ENV {
		command.env(name, value);
	}
	command
		.env("DEV_REVIEW_HOME", home)
		.env("DEV_FAST_REVIEW_SERVER_PORT", port.to_string())
		// It hands this pid to agents as the Desktop app's.
		.env("DEV_FAST_REVIEW_APP_PID", std::process::id().to_string())
		// A headless state dir or a dev checkout's identity would point it, and `whiteboard mcp`, elsewhere.
		.env_remove("DEV_REVIEW_SERVER_DIR")
		.env_remove("DEV_FAST_REVIEW_CHECKOUT")
		.env_remove("DEV_FAST_REVIEW_CHANNEL")
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	#[cfg(windows)]
	command.creation_flags(0x0800_0000);
	command
}

/// Everything the supervisor of one start shares with its [`Whiteboard`].
struct Supervisor {
	ready: watch::Sender<Ready>,
	child: Arc<Mutex<ChildState>>,
	exited: watch::Sender<bool>,
	home: PathBuf,
	timeout_ms: i64,
}

/// The `ready` line of the Desktop server's stdout: `{"event":"ready",…}`.
fn is_ready_line(line: &str) -> bool {
	serde_json::from_str::<Map<String, Value>>(line.trim())
		.is_ok_and(|event| event.get("event") == Some(&Value::String("ready".into())))
}

impl Supervisor {
	/// Starts the Desktop server and watches it until it exits: settles `ready` with its connection once its `ready`
	/// line says it is up, or with why it wasn't.
	async fn run(self, mut command: tokio::process::Command) {
		let mut child = match command.spawn() {
			Ok(child) => child,
			Err(error) => {
				settle(&self.ready, Err(format!("whiteboard's server didn't start: {error}")));
				return;
			}
		};
		locked(&self.child).pid = child.id();
		let output = Arc::new(Mutex::new(String::new()));
		let errors = child.stderr.take().map(|mut stderr| {
			let output = output.clone();
			tokio::spawn(async move {
				let mut chunk = [0_u8; 4096];
				while let Ok(read) = stderr.read(&mut chunk).await {
					if read == 0 {
						break;
					}
					let mut said = locked(&output);
					let all = format!("{}{}", said, String::from_utf8_lossy(&chunk[..read]));
					*said = tail(all);
				}
			})
		});
		let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
		let deadline = tokio::time::sleep(Duration::from_millis(self.timeout_ms.max(0).unsigned_abs()));
		tokio::pin!(deadline);
		let (mut stdout_open, mut timed_out) = (true, false);
		let status = loop {
			tokio::select! {
				line = lines.next_line(), if stdout_open => match line {
					Ok(Some(line)) if is_ready_line(&line) => self.say_ready(),
					Ok(Some(_)) => {}
					Ok(None) | Err(_) => stdout_open = false,
				},
				() = &mut deadline, if !timed_out => {
					timed_out = true;
					let said = locked(&output).trim().to_owned();
					settle(&self.ready, Err(format!("whiteboard's server didn't start in time: {said}")));
				}
				status = child.wait() => break status,
			}
		};
		if let Some(errors) = errors {
			let _ = errors.await;
		}
		locked(&self.child).pid = None;
		let code =
			status.ok().and_then(|status| status.code()).map_or_else(|| "null".to_owned(), |code| code.to_string());
		let said = locked(&output).trim().to_owned();
		settle(&self.ready, Err(format!("whiteboard's server exited {code}: {said}")));
		let _ = self.exited.send(true);
	}

	fn say_ready(&self) {
		match read_discovery(&self.home) {
			Some(found) => settle(
				&self.ready,
				Ok(WhiteboardConnection { url: found.url, token: found.token, pid: found.server_pid }),
			),
			None => settle(
				&self.ready,
				Err(format!("whiteboard's server is up but {} has no readable discovery file", self.home.display())),
			),
		}
	}
}

/// Why no server was started: whiteboard isn't built.
fn not_built() -> String {
	format!(
		"whiteboard isn't built: run `mise run build` (it builds {}), or install Jocasta's release",
		whiteboard_fork().display()
	)
}

/// Brings up the server for [`start_whiteboard`], or settles `ready` at once when there is nothing to start or a
/// healthy server already runs on this state.
async fn bring_up(runtime: Option<WhiteboardRuntime>, supervisor: Supervisor, origin: Arc<Mutex<String>>, port: u16) {
	let Some(runtime) = runtime else {
		settle(&supervisor.ready, Err(not_built()));
		return;
	};
	let home = supervisor.home.clone();
	if let Err(error) = std::fs::create_dir_all(&home) {
		settle(&supervisor.ready, Err(error.to_string()));
		return;
	}
	// Another service's server on this state: whiteboard's Desktop server takes no lock, so ask it first.
	if let Some(existing) = read_discovery(&home)
		&& healthy(&existing).await
	{
		*locked(&origin) = existing.url.clone();
		let found = WhiteboardConnection { url: existing.url, token: existing.token, pid: existing.server_pid };
		settle(&supervisor.ready, Ok(found));
		return;
	}
	let command = server_command(&runtime, &home, port);
	supervisor.run(command).await;
}

/// Starts `node <whiteboard>/dist/server/desktop-host.js` with `home` as its state (0393, 0419): its `ready` line on
/// stdout carries the url, token and pid, and `<home>/review-desktop/instances/stable.json` the same for
/// `whiteboard mcp`. A healthy server another Jocasta run left on the same state is used as is. Runs on the current
/// tokio runtime.
pub fn start_whiteboard(options: WhiteboardOptions) -> Whiteboard {
	let runtime = match options.runtime {
		RuntimeChoice::Found => whiteboard_runtime(),
		RuntimeChoice::Absent => None,
		RuntimeChoice::Given(runtime) => Some(runtime),
	};
	let home = options.home.unwrap_or_else(whiteboard_home);
	let port = options.port.or_else(|| free_port().ok()).unwrap_or(0);
	let origin = Arc::new(Mutex::new(format!("http://127.0.0.1:{port}")));
	let child = Arc::new(Mutex::new(ChildState::default()));
	let (ready_tx, ready) = watch::channel(None);
	let (exited_tx, exited) = watch::channel(false);
	let supervisor = Supervisor {
		ready: ready_tx,
		child: child.clone(),
		exited: exited_tx,
		home: home.clone(),
		timeout_ms: options.timeout_ms.unwrap_or(30_000),
	};
	tokio::spawn(bring_up(runtime, supervisor, origin.clone(), port));
	Whiteboard { origin, ready, home, child, exited }
}

/// The canvas build: `.vite/manifest.json` and `assets/`, beside `jocasta.exe` or in the built clone.
pub fn canvas_dir() -> Option<PathBuf> {
	let installed = install_dir(None).map(|dir| dir.join("whiteboard-canvas"));
	let built = Path::new(REPO)
		.join(whiteboard_fork())
		.join("packages")
		.join("review")
		.join("app")
		.join("dist")
		.join("desktop");
	installed.into_iter().chain([built]).find(|candidate| candidate.join(".vite").join("manifest.json").exists())
}

/// The canvas entry's files, from vite's manifest, as URLs under [`CANVAS_PATH`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasFiles {
	pub script: String,
	pub styles: Vec<String>,
	pub wasm: String,
}

/// One chunk of vite's manifest.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Chunk {
	file: String,
	is_entry: Option<bool>,
	name: Option<String>,
	css: Option<Vec<String>>,
}

/// The entry, its stylesheets and the router's wasm. In 0.5.2 the wasm belongs to a lazily loaded chunk, not to the
/// entry (0.2.4's entry listed it): it is the build's one `.wasm` file, wherever the manifest lists it (0615).
pub fn canvas_files(dir: &Path) -> Result<CanvasFiles> {
	let text = std::fs::read_to_string(dir.join(".vite").join("manifest.json"))?;
	let manifest: Map<String, Value> = serde_json::from_str(&text)?;
	let chunks: Vec<Chunk> = manifest.into_values().filter_map(|chunk| serde_json::from_value(chunk).ok()).collect();
	let entry = chunks.iter().find(|chunk| chunk.is_entry == Some(true) && chunk.name.as_deref() == Some("canvas"));
	let wasm = chunks.iter().find(|chunk| chunk.file.ends_with(".wasm")).map(|chunk| chunk.file.as_str());
	let (Some(entry), Some(wasm)) = (entry, wasm) else {
		return Err(Error::other(format!("{}: the canvas manifest has no canvas entry or no wasm", dir.display())));
	};
	Ok(CanvasFiles {
		script: format!("{CANVAS_PATH}{}", entry.file),
		styles: entry.css.iter().flatten().map(|file| format!("{CANVAS_PATH}{file}")).collect(),
		wasm: format!("{CANVAS_PATH}{wasm}"),
	})
}

/// The animation stage (0615): `assets/stage-<hash>.html` in the canvas build, the document the canvas loads into a
/// sandboxed frame to play an animation. The only `.html` the build has under `assets/`.
pub fn is_stage_file(pathname: &str) -> bool {
	pathname.starts_with(&format!("{CANVAS_PATH}assets/")) && pathname.ends_with(".html")
}

/// A file of the canvas build to serve, and the headers it is served with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasAsset {
	pub path: PathBuf,
	/// `content-type`.
	pub content_type: &'static str,
	/// `cache-control`: vite names every file by its content, so the window keeps it for a year (PM-3).
	pub cache_control: &'static str,
}

fn content_type_of(path: &Path) -> &'static str {
	match path.extension().and_then(|ext| ext.to_str()) {
		Some("js") => "text/javascript; charset=utf-8",
		Some("html") => "text/html; charset=utf-8",
		Some("css") => "text/css; charset=utf-8",
		Some("wasm") => "application/wasm",
		Some("woff2") => "font/woff2",
		Some("svg") => "image/svg+xml",
		Some("png") => "image/png",
		_ => "application/octet-stream",
	}
}

/// A file of the canvas build under [`CANVAS_PATH`]; `None` for anything outside `assets/`. vite names every file
/// there by its content, so the window keeps it for a year (PM-3).
pub fn canvas_asset(dir: &Path, pathname: &str) -> Option<CanvasAsset> {
	let encoded = pathname.get(CANVAS_PATH.len()..)?;
	let rel = percent_decode(encoded);
	// The path joined and normalized, lexically: it must stay under `<dir>/assets/`.
	let mut parts: Vec<&str> = Vec::new();
	for part in rel.split(|c| c == '/' || (cfg!(windows) && c == '\\')) {
		match part {
			"" | "." => {}
			".." => {
				parts.pop()?;
			}
			other => parts.push(other),
		}
	}
	if parts.len() < 2 || parts[0] != "assets" {
		return None;
	}
	let path = parts.iter().fold(dir.to_path_buf(), |path, part| path.join(part));
	path.is_file().then(|| CanvasAsset {
		content_type: content_type_of(&path),
		cache_control: "public, max-age=31536000, immutable",
		path,
	})
}

/// `decodeURIComponent`: `%XX` sequences as UTF-8 bytes (malformed ones stay as written).
fn percent_decode(text: &str) -> String {
	let bytes = text.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut i = 0;
	while i < bytes.len() {
		let hex = |at: usize| bytes.get(at).and_then(|b| char::from(*b).to_digit(16));
		match (bytes[i], hex(i + 1), hex(i + 2)) {
			(b'%', Some(high), Some(low)) => {
				out.push((high * 16 + low) as u8);
				i += 3;
			}
			(byte, _, _) => {
				out.push(byte);
				i += 1;
			}
		}
	}
	String::from_utf8_lossy(&out).into_owned()
}

/// The `content-type` of Monaco's editor worker.
pub const WORKER_CONTENT_TYPE: &str = "text/javascript; charset=utf-8";

/// Monaco's editor worker (0411), served at [`WORKER_PATH`] with [`WORKER_CONTENT_TYPE`] and no other header: the
/// bundle `build.rs` made with Bun, which the binary embeds (0639), so an install and a source run serve the same one.
pub fn monaco_worker() -> Response {
	let mut response = Response::new(Body::from(MONACO_WORKER));
	response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(WORKER_CONTENT_TYPE));
	response
}

#[cfg(test)]
#[path = "whiteboard_tests.rs"]
pub(crate) mod tests;
