//! The Jocasta service's server (the Rust port of `server.ts`; 4.1, 0392, 0552): 127.0.0.1 on `service.port` (0358), or
//! a free port for this run when that one is busy; `<data>/jocasta-service.json` with `port`, `token` and `pid`. Every
//! API call carries the token as a bearer (`jocasta mcp`, bare `jocasta`, the benchmark's clients); the browser's pages carry
//! a separate page token as a per-port cookie, set from a one-time launch code (`/?t=<code>`, SEC-NET-4, SEC-LOC-11). It
//! serves the MCP tools, the state feed, his questions and whiteboard's canvas page (0393), and runs the herdr-backed
//! service. A start that fails stops what it had started, its relay included (SRV-A-2).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::http::{HeaderName, HeaderValue};
use axum::response::Response;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::frontier::protocol::{
	ClientMessage, PluginAction, PopupView, ServerMessage, ServiceHealth, ServiceName, ServiceState, TOKEN_QUERY,
};
use crate::frontier::server::allow::{Answer, CONFIRM_WAIT_MS, Confirm, Confirmation};
use crate::frontier::server::asker::{Ask, Show, asker};
use crate::frontier::server::close::{CloseFlow, CloseHow, CloseOutcome, CloseParts, CloseService, close_flow};
use crate::frontier::server::day::{DayOptions, SharedCatalog};
use crate::frontier::server::door_routes::{DoorParts, DoorService, door_routes};
use crate::frontier::server::feed::{FeedService, Guard, Reply, feed_route, feed_socket, publish};
use crate::frontier::server::health_route::health_route;
use crate::frontier::server::herdr::{Herdr, HerdrOptions, Note, connect_herdr};
use crate::frontier::server::native::{Native, NativeServer};
use crate::frontier::server::page_routes::{LaunchCodes, PageParts, PageResponder, page_responder, whiteboard_reviews};
use crate::frontier::server::popups::{ALLOW, AnswerTaken, Popup, Popups, allow_question};
use crate::frontier::server::relay::{Relay, start_relay};
use crate::frontier::server::routes::{Route, Served, ServerHandle, listen};
use crate::frontier::server::service::{FindReview, Heal, JocastaService, OpenReview, RelayUrlOf, ServiceOptions};
use crate::frontier::server::stop_sequence::{
	StopBounds, StopParts, StopRun, StopService, StopStep, StopWhen, Stopper, stop_steps, stopper, undo_start,
};
use crate::frontier::server::thread_keys::{ActionService, KeyParts, ThreadKeys};
use crate::frontier::server::whiteboard::{
	ServiceStatus, Whiteboard, WhiteboardOptions, is_stage_file, start_whiteboard,
};
use crate::shared::credentials::take_calendar_env;
use crate::shared::errors::Result;
use crate::shared::herdr_session::herdr_session;
use crate::shared::link::open_in_browser;
use crate::shared::log::{LogLevel, log};
use crate::shared::process::AbortSignal;
use crate::shared::schema::open_catalog;
use crate::shared::service_file::{ServiceFile, remove_service_file, write_service_file};
use crate::shared::settings::{ServiceKey, read_settings, service_setting};

/// `service.port`'s default (0358).
const DEFAULT_PORT: u16 = 47_400;

/// How long the service's own stop may take on a failed start (the TypeScript's `ms: 30_000`).
const SERVICE_UNDO_MS: u64 = 30_000;

/// What sends a message to every state-feed client.
type Broadcast = Arc<dyn Fn(ServerMessage) + Send + Sync>;

/// Opens a terminal on herdr's session for the close question.
pub type Reattach = Arc<dyn Fn() -> Result<()> + Send + Sync>;

/// What shows a question to him: it gets the question's one-time code.
pub type OnQuestion = Arc<dyn Fn(&Popup) + Send + Sync>;

