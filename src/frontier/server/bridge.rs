//! The bridge (0675, 0681, 0684): one local pipe between the service and the one omp extension every pane loads
//! (`config/bridge-extension.ts`). It carries the panes' tool calls, the turn-end check, omp's state pushes, the
//! service's prompts and control messages, and the `jocasta` CLI words and the push hook (`shared::bridge_wire` has the
//! wire). The service sees a pane's omp only through [`Bridge`]; what omp can do is what its `caps` say, and the service
//! degrades by them (0684).
//!
//! Every string that reaches a model is a [`ModelText`], whose one constructor blanks the stored secrets (0681).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};
use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::frontier::server::push_gate::parse_push_request;
use crate::frontier::server::tools::{Caller, ToolReply};
use crate::shared::bridge_wire::{MAX_LINE, Stream, VERSION, bridge_address};
use crate::shared::clock::now_ms;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::same_token;
use crate::shared::line_rpc::LineReader;
use crate::shared::log::{LogLevel, log};
use crate::shared::process::AbortSignal;
use crate::shared::push_hook::{PushAnswer, PushRequest};

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

/// How long a prompt waits for its pane's omp to connect (0675).
pub const CONNECT_WAIT_MS: u64 = 30_000;

/// How long a peer may take to send its hello, and an omp to answer a prompt or a control message.
const HELLO_WAIT: Duration = Duration::from_secs(10);
const ANSWER_WAIT: Duration = Duration::from_secs(30);

/// A secret shorter than this is not blanked: a short value would mangle ordinary text (0681).
const MIN_SECRET: usize = 8;

/// The stored secrets the bridge blanks from everything a model reads (0681): the calendar URL, the service token, the
/// page token and the ntfy topics (and, from phase 5, the Gmail and Jira secrets).
#[derive(Debug, Clone, Default)]
pub struct Secrets(Vec<String>);

impl Secrets {
	/// The secrets to blank: each value of 8 or more characters counts (shorter ones would blank ordinary words).
	pub fn new(values: impl IntoIterator<Item = String>) -> Self {
		let mut secrets = Self::default();
		for value in values {
			secrets.add(&value);
		}
		secrets
	}

	/// Adds one value; one under 8 characters is ignored, and the longest are replaced first.
	pub fn add(&mut self, value: &str) {
		if value.len() >= MIN_SECRET && !self.0.iter().any(|known| known == value) {
			self.0.push(value.to_owned());
			self.0.sort_by_key(|known| std::cmp::Reverse(known.len()));
		}
	}
}

/// Text that may reach a model: every secret of 8 or more characters is blanked to `[secret]`. The only constructor is
/// [`ModelText::seal`] (0681).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelText(String);

impl ModelText {
	/// `text` with each secret of 8 or more characters replaced by `[secret]`: the one way to make model text (0681).
	pub fn seal(text: &str, secrets: &Secrets) -> Self {
		let mut sealed = text.to_owned();
		for secret in &secrets.0 {
			if sealed.contains(secret.as_str()) {
				sealed = sealed.replace(secret.as_str(), "[secret]");
			}
		}
		Self(sealed)
	}

	/// The sealed text.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

/// When omp takes a prompt (omp's own queue modes): `steer` between steps of a running turn, `followUp` after it,
/// `nextTurn` with the next one. An idle omp starts a turn for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deliver {
	Steer,
	FollowUp,
	NextTurn,
}

impl Deliver {
	fn as_str(self) -> &'static str {
		match self {
			Self::Steer => "steer",
			Self::FollowUp => "followUp",
			Self::NextTurn => "nextTurn",
		}
	}
}

/// What a pane's omp is, by its bridge (0675): the state its extension pushed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaneAgent {
	/// A turn runs (`agent_start` seen, `agent_end` with `idle: false`, or none yet).
	pub working: bool,
	/// A dialog of omp's waits on him: an approval, a plan review, a question (`input`).
	pub input_open: bool,
	/// Which dialog: `approval`, `plan` or `ask`.
	pub input_kind: Option<String>,
	/// Running async jobs at the last `agent_end`; `None` before one.
	pub jobs: Option<u32>,
	pub usage_percent: Option<f64>,
	pub usage_tokens: Option<u64>,
	/// Done and total todo tasks.
	pub todo: Option<(u32, u32)>,
	/// `agent_end` was seen with `idle: true`.
	pub idle_reported: bool,
	/// When omp last started or ended a turn, or connected (epoch ms): how long it has been idle (0676).
	pub active_at: i64,
}

