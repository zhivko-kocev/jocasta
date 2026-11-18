use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use tokio::sync::{Notify, oneshot};

use super::*;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// One attach: herdr's argv and the environment it got.
type Attach = (Vec<String>, HashMap<String, String>);
type CloseHandler = Box<dyn Fn() + Send + Sync>;
type Local<T> = Pin<Box<dyn Future<Output = T>>>;

/// What the fake's surface did, shared with the hooks a test sets.
#[derive(Default)]
struct Calls {
	started: Cell<u32>,
	attached: RefCell<Vec<Attach>>,
	/// What closing Jocasta did, in order (0434).
	closed: RefCell<Vec<String>>,
	printed: RefCell<Vec<String>>,
}

impl Calls {
	fn printed(&self) -> String {
		self.printed.borrow().join("\n")
	}

	fn closed(&self) -> Vec<String> {
		self.closed.borrow().clone()
	}
}

fn up() -> ServiceHealth {
	health(ServiceState::Up)
}

fn health(state: ServiceState) -> ServiceHealth {
	ServiceHealth { state, herdr_revivals: None, layer: None, hosts: None }
}

/// The deps of bare `jocasta`: every part is a hook a test replaces.
struct Fake {
	calls: Rc<Calls>,
	health: Box<dyn Fn() -> ServiceHealth>,
	/// `Some(code)`: the started service exits with it at once; `None`: it keeps running.
	start: Box<dyn Fn() -> Option<i32>>,
	last_error: Box<dyn Fn() -> Option<String>>,
	herdr: Option<PathBuf>,
	config: PathBuf,
	attach: Box<dyn Fn() -> Local<Result<i32>>>,
	wait_ms: Option<i64>,
	sleep: Box<dyn Fn(u64)>,
	enter: Box<dyn Fn() -> Box<dyn Fn() -> bool>>,
	on_close: Rc<RefCell<Option<CloseHandler>>>,
	tell: Box<dyn Fn() -> bool>,
	stop: Box<dyn Fn() -> Local<Result<()>>>,
	exit: Box<dyn Fn()>,
	watch: Box<dyn Fn() -> StopCheck>,
	herdr_up: Box<dyn Fn() -> bool>,
}

fn fake(config: PathBuf) -> Fake {
	let calls = Rc::new(Calls::default());
	let (tell_calls, stop_calls) = (Rc::clone(&calls), Rc::clone(&calls));
	Fake {
		calls,
		health: Box::new(up),
		start: Box::new(|| None),
		last_error: Box::new(|| None),
		herdr: Some(PathBuf::from("herdr.exe")),
		config,
		attach: Box::new(|| Box::pin(async { Ok(0) })),
		wait_ms: None,
		sleep: Box::new(|_| {}),
		enter: Box::new(|| Box::new(|| true)),
		on_close: Rc::default(),
		tell: Box::new(move || {
			tell_calls.closed.borrow_mut().push("tell service".to_owned());
			false
		}),
		stop: Box::new(move || {
			stop_calls.closed.borrow_mut().push("stop herdr".to_owned());
			Box::pin(async { Ok(()) })
		}),
		exit: Box::new(|| {}),
		watch: Box::new(|| Box::new(|| Box::pin(async { false }))),
		herdr_up: Box::new(|| true),
	}
}

impl SurfaceDeps for Fake {
	async fn service_health(&self) -> ServiceHealth {
		(self.health)()
	}
	async fn start_service(&self) -> i32 {
		self.calls.started.set(self.calls.started.get() + 1);
		match (self.start)() {
			Some(code) => code,
			None => std::future::pending().await,
		}
	}
	fn last_error(&self, _since: i64) -> Option<String> {
		(self.last_error)()
	}
	fn herdr(&self) -> Option<&Path> {
		self.herdr.as_deref()
	}
	fn herdr_config(&self) -> &Path {
		&self.config
	}
	async fn attach(&self, argv: &[String], env: &HashMap<String, String>) -> Result<i32> {
		self.calls.attached.borrow_mut().push((argv.to_vec(), env.clone()));
		(self.attach)().await
	}
	fn print(&self, line: &str) {
		self.calls.printed.borrow_mut().push(line.to_owned());
	}
	fn wait_ms(&self) -> Option<i64> {
		self.wait_ms
	}
	async fn sleep(&self, ms: u64) {
		(self.sleep)(ms);
	}
	fn enter_surface(&self) -> Box<dyn Fn() -> bool> {
		(self.enter)()
	}
	fn on_terminal_close(&self, handler: CloseHandler) -> Box<dyn FnOnce()> {
		*self.on_close.borrow_mut() = Some(handler);
		Box::new(|| {})
	}
	async fn tell_service_closed(&self) -> bool {
		(self.tell)()
	}
	async fn stop_herdr(&self) -> Result<()> {
		(self.stop)().await
	}
	fn exit(&self) {
		(self.exit)();
	}
	fn watch_service_stop(&self) -> StopCheck {
		(self.watch)()
	}
	async fn herdr_up(&self) -> bool {
		(self.herdr_up)()
	}
}

