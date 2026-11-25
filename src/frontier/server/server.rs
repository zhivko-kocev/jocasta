//! The Jocasta service's server (the Rust port of `server.ts`; 4.1, 0392, 0552): 127.0.0.1 on `service.port` (0358), or
//! a free port for this run when that one is busy; `<data>/jocasta-service.json` with `port`, `token` and `pid`. Every
//! API call carries the token as a bearer (`jocasta mcp`, bare `jocasta`, the benchmark's clients); the browser's pages carry
//! a separate page token as a per-port cookie, set from a one-time launch code (`/?t=<code>`, SEC-NET-4, SEC-LOC-11). It
//! serves the bridge's pipe, the state feed, his questions and the briefs (0678), and runs the herdr-backed
//! service. A start that fails stops what it had started, its bridge included (SRV-A-2).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::http::{HeaderName, HeaderValue};
use axum::response::Response;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::frontier::brief::BriefLinks;
use crate::frontier::brief::routes::{BriefActions, BriefRoutes, CONTENT_POLICY};
use crate::frontier::protocol::{ClientMessage, PluginAction, PopupView, ServerMessage, TOKEN_QUERY};
use crate::frontier::server::allow::{Answer, CONFIRM_WAIT_MS, Confirm, Confirmation};
use crate::frontier::server::asker::{Ask, Show, asker};
use crate::frontier::server::bridge::{Bridge, BridgeService, Secrets};
use crate::frontier::server::close::{CloseFlow, CloseHow, CloseOutcome, CloseParts, CloseService, close_flow};
use crate::frontier::server::day::{DayOptions, SharedCatalog};
use crate::frontier::server::feed::{FeedService, Guard, Reply, feed_route, feed_socket, publish};
use crate::frontier::server::health_route::health_route;
use crate::frontier::server::native::{Native, NativeServer};
use crate::frontier::server::page_routes::{LaunchCodes, PageParts, PageResponder, page_responder};
use crate::frontier::server::phone::{Phone, PhoneOptions, Timing, topics};
use crate::frontier::server::popups::{ALLOW, AnswerTaken, Popup, Popups, allow_question};
use crate::frontier::server::routes::{Route, Served, ServerHandle, listen};
use crate::frontier::server::service::{Changed, Heal, JocastaService, ServiceOptions};
use crate::frontier::server::stop_sequence::{
	StopBounds, StopParts, StopRun, StopService, StopStep, StopWhen, Stopper, stop_steps, stopper, undo_start,
};
use crate::frontier::server::surface::{Note, Surface};
use crate::frontier::server::thread_keys::{ActionService, KeyParts, ThreadKeys};
use crate::shared::credentials::{calendar_secret, gmail_secret, jira_secret, take_calendar_env, take_source_envs};
use crate::shared::errors::Result;
use crate::shared::link::open_in_browser;
use crate::shared::log::{LogLevel, log};
use crate::shared::paths::data_dir;
use crate::shared::process::AbortSignal;
use crate::shared::schema::open_catalog;
use crate::shared::service_file::{ServiceFile, remove_service_file, write_service_file};
use crate::shared::settings::{ServiceKey, read_ntfy, read_settings, service_setting};

/// `service.port`'s default (0358).
const DEFAULT_PORT: u16 = 47_400;

/// How long the service's own stop may take on a failed start (the TypeScript's `ms: 30_000`).
const SERVICE_UNDO_MS: u64 = 30_000;

/// How long the phone check-in's stop may take.
const PHONE_STOP_MS: u64 = 5_000;

/// What sends a message to every state-feed client.
type Broadcast = Arc<dyn Fn(ServerMessage) + Send + Sync>;

/// What shows a question to him: it gets the question's one-time code.
pub type OnQuestion = Arc<dyn Fn(&Popup) + Send + Sync>;

