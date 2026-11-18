use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::shared::git::git;
use crate::shared::identity::project_id_for;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

const DEMO: &str = "https://github.com/example-org/demo.git";

/// Runs git in `dir` with a throwaway identity; fails the test on a non-zero exit.
fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

/// A git repo with one commit, optionally with an origin URL (never fetched).
fn make_repo(dir: &Path, origin: Option<&str>) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	fs::write(dir.join("README.md"), "demo\n").unwrap();
	sh(dir, &["add", "."]);
	// The folder name keeps two fixture repos made in the same second from sharing a root commit.
	sh(dir, &["commit", "-q", "-m", &format!("init {}", dir.file_name().unwrap().to_string_lossy())]);
	if let Some(origin) = origin {
		sh(dir, &["remote", "add", "origin", origin]);
	}
	real_path(dir)
}

/// The test's root, its folder names as the file system spells them.
fn root(j: &TempJocasta) -> PathBuf {
	real_path(j.root())
}

fn roots_yaml(root: &Path, extra: &str) -> String {
	format!(
		"version: 1\n# my roots\nroots:\n  - path: {}\n    depth: 2\n    ignore: [\"*.wt\", \"node_modules\"]\n{extra}",
		root.display().to_string().replace('\\', "/")
	)
}

fn write_projects_yaml(text: &str) -> PathBuf {
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	fs::write(&file, text).unwrap();
	file
}

fn forward(path: &Path) -> String {
	path.display().to_string().replace('\\', "/")
}

fn names(db: &Connection) -> Vec<String> {
	list_projects(db, None).unwrap().into_iter().map(|p| p.name).collect()
}

#[tokio::test]
async fn the_scan_registers_git_top_levels_under_a_root_honoring_depth_and_ignore_and_skips_plain_folders_0105() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	make_repo(&code.join("demo"), Some(DEMO));
	make_repo(&code.join("group").join("api"), None);
	make_repo(&code.join("group").join("deeper").join("too-deep"), None);
	make_repo(&code.join("node_modules").join("pkg"), None);
	fs::create_dir_all(code.join("notes")).unwrap();
	write_projects_yaml(&roots_yaml(&code, ""));
	let db = open_catalog().unwrap();
	sync_catalog(&db, None).await.unwrap();
	assert_eq!(names(&db), ["api", "demo"]);
}

#[tokio::test]
async fn clones_and_worktrees_of_one_repo_are_one_project_with_main_clone_and_worktree_paths_0104() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	let main = make_repo(&code.join("a-demo"), Some(DEMO));
	let clone = make_repo(&code.join("b-demo"), Some("git@github.com:example-org/demo.git"));
	let worktree = code.join("c-demo-wt");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/x", &worktree.display().to_string()]);
	write_projects_yaml(&roots_yaml(&code, ""));
	let db = open_catalog().unwrap();
	sync_catalog(&db, None).await.unwrap();
	let projects = list_projects(&db, None).unwrap();
	assert_eq!(projects.len(), 1);
	let project = &projects[0];
	assert_eq!(project.id, project_id_for("github.com/example-org/demo"));
	assert_eq!(project.main_path, main);
	let mut roles: Vec<&str> = project.paths.iter().map(|p| p.role.as_str()).collect();
	roles.sort_unstable();
	assert_eq!(roles, ["clone", "main", "worktree"]);
	assert_eq!(project.paths.iter().find(|p| p.role == PathRole::Clone).map(|p| &p.path), Some(&clone));
}

#[tokio::test]
async fn an_unchanged_git_config_is_not_re_resolved_a_changed_one_is_0108() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	let repo = make_repo(&code.join("demo"), None);
	write_projects_yaml(&roots_yaml(&code, ""));
	let db = open_catalog().unwrap();
	sync_catalog(&db, None).await.unwrap();
	assert!(list_projects(&db, None).unwrap()[0].identity.starts_with("root:"));
	// Same mtime: a cached identity stays even though git would now answer differently.
	db.execute("UPDATE projects SET identity = 'root:cached'", []).unwrap();
	sync_catalog(&db, None).await.unwrap();
	assert_eq!(list_projects(&db, None).unwrap()[0].identity, "root:cached");
	// Adding an origin rewrites .git/config (its mtime moves), so the path is resolved again into the origin's project.
	sh(&repo, &["remote", "add", "origin", DEMO]);
	let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
	let config = fs::File::options().write(true).open(repo.join(".git").join("config")).unwrap();
	config.set_modified(later).unwrap();
	drop(config);
	sync_catalog(&db, None).await.unwrap();
	let identities: Vec<String> = list_projects(&db, None).unwrap().into_iter().map(|p| p.identity).collect();
	assert_eq!(identities, ["github.com/example-org/demo"]);
}

