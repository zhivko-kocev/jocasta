//! Tests of `mise.rs`, and the stand-in programs this test executable plays (the Rust form of the TypeScript's
//! `fake-*.ts` programs, 0630): a shim named like the program, in a folder of the test's, that runs this executable
//! again on a test function with the shim's arguments in `FAKE_ARGS`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::shared::env;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// Marks a shim executable (POSIX).
#[cfg(unix)]
fn make_runnable(shim: &Path) {
	use std::os::unix::fs::PermissionsExt;
	std::fs::set_permissions(shim, std::fs::Permissions::from_mode(0o755)).expect("making the shim run");
}

/// A `.cmd` shim runs as it is (Windows).
#[cfg(windows)]
fn make_runnable(_shim: &Path) {}

/// A shim `name` in `dir` that runs the test function `test` with the shim's arguments in `FAKE_ARGS` and a file
/// prefix in `FAKE_OUT`, then plays back what the stand-in left there ([`stand_in`]): stdout, stderr and the exit code,
/// with none of the test harness's own output mixed in. Returns the shim's path.
pub(crate) fn captured_shim(dir: &Path, name: &str, test: &str) -> PathBuf {
	let exe = std::env::current_exe().expect("the test executable");
	let exe = exe.display();
	let dir = dir.display();
	if cfg!(windows) {
		let shim = Path::new(&dir.to_string()).join(format!("{name}.cmd"));
		let script = format!(
			"@echo off\r\nsetlocal\r\nset \"FAKE_ARGS=%*\"\r\n:next\r\nset \"FAKE_OUT={dir}\\{name}-%RANDOM%%RANDOM%\"\r\nmkdir \"%FAKE_OUT%.d\" 2>nul || goto next\r\nset FAKE_CODE=99\r\n\"{exe}\" --exact {test} -q --nocapture >nul 2>nul\r\ntype \"%FAKE_OUT%.out\"\r\ntype \"%FAKE_OUT%.err\" 1>&2\r\nset /p FAKE_CODE=<\"%FAKE_OUT%.code\"\r\ndel \"%FAKE_OUT%.out\" \"%FAKE_OUT%.err\" \"%FAKE_OUT%.code\" >nul 2>nul\r\nrmdir \"%FAKE_OUT%.d\"\r\nexit /b %FAKE_CODE%\r\n"
		);
		std::fs::write(&shim, script).expect("writing the shim");
		shim
	} else {
		let shim = Path::new(&dir.to_string()).join(name);
		let script = format!(
			"#!/bin/sh\nFAKE_ARGS=\"$*\"\nFAKE_OUT=\"{dir}/{name}-$$\"\nexport FAKE_ARGS FAKE_OUT\n\"{exe}\" --exact {test} -q --nocapture >/dev/null 2>&1\ncat \"$FAKE_OUT.out\"\ncat \"$FAKE_OUT.err\" >&2\nread code < \"$FAKE_OUT.code\"\nrm -f \"$FAKE_OUT.out\" \"$FAKE_OUT.err\" \"$FAKE_OUT.code\"\nexit \"${{code:-99}}\"\n"
		);
		std::fs::write(&shim, script).expect("writing the shim");
		make_runnable(&shim);
		shim
	}
}

/// The stand-in side of [`captured_shim`]: does nothing in a normal test run; when a shim started it, `answer` gets the
/// shim's arguments joined by spaces (a batch file's quoting of a path with spaces removed) and returns what the program prints: stdout, stderr and its exit code.
pub(crate) fn stand_in(answer: impl FnOnce(String) -> (String, String, i32)) {
	let Some(prefix) = env::var("FAKE_OUT") else { return };
	let (out, err, code) = answer(env::var("FAKE_ARGS").unwrap_or_default().replace('"', ""));
	std::fs::write(format!("{prefix}.out"), out).expect("writing the stand-in's stdout");
	std::fs::write(format!("{prefix}.err"), err).expect("writing the stand-in's stderr");
	std::fs::write(format!("{prefix}.code"), code.to_string()).expect("writing the stand-in's exit code");
}

/// The environment variable that stands for an empty answer, which a Windows shim can't carry.
const EMPTY: &str = "<empty>";