impl PaneAgent {
	fn apply(&mut self, event: &PaneEvent) {
		match event {
			PaneEvent::AgentStart => {
				self.working = true;
				self.idle_reported = false;
				self.active_at = now_ms();
			}
			PaneEvent::AgentEnd { idle, jobs } => {
				self.working = !idle;
				self.idle_reported = *idle;
				self.active_at = now_ms();
				self.jobs = Some(*jobs);
			}
			PaneEvent::Usage { tokens, percent } => {
				self.usage_tokens = Some(*tokens);
				self.usage_percent = Some(*percent);
			}
			PaneEvent::Todo { done, total } => self.todo = Some((*done, *total)),
			PaneEvent::Input { open, kind } => {
				self.input_open = *open;
				self.input_kind = if *open { kind.clone() } else { None };
			}
			PaneEvent::Connected { .. } | PaneEvent::Caps(_) | PaneEvent::Shutdown | PaneEvent::Gone => {}
		}
	}
}

/// What a pane's omp pushed.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneEvent {
	/// The pane's hello was accepted.
	Connected {
		version: String,
		caps: Vec<String>,
		session: Option<String>,
	},
	AgentStart,
	AgentEnd {
		idle: bool,
		jobs: u32,
	},
	Usage {
		tokens: u64,
		percent: f64,
	},
	Todo {
		done: u32,
		total: u32,
	},
	Input {
		open: bool,
		kind: Option<String>,
	},
	/// What the extension found its omp can do, at `session_start` (0684).
	Caps(Vec<String>),
	/// omp is shutting down.
	Shutdown,
	/// The connection ended.
	Gone,
}

fn number(value: &Value, key: &str) -> Option<f64> {
	value.get(key).and_then(Value::as_f64)
}

impl PaneEvent {
	fn parse(message: &Value) -> Option<Self> {
		let count = |key: &str| number(message, key).map_or(0, |n| n.max(0.0) as u32);
		Some(match message.get("name")?.as_str()? {
			"agent_start" => Self::AgentStart,
			"agent_end" => Self::AgentEnd {
				idle: message.get("idle").and_then(Value::as_bool).unwrap_or(true),
				jobs: count("jobs"),
			},
			"usage" => Self::Usage {
				tokens: number(message, "tokens").map_or(0, |n| n.max(0.0) as u64),
				percent: number(message, "percent")?,
			},
			"todo" => Self::Todo { done: count("done"), total: count("total") },
			"input" => Self::Input {
				open: message.get("open").and_then(Value::as_bool)?,
				kind: message.get("kind").and_then(Value::as_str).map(str::to_owned),
			},
			"caps" => Self::Caps(strings(message.get("caps"))),
			"shutdown" => Self::Shutdown,
			_ => return None,
		})
	}
}

fn strings(value: Option<&Value>) -> Vec<String> {
	value
		.and_then(Value::as_array)
		.map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
		.unwrap_or_default()
}

/// A pane's part in Jocasta, by its key (0675).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
	Boss,
	Thread,
	Project,
}

impl Role {
	/// The role's name on the wire.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Boss => "boss",
			Self::Thread => "thread",
			Self::Project => "project",
		}
	}
}

/// Who a pane key is.
#[derive(Debug, Clone)]
pub struct Resolved {
	pub pane_id: String,
	pub role: Role,
	/// What the tools take to know the caller (0484, 0578).
	pub caller: Caller,
}

