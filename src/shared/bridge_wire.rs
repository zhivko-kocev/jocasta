//! The bridge's wire (0675): JSON lines on a local pipe, shared by the service, the `jocasta` CLI words and the push
//! hook. The pipe is `\\.\pipe\jocasta-bridge-<port>` on Windows and the Unix socket
//! `${XDG_RUNTIME_DIR:-/tmp/jocasta-<uid>}/bridge-<port>.sock` (directory 0700, socket 0600) elsewhere; `<port>` is the
//! service's port. A line is UTF-8, `\n`-terminated and at most [`MAX_LINE`] bytes (`line_rpc::LineReader`).

use std::io;
#[cfg(windows)]
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use crate::shared::line_rpc::LineReader;

/// The variable that names the bridge in a pane's environment (0675).
pub const BRIDGE_ENV: &str = "JOCASTA_BRIDGE";

/// The shipped extension every pane's omp loads with `-e` (0675).
pub const BRIDGE_EXTENSION: &str = "bridge-extension.ts";

/// The longest line either side takes.
pub const MAX_LINE: usize = 4 * 1024 * 1024;

/// The protocol version in `hello` and `welcome`.
pub const VERSION: u64 = 1;

/// A connected pipe or socket, either end.
pub trait Wire: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Wire for T {}

/// A boxed [`Wire`].
pub type Stream = Box<dyn Wire>;

/// Where the bridge of the service on `port` listens.
#[cfg(windows)]
pub fn bridge_address(port: u16) -> String {
	format!(r"\\.\pipe\jocasta-bridge-{port}")
}

/// Where the bridge of the service on `port` listens.
#[cfg(unix)]
pub fn bridge_address(port: u16) -> String {
	bridge_dir().join(format!("bridge-{port}.sock")).display().to_string()
}

/// `$XDG_RUNTIME_DIR/jocasta`, else `/tmp/jocasta-<uid>`: a folder of Jocasta's own (0702, #15).
#[cfg(unix)]
pub fn bridge_dir() -> std::path::PathBuf {
	match crate::shared::env::var_nonempty("XDG_RUNTIME_DIR") {
		Some(dir) => std::path::PathBuf::from(dir).join("jocasta"),
		None => std::path::PathBuf::from(format!("/tmp/jocasta-{}", own_uid())),
	}
}

/// The user's uid: that of their home folder, which they own.
#[cfg(unix)]
pub fn own_uid() -> u32 {
	use std::os::unix::fs::MetadataExt;
	std::env::home_dir().and_then(|home| std::fs::metadata(home).ok()).map_or(0, |meta| meta.uid())
}

/// Connects to the bridge at `address`, retrying a busy pipe for a moment.
#[cfg(windows)]
pub async fn connect(address: &str) -> io::Result<Stream> {
	use tokio::net::windows::named_pipe::ClientOptions;
	const PIPE_BUSY: i32 = 231;
	let mut tries = 0;
	loop {
		match ClientOptions::new().open(address) {
			Ok(client) => return Ok(Box::new(client)),
			Err(error) if error.raw_os_error() == Some(PIPE_BUSY) && tries < 50 => {
				tries += 1;
				tokio::time::sleep(Duration::from_millis(20)).await;
			}
			Err(error) => return Err(error),
		}
	}
}

/// Connects to the bridge at `address`.
#[cfg(unix)]
pub async fn connect(address: &str) -> io::Result<Stream> {
	Ok(Box::new(tokio::net::UnixStream::connect(address).await?))
}

/// Whether `error` means nothing serves the bridge: no pipe or socket of that name, or a refused connection.
pub fn unreachable(error: &io::Error) -> bool {
	matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
}

/// One line, `message` and a newline.
pub fn line_of(message: &Value) -> String {
	format!("{message}\n")
}

/// The hello of a CLI peer (`jocasta thread …`, the push hook).
pub fn cli_hello(token: &str) -> Value {
	json!({ "t": "hello", "v": VERSION, "token": token, "role": "cli" })
}

/// A CLI peer connected to the bridge: sends `cli` and `push` requests and reads their `result`.
pub struct CliPeer {
	read: LineReader<ReadHalf<Stream>>,
	write: WriteHalf<Stream>,
	next: u64,
}

impl CliPeer {
	/// Connects to the service on `port` with its `token`. `Err` is why not: nothing listens, or the hello was refused.
	pub async fn open(port: u16, token: &str) -> io::Result<Self> {
		let stream = connect(&bridge_address(port)).await?;
		let (read, write) = tokio::io::split(stream);
		let mut peer = Self { read: LineReader::new(read, MAX_LINE), write, next: 0 };
		peer.write.write_all(line_of(&cli_hello(token)).as_bytes()).await?;
		peer.write.flush().await?;
		let welcome = peer.read_message().await?;
		if welcome.get("t").and_then(Value::as_str) != Some("welcome") {
			return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the bridge refused the hello"));
		}
		Ok(peer)
	}

	async fn read_message(&mut self) -> io::Result<Value> {
		loop {
			match self.read.next_line().await {
				Ok(Some(line)) => {
					if let Ok(message) = serde_json::from_str::<Value>(&line) {
						return Ok(message);
					}
				}
				Ok(None) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the bridge closed")),
				Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error.to_string())),
			}
		}
	}

	/// Sends `message` with the next id and resolves the `result` that answers it.
	pub async fn request(&mut self, mut message: Value) -> io::Result<Value> {
		self.next += 1;
		let id = self.next.to_string();
		if let Some(object) = message.as_object_mut() {
			object.insert("id".to_owned(), json!(id));
		}
		self.write.write_all(line_of(&message).as_bytes()).await?;
		self.write.flush().await?;
		loop {
			let answer = self.read_message().await?;
			if answer.get("t").and_then(Value::as_str) == Some("result") && answer.get("id") == Some(&json!(id)) {
				return Ok(answer);
			}
		}
	}
}