/// A stand-in for mise in the mise tests: answers `settings get paranoid`, `trust --show`, `env --json` and
/// `tasks ls --json` from `FAKE_MISE_*` variables, and logs each call with its `MISE_ENV` to `FAKE_MISE_LOG`.
/// `trust --show` answers from `FAKE_MISE_TRUST_IN` (JSON: folder to answer) when it names the folder mise runs in,
/// else `FAKE_MISE_TRUST`; `trust <file>` succeeds, and `run scorecard` (the weekly review's task, 0616) prints
/// `FAKE_MISE_SCORECARD`. An answer of `<empty>` is empty text.
#[test]
fn fake_mise() {
	stand_in(|args| {
		let cwd = std::env::current_dir().unwrap_or_default();
		if let Some(log) = env::var("FAKE_MISE_LOG") {
			let line = json!({ "args": args, "cwd": cwd.display().to_string(), "miseEnv": env::var("MISE_ENV") });
			let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log).expect("the mise log");
			file.write_all(format!("{line}\n").as_bytes()).expect("logging a mise call");
		}
		let answer_of = |name: &str| env::var(name).map(|value| if value == EMPTY { String::new() } else { value });
		let by_folder: Value =
			serde_json::from_str(&env::var("FAKE_MISE_TRUST_IN").unwrap_or_else(|| "{}".into())).unwrap_or(Value::Null);
		let here = by_folder
			.as_object()
			.into_iter()
			.flatten()
			.find(|(dir, _)| path_key(&real_path(Path::new(dir))) == path_key(&real_path(&cwd)));
		let answer = if args.starts_with("trust ") && !args.starts_with("trust --") {
			Some(String::new())
		} else {
			match args.as_str() {
				"settings get paranoid" => answer_of("FAKE_MISE_PARANOID"),
				"trust --show" => match here {
					Some((_, answer)) => answer.as_str().map(str::to_owned),
					None => answer_of("FAKE_MISE_TRUST"),
				},
				"env --json" => answer_of("FAKE_MISE_ENV_JSON"),
				"tasks ls --json" => answer_of("FAKE_MISE_TASKS"),
				"run scorecard" => Some(String::new()),
				_ => None,
			}
		};
		let Some(answer) = answer else { return (String::new(), format!("fake mise: no answer for {args}\n"), 1) };
		if args == "run scorecard" {
			// FAKE_MISE_SCORECARD is the task's output; it may sleep first (FAKE_MISE_SCORECARD_SLEEP_MS) and exit
			// with FAKE_MISE_SCORECARD_EXIT.
			let sleep: u64 = env::var("FAKE_MISE_SCORECARD_SLEEP_MS").and_then(|ms| ms.parse().ok()).unwrap_or(0);
			std::thread::sleep(std::time::Duration::from_millis(sleep));
			let exit = env::var("FAKE_MISE_SCORECARD_EXIT");
			let err = if exit.is_some() { "scorecard: boom\n" } else { "" };
			let code = exit.and_then(|code| code.parse().ok()).unwrap_or(0);
			return (env::var("FAKE_MISE_SCORECARD").unwrap_or_default(), err.to_owned(), code);
		}
		(answer, String::new(), 0)
	});
}

const PARANOID: &str = "[\"config\"]\n";
const TRUSTED: &str = "~/src/demo/mise.toml: trusted\n";

/// The test's folder, its fake mise, and the log of the calls it saw.
struct Fixture {
	temp: TempJocasta,
	mise: Vec<String>,
}

impl Fixture {
	fn new() -> Self {
		let temp = temp_jocasta();
		let shim = captured_shim(temp.root(), "mise", "frontier::server::mise::tests::fake_mise");
		let mise = vec![shim.to_string_lossy().into_owned()];
		let fixture = Self { temp, mise };
		let _ = std::fs::remove_file(fixture.log());
		fixture
	}

	fn root(&self) -> &Path {
		self.temp.root()
	}

	fn log(&self) -> PathBuf {
		self.root().join("mise.log")
	}