/// What the bridge needs of the service: the dispatcher, the turn-end check and the pushes (0675).
pub trait BridgeService: Send + Sync {
	/// The pane `key` names, or `None` for an unknown key (the connection closes).
	fn resolve(&self, key: &str) -> Option<Resolved>;
	/// The tools `role` gets, as `{name, label, description, parameters}`.
	fn tools(&self, role: Role) -> Vec<Value>;
	/// One tool call; the arguments are checked against the tool's schema first.
	fn call(&self, who: Resolved, tool: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply>;
	/// One `jocasta thread|worktree` command.
	fn cli(&self, command: String, args: Map<String, Value>) -> BoxFuture<'_, ToolReply>;
	/// git's pre-push hook.
	fn push(&self, request: PushRequest) -> BoxFuture<'_, PushAnswer>;
	/// The turn-end check of the pane started with `pane_key`; `signal` ends when omp stops waiting.
	fn stop(&self, pane_key: String, signal: AbortSignal) -> BoxFuture<'_, String>;
	/// A pane's omp pushed `event`; `agent` is its state with the event in it.
	fn event(&self, who: &Resolved, event: &PaneEvent, agent: &PaneAgent);
}

type Ack = oneshot::Sender<std::result::Result<(), String>>;

#[derive(Default)]
struct PeerState {
	caps: HashSet<String>,
	/// The `caps` event came: a missing capability is then missing, not just not reported yet.
	caps_known: bool,
	agent: PaneAgent,
	/// The omp session file it runs now: from its hello, then from each `caps` and `agent_end` event, since a handoff or
	/// a new session changes it (0676).
	session: Option<String>,
}

/// One connected pane.
struct Peer {
	conn: u64,
	who: Resolved,
	pid: Option<u64>,
	version: String,
	lines: mpsc::UnboundedSender<String>,
	state: Mutex<PeerState>,
	acks: Mutex<HashMap<String, Ack>>,
	/// Ends when the connection does.
	closed: watch::Sender<bool>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Peer {
	fn send(&self, message: &Value) -> bool {
		self.lines.send(format!("{message}\n")).is_ok()
	}

	fn has(&self, cap: &str) -> bool {
		let state = locked(&self.state);
		state.caps.contains(cap) || (!state.caps_known && cap != "prompt")
	}

	/// Sends `message` (with a new `id`) and waits for the omp's `ack` or `nack`.
	async fn ask(&self, id: String, mut message: Value) -> Result<()> {
		let (ack, answer) = oneshot::channel();
		locked(&self.acks).insert(id.clone(), ack);
		message["id"] = json!(id);
		if !self.send(&message) {
			locked(&self.acks).remove(&id);
			return Err(Error::other(format!("omp in {} is gone", self.who.pane_id)));
		}
		let mut closed = self.closed.subscribe();
		let outcome = tokio::select! {
			outcome = answer => outcome.ok(),
			_ = closed.wait_for(|closed| *closed) => None,
			() = tokio::time::sleep(ANSWER_WAIT) => None,
		};
		locked(&self.acks).remove(&id);
		match outcome {
			Some(Ok(())) => Ok(()),
			Some(Err(error)) => Err(Error::other(error)),
			None => Err(Error::other(format!("omp in {} didn't answer", self.who.pane_id))),
		}
	}
}

/// A prompt as a test double's sink sees it: without the facts block code computed (0685), whose lines depend on the
/// repository's commits. The facts have their own tests, which read the whole text from `Recording::sent`.
#[cfg(test)]
fn without_facts(text: &str) -> String {
	let Some(at) = text.find("Facts as of ") else { return text.to_owned() };
	let end = text[at..].find("\n\n").map_or(text.len(), |end| at + end + 2);
	format!("{}{}", text[..at].trim_end(), &text[end..])
}

/// What a test double records instead of sending (`Bridge::recording`).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sent {
	Prompt { pane: String, text: String, deliver: Deliver },
	Compact { pane: String, instructions: String },
	Abort { pane: String },
	Shutdown { pane: String },
}

/// A test double's view of the calls a recording bridge takes.
#[cfg(test)]
pub(crate) type Sink = Arc<dyn Fn(&str, Vec<Value>) -> std::result::Result<(), String> + Send + Sync>;
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Recording {
	pub sent: Vec<Sent>,
	pub agents: HashMap<String, PaneAgent>,
	/// The session file each pane's omp runs (`session`).
	pub sessions: HashMap<String, String>,
	/// Panes whose omp reported no `shutdown` capability (0684): `shutdown` answers `Ok(false)` for them.
	pub no_shutdown: HashSet<String>,
	/// A pane whose prompts are refused with this text.
	pub refused: HashMap<String, String>,
	/// Where each recorded call is also reported, by the method name a test double knows it by (`prompt`, `stop`,
	/// `shutdown`); an `Err` refuses it.
	pub sink: Option<Sink>,
}

struct Inner {
	peers: Mutex<HashMap<String, Arc<Peer>>>,
	secrets: Mutex<Secrets>,
	address: Mutex<Option<String>>,
	token: Mutex<String>,
	service: Mutex<Option<Weak<dyn BridgeService>>>,
	/// Bumped at each accepted hello, which wakes the prompts waiting for their pane.
	hellos: watch::Sender<u64>,
	next: AtomicU64,
	tasks: Mutex<Vec<JoinHandle<()>>>,
	connect_wait: Mutex<Duration>,
	#[cfg(test)]
	recording: Mutex<Option<Recording>>,
}

/// The bridge: the listener, the connected panes and what the service asks of them. Clones share one bridge.
#[derive(Clone)]
pub struct Bridge {
	inner: Arc<Inner>,
}

impl Default for Bridge {
	fn default() -> Self {
		Self::new()
	}
}

impl Bridge {
	/// A bridge with no listener, no peers and no secrets yet.
	pub fn new() -> Self {
		Self {
			inner: Arc::new(Inner {
				peers: Mutex::new(HashMap::new()),
				secrets: Mutex::new(Secrets::default()),
				address: Mutex::new(None),
				token: Mutex::new(String::new()),
				service: Mutex::new(None),
				hellos: watch::channel(0).0,
				next: AtomicU64::new(1),
				tasks: Mutex::new(Vec::new()),
				connect_wait: Mutex::new(Duration::from_millis(CONNECT_WAIT_MS)),
				#[cfg(test)]
				recording: Mutex::new(None),
			}),
		}
	}

	/// A bridge that takes every pane as connected and records what it is asked to send (the service's tests).
	#[cfg(test)]
	pub(crate) fn recording() -> Self {
		let bridge = Self::new();
		*locked(&bridge.inner.recording) = Some(Recording::default());
		bridge
	}

	/// The recording, for a test to read and set.
	#[cfg(test)]
	pub(crate) fn record<R>(&self, f: impl FnOnce(&mut Recording) -> R) -> Option<R> {
		locked(&self.inner.recording).as_mut().map(f)
	}

