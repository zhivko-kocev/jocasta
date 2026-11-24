//! `thread_keys.rs`'s tests.

use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::FutureExt;
use serde_json::{Value, json};

use super::*;
use crate::frontier::server::popups::Popups;
use crate::shared::errors::Error;
use crate::shared::process::{AbortSignal, aborted};
use crate::shared::testing::temp_jocasta;

/// Waits (up to 10 s) until `check` holds.
async fn until(mut check: impl FnMut() -> bool) {
	for _ in 0..2_000 {
		if check() {
			return;
		}
		sleep(Duration::from_millis(5)).await;
	}
	panic!("the condition never held");
}

/// What the fake service was asked, and what each question's answer is.
struct Keyed {
	calls: Mutex<Vec<Vec<String>>>,
}

impl Keyed {
	fn note(&self, call: &[&str]) {
		self.calls.lock().unwrap().push(call.iter().map(|s| (*s).to_owned()).collect());
	}
}

impl KeyService for Keyed {
	fn key_stop<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		let pane = pane_id.unwrap_or("undefined").to_owned();
		Box::pin(async move { Err(Error::user(format!("Stop: {pane} isn't working; nothing to stop."))) })
	}
	fn key_review<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		let pane = pane_id.unwrap_or("undefined").to_owned();
		Box::pin(async move { Ok(format!("Reviewing {pane}")) })
	}
	fn key_finishable<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<KeyFinishable>> {
		let pane = pane_id.unwrap_or("undefined").to_owned();
		Box::pin(async move {
			Ok(KeyFinishable {
				pane_id: format!("{pane}-thread"),
				name: "acme-api/retry".to_owned(),
				detail: "2 commits".to_owned(),
				trunk: "main".to_owned(),
			})
		})
	}
	fn key_finish<'a>(&'a self, pane_id: &'a str, how: FinishHow) -> BoxFuture<'a, Result<KeyOutcome>> {
		self.note(&["finish", pane_id, how.as_str()]);
		Box::pin(async { Ok(said("Merged.")) })
	}
	fn new_thread_form<'a>(&'a self, pane_id: Option<&'a str>) -> BoxFuture<'a, Result<NewThreadForm>> {
		self.note(&["form", pane_id.unwrap_or("undefined")]);
		Box::pin(async { Ok(NewThreadForm { projects: vec!["acme-api".to_owned()], project: "acme-api".to_owned() }) })
	}
	fn key_start<'a>(&'a self, project: &'a str, brief: &'a str) -> BoxFuture<'a, Result<KeyOutcome>> {
		self.note(&["start", project, brief]);
		Box::pin(async { Ok(said("Started.")) })
	}
	fn key_stop_all<'a>(&'a self) -> BoxFuture<'a, Result<String>> {
		self.note(&["stop-all"]);
		Box::pin(async { Ok("stopped 3 turns".to_owned()) })
	}
}

/// How a question is answered: its choice and a form's fields.
type Answers = Arc<Mutex<HashMap<String, Value>>>;

/// His keys' deps against a fake service; `answers` answers each question by its title, typing its code or not.
struct Keys5 {
	deps: ActionDeps,
	service: Arc<Keyed>,
	asked: Arc<Mutex<Vec<String>>>,
	said: Arc<Mutex<Vec<String>>>,
	fronted: Arc<Mutex<Vec<Popup>>>,
	answers: Answers,
	types_code: Arc<AtomicBool>,
	answering: Arc<AtomicBool>,
}

