//! The cases of `test/frontier/server/pane-check.test.ts` through the real server: a project pane's turn-end check
//! (0617) asked as the bridge's `stop` request, over a stand-in mise whose `verify` task is the check
//! (`fake-mise-verify.ts`, 0630).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use regex::Regex;

use crate::frontier::server::bridge::BridgeService;
use crate::frontier::server::helpers::{
	TestServer, boss_pane, commit_in, demo_project, start_thread, until, with_server,
};
use crate::frontier::server::mise::tests::{captured_shim, stand_in};
use crate::frontier::server::server::ServerOptions;
use crate::frontier::server::wording::IDLE_CHECK_FRAME;
use crate::shared::env;
use crate::shared::layer::LAYER_ENV;
use crate::shared::pathkey::same_dir;
use crate::shared::process::AbortSignal;
use crate::shared::testing::{TempJocasta, temp_jocasta};

/// A stand-in for mise in these cases: a trusted project whose `verify` task is the check. `run -c verify` counts
/// itself in `$FAKE_VERIFY_DIR/runs`, waits `delay` milliseconds, then exits with `exit` (both files in that folder,
/// read when it starts; 0 when absent). A run that starts while another is under way adds a line to `overlaps`. Does
/// nothing in a normal test run.
#[test]
fn verify_mise() {
	stand_in(|args| {
		let dir = PathBuf::from(env::var("FAKE_VERIFY_DIR").unwrap_or_default());
		if args == "run -c verify" {
			let number = |name: &str| {
				fs::read_to_string(dir.join(name)).ok().and_then(|text| text.trim().parse::<u64>().ok()).unwrap_or(0)
			};
			let append = |name: &str| {
				let mut file =
					fs::OpenOptions::new().create(true).append(true).open(dir.join(name)).expect("a control file");
				file.write_all(if name == "runs" { b"run\n" } else { b"overlap\n" }).expect("writing a control file");
			};
			let (delay, exit) = (number("delay"), number("exit"));
			let active = dir.join("active");
			if active.exists() {
				append("overlaps");
			}
			fs::write(&active, "").expect("the active mark");
			append("runs");
			std::thread::sleep(Duration::from_millis(delay));
			let _ = fs::remove_file(&active);
			return (String::new(), "verify: boom\n".to_owned(), i32::try_from(exit).unwrap_or(1));
		}
		let answer = match args.as_str() {
			"settings get paranoid" => "true\n",
			"trust --show" | "env --json" if args == "trust --show" => "",
			"tasks ls --json" => r#"[{"name":"verify"}]"#,
			"env --json" => "{}",
			other if other.starts_with("trust ") && !other.starts_with("trust --") => "",
			other => return (String::new(), format!("fake mise: no answer for {other}\n"), 1),
		};
		(answer.to_owned(), String::new(), 0)
	});
}

/// What a test of a project pane's check works with.
#[derive(Clone)]
struct Fixture {
	server: TestServer,
	/// The project pane, in the project's main checkout.
	pane: String,
	/// The thread started in the project: its pane is no project pane.
	thread: String,
	/// The project's main checkout.
	main: PathBuf,
	/// The fake `verify` task's controls.
	dir: PathBuf,
}

impl Fixture {
	/// Sets the fake `verify` task's `exit` or `delay`, read when a run starts.
	fn control(&self, name: &str, value: u64) {
		fs::write(self.dir.join(name), value.to_string()).unwrap();
	}

	/// How many times `verify` ran.
	fn runs(&self) -> usize {
		line_count(&self.dir.join("runs"))
	}

	/// How many runs started while another was under way.
	fn overlaps(&self) -> usize {
		line_count(&self.dir.join("overlaps"))
	}

	/// The pane's check as the bridge's `stop` request asks for it, until `signal` aborts; "" for nothing.
	async fn ask_with(&self, pane: &str, signal: AbortSignal) -> String {
		let key = self
			.server
			.herdr
			.env_of(pane)
			.and_then(|env| env.into_iter().find(|(name, _)| name == "JOCASTA_PANE_KEY").map(|(_, key)| key))
			.unwrap_or_else(|| "no-such-key".to_owned());
		BridgeService::stop(&*self.server.service, key, signal).await
	}

