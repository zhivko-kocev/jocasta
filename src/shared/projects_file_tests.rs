use std::fs;
use std::path::PathBuf;

use super::*;
use crate::shared::testing::temp_jocasta;

/// A projects.yaml in the temp root with `text`.
fn file_with(root: &Path, text: &str) -> PathBuf {
	let file = root.join("projects.yaml");
	fs::write(&file, text).unwrap();
	file
}

fn push_of(file: &Path) -> Vec<(String, PushEntry)> {
	read_projects_file(Some(file)).unwrap().push
}

fn entry(repo: &str, way: PushWay, line: usize) -> (String, PushEntry) {
	(repo.to_owned(), PushEntry { way, line })
}

fn read(file: &Path) -> String {
	fs::read_to_string(file).unwrap()
}

#[test]
fn the_push_map_is_read_with_each_entrys_line_and_a_file_without_one_has_none() {
	let env = temp_jocasta();
	let file = file_with(
		env.root(),
		"version: 1\nadd:\n  - C:/code/acme\npush:\n  C:/code/acme: direct\n  C:/code/client-a: pull-requests\n",
	);
	assert_eq!(
		push_of(&file),
		[entry("C:/code/acme", PushWay::Direct, 5), entry("C:/code/client-a", PushWay::PullRequests, 6)]
	);
	assert_eq!(push_of(&file_with(env.root(), "version: 1\n")), []);
	assert_eq!(push_of(&file_with(env.root(), "version: 1\npush:\n")), []);
}

#[test]
fn a_bad_push_value_is_reported_by_file_line_and_key_like_the_files_other_problems() {
	let env = temp_jocasta();
	let file = file_with(env.root(), "version: 1\npush:\n  C:/code/acme: direkt\n");
	let error = read_projects_file(Some(&file)).unwrap_err();
	assert!(matches!(error, Error::User { exit_code: 1, .. }));
	assert!(error.to_string().starts_with(&format!("{}:3: push.C:/code/acme", file.display())));
}

#[test]
fn both_answers_are_written_into_a_file_that_has_no_push_map_keeping_comments_order_and_keys() {
	let env = temp_jocasta();
	let text = "# my projects\nversion: 1\nroots:\n  - path: C:/code # all of it\nnames:\n  p-1: Acme\nscopes: old\n";
	let file = file_with(env.root(), text);
	assert!(set_push_way("C:\\code\\acme", PushWay::Direct, Some(&file)).unwrap());
	assert_eq!(read(&file), format!("{text}push:\n  C:/code/acme: direct\n"));
	assert!(set_push_way("C:/code/client-a", PushWay::PullRequests, Some(&file)).unwrap());
	assert_eq!(read(&file), format!("{text}push:\n  C:/code/acme: direct\n  C:/code/client-a: pull-requests\n"));
	let pushed = push_of(&file);
	assert_eq!(pushed[1].1.way, PushWay::PullRequests);
}

#[test]
fn an_entry_for_the_same_folder_is_changed_in_place_with_its_comment_a_repeat_changes_nothing_and_keys_after_the_map_stay()
 {
	let env = temp_jocasta();
	let text = "version: 1\npush:\n  # per repo\n  C:/code/acme: pull-requests # PRs\n  C:/code/other: direct\nnames:\n  p-1: Acme\n";
	let file = file_with(env.root(), text);
	// Windows folds case, so another spelling of the folder is the same one; elsewhere only the slash differs.
	let spelled = if cfg!(windows) { "c:/CODE/acme/" } else { "C:/code/acme/" };
	assert!(set_push_way(spelled, PushWay::Direct, Some(&file)).unwrap());
	assert_eq!(read(&file), text.replace("pull-requests # PRs", "direct # PRs").replace("C:/code/acme", spelled));
	assert!(!set_push_way("C:/code/other", PushWay::Direct, Some(&file)).unwrap());
	assert!(set_push_way("C:/code/new", PushWay::Direct, Some(&file)).unwrap());
	assert!(read(&file).contains("  C:/code/other: direct\n  C:/code/new: direct\nnames:\n"));
}

#[test]
fn an_empty_or_flow_push_map_takes_the_entry_as_a_block_and_a_missing_file_is_made() {
	let env = temp_jocasta();
	let flow = file_with(env.root(), "version: 1\npush: {} # none yet\nnames: {}\n");
	set_push_way("C:/code/acme", PushWay::Direct, Some(&flow)).unwrap();
	assert_eq!(read(&flow), "version: 1\npush: # none yet\n  C:/code/acme: direct\nnames: {}\n");
	let empty = file_with(env.root(), "push:\nnames: {}\n");
	set_push_way("C:/code/acme", PushWay::PullRequests, Some(&empty)).unwrap();
	assert_eq!(read(&empty), "push:\n  C:/code/acme: pull-requests\nnames: {}\n");
	let made = env.root().join("none.yaml");
	set_push_way("C:/code/acme", PushWay::Direct, Some(&made)).unwrap();
	assert_eq!(read(&made), "version: 1\npush:\n  C:/code/acme: direct\n");
}

