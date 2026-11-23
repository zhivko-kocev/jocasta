//! The phone check-in through ntfy (0672): what needs him and what finished reaches his phone, and on his own server
//! he can answer an Allow from there.
//!
//! The mode is decided once at start from the settings. **Public** (a server that isn't marked his own, or no token):
//! only a kind and a count leave the machine, never text, buttons, a token or a reply stream. **Own** (`ownServer` and
//! a token): full text, Allow and Deny buttons, and a subscription to the reply topic, every request carrying his
//! token. The topics are generated here, 256 random bits each, and kept in `<data>/ntfy-topics.json`.
//!
//! An Allow token is made per question and button, answers only while its question is open, only once, and only when
//! it arrives on the reply stream (`Phone::listen`): nothing else reads a token.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;

use super::native::note_body;
use super::popups::{ALLOW, AnswerTaken, DENY, Popup, Popups, random_hex};
use crate::frontier::protocol::{LandedItem, NeedsYou};
use crate::shared::clock::now_ms;
use crate::shared::fsx::write_atomically;
use crate::shared::log::{LogLevel, log};
use crate::shared::settings::NtfySettings;

/// `<data>/ntfy-topics.json`.
pub const TOPICS_FILE: &str = "ntfy-topics.json";
/// `<data>/ntfy-sent.json`: what was pushed already, key to time.
pub const SENT_FILE: &str = "ntfy-sent.json";
/// A pushed key is forgotten after this long.
const SENT_KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// A publish gives up after this long.
const PUBLISH_MS: u64 = 15_000;
/// The least a topic is, in hex characters (256 bits are made; 128 are the least that is safe).
const TOPIC_MIN: usize = 64;

/// What the phone pushes: the dock's NEEDS YOU rows and the finished threads, and whether quiet hours hold them.
pub struct PhoneFacts {
	pub needs: Vec<NeedsYou>,
	pub finished: Vec<LandedItem>,
	pub quiet: bool,
}

/// What a server may carry (0672): kind and count only on a public one, text and Allow buttons on his own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneMode {
	Public,
	Own,
}

/// The mode the settings make: own only with `ownServer` and a token.
pub fn mode_of(ntfy: &NtfySettings) -> PhoneMode {
	if ntfy.own_server && ntfy.token.as_deref().is_some_and(|token| !token.is_empty()) {
		PhoneMode::Own
	} else {
		PhoneMode::Public
	}
}

/// The times of the loops: a test's seam, never a setting.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
	/// How often the facts are looked at.
	pub tick_ms: u64,
	/// The first wait before a reply stream reconnects, and the most it grows to (it doubles).
	pub backoff_start_ms: u64,
	pub backoff_max_ms: u64,
	/// A reply stream with no bytes for this long is dead.
	pub dead_ms: u64,
}

impl Default for Timing {
	fn default() -> Self {
		Self { tick_ms: 5_000, backoff_start_ms: 1_000, backoff_max_ms: 60_000, dead_ms: 90_000 }
	}
}

/// What `Phone::start` needs: his `ntfy` settings, the questions, the facts to push, and where the topics live.
pub struct PhoneOptions {
	/// `ntfy.*`; an empty server is off.
	pub ntfy: NtfySettings,
	pub popups: Arc<Popups>,
	pub facts: Arc<dyn Fn() -> PhoneFacts + Send + Sync>,
	pub data_dir: PathBuf,
	pub timing: Timing,
}

/// The two topics: pushes go to `main`, his answers come on `reply`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topics {
	pub main: String,
	pub reply: String,
}

