//! `plugin.rs`'s tests, ported from `test/frontier/server/plugin.test.ts`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::frontier::server::herdr::fake_herdr::FakeHerdr;
use crate::frontier::server::herdr::{
	AgentState, Env, HerdrError, HerdrResult, NotificationResult, OnChange, PaneIds, PaneMetadata, RespawnOptions,
	SplitOptions, StopWatch, TabPane, TabSpec, Tokens, WorkspaceInfo, WorktreeCreated, WorktreeOptions,
};
use crate::frontier::server::popups::{
	ALLOW, AnswerTaken, PopupRead, Popups, allow_question, finish_question as finish_view,
};
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

/// A boss workspace with its boss pane running omp.
async fn boss_session() -> (Arc<FakeHerdr>, String, String) {
	let herdr = Arc::new(FakeHerdr::new());
	let workspace = herdr
		.ensure_workspace("Jocasta", Path::new("/home"), Some(&["omp".to_owned()]), None)
		.await
		.expect("the workspace");
	let boss = herdr.state().pane_list[0].pane_id.clone();
	herdr.set_state(&boss, AgentState::Idle, None, true);
	(herdr, boss, workspace)
}

fn shown_code(herdr: &FakeHerdr) -> Option<String> {
	herdr
		.state()
		.popup
		.as_ref()
		.and_then(|env| env.iter().find(|(name, _)| name == POPUP_ENV))
		.map(|(_, code)| code.clone())
}

fn view_json(view: &AgentView) -> Value {
	json!({"source": view.source, "label": view.label, "filter": view.filter, "sort": view.sort})
}

#[tokio::test]
async fn today_opens_beside_the_boss_at_65_35_is_left_alone_while_open_and_opens_again_once_gone() {
	let _j = temp_jocasta();
	let (herdr, boss, workspace) = boss_session().await;
	let now = Arc::new(AtomicI64::new(1_000_000));
	let clock = Arc::clone(&now);
	let panes = PluginPanes::new(herdr.clone(), Some(Arc::new(move || clock.load(Ordering::SeqCst))));
	panes.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	let open = &herdr.of("openPluginPane")[0][0];
	assert_eq!(open["pluginId"], "jocasta");
	assert_eq!(open["entrypoint"], "today");
	assert_eq!(open["placement"], "split");
	assert_eq!(open["targetPaneId"], json!(boss));
	let today = herdr.state().pane_list.iter().find(|p| Some(&p.pane_id) == panes.today().as_ref()).cloned().unwrap();
	assert!(is_today_pane(&today));
	assert_eq!(herdr.of("setSplitRatio"), vec![vec![json!(today.tab_id), json!(0.65)]]);
	// The Agents view leaves the boss's workspace out.
	assert_eq!(herdr.of("setAgentView"), vec![vec![view_json(&agent_view(&workspace))]]);

	panes.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	assert_eq!(herdr.of("openPluginPane").len(), 1);
	assert_eq!(herdr.of("setAgentView").len(), 1);

	// Gone after a long life: open again at once.
	now.fetch_add(60_000, Ordering::SeqCst);
	herdr.state().pane_list.retain(|p| p.pane_id != today.pane_id);
	panes.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	assert_eq!(herdr.of("openPluginPane").len(), 2);
	// Gone right after opening: it waits before opening again.
	let again = panes.today();
	herdr.state().pane_list.retain(|p| Some(&p.pane_id) != again.as_ref());
	now.fetch_add(1_000, Ordering::SeqCst);
	panes.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	assert_eq!(herdr.of("openPluginPane").len(), 2);
	now.fetch_add(10_000, Ordering::SeqCst);
	panes.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	assert_eq!(herdr.of("openPluginPane").len(), 3);
}

#[tokio::test]
async fn a_today_found_in_the_boss_workspace_after_a_restart_is_taken_never_opened_twice() {
	let _j = temp_jocasta();
	let (herdr, boss, _) = boss_session().await;
	PluginPanes::new(herdr.clone(), None).heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	let again = PluginPanes::new(herdr.clone(), None);
	again.heal(&herdr.snapshot().await.unwrap(), Some(&boss)).await;
	assert_eq!(herdr.of("openPluginPane").len(), 1);
	assert!(again.today().is_some());
}