/// A throwaway session unset for the test (a live-check shell sets `JOCASTA_HERDR_SESSION`, 0441), and a surface.
fn setup() -> (TempJocasta, Fake) {
	let mut j = temp_jocasta();
	j.set_var("JOCASTA_HERDR_SESSION", None);
	let config = j.root().join("herdr.toml");
	(j, fake(config))
}

/// Another surface in the same test: only one `temp_jocasta` may be held at a time.
fn another(j: &TempJocasta) -> Fake {
	fake(j.root().join("herdr.toml"))
}

/// A client that always exits `code`.
fn always(f: &mut Fake, code: i32) {
	f.attach = Box::new(move || Box::pin(async move { Ok(code) }));
}

fn argv(list: &[&str]) -> Vec<String> {
	list.iter().map(|word| (*word).to_owned()).collect()
}

fn message(error: &Error) -> String {
	error.to_string()
}

#[tokio::test]
async fn bare_jocasta_starts_the_service_when_none_answers_then_attaches_heredrs_session_jocasta_0392_0552() {
	let (j, mut f) = setup();
	std::fs::write(j.root().join("herdr.toml"), "").unwrap();
	let state = Rc::new(Cell::new(ServiceState::Down));
	let (seen, starts) = (Rc::clone(&state), Rc::clone(&state));
	f.health = Box::new(move || health(seen.get()));
	f.start = Box::new(move || {
		starts.set(ServiceState::Up);
		None
	});
	assert_eq!(open_surface(&f).await.unwrap(), 0);
	assert_eq!(f.calls.started.get(), 1);
	let config = j.root().join("herdr.toml").to_string_lossy().into_owned();
	assert_eq!(
		*f.calls.attached.borrow(),
		[(argv(&["herdr.exe", "--session", "jocasta"]), HashMap::from([("HERDR_CONFIG_PATH".to_owned(), config)]))]
	);

	// Running already: no second service.
	let g = fake(j.root().join("herdr.toml"));
	open_surface(&g).await.unwrap();
	assert_eq!(g.calls.started.get(), 0);
}

#[tokio::test]
async fn a_throwaway_session_for_live_checks_is_attached_0441() {
	let (mut j, f) = setup();
	j.set_var("JOCASTA_HERDR_SESSION", Some("jocasta-polish"));
	open_surface(&f).await.unwrap();
	assert_eq!(f.calls.attached.borrow()[0].0, argv(&["herdr.exe", "--session", "jocasta-polish"]));
}

#[tokio::test]
async fn a_service_that_is_closing_is_waited_for_then_started_once_after_saying_so_lc_3() {
	let (_j, mut f) = setup();
	let states =
		Rc::new(RefCell::new(VecDeque::from([ServiceState::Closing, ServiceState::Closing, ServiceState::Down])));
	let order: Rc<RefCell<Vec<String>>> = Rc::default();
	let (queue, starts) = (Rc::clone(&states), Rc::clone(&order));
	f.health = Box::new(move || health(queue.borrow_mut().pop_front().unwrap_or(ServiceState::Up)));
	f.start = Box::new(move || {
		starts.borrow_mut().push("start".to_owned());
		None
	});
	open_surface(&f).await.unwrap();
	assert_eq!(f.calls.started.get(), 1);
	assert_eq!(f.calls.printed.borrow()[0], "jocasta: Jocasta is closing; it starts again once it has.");
	assert_eq!(*order.borrow(), ["start"]);
	assert_eq!(f.calls.attached.borrow().len(), 1);
}

