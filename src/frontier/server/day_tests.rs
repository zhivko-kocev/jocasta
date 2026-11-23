//! Tests of `day.rs`: the stand-in hooks the sibling test files share, and the cases of `test/frontier/server/day.test.ts`
//! aimed at `Day`. The cases that go through the service (herdr, `/api/mcp`, the page's websocket) wait for it; the
//! ones here that look like them (delivery, disk, `today`, the dashboard, a blocked thread's reminder, the worklog
//! refusal) are their `Day` halves, run with the hooks recorded.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{Local, TimeZone, Utc};
use futures_util::future::BoxFuture;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::frontier::protocol::{ActivitySummary, CalendarEntry, NeedsYou, NeedsYouKind, NextItem, PrFacts, Today};
use crate::shared::events::queue::EventSource;
use crate::shared::schema::open_catalog;
use crate::shared::settings::settings_file;
use crate::shared::testing::temp_jocasta;

/// `new Date(y, m - 1, d, h, mi)`: local time, epoch ms. Months here are 1-based.
pub(super) fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
	Local.with_ymd_and_hms(y, m, d, h, mi, 0).single().expect("a local time").timestamp_millis()
}

pub(super) fn project(id: &str, name: &str) -> Project {
	Project {
		id: id.to_owned(),
		identity: format!("root:{id}"),
		name: name.to_owned(),
		main_path: PathBuf::from(format!("/code/{name}")),
		git: true,
		paths: Vec::new(),
	}
}

/// A default thread, like day-threads.test.ts's `thread`.
pub(super) fn thread(change: impl FnOnce(&mut DayThread)) -> DayThread {
	let mut thread = DayThread {
		pane_id: "w1:p1".to_owned(),
		title: "t".to_owned(),
		project: Some("acme-api".to_owned()),
		line: String::new(),
		overlaps: None,
		state: ThreadState::Working,
		changed_at: 0,
		reviewed_at: None,
		cost: None,
		context: None,
		diff: None,
		check: None,
		last_write_ms: None,
	};
	change(&mut thread);
	thread
}

type Sessions = Arc<dyn Fn(&str) -> HashMap<String, Vec<WorklogSession>> + Send + Sync>;
pub(super) type Review = Arc<dyn Fn(&Project) -> Option<String> + Send + Sync>;
pub(super) type StartAnswer = Arc<dyn Fn(&str, &Schedule) -> Result<ScheduleStart> + Send + Sync>;

/// The hooks of a stand: what the service would be handed, recorded, and what it would answer, settable.
pub(super) struct Shared {
	pub sent: Mutex<Vec<ServerMessage>>,
	pub alerts: Mutex<Vec<(String, AlertDetails)>>,
	pub notes: Mutex<Vec<String>>,
	pub threads: Mutex<Vec<DayThread>>,
	pub leftovers: Mutex<Vec<Leftover>>,
	pub reviews: Mutex<Vec<String>>,
	pub review: Mutex<Review>,
	pub sessions: Mutex<Sessions>,
	pub projects: Vec<Project>,
	/// What the PR cache answers, by url.
	pub prs: Mutex<HashMap<String, PrFacts>>,
	/// The scheduled starts asked for: project name and brief.
	pub starts: Mutex<Vec<(String, String)>>,
	/// What starting a scheduled thread answers.
	pub start: Mutex<StartAnswer>,
}

impl DayHooks for Shared {
	fn send(&self, message: ServerMessage) {
		self.sent.lock().unwrap().push(message);
	}

	fn alert(&self, text: &str, details: AlertDetails) {
		self.alerts.lock().unwrap().push((text.to_owned(), details));
	}

	fn notify(&self, text: &str, _level: ToastLevel) {
		self.notes.lock().unwrap().push(text.to_owned());
	}

	fn projects(&self) -> Vec<Project> {
		self.projects.clone()
	}

	fn threads(&self) -> Vec<DayThread> {
		self.threads.lock().unwrap().clone()
	}

	fn sessions_by_project(&self, day: &str) -> HashMap<String, Vec<WorklogSession>> {
		(self.sessions.lock().unwrap().clone())(day)
	}

