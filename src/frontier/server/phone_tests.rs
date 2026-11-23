//! Tests of `phone.rs` (0672) against a local axum fake of an ntfy server. Nothing sleeps: `Phone::tick` is awaited
//! (its pushes are done when it returns), and what runs on its own (the Allow pushes, the reply stream) is awaited
//! through the fake's request counter.

use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::response::Response;
use std::convert::Infallible;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

use super::*;
use crate::frontier::protocol::PopupView;
use crate::frontier::server::popups::{Counts, PopupEnd, allow_question, stop_question};
use crate::shared::testing::{TempDir, temp_dir};

const SECRET: &str = "SECRET-sentinel";
const OWN_TOKEN: &str = "tk_example_token";

#[derive(Clone, Debug)]
struct Seen {
	method: String,
	path: String,
	query: String,
	headers: Vec<(String, String)>,
	body: String,
}

impl Seen {
	fn header(&self, name: &str) -> Option<&str> {
		self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
	}

	fn everything(&self) -> String {
		let headers: Vec<String> = self.headers.iter().map(|(key, value)| format!("{key}: {value}")).collect();
		format!("{} {}?{}\n{}\n{}", self.method, self.path, self.query, headers.join("\n"), self.body)
	}
}

type Line = Result<Bytes, Infallible>;

struct FakeState {
	seen: Mutex<Vec<Seen>>,
	count: watch::Sender<usize>,
	streams: Mutex<Vec<mpsc::UnboundedSender<Line>>>,
}

/// A local ntfy: records every request; a GET of `.../json` is a stream the test writes lines to.
struct Fake {
	url: String,
	state: Arc<FakeState>,
}

async fn handle(State(state): State<Arc<FakeState>>, request: Request) -> Response {
	let (parts, body) = request.into_parts();
	let body = axum::body::to_bytes(body, 1 << 20).await.unwrap_or_default();
	let seen = Seen {
		method: parts.method.to_string(),
		path: parts.uri.path().to_owned(),
		query: parts.uri.query().unwrap_or_default().to_owned(),
		headers: parts
			.headers
			.iter()
			.map(|(key, value)| (key.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
			.collect(),
		body: String::from_utf8_lossy(&body).into_owned(),
	};
	let stream = seen.method == "GET" && seen.path.ends_with("/json");
	let total = {
		let mut all = state.seen.lock().unwrap();
		all.push(seen);
		all.len()
	};
	let response = if stream {
		let (tx, rx) = mpsc::unbounded_channel::<Line>();
		let _ = tx.send(Ok(Bytes::from_static(b"{\"id\":\"open1\",\"event\":\"open\"}\n")));
		state.streams.lock().unwrap().push(tx);
		let items = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });
		Response::new(Body::from_stream(items))
	} else {
		Response::new(Body::from("{}"))
	};
	state.count.send_replace(total);
	response
}

impl Fake {
	async fn start() -> Self {
		let state = Arc::new(FakeState {
			seen: Mutex::new(Vec::new()),
			count: watch::channel(0).0,
			streams: Mutex::new(Vec::new()),
		});
		let app = axum::Router::new().fallback(handle).with_state(Arc::clone(&state));
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://{}", listener.local_addr().unwrap());
		tokio::spawn(async move {
			let _ = axum::serve(listener, app).await;
		});
		Self { url, state }
	}

	fn seen(&self) -> Vec<Seen> {
		self.state.seen.lock().unwrap().clone()
	}

	fn posts(&self) -> Vec<Seen> {
		self.seen().into_iter().filter(|seen| seen.method == "POST").collect()
	}

	fn gets(&self) -> Vec<Seen> {
		self.seen().into_iter().filter(|seen| seen.method == "GET").collect()
	}

	/// Waits until `done` holds of what was seen.
	async fn until(&self, done: impl Fn(&[Seen]) -> bool) {
		let mut changes = self.state.count.subscribe();
		let waiting = async {
			loop {
				if done(&self.seen()) {
					return;
				}
				changes.changed().await.unwrap();
			}
		};
		tokio::time::timeout(Duration::from_secs(20), waiting)
			.await
			.expect("the fake never saw what the test waited for");
	}