	fn mise(&self) -> Mise<'_> {
		Mise::Argv(&self.mise)
	}

	/// The env the fake mise answers from, with the pane's own env; never the `MISE_ENV` of a Jocasta pane they run in.
	fn base(&self, answers: &[(&str, &str)], extra: &[(&str, &str)]) -> Env {
		let mut base = process_env();
		base.remove("MISE_ENV");
		base.insert("FAKE_MISE_LOG".to_owned(), self.log().display().to_string());
		for (name, value) in answers.iter().chain(extra) {
			let value = if value.is_empty() { EMPTY } else { value };
			base.insert((*name).to_owned(), value.to_owned());
		}
		base
	}

	/// The calls the fake mise saw: its args and the `MISE_ENV` it ran with.
	fn calls(&self) -> Vec<(String, Option<String>)> {
		let Ok(text) = std::fs::read_to_string(self.log()) else { return Vec::new() };
		text.lines()
			.map(|line| serde_json::from_str::<Value>(line).expect("a log line"))
			.map(|call| {
				(call["args"].as_str().unwrap_or_default().to_owned(), call["miseEnv"].as_str().map(str::to_owned))
			})
			.collect()
	}

	fn trusts(&self) -> Vec<(String, Option<String>)> {
		self.calls().into_iter().filter(|(args, _)| args.starts_with("trust ") && args != "trust --show").collect()
	}
}

fn tasks(names: &[&str]) -> String {
	Value::Array(names.iter().map(|name| json!({ "name": name, "source": "x" })).collect()).to_string()
}

#[tokio::test]
async fn without_mise_on_path_a_panes_env_and_the_reviews_check_are_unchanged() {
	let f = Fixture::new();
	let base = f.base(&[], &[]);
	assert_eq!(mise_env("demo-1", f.root(), Some(&base), Mise::Absent).await, Env::new());
	assert_eq!(mise_check("demo-1", f.root(), Some(&base), Mise::Absent).await, None);
}

#[tokio::test]
async fn a_trusted_folder_under_paranoid_trust_gets_jocasta_pid_appended_to_mise_env_and_mises_env_is_merged() {
	let f = Fixture::new();
	let env_json = json!({ "PATH": "C:\\mise\\bun\\bin;C:\\Windows", "API_PORT": "3010", "MISE_ENV": "x" }).to_string();
	let answers = [("FAKE_MISE_PARANOID", PARANOID), ("FAKE_MISE_TRUST", TRUSTED), ("FAKE_MISE_ENV_JSON", &env_json)];
	let base = f.base(&answers, &[("MISE_ENV", "ci, jocasta-demo-1")]);
	let result = mise_env("demo-1", f.root(), Some(&base), f.mise()).await;
	assert_eq!(
		result,
		HashMap::from([
			("PATH".to_owned(), "C:\\mise\\bun\\bin;C:\\Windows".to_owned()),
			("API_PORT".to_owned(), "3010".to_owned()),
			("MISE_ENV".to_owned(), "ci,jocasta-demo-1".to_owned()),
		])
	);
	// mise itself ran with the project's environment selected, so Jocasta's own config for it loaded.
	assert!(f.calls().iter().all(|(_, mise_env)| mise_env.as_deref() == Some("ci,jocasta-demo-1")));
}

#[tokio::test]
async fn an_untrusted_repo_config_or_paranoid_trust_off_gives_only_mise_env_and_mises_env_is_never_read() {
	let f = Fixture::new();
	let only = Env::from([("MISE_ENV".to_owned(), "jocasta-demo-1".to_owned())]);
	let untrusted =
		f.base(&[("FAKE_MISE_PARANOID", PARANOID), ("FAKE_MISE_TRUST", "~/src/demo/mise.toml: untrusted\n")], &[]);
	assert_eq!(mise_env("demo-1", f.root(), Some(&untrusted), f.mise()).await, only);
	let relaxed = f.base(&[("FAKE_MISE_PARANOID", "false\n"), ("FAKE_MISE_TRUST", TRUSTED)], &[]);
	assert_eq!(mise_env("demo-1", f.root(), Some(&relaxed), f.mise()).await, only);
	assert!(!f.calls().iter().any(|(args, _)| args == "env --json"));
	assert!(f.trusts().is_empty());
}