	/// How long a prompt waits for its pane's hello (tests shorten it).
	pub fn set_connect_wait(&self, wait: Duration) {
		*locked(&self.inner.connect_wait) = wait;
	}

	/// The secrets every model-bound text is sealed with from now on.
	pub fn set_secrets(&self, secrets: Secrets) {
		*locked(&self.inner.secrets) = secrets;
	}

	/// `text` as model text: the stored secrets blanked (0681).
	pub fn seal(&self, text: &str) -> ModelText {
		ModelText::seal(text, &locked(&self.inner.secrets))
	}

	/// The pipe or socket the panes' `JOCASTA_BRIDGE` names; `None` until the bridge listens.
	pub fn address(&self) -> Option<String> {
		locked(&self.inner.address).clone()
	}

	fn peer(&self, pane_id: &str) -> Option<Arc<Peer>> {
		locked(&self.inner.peers).get(pane_id).cloned()
	}

	/// Whether omp in `pane_id` is connected.
	pub fn connected(&self, pane_id: &str) -> bool {
		#[cfg(test)]
		if let Some(recording) = locked(&self.inner.recording).as_ref()
			&& recording.agents.contains_key(pane_id)
		{
			return true;
		}
		self.peer(pane_id).is_some()
	}

	/// The panes whose omp is connected.
	pub fn panes(&self) -> Vec<String> {
		let panes: Vec<String> = locked(&self.inner.peers).keys().cloned().collect();
		#[cfg(test)]
		let panes = {
			let mut panes = panes;
			if let Some(recording) = locked(&self.inner.recording).as_ref() {
				panes.extend(recording.agents.keys().filter(|pane| !panes.contains(pane)).cloned().collect::<Vec<_>>());
			}
			panes
		};
		panes
	}

	/// What omp in `pane_id` reported, or `None` while it isn't connected.
	pub fn agent(&self, pane_id: &str) -> Option<PaneAgent> {
		#[cfg(test)]
		if let Some(recording) = locked(&self.inner.recording).as_ref()
			&& let Some(agent) = recording.agents.get(pane_id)
		{
			return Some(agent.clone());
		}
		self.peer(pane_id).map(|peer| locked(&peer.state).agent.clone())
	}

	/// The session file omp in `pane_id` runs, as its hello and its latest `caps` or `agent_end` said it.
	pub fn session(&self, pane_id: &str) -> Option<String> {
		#[cfg(test)]
		if let Some(recording) = locked(&self.inner.recording).as_ref()
			&& let Some(session) = recording.sessions.get(pane_id)
		{
			return Some(session.clone());
		}
		self.peer(pane_id).and_then(|peer| locked(&peer.state).session.clone())
	}

	/// Whether omp in `pane_id` can do `cap` (`prompt`, `compact`, `abort`, `shutdown`, `usage`, `jobs`): what it reported;
	/// a capability not reported yet counts, except `prompt`, which the hello says.
	pub fn can(&self, pane_id: &str, cap: &str) -> bool {
		#[cfg(test)]
		if locked(&self.inner.recording).is_some() && self.peer(pane_id).is_none() {
			return true;
		}
		self.peer(pane_id).is_some_and(|peer| peer.has(cap))
	}

	/// The version omp in `pane_id` said it is.
	pub fn version(&self, pane_id: &str) -> Option<String> {
		self.peer(pane_id).map(|peer| peer.version.clone())
	}

	/// The pid omp in `pane_id` said it has.
	pub fn pid(&self, pane_id: &str) -> Option<u64> {
		self.peer(pane_id).and_then(|peer| peer.pid)
	}

	fn id(&self) -> String {
		self.inner.next.fetch_add(1, Ordering::Relaxed).to_string()
	}

	/// The peer of `pane_id`, waiting for its hello up to the connect wait (0675).
	async fn wait_peer(&self, pane_id: &str) -> Result<Arc<Peer>> {
		let wait = *locked(&self.inner.connect_wait);
		let mut hellos = self.inner.hellos.subscribe();
		let deadline = tokio::time::Instant::now() + wait;
		loop {
			if let Some(peer) = self.peer(pane_id) {
				return Ok(peer);
			}
			if tokio::time::timeout_at(deadline, hellos.changed()).await.is_err() {
				return Err(Error::other(format!(
					"omp in {pane_id} didn't connect within {} s",
					wait.as_secs_f64().round()
				)));
			}
		}
	}