	fn prepare_sessions(&self, _day: &str) -> BoxFuture<'_, Result<()>> {
		Box::pin(async { Ok(()) })
	}

	fn services(&self) -> BoxFuture<'_, Result<Vec<ServiceHealth>>> {
		Box::pin(async { Ok(Vec::new()) })
	}

	fn project_name(&self, cwd: &Path) -> Option<String> {
		self.projects.iter().find(|p| p.main_path == cwd).map(|p| p.name.clone())
	}

	fn leftovers(&self) -> BoxFuture<'_, Result<Vec<Leftover>>> {
		let leftovers = self.leftovers.lock().unwrap().clone();
		Box::pin(async { Ok(leftovers) })
	}

	fn weekly_review(&self, project: &Project) -> BoxFuture<'_, Result<Option<String>>> {
		self.reviews.lock().unwrap().push(project.name.clone());
		let line = (self.review.lock().unwrap().clone())(project);
		Box::pin(async { Ok(line) })
	}

	fn pr_facts(&self, url: &str) -> Option<PrFacts> {
		self.prs.lock().unwrap().get(url).cloned()
	}

	fn start_scheduled(&self, project: &str, schedule: &Schedule) -> BoxFuture<'static, Result<ScheduleStart>> {
		self.starts.lock().unwrap().push((project.to_owned(), schedule.brief.clone()));
		let answer = (self.start.lock().unwrap().clone())(project, schedule);
		Box::pin(std::future::ready(answer))
	}
}

/// A Day on a fake clock with stand-in hooks.
pub(super) struct Stand {
	pub day: Day,
	pub shared: Arc<Shared>,
	pub catalog: SharedCatalog,
	clock: Arc<AtomicI64>,
}

impl Stand {
	/// A day at `time` with one project, `acme-api`, the pollers off and the disk half free.
	pub fn new(time: i64) -> Self {
		Self::with(time, vec![project("p1", "acme-api")], DayOptions { pollers: Some(false), ..DayOptions::default() })
	}

	pub fn in_projects(time: i64, projects: Vec<Project>) -> Self {
		Self::with(time, projects, DayOptions { pollers: Some(false), ..DayOptions::default() })
	}

	pub fn with(time: i64, projects: Vec<Project>, mut options: DayOptions) -> Self {
		let clock = Arc::new(AtomicI64::new(time));
		let now = Arc::clone(&clock);
		options.now = Some(Arc::new(move || now.load(Ordering::SeqCst)));
		options.disk_free.get_or_insert_with(|| Arc::new(|| 0.5));
		let shared = Arc::new(Shared {
			sent: Mutex::new(Vec::new()),
			alerts: Mutex::new(Vec::new()),
			notes: Mutex::new(Vec::new()),
			threads: Mutex::new(Vec::new()),
			leftovers: Mutex::new(Vec::new()),
			reviews: Mutex::new(Vec::new()),
			review: Mutex::new(Arc::new(|_| None)),
			sessions: Mutex::new(Arc::new(|_| HashMap::new())),
			projects,
			prs: Mutex::new(HashMap::new()),
			starts: Mutex::new(Vec::new()),
			start: Mutex::new(Arc::new(|_, _| Ok(ScheduleStart::Started))),
		});
		let catalog: SharedCatalog = Arc::new(Mutex::new(open_catalog().expect("the catalog")));
		let day = Day::new(Arc::clone(&catalog), Arc::clone(&shared) as Arc<dyn DayHooks>, options).expect("a day");
		Self { day, shared, catalog, clock }
	}

	pub fn set(&self, time: i64) {
		self.clock.store(time, Ordering::SeqCst);
	}

	pub fn add(&self, ms: i64) {
		self.clock.fetch_add(ms, Ordering::SeqCst);
	}

	pub fn now(&self) -> i64 {
		self.clock.load(Ordering::SeqCst)
	}

	pub fn alerts(&self) -> Vec<String> {
		self.shared.alerts.lock().unwrap().iter().map(|(text, _)| text.clone()).collect()
	}

	pub fn sent(&self) -> Vec<ServerMessage> {
		self.shared.sent.lock().unwrap().clone()
	}

	pub fn toasts(&self) -> Vec<ServerMessage> {
		self.sent().into_iter().filter(|m| matches!(m, ServerMessage::Toast { .. })).collect()
	}