/// What the server was started with: everything the TypeScript's `ServerOptions` names. The options that pass through
/// to the service are `ServiceOptions`' of the same name; the rest are the server's own. An unset option takes its
/// default.
#[derive(Default)]
pub struct ServerOptions {
	/// How omp starts: `["omp"]` by default.
	pub omp: Option<Vec<String>>,
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
	/// mise's argv; `Some(None)` runs none.
	pub mise: Option<Option<Vec<String>>>,
	/// How long the weekly big-picture review may run (0470).
	pub weekly_review_ms: Option<i64>,
	/// How long the project's `scorecard` task may run (0616).
	pub scorecard_ms: Option<i64>,
	/// `Some(false)`: the minute tick never starts a weekly review (0470).
	pub weekly_reviews: Option<bool>,
	/// `Some(false)`: the tick builds no morning or weekly brief by itself (0680).
	pub auto_briefs: Option<bool>,
	/// Asks him Allow or Deny (0484); his question through this server's question API by default.
	pub confirm: Option<Confirm>,
	/// How long an ask waits for his answer.
	pub confirm_wait_ms: Option<i64>,
	/// The boss pane's key (0484).
	pub boss_key: Option<String>,
	/// The bridge (0675): the one the server makes by default; a recording one in a test (test seam).
	pub bridge: Option<Bridge>,
	/// How long the end of a thread pane's omp is waited for before its leftovers end (0490).
	pub leftover_wait_ms: Option<i64>,
	/// When leftovers are looked for again (0490).
	pub leftover_again_ms: Option<i64>,
	/// `Some(false)`: a task end without a `report_result` goes straight to its review (0539).
	pub ask_report: Option<bool>,
	/// How long a plan-mode turn's end waits for the session to show its proposal; `PLAN_SETTLE_MS` (test seam).
	pub plan_settle_ms: Option<i64>,
	/// herdr (test seam): the server this service lives in by default.
	pub herdr: Option<Arc<dyn Surface>>,
	/// Phase 8's pollers and clock (test seam); tests pass `pollers: Some(false)` so nothing calls the real gh or az.
	pub day: Option<DayOptions>,
	/// The port to try first (test seam): `service.port`, else 47400 (0358).
	pub port: Option<u16>,
	/// How a brief reaches his browser (test seam): a page of this server opened with a fresh launch code by default.
	pub briefs: Option<Arc<dyn BriefLinks>>,
	/// Shows a question to him (test seam): it gets the question's one-time code, and the shell shows none.
	pub on_question: Option<OnQuestion>,
	/// herdr's client shells (fork plan phase 5): the dock beside the boss, the Agents view, his questions and his
	/// thread keys. The service herdr's server runs has them; a test's server has none unless set.
	pub shell: Option<Arc<Native>>,
	/// Each stop step's bound in ms (test seam): service 30 s, herdr 15 s.
	pub stop_bounds_ms: StopBounds,
}

impl ServerOptions {
	/// The options that pass through to the service: all but the server's own.
	fn pass_on(&mut self, service: &mut ServiceOptions) {
		service.omp = self.omp.take();
		service.boss_command = self.boss_command.take();
		service.pane_prompt = self.pane_prompt.take();
		service.gh = self.gh.take();
		service.gh_timeout_ms = self.gh_timeout_ms;
		service.plan_settle_ms = self.plan_settle_ms;
		service.sessions_dir = self.sessions_dir.take();
		service.agent_wait_ms = self.agent_wait_ms;
		service.pane_check_wait_ms = self.pane_check_wait_ms;
		service.refresh_ms = self.refresh_ms;
		service.mise = self.mise.take();
		service.weekly_review_ms = self.weekly_review_ms;
		service.scorecard_ms = self.scorecard_ms;
		service.weekly_reviews = self.weekly_reviews;
		service.auto_briefs = self.auto_briefs;
		service.confirm_wait_ms = self.confirm_wait_ms;
		service.boss_key = self.boss_key.take();
		service.bridge = self.bridge.clone();
		service.leftover_wait_ms = self.leftover_wait_ms;
		service.leftover_again_ms = self.leftover_again_ms;
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
	/// The bridge (0675): where the panes' omps and the CLI connect.
	pub bridge: Bridge,
	pub service: Arc<JocastaService>,
	inner: Arc<Inner>,
}

struct Inner {
	codes: Arc<LaunchCodes>,
	stops: Arc<Stopper>,
	close: CloseFlow,
	question: Ask,
	popups: Arc<Popups>,
	keys: Arc<ThreadKeys>,
	herdr: Arc<dyn Surface>,
}

impl JocastaServer {
	/// A page of this server by its path, `http://127.0.0.1:<port><path>?t=<code>`, with a fresh one-time launch code
	/// each time it is read (SEC-LOC-11): used once within a minute, it becomes the page cookie and shows the page.
	pub fn page_url(&self, path: &str) -> String {
		page_url(self.port, &self.inner.codes, path)
	}