	async fn posts_reach(&self, count: usize) {
		self.until(|seen| seen.iter().filter(|s| s.method == "POST").count() >= count).await;
	}

	async fn gets_reach(&self, count: usize) {
		self.until(|seen| seen.iter().filter(|s| s.method == "GET").count() >= count).await;
	}

	/// A line on the newest reply stream.
	fn send(&self, line: Value) {
		let streams = self.state.streams.lock().unwrap();
		streams.last().expect("a stream is open").send(Ok(Bytes::from(format!("{line}\n")))).unwrap();
	}

	/// Ends every open stream, as a server that hangs up.
	fn hang_up(&self) {
		self.state.streams.lock().unwrap().clear();
	}
}

/// The shared facts a test edits.
#[derive(Default)]
struct World {
	needs: Vec<NeedsYou>,
	finished: Vec<LandedItem>,
	quiet: bool,
}

struct Rig {
	fake: Fake,
	popups: Arc<Popups>,
	world: Arc<Mutex<World>>,
	dir: TempDir,
}

impl Rig {
	async fn new() -> Self {
		Self {
			fake: Fake::start().await,
			popups: Arc::new(Popups::new()),
			world: Arc::new(Mutex::new(World::default())),
			dir: temp_dir("phone"),
		}
	}

	fn timing() -> Timing {
		Timing { tick_ms: 3_600_000, backoff_start_ms: 5, backoff_max_ms: 20, dead_ms: 20_000 }
	}

	fn options(&self, own_server: bool, token: Option<&str>, timing: Timing) -> PhoneOptions {
		let world = Arc::clone(&self.world);
		PhoneOptions {
			ntfy: NtfySettings { server: Some(self.fake.url.clone()), own_server, token: token.map(str::to_owned) },
			popups: Arc::clone(&self.popups),
			facts: Arc::new(move || {
				let world = world.lock().unwrap();
				PhoneFacts { needs: world.needs.clone(), finished: world.finished.clone(), quiet: world.quiet }
			}),
			data_dir: self.dir.path().to_path_buf(),
			timing,
		}
	}

	fn public(&self) -> Arc<Phone> {
		Phone::start(self.options(false, Some(OWN_TOKEN), Self::timing())).expect("on")
	}

	fn own(&self) -> Arc<Phone> {
		Phone::start(self.options(true, Some(OWN_TOKEN), Self::timing())).expect("on")
	}

	fn needs(&self, rows: &[NeedsYou]) {
		self.world.lock().unwrap().needs = rows.to_vec();
	}
}

fn row(title: &str, what: &str) -> NeedsYou {
	serde_json::from_value(json!({
		"kind": "review",
		"text": format!("{title} — {what}"),
		"paneId": format!("pane-{title}"),
		"project": format!("{SECRET}-project"),
	}))
	.unwrap()
}

fn landed(title: &str) -> LandedItem {
	serde_json::from_value(json!({
		"title": title,
		"project": format!("{SECRET}-project"),
		"line": format!("{SECRET} line C:/secret/path"),
		"paneId": format!("pane-{title}"),
	}))
	.unwrap()
}

fn allow_box(message: &str) -> PopupView {
	allow_question(message, &format!("{SECRET} detail"), Some("ABC"))
}

/// The `body=<token>` of each button of an `Actions` header: (Allow's, Deny's).
fn tokens_of(seen: &Seen) -> (String, String) {
	let actions = seen.header("actions").expect("an Actions header");
	let mut found = actions.split(';').map(|button| {
		let at = button.find("body=").expect("a body") + "body=".len();
		button[at..].split(',').next().unwrap().trim().to_owned()
	});
	(found.next().unwrap(), found.next().unwrap())
}

fn reply(token: &str, id: &str) -> Value {
	json!({"id": id, "event": "message", "message": token})
}