	#[cfg(test)]
	fn recorded(&self, pane_id: &str, sent: Sent) -> Option<Result<()>> {
		let sink = {
			let mut guard = locked(&self.inner.recording);
			let recording = guard.as_mut()?;
			if self.peer(pane_id).is_some() {
				return None;
			}
			if let Sent::Prompt { .. } = &sent
				&& let Some(refused) = recording.refused.get(pane_id)
			{
				return Some(Err(Error::other(refused.clone())));
			}
			recording.sink.clone()
		};
		// The sink runs unlocked: a test's sink may take its time, and the bridge is used meanwhile.
		if let Some(sink) = sink {
			let (method, args) = match &sent {
				Sent::Prompt { pane, text, .. } => ("prompt", vec![json!(pane), json!(without_facts(text))]),
				Sent::Compact { pane, instructions } => {
					("prompt", vec![json!(pane), json!(format!("/handoff {instructions}"))])
				}
				Sent::Abort { pane } => ("stop", vec![json!(pane)]),
				Sent::Shutdown { pane } => ("shutdown", vec![json!(pane)]),
			};
			if let Err(refused) = sink(method, args) {
				return Some(Err(Error::other(refused)));
			}
		}
		locked(&self.inner.recording).as_mut()?.sent.push(sent);
		Some(Ok(()))
	}

	/// Puts `text` to the omp in `pane_id` as a user message (`pi.sendUserMessage`): queued until the pane's hello, and
	/// refused after 30 s without one. `Err` when its omp can't take prompts (0684) or refuses this one.
	pub async fn prompt(&self, pane_id: &str, text: ModelText, deliver: Deliver) -> Result<()> {
		#[cfg(test)]
		if let Some(done) =
			self.recorded(pane_id, Sent::Prompt { pane: pane_id.to_owned(), text: text.as_str().to_owned(), deliver })
		{
			return done;
		}
		let peer = self.wait_peer(pane_id).await?;
		if !peer.has("prompt") {
			return Err(Error::other(format!("omp {} can't take prompts from Jocasta; check the pin", peer.version)));
		}
		peer.ask(self.id(), json!({ "t": "prompt", "text": text.as_str(), "deliver": deliver.as_str() })).await
	}

	/// Asks the omp in `pane_id` to compact its session by handoff (`ctx.compact`): the instructions are the handoff's
	/// guidance. Queued like a prompt. `Ok(false)` when its omp can't (no `compact` capability): the caller sends the
	/// slash command as a prompt instead.
	pub async fn compact(&self, pane_id: &str, instructions: ModelText) -> Result<bool> {
		#[cfg(test)]
		if let Some(done) = self.recorded(
			pane_id,
			Sent::Compact { pane: pane_id.to_owned(), instructions: instructions.as_str().to_owned() },
		) {
			return done.map(|()| true);
		}
		let peer = self.wait_peer(pane_id).await?;
		if !peer.has("compact") {
			return Ok(false);
		}
		peer.ask(self.id(), json!({ "t": "compact", "mode": "handoff", "instructions": instructions.as_str() }))
			.await?;
		Ok(true)
	}

	/// Aborts the turn of the omp in `pane_id` (`ctx.abort`). `Ok(false)` when its omp can't (0684): the caller ends its
	/// process tree instead.
	pub fn abort(&self, pane_id: &str) -> Result<bool> {
		#[cfg(test)]
		if let Some(done) = self.recorded(pane_id, Sent::Abort { pane: pane_id.to_owned() }) {
			return done.map(|()| true);
		}
		let peer = self.peer(pane_id).ok_or_else(|| Error::other(format!("omp in {pane_id} isn't connected")))?;
		if !peer.has("abort") {
			return Ok(false);
		}
		Ok(peer.send(&json!({ "t": "abort" })))
	}

	/// Asks the omp in `pane_id` to end (`ctx.shutdown`). `Ok(false)` when its omp can't (0684).
	pub fn shutdown(&self, pane_id: &str) -> Result<bool> {
		#[cfg(test)]
		if self.peer(pane_id).is_none()
			&& locked(&self.inner.recording).as_ref().is_some_and(|recording| recording.no_shutdown.contains(pane_id))
		{
			return Ok(false);
		}
		#[cfg(test)]
		if let Some(done) = self.recorded(pane_id, Sent::Shutdown { pane: pane_id.to_owned() }) {
			return done.map(|()| true);
		}
		let peer = self.peer(pane_id).ok_or_else(|| Error::other(format!("omp in {pane_id} isn't connected")))?;
		if !peer.has("shutdown") {
			return Ok(false);
		}
		Ok(peer.send(&json!({ "t": "shutdown" })))
	}

	/// Resolves once the omp in `pane_id` is gone, or `ms` passed; whether it went.
	pub async fn gone(&self, pane_id: &str, ms: u64) -> bool {
		let Some(peer) = self.peer(pane_id) else { return true };
		let mut closed = peer.closed.subscribe();
		tokio::time::timeout(Duration::from_millis(ms), closed.wait_for(|closed| *closed)).await.is_ok()
	}

	/// Starts listening on the service's `port`; `token` is the CLI peers' (the service file's). `service` is held weakly.
	pub fn listen(&self, port: u16, token: &str, service: &Arc<dyn BridgeService>) -> Result<()> {
		let address = bridge_address(port);
		*locked(&self.inner.token) = token.to_owned();
		*locked(&self.inner.service) = Some(Arc::downgrade(service));
		let accept = listener(self, &address)?;
		locked(&self.inner.tasks).push(accept);
		*locked(&self.inner.address) = Some(address);
		Ok(())
	}