/// What the server was started with: everything the TypeScript's `ServerOptions` names. The options that pass through
/// to the service are `ServiceOptions`' of the same name; the rest are the server's own. An unset option takes its
/// default.
#[derive(Default)]
pub struct ServerOptions {
	/// How omp starts: `["omp"]` by default.
	pub omp: Option<Vec<String>>,
	/// How hunk starts; `Some(None)` is none installed.
	pub hunk: Option<Option<Vec<String>>>,
	/// The loopback port of Jocasta's own hunk session daemon.
	pub hunk_port: Option<u16>,
	/// The boss pane's argv.
	pub boss_command: Option<Vec<String>>,
	/// A prompt file every project and thread pane appends; `Some(None)` appends nothing.
	pub pane_prompt: Option<Option<PathBuf>>,
	/// How gh starts.
	pub gh: Option<Vec<String>>,
	/// How long one gh call may take before it is ended.
	pub gh_timeout_ms: Option<i64>,
	/// omp's sessions dir.
	pub sessions_dir: Option<PathBuf>,
	/// How long `thread_start` waits for omp to come up in the new pane.
	pub agent_wait_ms: Option<i64>,
	/// How long a project pane's check request waits for its check (0617).
	pub pane_check_wait_ms: Option<i64>,
	/// How often panes, sidebar tokens and the HUD are refreshed.
	pub refresh_ms: Option<i64>,
	/// A whiteboard review by id; overrides whiteboard's own.
	pub find_review: Option<FindReview>,
	/// Opens a whiteboard review in his browser; overrides whiteboard's own.
	pub open_review: Option<OpenReview>,
	/// How long `thread_start` waits for the pane to start working before one Enter.
	pub submit_wait_ms: Option<i64>,
	/// mise's argv; `Some(None)` runs none.
	pub mise: Option<Option<Vec<String>>>,
	/// How long the weekly big-picture review may run (0470).
	pub weekly_review_ms: Option<i64>,
	/// How long the project's `scorecard` task may run (0616).
	pub scorecard_ms: Option<i64>,
	/// `Some(false)`: the minute tick never starts a weekly review (0470).
	pub weekly_reviews: Option<bool>,
	/// Asks him Allow or Deny (0484); his question through this server's question API by default.
	pub confirm: Option<Confirm>,
	/// How long an ask waits for his answer.
	pub confirm_wait_ms: Option<i64>,
	/// The boss pane's key (0484).
	pub boss_key: Option<String>,
	/// How long the end of a thread pane's omp is waited for before its leftovers end (0490).
	pub leftover_wait_ms: Option<i64>,
	/// When leftovers are looked for again (0490).
	pub leftover_again_ms: Option<i64>,
	/// `collab.relayUrl` as the omp of a command resolves it.
	pub relay_url_of: Option<RelayUrlOf>,
	/// omp's host registry folder.
	pub hosts_dir: Option<PathBuf>,
	/// How often the thread panes' omp hosts are asked for their state.
	pub host_poll_ms: Option<i64>,
	/// `Some(false)`: a task end without a `report_result` goes straight to its review (0539).
	pub ask_report: Option<bool>,
	/// How long a plan-mode turn's end waits for the session to show its proposal; `PLAN_SETTLE_MS` (test seam).
	pub plan_settle_ms: Option<i64>,
	/// The collab relay's port on 127.0.0.1 (test seam): `RELAY_PORT`; `Some(0)` a free one. Taken: the start fails
	/// (0561).
	pub relay_port: Option<u16>,
	/// herdr (test seam): the server this service lives in by default.
	pub herdr: Option<Arc<dyn Herdr>>,
	/// herdr's session, for its name in the stop steps and the panes' commands (smoke seam); `jocasta` by default.
	pub herdr_session: Option<String>,
	/// The API socket of the herdr server this service lives in, as clients connect to it; the session's when absent.
	pub herdr_socket: Option<String>,
	/// Phase 8's pollers and clock (test seam); tests pass `pollers: Some(false)` so nothing calls the real gh or az.
	pub day: Option<DayOptions>,
	/// The port to try first (test seam): `service.port`, else 47400 (0358).
	pub port: Option<u16>,
	/// whiteboard's server (0393): started from the install or the built clone by default (`None`); `Some(None)` for
	/// none (tests).
	pub whiteboard: Option<Option<WhiteboardOptions>>,
	/// Shows a question to him (test seam): it gets the question's one-time code, and the shell shows none.
	pub on_question: Option<OnQuestion>,
	/// herdr's client shells (fork plan phase 5): the dock beside the boss, the Agents view, his questions and his
	/// thread keys. The service herdr's server runs has them; a test's server has none unless set.
	pub shell: Option<Arc<Native>>,
	/// Opens a new terminal attached to herdr's session, as bare `jocasta` does (0549): his client closed while
	/// threads are at work, so the close question has somewhere to show. A new Windows Terminal window running
	/// `jocasta` by default (test seam).
	pub reattach: Option<Reattach>,
	/// Each stop step's bound in ms (test seam): service 30 s, herdr 15 s, whiteboard 5 s.
	pub stop_bounds_ms: StopBounds,
}