	pub fn notes(&self) -> Vec<String> {
		self.shared.notes.lock().unwrap().clone()
	}
}

/// Lets the tasks the day spawned run (`await Bun.sleep(0)`).
pub(super) async fn settle() {
	for _ in 0..20 {
		tokio::task::yield_now().await;
	}
}

/// Writes `jocasta.yaml` (JSON is YAML).
pub(super) fn write_settings(text: &str) {
	let file = settings_file();
	std::fs::create_dir_all(file.parent().expect("a folder")).expect("the config folder");
	std::fs::write(file, text).expect("jocasta.yaml");
}

/// An in-process HTTP server answering each request target through `answer` (status, body), recording each target.
pub(super) async fn serve(
	answer: impl Fn(&str) -> (u16, String) + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listening");
	let base = format!("http://{}", listener.local_addr().expect("its address"));
	let asked = Arc::new(Mutex::new(Vec::new()));
	let seen = Arc::clone(&asked);
	tokio::spawn(async move {
		while let Ok((mut stream, _)) = listener.accept().await {
			let mut head = Vec::new();
			let mut chunk = [0u8; 1024];
			while !head.windows(4).any(|window| window == b"\r\n\r\n") {
				match stream.read(&mut chunk).await {
					Ok(0) | Err(_) => break,
					Ok(n) => head.extend_from_slice(&chunk[..n]),
				}
			}
			let text = String::from_utf8_lossy(&head).into_owned();
			let target = text.split_whitespace().nth(1).unwrap_or("").to_owned();
			seen.lock().unwrap().push(target.clone());
			let (status, body) = answer(&target);
			let reply = format!(
				"HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
				body.len()
			);
			let _ = stream.write_all(reply.as_bytes()).await;
			let _ = stream.shutdown().await;
		}
	});
	(base, asked)
}

fn urgent(source_id: &str, kind: &str, text: &str) -> NewEvent {
	NewEvent {
		source_id: source_id.to_owned(),
		source: EventSource::Github,
		kind: kind.to_owned(),
		priority: EventPriority::Urgent,
		text: text.to_owned(),
		url: None,
		project_id: None,
		thread_id: None,
	}
}

fn normal(source_id: &str, kind: &str, text: &str) -> NewEvent {
	NewEvent { priority: EventPriority::Normal, ..urgent(source_id, kind, text) }
}

