//! omp's local host registry (0547, 0561): every omp hosting a collab room (Jocasta's panes, through its relay) has
//! `<home>/.omp/run/collab-hosts/<entry>.json` (`{version, instanceId, pid, endpoint, createdAt, token}`) and answers
//! one JSON line on that endpoint, a named pipe on Windows: `{"v":1,"token":…,"op":"snapshot"}` →
//! `{ok, v, snapshot: {pid, cwd, busy, inputRequired, relayConnected, …}}` (omp 19.1.0 `src/collab/registry.ts`,
//! protocol v1, documented for dashboards). Read directly: `omp collab list` takes about 0.7 s a call, a pipe 0–5 ms.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::shared::env;
use crate::shared::fsx::as_object;
use crate::shared::proctree::pid_alive;

/// The registry protocol Jocasta speaks (`COLLAB_REGISTRY_VERSION`).
const REGISTRY_VERSION: u64 = 1;

/// How long one host may take to answer before it counts as not answering.
const QUERY_TIMEOUT_MS: i64 = 1_000;

/// Answers beyond this are no snapshot (omp's own bound is 64 KiB).
const MAX_ANSWER: usize = 64 * 1024;

/// `~/.omp/run/collab-hosts` (omp's `collabHostsRuntimeDir`: `PI_CONFIG_DIR` renames `.omp`; no profile in it).
fn hosts_dir() -> PathBuf {
	let mut dir = std::env::home_dir().unwrap_or_default();
	let config = env::var_nonempty("PI_CONFIG_DIR").unwrap_or_else(|| ".omp".to_owned());
	// A path join as JavaScript's: an absolute name is appended, not a new root.
	for part in Path::new(&config).components() {
		if matches!(part, Component::Normal(_) | Component::ParentDir) {
			dir.push(part);
		}
	}
	dir.join("run").join("collab-hosts")
}

/// One host's state as omp reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostState {
	pub pid: u32,
	pub cwd: PathBuf,
	/// A turn running; `None` when the host doesn't say (an older omp): unknown, never idle.
	pub busy: Option<bool>,
	/// A dialog of the host's (a question, an approval) waiting for an answer.
	pub input_required: bool,
	pub relay_connected: bool,
}

struct Entry {
	pid: u32,
	endpoint: String,
	token: String,
}

/// A JSON number as a pid.
fn pid_of(value: Option<&Value>) -> Option<u32> {
	let n = value?.as_f64()?;
	(n >= 0.0 && n <= f64::from(u32::MAX)).then_some(n as u32)
}

fn entry_of(text: &str) -> Option<Entry> {
	let value: Value = serde_json::from_str(text).ok()?;
	let raw = as_object(&value)?;
	if raw.get("version").and_then(Value::as_f64) != Some(REGISTRY_VERSION as f64) {
		return None;
	}
	Some(Entry {
		pid: pid_of(raw.get("pid"))?,
		endpoint: raw.get("endpoint")?.as_str()?.to_owned(),
		token: raw.get("token")?.as_str()?.to_owned(),
	})
}

fn state_of(answer: &str) -> Option<HostState> {
	let value: Value = serde_json::from_str(answer).ok()?;
	let raw = as_object(&value)?;
	let snapshot =
		if raw.get("ok") == Some(&Value::Bool(true)) { as_object(raw.get("snapshot")?)? } else { return None };
	Some(HostState {
		pid: pid_of(snapshot.get("pid"))?,
		cwd: PathBuf::from(snapshot.get("cwd")?.as_str()?),
		busy: snapshot.get("busy").and_then(Value::as_bool),
		input_required: snapshot.get("inputRequired") == Some(&Value::Bool(true)),
		relay_connected: snapshot.get("relayConnected") == Some(&Value::Bool(true)),
	})
}

/// One snapshot request and its answer over an open endpoint.
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, token: &str) -> Option<HostState> {
	let request = json!({ "v": REGISTRY_VERSION, "token": token, "op": "snapshot" });
	stream.write_all(format!("{request}\n").as_bytes()).await.ok()?;
	let mut answer = Vec::new();
	let mut chunk = [0_u8; 4096];
	loop {
		let read = stream.read(&mut chunk).await.unwrap_or(0);
		answer.extend_from_slice(&chunk[..read]);
		if let Some(end) = answer.iter().position(|b| *b == b'\n') {
			return state_of(&String::from_utf8_lossy(&answer[..end]));
		}
		if read == 0 {
			// The host closed its end: what it said is the answer.
			return state_of(&String::from_utf8_lossy(&answer));
		}
		if answer.len() > MAX_ANSWER {
			return None;
		}
	}
}