#[tokio::test]
async fn a_hung_service_gets_a_new_one_started_which_takes_over_0534() {
	let (_j, mut f) = setup();
	let state = Rc::new(Cell::new(ServiceState::Busy));
	let (seen, starts) = (Rc::clone(&state), Rc::clone(&state));
	f.health = Box::new(move || health(seen.get()));
	f.start = Box::new(move || {
		starts.set(ServiceState::Up);
		None
	});
	open_surface(&f).await.unwrap();
	assert_eq!(f.calls.started.get(), 1);
	assert_eq!(f.calls.attached.borrow().len(), 1);
}

#[tokio::test]
async fn a_service_it_started_that_doesnt_answer_in_time_is_said_a_missing_herdr_toml_points_at_setup_no_herdr_refuses()
{
	let (j, mut f) = setup();
	f.health = Box::new(|| health(ServiceState::Down));
	f.wait_ms = Some(0);
	open_surface(&f).await.unwrap();
	assert_eq!(f.calls.started.get(), 1);
	assert!(f.calls.printed().contains("the service is still starting"));
	assert_eq!(f.calls.attached.borrow().len(), 1);
	assert!(f.calls.attached.borrow()[0].1.is_empty());
	assert!(f.calls.printed().contains("run `jocasta setup`"));
	let mut none = another(&j);
	none.herdr = None;
	assert!(message(&open_surface(&none).await.unwrap_err()).contains("herdr isn't installed"));
}

#[tokio::test]
async fn a_service_that_exits_before_it_answers_is_said_with_its_last_error_and_nothing_attaches_exit_1_0582() {
	let (j, mut f) = setup();
	let asked = Rc::new(Cell::new(0));
	let counter = Rc::clone(&asked);
	f.health = Box::new(move || {
		counter.set(counter.get() + 1);
		health(ServiceState::Down)
	});
	f.start = Box::new(|| Some(1));
	f.last_error = Box::new(|| Some("C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token".to_owned()));
	assert_eq!(open_surface(&f).await.unwrap(), 1);
	assert_eq!(
		*f.calls.printed.borrow(),
		[
			"jocasta: the service stopped while starting: C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token",
			"jocasta: run `jocasta diagnose` for the full log.",
		]
	);
	assert!(f.calls.attached.borrow().is_empty());
	assert!(asked.get() < 5);

	// One that left because another service answers: that one is attached to.
	let mut left = another(&j);
	let state = Rc::new(Cell::new(ServiceState::Down));
	let (seen, starts) = (Rc::clone(&state), Rc::clone(&state));
	left.health = Box::new(move || health(seen.get()));
	left.start = Box::new(move || {
		starts.set(ServiceState::Up);
		Some(0)
	});
	assert_eq!(open_surface(&left).await.unwrap(), 0);
	assert_eq!(left.calls.attached.borrow().len(), 1);
}

#[test]
fn last_logged_error_the_newest_error_lines_message_since_the_start_without_the_services_lead_0582() {
	let j = temp_jocasta();
	let dir = j.root().join("logs-last");
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(
		dir.join("jocasta-2026-11-08.log"),
		"2026-11-08T23:59:00.000Z ERROR [service] didn't start: yesterday's\n",
	)
	.unwrap();
	std::fs::write(
		dir.join("jocasta-2026-11-09.log"),
		[
			"2026-11-09T10:00:00.000Z ERROR [service] didn't start: earlier",
			"2026-11-09T10:05:00.000Z WARN [setup] a warning",
			"2026-11-09T10:05:01.000Z ERROR [service] didn't start: C:/x/projects.yaml:4: Unexpected token",
			"2026-11-09T10:05:02.000Z INFO [service] after",
			"",
		]
		.join("\n"),
	)
	.unwrap();
	let at = |text: &str| chrono::DateTime::parse_from_rfc3339(text).unwrap().timestamp_millis();
	assert_eq!(
		last_logged_error(&dir, at("2026-11-09T10:05:00.500Z")).as_deref(),
		Some("C:/x/projects.yaml:4: Unexpected token")
	);
	assert_eq!(last_logged_error(&dir, at("2026-11-09T11:00:00.000Z")), None);
	assert_eq!(last_logged_error(&j.root().join("none"), 0), None);
}