impl ServerOptions {
	/// The options that pass through to the service: all but the server's own.
	fn pass_on(&mut self, service: &mut ServiceOptions) {
		service.omp = self.omp.take();
		service.hunk = self.hunk.take();
		service.hunk_port = self.hunk_port;
		service.boss_command = self.boss_command.take();
		service.pane_prompt = self.pane_prompt.take();
		service.gh = self.gh.take();
		service.gh_timeout_ms = self.gh_timeout_ms;
		service.plan_settle_ms = self.plan_settle_ms;
		service.sessions_dir = self.sessions_dir.take();
		service.agent_wait_ms = self.agent_wait_ms;
		service.pane_check_wait_ms = self.pane_check_wait_ms;
		service.refresh_ms = self.refresh_ms;
		service.submit_wait_ms = self.submit_wait_ms;
		service.mise = self.mise.take();
		service.weekly_review_ms = self.weekly_review_ms;
		service.scorecard_ms = self.scorecard_ms;
		service.weekly_reviews = self.weekly_reviews;
		service.confirm_wait_ms = self.confirm_wait_ms;
		service.boss_key = self.boss_key.take();
		service.leftover_wait_ms = self.leftover_wait_ms;
		service.leftover_again_ms = self.leftover_again_ms;
		service.relay_port = self.relay_port;
		service.relay_url_of = self.relay_url_of.take();
		service.hosts_dir = self.hosts_dir.take();
		service.host_poll_ms = self.host_poll_ms;
		service.ask_report = self.ask_report;
		service.day = self.day.take();
	}
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The running server: what a caller may ask of it. Cheap to clone; every clone is the same server.
#[derive(Clone)]
pub struct JocastaServer {
	pub port: u16,
	/// The API token: the bearer of every call (`jocasta-service.json`); never a cookie or in a URL (SEC-NET-4).
	pub token: String,
	/// The collab relay's port on 127.0.0.1 (0561).
	pub relay_port: u16,
	pub service: Arc<JocastaService>,
	/// whiteboard's server, the service's child (0393); `None` when the server was started without it.
	pub whiteboard: Option<Arc<Whiteboard>>,
	inner: Arc<Inner>,
}

struct Inner {
	codes: Arc<LaunchCodes>,
	stops: Arc<Stopper>,
	close: CloseFlow,
	question: Ask,
	popups: Arc<Popups>,
	keys: Arc<ThreadKeys>,
	herdr: Arc<dyn Herdr>,
}

impl JocastaServer {
	/// A browser page's URL, `http://127.0.0.1:<port>/?t=<code>`, with a fresh one-time launch code each time it is
	/// read (SEC-LOC-11): used once within a minute, it becomes the page cookie.
	pub fn url(&self) -> String {
		page_url(self.port, &self.inner.codes, "")
	}

	/// The service closes and `jocasta-service.json` goes away; herdr and every pane keep running (a failed start).
	pub async fn stop(&self) {
		self.inner.stops.stop(false).await;
	}

	/// Closes Jocasta as one (0434, 0549): the quit action's close. With `ask` and any thread working or awaiting him,
	/// the close question is asked first (Stop everything or Cancel); Cancel leaves everything as it was. Otherwise
	/// everything Jocasta started stops (`stop_everything`) and `stopped` resolves.
	pub async fn shutdown(&self, ask: bool) -> CloseOutcome {
		self.inner.close.close(CloseHow { ask, reattach: false }).await
	}

	/// His herdr client closed (he quit it, or closed its terminal; 0549): with nothing working or awaiting him,
	/// everything stops at once; otherwise a new terminal attaches to the session again and the close question is
	/// asked there.
	pub async fn client_closed(&self) -> CloseOutcome {
		self.inner.close.close(CloseHow { ask: true, reattach: true }).await
	}

	/// Registers something else Jocasta started that stops with it (0549): run on Stop everything after herdr's stops,
	/// bounded by `ms` (default 10 s). Returns the function that unregisters it.
	pub fn stops_with(&self, name: &str, run: StopRun, ms: Option<u64>) -> impl FnOnce() + Send + 'static {
		self.inner.stops.add(StopStep::new(name, run).bounded(ms.unwrap_or(10_000)).when(StopWhen::All))
	}

	/// Resolves once the server has stopped, by `stop()` or a close; every step is bounded, so it always does.
	pub async fn stopped(&self) {
		self.inner.stops.stopped().await;
	}

	/// Resolves once the Jocasta service has started (the boss pane ensured, threads found, sessions read); fails when
	/// it couldn't. The server listens before that.
	pub async fn ready(&self) -> Result<()> {
		self.service.start().await
	}

	/// Asks him a question (0519, 0552): its one-time code goes to what shows it (herdr's client shell in front, a
	/// test's `on_question`) and nowhere else. Every open one ends unanswered when the server stops. `from_key`: one of
	/// his keys opened it (no notification; Allow questions go first, 0578, 0581).
	pub fn question(&self, view: &PopupView, signal: Option<AbortSignal>, from_key: bool) -> Popup {
		(self.inner.question)(view, signal, from_key)
	}

