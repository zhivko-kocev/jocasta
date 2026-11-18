use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::shared::paths::projects_file;
use crate::shared::schema::open_catalog;
use crate::shared::testing::{TempJocasta, temp_jocasta};

fn sh(dir: &Path, args: &[&str]) -> String {
	let mut full = vec!["-c", "user.name=Test", "-c", "user.email=test@example.com"];
	full.extend_from_slice(args);
	git(dir, &full).unwrap().unwrap_or_else(|| panic!("git {args:?} failed in {}", dir.display()))
}

fn make_repo(dir: &Path, commit: bool) -> PathBuf {
	fs::create_dir_all(dir).unwrap();
	sh(dir, &["init", "-q", "-b", "main"]);
	if commit {
		fs::write(dir.join("README.md"), "demo\n").unwrap();
		sh(dir, &["add", "."]);
		sh(dir, &["commit", "-q", "-m", &format!("init {}", dir.file_name().unwrap().to_string_lossy())]);
	}
	real_path(dir)
}

fn root(j: &TempJocasta) -> PathBuf {
	real_path(j.root())
}

fn write_roots(code: &Path) {
	let file = projects_file();
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	let text = format!(
		"version: 1\nroots:\n  - path: {}\n    depth: 2\n    ignore: [\"*.wt\"]\n",
		code.display().to_string().replace('\\', "/")
	);
	fs::write(file, text).unwrap();
}

struct Heard {
	problems: Arc<Mutex<Vec<String>>>,
	synced: Arc<Mutex<u32>>,
}

fn places(db: &SharedCatalog) -> (Places, Heard) {
	let problems = Arc::new(Mutex::new(Vec::new()));
	let synced = Arc::new(Mutex::new(0));
	let hooks = PlacesHooks {
		problem: Box::new({
			let problems = Arc::clone(&problems);
			move |text| problems.lock().unwrap().push(text.to_owned())
		}),
		synced: Box::new({
			let synced = Arc::clone(&synced);
			move || *synced.lock().unwrap() += 1
		}),
	};
	(Places::new(db.clone(), hooks), Heard { problems, synced })
}

#[test]
fn a_checkout_names_its_top_main_checkout_and_branch_and_a_worktree_names_its_main() {
	let j = temp_jocasta();
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	let main = make_repo(&root(&j).join("demo"), true);
	let place = places.checkout(&main).unwrap().unwrap();
	assert_eq!(
		(place.top.clone(), place.main.clone(), place.branch.as_deref()),
		(main.clone(), main.clone(), Some("main"))
	);
	let wt = root(&j).join("demo.wt").join("x");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/x", &wt.to_string_lossy()]);
	let place = places.checkout(&wt).unwrap().unwrap();
	assert_eq!(real_path(&place.top), real_path(&wt));
	assert_eq!(real_path(&place.main), main);
	assert_eq!(place.branch.as_deref(), Some("feature/x"));
	// A subfolder sits in the same checkout.
	fs::create_dir_all(main.join("sub")).unwrap();
	assert_eq!(places.checkout(&main.join("sub")).unwrap().unwrap().top, main);
}

#[test]
fn a_repo_without_a_commit_names_its_branch_and_a_detached_head_has_none() {
	let j = temp_jocasta();
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	let fresh = make_repo(&root(&j).join("fresh"), false);
	let place = places.checkout(&fresh).unwrap().unwrap();
	assert_eq!((place.top, place.branch.as_deref()), (fresh, Some("main")));
	let detached = make_repo(&root(&j).join("detached"), true);
	sh(&detached, &["checkout", "-q", "--detach"]);
	assert_eq!(places.checkout(&detached).unwrap().unwrap().branch, None);
}

#[test]
fn a_folder_outside_git_or_gone_has_no_checkout_and_a_checkout_is_kept_until_forget() {
	let j = temp_jocasta();
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	let plain = root(&j).join("plain");
	fs::create_dir_all(&plain).unwrap();
	assert_eq!(places.checkout(&plain).unwrap(), None);
	assert_eq!(places.checkout(&root(&j).join("gone")).unwrap(), None);
	let repo = make_repo(&plain, true);
	// Kept: still the answer given before it was a repo.
	assert_eq!(places.checkout(&repo).unwrap(), None);
	places.forget();
	assert!(places.checkout(&repo).unwrap().is_some());
}

#[tokio::test]
async fn a_folder_names_its_watched_project_through_worktrees_and_removed_threads_and_only_the_main_is_the_pane() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	let main = make_repo(&code.join("demo"), true);
	let wt = code.join("demo.wt").join("x");
	sh(&main, &["worktree", "add", "-q", "-b", "feature/x", &wt.to_string_lossy()]);
	fs::create_dir_all(code.join("loose")).unwrap();
	write_roots(&code);
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, heard) = places(&db);
	places.sync().await;
	assert_eq!(*heard.synced.lock().unwrap(), 1);
	let project = places.project_at(&main).unwrap().unwrap();
	assert_eq!(project.name, "demo");
	assert_eq!(places.project_at(&wt).unwrap().unwrap().id, project.id);
	assert_eq!(places.project_at(&main.join("missing")).unwrap(), None);
	assert_eq!(places.project_at(&code.join("loose")).unwrap(), None);
	assert_eq!(places.project_pane_at(&main).unwrap().unwrap().id, project.id);
	assert_eq!(places.project_pane_at(&wt).unwrap(), None);
	// A worktree removed by `thread_finish` still names its project (CO-2), also for a folder inside it.
	let (fresh, _) = places_over(&db);
	fresh.removed(&wt, &project);
	fs::remove_dir_all(&wt).unwrap();
	assert_eq!(fresh.project_at(&wt).unwrap().unwrap().id, project.id);
	assert_eq!(fresh.project_at(&wt.join("src").join("deep")).unwrap().unwrap().id, project.id);
	assert_eq!(fresh.project_at(&code.join("loose").join("x")).unwrap(), None);
}