#[tokio::test]
async fn herdrs_client_exiting_closes_jocasta_the_service_is_told_when_it_answers_else_herdr_stops_here_0434_0549() {
	let (j, mut told) = setup();
	let calls = Rc::clone(&told.calls);
	told.tell = Box::new(move || {
		calls.closed.borrow_mut().push("tell service".to_owned());
		true
	});
	open_surface(&told).await.unwrap();
	assert_eq!(told.calls.closed(), ["tell service"]);
	// The client exited by itself: what happens next is said, since a question may open in a new terminal.
	assert!(
		told.calls
			.printed
			.borrow()
			.contains(&"jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.".to_owned())
	);

	let alone = another(&j);
	assert_eq!(open_surface(&alone).await.unwrap(), 0);
	assert_eq!(alone.calls.closed(), ["tell service", "stop herdr"]);

	// herdr that won't stop is said.
	let mut stuck = another(&j);
	stuck.stop = Box::new(|| Box::pin(async { Err(Error::other("didn't stop in 3 s")) }));
	open_surface(&stuck).await.unwrap();
	assert!(
		stuck
			.calls
			.printed
			.borrow()
			.contains(&"jocasta: herdr session jocasta didn't stop: didn't stop in 3 s".to_owned())
	);
	assert_eq!(stuck.calls.closed(), ["tell service"]);

	// herdr that never answers is waited for no longer than its bound.
	let mut hung = another(&j);
	let reached = Rc::new(Notify::new());
	let signal = Rc::clone(&reached);
	hung.stop = Box::new(move || {
		signal.notify_one();
		Box::pin(std::future::pending())
	});
	tokio::select! {
		_ = open_surface(&hung) => panic!("a herdr that never stops ended it"),
		() = reached.notified() => {}
	}
	assert_eq!(hung.calls.closed(), ["tell service"]);
}

#[tokio::test]
async fn closing_the_terminal_tells_the_service_once_and_ends_this_process_0549() {
	let (_j, mut f) = setup();
	let (client, gate) = oneshot::channel::<i32>();
	let gate = Rc::new(RefCell::new(Some(gate)));
	f.attach = Box::new(move || {
		let gate = gate.borrow_mut().take().unwrap();
		Box::pin(async move { Ok(gate.await.unwrap_or(0)) })
	});
	let exited = Rc::new(Notify::new());
	let signal = Rc::clone(&exited);
	f.exit = Box::new(move || signal.notify_one());
	let close = Rc::clone(&f.on_close);
	let run = open_surface(&f);
	let drive = async {
		while close.borrow().is_none() {
			tokio::time::sleep(Duration::from_millis(1)).await;
		}
		(close.borrow().as_ref().unwrap())();
		exited.notified().await;
		assert_eq!(f.calls.closed(), ["tell service", "stop herdr"]);
		client.send(0).unwrap();
	};
	let (result, ()) = tokio::join!(run, drive);
	assert_eq!(result.unwrap(), 0);
	// The client's exit after it closes nothing more.
	assert_eq!(f.calls.closed(), ["tell service", "stop herdr"]);
}