// `day.test.ts` "delivery (0299, 0405, 0554)": the `Day` half, without herdr's notifications or the boss's pane.
#[tokio::test]
async fn delivery_gives_each_event_an_alert_and_a_toast_one_reminder_after_30_minutes_and_quiet_hours_hold_normal() {
	let _j = temp_jocasta();
	write_settings(r#"{"service":{"quietHours":"22:00-08:00"}}"#);
	let s = Stand::new(at(2026, 10, 8, 10, 0));
	let day = &s.day;

	let mut pipeline = urgent("github:pipeline:acme/web#7", "pipeline", "Pipeline red — web on main");
	pipeline.url = Some("https://example.test/run/1".to_owned());
	day.raise(&pipeline).unwrap();
	{
		let alerts = s.shared.alerts.lock().unwrap();
		// What a poller said is marked as outside words for the boss (SEC-2).
		assert_eq!(alerts[0].0, "Pipeline red — web on main");
		assert_eq!(
			alerts[0].1,
			AlertDetails {
				priority: ToastPriority::Urgent,
				url: Some("https://example.test/run/1".to_owned()),
				reminder: false,
				outside: vec!["web on main".to_owned()],
			}
		);
	}
	assert_eq!(
		s.toasts(),
		vec![ServerMessage::Toast {
			reason: ToastReason::Event,
			level: ToastLevel::Warning,
			text: "Pipeline red — web on main".to_owned(),
			priority: Some(ToastPriority::Urgent),
			pane_id: None,
			url: Some("https://example.test/run/1".to_owned()),
		}]
	);
	// The same source id again is not raised twice (0298).
	assert!(day.raise(&urgent("github:pipeline:acme/web#7", "pipeline", "x")).unwrap().is_none());

	day.raise(&normal("github:review:acme/api#42", "review", "Review requested — Add export")).unwrap();
	assert_eq!(s.alerts()[1], "Review requested — Add export");
	assert_eq!(s.shared.alerts.lock().unwrap()[1].1.outside, vec!["Add export".to_owned()]);
	// Low goes only to `today`.
	let mut low = normal("x:low", "other", "fyi");
	low.priority = EventPriority::Low;
	day.raise(&low).unwrap();
	assert_eq!(s.alerts().len(), 2);

	// 30 minutes later, both are still open: one reminder each, and only one.
	s.add(30 * 60_000);
	day.tick().unwrap();
	assert_eq!(
		&s.alerts()[2..],
		["Still open: Pipeline red — web on main", "Still open: Review requested — Add export"]
	);
	assert!(s.shared.alerts.lock().unwrap()[2].1.reminder);
	s.add(30 * 60_000);
	day.tick().unwrap();
	day.raise(&normal("test:sentinel", "other", "sentinel")).unwrap();
	assert_eq!(s.alerts().len(), 5);
	assert_eq!(s.alerts()[4], "sentinel");

	// Quiet hours: normal waits for the morning; urgent goes through.
	s.set(at(2026, 10, 8, 22, 30));
	day.tick().unwrap();
	assert!(day.quiet());
	day.raise(&normal("github:review:acme/api#43", "review", "late")).unwrap();
	day.raise(&urgent("machine:test", "disk", "Disk almost full — 5% free")).unwrap();
	assert_eq!(s.alerts().len(), 6);
	assert_eq!(s.alerts()[5], "Disk almost full — 5% free");
}

#[tokio::test]
async fn disk_under_10_percent_free_is_urgent_once_and_resolved_when_space_is_back() {
	let _j = temp_jocasta();
	let free = Arc::new(Mutex::new(0.05));
	let seen = Arc::clone(&free);
	let s = Stand::with(
		at(2026, 10, 8, 10, 0),
		vec![project("p1", "acme-api")],
		DayOptions {
			pollers: Some(false),
			disk_free: Some(Arc::new(move || *seen.lock().unwrap())),
			..DayOptions::default()
		},
	);
	s.day.tick().unwrap();
	s.day.tick().unwrap();
	assert_eq!(s.alerts(), ["Disk almost full — 5% free"]);
	*free.lock().unwrap() = 0.5;
	s.day.tick().unwrap();
	assert!(s.day.queue(|q| q.by_source("machine:disk")).unwrap().unwrap().resolved_at.is_some());
}

#[tokio::test]
async fn a_blocked_threads_question_stays_marked_as_outside_in_its_reminder_and_jocastas_own_tails_dont() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 10, 8, 10, 0));
	s.day.thread_blocked("p1", "acme/x — Run rm -rf now?", None).unwrap();
	s.day.thread_blocked("p2", "acme/y — awaiting you", None).unwrap();
	s.add(30 * 60_000);
	s.day.tick().unwrap();
	let alerts = s.shared.alerts.lock().unwrap().clone();
	assert_eq!(
		alerts.iter().map(|(text, details)| (text.as_str(), details.outside.clone())).collect::<Vec<_>>(),
		[
			("Still open: acme/x — Run rm -rf now?", vec!["Run rm -rf now?".to_owned()]),
			("Still open: acme/y — awaiting you", Vec::new()),
		]
	);
	s.day.thread_unblocked("p1").unwrap();
	assert!(s.day.queue(|q| q.by_source("thread:blocked:p1")).unwrap().unwrap().resolved_at.is_some());
}

/// A feed whose standup is at 10:00 local every day, 15 minutes, with a Meet link in its description.
fn feed() -> String {
	let start = Local.with_ymd_and_hms(2026, 10, 8, 10, 0, 0).single().unwrap().with_timezone(&Utc);
	[
		"BEGIN:VCALENDAR".to_owned(),
		"VERSION:2.0".to_owned(),
		"BEGIN:VEVENT".to_owned(),
		"UID:standup@test".to_owned(),
		"SUMMARY:standup (client-a)".to_owned(),
		format!("DTSTART:{}", start.format("%Y%m%dT%H%M%SZ")),
		"DURATION:PT15M".to_owned(),
		"RRULE:FREQ=DAILY".to_owned(),
		"LOCATION:Google Meet".to_owned(),
		"DESCRIPTION:Join with Google Meet: https://meet.google.com/abc-defg-hij".to_owned(),
		"END:VEVENT".to_owned(),
		"END:VCALENDAR".to_owned(),
		String::new(),
	]
	.join("\r\n")
}

