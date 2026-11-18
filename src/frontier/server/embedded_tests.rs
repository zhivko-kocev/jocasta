use super::*;

fn words(args: &[&str]) -> Vec<String> {
	args.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn only_a_server_run_bare_jocasta_launched_gets_the_service_whatever_its_session() {
	for args in [&["server"][..], &["--session", "t", "server"], &["--session=t", "server"]] {
		assert!(serves(&words(args), Some("1")), "{args:?}");
		assert!(!serves(&words(args), None), "{args:?}");
		assert!(!serves(&words(args), Some("0")), "{args:?}");
	}
	for args in
		[&[][..], &["server", "stop"], &["server", "reload-config"], &["pane", "list"], &["--session", "server"]]
	{
		assert!(!serves(&words(args), Some("1")), "{args:?}");
	}
}