	/// Takes an answer to question `code` as its shell sends it: what it made of it, and the wrong codes it still takes.
	pub fn answer(&self, code: &str, answer: &Value) -> (AnswerTaken, u32) {
		let popups = &self.inner.popups;
		let taken = popups.answer(code, answer);
		(taken, popups.left(code))
	}
}

impl NativeServer for JocastaServer {
	fn answer(&self, code: &str, answer: &Value) -> (AnswerTaken, u32) {
		JocastaServer::answer(self, code, answer)
	}

	fn key(&self, action: PluginAction, pane_id: Option<String>) -> BoxFuture<'static, ()> {
		let keys = Arc::clone(&self.inner.keys);
		async move { keys.run(action, pane_id).await }.boxed()
	}

	fn snapshot(&self) -> Option<ServerMessage> {
		match FeedService::snapshot(&*self.service) {
			Ok(snapshot) => Some(snapshot),
			Err(error) => {
				log(LogLevel::Error, "server", &format!("the shell's snapshot failed: {error}"), None);
				None
			}
		}
	}

	fn handle(&self, message: ClientMessage, reply: Reply) -> BoxFuture<'static, ()> {
		let service = Arc::clone(&self.service);
		async move { FeedService::handle(&*service, message, reply).await }.boxed()
	}

	fn notify(&self, note: Note) -> BoxFuture<'static, ()> {
		let herdr = Arc::clone(&self.inner.herdr);
		async move {
			if let Err(error) = herdr.notify(&note).await {
				log(LogLevel::Warn, "question", &format!("the question's notification failed: {error}"), None);
			}
		}
		.boxed()
	}
}

/// A page URL with a fresh launch code; `query` goes after it (`&review=…`).
fn page_url(port: u16, codes: &LaunchCodes, query: &str) -> String {
	format!("http://127.0.0.1:{port}/?{TOKEN_QUERY}={}{query}", codes.mint())
}

/// 256 random bits as hex.
fn random_hex() -> String {
	let mut bytes = [0_u8; 32];
	getrandom::fill(&mut bytes).expect("the OS has random bytes");
	bytes.iter().fold(String::with_capacity(64), |mut hex, byte| {
		let _ = write!(hex, "{byte:02x}");
		hex
	})
}

/// `service.port` in `jocasta.yaml` (0358, 0387; `frontier.port` still read, 0584), checked by the settings schema: a
/// port number, else 47400. Never fails (0584): a malformed file or a bad value is a settings problem the service
/// notifies once at start, and the start goes on with the default.
fn configured_port() -> u16 {
	read_settings()
		.ok()
		.and_then(|settings| service_setting(&settings, ServiceKey::Port).ok().flatten())
		.and_then(|value| value.as_u64())
		.and_then(|port| u16::try_from(port).ok())
		.unwrap_or(DEFAULT_PORT)
}

/// The herdr server of `session` this service lives in, at `socket` when given; it is never started from here.
async fn connect_session(session: &str, socket: Option<String>) -> Result<Arc<dyn Herdr>> {
	let options = HerdrOptions { session: session.to_owned(), socket_path: socket, ..HerdrOptions::default() };
	Ok(connect_herdr(options).await?)
}

/// His Allow/Deny before a risky boss action (0484), Deny by default and answered only with its one-time code. When the
/// answer isn't wanted any more (the wait ran out, Jocasta is closing), it goes as Deny. How it ended tells the boss the
/// true reason of a Deny (0578).
fn allow_box(question: Ask) -> Confirm {
	Arc::new(move |ask: Confirmation, signal: AbortSignal| {
		let popup = question(&allow_question(&ask.message, &ask.detail, None), Some(signal), false);
		Box::pin(async move {
			let answer = popup.answer().await;
			Ok(Answer { allow: answer == ALLOW, how: popup.how().await })
		})
	})
}

/// The dashboard's services row (0429, 0444): whiteboard, up or why not.
async fn services_row(whiteboard: Option<Arc<Whiteboard>>) -> Vec<ServiceHealth> {
	let status = match whiteboard {
		Some(whiteboard) => whiteboard.check().await,
		None => ServiceStatus { state: ServiceState::Down, note: Some("off in this run".to_owned()) },
	};
	vec![ServiceHealth { name: ServiceName::Whiteboard, state: status.state, note: status.note }]
}

