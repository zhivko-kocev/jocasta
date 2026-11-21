use super::*;

fn words(args: &[&str]) -> Vec<String> {
	args.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn only_a_server_run_bare_jocasta_launched_or_a_handoff_started_gets_the_service_whatever_its_session() {
	let handoff = ["server", "--handoff-import", "/tmp/herdr-handoff-1.sock", "1-2"];
	for args in [&["server"][..], &["--session", "t", "server"], &["--session=t", "server"], &handoff] {
		assert!(serves(&words(args), Some("1")), "{args:?}");
		assert!(!serves(&words(args), None), "{args:?}");
		assert!(!serves(&words(args), Some("0")), "{args:?}");
	}
	for args in [
		&[][..],
		&["server", "stop"],
		&["server", "reload-config"],
		&["server", "live-handoff"],
		&["server", "--handoff-import"],
		&["pane", "list"],
		&["--session", "server"],
	] {
		assert!(!serves(&words(args), Some("1")), "{args:?}");
	}
}
