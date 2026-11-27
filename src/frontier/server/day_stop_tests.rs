//! Tests of `day.rs` ported from `test/frontier/server/day-stop.test.ts`: the day's stop ends the poller rounds under
//! way and awaits them before the databases close (F7): a slow `gh` round used to finish after Stop everything and log
//! "Poller github failed: Cannot use a closed database".

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Notify, watch};

use super::tests::{Stand, at, project, settle};
use super::*;
use crate::shared::process::aborted;
use crate::shared::testing::temp_jocasta;

/// A Day whose `gh` is `run`, the pollers off so the test runs the rounds; its notifications are collected.
fn stand(run: impl RunCli + 'static) -> Stand {
	Stand::with(
		at(2026, 10, 8, 10, 0),
		vec![project("p1", "acme-api")],
		DayOptions { pollers: Some(false), run: Some(Arc::new(run)), ..DayOptions::default() },
	)
}

/// The service's order: the day stops, then the catalog closes.
async fn close(s: &Stand) {
	s.day.stop().await;
}

/// A pull request of his whose checks failed, so a finished round has something to write; keyed by `gh api`'s route.
fn answer(route: &str) -> String {
	match route {
		"notifications" => "[[]]".to_owned(),
		"graphql" => serde_json::json!({
			"data": {
				"open": { "nodes": [{ "url": "https://github.com/acme/api/pull/7" }] },
				"recent": { "nodes": [{
					"number": 7,
					"title": "Fix",
					"url": "https://github.com/acme/api/pull/7",
					"repository": { "nameWithOwner": "acme/api" },
					"commits": { "nodes": [{ "commit": { "statusCheckRollup": { "contexts": { "nodes": [{
						"name": "ci",
						"status": "COMPLETED",
						"conclusion": "FAILURE",
						"detailsUrl": "https://ci.example.org/7",
					}] } } } }] },
				}] },
			},
		})
		.to_string(),
		_ => String::new(),
	}
}

#[tokio::test]
async fn a_slow_round_that_ignores_the_stop_is_awaited_and_writes_nothing_once_the_day_stopped() {
	let _j = temp_jocasta();
	let asked = Arc::new(Notify::new());
	let (slow, slow_seen) = watch::channel(false);
	let calls = Arc::new(Mutex::new(Vec::<String>::new()));
	let (asking, called, held) = (Arc::clone(&asked), Arc::clone(&calls), slow_seen);
	let s = stand(
		move |_command: CliCommand, args: Vec<String>, _ok: Vec<i32>, _signal: Option<AbortSignal>| -> RunFuture {
			let route = args.get(1).cloned().unwrap_or_default();
			called.lock().unwrap().push(route.clone());
			asking.notify_one();
			let mut held = held.clone();
			Box::pin(async move {
				let _ = held.wait_for(|open| *open).await;
				Ok(answer(&route))
			})
		},
	);
	let day = s.day.clone();
	let round = tokio::spawn(async move { day.poll(PollerName::Github).await });
	asked.notified().await;
	let closed = Arc::new(AtomicBool::new(false));
	let (stopping, flag) = (s.day.clone(), Arc::clone(&closed));
	let closing = tokio::spawn(async move {
		stopping.stop().await;
		flag.store(true, Ordering::SeqCst);
	});
	// The stop waits for the round's `gh` while it runs.
	settle().await;
	assert!(!closed.load(Ordering::SeqCst));
	slow.send_replace(true);
	closing.await.unwrap();
	// Stop resolved only after the round's last `gh` call returned, before the catalog closed.
	assert_eq!(*calls.lock().unwrap(), ["notifications", "graphql"]);
	round.await.unwrap();
	assert!(s.notes().is_empty());
	// The finished round wrote nothing once the day stopped.
	assert!(s.day.queue(|q| q.open()).unwrap().is_empty());
}

#[tokio::test]
async fn a_round_under_way_is_ended_by_the_stop_gh_gets_the_abort_and_nothing_is_reported() {
	let _j = temp_jocasta();
	let signal = Arc::new(Mutex::new(None::<AbortSignal>));
	let asked = Arc::new(Notify::new());
	let (keep, asking) = (Arc::clone(&signal), Arc::clone(&asked));
	let s = stand(
		move |_command: CliCommand, _args: Vec<String>, _ok: Vec<i32>, given: Option<AbortSignal>| -> RunFuture {
			*keep.lock().unwrap() = given.clone();
			asking.notify_one();
			let mut given = given;
			Box::pin(async move {
				aborted(&mut given).await;
				Err(Error::other("gh api notifications failed: exit 143"))
			})
		},
	);
	let day = s.day.clone();
	tokio::spawn(async move { day.poll(PollerName::Github).await });
	asked.notified().await;
	let given = signal.lock().unwrap().clone().expect("the day gave gh a signal");
	assert!(given.borrow().is_none());
	// Without the abort this `gh` never answers, and the stop would never resolve.
	close(&s).await;
	assert!(given.borrow().is_some());
	assert!(s.notes().is_empty());
}

#[tokio::test]
async fn no_round_starts_once_the_day_stopped() {
	let _j = temp_jocasta();
	let calls = Arc::new(Mutex::new(Vec::<String>::new()));
	let called = Arc::clone(&calls);
	let s = stand(
		move |_command: CliCommand, args: Vec<String>, _ok: Vec<i32>, _signal: Option<AbortSignal>| -> RunFuture {
			called.lock().unwrap().push(args.first().cloned().unwrap_or_default());
			Box::pin(async { Ok("[]".to_owned()) })
		},
	);
	close(&s).await;
	s.day.poll(PollerName::Github).await;
	assert!(calls.lock().unwrap().is_empty());
	assert!(s.notes().is_empty());
}

#[tokio::test]
async fn gh_without_a_login_is_one_header_line_and_no_notification_until_it_signs_in() {
	let _j = temp_jocasta();
	let signed_in = Arc::new(AtomicBool::new(false));
	let login = Arc::clone(&signed_in);
	let s = stand(
		move |_command: CliCommand, args: Vec<String>, _ok: Vec<i32>, _signal: Option<AbortSignal>| -> RunFuture {
			let route = args.get(1).cloned().unwrap_or_default();
			let signed_in = login.load(Ordering::SeqCst);
			Box::pin(async move {
				if signed_in {
					Ok(answer(&route))
				} else {
					Err(Error::other(
						"gh api notifications failed: exit 4: To get started with GitHub CLI, please run:  gh auth login",
					))
				}
			})
		},
	);
	s.day.poll(PollerName::Github).await;
	s.day.poll(PollerName::Github).await;
	assert!(s.notes().is_empty(), "no notification: {:?}", s.notes());
	assert_eq!(s.day.problems(), [GH_SIGNED_OUT]);
	signed_in.store(true, Ordering::SeqCst);
	s.day.poll(PollerName::Github).await;
	assert!(s.day.problems().is_empty());
	close(&s).await;
}