#[cfg(windows)]
async fn connect_and_ask(entry: &Entry) -> Option<HostState> {
	use tokio::net::windows::named_pipe::ClientOptions;
	// ERROR_PIPE_BUSY: every instance is taken for the moment; the host frees one in a moment.
	const PIPE_BUSY: i32 = 231;
	let client = loop {
		match ClientOptions::new().open(&entry.endpoint) {
			Ok(client) => break client,
			Err(error) if error.raw_os_error() == Some(PIPE_BUSY) => {
				tokio::time::sleep(Duration::from_millis(10)).await
			}
			Err(_) => return None,
		}
	};
	exchange(client, &entry.token).await
}

#[cfg(unix)]
async fn connect_and_ask(entry: &Entry) -> Option<HostState> {
	let stream = tokio::net::UnixStream::connect(&entry.endpoint).await.ok()?;
	exchange(stream, &entry.token).await
}

/// One host's snapshot, or `None` when it doesn't answer one in time.
async fn query(entry: &Entry, timeout_ms: i64) -> Option<HostState> {
	let limit = Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
	tokio::time::timeout(limit, connect_and_ask(entry)).await.ok().flatten()
}

/// The `.json` files of `dir`; `None` when it can't be listed.
fn json_files(dir: &Path) -> Option<Vec<PathBuf>> {
	let names = std::fs::read_dir(dir).ok()?;
	Some(
		names
			.flatten()
			.filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
			.map(|entry| entry.path())
			.collect(),
	)
}

/// The hosts in `dir` (`~/.omp/run/collab-hosts` by default) whose process is one of `pids`, each as it answers now;
/// one that doesn't answer is left out. Other omps (his own, outside Jocasta) are never asked. omp's own listing
/// prunes stale entries; this never deletes. `timeout_ms` is how long one host may take (a second by default).
pub async fn read_hosts(pids: &HashSet<u32>, dir: Option<&Path>, timeout_ms: Option<i64>) -> Vec<HostState> {
	if pids.is_empty() {
		return Vec::new();
	}
	let dir = dir.map_or_else(hosts_dir, Path::to_path_buf);
	let Some(files) = json_files(&dir) else { return Vec::new() };
	let entries: Vec<Entry> = files
		.iter()
		.filter_map(|file| std::fs::read_to_string(file).ok())
		.filter_map(|text| entry_of(&text))
		.filter(|entry| pids.contains(&entry.pid))
		.collect();
	let timeout_ms = timeout_ms.unwrap_or(QUERY_TIMEOUT_MS);
	let states = futures_util::future::join_all(entries.iter().map(|entry| query(entry, timeout_ms))).await;
	states.into_iter().flatten().filter(|state| pids.contains(&state.pid)).collect()
}

/// Removes the entries in `dir` (`~/.omp/run/collab-hosts` by default) whose process is one of `pids` and no longer
/// runs (0547): Stop everything's omps, which omp's own listing would prune only later. An entry whose pid runs, or
/// that names another pid, stays. Returns how many went.
pub fn remove_host_entries(pids: &HashSet<u32>, dir: Option<&Path>) -> usize {
	let dir = dir.map_or_else(hosts_dir, Path::to_path_buf);
	let Some(files) = json_files(&dir) else { return 0 };
	let mut removed = 0;
	for file in files {
		let Some(entry) = std::fs::read_to_string(&file).ok().and_then(|text| entry_of(&text)) else { continue };
		if !pids.contains(&entry.pid) || pid_alive(entry.pid) {
			continue;
		}
		// Gone already, or held: omp's own listing prunes it later.
		if std::fs::remove_file(&file).is_ok() {
			removed += 1;
		}
	}
	removed
}

/// A stand-in for one omp host in omp's local registry (omp 18.22.0 `src/collab/registry.ts`, protocol v1): a metadata
/// file in `dir` and an endpoint (a named pipe on Windows, a socket elsewhere) answering one `snapshot` line for its
/// bearer token, with the state a test sets. It runs in-process on tokio.
#[cfg(test)]
pub(crate) mod fake {
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::sync::{Arc, Mutex};

	use super::*;

	#[derive(Clone)]
	pub(crate) struct FakeHostState {
		pub pid: u32,
		pub cwd: String,
		pub busy: Option<bool>,
		pub input_required: bool,
	}