#[tokio::test]
async fn bare_jocasta_exits_0_when_the_service_stopped_herdr_itself_any_other_client_exit_keeps_herdrs_code_0491() {
	let (j, mut stopped) = setup();
	always(&mut stopped, 1);
	stopped.watch = Box::new(|| Box::new(|| Box::pin(async { true })));
	assert_eq!(open_surface(&stopped).await.unwrap(), 0);
	// Its own stop ended everything: nothing is closed a second time, and nothing says Jocasta is closing.
	assert!(stopped.calls.closed().is_empty());
	assert!(
		!stopped
			.calls
			.printed
			.borrow()
			.contains(&"jocasta: closing Jocasta; with threads at work it asks first, in a new terminal.".to_owned())
	);
	let mut quits = another(&j);
	always(&mut quits, 1);
	assert_eq!(open_surface(&quits).await.unwrap(), 1);
	// The check is made only for a failing client, and only once herdr's client has exited.
	let mut watching = another(&j);
	let (watched, checked) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
	let (w, c) = (Rc::clone(&watched), Rc::clone(&checked));
	watching.watch = Box::new(move || {
		w.set(w.get() + 1);
		let c = Rc::clone(&c);
		Box::new(move || {
			c.set(c.get() + 1);
			Box::pin(async { true })
		})
	});
	assert_eq!(open_surface(&watching).await.unwrap(), 0);
	assert_eq!((watched.get(), checked.get()), (1, 0));

	// Which service stopped it: the one jocasta-service.json named as the client started.
	let named = ServiceFile { port: 1, token: "t".to_owned(), pid: 7 };
	let closing = async |_: &ServiceFile| ServiceState::Closing;
	let up = async |_: &ServiceFile| ServiceState::Up;
	let down = async |_: &ServiceFile| ServiceState::Down;
	// Stop everything under way: /health says closing.
	assert!(service_stopped(Some(&named), Some(&named), closing).await);
	// Its close already ended by removing the file, or another service has started since.
	assert!(service_stopped(Some(&named), None, down).await);
	assert!(service_stopped(Some(&named), Some(&ServiceFile { pid: 8, ..named.clone() }), up).await);
	// herdr quit while the service runs, or the service died (its file stays): herdr's code.
	assert!(!service_stopped(Some(&named), Some(&named), up).await);
	assert!(!service_stopped(Some(&named), Some(&named), down).await);
	// No service when the client started: herdr's code.
	assert!(!service_stopped(None, None, closing).await);
}

#[tokio::test]
async fn a_herdr_server_that_dies_under_the_client_is_started_again_by_the_service_and_its_client_attached_again_0518()
{
	// herdr's server: answering, or gone until `back` more pings.
	let server = Rc::new(Cell::new((true, 0)));
	let (j, mut healed) = setup();
	let pings = Rc::clone(&server);
	healed.herdr_up = Box::new(move || {
		let (up, back) = pings.get();
		if up {
			return true;
		}
		if back > 0 {
			pings.set((false, back - 1));
			return false;
		}
		pings.set((true, 0));
		true
	});
	let slept = Rc::new(Cell::new(0));
	let total = Rc::clone(&slept);
	healed.sleep = Box::new(move |ms| total.set(total.get() + ms));
	let order: Rc<RefCell<Vec<String>>> = Rc::default();
	let (attach_order, tell_order) = (Rc::clone(&order), Rc::clone(&order));
	let exit_codes = Rc::new(RefCell::new(VecDeque::from([1, 0])));
	let dying = Rc::clone(&server);
	healed.attach = Box::new(move || {
		attach_order.borrow_mut().push("attach".to_owned());
		let code = exit_codes.borrow_mut().pop_front().unwrap_or(0);
		// The first client's server dies under it; the service has it back after a few pings.
		if code == 1 {
			dying.set((false, 3));
		}
		Box::pin(async move { Ok(code) })
	});
	healed.tell = Box::new(move || {
		tell_order.borrow_mut().push("close".to_owned());
		true
	});
	// The later client's exit is him quitting it while its server runs: Jocasta closes as before.
	assert_eq!(open_surface(&healed).await.unwrap(), 0);
	assert_eq!(*order.borrow(), ["attach", "attach", "close"]);
	// Attached again the same way as the first time.
	assert_eq!(healed.calls.attached.borrow()[1], healed.calls.attached.borrow()[0]);
	assert_eq!(slept.get(), 3 * 250);
	assert!(
		healed
			.calls
			.printed
			.borrow()
			.contains(&"jocasta: herdr session jocasta stopped; waiting for the service to start it again.".to_owned())
	);

	// herdr's server still running after the client exits and the service started none again: he quit it, a close.
	let mut quit = another(&j);
	always(&mut quit, 1);
	quit.health = Box::new(|| ServiceHealth { herdr_revivals: Some(0), ..up() });
	assert_eq!(open_surface(&quit).await.unwrap(), 1);
	assert_eq!(quit.calls.attached.borrow().len(), 1);
	assert_eq!(quit.calls.closed(), ["tell service", "stop herdr"]);

	// No service answers, so nothing starts herdr again: a close, without waiting.
	let mut alone = another(&j);
	always(&mut alone, 1);
	alone.health = Box::new(|| health(ServiceState::Down));
	alone.wait_ms = Some(0);
	let alone_pings = Rc::new(Cell::new(0));
	let counter = Rc::clone(&alone_pings);
	alone.herdr_up = Box::new(move || {
		counter.set(counter.get() + 1);
		false
	});
	assert_eq!(open_surface(&alone).await.unwrap(), 1);
	assert_eq!(alone.calls.attached.borrow().len(), 1);
	assert_eq!(alone.calls.closed(), ["tell service", "stop herdr"]);
	assert_eq!(alone_pings.get(), 1);

	// herdr not back within 30 s: a close, said.
	let mut late = another(&j);
	always(&mut late, 1);
	late.herdr_up = Box::new(|| false);
	let waited = Rc::new(Cell::new(0));
	let total = Rc::clone(&waited);
	late.sleep = Box::new(move |ms| total.set(total.get() + ms));
	assert_eq!(open_surface(&late).await.unwrap(), 1);
	assert_eq!(waited.get(), 30_000);
	assert_eq!(late.calls.attached.borrow().len(), 1);
	assert_eq!(late.calls.closed(), ["tell service", "stop herdr"]);
	assert!(late.calls.printed.borrow().contains(&"jocasta: herdr session jocasta isn't back after 30 s.".to_owned()));

	// A service that starts closing while it waits starts nothing: the wait ends there.
	let mut stopping = another(&j);
	always(&mut stopping, 1);
	let state = Rc::new(Cell::new(ServiceState::Up));
	let (seen, moved) = (Rc::clone(&state), Rc::clone(&state));
	stopping.health = Box::new(move || health(seen.get()));
	stopping.herdr_up = Box::new(|| false);
	let closing_wait = Rc::new(Cell::new(0));
	let total = Rc::clone(&closing_wait);
	stopping.sleep = Box::new(move |ms| {
		total.set(total.get() + ms);
		moved.set(ServiceState::Closing);
	});
	assert_eq!(open_surface(&stopping).await.unwrap(), 1);
	assert_eq!(closing_wait.get(), 250);
	assert_eq!(stopping.calls.attached.borrow().len(), 1);

	// The service's own stop (0491) is still checked first: 0, nothing closed, herdr never pinged.
	let mut own = another(&j);
	always(&mut own, 1);
	own.watch = Box::new(|| Box::new(|| Box::pin(async { true })));
	let pinged = Rc::new(Cell::new(0));
	let counter = Rc::clone(&pinged);
	own.herdr_up = Box::new(move || {
		counter.set(counter.get() + 1);
		false
	});
	assert_eq!(open_surface(&own).await.unwrap(), 0);
	assert_eq!((own.calls.attached.borrow().len(), pinged.get()), (1, 0));
	assert!(own.calls.closed().is_empty());
}