	/// The service closes and `jocasta-service.json` goes away; herdr and every pane keep running (a failed start).
	pub async fn stop(&self) {
		self.inner.stops.stop(false).await;
	}

	/// Closes Jocasta as one (0434, 0549): the quit action's close. With `ask` and any thread working or awaiting him,
	/// the close question is asked first (Stop everything or Cancel); Cancel leaves everything as it was. Otherwise
	/// everything Jocasta started stops (`stop_everything`) and `stopped` resolves.
	pub async fn shutdown(&self, ask: bool) -> CloseOutcome {
		self.inner.close.close(CloseHow { ask }).await
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

/// A page URL with a fresh launch code (`http://127.0.0.1:<port><path>?t=<code>`).
fn page_url(port: u16, codes: &LaunchCodes, path: &str) -> String {
	format!("http://127.0.0.1:{port}{path}?{TOKEN_QUERY}={}", codes.mint())
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

fn set_header(response: &mut Response, name: &'static str, value: &str) {
	if let Ok(value) = HeaderValue::from_str(value) {
		response.headers_mut().insert(HeaderName::from_static(name), value);
	}
}

/// Every page response carries the briefs' content policy (0678): scripts and styles come only from this server and
/// nothing is framed or posted elsewhere.
pub fn guarded(mut response: Response) -> Response {
	set_header(&mut response, "content-security-policy", CONTENT_POLICY);
	set_header(&mut response, "x-content-type-options", "nosniff");
	set_header(&mut response, "referrer-policy", "no-referrer");
	response
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
	take_source_envs();
	let tokens = Tokens { api: random_hex(), page: random_hex() };
	let bridge = options.bridge.get_or_insert_with(Bridge::new).clone();
	bridge.set_secrets(secrets_of(&tokens));
	let mut undo: Undo = vec![StopStep::new("the bridge", bridge_stop(&bridge))];
	match start_rest(&mut options, bridge, tokens, &mut undo).await {
		Ok(server) => Ok(server),
		Err(error) => {
			undo_start(&undo).await;
			Err(error)
		}
	}
}

/// What the bridge blanks from everything a model reads (0681): the calendar URL, the Gmail app password and the Jira
/// token (0680), both tokens, the ntfy server and token and the phone's two topics.
fn secrets_of(tokens: &Tokens) -> Secrets {
	let ntfy = read_ntfy();
	// The phone's two topics are its passwords on a public server; the same ones the phone starts with.
	let topics = ntfy.server.is_some().then(|| topics(&data_dir()));
	Secrets::new(
		[
			calendar_secret(),
			gmail_secret(),
			jira_secret(),
			Some(tokens.api.clone()),
			Some(tokens.page.clone()),
			ntfy.server,
			ntfy.token,
			topics.as_ref().map(|topics| topics.main.clone()),
			topics.map(|topics| topics.reply),
		]
		.into_iter()
		.flatten(),
	)
}

fn bridge_stop(bridge: &Bridge) -> StopRun {
	let bridge = bridge.clone();
	Arc::new(move || {
		let bridge = bridge.clone();
		Box::pin(async move {
			bridge.stop();
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

/// The boss's workspace, told to the shell on every refresh: the service's `dock` hook.
fn heal_of(shell: Arc<Native>) -> Heal {
	Arc::new(move |snapshot, boss| {
		let shell = Arc::clone(&shell);
		Box::pin(async move {
			shell.heal(&snapshot, boss.as_deref()).await;
			Ok(())
		})
	})
}

/// herdr's changes, as the herdr server this service runs in reports them: the service's `herdr_changed` hook.
fn changes_of(shell: Arc<Native>) -> Changed {
	Arc::new(move || shell.herdr_changed())
}

/// The service over a new catalog, with the options that pass through and the server's own seams in place.
fn build_service(
	options: &mut ServerOptions,
	herdr: &Arc<dyn Surface>,
	broadcast: &Broadcast,
	briefs: Arc<dyn BriefLinks>,
) -> Result<(Arc<JocastaService>, Questions)> {
	let shell = options.shell.clone();
	let asking = questions(options, shell.as_ref());
	let mut service = ServiceOptions::new(Arc::clone(herdr), Arc::clone(broadcast));
	let given = options.briefs.take();
	let confirm = options.confirm.take();
	options.pass_on(&mut service);
	service.dock = shell.clone().map(heal_of);
	service.herdr_changed = shell.map(changes_of);
	service.confirm = Some(confirm.unwrap_or_else(|| allow_box(Arc::clone(&asking.question))));
	service.ask = Some(Arc::clone(&asking.question));
	service.briefs = Some(given.unwrap_or(briefs));
	let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog()?));
	Ok((JocastaService::new(catalog, service)?, asking))
}

/// What the stopper, the close and the routes are built from.
struct Rig<'a> {
	herdr: &'a Arc<dyn Surface>,
	service: &'a Arc<JocastaService>,
	asking: &'a Questions,
	broadcast: &'a Broadcast,
	phone: Option<&'a Arc<Phone>>,
}

/// The phone check-in (0672), when `ntfy.server` is set: it pushes what the service's `phone_facts` says.
fn start_phone(service: &Arc<JocastaService>, popups: &Arc<Popups>) -> Option<Arc<Phone>> {
	let ntfy = read_ntfy();
	let facts = Arc::clone(service);
	Phone::start(PhoneOptions {
		ntfy,
		popups: Arc::clone(popups),
		facts: Arc::new(move || facts.phone_facts()),
		data_dir: data_dir(),
		timing: Timing::default(),
	})
}

/// The step that stops the phone check-in.
fn phone_stop(phone: &Arc<Phone>) -> StopStep {
	let phone = Arc::clone(phone);
	StopStep::new(
		"the phone check-in",
		Arc::new(move || {
			let phone = Arc::clone(&phone);
			Box::pin(async move {
				phone.stop().await;
				Ok(())
			})
		}),
	)
	.bounded(PHONE_STOP_MS)
}

/// The stopper: what stops, in what order, with the listening server read when it stops.
fn stopper_of(
	rig: &Rig<'_>,
	bridge: StopRun,
	listening: &Arc<Mutex<Option<ServerHandle>>>,
	bounds: StopBounds,
) -> Arc<Stopper> {
	let popups = Arc::clone(&rig.asking.popups);
	let herdr = Arc::clone(rig.herdr);
	let service = Arc::clone(rig.service);
	let (broadcast, listening) = (Arc::clone(rig.broadcast), Arc::clone(listening));
	let phone = rig.phone.cloned();
	Arc::new(stopper(move |mut extra_stops| {
		// The phone check-in stops first of what else Jocasta started (0672).
		if let Some(phone) = &phone {
			extra_stops.insert(0, phone_stop(phone));
		}
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
			service: Arc::clone(&service) as Arc<dyn StopService>,
			extra_stops,
			bridge: Arc::clone(&bridge),
			server,
			bounds,
		})
	}))
}

/// His close: asks first with threads at work, then stops everything.
fn close_of(rig: &Rig<'_>, stops: &Arc<Stopper>) -> CloseFlow {
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
			async move { close.close(CloseHow { ask: true }).await }.boxed()
		}),
		waiting_ms,
	})
}