/// The canvas page's Content-Security-Policy (0368, 0552): scripts, styles, workers and fetches come only from this
/// server, plus whiteboard's server on loopback (0393), which the canvas fetches from and shows images of.
/// `wasm-unsafe-eval` lets the canvas's router (libavoid) compile its wasm, and `unsafe-eval` lets its embind glue build
/// functions from strings, so whiteboard's software maps route (0493); inline scripts and other origins stay blocked.
/// Inline styles stay allowed: the canvas and Monaco set `style` attributes, which take no nonce, and a style can't run
/// code. Monaco's worker is this server's (0411).
pub fn content_policy(port: u16, whiteboard: Option<&str>) -> String {
	let other = whiteboard.map_or_else(String::new, |origin| format!(" {origin}"));
	[
		"default-src 'self'".to_owned(),
		"script-src 'self' 'wasm-unsafe-eval' 'unsafe-eval'".to_owned(),
		"style-src 'self' 'unsafe-inline'".to_owned(),
		format!("img-src 'self' data: blob:{other}"),
		"font-src 'self' data:".to_owned(),
		"media-src 'self' data: blob:".to_owned(),
		format!("connect-src 'self' ws://127.0.0.1:{port}{other}"),
		"worker-src 'self'".to_owned(),
		"object-src 'none'".to_owned(),
		"base-uri 'none'".to_owned(),
		"form-action 'none'".to_owned(),
		"frame-ancestors 'none'".to_owned(),
	]
	.join("; ")
}

fn set_header(response: &mut Response, name: &'static str, value: &str) {
	if let Ok(value) = HeaderValue::from_str(value) {
		response.headers_mut().insert(HeaderName::from_static(name), value);
	}
}

/// Every response carries the content policy, and the page runs cross-origin isolated (0354): COOP/COEP, and its own
/// files say they may be embedded only by this origin.
fn guarded(mut response: Response, port: u16, whiteboard: Option<&str>) -> Response {
	set_header(&mut response, "content-security-policy", &content_policy(port, whiteboard));
	set_header(&mut response, "cross-origin-opener-policy", "same-origin");
	set_header(&mut response, "cross-origin-embedder-policy", "require-corp");
	set_header(&mut response, "cross-origin-resource-policy", "same-origin");
	response
}

/// The animation stage's own headers (0615). The canvas plays an animation in a sandboxed frame, an opaque origin that
/// carries its own meta policy, so the page's policy must not apply to it (its inline boot script and `blob:` runtime
/// would be blocked, and `frame-ancestors 'none'` forbids the frame itself); under the page's COEP its resource policy
/// must be cross-origin for the frame to load it. Only the stage file gets this; everything else goes through `guarded`.
fn guarded_stage(mut response: Response) -> Response {
	set_header(&mut response, "cross-origin-opener-policy", "same-origin");
	set_header(&mut response, "cross-origin-embedder-policy", "require-corp");
	set_header(&mut response, "cross-origin-resource-policy", "cross-origin");
	response
}

/// A page response with its headers (0615): the stage file `guarded_stage`'s, every other response `guarded`'s.
pub fn page_guarded(served: Response, pathname: &str, port: u16, whiteboard: Option<&str>) -> Response {
	if served.status().is_success() && is_stage_file(pathname) {
		guarded_stage(served)
	} else {
		guarded(served, port, whiteboard)
	}
}

/// What a failed start undoes, each registered as its resource starts, run newest first (SRV-A-2).
type Undo = Vec<StopStep>;

/// The server's two tokens: the API's and the browser's own (SEC-NET-4).
struct Tokens {
	api: String,
	page: String,
}

/// What asks his questions.
struct Questions {
	popups: Arc<Popups>,
	question: Ask,
}

/// The Jocasta service's server (4.1, 0392, 0552): see the module's text.
pub async fn start_server(mut options: ServerOptions) -> Result<JocastaServer> {
	// The calendar's secret leaves this process's environment before anything is spawned (SEC-5, 0430).
	take_calendar_env();
	let tokens = Tokens { api: random_hex(), page: random_hex() };
	// The collab relay every omp pane hosts its session in (0536, 0561), first: a port another program holds stops the
	// start before herdr or any pane starts. It stops with Jocasta, and with a failed start.
	let relay = Arc::new(start_relay(options.relay_port)?);
	let mut undo: Undo = vec![StopStep::new("the collab relay", relay_stop(&relay))];
	match start_rest(&mut options, relay, tokens, &mut undo).await {
		Ok(server) => Ok(server),
		Err(error) => {
			undo_start(&undo).await;
			Err(error)
		}
	}
}

fn relay_stop(relay: &Arc<Relay>) -> StopRun {
	let relay = Arc::clone(relay);
	Arc::new(move || {
		let relay = Arc::clone(&relay);
		Box::pin(async move {
			relay.stop().await;
			Ok(())
		})
	})
}

