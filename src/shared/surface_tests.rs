use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;

use super::*;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// A machine for bare `jocasta`: the service's states in turn (the last one stays), the log's last error, what it
/// printed.
struct Fake {
	states: RefCell<VecDeque<ServiceState>>,
	last_error: Option<String>,
	config: PathBuf,
	printed: RefCell<Vec<String>>,
}

impl SurfaceDeps for Fake {
	async fn service_health(&self) -> ServiceHealth {
		let mut states = self.states.borrow_mut();
		let state = if states.len() > 1 { states.pop_front() } else { states.front().copied() };
		ServiceHealth { state: state.unwrap_or(ServiceState::Down), layer: None, hosts: None }
	}

	fn last_error(&self, _since: i64) -> Option<String> {
		self.last_error.clone()
	}

	fn herdr_config(&self) -> &Path {
		&self.config
	}

	fn print(&self, line: &str) {
		self.printed.borrow_mut().push(line.to_owned());
	}

	async fn sleep(&self, _ms: u64) {}
}

fn fake(j: &TempJocasta, states: &[ServiceState]) -> Fake {
	let config = j.root().join("herdr.toml");
	std::fs::write(&config, "").unwrap();
	Fake {
		states: RefCell::new(states.iter().copied().collect()),
		last_error: None,
		config,
		printed: RefCell::default(),
	}
}

#[tokio::test]
async fn a_service_that_is_closing_is_waited_for_after_saying_so_lc_3() {
	let j = temp_jocasta();
	let f = fake(&j, &[ServiceState::Closing, ServiceState::Closing, ServiceState::Down]);
	before_surface(&f).await.unwrap();
	assert_eq!(*f.printed.borrow(), ["jocasta: Jocasta is closing; it starts again once it has."]);
	assert!(f.states.borrow().iter().all(|state| *state == ServiceState::Down));
}

#[tokio::test]
async fn a_missing_herdr_toml_points_at_setup_and_the_client_still_attaches() {
	let j = temp_jocasta();
	let f = fake(&j, &[ServiceState::Down]);
	std::fs::remove_file(&f.config).unwrap();
	before_surface(&f).await.unwrap();
	let printed = f.printed.borrow();
	assert_eq!(printed.len(), 1);
	assert!(printed[0].ends_with("is missing; run `jocasta setup` for Jocasta's herdr look."), "{printed:?}");
}

#[tokio::test]
async fn a_service_that_couldnt_start_is_said_with_its_last_error_and_exit_1_once_the_client_ends_0582() {
	let j = temp_jocasta();
	// herdr's client ends before the service's stop has run: a closing service is waited for.
	let mut f = fake(&j, &[ServiceState::Closing, ServiceState::Closing, ServiceState::Down]);
	f.last_error = Some("C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token".to_owned());
	assert_eq!(after_surface(&f, 0, 0).await, 1);
	let said = [
		"jocasta: Jocasta stopped while starting: C:/Users/x/.jocasta/agent/projects.yaml:4: Unexpected token",
		"jocasta: run `jocasta diagnose` for the full log.",
	];
	assert_eq!(*f.printed.borrow(), said);
	// A client that lost its server (exit 1) waits for the service even while it still answers.
	let mut lost = fake(&j, &[ServiceState::Up, ServiceState::Closing, ServiceState::Down]);
	lost.last_error = f.last_error.clone();
	assert_eq!(after_surface(&lost, 0, 1).await, 1);
	assert_eq!(*lost.printed.borrow(), said);
	// A detach with Jocasta running, or a service that logged nothing: herdr's client's own exit code.
	let mut up = fake(&j, &[ServiceState::Up, ServiceState::Down]);
	up.last_error = f.last_error.clone();
	assert_eq!(after_surface(&up, 0, 0).await, 0);
	let quiet = fake(&j, &[ServiceState::Down]);
	assert_eq!(after_surface(&quiet, 0, 3).await, 3);
	assert!(up.printed.borrow().is_empty() && quiet.printed.borrow().is_empty());
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