// `day.test.ts` "today (0300, 0409)": the `Day` half, over the calendar fetch seam instead of a page.
#[tokio::test]
async fn today_has_the_calendar_with_links_awaiting_you_and_next_and_a_meeting_in_5_minutes_is_urgent() {
	let _j = temp_jocasta();
	let demo = project("p9", "demo");
	let body = feed();
	let s = Stand::with(
		at(2026, 10, 8, 9, 0),
		vec![demo.clone()],
		DayOptions {
			pollers: Some(false),
			calendar_secret: Some(Arc::new(|| {
				Some("https://calendar.example.test/private-abc123/basic.ics".to_owned())
			})),
			calendar_fetch: Some(Arc::new(move |_| {
				let body = body.clone();
				Box::pin(async move { Some(Fetched { status: 200, body }) })
			})),
			..DayOptions::default()
		},
	);
	let day = &s.day;
	day.poll(PollerName::Calendar).await;
	let mut review = normal("github:review:acme/api#42", "review", "Review requested — Add export (acme/api #42)");
	review.url = Some("https://github.com/acme/api/pull/42".to_owned());
	day.raise(&review).unwrap();
	day.remind("call the bank", "15:00", None).unwrap();
	let note = day.note("try the new parser", &demo).unwrap();

	assert_eq!(
		day.today().unwrap(),
		Today {
			date: "2026-10-08".to_owned(),
			calendar: vec![CalendarEntry {
				time: "10:00".to_owned(),
				title: "standup (client-a)".to_owned(),
				url: Some("https://meet.google.com/abc-defg-hij".to_owned()),
				end: Some("10:15".to_owned()),
			}],
			needs_you: vec![NeedsYou {
				url: Some("https://github.com/acme/api/pull/42".to_owned()),
				..needs_you(NeedsYouKind::Review, "Review requested — Add export (acme/api #42)")
			}],
			landed: vec![],
			underway: vec![],
			next: vec![NextItem {
				id: note.id,
				text: "try the new parser".to_owned(),
				project: Some("demo".to_owned())
			}],
			scheduled: vec![],
		}
	);

	// 09:56: the standup is 4 minutes out, so it is urgent, once.
	s.set(at(2026, 10, 8, 9, 56));
	day.tick().unwrap();
	let soon = "Meeting in 4 min — standup (client-a)";
	{
		let alerts = s.shared.alerts.lock().unwrap();
		assert!(alerts.iter().any(|(text, details)| text == soon
			&& details.url.as_deref() == Some("https://meet.google.com/abc-defg-hij")
			&& details.outside == ["standup (client-a)"]));
	}
	day.tick().unwrap();
	s.set(at(2026, 10, 8, 10, 1));
	day.tick().unwrap();
	assert_eq!(s.alerts().iter().filter(|l| *l == soon).count(), 1);
	assert!(!day.queue(|q| q.open()).unwrap().iter().any(|r| r.kind == "meeting-soon"));

	// The boss's text form follows 0300's layout.
	assert_eq!(
		day.today_text().unwrap().split('\n').collect::<Vec<_>>(),
		[
			"TODAY · Thu 8 Oct".to_owned(),
			"Calendar     10:00 standup (client-a)".to_owned(),
			"Awaiting you Review requested — Add export (acme/api #42)".to_owned(),
			"Landed       nothing".to_owned(),
			"Underway     nothing".to_owned(),
			format!("Next         demo: try the new parser (#{})", note.id),
		]
	);
}