fn key_deps() -> Keys5 {
	let service = Arc::new(Keyed { calls: Mutex::new(Vec::new()) });
	let asked = Arc::new(Mutex::new(Vec::new()));
	let said_lines = Arc::new(Mutex::new(Vec::new()));
	let fronted = Arc::new(Mutex::new(Vec::new()));
	let answers: Answers = Arc::new(Mutex::new(HashMap::new()));
	let types_code = Arc::new(AtomicBool::new(true));
	let answering = Arc::new(AtomicBool::new(true));
	let popups = Arc::new(Popups::new());
	let (a, on, typing, mine) =
		(Arc::clone(&asked), Arc::clone(&answering), Arc::clone(&types_code), Arc::clone(&answers));
	let ask = move |view: &PopupView, signal: Option<AbortSignal>, _from_key: bool| {
		let popup = popups.open(view, true);
		let closing = popup.clone();
		tokio::spawn(async move {
			aborted(&mut signal.clone()).await;
			closing.close(None);
		});
		a.lock().unwrap().push(view.title.clone());
		if on.load(Ordering::SeqCst) {
			let mut answer =
				mine.lock().unwrap().get(&view.title).cloned().unwrap_or_else(|| json!({"choice": view.fallback}));
			let types = typing.load(Ordering::SeqCst);
			answer["typed"] = if types { json!(view.code) } else { json!("AAA") };
			let (code, popups) = (popup.code().to_owned(), Arc::clone(&popups));
			tokio::spawn(async move {
				for _ in 0..(if types { 1 } else { 3 }) {
					popups.answer(&code, &answer);
				}
			});
		}
		popup
	};
	let (f, s) = (Arc::clone(&fronted), Arc::clone(&said_lines));
	let deps = ActionDeps {
		service: Arc::clone(&service) as Arc<dyn KeyService>,
		ask: Arc::new(ask),
		front: Box::new(move |popup| f.lock().unwrap().push(popup.clone())),
		quit: Box::new(|| Box::pin(async { Ok(CloseOutcome::Kept) })),
		progress: Box::new(move |text| s.lock().unwrap().push(text.to_owned())),
		asking: Mutex::new(HashMap::new()),
		wait_ms: 60_000,
	};
	Keys5 { deps, service, asked, said: said_lines, fronted, answers, types_code, answering }
}

fn text_of(outcome: Result<Option<ActionOutcome>>) -> String {
	outcome.unwrap().unwrap().text
}

#[tokio::test]
async fn his_keys_finish_asks_merge_pr_cancel_new_thread_starts_from_the_form_refusals_are_errors() {
	let keys = key_deps();
	keys.answers.lock().unwrap().insert("Jocasta: Finish acme-api/retry".to_owned(), json!({"choice": MERGE}));
	keys.answers.lock().unwrap().insert(
		"Jocasta: New thread".to_owned(),
		json!({"choice": START, "project": "acme-api", "brief": "Fix the retry."}),
	);
	let deps = &keys.deps;
	assert_eq!(run_action(PluginAction::Finish, Some("p1"), deps).await.unwrap(), Some(said("Merged.")));
	assert_eq!(*keys.service.calls.lock().unwrap(), vec![vec!["finish", "p1-thread", "merge"]]);
	// The start line goes out at once, before the slow part (UX-5).
	assert_eq!(*keys.said.lock().unwrap(), vec!["Merging acme-api/retry into main…"]);
	// New thread's form is for the picked pane's project (0581, C10).
	assert_eq!(run_action(PluginAction::NewThread, Some("p9"), deps).await.unwrap(), Some(said("Started.")));
	assert_eq!(
		keys.service.calls.lock().unwrap()[1..],
		[vec!["form", "p9"], vec!["start", "acme-api", "Fix the retry."]]
	);
	assert_eq!(keys.said.lock().unwrap().last().unwrap(), "Starting a thread in acme-api: worktree, omp, brief…");
	assert_eq!(*keys.asked.lock().unwrap(), ["Jocasta: Finish acme-api/retry", "Jocasta: New thread"]);
	assert_eq!(run_action(PluginAction::Review, Some("p2"), deps).await.unwrap(), Some(said("Reviewing p2")));
	assert_eq!(run_action(PluginAction::Quit, None, deps).await.unwrap(), Some(said("Jocasta keeps running.")));
	let stopped = run_action(PluginAction::Stop, Some("p3"), deps).await.unwrap_err();
	assert_eq!(stopped.to_string(), "Stop: p3 isn't working; nothing to stop.");
	// Cancel leaves the thread as it was.
	keys.answers.lock().unwrap().insert("Jocasta: Finish acme-api/retry".to_owned(), json!({"choice": "Cancel"}));
	assert_eq!(
		text_of(run_action(PluginAction::Finish, Some("p1"), deps).await),
		"Finish: acme-api/retry stays as it was."
	);
	// Merge without its code, three times: cancelled.
	keys.answers.lock().unwrap().insert("Jocasta: Finish acme-api/retry".to_owned(), json!({"choice": MERGE}));
	keys.types_code.store(false, Ordering::SeqCst);
	assert_eq!(
		text_of(run_action(PluginAction::Finish, Some("p1"), deps).await),
		"Finish: acme-api/retry stays as it was."
	);
	assert_eq!(keys.service.calls.lock().unwrap().iter().filter(|call| call[0] == "finish").count(), 1);
}