	/// The pane's check, its turn never aborted.
	async fn ask(&self, pane: &str) -> String {
		let (hold, signal) = tokio::sync::watch::channel(None::<String>);
		let text = self.ask_with(pane, signal).await;
		drop(hold);
		text
	}

	/// Today's activity lines.
	fn today(&self) -> Vec<String> {
		self.server.service.day().dashboard().activity.events.into_iter().map(|event| event.text).collect()
	}

	/// How many of today's lines match `pattern`.
	fn today_matching(&self, pattern: &Regex) -> usize {
		self.today().iter().filter(|line| pattern.is_match(line)).count()
	}
}

/// How many lines `file` has; 0 when it is absent.
fn line_count(file: &Path) -> usize {
	fs::read_to_string(file).map_or(0, |text| text.trim().split('\n').count())
}

/// A service over a demo project with a thread (so the project pane exists) and a `verify` task that passes.
async fn with_project_pane<F, Fut>(j: &mut TempJocasta, wait_ms: Option<i64>, run: F)
where
	F: FnOnce(Fixture) -> Fut,
	Fut: Future<Output = ()>,
{
	let project = demo_project(j.root(), "demo").await;
	let dir = j.root().join("verify");
	fs::create_dir_all(&dir).unwrap();
	j.set_var("FAKE_VERIFY_DIR", dir.to_str());
	let shims = j.root().join("fake-mise");
	fs::create_dir_all(&shims).unwrap();
	let shim = captured_shim(&shims, "mise", "frontier::server::e2e_pane_check_tests::verify_mise");
	let options = ServerOptions {
		mise: Some(Some(vec![shim.display().to_string()])),
		pane_check_wait_ms: wait_ms,
		..ServerOptions::default()
	};
	with_server(options, |server| async move {
		let started = start_thread(&server, Default::default()).await;
		let pane = server
			.herdr
			.state()
			.pane_list
			.iter()
			.find(|pane| pane.cwd.as_deref().is_some_and(|cwd| same_dir(cwd, &project.main_path)))
			.map(|pane| pane.pane_id.clone())
			.expect("the project pane");
		run(Fixture { server, pane, thread: started.pane_id, main: project.main_path, dir }).await;
	})
	.await;
}

/// Edits the project's README, so its tree differs from the last one.
fn edit(main: &Path, text: &str) {
	fs::write(main.join("README.md"), format!("demo\n{text}\n")).unwrap();
}

