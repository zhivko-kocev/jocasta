//! Tests of the day's briefs (0680): the morning brief's schedule and key, the rows of `today` that open the briefs, and
//! the work graph.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{Local, TimeZone};

use super::*;
use crate::frontier::brief::BriefLinks;
use crate::frontier::brief::model::Section;
use crate::frontier::server::context::{TestContext, test_context};
use crate::frontier::server::day::DayOptions;
use crate::frontier::server::pending::{After, PendingArgs};

fn at(y: i32, month: u32, d: u32, h: u32, m: u32) -> i64 {
	Local.with_ymd_and_hms(y, month, d, h, m, 0).single().expect("a local time").timestamp_millis()
}

/// The page server's stand-in: links by id, and the ids opened.
#[derive(Default)]
struct FakeLinks {
	opened: Mutex<Vec<String>>,
}

impl BriefLinks for FakeLinks {
	fn url(&self, id: &str) -> String {
		format!("http://127.0.0.1:1/brief/{id}")
	}

	fn open(&self, id: &str) {
		self.opened.lock().unwrap_or_else(PoisonError::into_inner).push(id.to_owned());
	}

	fn open_board(&self) {
		self.opened.lock().unwrap_or_else(PoisonError::into_inner).push("board".to_owned());
	}
}

fn opened(links: &FakeLinks) -> Vec<String> {
	links.opened.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// A context whose day clock is `clock`; automatic briefs `auto`; page links `links`.
fn world(clock: &Arc<AtomicI64>, links: Option<&Arc<FakeLinks>>, auto: bool) -> TestContext {
	let (clock, links) = (Arc::clone(clock), links.map(Arc::clone));
	test_context(move |options| {
		options.day = Some(DayOptions {
			pollers: Some(false),
			disk_free: Some(Arc::new(|| 0.5)),
			now: Some(Arc::new(move || clock.load(Ordering::SeqCst))),
			..DayOptions::default()
		});
		options.briefs = links.map(|links| links as Arc<dyn BriefLinks>);
		options.auto_briefs = Some(auto);
	})
}

#[test]
fn ids_name_the_local_day_and_the_iso_week() {
	assert_eq!(morning_id(at(2026, 11, 24, 7, 0)), "morning-2026-11-24");
	// Monday 2026-11-16 starts ISO week 47; the Sunday before it is week 46, the one after it still 47.
	assert_eq!(weekly_id_of("2026-11-16").as_deref(), Some("weekly-2026-W47"));
	assert_eq!(weekly_id(at(2026, 11, 22, 23, 0)), "weekly-2026-W47");
	assert_eq!(weekly_id(at(2026, 11, 15, 23, 0)), "weekly-2026-W46");
	assert_eq!(weekly_id_of("not a date"), None);
}

#[tokio::test]
async fn the_morning_brief_is_built_by_the_first_tick_after_six_once_a_day_and_nothing_opens() {
	let clock = Arc::new(AtomicI64::new(at(2026, 11, 24, 5, 59)));
	let links = Arc::new(FakeLinks::default());
	let t = world(&clock, Some(&links), true);
	let id = "morning-2026-11-24";
	// Before 06:00 nothing is built.
	morning_if_due(&t.ctx).await.unwrap();
	assert!(!store::exists(id));
	assert!(super::links(&t.ctx, clock.load(Ordering::SeqCst)).is_empty());
	// The first tick after it builds the brief, and `today` lists it; the page is not opened.
	clock.store(at(2026, 11, 24, 6, 1), Ordering::SeqCst);
	morning_if_due(&t.ctx).await.unwrap();
	let built = store::load(id).expect("the brief is stored");
	assert_eq!((built.kind.as_str(), built.title.as_str()), ("morning", "Morning brief"));
	assert_eq!(
		super::links(&t.ctx, clock.load(Ordering::SeqCst)),
		[BriefLink { label: "morning brief".to_owned(), url: format!("http://127.0.0.1:1/brief/{id}") }]
	);
	assert!(opened(&links).is_empty());
	// A later tick of the same day leaves it as it is.
	clock.store(at(2026, 11, 24, 9, 0), Ordering::SeqCst);
	morning_if_due(&t.ctx).await.unwrap();
	assert_eq!(store::load(id).unwrap().generated_at_ms, built.generated_at_ms);
	// The next day has none yet, so its first tick builds its own.
	clock.store(at(2026, 11, 25, 6, 5), Ordering::SeqCst);
	assert!(super::links(&t.ctx, clock.load(Ordering::SeqCst)).is_empty());
	morning_if_due(&t.ctx).await.unwrap();
	assert!(store::exists("morning-2026-11-25"));
}

#[tokio::test]
async fn with_automatic_briefs_off_the_tick_builds_none_but_his_key_still_builds_and_opens_one() {
	let clock = Arc::new(AtomicI64::new(at(2026, 11, 24, 8, 0)));
	let links = Arc::new(FakeLinks::default());
	let t = world(&clock, Some(&links), false);
	morning_if_due(&t.ctx).await.unwrap();
	assert!(!store::exists("morning-2026-11-24"));
	assert_eq!(open_morning(&t.ctx).await.unwrap(), "Morning brief opened.");
	assert!(store::exists("morning-2026-11-24"));
	assert_eq!(opened(&links), ["morning-2026-11-24"]);
	// The key builds it again from what the day holds now.
	let first = store::load("morning-2026-11-24").unwrap().generated_at_ms;
	clock.store(at(2026, 11, 24, 8, 30), Ordering::SeqCst);
	open_morning(&t.ctx).await.unwrap();
	assert!(store::load("morning-2026-11-24").unwrap().generated_at_ms > first);
	assert_eq!(opened(&links).len(), 2);
}

#[tokio::test]
async fn without_a_page_server_there_are_no_rows_and_the_key_only_builds() {
	let clock = Arc::new(AtomicI64::new(at(2026, 11, 24, 8, 0)));
	let t = world(&clock, None, false);
	assert_eq!(open_morning(&t.ctx).await.unwrap(), "Morning brief built.");
	assert!(super::links(&t.ctx, clock.load(Ordering::SeqCst)).is_empty());
}

#[tokio::test]
async fn the_weekly_brief_has_a_row_for_its_week_once_stored() {
	let clock = Arc::new(AtomicI64::new(at(2026, 11, 18, 11, 0)));
	let links_of = Arc::new(FakeLinks::default());
	let t = world(&clock, Some(&links_of), false);
	let brief = crate::frontier::brief::model::Brief {
		id: "weekly-2026-W47".to_owned(),
		kind: "weekly".to_owned(),
		title: "Weekly brief".to_owned(),
		subtitle: String::new(),
		generated_at_ms: 1,
		sections: vec![Section::Text { heading: None, body: "x".to_owned() }],
	};
	store::save(&brief).unwrap();
	let rows = super::links(&t.ctx, clock.load(Ordering::SeqCst));
	assert_eq!(rows.iter().map(|row| row.label.as_str()).collect::<Vec<_>>(), ["weekly brief"]);
	// The week after has none.
	assert!(links(&t.ctx, at(2026, 11, 25, 11, 0)).is_empty());
}

#[tokio::test]
async fn the_work_graph_has_a_box_for_each_open_thread_and_waiting_start_and_an_arrow_for_each_after() {
	let clock = Arc::new(AtomicI64::new(at(2026, 11, 24, 8, 0)));
	let t = world(&clock, None, false);
	t.with_thread("1-1", "Readme line");
	let args = PendingArgs {
		project: "demo".to_owned(),
		title: "Docs pass".to_owned(),
		brief: "Do more.".to_owned(),
		done_when: Vec::new(),
		heavy: false,
		budget: None,
		door: "boss".to_owned(),
		start_at: None,
		pr: None,
	};
	let after = vec![After { thread: "pane:1-1".to_owned(), name: "Readme line".to_owned(), met: false, pr: None }];
	t.ctx.pending.enqueue("p-demo", args, after, 1).unwrap();
	let today = t.ctx.day.today().unwrap();
	let (nodes, edges) = work_graph(&t.ctx, &today);
	let labels: Vec<&str> = nodes.iter().map(|node| node.label.as_str()).collect();
	assert_eq!(labels, ["demo/Readme line", "Docs pass"]);
	assert_eq!(edges.len(), 1);
	assert_eq!((edges[0].from.as_str(), edges[0].to.as_str()), ("pane:1-1", nodes[1].id.as_str()));
	// The morning brief draws it as a flow.
	build_morning(&t.ctx).await.unwrap();
	let brief = store::load("morning-2026-11-24").unwrap();
	let Section::Flow { nodes, .. } = &brief.sections[3] else { panic!("the work graph is a flow") };
	assert_eq!((nodes[0].layer, nodes[1].layer), (0, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn his_b_key_builds_and_opens_the_morning_brief_and_today_gains_a_row_that_enter_opens_0680() {
	use crate::frontier::protocol::{ClientMessage, ServerMessage};
	use crate::frontier::server::helpers::{Page, until, with_server};
	use crate::frontier::server::server::ServerOptions;
	use crate::shared::testing::temp_jocasta;

	let _j = temp_jocasta();
	let links = Arc::new(FakeLinks::default());
	let options = ServerOptions { briefs: Some(Arc::clone(&links) as Arc<dyn BriefLinks>), ..ServerOptions::default() };
	with_server(options, |server| async move {
		let page = Page::open(&server, "").await;
		// No brief yet: no row, and nothing opened by itself.
		assert!(server.service.day().today().unwrap().briefs.is_empty());
		assert!(opened(&links).is_empty());
		page.send(&ClientMessage::BriefMorning);
		let action = page.wait_for(|m| matches!(m, ServerMessage::Action { .. })).await;
		assert_eq!(action, ServerMessage::Action { text: "Morning brief opened.".to_owned(), is_error: false });
		let ServerMessage::Today { today } =
			page.wait_for(|m| matches!(m, ServerMessage::Today { today } if !today.briefs.is_empty())).await
		else {
			unreachable!("a today message")
		};
		assert_eq!(today.briefs.len(), 1);
		let row = &today.briefs[0];
		assert_eq!(row.label, "morning brief");
		assert!(row.url.contains("/brief/morning-"), "{}", row.url);
		let id = row.url.rsplit('/').next().unwrap().to_owned();
		assert_eq!(opened(&links), std::slice::from_ref(&id));
		// Enter on the row sends `link.open` with its link: the page opens again with a fresh launch code.
		page.send(&ClientMessage::LinkOpen { url: row.url.clone() });
		until(|| opened(&links).len() == 2).await;
		assert_eq!(opened(&links)[1], id);
	})
	.await;
}