#[tokio::test]
async fn heal_does_nothing_without_the_boss_pane() {
	let _j = temp_jocasta();
	let (herdr, _, _) = boss_session().await;
	let panes = PluginPanes::new(herdr.clone(), None);
	let snapshot = herdr.snapshot().await.unwrap();
	panes.heal(&snapshot, None).await;
	panes.heal(&snapshot, Some("nope")).await;
	assert!(herdr.of("openPluginPane").is_empty());
	assert!(herdr.of("setAgentView").is_empty());
}

#[tokio::test]
async fn the_agents_view_is_applied_once_the_boss_workspace_exists() {
	let herdr = FakeHerdr::new();
	assert!(!apply_agent_view(&herdr).await.unwrap());
	assert!(herdr.of("setAgentView").is_empty());
	let workspace = herdr.ensure_workspace("Jocasta", Path::new("/home"), None, None).await.unwrap();
	assert!(apply_agent_view(&herdr).await.unwrap());
	assert_eq!(herdr.of("setAgentView"), vec![vec![view_json(&agent_view(&workspace))]]);
}

#[test]
fn the_agents_view_sorts_attention_then_status_and_leaves_the_boss_workspace_out() {
	let view = agent_view("w1");
	assert_eq!(view.source, "plugin:jocasta");
	assert_eq!(view.label.as_deref(), Some("threads"));
	assert_eq!(view.filter, Some(json!({"op": "not", "filter": {"op": "eq", "field": "workspace_id", "value": "w1"}})));
	let sort = view.sort.unwrap();
	assert_eq!(
		sort.iter().map(|s| s["field"].as_str().unwrap()).collect::<Vec<_>>(),
		["attention", "status", "state_change_seq"]
	);
}

#[test]
fn a_notification_body_is_cut_to_240_characters_with_an_ellipsis() {
	assert_eq!(note_body("short"), "short");
	let exact = "x".repeat(240);
	assert_eq!(note_body(&exact), exact);
	let cut = note_body(&"x".repeat(241));
	assert_eq!(cut.encode_utf16().count(), 240);
	assert!(cut.ends_with('…'));
}

fn ask_options(hold: i64, retry: i64) -> AskOptions {
	AskOptions { hold_wait_ms: Some(hold), retry_ms: Some(retry), now: None }
}

fn allow_view(message: &str, code: &str) -> PopupView {
	allow_question(message, "d", Some(code))
}

#[tokio::test]
async fn questions_show_one_at_a_time_in_the_ask_popup_with_a_request_notification_a_popup_closed_unanswered_denies() {
	let _j = temp_jocasta();
	let (herdr, _, _) = boss_session().await;
	let ask = AskPopups::new(herdr.clone(), ask_options(60_000, 60_000));
	let popups = Popups::new();
	let first = popups.open(&allow_view("Remove it?", "ABC"), false);
	let second = popups.open(&allow_view("Merge it?", "DEF"), false);
	ask.show(&first);
	ask.show(&second);
	until(|| herdr.state().popup.is_some()).await;
	assert_eq!(shown_code(&herdr).as_deref(), Some(first.code()));
	assert_eq!(
		herdr.of("notify"),
		vec![vec![json!({"title": "Jocasta asks", "body": "Remove it?", "sound": "request"})]]
	);
	// Only the shown question's code holds.
	assert_eq!(ask.hold(second.code()).status(), 403);
	let held = ask.hold(first.code());
	assert_eq!(held.status(), 200);
	assert_eq!(held.headers()["content-type"], "text/event-stream");
	assert_eq!(held.headers()["cache-control"], "no-store");
	assert_eq!(ask.hold(first.code()).status(), 403);
	assert_eq!(herdr.of("openPluginPane").len(), 1);
	// Closed some other way: the held request ends, the question denies, the next one shows.
	drop(held);
	assert_eq!(first.answer().await, "Deny");
	assert_eq!(first.how().await, PopupEnd::Closed);
	until(|| shown_code(&herdr).as_deref() == Some(second.code())).await;
	let reader = ask.hold(second.code()).into_body();
	assert_eq!(popups.answer(second.code(), &json!({"choice": ALLOW, "typed": "DEF"})), AnswerTaken::Taken);
	// The answer ends the held request, so the popup closes.
	let bytes = axum::body::to_bytes(reader, usize::MAX).await.unwrap();
	let text = String::from_utf8(bytes.to_vec()).unwrap();
	assert!(text.starts_with(": shown\n\n"), "{text}");
	assert!(text.contains("event: end"), "{text}");
}