#[tokio::test]
async fn nothing_is_asked_of_the_network_until_ntfy_server_is_set() {
	let rig = Rig::new().await;
	let mut options = rig.options(true, Some(OWN_TOKEN), Rig::timing());
	options.ntfy.server = None;
	assert!(Phone::start(options).is_none());
	rig.needs(&[row("one", "waits")]);
	rig.popups.open(&allow_box("Remove it?"), false);
	assert!(rig.fake.seen().is_empty());
}

#[tokio::test]
async fn a_public_server_never_gets_text_buttons_or_the_token_and_pushes_a_kind_and_a_count_once_per_tick() {
	for (own_server, token) in [(false, Some(OWN_TOKEN)), (true, None), (false, None)] {
		let rig = Rig::new().await;
		let phone = Phone::start(rig.options(own_server, token, Rig::timing())).unwrap();
		assert_eq!(phone.mode(), PhoneMode::Public);
		rig.needs(&[row("alpha", &format!("{SECRET} detail")), row("beta", "waits")]);
		rig.world.lock().unwrap().finished = vec![landed("gamma")];
		phone.core.tick().await;
		let titles: Vec<String> =
			rig.fake.posts().iter().map(|post| post.header("title").unwrap().to_owned()).collect();
		assert_eq!(titles, ["Jocasta: 2 need you", "Jocasta: a thread finished"]);
		// One new row later is one push; the rows already told are not.
		rig.needs(&[row("alpha", "waits"), row("beta", "waits"), row("delta", "waits")]);
		phone.core.tick().await;
		assert_eq!(rig.fake.posts().last().unwrap().header("title"), Some("Jocasta: 1 needs you"));
		phone.core.tick().await;
		assert_eq!(rig.fake.posts().len(), 3);
		// An Allow is told without its question.
		let popup = rig.popups.open(&allow_box(&format!("{SECRET} message")), false);
		rig.fake.posts_reach(4).await;
		let allow = rig.fake.posts().remove(3);
		assert_eq!(allow.header("title"), Some("Jocasta: an Allow is waiting"));
		assert!(popup.open());
		for seen in rig.fake.seen() {
			let text = seen.everything();
			assert!(!text.contains(SECRET) && !text.contains("secret"), "{text}");
			assert!(seen.header("actions").is_none() && seen.header("authorization").is_none(), "{text}");
		}
		// No reply stream, whatever the settings say.
		assert!(rig.fake.gets().is_empty());
		phone.stop().await;
	}
}

#[tokio::test]
async fn quiet_hours_hold_needs_and_finished_pushes_but_not_an_allow_and_a_held_row_is_pushed_after() {
	let rig = Rig::new().await;
	let phone = rig.public();
	rig.needs(&[row("alpha", "waits")]);
	rig.world.lock().unwrap().quiet = true;
	phone.core.tick().await;
	assert!(rig.fake.posts().is_empty());
	rig.popups.open(&allow_box("Remove it?"), false);
	rig.fake.posts_reach(1).await;
	assert_eq!(rig.fake.posts()[0].header("title"), Some("Jocasta: an Allow is waiting"));
	rig.world.lock().unwrap().quiet = false;
	phone.core.tick().await;
	assert_eq!(rig.fake.posts().len(), 2);
	assert_eq!(rig.fake.posts()[1].header("title"), Some("Jocasta: 1 needs you"));
}

#[tokio::test]
async fn a_row_is_pushed_once_across_a_restart_and_a_key_opened_or_other_question_is_not_pushed() {
	let rig = Rig::new().await;
	let first = rig.own();
	rig.needs(&[row("alpha", "waits")]);
	first.core.tick().await;
	assert_eq!(rig.fake.posts().len(), 1);
	first.stop().await;
	let again = rig.own();
	again.core.tick().await;
	assert_eq!(rig.fake.posts().len(), 1);
	// Opened by his key, or not an Allow: nothing; then a real one, whose push is the first of the Allow pushes.
	rig.popups.open(&allow_box("By key?"), true);
	rig.popups.open(&stop_question(Counts { working: 1, awaiting: 0 }, None), false);
	rig.popups.open(&allow_box("Remove it?"), false);
	rig.fake.posts_reach(2).await;
	assert_eq!(rig.fake.posts()[1].body, "Remove it?");
	assert_eq!(rig.fake.posts().len(), 2);
	again.stop().await;
}