#[tokio::test]
async fn project_add_persists_to_add_keeping_comments_registers_non_git_folders_by_path_and_remove_excludes_0109_0111()
{
	let j = temp_jocasta();
	let file = write_projects_yaml("version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n");
	let notes = root(&j).join("notes");
	fs::create_dir_all(&notes).unwrap();
	let db = open_catalog().unwrap();
	let added = add_project(&db, &notes, None).await.unwrap().project;
	assert!(added.identity.starts_with("path:"));
	assert!(!added.git);
	let text = fs::read_to_string(&file).unwrap();
	assert!(text.contains("version: 1 # keep me\n# adds below\nadd:\n  - /nowhere/else # old\n  - "), "{text}");
	assert_eq!(read_projects_file(Some(&file)).unwrap().add.len(), 2);

	let removed = remove_project(&db, &added.name, None).await.unwrap();
	assert_eq!(removed.id, added.id);
	assert_eq!(list_projects(&db, None).unwrap(), []);
	let after = read_projects_file(Some(&file)).unwrap();
	assert_eq!(after.add, ["/nowhere/else"]);
	assert_eq!(after.exclude.len(), 1);
	assert!(fs::read_to_string(&file).unwrap().contains("# adds below"));
}

#[tokio::test]
async fn removing_a_scanned_repo_excludes_it_so_the_next_scan_skips_it_adding_it_again_lifts_the_exclusion() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	let repo = make_repo(&code.join("demo"), Some(DEMO));
	write_projects_yaml(&roots_yaml(&code, ""));
	let db = open_catalog().unwrap();
	sync_catalog(&db, None).await.unwrap();
	remove_project(&db, &project_id_for("github.com/example-org/demo"), None).await.unwrap();
	sync_catalog(&db, None).await.unwrap();
	assert_eq!(list_projects(&db, None).unwrap(), []);
	add_project(&db, &repo, None).await.unwrap();
	assert_eq!(names(&db), ["demo"]);
	assert!(read_projects_file(None).unwrap().exclude.is_empty());
}

#[tokio::test]
async fn project_add_on_a_subfolder_registers_the_repos_top_level() {
	let j = temp_jocasta();
	let repo = make_repo(&root(&j).join("demo"), Some(DEMO));
	fs::create_dir_all(repo.join("src")).unwrap();
	let db = open_catalog().unwrap();
	assert_eq!(add_project(&db, &repo.join("src"), None).await.unwrap().project.main_path, repo);
}

#[tokio::test]
async fn home_and_drive_roots_are_never_projects_0106() {
	let j = temp_jocasta();
	let db = open_catalog().unwrap();
	let home = std::env::home_dir().unwrap();
	assert!(add_project(&db, &home, None).await.unwrap_err().is_user());
	let top = root(&j).ancestors().last().unwrap().to_path_buf();
	assert!(add_project(&db, &top, None).await.unwrap_err().is_user());
}

#[tokio::test]
async fn remove_refuses_an_unknown_name_and_an_ambiguous_one() {
	let j = temp_jocasta();
	write_projects_yaml("version: 1\n");
	let one = root(&j).join("x").join("api");
	let two = root(&j).join("y").join("api");
	fs::create_dir_all(&one).unwrap();
	fs::create_dir_all(&two).unwrap();
	let db = open_catalog().unwrap();
	add_project(&db, &one, None).await.unwrap();
	add_project(&db, &two, None).await.unwrap();
	assert!(remove_project(&db, "nope", None).await.unwrap_err().is_user());
	let ambiguous = remove_project(&db, "api", None).await.unwrap_err();
	assert!(ambiguous.is_user());
	assert!(ambiguous.to_string().contains("2 projects are named api"), "{ambiguous}");
}

