//! Tests of `close.rs`. The TypeScript tests the close through the whole service (`shutdown.test.ts`, which waits for
//! `server.rs`); here its flow runs on stand-ins for the service, the question and the stop.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures_util::FutureExt;
use serde_json::json;

use super::*;
use crate::frontier::server::popups::{CANCEL, Popups};

struct Busy(Mutex<Counts>);

impl CloseService for Busy {
	fn discovered(&self) -> BoxFuture<'static, ()> {
		async {}.boxed()
	}

	fn busy_threads(&self) -> Counts {
		*locked(&self.0)
	}
}

struct World {
	popups: Arc<Popups>,
	asked: Arc<Mutex<Vec<(String, bool)>>>,
	stops: Arc<AtomicUsize>,
	stopping: Arc<AtomicBool>,
	flow: CloseFlow,
}

/// A close whose question is answered with `choice` at once; `None` leaves it open.
fn world(counts: Counts, choice: Option<&'static str>) -> World {
	let popups = Arc::new(Popups::new());
	let asked = Arc::new(Mutex::new(Vec::new()));
	let (stops, stopping) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(false)));
	let opens = popups.clone();
	let record = asked.clone();
	let question: Ask = Arc::new(move |view, _signal, from_key| {
		locked(&record).push((view.message.clone(), from_key));
		let popup = opens.open(view, from_key);
		if let Some(choice) = choice {
			opens.answer(popup.code(), &json!({ "choice": choice, "typed": view.code }));
		}
		popup
	});
	let (stop_count, under_way) = (stops.clone(), stopping.clone());
	let flow = close_flow(CloseParts {
		service: Arc::new(Busy(Mutex::new(counts))),
		question,
		stopping: Arc::new(move || under_way.load(Ordering::SeqCst).then(|| async {}.boxed())),
		stop_all: Arc::new(move || {
			stop_count.fetch_add(1, Ordering::SeqCst);
			async {}.boxed()
		}),
	});
	World { popups, asked, stops, stopping, flow }
}

const IDLE: Counts = Counts { working: 0, awaiting: 0 };
const ASKED: &str = "Stop 1 working thread? They pick up where they left off next time.";

fn asking() -> CloseHow {
	CloseHow { ask: true }
}

#[tokio::test]
async fn with_no_thread_at_work_closing_asks_nothing_and_stops_everything() {
	let w = world(IDLE, None);
	assert_eq!(w.flow.close(asking()).await, CloseOutcome::Stopped);
	assert!(locked(&w.asked).is_empty());
	assert_eq!(w.stops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn closing_without_asking_stops_everything_whatever_is_at_work() {
	let w = world(Counts { working: 2, awaiting: 1 }, None);
	assert_eq!(w.flow.close(CloseHow { ask: false }).await, CloseOutcome::Stopped);
	assert!(locked(&w.asked).is_empty());
	assert_eq!(w.stops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_working_or_awaiting_thread_asks_first_and_cancel_leaves_everything_running() {
	let w = world(Counts { working: 1, awaiting: 0 }, Some(CANCEL));
	assert_eq!(w.flow.close(asking()).await, CloseOutcome::Kept);
	// The quit action asks where he is, with no notification (`from_key`).
	assert_eq!(*locked(&w.asked), [(ASKED.to_owned(), true)]);
	assert_eq!(w.stops.load(Ordering::SeqCst), 0);
	assert_eq!(w.stops.load(Ordering::SeqCst), 0);
	// Kept: a later close asks again.
	assert_eq!(w.flow.close(asking()).await, CloseOutcome::Kept);
	assert_eq!(locked(&w.asked).len(), 2);
}

#[tokio::test]
async fn stop_everything_stops_after_the_question() {
	let w = world(Counts { working: 0, awaiting: 2 }, Some(STOP));
	assert_eq!(w.flow.close(asking()).await, CloseOutcome::Stopped);
	assert_eq!(
		*locked(&w.asked),
		[("Stop 2 threads awaiting you? They pick up where they left off next time.".to_owned(), true)]
	);
	assert_eq!(w.stops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn one_close_at_a_time_a_second_asks_nothing_more_and_shares_the_answer() {
	let w = world(Counts { working: 1, awaiting: 0 }, None);
	let first = tokio::spawn({
		let flow = w.flow.clone();
		async move { flow.close(asking()).await }
	});
	while w.popups.list().is_empty() {
		tokio::task::yield_now().await;
	}
	let second = tokio::spawn({
		let flow = w.flow.clone();
		async move { flow.close(asking()).await }
	});
	tokio::time::sleep(Duration::from_millis(50)).await;
	assert_eq!(locked(&w.asked).len(), 1);
	let popup = w.popups.list().remove(0);
	w.popups.answer(popup.code(), &json!({ "choice": CANCEL }));
	assert_eq!(first.await.unwrap(), CloseOutcome::Kept);
	assert_eq!(second.await.unwrap(), CloseOutcome::Kept);
	assert_eq!(locked(&w.asked).len(), 1);
}

#[tokio::test]
async fn a_stop_under_way_is_waited_for_and_asks_nothing() {
	let w = world(Counts { working: 1, awaiting: 0 }, None);
	w.stopping.store(true, Ordering::SeqCst);
	assert_eq!(w.flow.close(asking()).await, CloseOutcome::Stopped);
	assert!(locked(&w.asked).is_empty());
	assert_eq!(w.stops.load(Ordering::SeqCst), 0);
}