#[test]
fn what_was_pushed_is_forgotten_after_a_week_and_the_backoff_doubles_to_its_ceiling() {
	let dir = temp_dir("phone-sent");
	let now = now_ms();
	let old = now - SENT_KEEP_MS - 1;
	std::fs::write(dir.path().join(SENT_FILE), format!("{{\"old\": {old}, \"new\": {now}}}")).unwrap();
	let sent = Sent::load(dir.path());
	assert!(!sent.at.contains_key("old") && sent.at.contains_key("new"));
	assert_eq!(next_delay(1_000, 60_000), 2_000);
	assert_eq!(next_delay(40_000, 60_000), 60_000);
	assert_eq!(next_delay(60_000, 60_000), 60_000);
}

#[test]
fn the_topics_are_made_once_long_different_and_kept() {
	let dir = temp_dir("phone-topics");
	assert_eq!(read_topics(dir.path()), None);
	let made = topics(dir.path());
	assert!(made.main.len() >= 64 && made.reply.len() >= 64);
	assert_ne!(made.main, made.reply);
	assert!(made.main.bytes().all(|byte| byte.is_ascii_hexdigit()));
	assert_eq!(topics(dir.path()), made);
	assert_eq!(read_topics(dir.path()), Some(made.clone()));
	std::fs::write(dir.path().join(TOPICS_FILE), "{\"main\":\"short\",\"reply\":\"short\"}").unwrap();
	assert_eq!(read_topics(dir.path()), None);
	assert_ne!(topics(dir.path()), made);
	let ntfy =
		|own_server, token: Option<&str>| NtfySettings { server: None, own_server, token: token.map(str::to_owned) };
	assert_eq!(mode_of(&ntfy(true, Some("t"))), PhoneMode::Own);
	assert_eq!(mode_of(&ntfy(true, None)), PhoneMode::Public);
	assert_eq!(mode_of(&ntfy(false, Some("t"))), PhoneMode::Public);
}

#[tokio::test]
async fn the_phone_posts_to_the_generated_topic_and_listens_on_the_other() {
	let rig = Rig::new().await;
	let phone = rig.own();
	rig.needs(&[row("alpha", "waits")]);
	phone.core.tick().await;
	rig.fake.gets_reach(1).await;
	let made = read_topics(rig.dir.path()).unwrap();
	assert_eq!(rig.fake.posts()[0].path, format!("/{}", made.main));
	assert_eq!(rig.fake.gets()[0].path, format!("/{}/json", made.reply));
	phone.stop().await;
}

#[tokio::test]
async fn on_his_own_server_an_allow_pushes_its_message_cut_to_240_with_two_buttons_and_every_request_carries_his_token()
{
	let rig = Rig::new().await;
	let phone = rig.own();
	assert_eq!(phone.mode(), PhoneMode::Own);
	let long = "x".repeat(400);
	rig.popups.open(&allow_box(&long), false);
	rig.fake.posts_reach(1).await;
	rig.fake.gets_reach(1).await;
	let push = rig.fake.posts().remove(0);
	assert_eq!(push.body, note_body(&long));
	assert_eq!(push.body.encode_utf16().count(), 240);
	assert!(!push.everything().contains(SECRET), "the detail never leaves");
	let actions = push.header("actions").unwrap();
	assert!(actions.contains("http, Allow, ") && actions.contains("http, Deny, "), "{actions}");
	assert!(actions.contains("method=POST") && actions.contains("clear=true"));
	assert!(actions.contains(&format!("headers.Authorization=Bearer {OWN_TOKEN}")));
	let (allow, deny) = tokens_of(&push);
	assert_ne!(allow, deny);
	assert!(allow.len() >= 64 && deny.len() >= 64);
	let made = read_topics(rig.dir.path()).unwrap();
	assert!(actions.contains(&format!("{}/{}", rig.fake.url, made.reply)));
	for seen in rig.fake.seen() {
		assert_eq!(seen.header("authorization"), Some(format!("Bearer {OWN_TOKEN}").as_str()), "{}", seen.path);
	}
	phone.stop().await;
}