	/// Stops listening and closes every connection.
	pub fn stop(&self) {
		for task in locked(&self.inner.tasks).drain(..) {
			task.abort();
		}
		let peers: Vec<Arc<Peer>> = locked(&self.inner.peers).drain().map(|(_, peer)| peer).collect();
		for peer in peers {
			peer.closed.send_replace(true);
		}
		#[cfg(unix)]
		if let Some(address) = locked(&self.inner.address).take() {
			let _ = std::fs::remove_file(address);
		}
	}

	fn service(&self) -> Option<Arc<dyn BridgeService>> {
		locked(&self.inner.service).as_ref().and_then(Weak::upgrade)
	}
}

#[cfg(windows)]
fn listener(bridge: &Bridge, address: &str) -> Result<JoinHandle<()>> {
	use tokio::net::windows::named_pipe::ServerOptions;
	let mut server = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(address)?;
	let (bridge, address) = (bridge.clone(), address.to_owned());
	Ok(tokio::spawn(async move {
		loop {
			if server.connect().await.is_err() {
				return;
			}
			let connected = server;
			server = match ServerOptions::new().reject_remote_clients(true).create(&address) {
				Ok(next) => next,
				Err(error) => {
					log(LogLevel::Error, "bridge", &format!("the pipe can't take another peer: {error}"), None);
					return;
				}
			};
			let serving = bridge.clone();
			let task = tokio::spawn(async move { serve(serving, Box::new(connected)).await });
			locked(bridge_tasks(&bridge)).retain(|task| !task.is_finished());
			locked(bridge_tasks(&bridge)).push(task);
		}
	}))
}

fn bridge_tasks(bridge: &Bridge) -> &Mutex<Vec<JoinHandle<()>>> {
	&bridge.inner.tasks
}

#[cfg(unix)]
fn listener(bridge: &Bridge, address: &str) -> Result<JoinHandle<()>> {
	use std::os::unix::fs::PermissionsExt;
	let path = std::path::Path::new(address);
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
		std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
	}
	let _ = std::fs::remove_file(path);
	let socket = tokio::net::UnixListener::bind(path)?;
	std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
	let bridge = bridge.clone();
	Ok(tokio::spawn(async move {
		while let Ok((connected, _)) = socket.accept().await {
			let serving = bridge.clone();
			let task = tokio::spawn(async move { serve(serving, Box::new(connected)).await });
			locked(bridge_tasks(&bridge)).retain(|task| !task.is_finished());
			locked(bridge_tasks(&bridge)).push(task);
		}
	}))
}

/// What a connection turned out to be after its hello.
enum Identity {
	Pane(Arc<Peer>),
	Cli,
}

/// Calls and checks a peer has under way: `cancel` ends a call's task and tells a check its pane stopped waiting.
#[derive(Default)]
struct Inflight {
	calls: HashMap<String, JoinHandle<()>>,
	stops: HashMap<String, watch::Sender<Option<String>>>,
}

type Lines = mpsc::UnboundedSender<String>;

/// What a connection answers on and keeps its running calls in.
struct Conn {
	lines: Lines,
	inflight: Arc<Mutex<Inflight>>,
}

/// The `result` line for a tool's `reply`; its `text` and `error` are sealed model text (0681).
fn result_line(id: &str, reply: &ToolReply, bridge: &Bridge) -> String {
	let ok = reply.is_error != Some(true);
	let key = if ok { "text" } else { "error" };
	format!("{}\n", json!({ "t": "result", "id": id, key: bridge.seal(&reply.text).as_str(), "ok": ok }))
}

async fn write_lines(mut write: WriteHalf<Stream>, mut lines: mpsc::UnboundedReceiver<String>) {
	while let Some(line) = lines.recv().await {
		if write.write_all(line.as_bytes()).await.is_err() || write.flush().await.is_err() {
			return;
		}
	}
	let _ = write.shutdown().await;
}