	pub(crate) struct FakeHost {
		/// What the host answers from now on.
		pub state: Arc<Mutex<FakeHostState>>,
		asked: Arc<AtomicUsize>,
		meta: PathBuf,
		/// Its endpoint: on Unix a socket file, removed when the host is dropped.
		endpoint: String,
		task: Option<tokio::task::JoinHandle<()>>,
	}

	impl Drop for FakeHost {
		fn drop(&mut self) {
			if cfg!(unix) {
				let _ = std::fs::remove_file(&self.endpoint);
			}
		}
	}

	impl FakeHost {
		/// How many snapshots it answered.
		pub(crate) fn asked(&self) -> usize {
			self.asked.load(Ordering::SeqCst)
		}

		pub(crate) async fn close(&mut self) {
			let _ = std::fs::remove_file(&self.meta);
			if let Some(task) = self.task.take() {
				task.abort();
				let _ = task.await;
			}
		}
	}

	fn random_hex() -> String {
		uuid::Uuid::new_v4().simple().to_string()[..16].to_owned()
	}

	/// What the host answers one request line: the snapshot, or `authentication_failed`.
	fn answer(line: &str, token: &str, entry: &str, state: &FakeHostState, asked: &AtomicUsize) -> String {
		let request: Value = serde_json::from_str(line).unwrap_or(Value::Null);
		if request["v"] != 1 || request["token"] != token || request["op"] != "snapshot" {
			return format!("{}\n", json!({ "ok": false, "v": 1, "error": "authentication_failed" }));
		}
		asked.fetch_add(1, Ordering::SeqCst);
		let snapshot = json!({
			"instanceId": entry,
			"generation": 1,
			"sessionId": "s",
			"sessionName": null,
			"model": null,
			"startedAt": 0,
			"participants": 1,
			"relayConnected": true,
			"access": "control",
			"pid": state.pid,
			"cwd": state.cwd,
			"busy": state.busy,
			"inputRequired": state.input_required,
		});
		format!("{}\n", json!({ "ok": true, "v": 1, "snapshot": snapshot }))
	}

	async fn serve_one<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, reply: impl FnOnce(&str) -> String) {
		let mut line = Vec::new();
		let mut chunk = [0_u8; 1024];
		while !line.contains(&b'\n') {
			match stream.read(&mut chunk).await {
				Ok(0) | Err(_) => return,
				Ok(read) => line.extend_from_slice(&chunk[..read]),
			}
		}
		let end = line.iter().position(|b| *b == b'\n').unwrap_or(line.len());
		let text = reply(&String::from_utf8_lossy(&line[..end]));
		let _ = stream.write_all(text.as_bytes()).await;
		let _ = stream.shutdown().await;
	}

	pub(crate) async fn fake_host(dir: &Path, state: FakeHostState, token: Option<&str>) -> FakeHost {
		std::fs::create_dir_all(dir).unwrap();
		let entry = random_hex();
		let token = token.map_or_else(random_hex, str::to_owned);
		let shared = Arc::new(Mutex::new(state.clone()));
		let asked = Arc::new(AtomicUsize::new(0));
		let reply = {
			let (shared, asked, token, entry) = (shared.clone(), asked.clone(), token.clone(), entry.clone());
			move |line: &str| {
				let state = shared.lock().unwrap().clone();
				answer(line, &token, &entry, &state, &asked)
			}
		};
		let (endpoint, task) = listen(&entry, reply);
		let meta = dir.join(format!("{entry}.json"));
		let body = json!({
			"version": 1, "instanceId": entry, "pid": state.pid, "endpoint": endpoint, "createdAt": 0, "token": token,
		});
		std::fs::write(&meta, body.to_string()).unwrap();
		FakeHost { state: shared, asked, meta, endpoint, task: Some(task) }
	}

	#[cfg(windows)]
	fn listen(
		entry: &str,
		reply: impl Fn(&str) -> String + Clone + Send + 'static,
	) -> (String, tokio::task::JoinHandle<()>) {
		use tokio::net::windows::named_pipe::ServerOptions;
		let endpoint = format!(r"\\.\pipe\jocasta-test-{entry}");
		let mut server = ServerOptions::new().first_pipe_instance(true).create(&endpoint).unwrap();
		let name = endpoint.clone();
		let task = tokio::spawn(async move {
			loop {
				if server.connect().await.is_err() {
					return;
				}
				let Ok(next) = ServerOptions::new().create(&name) else { return };
				let connected = std::mem::replace(&mut server, next);
				tokio::spawn(serve_one(connected, reply.clone()));
			}
		});
		(endpoint, task)
	}

	/// The socket sits in the temp folder by its entry's name, not in the registry's folder: the registry gives the
	/// endpoint whole, and a socket path is held to about 100 bytes (`SUN_LEN`), which a sandboxed test folder
	/// passes.
	#[cfg(unix)]
	fn listen(
		entry: &str,
		reply: impl Fn(&str) -> String + Clone + Send + 'static,
	) -> (String, tokio::task::JoinHandle<()>) {
		let endpoint = std::env::temp_dir().join(format!("jocasta-host-{entry}.sock"));
		let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
		let task = tokio::spawn(async move {
			while let Ok((stream, _)) = listener.accept().await {
				tokio::spawn(serve_one(stream, reply.clone()));
			}
		});
		(endpoint.display().to_string(), task)
	}
}