#[tokio::test]
async fn a_token_answers_only_its_open_question_once_and_only_from_the_reply_stream_allow_with_the_typed_code() {
	let rig = Rig::new().await;
	let phone = rig.own();
	let mut asked = Vec::new();
	for message in ["first?", "second?", "third?"] {
		let popup = rig.popups.open(&allow_box(message), false);
		asked.push(popup);
		rig.fake.posts_reach(asked.len()).await;
	}
	let posts = rig.fake.posts();
	let (first, second, third) = (tokens_of(&posts[0]), tokens_of(&posts[1]), tokens_of(&posts[2]));
	rig.fake.gets_reach(1).await;
	// Refused: an unknown token, and the token of a question that closed meanwhile.
	asked[1].close(None);
	rig.fake.send(reply("deadbeef", "m1"));
	rig.fake.send(reply(&second.0, "m2"));
	// Taken: Allow with the typed code, as if he'd typed it.
	rig.fake.send(reply(&first.0, "m3"));
	assert_eq!(asked[0].answer().await, ALLOW);
	assert_eq!(asked[0].how().await, PopupEnd::Answered);
	assert_eq!(asked[1].how().await, PopupEnd::Withdrawn);
	assert_eq!(asked[1].answer().await, DENY);
	assert!(asked[2].open(), "the others' tokens were not touched");
	// A used token, and the other button of an answered question, find nothing.
	assert!(locked(&phone.core.tokens).values().all(|(code, _)| code == asked[2].code()));
	rig.fake.send(reply(&first.1, "m4"));
	rig.fake.send(reply(&third.1, "m5"));
	assert_eq!(asked[2].answer().await, DENY);
	assert_eq!(asked[2].how().await, PopupEnd::Answered);
	assert!(locked(&phone.core.tokens).is_empty());
	phone.stop().await;
}

#[tokio::test]
async fn the_typed_code_still_answers_while_the_phone_runs_and_a_stopped_phone_forgets_its_tokens() {
	let rig = Rig::new().await;
	let phone = rig.own();
	let popup = rig.popups.open(&allow_box("Remove it?"), false);
	rig.fake.posts_reach(1).await;
	assert_eq!(rig.popups.answer(popup.code(), &json!({"choice": ALLOW, "typed": "abc"})), AnswerTaken::Taken);
	assert_eq!(popup.answer().await, ALLOW);
	let again = rig.popups.open(&allow_box("Again?"), false);
	rig.fake.posts_reach(2).await;
	assert!(!locked(&phone.core.tokens).is_empty());
	phone.stop().await;
	assert!(locked(&phone.core.tokens).is_empty());
	assert!(again.open());
}

#[tokio::test]
async fn the_reply_stream_reconnects_from_the_last_id_and_the_first_connection_starts_from_now() {
	let rig = Rig::new().await;
	let before = now_ms() / 1000;
	let phone = rig.own();
	rig.fake.gets_reach(1).await;
	let query = rig.fake.gets()[0].query.clone();
	let since: i64 = query.strip_prefix("since=").expect("a since").parse().expect("a unix time");
	assert!(since >= before && since <= now_ms() / 1000, "{query}");
	rig.fake.send(reply("deadbeef", "id-7"));
	rig.fake.hang_up();
	rig.fake.gets_reach(2).await;
	assert_eq!(rig.fake.gets()[1].query, "since=id-7");
	phone.stop().await;
}

#[tokio::test]
async fn a_reply_stream_with_no_bytes_for_the_dead_time_is_dropped_and_connected_again() {
	let rig = Rig::new().await;
	let timing = Timing { dead_ms: 100, ..Rig::timing() };
	let phone = Phone::start(rig.options(true, Some(OWN_TOKEN), timing)).unwrap();
	rig.fake.gets_reach(2).await;
	phone.stop().await;
}
