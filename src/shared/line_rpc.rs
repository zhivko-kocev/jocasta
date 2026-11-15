//! Newline-delimited JSON (DUP-8): the one line splitter, and the one child process spoken to by JSON lines over its
//! stdio, as the code index's CBM server (0542, 0562) and omp's auth gateway (0463) are: each request carries a
//! numeric `id`, each answer echoes it, and answers may come in any order. Messages are `serde_json::Value`s.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};

use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::process::{AbortSignal, aborted, limit};

/// Cuts text into lines: `push` takes the next chunk and returns the lines it completed, trimmed (maybe empty).
#[derive(Debug, Default)]
pub struct LineSplitter {
	buffered: String,
}

impl LineSplitter {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn push(&mut self, chunk: &str) -> Vec<String> {
		self.buffered.push_str(chunk);
		let mut lines = Vec::new();
		while let Some(end) = self.buffered.find('\n') {
			lines.push(self.buffered[..end].trim().to_owned());
			self.buffered.drain(..=end);
		}
		lines
	}
}

/// How much of the child's stderr is kept for its exit error.
const STDERR_TAIL: usize = 500;

/// Text decoded from a byte stream a chunk at a time, a character cut by a chunk's end kept for the next
/// (`TextDecoder`'s `stream: true`).
#[derive(Default)]
struct Utf8Stream {
	partial: Vec<u8>,
}

impl Utf8Stream {
	fn decode(&mut self, chunk: &[u8]) -> String {
		self.partial.extend_from_slice(chunk);
		let valid = match std::str::from_utf8(&self.partial) {
			Ok(_) => self.partial.len(),
			Err(error) if error.error_len().is_none() => error.valid_up_to(),
			Err(_) => self.partial.len(),
		};
		let text = String::from_utf8_lossy(&self.partial[..valid]).into_owned();
		self.partial.drain(..valid);
		text
	}
}

/// What starting a [`LineRpc`] needs.
pub struct LineRpcOptions {
	pub cwd: Option<PathBuf>,
	/// The whole environment; the service's own when not given.
	pub env: Option<HashMap<String, String>>,
	/// The error the waiting calls, and every later one, fail with once the child exited: its exit code and the last
	/// 500 characters of its stderr.
	pub exit_error: ExitError,
	/// A JSON line that is no answer (no numeric `id`), such as the gateway's `{ready, version}`.
	pub on_other: Option<Box<dyn Fn(Value) + Send + Sync>>,
	/// Runs once the child exited and its waiting calls failed.
	pub on_exit: Option<Box<dyn Fn() + Send + Sync>>,
}

/// What one call may add.
#[derive(Default)]
pub struct RequestOptions {
	/// The call fails with `timeout_message` after this long.
	pub timeout_ms: Option<i64>,
	pub timeout_message: Option<String>,
	/// The call stops waiting when it aborts, failing with `abort_message`; the protocol has no cancel line.
	pub signal: Option<AbortSignal>,
	pub abort_message: Option<String>,
}

type Waiter = oneshot::Sender<Option<Value>>;

/// Builds the error of a child that exited from its exit code and stderr tail.
pub type ExitError = Box<dyn Fn(i32, &str) -> Error + Send + Sync>;

struct Inner {
	pending: Mutex<HashMap<u64, Waiter>>,
	next_id: AtomicU64,
	/// The exit code and stderr tail, once the child exited.
	gone: Mutex<Option<(i32, String)>>,
	exit_error: ExitError,
	on_other: Option<Box<dyn Fn(Value) + Send + Sync>>,
	exited: watch::Sender<Option<i32>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// JavaScript's truthiness of a parsed JSON value.
fn truthy(value: &Value) -> bool {
	match value {
		Value::Null | Value::Bool(false) => false,
		Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
		Value::String(text) => !text.is_empty(),
		_ => true,
	}
}

impl Inner {
	fn exit_error(&self) -> Option<Error> {
		locked(&self.gone).as_ref().map(|(code, tail)| (self.exit_error)(*code, tail))
	}