/// The questions' asker: what shows a question is the test's `on_question`, else herdr's client shell in front.
fn questions(options: &mut ServerOptions, shell: Option<&Arc<Native>>) -> Questions {
	let popups = Arc::new(Popups::new());
	let show: Option<Show> = match (options.on_question.take(), shell) {
		(Some(on_question), _) => Some(Arc::new(move |popup: &Popup| {
			on_question(popup);
			Ok(())
		})),
		(None, Some(shell)) => {
			let shell = Arc::clone(shell);
			Some(Arc::new(move |popup: &Popup| {
				shell.show(popup);
				Ok(())
			}))
		}
		(None, None) => None,
	};
	let question = asker(Arc::clone(&popups), show);
	Questions { popups, question }
}

/// The Agents view and the dock's workspace, healed on every refresh: the service's `plugin` hook.
fn heal_of(herdr: &Arc<dyn Herdr>, shell: Arc<Native>) -> Heal {
	let herdr = Arc::clone(herdr);
	Arc::new(move |snapshot, boss| {
		let (herdr, shell) = (Arc::clone(&herdr), Arc::clone(&shell));
		Box::pin(async move {
			shell.heal(&*herdr, &snapshot, boss.as_deref()).await;
			Ok(())
		})
	})
}

/// The service over a new catalog, with the options that pass through and the server's own seams in place.
fn build_service(
	options: &mut ServerOptions,
	herdr: &Arc<dyn Herdr>,
	broadcast: &Broadcast,
	whiteboard: Option<&Arc<Whiteboard>>,
	page_open: Arc<dyn Fn(&str) + Send + Sync>,
) -> Result<(Arc<JocastaService>, Questions)> {
	let shell = options.shell.clone();
	let asking = questions(options, shell.as_ref());
	let mut service = ServiceOptions::new(Arc::clone(herdr), Arc::clone(broadcast));
	let (find_review, open_review) = (options.find_review.take(), options.open_review.take());
	let confirm = options.confirm.take();
	options.pass_on(&mut service);
	service.plugin = shell.map(|shell| heal_of(herdr, shell));
	service.confirm = Some(confirm.unwrap_or_else(|| allow_box(Arc::clone(&asking.question))));
	let row = whiteboard.cloned();
	service.services =
		Some(Arc::new(move || -> BoxFuture<'static, Vec<ServiceHealth>> { services_row(row.clone()).boxed() }));
	// The boss's `show` on this server's canvas page (0552), unless the test's.
	if let Some(whiteboard) = whiteboard {
		let reviews = whiteboard_reviews(Arc::clone(whiteboard), page_open);
		service.find_review = Some(reviews.find_review);
		service.open_review = Some(reviews.open_review);
	}
	service.find_review = find_review.or(service.find_review);
	service.open_review = open_review.or(service.open_review);
	let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog()?));
	Ok((JocastaService::new(catalog, service)?, asking))
}

/// What the stopper, the close and the routes are built from.
struct Rig<'a> {
	tokens: &'a Tokens,
	herdr: &'a Arc<dyn Herdr>,
	session: &'a str,
	service: &'a Arc<JocastaService>,
	asking: &'a Questions,
	whiteboard: Option<&'a Arc<Whiteboard>>,
	broadcast: &'a Broadcast,
}

/// The stopper: what stops, in what order, with the listening server read when it stops.
fn stopper_of(
	rig: &Rig<'_>,
	relay: StopRun,
	listening: &Arc<Mutex<Option<ServerHandle>>>,
	bounds: StopBounds,
) -> Arc<Stopper> {
	let popups = Arc::clone(&rig.asking.popups);
	let (herdr, session) = (Arc::clone(rig.herdr), rig.session.to_owned());
	let (service, whiteboard) = (Arc::clone(rig.service), rig.whiteboard.cloned());
	let (broadcast, listening) = (Arc::clone(rig.broadcast), Arc::clone(listening));
	Arc::new(stopper(move |extra_stops| {
		let slot = Arc::clone(&listening);
		let server: StopRun = Arc::new(move || {
			let handle = locked(&slot).clone();
			Box::pin(async move {
				if let Some(handle) = handle {
					handle.stop(true).await;
				}
				Ok(())
			})
		});
		stop_steps(StopParts {
			popups: Arc::clone(&popups),
			broadcast: Arc::clone(&broadcast),
			herdr: Arc::clone(&herdr),
			session: session.clone(),
			service: Arc::clone(&service) as Arc<dyn StopService>,
			extra_stops,
			whiteboard: whiteboard.clone(),
			relay: Arc::clone(&relay),
			server,
			bounds,
		})
	}))
}