#[tokio::test]
async fn the_reviews_mise_check_is_verify_else_test_only_for_a_folder_mise_may_load() {
	let f = Fixture::new();
	let check = |names: &[&str]| {
		let list = tasks(names);
		let base =
			f.base(&[("FAKE_MISE_PARANOID", PARANOID), ("FAKE_MISE_TRUST", ""), ("FAKE_MISE_TASKS", &list)], &[]);
		let (root, mise) = (f.root().to_path_buf(), f.mise.clone());
		async move { mise_check("demo-1", &root, Some(&base), Mise::Argv(&mise)).await }
	};
	assert_eq!(check(&["test", "verify"]).await, Some(MiseCheck::Task(MiseCheckTask::Verify)));
	assert_eq!(check(&["lint", "test"]).await, Some(MiseCheck::Task(MiseCheckTask::Test)));
	assert_eq!(check(&["lint"]).await, None);
}

#[tokio::test]
async fn a_repo_config_mise_may_not_load_says_why_the_projects_check_didnt_run_no_repo_config_says_nothing() {
	let f = Fixture::new();
	let list = tasks(&["verify"]);
	let untrusted = f.base(
		&[
			("FAKE_MISE_PARANOID", PARANOID),
			("FAKE_MISE_TRUST", "~/x/mise.toml: untrusted\n"),
			("FAKE_MISE_TASKS", &list),
		],
		&[],
	);
	assert_eq!(
		mise_check("demo-1", f.root(), Some(&untrusted), f.mise()).await,
		Some(MiseCheck::Skipped("~/x/mise.toml isn't trusted in mise".to_owned()))
	);
	let relaxed_answers = [
		("FAKE_MISE_PARANOID", "false\n"),
		("FAKE_MISE_TRUST", "C:\\src\\demo: trusted\r\n"),
		("FAKE_MISE_TASKS", list.as_str()),
	];
	let relaxed = f.base(&relaxed_answers, &[]);
	assert_eq!(
		mise_check("demo-1", f.root(), Some(&relaxed), f.mise()).await,
		Some(MiseCheck::Skipped("mise's paranoid config trust is off".to_owned()))
	);
	let none = f.base(&relaxed_answers, &[("FAKE_MISE_TRUST", "")]);
	assert_eq!(mise_check("demo-1", f.root(), Some(&none), f.mise()).await, None);
	// mise's tasks are never listed for a config it may not load.
	assert!(!f.calls().iter().any(|(args, _)| args == "tasks ls --json"));
}

/// A main checkout and a thread worktree, each with a root `mise.toml` and a `sub/mise.toml`, and mise's answers.
struct Checkouts {
	main: PathBuf,
	wt: PathBuf,
}

impl Checkouts {
	fn new(f: &Fixture, worktree_text: &str) -> Self {
		let main = f.root().join("demo");
		let wt = f.root().join("demo.wt").join("fix-x");
		for dir in [&main, &wt] {
			std::fs::create_dir_all(dir.join("sub")).unwrap();
		}
		std::fs::write(main.join("mise.toml"), "[tools]\nbun = \"1.4.3\"\n").unwrap();
		std::fs::write(main.join("sub").join("mise.toml"), "[tasks.x]\nrun = 'x'\n").unwrap();
		std::fs::write(wt.join("mise.toml"), worktree_text).unwrap();
		std::fs::write(wt.join("sub").join("mise.toml"), "[tasks.x]\nrun = 'x'\n").unwrap();
		Self { main, wt }
	}

	fn show(&self, dir: &Path, main_sub: &str, status: &str) -> String {
		let sub = if dir == self.main { main_sub } else { "untrusted" };
		format!(
			"{}: {status}\n{}: {sub}\n",
			dir.join("mise.toml").display(),
			dir.join("sub").join("mise.toml").display()
		)
	}

	fn answers(&self, main_sub: &str, paranoid: &str) -> Vec<(String, String)> {
		let by_folder = json!({
			self.main.display().to_string(): self.show(&self.main, main_sub, "trusted"),
			self.wt.display().to_string(): self.show(&self.wt, main_sub, "untrusted"),
		});
		vec![
			("FAKE_MISE_PARANOID".to_owned(), paranoid.to_owned()),
			("FAKE_MISE_TRUST_IN".to_owned(), by_folder.to_string()),
		]
	}
}

fn pairs(answers: &[(String, String)]) -> Vec<(&str, &str)> {
	answers.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect()
}