/// A herdr that refuses the first `refusals` popups, as `ui_busy`.
struct Busy {
	inner: Arc<FakeHerdr>,
	refusals: AtomicI32,
}

impl Herdr for Busy {
	fn started_server(&self) -> bool {
		self.inner.started_server()
	}
	fn ping(&self) -> BoxFuture<'_, HerdrResult<Option<String>>> {
		self.inner.ping()
	}
	fn ensure_workspace<'a>(
		&'a self,
		label: &'a str,
		cwd: &'a Path,
		command: Option<&'a [String]>,
		env: Option<&'a Env>,
	) -> BoxFuture<'a, HerdrResult<String>> {
		self.inner.ensure_workspace(label, cwd, command, env)
	}
	fn workspaces(&self) -> BoxFuture<'_, HerdrResult<Vec<WorkspaceInfo>>> {
		self.inner.workspaces()
	}
	fn snapshot(&self) -> BoxFuture<'_, HerdrResult<HerdrSnapshot>> {
		self.inner.snapshot()
	}
	fn close_workspace<'a>(&'a self, workspace_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.close_workspace(workspace_id)
	}
	fn create_tab<'a>(&'a self, workspace_id: &'a str, tab: &'a TabSpec) -> BoxFuture<'a, HerdrResult<TabPane>> {
		self.inner.create_tab(workspace_id, tab)
	}
	fn create_worktree<'a>(
		&'a self,
		workspace_id: &'a str,
		opts: WorktreeOptions,
	) -> BoxFuture<'a, HerdrResult<WorktreeCreated>> {
		self.inner.create_worktree(workspace_id, opts)
	}
	fn move_workspace<'a>(&'a self, workspace_id: &'a str, index: usize) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.move_workspace(workspace_id, index)
	}
	fn respawn<'a>(&'a self, pane_id: &'a str, opts: &'a RespawnOptions) -> BoxFuture<'a, HerdrResult<TabPane>> {
		self.inner.respawn(pane_id, opts)
	}
	fn split_beside<'a>(&'a self, pane_id: &'a str, opts: &'a SplitOptions) -> BoxFuture<'a, HerdrResult<String>> {
		self.inner.split_beside(pane_id, opts)
	}
	fn prompt<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.prompt(pane_id, text)
	}
	fn send_text<'a>(&'a self, pane_id: &'a str, text: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.send_text(pane_id, text)
	}
	fn stop<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.stop(pane_id)
	}
	fn submit<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.submit(pane_id)
	}
	fn send_keys<'a>(&'a self, pane_id: &'a str, keys: &'a [String]) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.send_keys(pane_id, keys)
	}
	fn notify<'a>(&'a self, note: &'a Note) -> BoxFuture<'a, HerdrResult<NotificationResult>> {
		self.inner.notify(note)
	}
	fn focus<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.focus(pane_id)
	}
	fn open_plugin_pane<'a>(&'a self, opts: &'a PluginPaneOptions) -> BoxFuture<'a, HerdrResult<Option<String>>> {
		if self.refusals.fetch_sub(1, Ordering::SeqCst) > 0 {
			return Box::pin(std::future::ready(Err(HerdrError::new("error", "ui_busy"))));
		}
		self.inner.open_plugin_pane(opts)
	}
	fn close_popup(&self) -> BoxFuture<'_, HerdrResult<bool>> {
		self.inner.close_popup()
	}
	fn set_split_ratio<'a>(&'a self, tab_id: &'a str, ratio: f64) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.set_split_ratio(tab_id, ratio)
	}
	fn set_agent_view<'a>(&'a self, view: &'a AgentView) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.set_agent_view(view)
	}
	fn screen<'a>(&'a self, pane_id: &'a str) -> BoxFuture<'a, HerdrResult<String>> {
		self.inner.screen(pane_id)
	}
	fn report_pane_metadata<'a>(&'a self, pane_id: &'a str, meta: &'a PaneMetadata) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.report_pane_metadata(pane_id, meta)
	}
	fn report_workspace_metadata<'a>(
		&'a self,
		workspace_id: &'a str,
		tokens: &'a Tokens,
		ttl_ms: Option<i64>,
	) -> BoxFuture<'a, HerdrResult<()>> {
		self.inner.report_workspace_metadata(workspace_id, tokens, ttl_ms)
	}
	fn panes(&self) -> BoxFuture<'_, HerdrResult<Vec<PaneInfo>>> {
		self.inner.panes()
	}
	fn pane_root<'a>(&'a self, pane_id: &'a str, timeout_ms: Option<i64>) -> BoxFuture<'a, HerdrResult<Option<u32>>> {
		self.inner.pane_root(pane_id, timeout_ms)
	}
	fn watch(&self, pane_ids: PaneIds, on_change: OnChange) -> StopWatch {
		self.inner.watch(pane_ids, on_change)
	}
	fn stop_server(&self) -> BoxFuture<'_, HerdrResult<()>> {
		self.inner.stop_server()
	}
	fn close(&self) {
		self.inner.close();
	}
}