/// His close: asks first with threads at work, then stops everything.
fn close_of(rig: &Rig<'_>, stops: &Arc<Stopper>, reattach: Option<Reattach>) -> CloseFlow {
	let (stopping, stop_all) = (Arc::clone(stops), Arc::clone(stops));
	close_flow(CloseParts {
		service: Arc::clone(rig.service) as Arc<dyn CloseService>,
		question: Arc::clone(&rig.asking.question),
		stopping: Arc::new(move || {
			let stops = Arc::clone(&stopping);
			stops.stopping().then(|| async move { stops.stopped().await }.boxed())
		}),
		stop_all: Arc::new(move || {
			let stops = Arc::clone(&stop_all);
			async move { stops.stop(true).await }.boxed()
		}),
		reattach,
	})
}

/// The API's routes in order, before the pages.
fn routes_of(rig: &Rig<'_>, close: &CloseFlow) -> Vec<Box<dyn Route>> {
	let closed = close.clone();
	door_routes(DoorParts {
		token: rig.tokens.api.clone(),
		service: Arc::clone(rig.service) as Arc<dyn DoorService>,
		client_closed: Arc::new(move || {
			let close = closed.clone();
			tokio::spawn(async move { close.close(CloseHow { ask: true, reattach: true }).await });
		}),
	})
}

/// His thread keys, from the dock or his keybinds.
fn keys_of(rig: &Rig<'_>, close: &CloseFlow, waiting_ms: i64, shell: Option<Arc<Native>>) -> Arc<ThreadKeys> {
	let quitting = close.clone();
	ThreadKeys::new(KeyParts {
		service: Arc::clone(rig.service) as Arc<dyn ActionService>,
		ask: Arc::clone(&rig.asking.question),
		front: Box::new(move |popup| {
			if let Some(shell) = &shell {
				shell.front(popup);
			}
		}),
		broadcast: Arc::clone(rig.broadcast),
		quit: Arc::new(move || {
			let close = quitting.clone();
			async move { close.close(CloseHow { ask: true, reattach: false }).await }.boxed()
		}),
		waiting_ms,
	})
}

/// The undo step that closes herdr's connection.
fn undo_herdr(herdr: &Arc<dyn Herdr>) -> StopStep {
	let herdr = Arc::clone(herdr);
	StopStep::sync("herdr's connection", move || {
		herdr.close();
		Ok(())
	})
}

/// whiteboard's server, the service's child (0393): it starts beside the service and stops with Jocasta.
fn start_child_whiteboard(options: Option<Option<WhiteboardOptions>>, undo: &mut Undo) -> Option<Arc<Whiteboard>> {
	let given = match options {
		None => WhiteboardOptions::default(),
		Some(None) => return None,
		Some(Some(given)) => given,
	};
	let whiteboard = Arc::new(start_whiteboard(given));
	let stopping = Arc::clone(&whiteboard);
	undo.push(StopStep::new(
		"whiteboard",
		Arc::new(move || {
			let whiteboard = Arc::clone(&stopping);
			Box::pin(async move {
				whiteboard.stop().await;
				Ok(())
			})
		}),
	));
	Some(whiteboard)
}

/// Opens a page of this server in his browser (the boss's `show`, 0552).
fn page_opener(port: &Arc<AtomicU16>, codes: &Arc<LaunchCodes>) -> Arc<dyn Fn(&str) + Send + Sync> {
	let (port, codes) = (Arc::clone(port), Arc::clone(codes));
	Arc::new(move |query| {
		if let Err(error) = open_in_browser(&page_url(port.load(Ordering::SeqCst), &codes, query)) {
			log(LogLevel::Warn, "server", &format!("couldn't open the page: {error}"), None);
		}
	})
}

/// What listens: the API's routes, then every page with the content policy on it.
fn served(routes: Vec<Box<dyn Route>>, pages: PageResponder, whiteboard: Option<Arc<Whiteboard>>) -> Served {
	Served {
		routes,
		page: Box::new(move |request, url, at| {
			let (pages, whiteboard) = (Arc::clone(&pages), whiteboard.clone());
			Box::pin(async move {
				let pathname = url.path().to_owned();
				let served = pages(request, url).await;
				page_guarded(served, &pathname, at, whiteboard.map(|whiteboard| whiteboard.origin()).as_deref())
			})
		}),
	}
}

/// The undo step that stops the service.
fn undo_service(service: &Arc<JocastaService>) -> StopStep {
	let service = Arc::clone(service);
	StopStep::new(
		"the service",
		Arc::new(move || {
			let service = Arc::clone(&service);
			Box::pin(async move {
				service.stop().await;
				Ok(())
			})
		}),
	)
	.bounded(SERVICE_UNDO_MS)
}

/// The undo step that stops the listening server.
fn undo_server(handle: &ServerHandle) -> StopStep {
	let handle = handle.clone();
	StopStep::new(
		"the server",
		Arc::new(move || {
			let handle = handle.clone();
			Box::pin(async move {
				handle.stop(true).await;
				Ok(())
			})
		}),
	)
}

