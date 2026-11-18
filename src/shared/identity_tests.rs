use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::shared::git::git;
use crate::shared::testing::temp_jocasta;

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
	dir.canonicalize().unwrap()
}

#[test]
fn normalizes_origins_0103() {
	for (url, key) in [
		("https://github.com/example-org/demo.git", "github.com/example-org/demo"),
		("git@github.com:example-org/demo.git", "github.com/example-org/demo"),
		("ssh://git@GitHub.com:22/Example-Org/Demo.git", "github.com/example-org/demo"),
		("https://user:token@github.com/example-org/demo/", "github.com/example-org/demo"),
		("https://dev.azure.com/acme/Web/_git/api", "dev.azure.com/acme/web/api"),
		("https://acme@dev.azure.com/acme/Web/_git/api", "dev.azure.com/acme/web/api"),
		("https://acme.visualstudio.com/Web/_git/api", "dev.azure.com/acme/web/api"),
		("https://acme.visualstudio.com/DefaultCollection/Web/_git/api", "dev.azure.com/acme/web/api"),
		("git@ssh.dev.azure.com:v3/acme/Web/api", "dev.azure.com/acme/web/api"),
		("acme@vs-ssh.visualstudio.com:v3/acme/Web/api", "dev.azure.com/acme/web/api"),
		("https://dev.azure.com/acme/My%20Project/_git/api", "dev.azure.com/acme/my project/api"),
		// A stray `%` keeps the path as written instead of failing out of the catalog's sync (SHR-1).
		("https://dev.azure.com/acme/50%-off/_git/web", "dev.azure.com/acme/50%-off/web"),
	] {
		assert_eq!(normalize_origin(url).unwrap(), key, "{url}");
	}
}

#[test]
fn a_local_path_or_file_url_origin_is_its_own_lowercased_key() {
	assert_eq!(normalize_origin("D:\\Code\\Demo.git\\").unwrap(), "d:/code/demo");
	assert_eq!(normalize_origin("file:///C:/Repos/Demo.git").unwrap(), "file:///c:/repos/demo");
	assert!(normalize_origin("https://").is_err());
}

#[tokio::test]
async fn identity_precedence_origin_then_the_first_root_commit_then_the_path_0103() {
	let j = temp_jocasta();
	let with_origin = make_repo(&j.root().join("a"), Some("https://github.com/example-org/demo.git"));
	assert_eq!(identity_key(&with_origin).await.unwrap(), "github.com/example-org/demo");

	let no_origin = make_repo(&j.root().join("b"), None);
	let root = sh(&no_origin, &["rev-list", "--max-parents=0", "HEAD"]);
	assert_eq!(identity_key(&no_origin).await.unwrap(), format!("root:{root}"));

	let empty = j.root().join("c");
	fs::create_dir_all(&empty).unwrap();
	sh(&empty, &["init", "-q"]);
	let key = identity_key(&empty).await.unwrap();
	assert!(key.starts_with("path:") && key.ends_with("/c"), "{key}");
}

#[test]
fn project_ids_are_p_plus_16_hex_of_sha_256_of_the_key_names_come_from_origin_else_the_folder_0107_0110() {
	let id = project_id_for("github.com/example-org/demo");
	assert!(id.len() == 18 && id.starts_with("p-") && id[2..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
	assert_eq!(project_id_for("github.com/example-org/demo"), id);
	assert_ne!(project_id_for("github.com/example-org/other"), id);
	assert_eq!(default_project_name("github.com/example-org/demo", Path::new("D:/code/demo-clone")), "demo");
	assert_eq!(default_project_name("root:abc", &Path::new("x").join("notes")), "notes");
}