/// The headline and weather answers of `day.test.ts`'s `dashboardFetch`, on a local server.
fn dashboard_answer(news_down: &AtomicBool, target: &str) -> (u16, String) {
	let (path, query) = target.split_once('?').unwrap_or((target, ""));
	let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")));
	match path {
		"/news" if news_down.load(Ordering::SeqCst) => (503, "busy".to_owned()),
		"/news" => {
			let feed = param("source").or_else(|| param("topic")).unwrap_or("");
			let body = serde_json::json!([{
				"title": format!("{feed} one"),
				"url": format!("https://example.test/{feed}/{}", param("limit").unwrap_or("all")),
				"source": if feed == "gaming" { "pcgamer" } else { feed },
				"published_at": "2026-10-08T08:00:00Z",
			}]);
			(200, body.to_string())
		}
		"/geo" => (
			200,
			serde_json::json!({"results": [{"name": "Example City", "latitude": 12.3, "longitude": 56.7, "country": "Example Country"}]})
				.to_string(),
		),
		"/forecast" => (
			200,
			serde_json::json!({
				"utc_offset_seconds": 10800,
				"current": {"time": "2026-10-08T12:00", "temperature_2m": 14, "weather_code": 61},
				"daily": {"temperature_2m_max": [16], "temperature_2m_min": [8]},
			})
			.to_string(),
		),
		_ => (404, String::new()),
	}
}

// `day.test.ts` "the dashboard (0429, 0442)": the `Day` half.
#[tokio::test]
async fn the_dashboard_has_news_and_weather_a_news_failure_is_said_once_and_the_last_list_stays_and_todays_activity() {
	let _j = temp_jocasta();
	write_settings("{}");
	let down = Arc::new(AtomicBool::new(false));
	let seen = Arc::clone(&down);
	let (base, _) = serve(move |target| dashboard_answer(&seen, target)).await;
	let demo = project("p9", "demo");
	let s = Stand::with(
		at(2026, 10, 8, 9, 0),
		vec![demo.clone()],
		DayOptions {
			pollers: Some(false),
			news_endpoint: Some(format!("{base}/news")),
			geocoding_endpoint: Some(format!("{base}/geo")),
			forecast_endpoint: Some(format!("{base}/forecast")),
			..DayOptions::default()
		},
	);
	let day = &s.day;
	assert!(day.dashboard().weather.is_none());
	assert!(day.dashboard().news.is_empty());

	day.poll(PollerName::News).await;
	let heading = |name: &str, source: &str, url: &str| NewsList {
		heading: name.to_owned(),
		items: vec![crate::frontier::protocol::Headline {
			title: format!("{source} one"),
			source: source.to_owned(),
			url: url.to_owned(),
			at: "2026-10-08T08:00:00.000Z".to_owned(),
		}],
	};
	let mut gaming = heading("Gaming", "gaming", "https://example.test/gaming/10");
	gaming.items[0].source = "pcgamer".to_owned();
	// 0442's default feeds: every Hacker News and LWN item (no limit asked), the newest 10 gaming headlines.
	assert_eq!(
		day.dashboard().news,
		vec![
			heading("Hacker News", "hn", "https://example.test/hn/all"),
			heading("LWN", "lwn", "https://example.test/lwn/all"),
			gaming,
		]
	);
	// Weather stays hidden until dashboard.weather is set.
	day.poll(PollerName::Weather).await;
	assert!(day.dashboard().weather.is_none());
	write_settings(r#"{"dashboard":{"weather":"Example City"}}"#);
	day.poll(PollerName::Weather).await;
	assert_eq!(
		day.dashboard().weather,
		Some(Weather {
			place: "Example City, Example Country".to_owned(),
			temp_c: 14.0,
			code: 61,
			summary: "Light rain".to_owned(),
			high_c: 16.0,
			low_c: 8.0,
			at: "2026-10-08T09:00:00.000Z".to_owned(),
		})
	);

	down.store(true, Ordering::SeqCst);
	day.poll(PollerName::News).await;
	day.poll(PollerName::News).await;
	let failed = day.dashboard();
	let said = "News (Hacker News): brutalist.report answered HTTP 503";
	assert_eq!(failed.news_error.as_deref(), Some(said));
	assert_eq!(failed.news.iter().map(|n| n.items.len()).collect::<Vec<_>>(), [1, 1, 1]);
	assert_eq!(s.notes().iter().filter(|note| *note == said).count(), 1);
	// Never an [event].
	assert!(s.alerts().iter().all(|line| !line.contains("hn one")));

	// Today's activity: the latest delivered events and the summaries saved today.
	day.raise(&normal("x:1", "review", "Review requested — A")).unwrap();
	// A summary from a project's folder names its project; its first line is content, not a heading (N-4).
	s.catalog
		.lock()
		.unwrap()
		.execute(
			"INSERT INTO summaries (session_file, entry_id, kind, cwd, at, summary) VALUES (?, ?, ?, ?, ?, ?)",
			rusqlite::params!["s.jsonl", "e1", "compact", "/code/demo", s.now(), "## Goal\nShip the parser"],
		)
		.unwrap();
	// The service's own thread lines join the queue's (N-4).
	day.record_event("demo/Docs — done and reviewed: 1 new commit, 0 notes, tests pass");
	day.tick().unwrap();
	let activity = day.dashboard().activity;
	let mut texts: Vec<_> = activity.events.iter().map(|e| e.text.clone()).collect();
	texts.sort();
	assert_eq!(texts, ["Review requested — A", "demo/Docs — done and reviewed: 1 new commit, 0 notes, tests pass"]);
	assert_eq!(
		activity.summaries,
		vec![ActivitySummary {
			at: iso(s.now()),
			project: Some("demo".to_owned()),
			text: "Ship the parser".to_owned()
		}]
	);
	// The page heard of the dashboard when it changed.
	assert!(s.sent().iter().any(|m| matches!(m, ServerMessage::Dashboard { dashboard } if dashboard.news.len() == 3)));
}

#[tokio::test]
async fn the_worklog_tool_refuses_a_day_not_on_the_calendar() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 10, 8, 9, 0));
	let refused = s.day.worklog(Some("2026-02-30"), None).await.unwrap_err();
	// The service says it as `worklog: no such day: 2026-02-30` (CO-22).
	assert_eq!(refused.to_string(), "no such day: 2026-02-30");
	let refused = s.day.worklog(Some("soon"), None).await.unwrap_err();
	assert_eq!(refused.to_string(), "\"soon\" is not a day: use YYYY-MM-DD");
}