	/// One line the child wrote: an answer to its caller, an unsolicited message, or no JSON at all (skipped).
	fn line(&self, text: &str) {
		let Ok(message) = serde_json::from_str::<Value>(text) else { return };
		if let Some(id) = message.get("id").and_then(Value::as_u64).filter(|_| message["id"].is_number()) {
			if let Some(waiter) = locked(&self.pending).remove(&id) {
				let _ = waiter.send(Some(message));
			}
		} else if message.get("id").is_some_and(Value::is_number) {
			// A number that is no id (negative or fractional): nobody waits on it.
		} else if truthy(&message)
			&& let Some(other) = &self.on_other
		{
			other(message);
		}
	}
}

/// One running child answering JSON lines by `id`; a line that isn't JSON is skipped.
pub struct LineRpc {
	inner: Arc<Inner>,
	lines: Mutex<Option<mpsc::UnboundedSender<String>>>,
	kill: Mutex<Option<oneshot::Sender<()>>>,
}

async fn read_chunks(mut stream: impl AsyncRead + Unpin, mut each: impl FnMut(&[u8])) {
	let mut chunk = vec![0u8; 16 * 1024];
	while let Ok(read) = stream.read(&mut chunk).await {
		if read == 0 {
			return;
		}
		each(&chunk[..read]);
	}
}

impl LineRpc {
	/// Starts `argv`; fails when it can't be started, as the spawn does. Call it inside a tokio runtime.
	pub fn new(argv: &[impl AsRef<OsStr>], options: LineRpcOptions) -> Result<Self> {
		let (program, args) = argv.split_first().ok_or_else(|| Error::other("no program to run"))?;
		let mut command = tokio::process::Command::new(program);
		command.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
		if let Some(cwd) = &options.cwd {
			command.current_dir(cwd);
		}
		if let Some(vars) = &options.env {
			command.env_clear().envs(vars);
		}
		env::apply_to_tokio(&mut command);
		#[cfg(windows)]
		command.creation_flags(0x0800_0000);
		let mut child = command.spawn()?;
		let (exited, _) = watch::channel(None);
		let inner = Arc::new(Inner {
			pending: Mutex::new(HashMap::new()),
			next_id: AtomicU64::new(1),
			gone: Mutex::new(None),
			exit_error: options.exit_error,
			on_other: options.on_other,
			exited,
		});
		let (lines, mut outgoing) = mpsc::unbounded_channel::<String>();
		if let Some(mut stdin) = child.stdin.take() {
			tokio::spawn(async move {
				while let Some(line) = outgoing.recv().await {
					if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
						return;
					}
				}
			});
		}
		let answers = child.stdout.take().map(|stdout| {
			let inner = inner.clone();
			tokio::spawn(async move {
				let (mut decoder, mut splitter) = (Utf8Stream::default(), LineSplitter::new());
				read_chunks(stdout, |chunk| {
					for text in splitter.push(&decoder.decode(chunk)) {
						inner.line(&text);
					}
				})
				.await;
			})
		});
		let tail = Arc::new(Mutex::new(String::new()));
		let errors = child.stderr.take().map(|stderr| {
			let tail = tail.clone();
			tokio::spawn(async move {
				let mut decoder = Utf8Stream::default();
				read_chunks(stderr, |chunk| {
					let mut kept = locked(&tail);
					kept.push_str(&decoder.decode(chunk));
					let extra = kept.chars().count().saturating_sub(STDERR_TAIL);
					if extra > 0 {
						let cut = kept.char_indices().nth(extra).map_or(kept.len(), |(at, _)| at);
						kept.drain(..cut);
					}
				})
				.await;
			})
		});
		let (kill, killed) = oneshot::channel::<()>();
		let on_exit = options.on_exit;
		let settle = inner.clone();
		tokio::spawn(async move {
			let status = tokio::select! {
				status = child.wait() => status,
				Ok(()) = killed => {
					let _ = child.kill().await;
					child.wait().await
				}
			};
			let code = status.ok().and_then(|status| status.code()).unwrap_or(-1);
			// Every line it wrote is read before the calls still waiting fail.
			for task in [answers, errors].into_iter().flatten() {
				let _ = task.await;
			}
			*locked(&settle.gone) = Some((code, locked(&tail).clone()));
			for (_, waiter) in locked(&settle.pending).drain() {
				let _ = waiter.send(None);
			}
			if let Some(on_exit) = on_exit {
				on_exit();
			}
			settle.exited.send_replace(Some(code));
		});
		Ok(Self { inner, lines: Mutex::new(Some(lines)), kill: Mutex::new(Some(kill)) })
	}

	/// Settles with the exit code once every line the child wrote was read and the calls still waiting failed.
	pub async fn exited(&self) -> i32 {
		let mut exited = self.inner.exited.subscribe();
		match exited.wait_for(Option::is_some).await {
			Ok(code) => (*code).unwrap_or(-1),
			Err(_) => -1,
		}
	}