#[tokio::test]
async fn a_popup_herdr_could_not_open_is_asked_for_again_until_it_shows() {
	let _j = temp_jocasta();
	let (fake, _, _) = boss_session().await;
	let herdr = Arc::new(Busy { inner: Arc::clone(&fake), refusals: AtomicI32::new(2) });
	let ask = AskPopups::new(herdr.clone(), ask_options(60_000, 5));
	let popup = Popups::new().open(&allow_question("m", "d", None), false);
	ask.show(&popup);
	until(|| fake.state().popup.is_some()).await;
	assert_eq!(shown_code(&fake).as_deref(), Some(popup.code()));
	assert_eq!(herdr.refusals.load(Ordering::SeqCst), -1);
	popup.close(None);
}

#[tokio::test]
async fn questions_his_own_keys_opened_raise_no_notification_and_allow_questions_go_ahead_of_them() {
	let _j = temp_jocasta();
	let (herdr, _, _) = boss_session().await;
	let ask = AskPopups::new(herdr.clone(), ask_options(60_000, 60_000));
	let popups = Popups::new();
	let first = popups.open(&finish_view("acme-api/retry", "d", Some("ABC")), true);
	ask.show(&first);
	until(|| shown_code(&herdr).as_deref() == Some(first.code())).await;
	let key = popups.open(&finish_view("acme-api/other", "d", Some("DEF")), true);
	let allow = popups.open(&allow_view("Merge it?", "GHJ"), false);
	ask.show(&key);
	ask.show(&allow);
	first.close(None);
	// The Allow question, though asked after the key's, shows next, with its notification.
	until(|| shown_code(&herdr).as_deref() == Some(allow.code())).await;
	assert_eq!(
		herdr.of("notify"),
		vec![vec![json!({"title": "Jocasta asks", "body": "Merge it?", "sound": "request"})]]
	);
	allow.close(None);
	until(|| shown_code(&herdr).as_deref() == Some(key.code())).await;
	assert_eq!(herdr.of("notify").len(), 1);
	key.close(None);
}

#[tokio::test]
async fn a_question_asked_again_goes_first_among_the_waiting_and_the_one_shown_stays() {
	let _j = temp_jocasta();
	let (herdr, _, _) = boss_session().await;
	let ask = AskPopups::new(herdr.clone(), ask_options(60_000, 60_000));
	let popups = Popups::new();
	let a = popups.open(&allow_view("a", "AAA"), false);
	let b = popups.open(&allow_view("b", "BBB"), false);
	let c = popups.open(&allow_view("c", "CCC"), false);
	for popup in [&a, &b, &c] {
		ask.show(popup);
	}
	until(|| shown_code(&herdr).as_deref() == Some(a.code())).await;
	// The one shown now stays as it is; the others reorder.
	ask.front(&a);
	ask.front(&c);
	a.close(None);
	until(|| shown_code(&herdr).as_deref() == Some(c.code())).await;
	c.close(None);
	until(|| shown_code(&herdr).as_deref() == Some(b.code())).await;
	b.close(None);
}

#[tokio::test]
async fn a_question_its_popup_read_but_never_held_counts_as_closed_and_is_not_opened_again() {
	let _j = temp_jocasta();
	let (herdr, _, _) = boss_session().await;
	let ask = AskPopups::new(herdr.clone(), ask_options(20, 20));
	let popups = Popups::new();
	let popup = popups.open(&allow_question("m", "d", None), false);
	ask.show(&popup);
	until(|| herdr.state().popup.is_some()).await;
	assert!(matches!(popups.read(popup.code()), Some(PopupRead::First { .. })));
	assert_eq!(popup.how().await, PopupEnd::Closed);
	assert!(herdr.of("openPluginPane").len() <= 2);
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