fn keys(paths: &[PathBuf]) -> Vec<String> {
	paths.iter().map(|path| path_key(path)).collect()
}

#[tokio::test]
async fn a_worktrees_byte_for_byte_copy_of_a_config_the_main_checkout_trusts_is_trusted_under_paranoid_trust_only() {
	let f = Fixture::new();
	let c = Checkouts::new(&f, "[tools]\nbun = \"1.4.3\"\n");
	let answers = c.answers("trusted", PARANOID);
	let base = f.base(&pairs(&answers), &[]);
	let trusted = trust_copies("demo-1", &c.main, &c.wt, Some(&base), f.mise()).await;
	let expected = [c.wt.join("mise.toml"), c.wt.join("sub").join("mise.toml")];
	assert_eq!(keys(&trusted), keys(&expected));
	let expected_calls: Vec<String> =
		expected.iter().map(|path| format!("trust {}", real_path(path).display())).collect();
	let trusts = f.trusts();
	assert_eq!(trusts.iter().map(|(args, _)| args.clone()).collect::<Vec<_>>(), expected_calls);
	assert!(trusts.iter().all(|(_, mise_env)| mise_env.as_deref() == Some("jocasta-demo-1")));
	std::fs::remove_file(f.log()).unwrap();
	let off = c.answers("trusted", "false\n");
	let relaxed = f.base(&pairs(&off), &[]);
	assert!(trust_copies("demo-1", &c.main, &c.wt, Some(&relaxed), f.mise()).await.is_empty());
	assert!(f.trusts().is_empty());
	assert!(trust_copies("demo-1", &c.main, &c.wt, Some(&f.base(&[], &[])), Mise::Absent).await.is_empty());
}

#[tokio::test]
async fn a_changed_copy_or_a_copy_of_a_config_the_main_checkout_doesnt_trust_stays_untrusted() {
	let f = Fixture::new();
	let c = Checkouts::new(&f, "[tools]\nbun = \"1.4.3\"\n[env]\nX = \"1\"\n");
	let answers = c.answers("untrusted", PARANOID);
	let base = f.base(&pairs(&answers), &[]);
	assert!(trust_copies("demo-1", &c.main, &c.wt, Some(&base), f.mise()).await.is_empty());
	assert!(f.trusts().is_empty());
}

#[tokio::test]
async fn configs_mise_shows_under_its_home_folder_as_tilde_on_linux_and_macos_are_found_too() {
	let f = Fixture::new();
	let c = Checkouts::new(&f, "[tools]\nbun = \"1.4.3\"\n");
	let root = f.root().display().to_string();
	let shown = |dir: &Path, status: &str| {
		let path = dir.join("mise.toml").display().to_string();
		format!("~{}: {status}\n", &path[root.len()..])
	};
	let by_folder = json!({
		c.main.display().to_string(): shown(&c.main, "trusted"),
		c.wt.display().to_string(): shown(&c.wt, "untrusted"),
	})
	.to_string();
	let answers = [("FAKE_MISE_PARANOID", PARANOID), ("FAKE_MISE_TRUST_IN", by_folder.as_str())];
	let base = f.base(&answers, &[("HOME", root.as_str())]);
	let trusted = trust_copies("demo-1", &c.main, &c.wt, Some(&base), f.mise()).await;
	assert_eq!(keys(&trusted), keys(&[c.wt.join("mise.toml")]));
}

#[test]
fn a_thrown_error_reads_as_javascripts_string_of_it_name_then_message() {
	assert_eq!(js_string(&Error::other("spawn x ENOENT")), "Error: spawn x ENOENT");
	assert_eq!(js_string(&Error::user("refused")), "UserError: refused");
}

#[tokio::test]
async fn a_mise_that_did_not_start_is_logged_with_its_error_prefix() {
	let _j = temp_jocasta();
	let cwd = crate::shared::testing::temp_dir("mise-cwd");
	let missing = cwd.path().join("no-such-mise").display().to_string();
	assert_eq!(run_mise(&[missing], &["ls"], cwd.path(), &HashMap::new()).await, None);
	let logged: String = std::fs::read_dir(crate::shared::log::logs_dir())
		.unwrap()
		.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
		.collect();
	assert!(logged.contains(" failed: Error: "), "{logged}");
}