#[tokio::test]
async fn a_herdr_crash_the_service_healed_before_the_client_looked_is_not_him_quitting_the_client_attaches_again_0534()
{
	// herdr's server crashed under the client and the service started it again at once: by the time the client's exit
	// is looked at, herdr answers again. Only the service's restart count says it wasn't him.
	let revivals = Rc::new(Cell::new(0_u64));
	let (j, mut crashed) = setup();
	let seen = Rc::clone(&revivals);
	crashed.health = Box::new(move || ServiceHealth { herdr_revivals: Some(seen.get()), ..up() });
	let order: Rc<RefCell<Vec<String>>> = Rc::default();
	let (attach_order, tell_order) = (Rc::clone(&order), Rc::clone(&order));
	let exit_codes = Rc::new(RefCell::new(VecDeque::from([1, 0])));
	let bumped = Rc::clone(&revivals);
	crashed.attach = Box::new(move || {
		attach_order.borrow_mut().push("attach".to_owned());
		let code = exit_codes.borrow_mut().pop_front().unwrap_or(0);
		// The crash and the service's restart both happen before the client's exit is looked at.
		if code == 1 {
			bumped.set(bumped.get() + 1);
		}
		Box::pin(async move { Ok(code) })
	});
	crashed.tell = Box::new(move || {
		tell_order.borrow_mut().push("close".to_owned());
		true
	});
	assert_eq!(open_surface(&crashed).await.unwrap(), 0);
	assert_eq!(*order.borrow(), ["attach", "attach", "close"]);

	// The same exit with no restart since the client attached is him quitting: a close at once.
	let mut quit = another(&j);
	quit.health = Box::new(|| ServiceHealth { herdr_revivals: Some(3), ..up() });
	always(&mut quit, 1);
	let calls = Rc::clone(&quit.calls);
	quit.tell = Box::new(move || {
		calls.closed.borrow_mut().push("tell service".to_owned());
		true
	});
	assert_eq!(open_surface(&quit).await.unwrap(), 1);
	assert_eq!(quit.calls.closed(), ["tell service"]);
}