/// The page responder: the bearer's `/health`, and the browser's pages.
fn pages_of(
	tokens: &Tokens,
	parts: (&Arc<LaunchCodes>, &Arc<AtomicU16>),
	whiteboard: Option<Arc<Whiteboard>>,
	health: Box<dyn Fn() -> bool + Send + Sync>,
) -> PageResponder {
	let (codes, port) = (Arc::clone(parts.0), Arc::clone(parts.1));
	page_responder(PageParts {
		token: tokens.api.clone(),
		page_token: tokens.page.clone(),
		codes,
		port: Arc::new(move || port.load(Ordering::SeqCst)),
		whiteboard,
		bearer_routes: vec![Box::new(health_route(&tokens.api, health))],
	})
}

async fn start_rest(
	options: &mut ServerOptions,
	relay: Arc<Relay>,
	tokens: Tokens,
	undo: &mut Undo,
) -> Result<JocastaServer> {
	let listening: Arc<Mutex<Option<ServerHandle>>> = Arc::new(Mutex::new(None));
	let slot = Arc::clone(&listening);
	let shell = options.shell.clone();
	let broadcast: Broadcast = Arc::new(move |message| {
		if let Some(shell) = &shell {
			shell.feed(&message);
		}
		publish(locked(&slot).as_ref(), &message);
	});
	let session = match options.herdr_session.take() {
		Some(session) => session,
		None => herdr_session()?,
	};
	let herdr = match options.herdr.clone() {
		Some(herdr) => herdr,
		None => connect_session(&session, options.herdr_socket.take()).await?,
	};
	undo.push(undo_herdr(&herdr));
	let whiteboard = start_child_whiteboard(options.whiteboard.take(), undo);
	let port = Arc::new(AtomicU16::new(0));
	let codes = Arc::new(LaunchCodes::default());
	let waiting_ms = options.confirm_wait_ms.unwrap_or(CONFIRM_WAIT_MS);
	let opener = page_opener(&port, &codes);
	// The panes' overlays name the relay as it listens (a test's `Some(0)` asks for a free port).
	options.relay_port = Some(relay.port);
	let (service, asking) = build_service(options, &herdr, &broadcast, whiteboard.as_ref(), opener)?;
	// Its stop closes what it opened (the catalog, the day's stores) and herdr's connection.
	undo.push(undo_service(&service));
	let rig = Rig {
		tokens: &tokens,
		herdr: &herdr,
		session: &session,
		service: &service,
		asking: &asking,
		whiteboard: whiteboard.as_ref(),
		broadcast: &broadcast,
	};
	let stops = stopper_of(&rig, relay_stop(&relay), &listening, options.stop_bounds_ms);
	let close = close_of(&rig, &stops, options.reattach.take());
	let mut routes = routes_of(&rig, &close);
	let closing = Arc::clone(&stops);
	let pages = pages_of(&tokens, (&codes, &port), whiteboard.clone(), Box::new(move || closing.stopping()));
	let guard: Guard = Arc::new(|response, port| guarded(response, port, None));
	let feed = feed_socket(Arc::clone(&service) as Arc<dyn FeedService>);
	routes.push(Box::new(feed_route(&tokens.api, guard, feed)));
	let handle = listen(options.port.unwrap_or_else(configured_port), served(routes, pages, whiteboard.clone()))?;
	*locked(&listening) = Some(handle.clone());
	undo.push(undo_server(&handle));
	port.store(handle.port(), Ordering::SeqCst);
	undo.push(StopStep::sync("jocasta-service.json", || {
		remove_service_file(std::process::id());
		Ok(())
	}));
	write_service_file(&ServiceFile { port: handle.port(), token: tokens.api.clone(), pid: std::process::id() }, None)?;
	// The service starts once the server listens: its start (git per pane folder, the first read of every session
	// today) takes seconds on a busy day, and bare `jocasta` must not wait for it.
	let starting = service.start();
	tokio::spawn(async move {
		if let Err(error) = starting.await {
			log(LogLevel::Error, "service", &format!("didn't start: {error}"), None);
		}
	});
	let keys = keys_of(&rig, &close, waiting_ms, options.shell.clone());
	Ok(JocastaServer {
		port: handle.port(),
		token: tokens.api,
		relay_port: relay.port,
		service,
		whiteboard,
		inner: Arc::new(Inner { codes, stops, close, question: asking.question, popups: asking.popups, keys, herdr }),
	})
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod shutdown_tests;

#[cfg(test)]
#[path = "server_popups_tests.rs"]
mod server_popups_tests;

#[cfg(test)]
#[path = "server_whiteboard_tests.rs"]
mod server_whiteboard_tests;
