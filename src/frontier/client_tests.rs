use std::path::PathBuf;

use super::*;
use crate::shared::fsx::free_port;
use crate::shared::service_file::{ServiceFile, write_service_file};
use crate::shared::testing::temp_jocasta;

const TOKEN: &str = "t0ken";

/// Names the stand-in on `port` in `jocasta-service.json` with this process's (live) pid.
fn name_service(port: u16) {
	write_service_file(&ServiceFile { port, token: TOKEN.to_owned(), pid: std::process::id() }, None).unwrap();
}

fn push() -> PushRequest {
	PushRequest { dir: PathBuf::from("."), remote: "origin".to_owned(), refs: Vec::new(), pane_key: None }
}

#[tokio::test]
async fn a_service_file_whose_process_is_gone_means_no_service_the_one_not_running_text_0581() {
	let _j = temp_jocasta();
	write_service_file(&ServiceFile { port: 1, token: TOKEN.to_owned(), pid: 2_u32.pow(31) - 2 }, None).unwrap();
	assert_eq!(service_at(), None);
	assert_eq!(ask_push(&push()).await.unwrap_err(), "Jocasta isn't running; start it with jocasta.");
	assert_eq!(ask_pane_check(Some("k")).await, "");
}

#[tokio::test]
async fn a_service_file_naming_a_live_process_with_nothing_listening_on_its_port_is_the_same_text_0581_ux_13() {
	let _j = temp_jocasta();
	name_service(free_port().unwrap());
	assert!(service_at().is_some());
	assert_eq!(ask_push(&push()).await.unwrap_err(), NOT_RUNNING);
}