#[cfg(test)]
mod tests {
	use super::fake::{FakeHostState, fake_host};
	use super::*;
	use crate::shared::testing::temp_dir;

	fn host_state(pid: u32, cwd: &str, busy: Option<bool>, input_required: bool) -> FakeHostState {
		FakeHostState { pid, cwd: cwd.to_owned(), busy, input_required }
	}

	#[tokio::test]
	async fn the_host_registry_answers_each_host_of_jocastas_pids_through_its_pipe_never_anothers_a_host_that_doesnt_answer_is_left_out()
	 {
		let root = temp_dir("hosts");
		let dir = root.path().join("collab-hosts");
		let mut ours = fake_host(&dir, host_state(101, "C:/wt/a", Some(true), false), None).await;
		let mut his = fake_host(&dir, host_state(202, "C:/his", Some(false), false), None).await;
		let mut asking = fake_host(&dir, host_state(303, "C:/wt/b", Some(false), true), None).await;
		// A torn metadata file, and a host whose process is gone (its endpoint answers nothing): left out.
		std::fs::write(dir.join("torn.json"), "{").unwrap();
		let gone = if cfg!(windows) {
			r"\\.\pipe\jocasta-test-gone".to_owned()
		} else {
			dir.join("gone.sock").display().to_string()
		};
		let gone_entry = json!({ "version": 1, "instanceId": "gone0000", "pid": 404, "endpoint": gone, "createdAt": 0, "token": "t" });
		std::fs::write(dir.join("gone.json"), gone_entry.to_string()).unwrap();

		let pids = |list: &[u32]| list.iter().copied().collect::<HashSet<u32>>();
		let mut hosts = read_hosts(&pids(&[101, 303, 404]), Some(&dir), None).await;
		hosts.sort_by_key(|host| host.pid);
		let host = |pid, cwd: &str, busy, input_required| HostState {
			pid,
			cwd: PathBuf::from(cwd),
			busy,
			input_required,
			relay_connected: true,
		};
		assert_eq!(hosts, vec![host(101, "C:/wt/a", Some(true), false), host(303, "C:/wt/b", Some(false), true)]);
		assert_eq!(his.asked(), 0);
		ours.state.lock().unwrap().busy = None;
		assert_eq!(read_hosts(&pids(&[101]), Some(&dir), None).await[0].busy, None);
		ours.close().await;
		assert_eq!(read_hosts(&pids(&[101]), Some(&dir), None).await, vec![]);
		assert_eq!(read_hosts(&pids(&[101]), Some(&root.path().join("none")), None).await, vec![]);
		assert_eq!(read_hosts(&pids(&[]), Some(&dir), None).await, vec![]);
		ours.close().await;
		his.close().await;
		asking.close().await;
	}

	#[test]
	fn a_stopped_hosts_entry_is_removed_only_when_its_pid_is_ours_and_gone() {
		let root = temp_dir("hosts-remove");
		let entry = |pid: u32| json!({ "version": 1, "pid": pid, "endpoint": "x", "token": "t" }).to_string();
		// A pid far above any this machine runs; the test process's own pid runs.
		let gone = u32::MAX - 1;
		std::fs::write(root.path().join("gone.json"), entry(gone)).unwrap();
		std::fs::write(root.path().join("other.json"), entry(gone - 1)).unwrap();
		std::fs::write(root.path().join("alive.json"), entry(std::process::id())).unwrap();
		let pids: HashSet<u32> = [gone, std::process::id()].into();
		assert_eq!(remove_host_entries(&pids, Some(root.path())), 1);
		assert!(!root.path().join("gone.json").exists());
		assert!(root.path().join("other.json").exists() && root.path().join("alive.json").exists());
	}
}
