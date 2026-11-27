use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::shared::testing::temp_jocasta;

fn project(id: &str, main: &Path) -> Project {
	Project {
		id: id.to_owned(),
		identity: format!("path:{}", main.display()),
		name: "acme-api".to_owned(),
		main_path: main.to_path_buf(),
		git: true,
		paths: Vec::new(),
	}
}

fn git(cwd: &Path, args: &[&str]) -> String {
	let out = Command::new("git").args(args).current_dir(cwd).output().expect("git runs");
	assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
	String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with `mise.toml` as given, one commit; its head.
fn repo(root: &Path, mise_toml: &str) -> (PathBuf, String) {
	let main = root.join("acme-api");
	std::fs::create_dir_all(&main).unwrap();
	git(&main, &["init", "-q"]);
	std::fs::write(main.join("mise.toml"), mise_toml).unwrap();
	std::fs::write(main.join("untracked-secret.txt"), "never exported").unwrap();
	git(&main, &["add", "mise.toml"]);
	git(&main, &["-c", "user.name=t", "-c", "user.email=t@example.org", "commit", "-qm", "one"]);
	let head = git(&main, &["rev-parse", "HEAD"]);
	(main, head)
}

#[test]
fn the_container_gets_the_commit_read_only_the_cache_volume_and_no_host_home_or_socket_0708() {
	let files = vec![(PathBuf::from("/h/.config/mise/config.acme-api.toml"), "/etc/mise/conf.d/10-project.toml")];
	let argv = docker_args("jocasta-cr-1", Path::new("/tmp/x/src.tar"), "p/1", &files, "mise run test");
	let line = argv.join(" ");
	assert!(line.starts_with("docker run --rm --init --name jocasta-cr-1 "));
	assert!(line.contains("--mount type=bind,source=/tmp/x/src.tar,target=/src.tar,readonly"));
	assert!(line.contains("-v jocasta-cache-p-1:/cache"));
	assert!(line.contains(
		"--mount type=bind,source=/h/.config/mise/config.acme-api.toml,target=/etc/mise/conf.d/10-project.toml,readonly"
	));
	assert!(line.contains("-e MISE_TRUSTED_CONFIG_PATHS=/work"));
	assert!(!line.contains("docker.sock"));
	assert!(!argv.iter().any(|word| word.starts_with("HOME=") || word.contains(":/root")), "no home folder");
	assert_eq!(argv.iter().filter(|word| *word == "-v").count(), 1, "only the cache volume");
	assert!(argv.last().unwrap().ends_with("&& mise run test"));
	assert_eq!(argv[argv.len() - 4], image());
	assert_eq!(argv[argv.len() - 3..argv.len() - 1], ["sh", "-lc"], "the image has a shell and no entrypoint");
}

#[test]
fn the_image_is_built_locally_from_debian_slim_with_mise_copied_in_and_tagged_by_its_dockerfile_0708() {
	assert!(DOCKERFILE.starts_with("FROM debian:stable-slim\n"));
	for package in ["ca-certificates", "git", "tar", "xz-utils", "unzip", "curl", "build-essential", "pkg-config"] {
		assert!(DOCKERFILE.contains(&format!(" {package}")), "{package}");
	}
	assert!(DOCKERFILE.contains("--no-install-recommends") && DOCKERFILE.contains("rm -rf /var/lib/apt/lists/*"));
	assert!(DOCKERFILE.contains("COPY --from=jdxcode/mise:latest /usr/local/bin/mise /usr/local/bin/mise\n"));
	assert!(DOCKERFILE.ends_with("ENTRYPOINT []\n"), "sh -lc runs as given, not through mise");
	let tag = image();
	let hash = tag.strip_prefix("jocasta-cleanroom:").expect("the tag");
	assert!(hash.len() == 12 && hash.chars().all(|c| c.is_ascii_hexdigit()), "{tag}");
	assert_eq!(image(), tag, "the same Dockerfile gives the same tag");
	assert_eq!(build_args(&tag), ["docker", "build", "-t", tag.as_str(), "-"], "the Dockerfile on stdin, no context");
}

#[test]
fn the_check_script_says_its_task_or_none_and_the_tail_keeps_the_last_twenty_lines() {
	assert_eq!(task_of("x\njocasta-check: verify\nok"), Some("verify".to_owned()));
	assert_eq!(task_of("jocasta-check: none\n"), Some(NO_TASK.to_owned()));
	assert_eq!(task_of("mise ERROR install failed"), None);
	let long: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
	assert_eq!(tail(&long.join("\n")).lines().next(), Some("11"));
}

#[tokio::test]
async fn off_in_jocasta_yaml_or_for_the_repo_in_projects_yaml_is_not_possible_without_docker_0708() {
	let temp = temp_jocasta();
	let main = temp.root().join("acme-api");
	let config = temp.root().join("config");
	std::fs::create_dir_all(&config).unwrap();
	std::fs::write(config.join("projects.yaml"), format!("cleanRoom:\n  {}: off\n", main.display())).unwrap();
	match check(&project("p1", &main), "HEAD").await {
		Check::NotPossible(why) => assert!(why.contains("projects.yaml cleanRoom"), "{why}"),
		other => panic!("{other:?}"),
	}
	std::fs::write(config.join("projects.yaml"), "").unwrap();
	std::fs::write(config.join("jocasta.yaml"), "checks:\n  cleanRoom: off\n").unwrap();
	match check(&project("p1", &main), "HEAD").await {
		Check::NotPossible(why) => assert!(why.contains("checks.cleanRoom"), "{why}"),
		other => panic!("{other:?}"),
	}
}

/// Removes the test's cache volume when it ends.
struct Volume(String);

impl Drop for Volume {
	fn drop(&mut self) {
		let _ = Command::new("docker").args(["volume", "rm", "-f", &self.0]).output();
	}
}

#[tokio::test]
async fn a_failing_verify_task_runs_on_the_committed_tree_only_and_is_kept_per_tree_0708() {
	if !available().await {
		eprintln!("skipped: Docker isn't running here (docker info failed); the clean room needs it");
		return;
	}
	let temp = temp_jocasta();
	let toml = "[tasks.verify]\nrun = \"echo verify-ran; test ! -e untracked-secret.txt && echo no-secret; exit 3\"\n";
	let (main, head) = repo(temp.root(), toml);
	let id = format!("test-{}", uuid::Uuid::new_v4().simple());
	let _volume = Volume(volume_of(&id));
	let project = project(&id, &main);
	let Check::Ran { task, ran } = check(&project, &head).await else { panic!("the check didn't run") };
	assert_eq!((task.as_str(), ran.exit), ("verify", Some(3)), "{}", ran.output);
	assert!(ran.output.contains("verify-ran") && ran.output.contains("no-secret"), "{}", ran.output);
	let again = check(&project, &head).await;
	assert_eq!(again, Check::Ran { task, ran }, "the stored result answers");
}

#[tokio::test]
async fn a_project_without_verify_or_test_is_not_possible_0708() {
	if !available().await {
		eprintln!("skipped: Docker isn't running here (docker info failed); the clean room needs it");
		return;
	}
	let temp = temp_jocasta();
	let (main, head) = repo(temp.root(), "[tasks.build]\nrun = \"true\"\n");
	let id = format!("test-{}", uuid::Uuid::new_v4().simple());
	let _volume = Volume(volume_of(&id));
	assert_eq!(
		check(&project(&id, &main), &head).await,
		Check::NotPossible("the project has neither a verify nor a test task".to_owned())
	);
}