fn starts_with_failing_check(text: &str) -> bool {
	text.starts_with(&format!("{IDLE_CHECK_FRAME}\n\nchecks: FAIL"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_panes_turn_end_with_edits_not_yet_checked_gets_the_failing_check_in_the_threads_frame_once_per_tree_a_pass_and_a_tree_with_no_edits_say_nothing_and_cost_no_run_0617()
 {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, None, |f| async move {
		// No edits since HEAD: nothing to check.
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 0);

		f.control("exit", 1);
		edit(&f.main, "one");
		let text = f.ask(&f.pane).await;
		assert!(starts_with_failing_check(&text), "{text}");
		assert_eq!(f.runs(), 1);
		// The event is called again after every settle, the continuation's included: the same tree is never answered twice.
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 1);

		// A pass says nothing; the tree is not checked again.
		f.control("exit", 0);
		edit(&f.main, "two");
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 2);

		// Committed: the tree is HEAD's.
		commit_in(&f.main, "demo\ntwo\n", "Add two");
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 2);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_threads_key_the_bosss_key_and_an_unknown_key_get_nothing_and_run_nothing_0617() {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, None, |f| async move {
		f.control("exit", 1);
		edit(&f.main, "one");
		assert_eq!(f.ask(&f.thread).await, "");
		let boss = boss_pane(&f.server.herdr).expect("a boss pane");
		assert_eq!(f.ask(&boss).await, "");
		assert_eq!(f.ask("no such pane").await, "");
		assert_eq!(f.runs(), 0);
		// The project pane itself still gets it.
		assert!(f.ask(&f.pane).await.starts_with(IDLE_CHECK_FRAME));
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_layer_off_a_project_panes_check_runs_nothing_and_answers_nothing_0576_0617() {
	let mut j = temp_jocasta();
	j.set_var(LAYER_ENV, Some("off"));
	with_project_pane(&mut j, None, |f| async move {
		f.control("exit", 1);
		edit(&f.main, "one");
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 0);
	})
	.await;
}

fn failed_late() -> Regex {
	Regex::new(
		r"^demo — checks failed on tree [0-9a-f]{6} after its turn ended; its agent gets them at its next turn end$",
	)
	.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_that_outlasts_the_wait_lets_the_turn_end_its_failure_reaches_today_and_is_owed_once_to_the_next_request_on_the_same_tree_and_dropped_when_the_tree_changed_0617()
 {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, Some(200), |f| async move {
		f.control("exit", 1);
		f.control("delay", 1500);
		edit(&f.main, "one");
		assert_eq!(f.ask(&f.pane).await, "");
		until(|| f.runs() == 1).await;
		// Called again while the check runs: the same tree, no second run, nothing owed yet.
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 1);
		let late = failed_late();
		until(|| f.today_matching(&late) > 0).await;
		assert_eq!(f.today_matching(&late), 1);
		assert!(starts_with_failing_check(&f.ask(&f.pane).await));
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 1);

		// Slow again, and the tree changes before it ends: the owed failure is for a tree no longer there.
		edit(&f.main, "two");
		assert_eq!(f.ask(&f.pane).await, "");
		until(|| f.runs() == 2).await;
		f.control("exit", 0);
		f.control("delay", 0);
		edit(&f.main, "three");
		until(|| f.today_matching(&late) == 2).await;
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.ask(&f.pane).await, "");
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_end_on_a_new_tree_while_the_panes_last_check_still_runs_waits_for_it_two_checks_never_run_at_once_in_one_folder_0617()
 {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, Some(200), |f| async move {
		f.control("exit", 1);
		f.control("delay", 1500);
		edit(&f.main, "one");
		assert_eq!(f.ask(&f.pane).await, "");
		until(|| f.runs() == 1).await;
		edit(&f.main, "two");
		assert_eq!(f.ask(&f.pane).await, "");
		assert_eq!(f.runs(), 1);
		until(|| f.runs() == 2).await;
		let failed = Regex::new("checks failed").unwrap();
		until(|| f.today_matching(&failed) == 2).await;
		assert_eq!(f.overlaps(), 0);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_role_that_stops_asking_mid_wait_ends_the_services_wait_the_failure_then_reaches_today_and_is_owed_to_the_next_turn_end_on_the_tree_0617()
 {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, Some(60_000), |f| async move {
		f.control("exit", 1);
		f.control("delay", 1500);
		edit(&f.main, "one");
		let (abort, signal) = tokio::sync::watch::channel(None::<String>);
		let asking = f.clone();
		let asked = tokio::spawn(async move { asking.ask_with(&asking.pane, signal).await });
		until(|| f.runs() == 1).await;
		let _ = abort.send(Some("the stop hook was aborted".to_owned()));
		assert_eq!(asked.await.unwrap(), "");
		// Within the 60 s wait: had the service not seen the role leave, the failure would go to the gone request and
		// today would say nothing.
		let failed = Regex::new("checks failed").unwrap();
		until(|| f.today_matching(&failed) > 0).await;
		assert!(starts_with_failing_check(&f.ask(&f.pane).await));
		assert_eq!(f.runs(), 1);
	})
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_the_service_ends_a_pane_checks_wait_at_once_and_writes_no_today_line_0617() {
	let mut j = temp_jocasta();
	with_project_pane(&mut j, Some(18_000), |f| async move {
		f.control("exit", 1);
		f.control("delay", 20_000);
		edit(&f.main, "one");
		let asking = f.clone();
		let asked = tokio::spawn(async move { asking.ask(&asking.pane).await });
		until(|| f.runs() == 1).await;
		let started = Instant::now();
		f.server.stop().await;
		assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
		assert_eq!(asked.await.unwrap(), "");
		assert!(!f.today().iter().any(|line| line.contains("checks failed")));
	})
	.await;
}