/// The undo step that closes the surface's watches.
fn undo_herdr(herdr: &Arc<dyn Surface>) -> StopStep {
	let herdr = Arc::clone(herdr);
	StopStep::sync("herdr's connection", move || {
		herdr.close();
		Ok(())
	})
}

/// How a brief reaches his browser (0678): its page on this server, opened with a fresh launch code.
struct ServerBriefs {
	port: Arc<AtomicU16>,
	codes: Arc<LaunchCodes>,
}

impl BriefLinks for ServerBriefs {
	fn url(&self, id: &str) -> String {
		format!("http://127.0.0.1:{}/brief/{id}", self.port.load(Ordering::SeqCst))
	}

	fn open(&self, id: &str) {
		let url = page_url(self.port.load(Ordering::SeqCst), &self.codes, &format!("/brief/{id}"));
		if let Err(error) = open_in_browser(&url) {
			log(LogLevel::Warn, "server", &format!("couldn't open the brief: {error}"), None);
		}
	}
}

/// What listens: the API's routes, then every page with the content policy on it.
fn served(routes: Vec<Box<dyn Route>>, pages: PageResponder) -> Served {
	Served {
		routes,
		page: Box::new(move |request, url, _at| {
			let pages = Arc::clone(&pages);
			Box::pin(async move { guarded(pages(request, url).await) })
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
	briefs: BriefRoutes,
	health: Box<dyn Fn() -> bool + Send + Sync>,
) -> PageResponder {
	let (codes, port) = (Arc::clone(parts.0), Arc::clone(parts.1));
	page_responder(PageParts {
		token: tokens.api.clone(),
		page_token: tokens.page.clone(),
		codes,
		port: Arc::new(move || port.load(Ordering::SeqCst)),
		briefs,
		bearer_routes: vec![Box::new(health_route(&tokens.api, health))],
	})
}

async fn start_rest(
	options: &mut ServerOptions,
	bridge: Bridge,
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
	let Some(herdr) = options.herdr.clone() else {
		return Err(crate::shared::errors::Error::other("the server was started without a surface"));
	};
	undo.push(undo_herdr(&herdr));
	let briefs_of_server =
		Arc::new(ServerBriefs { port: Arc::new(AtomicU16::new(0)), codes: Arc::new(LaunchCodes::default()) });
	let (port, codes) = (Arc::clone(&briefs_of_server.port), Arc::clone(&briefs_of_server.codes));
	let waiting_ms = options.confirm_wait_ms.unwrap_or(CONFIRM_WAIT_MS);
	let (service, asking) = build_service(options, &herdr, &broadcast, briefs_of_server)?;
	// Its stop closes what it opened (the catalog, the day's stores) and herdr's connection.
	undo.push(undo_service(&service));
	let phone = start_phone(&service, &asking.popups);
	if let Some(phone) = &phone {
		undo.push(phone_stop(phone));
	}
	let rig = Rig { herdr: &herdr, service: &service, asking: &asking, broadcast: &broadcast, phone: phone.as_ref() };
	let stops = stopper_of(&rig, bridge_stop(&bridge), &listening, options.stop_bounds_ms);
	let close = close_of(&rig, &stops);
	let mut routes: Vec<Box<dyn Route>> = Vec::new();
	let closing = Arc::clone(&stops);
	let actions = Arc::clone(&service) as Arc<dyn BriefActions>;
	let brief_routes = BriefRoutes::new(
		Arc::new({
			let port = Arc::clone(&port);
			move || port.load(Ordering::SeqCst)
		}),
		actions,
	);
	let pages = pages_of(&tokens, (&codes, &port), brief_routes, Box::new(move || closing.stopping()));
	let guard: Guard = Arc::new(|response, _port| guarded(response));
	let feed = feed_socket(Arc::clone(&service) as Arc<dyn FeedService>);
	routes.push(Box::new(feed_route(&tokens.api, guard, feed)));
	let handle = listen(options.port.unwrap_or_else(configured_port), served(routes, pages))?;
	*locked(&listening) = Some(handle.clone());
	undo.push(undo_server(&handle));
	port.store(handle.port(), Ordering::SeqCst);
	// The panes' omps connect to the bridge on the port the server got (0675); it listens before the service starts the
	// boss, whose environment names it.
	bridge.listen(handle.port(), &tokens.api, &(Arc::clone(&service) as Arc<dyn BridgeService>))?;
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
		bridge,
		service,
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
