use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::shared::git::git;
use crate::shared::pathkey::real_path;
use crate::shared::paths::projects_file;
use crate::shared::testing::temp_jocasta;

const DEMO: &str = "https://github.com/example-org/demo.git";

fn sh(dir: &Path, args: &[&str]) {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()));
}

fn make_repo(dir: &Path, origin: &str) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("README.md"), "demo\n").unwrap();
	sh(dir, &["add", "."]);
	sh(dir, &["commit", "-q", "-m", "init demo"]);
	sh(dir, &["remote", "add", "origin", origin]);
	real_path(dir)
}

fn args(list: &[&str]) -> Vec<String> {
	list.iter().map(|a| (*a).to_owned()).collect()
}

#[tokio::test]
async fn project_add_twice_says_its_already_a_project_list_shows_watched_repos_or_how_to_add_one_other_verbs_are_usage_0111()
 {
	let j = temp_jocasta();
	assert_eq!(
		project_command(&args(&["list"])).await.unwrap(),
		["No projects yet: add one with jocasta project add <path>, or a watched root in projects.yaml."]
	);
	let code = real_path(j.root()).join("code");
	let repo = make_repo(&code.join("demo"), DEMO);
	let added = project_command(&args(&["add", &repo.display().to_string()])).await.unwrap();
	assert!(added[0].starts_with("added demo ("), "{added:?}");
	let again = project_command(&args(&["add", &repo.display().to_string()])).await.unwrap();
	let pattern = regex::Regex::new(r"^demo \(p-[0-9a-f]+\) is already a project, at ").unwrap();
	assert!(pattern.is_match(&again[0]), "{again:?}");
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	fs::write(
		&file,
		format!("version: 1\nroots:\n  - path: {}\n    depth: 1\n", code.display().to_string().replace('\\', "/")),
	)
	.unwrap();
	let listed = project_command(&args(&["list"])).await.unwrap();
	assert!(regex::Regex::new(r"^p-[0-9a-f]{16} {2}demo {2}").unwrap().is_match(&listed[0]), "{listed:?}");
	let error = project_command(&args(&["rename", "x"])).await.unwrap_err();
	assert!(error.is_user());
}

#[tokio::test]
async fn project_usage_and_help_touch_nothing_no_catalog_no_data_folder() {
	let j = temp_jocasta();
	for case in [&["--help"][..], &[], &["rename", "x"], &["add"], &["list", "x"]] {
		let error = project_command(&args(case)).await.unwrap_err();
		assert!(error.is_user());
		assert_eq!(error.to_string(), "usage: jocasta project list | add <path> | remove <name|id>");
		assert!(matches!(error, Error::User { exit_code: 2, .. }));
	}
	assert!(!j.root().join("data").exists());
}