fn is_topic(topic: &str) -> bool {
	topic.len() >= TOPIC_MIN && topic.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The topics already made; `None` before the first start, or for a file that isn't two good, different topics.
pub fn read_topics(data_dir: &Path) -> Option<Topics> {
	let topics: Topics = serde_json::from_str(&std::fs::read_to_string(data_dir.join(TOPICS_FILE)).ok()?).ok()?;
	(is_topic(&topics.main) && is_topic(&topics.reply) && topics.main != topics.reply).then_some(topics)
}

/// The topics, made and kept once.
pub fn topics(data_dir: &Path) -> Topics {
	if let Some(found) = read_topics(data_dir) {
		return found;
	}
	let made = Topics { main: random_hex(), reply: random_hex() };
	let written = std::fs::create_dir_all(data_dir)
		.map_err(crate::shared::errors::Error::from)
		.and_then(|()| Ok(serde_json::to_vec_pretty(&made)?))
		.and_then(|text| write_atomically(&data_dir.join(TOPICS_FILE), text, Some(0o600)));
	if let Err(error) = written {
		log(LogLevel::Warn, "phone", &format!("couldn't keep the phone's topics: {error}"), None);
	}
	made
}

/// The wait after `current` before the next reconnect.
fn next_delay(current: u64, max: u64) -> u64 {
	current.saturating_mul(2).min(max)
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What was pushed already, key to time, kept in `ntfy-sent.json`.
struct Sent {
	file: PathBuf,
	at: BTreeMap<String, i64>,
}

impl Sent {
	fn load(data_dir: &Path) -> Self {
		let file = data_dir.join(SENT_FILE);
		let at =
			std::fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
		let mut sent = Self { file, at };
		sent.prune(now_ms());
		sent
	}

	fn prune(&mut self, now: i64) {
		self.at.retain(|_, at| now - *at < SENT_KEEP_MS);
	}

	fn save(&mut self, now: i64) {
		self.prune(now);
		let written = serde_json::to_vec_pretty(&self.at)
			.map_err(crate::shared::errors::Error::from)
			.and_then(|text| write_atomically(&self.file, text, None));
		if let Err(error) = written {
			log(LogLevel::Warn, "phone", &format!("couldn't keep what the phone was told: {error}"), None);
		}
	}
}

fn hash(parts: &[&str]) -> String {
	let digest = Sha256::digest(parts.join("\u{1f}").as_bytes());
	digest[..12].iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A row's key: its kind and subject, so a wait that grows doesn't push it again.
fn needs_key(row: &NeedsYou) -> String {
	let subject = row.text.split(" — ").next().unwrap_or_default();
	hash(&[
		"needs",
		&format!("{:?}", row.kind),
		row.pane_id.as_deref().unwrap_or_default(),
		row.project.as_deref().unwrap_or_default(),
		subject,
	])
}

fn finished_key(item: &LandedItem) -> String {
	hash(&[
		"finished",
		item.pane_id.as_deref().unwrap_or_default(),
		item.project.as_deref().unwrap_or_default(),
		&item.title,
		&item.since.map(|since| since.to_string()).unwrap_or_default(),
	])
}

/// One push.
struct Note {
	title: String,
	body: String,
	priority: &'static str,
	/// The `Actions` header (own mode only).
	actions: Option<String>,
}

impl Note {
	fn plain(title: impl Into<String>, priority: &'static str) -> Self {
		let title = title.into();
		Self { body: title.clone(), title, priority, actions: None }
	}
}

fn plural(count: usize, one: &str, many: &str) -> String {
	if count == 1 { one.to_owned() } else { format!("{count} {many}") }
}

/// The one push a public server gets for `count` rows that need him.
fn public_needs(count: usize) -> Note {
	Note::plain(format!("Jocasta: {}", plural(count, "1 needs you", "need you")), "high")
}

fn public_finished(count: usize) -> Note {
	Note::plain(format!("Jocasta: {}", plural(count, "a thread finished", "threads finished")), "default")
}

struct Core {
	client: reqwest::Client,
	/// The server without a trailing slash.
	server: String,
	mode: PhoneMode,
	/// His ntfy token: own mode only.
	token: Option<String>,
	topics: Topics,
	popups: Arc<Popups>,
	facts: Arc<dyn Fn() -> PhoneFacts + Send + Sync>,
	timing: Timing,
	/// Open questions' one-time tokens: token to (the question's code, its choice).
	tokens: Mutex<HashMap<String, (String, &'static str)>>,
	sent: tokio::sync::Mutex<Sent>,
}

impl Core {
	fn url(&self, topic: &str) -> String {
		format!("{}/{topic}", self.server)
	}

	/// Publishes to the main topic. A public server gets no `Actions` and no token whatever the note holds.
	async fn publish(&self, note: &Note) -> Result<(), String> {
		let mut request = self
			.client
			.post(self.url(&self.topics.main))
			.timeout(Duration::from_millis(PUBLISH_MS))
			.header("Title", &note.title)
			.header("Priority", note.priority)
			.body(note.body.clone());
		if self.mode == PhoneMode::Own {
			if let Some(actions) = &note.actions {
				request = request.header("Actions", actions);
			}
			if let Some(token) = &self.token {
				request = request.bearer_auth(token);
			}
		}
		let response = request.send().await.map_err(|error| error.without_url().to_string())?;
		if response.status().is_success() { Ok(()) } else { Err(format!("the server said {}", response.status())) }
	}

	fn say(&self, what: &str, error: &str) {
		log(LogLevel::Warn, "phone", &format!("{what}: {error}"), None);
	}

	/// One look at the facts: what is new is pushed once, and not while quiet hours hold it.
	async fn tick(&self) {
		let facts = (self.facts)();
		if facts.quiet {
			return;
		}
		let mut sent = self.sent.lock().await;
		let mut seen = std::collections::HashSet::new();
		let needs: Vec<(String, &NeedsYou)> = facts
			.needs
			.iter()
			.map(|row| (needs_key(row), row))
			.filter(|(key, _)| !sent.at.contains_key(key) && seen.insert(key.clone()))
			.collect();
		let finished: Vec<(String, &LandedItem)> = facts
			.finished
			.iter()
			.map(|item| (finished_key(item), item))
			.filter(|(key, _)| !sent.at.contains_key(key) && seen.insert(key.clone()))
			.collect();
		let mut changed = false;
		let now = now_ms();
		match self.mode {
			PhoneMode::Public => {
				if !needs.is_empty() {
					match self.publish(&public_needs(needs.len())).await {
						Ok(()) => needs.into_iter().for_each(|(key, _)| {
							sent.at.insert(key, now);
						}),
						Err(error) => self.say("a push failed", &error),
					}
					changed = true;
				}
				if !finished.is_empty() {
					match self.publish(&public_finished(finished.len())).await {
						Ok(()) => finished.into_iter().for_each(|(key, _)| {
							sent.at.insert(key, now);
						}),
						Err(error) => self.say("a push failed", &error),
					}
					changed = true;
				}
			}
			PhoneMode::Own => {
				for (key, row) in needs {
					let note = Note {
						title: "Jocasta: needs you".to_owned(),
						body: note_body(&row.text),
						priority: "high",
						actions: None,
					};
					match self.publish(&note).await {
						Ok(()) => {
							sent.at.insert(key, now);
						}
						Err(error) => self.say("a push failed", &error),
					}
					changed = true;
				}
				for (key, item) in finished {
					let whose = item.project.as_deref().map(|project| format!("{project}: ")).unwrap_or_default();
					let note = Note {
						title: "Jocasta: finished".to_owned(),
						body: note_body(&format!("{whose}{} — {}", item.title, item.line)),
						priority: "default",
						actions: None,
					};
					match self.publish(&note).await {
						Ok(()) => {
							sent.at.insert(key, now);
						}
						Err(error) => self.say("a push failed", &error),
					}
					changed = true;
				}
			}
		}
		if changed {
			sent.save(now);
		}
	}

	/// A new Allow question: pushed at once, quiet hours or not. Questions his keys opened are not.
	async fn push_allow(self: &Arc<Self>, popup: Popup) {
		let view = popup.view();
		if popup.from_key() || !popup.open() || view.choices != [ALLOW, DENY] {
			return;
		}
		if self.mode == PhoneMode::Public {
			if let Err(error) = self.publish(&Note::plain("Jocasta: an Allow is waiting", "high")).await {
				self.say("a push failed", &error);
			}
			return;
		}
		let (allow, deny) = (random_hex(), random_hex());
		{
			let mut tokens = locked(&self.tokens);
			tokens.insert(allow.clone(), (popup.code().to_owned(), ALLOW));
			tokens.insert(deny.clone(), (popup.code().to_owned(), DENY));
		}
		let me = Arc::clone(self);
		let (code, watched) = (popup.code().to_owned(), popup.clone());
		tokio::spawn(async move {
			watched.answer().await;
			me.drop_tokens(&code);
		});
		let button = |name: &str, token: &str| {
			let bearer = self.token.as_deref().unwrap_or_default();
			format!(
				"http, {name}, {}, method=POST, body={token}, clear=true, headers.Authorization=Bearer {bearer}",
				self.url(&self.topics.reply)
			)
		};
		let note = Note {
			title: view.title.clone(),
			body: note_body(&view.message),
			priority: "high",
			actions: Some(format!("{}; {}", button(ALLOW, &allow), button(DENY, &deny))),
		};
		if let Err(error) = self.publish(&note).await {
			self.say("a push failed", &error);
			self.drop_tokens(popup.code());
		}
	}

	fn drop_tokens(&self, code: &str) {
		locked(&self.tokens).retain(|_, (owner, _)| owner != code);
	}

	/// A token that came on the reply stream: it answers its question once, while that is open. A closed question's,
	/// a used and an unknown token do nothing.
	fn accept(&self, token: &str) {
		let found = locked(&self.tokens).remove(token);
		let Some((code, choice)) = found else {
			log(LogLevel::Info, "phone", "refused a reply that holds no live token", None);
			return;
		};
		let Some(popup) = self.popups.get(&code) else {
			log(LogLevel::Info, "phone", "refused a token whose question is closed", None);
			return;
		};
		let answer = if choice == ALLOW {
			json!({"choice": ALLOW, "typed": popup.view().code})
		} else {
			json!({"choice": DENY})
		};
		if self.popups.answer(&code, &answer) == AnswerTaken::Taken {
			log(LogLevel::Info, "phone", &format!("his phone answered {choice}"), None);
		}
		self.drop_tokens(&code);
	}

	/// One reply-stream line; `since` follows the last message's id.
	fn line(&self, line: &[u8], since: &mut String) -> bool {
		let Ok(Value::Object(event)) = serde_json::from_slice::<Value>(line) else { return false };
		if event.get("event").and_then(Value::as_str) != Some("message") {
			return false;
		}
		if let Some(id) = event.get("id").and_then(Value::as_str) {
			id.clone_into(since);
		}
		if let Some(token) = event.get("message").and_then(Value::as_str) {
			self.accept(token.trim());
		}
		true
	}

	/// One connection to the reply stream, to its end; whether a message came over it.
	async fn read_stream(&self, since: &mut String) -> bool {
		let dead = Duration::from_millis(self.timing.dead_ms);
		let mut request = self.client.get(format!("{}/json?since={since}", self.url(&self.topics.reply)));
		if let Some(token) = &self.token {
			request = request.bearer_auth(token);
		}
		let mut response = match tokio::time::timeout(dead, request.send()).await {
			Ok(Ok(response)) if response.status().is_success() => response,
			Ok(Ok(response)) => {
				self.say("the reply stream refused", &response.status().to_string());
				return false;
			}
			Ok(Err(error)) => {
				self.say("the reply stream didn't open", &error.without_url().to_string());
				return false;
			}
			Err(_) => return false,
		};
		let (mut buffer, mut got) = (Vec::<u8>::new(), false);
		while let Ok(Ok(Some(bytes))) = tokio::time::timeout(dead, response.chunk()).await {
			buffer.extend_from_slice(&bytes);
			while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
				let line: Vec<u8> = buffer.drain(..=end).collect();
				got |= self.line(&line, since);
			}
		}
		got
	}

	/// The reply stream, reconnected for good: back-off from `backoff_start_ms` doubling to `backoff_max_ms`, back to
	/// the start after a connection that brought a message. The first connection asks only for what comes after now.
	async fn listen(self: Arc<Self>) {
		let mut since = (now_ms() / 1000).to_string();
		let mut delay = self.timing.backoff_start_ms;
		loop {
			if self.read_stream(&mut since).await {
				delay = self.timing.backoff_start_ms;
			}
			tokio::time::sleep(Duration::from_millis(delay)).await;
			delay = next_delay(delay, self.timing.backoff_max_ms);
		}
	}
}

/// The running phone check-in.
pub struct Phone {
	core: Arc<Core>,
	tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Phone {
	/// Starts the check-in; `None` when `server` is empty. The mode is chosen here, once.
	pub fn start(options: PhoneOptions) -> Option<Arc<Phone>> {
		let server = options.ntfy.server.as_deref().unwrap_or_default().trim().trim_end_matches('/').to_owned();
		if server.is_empty() {
			return None;
		}
		let mode = mode_of(&options.ntfy);
		let core = Arc::new(Core {
			client: reqwest::Client::new(),
			server,
			mode,
			// In public mode the token is never kept, so it can't be sent.
			token: options.ntfy.token.filter(|_| mode == PhoneMode::Own),
			topics: topics(&options.data_dir),
			facts: options.facts,
			timing: options.timing,
			tokens: Mutex::new(HashMap::new()),
			sent: tokio::sync::Mutex::new(Sent::load(&options.data_dir)),
			popups: options.popups,
		});
		let mut opened = core.popups.opened.subscribe();
		let mut tasks = Vec::new();
		let me = Arc::clone(&core);
		tasks.push(tokio::spawn(async move {
			use tokio::sync::broadcast::error::RecvError;
			loop {
				match opened.recv().await {
					Ok(popup) => me.push_allow(popup).await,
					Err(RecvError::Lagged(_)) => {}
					Err(RecvError::Closed) => break,
				}
			}
		}));
		let me = Arc::clone(&core);
		tasks.push(tokio::spawn(async move {
			let mut every = tokio::time::interval(Duration::from_millis(me.timing.tick_ms.max(1)));
			every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
			loop {
				every.tick().await;
				me.tick().await;
			}
		}));
		if mode == PhoneMode::Own {
			tasks.push(tokio::spawn(Arc::clone(&core).listen()));
		}
		Some(Arc::new(Self { core, tasks: Mutex::new(tasks) }))
	}

	/// The mode chosen at start.
	pub fn mode(&self) -> PhoneMode {
		self.core.mode
	}

	/// Stops the loops and forgets every token.
	pub async fn stop(&self) {
		let tasks = std::mem::take(&mut *locked(&self.tasks));
		for task in &tasks {
			task.abort();
		}
		for task in tasks {
			let _ = task.await;
		}
		locked(&self.core.tokens).clear();
	}
}

#[cfg(test)]
#[path = "phone_tests.rs"]
mod tests;