	/// Sends `message(id)` as one line and resolves the answer with that `id`; a call after the exit fails with its
	/// error.
	pub async fn request(&self, message: impl FnOnce(u64) -> Value, mut options: RequestOptions) -> Result<Value> {
		if let Some(error) = self.inner.exit_error() {
			return Err(error);
		}
		let abort_error = |text: &Option<String>| Error::other(text.clone().unwrap_or_else(|| "aborted".to_owned()));
		if options.signal.as_ref().is_some_and(|signal| signal.borrow().is_some()) {
			return Err(abort_error(&options.abort_message));
		}
		let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
		let (waiter, answer) = oneshot::channel();
		locked(&self.inner.pending).insert(id, waiter);
		self.send(&message(id));
		let outcome = tokio::select! {
			answer = answer => answer.ok().flatten().ok_or(()),
			() = limit(options.timeout_ms) => Err(()),
			_ = aborted(&mut options.signal) => Err(()),
		};
		// A timed-out or aborted call stops waiting; its late answer is dropped.
		let waiting = locked(&self.inner.pending).remove(&id).is_some();
		match outcome {
			Ok(answer) => Ok(answer),
			Err(()) if waiting => Err(self.stopped_waiting(&options, &abort_error)),
			Err(()) => Err(self.inner.exit_error().unwrap_or_else(|| Error::other("the child is gone"))),
		}
	}

	/// The error of a call that stopped waiting: its time ran out or its signal aborted.
	fn stopped_waiting(&self, options: &RequestOptions, abort_error: &dyn Fn(&Option<String>) -> Error) -> Error {
		if options.signal.as_ref().is_some_and(|signal| signal.borrow().is_some()) {
			return abort_error(&options.abort_message);
		}
		let ms = options.timeout_ms.unwrap_or(0);
		Error::other(
			options.timeout_message.clone().unwrap_or_else(|| format!("no answer within {} s", ms as f64 / 1000.0)),
		)
	}

	/// Sends one line that gets no answer (a JSON-RPC notification).
	pub fn send(&self, message: &Value) {
		if let Some(lines) = locked(&self.lines).as_ref() {
			let _ = lines.send(format!("{message}\n"));
		}
	}

	/// Lets the service exit while the child still runs: a tokio child never keeps the process alive, so this does
	/// nothing here (the TypeScript's `unref`).
	pub fn unref(&self) {}

	/// Closes the child's stdin; a stdio server leaves once it has written its answers.
	pub fn close(&self) {
		locked(&self.lines).take();
	}

	/// Ends the child (not what it started).
	pub fn kill(&self) {
		if let Some(kill) = locked(&self.kill).take() {
			let _ = kill.send(());
		}
	}
}

#[cfg(test)]
mod tests {
	use std::io::{BufRead, Write};
	use std::sync::atomic::AtomicUsize;

	use serde_json::json;

	use super::*;

	const CHILD_VAR: &str = "JOCASTA_TEST_LINE_RPC_CHILD";

	#[test]
	fn the_splitter_returns_the_lines_a_chunk_completes_trimmed_and_keeps_a_torn_line_for_the_next_chunk() {
		let mut lines = LineSplitter::new();
		assert_eq!(lines.push("{\"a\":1}\r\n  b  \n{\"c\""), vec!["{\"a\":1}", "b"]);
		assert_eq!(lines.push(":2}"), Vec::<String>::new());
		assert_eq!(lines.push("\n\n"), vec!["{\"c\":2}", ""]);
	}

	/// The child: says hello, writes a non-JSON line, answers requests in reverse order once two arrived, never
	/// answers `silent`, and exits 3 with a stderr line on `die`. It is this test executable run on this function.
	#[test]
	fn line_rpc_child() {
		if std::env::var(CHILD_VAR).is_err() {
			return;
		}
		let mut out = std::io::stdout();
		out.write_all(b"{\"hello\":true}\nnot json\n").unwrap();
		out.flush().unwrap();
		let mut held: Vec<Value> = Vec::new();
		for line in std::io::stdin().lock().lines() {
			let message: Value = serde_json::from_str(&line.unwrap()).unwrap();
			match message["echo"].as_str() {
				Some("die") => {
					eprint!("first\nlast words\n");
					std::process::exit(3);
				}
				Some("silent") => continue,
				_ => held.push(message),
			}
			if held.len() == 2 {
				for one in held.drain(..).rev() {
					writeln!(out, "{one}").unwrap();
				}
				out.flush().unwrap();
			}
		}
	}