#[tokio::test]
async fn his_keys_pr_finishes_without_a_start_line_and_a_new_thread_without_a_brief_starts_none() {
	let keys = key_deps();
	keys.answers.lock().unwrap().insert("Jocasta: Finish acme-api/retry".to_owned(), json!({"choice": PR}));
	keys.answers
		.lock()
		.unwrap()
		.insert("Jocasta: New thread".to_owned(), json!({"choice": START, "project": "acme-api"}));
	let deps = &keys.deps;
	assert_eq!(run_action(PluginAction::Finish, Some("p1"), deps).await.unwrap(), Some(said("Merged.")));
	assert_eq!(*keys.service.calls.lock().unwrap(), vec![vec!["finish", "p1-thread", "pr"]]);
	assert!(keys.said.lock().unwrap().is_empty());
	// The form's Start with no brief is answered `Invalid` by the popup, so the question stays open; it ends unanswered.
	let mut short = key_deps();
	short.deps.wait_ms = 50;
	short
		.answers
		.lock()
		.unwrap()
		.insert("Jocasta: New thread".to_owned(), json!({"choice": START, "project": "acme-api"}));
	assert_eq!(text_of(run_action(PluginAction::NewThread, None, &short.deps).await), "New thread: none started.");
	assert!(short.said.lock().unwrap().is_empty());
}

#[tokio::test]
async fn one_open_question_per_key_and_thread_a_repeat_brings_it_to_the_front_finish_and_new_thread_wait_as_allow_does()
{
	let mut keys = key_deps();
	keys.answering.store(false, Ordering::SeqCst);
	keys.deps.wait_ms = 50;
	let deps = Arc::new(keys.deps);
	let finishing = tokio::spawn({
		let deps = Arc::clone(&deps);
		async move { run_action(PluginAction::Finish, Some("p1"), &deps).await }
	});
	until(|| keys.asked.lock().unwrap().len() == 1).await;
	let repeat = run_action(PluginAction::Finish, Some("p1"), &deps).await.unwrap().unwrap();
	assert_eq!(
		repeat,
		ActionOutcome { text: "Finish: already asking about acme-api/retry.".to_owned(), is_error: false }
	);
	let forming = tokio::spawn({
		let deps = Arc::clone(&deps);
		async move { run_action(PluginAction::NewThread, None, &deps).await }
	});
	until(|| keys.asked.lock().unwrap().len() == 2).await;
	assert_eq!(run_action(PluginAction::NewThread, None, &deps).await.unwrap(), None);
	assert_eq!(keys.asked.lock().unwrap().len(), 2);
	assert_eq!(keys.fronted.lock().unwrap().len(), 2);
	// Unanswered, each ends after its wait.
	assert_eq!(text_of(finishing.await.unwrap()), "Finish: acme-api/retry stays as it was.");
	assert_eq!(text_of(forming.await.unwrap()), "New thread: none started.");
	assert!(deps.asking.lock().unwrap().is_empty());
}

#[tokio::test]
async fn quit_says_whether_jocasta_keeps_running_or_stopped() {
	let mut keys = key_deps();
	keys.deps.quit = Box::new(|| Box::pin(async { Ok(CloseOutcome::Stopped) }));
	assert_eq!(text_of(run_action(PluginAction::Quit, None, &keys.deps).await), "Jocasta stopped.");
}

#[derive(Default)]
struct Notes(Mutex<Vec<(Toast, bool)>>);

impl Notifier for Notes {
	fn notify(&self, toast: Toast, show: bool) -> BoxFuture<'_, ()> {
		locked(&self.0).push((toast, show));
		async {}.boxed()
	}
}

/// The keys these tests don't reach: each refuses.
impl KeyService for Notes {
	fn key_stop<'a>(&'a self, _pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_review<'a>(&'a self, _pane_id: Option<&'a str>) -> BoxFuture<'a, Result<String>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_finishable<'a>(&'a self, _pane_id: Option<&'a str>) -> BoxFuture<'a, Result<KeyFinishable>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_finish<'a>(&'a self, _pane_id: &'a str, _how: FinishHow) -> BoxFuture<'a, Result<KeyOutcome>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn new_thread_form<'a>(&'a self, _pane_id: Option<&'a str>) -> BoxFuture<'a, Result<NewThreadForm>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_start<'a>(&'a self, _project: &'a str, _brief: &'a str) -> BoxFuture<'a, Result<KeyOutcome>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}

	fn key_stop_all<'a>(&'a self) -> BoxFuture<'a, Result<String>> {
		async { Err(Error::user("no keys here")) }.boxed()
	}
}