#[tokio::test]
async fn names_renames_a_project_by_id_0112() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	make_repo(&code.join("demo"), Some(DEMO));
	let id = project_id_for("github.com/example-org/demo");
	write_projects_yaml(&roots_yaml(&code, &format!("names:\n  {id}: demo-web\n")));
	let db = open_catalog().unwrap();
	sync_catalog(&db, None).await.unwrap();
	assert_eq!(names(&db), ["demo-web"]);
}

#[test]
fn list_edits_rewrite_a_flow_sequence_as_a_block_and_keep_crlf_line_endings() {
	let _j = temp_jocasta();
	let file = write_projects_yaml("version: 1\r\nexclude: [] # none yet\r\nroots: []\r\n");
	assert!(add_to_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert!(!add_to_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert_eq!(
		fs::read_to_string(&file).unwrap(),
		"version: 1\r\nexclude: # none yet\r\n  - D:/old-fork\r\nroots: []\r\n"
	);
	assert!(remove_from_list(ListKey::Exclude, "D:/old-fork", Some(&file)).unwrap());
	assert!(read_projects_file(Some(&file)).unwrap().exclude.is_empty());
}

#[tokio::test]
async fn a_malformed_projects_yaml_stops_with_one_line_naming_the_file_and_line_exit_1_0582() {
	let _j = temp_jocasta();
	let file = write_projects_yaml("version: 1\nroots:\n  - path: D:/code\n    depth: 0\n");
	let db = open_catalog().unwrap();
	let depth = sync_catalog(&db, None).await.unwrap_err();
	let Error::User { message, exit_code } = depth else { panic!("not a user error: {depth}") };
	assert_eq!(exit_code, 1);
	assert_eq!(message, format!("{}:4: roots[0].depth must be >= 1", file.display()));
	write_projects_yaml("version: 1\nadd: [unclosed\n");
	// The parser's own reason differs from Bun's ("Unexpected token"); the file, line and exit code are the contract.
	let Error::User { message, exit_code } = read_projects_file(Some(&file)).unwrap_err() else {
		panic!("not a user error")
	};
	assert_eq!(exit_code, 1);
	assert!(message.starts_with(&format!("{}:2: ", file.display())), "{message}");
}

#[tokio::test]
async fn project_add_on_a_linked_worktree_adds_its_main_checkout_and_writes_add_only_for_that_0582() {
	let j = temp_jocasta();
	write_projects_yaml("version: 1\n");
	let main = make_repo(&root(&j).join("acme-web"), Some(DEMO));
	let worktree = root(&j).join("acme-web.wt").join("feature");
	sh(&main, &["worktree", "add", "-q", "-b", "feature", &worktree.display().to_string()]);
	let db = open_catalog().unwrap();
	let added = add_project(&db, &worktree, None).await.unwrap();
	assert_eq!(added.project.main_path, main);
	assert_eq!(added.main_checkout_of, Some(real_path(&worktree)));
	assert_eq!(read_projects_file(None).unwrap().add, [forward(&main)]);
	let ids: Vec<String> = list_projects(&db, None).unwrap().into_iter().map(|p| p.id).collect();
	assert_eq!(ids, [project_id_for("github.com/example-org/demo")]);
}

#[tokio::test]
async fn a_repo_first_seen_before_its_first_commit_becomes_git_once_it_has_one_shr_2() {
	let j = temp_jocasta();
	let fresh = root(&j).join("fresh");
	fs::create_dir_all(&fresh).unwrap();
	sh(&fresh, &["init", "-q", "-b", "main"]);
	write_projects_yaml("version: 1\n");
	let db = open_catalog().unwrap();
	let before = add_project(&db, &fresh, None).await.unwrap().project;
	assert!(!before.git);
	assert!(before.identity.starts_with("path:"));
	fs::write(fresh.join("README.md"), "fresh\n").unwrap();
	sh(&fresh, &["add", "."]);
	sh(&fresh, &["commit", "-q", "-m", "first"]);
	sync_catalog(&db, None).await.unwrap();
	let after = list_projects(&db, None).unwrap().remove(0);
	assert!(after.git);
	assert_eq!(after.identity, format!("root:{}", sh(&fresh, &["rev-list", "--max-parents=0", "HEAD"])));
	assert_eq!(after.main_path, real_path(&fresh));
}