/// One connection's life: its hello, then its messages until it closes.
async fn serve(bridge: Bridge, stream: Stream) {
	let (read, write) = tokio::io::split(stream);
	let (lines, outgoing) = mpsc::unbounded_channel::<String>();
	let writer = tokio::spawn(write_lines(write, outgoing));
	let mut reader = LineReader::new(read, MAX_LINE);
	let first = tokio::time::timeout(HELLO_WAIT, reader.next_line()).await;
	let Ok(Ok(Some(hello))) = first else {
		writer.abort();
		return;
	};
	let Some(identity) = accept(&bridge, &serde_json::from_str(&hello).unwrap_or(Value::Null), &lines) else {
		// Close: the writer ends once its sender is dropped.
		drop(lines);
		let _ = writer.await;
		return;
	};
	let conn = Conn { lines: lines.clone(), inflight: Arc::new(Mutex::new(Inflight::default())) };
	loop {
		match reader.next_line().await {
			Ok(Some(line)) => {
				if let Ok(message) = serde_json::from_str::<Value>(&line) {
					handle(&bridge, &identity, &message, &conn);
				}
			}
			Ok(None) => break,
			Err(error) => {
				log(LogLevel::Warn, "bridge", &format!("a peer's line was refused: {error}"), None);
				break;
			}
		}
	}
	leave(&bridge, &identity, &conn.inflight);
	// The writer ends once every sender is gone: the connection's own, the pane's `Peer` and the calls'. Until it
	// ends the stream stays open, so a peer sent over the line cap would never see its connection close.
	drop(conn);
	drop(identity);
	drop(lines);
	let _ = writer.await;
}

/// The hello of a connection: the peer it is, welcomed; `None` closes it.
fn accept(bridge: &Bridge, hello: &Value, lines: &Lines) -> Option<Identity> {
	let service = bridge.service()?;
	if hello.get("t").and_then(Value::as_str) != Some("hello")
		|| hello.get("v").and_then(Value::as_u64) != Some(VERSION)
	{
		return None;
	}
	if hello.get("role").and_then(Value::as_str) == Some("cli") {
		let token = locked(&bridge.inner.token).clone();
		if !same_token(hello.get("token").and_then(Value::as_str), &token) {
			return None;
		}
		let _ = lines.send(format!("{}\n", json!({ "t": "welcome", "v": VERSION, "role": "cli", "tools": [] })));
		return Some(Identity::Cli);
	}
	let who = service.resolve(hello.get("key").and_then(Value::as_str)?)?;
	let (closed, _) = watch::channel(false);
	let peer = Arc::new(Peer {
		conn: bridge.inner.next.fetch_add(1, Ordering::Relaxed),
		pid: hello.get("pid").and_then(Value::as_u64),
		version: hello.get("harness_version").and_then(Value::as_str).unwrap_or("unknown").to_owned(),
		lines: lines.clone(),
		state: Mutex::new(PeerState {
			session: hello.get("session").and_then(Value::as_str).map(str::to_owned),
			..PeerState::default()
		}),
		acks: Mutex::new(HashMap::new()),
		closed,
		who,
	});
	let caps = strings(hello.get("caps"));
	{
		let mut state = locked(&peer.state);
		state.caps = caps.iter().cloned().collect();
		state.agent.active_at = now_ms();
	}
	let welcome =
		json!({ "t": "welcome", "v": VERSION, "role": peer.who.role.as_str(), "tools": service.tools(peer.who.role) });
	let _ = lines.send(format!("{welcome}\n"));
	// A pane's omp that started again takes the place of the one before it.
	let before = locked(&bridge.inner.peers).insert(peer.who.pane_id.clone(), peer.clone());
	if let Some(before) = before {
		before.closed.send_replace(true);
	}
	let session = locked(&peer.state).session.clone();
	let event = PaneEvent::Connected { version: peer.version.clone(), caps, session };
	service.event(&peer.who, &event, &PaneAgent::default());
	bridge.inner.hellos.send_modify(|count| *count += 1);
	Some(Identity::Pane(peer))
}

/// A connection ended: its pane is no longer there (unless a newer omp took its place), and its calls end.
fn leave(bridge: &Bridge, identity: &Identity, inflight: &Arc<Mutex<Inflight>>) {
	let mut inflight = locked(inflight);
	for (_, call) in inflight.calls.drain() {
		call.abort();
	}
	for (_, stop) in inflight.stops.drain() {
		stop.send_replace(Some("the connection ended".to_owned()));
	}
	let Identity::Pane(peer) = identity else { return };
	peer.closed.send_replace(true);
	let gone = {
		let mut peers = locked(&bridge.inner.peers);
		let current = peers.get(&peer.who.pane_id).is_some_and(|held| held.conn == peer.conn);
		if current {
			peers.remove(&peer.who.pane_id);
		}
		current
	};
	if gone && let Some(service) = bridge.service() {
		let agent = locked(&peer.state).agent.clone();
		service.event(&peer.who, &PaneEvent::Gone, &agent);
	}
}

fn handle(bridge: &Bridge, identity: &Identity, message: &Value, conn: &Conn) {
	let Some(kind) = message.get("t").and_then(Value::as_str) else { return };
	let id = message.get("id").map(|id| id.as_str().map_or_else(|| id.to_string(), str::to_owned));
	match (kind, identity, id) {
		("call", Identity::Pane(peer), Some(id)) => start_call(bridge, peer, message, id, conn),
		("stop", Identity::Pane(peer), Some(id)) => start_stop(bridge, peer, id, conn),
		("cancel", _, Some(id)) => cancel(&conn.inflight, &id),
		("event", Identity::Pane(peer), _) => event(bridge, peer, message),
		("ack", Identity::Pane(peer), Some(id)) => answer(peer, &id, Ok(())),
		("nack", Identity::Pane(peer), Some(id)) => {
			let error = message.get("error").and_then(Value::as_str).unwrap_or("omp refused it");
			answer(peer, &id, Err(error.to_owned()));
		}
		("cli" | "push", Identity::Cli, Some(id)) => start_cli(bridge, kind, message, id, conn),
		_ => {}
	}
}