fn places_over(db: &SharedCatalog) -> (Places, Heard) {
	places(db)
}

#[tokio::test]
async fn the_trunk_is_the_remotes_default_then_main_master_or_trunk_then_the_main_checkouts_branch() {
	let j = temp_jocasta();
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	let project = |main: &Path| Project {
		id: main.display().to_string(),
		identity: String::new(),
		name: "p".to_owned(),
		main_path: main.to_path_buf(),
		git: true,
		paths: Vec::new(),
	};
	let on_main = make_repo(&root(&j).join("a"), true);
	assert_eq!(places.trunk(&project(&on_main)).await.unwrap().as_deref(), Some("main"));
	let master = make_repo(&root(&j).join("b"), true);
	sh(&master, &["branch", "-m", "master"]);
	assert_eq!(places.trunk(&project(&master)).await.unwrap().as_deref(), Some("master"));
	// Neither main, master nor trunk: the checked-out branch.
	let odd = make_repo(&root(&j).join("c"), true);
	sh(&odd, &["branch", "-m", "develop"]);
	assert_eq!(places.trunk(&project(&odd)).await.unwrap().as_deref(), Some("develop"));
	// The remote's default branch wins over main.
	let remote = make_repo(&root(&j).join("d"), true);
	sh(&remote, &["branch", "release"]);
	sh(&remote, &["update-ref", "refs/remotes/origin/release", "release"]);
	sh(&remote, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/release"]);
	assert_eq!(places.trunk(&project(&remote)).await.unwrap().as_deref(), Some("release"));
	// No commit and no branch to name: none.
	let empty = root(&j).join("e");
	fs::create_dir_all(&empty).unwrap();
	assert_eq!(places.trunk(&project(&empty)).await.unwrap(), None);
	// Kept: a branch renamed afterwards is not seen until `forget`.
	sh(&on_main, &["branch", "-m", "trunk"]);
	assert_eq!(places.trunk(&project(&on_main)).await.unwrap().as_deref(), Some("main"));
	places.forget();
	assert_eq!(places.trunk(&project(&on_main)).await.unwrap().as_deref(), Some("trunk"));
}

#[tokio::test]
async fn find_takes_an_id_or_a_name_in_any_case_and_refuses_an_unknown_one() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	make_repo(&code.join("Demo"), true);
	write_roots(&code);
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	places.sync().await;
	let project = places.find(" demo ").unwrap();
	assert_eq!(project.name, "Demo");
	assert_eq!(places.find(&project.id).unwrap().id, project.id);
	let Err(Error::User { message, .. }) = places.find("nope") else { panic!("an unknown project is a refusal") };
	assert_eq!(message, "no project nope.");
}

#[tokio::test]
async fn a_malformed_projects_file_keeps_the_last_good_list_and_is_said_once_until_it_changes() {
	let j = temp_jocasta();
	let code = root(&j).join("code");
	make_repo(&code.join("demo"), true);
	write_roots(&code);
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, heard) = places(&db);
	places.sync().await;
	assert_eq!(places.listed().len(), 1);
	fs::write(projects_file(), "roots: [unclosed\n").unwrap();
	assert_eq!(places.listed().len(), 1);
	assert_eq!(places.listed().len(), 1);
	places.sync().await;
	assert_eq!(heard.problems.lock().unwrap().len(), 1);
	assert!(heard.problems.lock().unwrap()[0].starts_with("projects.yaml: "));
	assert!(heard.problems.lock().unwrap()[0].ends_with("; showing the last good list."));
	// Fixed: the list is read again, and the same problem later is said again.
	write_roots(&code);
	assert_eq!(places.listed().len(), 1);
	fs::write(projects_file(), "roots: [unclosed\n").unwrap();
	places.listed();
	assert_eq!(heard.problems.lock().unwrap().len(), 2);
}

#[test]
fn places_expire_only_once_they_are_older_than_ten_minutes() {
	let j = temp_jocasta();
	let db: SharedCatalog = Arc::new(Mutex::new(open_catalog().unwrap()));
	let (places, _) = places(&db);
	let plain = root(&j).join("plain");
	fs::create_dir_all(&plain).unwrap();
	assert_eq!(places.checkout(&plain).unwrap(), None);
	let repo = make_repo(&plain, true);
	let born = places.state().places_at;
	places.expire(born + PLACES_KEPT_MS);
	assert_eq!(places.checkout(&repo).unwrap(), None);
	places.expire(born + PLACES_KEPT_MS + 1);
	assert!(places.checkout(&repo).unwrap().is_some());
}
