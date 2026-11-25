use std::time::Duration;

use super::*;

#[tokio::test]
async fn a_change_wakes_every_waiter_even_one_that_starts_waiting_after_it() {
	let (ui, _rx) = std::sync::mpsc::channel();
	let hub = Hub::new(ui);
	let (refresh, watch) = (hub.changed(), hub.changed());
	hub.touched();
	for waiter in [&refresh, &watch] {
		tokio::time::timeout(Duration::from_secs(1), waiter.notified()).await.expect("each waiter is woken");
	}
	// A waiter that went is dropped from the list, not woken.
	drop(watch);
	hub.touched();
	assert_eq!(locked(&hub.changed).len(), 1);
}

#[test]
fn a_sleeping_tab_reads_asleep_in_the_sidebar_not_starting_0689() {
	let (ui, _rx) = std::sync::mpsc::channel();
	let hub = Hub::new(ui);
	let wake: Wake = Arc::new(|_done| {});
	let pane = hub.start_still("acme · asleep", wake);
	hub.add_workspace("acme", Path::new("."), pane);
	let rows = hub.rows(0);
	assert_eq!(
		rows.iter().map(|row| (row.label.as_str(), row.text.as_str())).collect::<Vec<_>>(),
		[("acme", "asleep")]
	);
}