#[test]
fn list_edits_rewrite_a_flow_sequence_as_a_block_and_keep_crlf_line_endings() {
	let env = temp_jocasta();
	let file = file_with(env.root(), "version: 1\r\nexclude: [] # none yet\r\nroots: []\r\n");
	assert!(add_to_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert!(!add_to_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert_eq!(read(&file), "version: 1\r\nexclude: # none yet\r\n  - D:/old-fork\r\nroots: []\r\n");
	assert!(remove_from_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert_eq!(read_projects_file(Some(&file)).unwrap().exclude, Vec::<String>::new());
}

#[test]
fn a_list_edit_keeps_comments_adds_after_the_last_item_and_makes_a_missing_list() {
	let env = temp_jocasta();
	let file = file_with(env.root(), "version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n");
	assert!(add_to_list(ListKey::Add, "/some/where", Some(&file)).unwrap());
	assert_eq!(read(&file), "version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n  - /some/where\n");
	assert_eq!(read_projects_file(Some(&file)).unwrap().add, ["/nowhere/else", "/some/where"]);
	assert!(add_to_list(ListKey::Exclude, "D:\\fork", Some(&file)).unwrap());
	assert!(read(&file).ends_with("exclude:\n  - D:/fork\n"));
	assert!(!remove_from_list(ListKey::Add, "/missing", Some(&file)).unwrap());
	assert!(remove_from_list(ListKey::Add, "/nowhere/else/", Some(&file)).unwrap());
	assert_eq!(read_projects_file(Some(&file)).unwrap().add, ["/some/where"]);
	assert!(read(&file).contains("# adds below"));
}

#[test]
fn a_malformed_file_stops_with_one_line_naming_the_file_and_line_exit_1() {
	let env = temp_jocasta();
	let file = file_with(env.root(), "version: 1\nroots:\n  - path: D:/code\n    depth: 0\n");
	let depth = read_projects_file(Some(&file)).unwrap_err();
	assert!(matches!(depth, Error::User { exit_code: 1, .. }));
	assert_eq!(depth.to_string(), format!("{}:4: roots[0].depth must be >= 1", file.display()));
	file_with(env.root(), "version: 1\nadd: [unclosed\n");
	let syntax = read_projects_file(Some(&file)).unwrap_err();
	assert!(matches!(syntax, Error::User { exit_code: 1, .. }));
	assert!(syntax.to_string().starts_with(&format!("{}:2: ", file.display())));
}

#[test]
fn the_schemas_other_messages_are_the_checkers() {
	let env = temp_jocasta();
	let message = |text: &str| {
		let file = file_with(env.root(), text);
		read_projects_file(Some(&file)).unwrap_err().to_string().replace(&format!("{}:", file.display()), "")
	};
	assert_eq!(message("- a\n- b\n"), "1: the file must be object");
	assert_eq!(message("version: 2\n"), "1: version must be equal to constant");
	assert_eq!(message("roots: x\n"), "1: roots must be array");
	assert_eq!(message("roots:\n  - path: 1\n"), "2: roots[0].path must be string");
	assert_eq!(message("roots:\n  - depth: 2\n"), "2: roots[0] must have required properties path");
	assert_eq!(message("roots:\n  - path: a\n    depth: 1.5\n"), "3: roots[0].depth must be integer");
	assert_eq!(message("add: [1]\n"), "1: add[0] must be string");
	assert_eq!(message("add: x\n"), "1: add must be array");
	assert_eq!(message("names:\n  a: 1\n"), "2: names.a must be string");
	assert_eq!(message("names: [1]\n"), "1: names must be object");
	assert_eq!(message("push: x\n"), "1: push must be object");
}

#[test]
fn a_missing_file_is_an_empty_one_and_the_files_other_keys_are_read() {
	let env = temp_jocasta();
	assert_eq!(read_projects_file(Some(&env.root().join("none.yaml"))).unwrap(), ProjectsFile::default());
	let file = file_with(
		env.root(),
		"version: 1\nroots:\n  - path: C:/code\n    ignore: [old]\n  - path: D:/x\n    depth: 2\nnames:\n  p-1: Acme\nscopes: old\n",
	);
	let read = read_projects_file(Some(&file)).unwrap();
	assert_eq!(
		read.roots,
		[
			WatchedRoot { path: "C:/code".to_owned(), depth: 1, ignore: vec!["old".to_owned()] },
			WatchedRoot { path: "D:/x".to_owned(), depth: 2, ignore: vec![] },
		]
	);
	assert_eq!(read.names, [("p-1".to_owned(), "Acme".to_owned())]);
}

#[test]
fn clean_room_off_and_pr_run_answers_are_read_and_an_answer_is_added_or_changed_in_place_0706_0708() {
	let env = temp_jocasta();
	let file = file_with(env.root(), "# mine\ncleanRoom:\n  /code/acme: off\n  /code/client-a: auto\nprRun: {}\n");
	let read_back = read_projects_file(Some(&file)).unwrap();
	assert_eq!(read_back.clean_room_off, ["/code/acme"]);
	assert!(set_pr_run("/code/acme", "octo-cat", true, Some(&file)).unwrap());
	assert!(set_pr_run("/code/acme", "Jane Doe", false, Some(&file)).unwrap());
	assert!(!set_pr_run("/code/acme", "octo-cat", true, Some(&file)).unwrap(), "already so");
	assert!(set_pr_run("/code/acme", "octo-cat", false, Some(&file)).unwrap());
	assert!(set_pr_run("/code/client-a", "octo-cat", true, Some(&file)).unwrap());
	assert_eq!(
		read(&file),
		"# mine\ncleanRoom:\n  /code/acme: off\n  /code/client-a: auto\nprRun:\n  /code/acme: {\"octo-cat\": deny, \"Jane Doe\": deny}\n  /code/client-a: {\"octo-cat\": allow}\n"
	);
	let parsed = read_projects_file(Some(&file)).unwrap();
	assert_eq!(parsed.pr_run_answer(Path::new("/code/acme"), "Jane Doe"), Some(false));
	assert_eq!(parsed.pr_run_answer(Path::new("/code/client-a"), "octo-cat"), Some(true));
	assert_eq!(parsed.pr_run_answer(Path::new("/code/client-a"), "someone"), None);
	file_with(env.root(), "prRun:\n  /code/acme: allow\n");
	assert!(read_projects_file(Some(&file)).unwrap_err().to_string().contains("prRun"));
}