	fn start(others: Arc<Mutex<Vec<Value>>>, exits: Arc<AtomicUsize>) -> LineRpc {
		let mut vars: HashMap<String, String> =
			env::vars().into_iter().filter_map(|(n, v)| Some((n.into_string().ok()?, v.into_string().ok()?))).collect();
		vars.insert(CHILD_VAR.to_owned(), "1".to_owned());
		let exe = std::env::current_exe().unwrap();
		let argv = [
			exe.into_os_string(),
			"--exact".into(),
			"shared::line_rpc::tests::line_rpc_child".into(),
			"-q".into(),
			"--nocapture".into(),
		];
		LineRpc::new(
			&argv,
			LineRpcOptions {
				cwd: None,
				env: Some(vars),
				exit_error: Box::new(|code, stderr| Error::other(format!("child exited {code}: {}", stderr.trim()))),
				on_other: Some(Box::new(move |line| locked(&others).push(line))),
				on_exit: Some(Box::new(move || {
					exits.fetch_add(1, Ordering::Relaxed);
				})),
			},
		)
		.unwrap()
	}

	fn quiet() -> LineRpc {
		start(Arc::default(), Arc::default())
	}

	#[tokio::test]
	async fn answers_by_id_in_any_order_and_hands_unsolicited_lines_to_on_other() {
		let others: Arc<Mutex<Vec<Value>>> = Arc::default();
		let rpc = start(others.clone(), Arc::default());
		let (first, second) = tokio::join!(
			rpc.request(|id| json!({"id": id, "echo": "one"}), RequestOptions::default()),
			rpc.request(|id| json!({"id": id, "echo": "two"}), RequestOptions::default()),
		);
		assert_eq!(first.unwrap(), json!({"id": 1, "echo": "one"}));
		assert_eq!(second.unwrap(), json!({"id": 2, "echo": "two"}));
		assert_eq!(*locked(&others), vec![json!({"hello": true})]);
		rpc.close();
		rpc.exited().await;
	}

	#[tokio::test]
	async fn a_call_times_out_or_aborts_with_its_own_text() {
		let rpc = quiet();
		let slow = rpc
			.request(
				|id| json!({"id": id, "echo": "silent"}),
				RequestOptions {
					timeout_ms: Some(50),
					timeout_message: Some("too slow".into()),
					..RequestOptions::default()
				},
			)
			.await;
		assert_eq!(slow.unwrap_err().to_string(), "too slow");
		let (abort, signal) = watch::channel(None);
		let waiting = rpc.request(
			|id| json!({"id": id, "echo": "silent"}),
			RequestOptions {
				signal: Some(signal.clone()),
				abort_message: Some("call aborted".into()),
				..RequestOptions::default()
			},
		);
		let aborting = async {
			tokio::time::sleep(std::time::Duration::from_millis(50)).await;
			abort.send_replace(Some("stop".into()));
		};
		let (waited, ()) = tokio::join!(waiting, aborting);
		assert_eq!(waited.unwrap_err().to_string(), "call aborted");
		let late = rpc
			.request(
				|id| json!({"id": id}),
				RequestOptions { signal: Some(signal), abort_message: Some("x".into()), ..RequestOptions::default() },
			)
			.await;
		assert_eq!(late.unwrap_err().to_string(), "x");
		rpc.close();
		rpc.exited().await;
	}

	#[tokio::test]
	async fn an_exit_fails_the_waiting_calls_and_every_later_one_with_the_exit_error_then_runs_on_exit() {
		let exits = Arc::new(AtomicUsize::new(0));
		let rpc = start(Arc::default(), exits.clone());
		let waiting = rpc.request(|id| json!({"id": id, "echo": "silent"}), RequestOptions::default());
		let dying = async { rpc.send(&json!({"echo": "die"})) };
		let (waited, ()) = tokio::join!(waiting, dying);
		assert!(waited.unwrap_err().to_string().contains("child exited 3: first\nlast words"));
		assert_eq!(rpc.exited().await, 3);
		assert_eq!(exits.load(Ordering::Relaxed), 1);
		let later = rpc.request(|id| json!({"id": id}), RequestOptions::default()).await;
		assert!(later.unwrap_err().to_string().contains("child exited 3"));
	}
}