#[tokio::test]
async fn a_client_exit_while_another_jocasta_holds_the_session_closes_nothing_0434() {
	let (j, mut f) = setup();
	f.enter = Box::new(|| Box::new(|| false));
	open_surface(&f).await.unwrap();
	assert_eq!(f.calls.attached.borrow().len(), 1);
	assert!(f.calls.closed().is_empty());

	// A client that couldn't start closes nothing either.
	let mut failed = another(&j);
	let left = Rc::new(Cell::new(0));
	let counter = Rc::clone(&left);
	failed.enter = Box::new(move || {
		let counter = Rc::clone(&counter);
		Box::new(move || {
			counter.set(counter.get() + 1);
			counter.get() > 0
		})
	});
	failed.attach = Box::new(|| Box::pin(async { Err(Error::other("spawn failed")) }));
	assert!(message(&open_surface(&failed).await.unwrap_err()).contains("spawn failed"));
	assert_eq!(left.get(), 1);
	assert!(failed.calls.closed().is_empty());
}

#[tokio::test]
async fn the_terminal_closing_closes_jocasta_once_with_herdrs_client_exiting_at_the_same_time_then_exits_0434() {
	let (_j, mut f) = setup();
	let (client, gate) = oneshot::channel::<i32>();
	let gate = Rc::new(RefCell::new(Some(gate)));
	f.attach = Box::new(move || {
		let gate = gate.borrow_mut().take().unwrap();
		Box::pin(async move { Ok(gate.await.unwrap_or(0)) })
	});
	let exited = Rc::new(Cell::new(false));
	let flag = Rc::clone(&exited);
	f.exit = Box::new(move || flag.set(true));
	let close = Rc::clone(&f.on_close);
	let run = open_surface(&f);
	let drive = async {
		while close.borrow().is_none() {
			tokio::time::sleep(Duration::from_millis(1)).await;
		}
		(close.borrow().as_ref().unwrap())();
		client.send(0).unwrap();
	};
	let (result, ()) = tokio::join!(run, drive);
	assert_eq!(result.unwrap(), 0);
	// The exit call may land just after the client's; the close itself ran once.
	assert_eq!(f.calls.closed(), ["tell service", "stop herdr"]);
	let _ = exited.get();
}

#[test]
fn attached_surface_the_last_jocasta_to_leave_closes_files_of_jocasta_processes_that_are_gone_dont_count() {
	let j = temp_jocasta();
	let dir = j.root().join("surfaces");
	let alive: Rc<RefCell<HashSet<u32>>> = Rc::new(RefCell::new(HashSet::from([101, 102])));
	let make = |pid: u32| {
		let alive = Rc::clone(&alive);
		attached_surface(
			&dir,
			Some(pid),
			Some(Box::new(move |p| alive.borrow().contains(&p))),
			Some(Box::new(|_| None)),
		)
		.unwrap()
	};
	let first = make(101);
	let second = make(102);
	std::fs::write(dir.join("999"), "").unwrap();
	assert!(!first.leave());
	alive.borrow_mut().remove(&101);
	assert!(second.leave());
	assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn attached_surface_a_file_whose_pid_now_belongs_to_a_process_started_after_it_was_written_is_stale_lc_4() {
	let j = temp_jocasta();
	let dir = j.root().join("surfaces");
	let now = || i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
	let own =
		attached_surface(&dir, Some(101), Some(Box::new(|_| true)), Some(Box::new(move |_| Some(now())))).unwrap();
	let stale = dir.join("999");
	std::fs::write(&stale, "").unwrap();
	let hour_ago = SystemTime::now() - Duration::from_secs(3_600);
	std::fs::File::options().write(true).open(&stale).unwrap().set_modified(hour_ago).unwrap();
	assert!(own.leave());
	assert!(!stale.exists());
}