#[tokio::test]
async fn a_reminder_refuses_impossible_times_days_off_the_calendar_and_moments_past_f_11() {
	let _j = temp_jocasta();
	let s = Stand::new(at(2026, 10, 8, 9, 0));
	let first = s.day.remind("call the bank", "15:00", None).unwrap();
	assert_eq!(first.text, "call the bank");
	assert_eq!(first.due_at.as_deref(), Some(iso(at(2026, 10, 8, 15, 0)).as_str()));
	assert!(s.day.remind("x", "soon", None).unwrap_err().to_string().contains("\"soon\" is not a time"));
	for (when, why) in [
		("25:00", "hours run 00–23"),
		("12:75", "minutes 00–59"),
		("2026-02-30 10:00", "not a day on the calendar"),
		("2026-10-01 09:00", "has already passed"),
		("2026-10-07T08:00:00Z", "has already passed"),
	] {
		let refused = s.day.remind("x", when, None).unwrap_err().to_string();
		assert!(refused.contains(why), "{when}: {refused}");
	}
	assert_eq!(s.day.remind("  ", "15:00", None).unwrap_err().to_string(), "a reminder needs its text");
	// A time that passed today is tomorrow's.
	let tomorrow = s.day.remind("stretch", "08:00", None).unwrap();
	assert_eq!(tomorrow.due_at.as_deref(), Some(iso(at(2026, 10, 9, 8, 0)).as_str()));
	// Not due yet: not awaiting him (N-5); `done` resolves it.
	assert!(!s.day.today_text().unwrap().contains("call the bank"));
	assert!(s.day.done(first.id).unwrap());
	assert!(!s.day.done(first.id).unwrap());
	// A due one reads as a reminder with its time (UX-05).
	let second = s.day.remind("send the invoice", "16:07", None).unwrap();
	s.set(at(2026, 10, 8, 16, 7));
	s.day.tick().unwrap();
	assert_eq!(s.alerts(), ["Reminder (16:07) — send the invoice"]);
	assert!(s.day.today_text().unwrap().contains("16:07 send the invoice"));
	assert!(s.day.done(second.id).unwrap());
}

/// A bare Awaiting-you row.
pub(super) fn needs_you(kind: NeedsYouKind, text: &str) -> NeedsYou {
	NeedsYou {
		kind,
		text: text.to_owned(),
		url: None,
		pane_id: None,
		project: None,
		time: None,
		since: None,
		overlaps: None,
	}
}
