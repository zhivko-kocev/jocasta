//! Tests of `worktrees.rs`. The TypeScript has none of its own: the service tests (`service-thread-start.test.ts`,
//! `service-finish.test.ts`) reach it through the service, so these check the module's own contract.

use std::fs;
use std::path::Path;

use super::*;
use crate::shared::testing::{temp_dir, temp_jocasta};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn repo(root: &Path, name: &str) -> PathBuf {
	let dir = root.join(name);
	fs::create_dir_all(&dir).unwrap();
	sh(&dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("README.md"), "demo\n").unwrap();
	sh(&dir, &["add", "."]);
	sh(&dir, &["commit", "-q", "-m", "init"]);
	dir
}

#[test]
fn a_thread_pane_s_variables_follow_the_slug_and_number_0457() {
	let env = thread_env("My App", "Fix_it!!", 3);
	assert_eq!(
		env,
		vec![
			("JOCASTA_THREAD".to_owned(), "Fix_it!!".to_owned()),
			("COMPOSE_PROJECT_NAME".to_owned(), "my-app-fix_it-".to_owned()),
			("JOCASTA_PORT_OFFSET".to_owned(), "30".to_owned()),
		]
	);
	assert_eq!(thread_env("-x", "y", 1)[1].1, "x-y");
}

#[test]
fn thread_numbers_give_the_lowest_free_positive_integer_and_forget_frees_it_0457() {
	let _j = temp_jocasta();
	let numbers = ThreadNumbers::new(None);
	let (a, b) = (PathBuf::from("/wt/a"), PathBuf::from("/wt/b"));
	let open = [a.clone(), b.clone()];
	assert_eq!(numbers.of(&a, &open).unwrap(), 1);
	assert_eq!(numbers.of(&b, &open).unwrap(), 2);
	assert_eq!(numbers.of(&a, &open).unwrap(), 1);
	numbers.forget(&a).unwrap();
	assert_eq!(numbers.of(&PathBuf::from("/wt/c"), &[b, PathBuf::from("/wt/c")]).unwrap(), 1);
	let text = fs::read_to_string(data_dir().join("thread-numbers.json")).unwrap();
	assert!(text.starts_with("{\n  \"") && text.ends_with("}\n"), "{text}");
}

#[test]
fn reviewed_commits_are_kept_per_worktree_and_forgotten_by_none_0394() {
	let _j = temp_jocasta();
	let dir = temp_dir("reviewed");
	let reviewed = ReviewedCommits::new(None);
	assert_eq!(reviewed.of(dir.path()), None);
	reviewed.mark(dir.path(), Some("abc123")).unwrap();
	assert_eq!(reviewed.of(dir.path()).as_deref(), Some("abc123"));
	reviewed.mark(dir.path(), None).unwrap();
	assert_eq!(reviewed.of(dir.path()), None);
	assert!(reviewed_file().ends_with("reviewed.json"));
}

#[test]
fn a_new_thread_takes_the_first_branch_and_folder_free_for_both_0455() {
	let root = temp_dir("place");
	let main = repo(root.path(), "app");
	let first = thread_place(&main, "fix-it").unwrap();
	assert_eq!(first.branch, "feature/fix-it");
	assert_eq!(first.path, root.path().join("app.wt").join("fix-it"));
	// The branch alone being taken moves both on.
	sh(&main, &["branch", "feature/fix-it"]);
	assert_eq!(thread_place(&main, "fix-it").unwrap().branch, "feature/fix-it-2");
	fs::create_dir_all(root.path().join("app.wt").join("fix-it-2")).unwrap();
	assert_eq!(thread_place(&main, "fix-it").unwrap().branch, "feature/fix-it-3");
	let fallback = thread_place(&main, "").unwrap();
	assert!(fallback.branch.starts_with("feature/thread-") && fallback.branch.ends_with("-1"), "{}", fallback.branch);
}

#[tokio::test]
async fn worktree_include_copies_only_untracked_ignored_files_and_leaves_existing_ones_0457() {
	let root = temp_dir("include");
	let main = repo(root.path(), "app");
	fs::write(main.join(".gitignore"), ".env\nsecrets/\n").unwrap();
	fs::write(main.join(".worktreeinclude"), ".env\nsecrets/\n").unwrap();
	sh(&main, &["add", "."]);
	sh(&main, &["commit", "-q", "-m", "ignore"]);
	fs::write(main.join(".env"), "A=1\n").unwrap();
	fs::create_dir_all(main.join("secrets")).unwrap();
	fs::write(main.join("secrets").join("k"), "k\n").unwrap();
	let tree = root.path().join("wt");
	fs::create_dir_all(&tree).unwrap();
	fs::write(tree.join(".env"), "MINE\n").unwrap();
	let copied = copy_worktree_include(&main, &tree).await.unwrap();
	assert_eq!(copied, vec!["secrets/k".to_owned()]);
	assert_eq!(fs::read_to_string(tree.join(".env")).unwrap(), "MINE\n");
	assert_eq!(copy_worktree_include(&tree, &main).await.unwrap(), Vec::<String>::new());
}

fn trunk(_: &Project) -> BoxFuture<'_, Option<String>> {
	Box::pin(async { Some("main".to_owned()) })
}

#[tokio::test]
async fn a_checkout_is_removed_and_a_leftover_one_is_listed_until_then_0456_0486() {
	let root = temp_dir("leftover");
	let main = repo(root.path(), "app");
	let tree = root.path().join("app.wt").join("one");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/one", &tree.to_string_lossy()]);
	fs::write(tree.join("dirty.txt"), "x\n").unwrap();
	let demo = Project {
		id: "p".to_owned(),
		identity: "root:p".to_owned(),
		name: "app".to_owned(),
		main_path: main.clone(),
		git: true,
		paths: Vec::new(),
	};
	let left = find_leftovers(std::slice::from_ref(&demo), &[], trunk).await.unwrap();
	assert_eq!(left.len(), 1);
	assert_eq!((left[0].branch.as_deref(), left[0].ahead, left[0].dirty), (Some("feature/one"), 0, 1));
	// An open thread holds it: no leftover.
	assert!(find_leftovers(std::slice::from_ref(&demo), std::slice::from_ref(&tree), trunk).await.unwrap().is_empty());
	// A dirty checkout is refused at once, not waited on.
	let reason = remove_checkout(&main, &tree, Some(5_000)).await.expect("refused");
	assert!(reason.contains("modified or untracked") || reason.contains("contains modified"), "{reason}");
	fs::remove_file(tree.join("dirty.txt")).unwrap();
	assert_eq!(remove_checkout(&main, &tree, Some(5_000)).await, None);
	assert!(!tree.exists());
}
