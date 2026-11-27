use serde_json::json;

use super::*;
use crate::shared::testing::temp_dir;

fn ran(exit: Option<i32>, output: &str) -> Ran {
	Ran { exit, output: output.to_owned(), ms: 1 }
}

#[test]
fn a_step_passes_on_its_text_or_its_exit_code_0706() {
	assert!(gives(&ran(Some(1), "3 passed"), "passed"));
	assert!(!gives(&ran(Some(0), "3 failed"), "passed"));
	assert!(gives(&ran(Some(2), ""), "exit:2"));
	assert!(!gives(&ran(Some(0), "exit:2"), "exit:2"));
	assert!(!gives(&ran(None, ""), "exit:0"));
	assert_eq!(said(&ran(Some(3), "a\nb")), "exit 3: a\nb");
	assert_eq!(said(&ran(None, "")), "it didn't end in time: no output");
}

#[test]
fn steps_are_a_non_empty_list_of_run_and_expect_0706() {
	let steps = steps_of(Some(&json!([{ "run": "cargo run -- --help", "expect": "Usage" }])), "review_try").unwrap();
	assert_eq!(steps, vec![Step { run: "cargo run -- --help".to_owned(), expect: "Usage".to_owned() }]);
	let refused = |value: serde_json::Value| steps_of(Some(&value), "review_try").unwrap_err().to_string();
	assert!(refused(json!([])).contains("at least one"));
	assert!(refused(json!([{ "run": "x" }])).contains("step 1 needs `run`"));
	assert!(refused(json!([{ "run": "x", "expect": "y", "shell": "bash" }])).contains("unknown key `shell`"));
	assert!(refused(json!([{ "run": "x", "expect": "exit:zero" }])).contains("isn't exit:<number>"));
}

#[tokio::test]
async fn a_step_here_runs_in_its_folder_with_its_environment_0706() {
	let dir = temp_dir("steps");
	let env: HashMap<String, String> = crate::shared::env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.chain([("JOCASTA_STEP_TEST".to_owned(), "seen".to_owned())])
		.collect();
	std::fs::write(dir.path().join("marker.txt"), "here").unwrap();
	let place = Place::Here { path: dir.path(), env: &env };
	let command = if cfg!(windows) {
		"type marker.txt && echo %JOCASTA_STEP_TEST%"
	} else {
		"cat marker.txt; echo; echo $JOCASTA_STEP_TEST"
	};
	let done = run(&place, command).await.unwrap();
	assert_eq!(done.exit, Some(0), "{}", done.output);
	assert!(gives(&done, "here") && gives(&done, "seen"), "{}", done.output);
	assert!(gives(&run(&place, "exit 4").await.unwrap(), "exit:4"));
}