struct Actors {
	notes: Arc<Notes>,
	broadcast: Arc<Mutex<Vec<ServerMessage>>>,
	keys: Arc<ThreadKeys>,
}

fn actors() -> Actors {
	let notes = Arc::new(Notes::default());
	let sent = Arc::new(Mutex::new(Vec::new()));
	let record = Arc::clone(&sent);
	let keys = ThreadKeys::new(KeyParts {
		service: notes.clone(),
		ask: crate::frontier::server::asker::asker(Arc::new(Popups::new()), None),
		front: Box::new(|_| {}),
		broadcast: Arc::new(move |message| locked(&record).push(message)),
		quit: Arc::new(|| async { CloseOutcome::Kept }.boxed()),
		waiting_ms: 1_000,
	});
	Actors { notes, broadcast: sent, keys }
}

#[tokio::test]
async fn a_refused_key_is_said_in_the_dock_and_notified() {
	let _j = temp_jocasta();
	let a = actors();
	Arc::clone(&a.keys).run(PluginAction::Stop, Some("p_1".to_owned())).await;
	assert_eq!(*locked(&a.broadcast), [ServerMessage::Action { text: "no keys here".to_owned(), is_error: true }]);
	until(|| locked(&a.notes.0).len() == 1).await;
}

#[tokio::test]
async fn an_outcome_is_broadcast_a_refusal_also_notifies_once_per_text_each_thirty_seconds() {
	let _j = temp_jocasta();
	let a = actors();
	let state = Arc::clone(&a.keys.reporter);
	let refusal = |text: &str| ActionOutcome { text: text.to_owned(), is_error: true };
	state.report(said("Finished Fix login"));
	assert!(locked(&a.notes.0).is_empty());
	state.report(refusal("Stop: no thread here"));
	state.report(refusal("Stop: no thread here"));
	state.report(refusal("Other refusal"));
	until(|| locked(&a.notes.0).len() == 3).await;
	{
		let notes = locked(&a.notes.0);
		let shown: Vec<(&str, bool)> = notes.iter().map(|(toast, show)| (toast.text.as_str(), *show)).collect();
		assert_eq!(shown, [("Stop: no thread here", true), ("Stop: no thread here", false), ("Other refusal", true)]);
		assert!(
			notes.iter().all(|(toast, _)| toast.reason == ToastReason::Failed && toast.level == ToastLevel::Warning)
		);
	}
	assert_eq!(
		*locked(&a.broadcast),
		[
			ServerMessage::Action { text: "Finished Fix login".to_owned(), is_error: false },
			ServerMessage::Action { text: "Stop: no thread here".to_owned(), is_error: true },
			ServerMessage::Action { text: "Stop: no thread here".to_owned(), is_error: true },
			ServerMessage::Action { text: "Other refusal".to_owned(), is_error: true },
		]
	);
	// Thirty seconds later the same text is shown again.
	for (_, at) in locked(&state.refusals_shown).iter_mut() {
		*at -= REFUSAL_TOAST_MS;
	}
	state.report(refusal("Stop: no thread here"));
	until(|| locked(&a.notes.0).len() == 4).await;
	assert!(locked(&a.notes.0)[3].1);
	// Control characters are cleaned from what he reads.
	state.report(said("bad \x1b[2J text"));
	let last = locked(&a.broadcast).last().cloned();
	assert_eq!(last, Some(ServerMessage::Action { text: pane_text("bad \x1b[2J text"), is_error: false }));
}

#[tokio::test]
async fn stop_all_asks_first_and_only_a_yes_stops_every_turn_0686() {
	let keys = key_deps();
	let title = "Jocasta: Stop every running turn?";
	let deps = &keys.deps;
	// No is the default: nothing is stopped.
	assert_eq!(text_of(run_action(PluginAction::StopAll, None, deps).await), "Stop all: every turn runs on.");
	assert!(keys.service.calls.lock().unwrap().is_empty());
	keys.answers.lock().unwrap().insert(title.to_owned(), json!({"choice": "Yes"}));
	assert_eq!(text_of(run_action(PluginAction::StopAll, None, deps).await), "stopped 3 turns");
	assert_eq!(*keys.service.calls.lock().unwrap(), vec![vec!["stop-all"]]);
	assert_eq!(*keys.asked.lock().unwrap(), [title, title]);
}