fn answer(peer: &Peer, id: &str, outcome: std::result::Result<(), String>) {
	if let Some(ack) = locked(&peer.acks).remove(id) {
		let _ = ack.send(outcome);
	}
}

fn cancel(inflight: &Arc<Mutex<Inflight>>, id: &str) {
	let mut inflight = locked(inflight);
	if let Some(call) = inflight.calls.remove(id) {
		call.abort();
	}
	if let Some(stop) = inflight.stops.remove(id) {
		stop.send_replace(Some("omp stopped waiting".to_owned()));
	}
}

fn event(bridge: &Bridge, peer: &Arc<Peer>, message: &Value) {
	let Some(event) = PaneEvent::parse(message) else { return };
	let agent = {
		let mut state = locked(&peer.state);
		if let Some(session) = message.get("session").and_then(Value::as_str).filter(|session| !session.is_empty()) {
			state.session = Some(session.to_owned());
		}
		if let PaneEvent::Caps(caps) = &event {
			state.caps.extend(caps.iter().cloned());
			state.caps_known = true;
		}
		state.agent.apply(&event);
		state.agent.clone()
	};
	if let Some(service) = bridge.service() {
		service.event(&peer.who, &event, &agent);
	}
}

fn object_of(value: Option<&Value>) -> Map<String, Value> {
	value.and_then(Value::as_object).cloned().unwrap_or_default()
}

fn start_call(bridge: &Bridge, peer: &Arc<Peer>, message: &Value, id: String, conn: &Conn) {
	let Some(service) = bridge.service() else { return };
	let (lines, inflight) = (&conn.lines, &conn.inflight);
	let tool = message.get("tool").and_then(Value::as_str).unwrap_or_default().to_owned();
	let args = object_of(message.get("args"));
	let (who, lines, bridge_for_seal, done) = (peer.who.clone(), lines.clone(), bridge.clone(), inflight.clone());
	let call_id = id.clone();
	let task = tokio::spawn(async move {
		let reply = service.call(who, tool, args).await;
		let _ = lines.send(result_line(&call_id, &reply, &bridge_for_seal));
		locked(&done).calls.remove(&call_id);
	});
	locked(inflight).calls.insert(id, task);
}

fn start_stop(bridge: &Bridge, peer: &Arc<Peer>, id: String, conn: &Conn) {
	let Some(service) = bridge.service() else { return };
	let key = locked_key(peer);
	let (signal_tx, signal) = watch::channel(None);
	locked(&conn.inflight).stops.insert(id.clone(), signal_tx);
	let (lines, bridge, done) = (conn.lines.clone(), bridge.clone(), conn.inflight.clone());
	tokio::spawn(async move {
		let context = service.stop(key, signal).await;
		let sealed = bridge.seal(&context);
		let line = json!({ "t": "stop_result", "id": id, "context": sealed.as_str() });
		let _ = lines.send(format!("{line}\n"));
		locked(&done).stops.remove(&id);
	});
}

/// The key a pane's turn-end check is asked with: the pane's own, as its hello named it.
fn locked_key(peer: &Peer) -> String {
	peer.who.caller.pane_key.clone().unwrap_or_default()
}

fn start_cli(bridge: &Bridge, kind: &str, message: &Value, id: String, conn: &Conn) {
	let Some(service) = bridge.service() else { return };
	let (lines, inflight) = (&conn.lines, &conn.inflight);
	let (lines, bridge, done, kind) = (lines.clone(), bridge.clone(), inflight.clone(), kind.to_owned());
	let message = message.clone();
	let call_id = id.clone();
	let task = tokio::spawn(async move {
		let line = if kind == "cli" {
			let command = message.get("command").and_then(Value::as_str).unwrap_or_default().to_owned();
			let reply = service.cli(command, object_of(message.get("args"))).await;
			result_line(&call_id, &reply, &bridge)
		} else {
			let request = parse_push_request(&object_of(Some(&message)));
			let answer = match request {
				Some(request) => service.push(request).await,
				None => PushAnswer { allow: false, reason: Some("not a push request".to_owned()) },
			};
			let reason = answer.reason.as_deref().map(|reason| bridge.seal(reason));
			let line = json!({
				"t": "result", "id": call_id, "ok": true, "allow": answer.allow,
				"reason": reason.as_ref().map(ModelText::as_str),
			});
			format!("{line}\n")
		};
		let _ = lines.send(line);
		locked(&done).calls.remove(&call_id);
	});
	locked(inflight).calls.insert(id, task);
}
