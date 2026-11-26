//! Tests of `sessions.rs`, ported from `test/frontier/server/sessions.test.ts`.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::*;
use crate::shared::omp_session::set_mtime;
use crate::shared::testing::temp_jocasta;

fn now() -> i64 {
	(system_clock())()
}

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 1e-9, "{actual} is not {expected}");
}

/// A clock the test moves.
#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

impl TestClock {
	fn new(at: i64) -> Self {
		TestClock(Arc::new(AtomicI64::new(at)))
	}

	fn get(&self) -> i64 {
		self.0.load(Ordering::SeqCst)
	}

	fn set(&self, at: i64) {
		self.0.store(at, Ordering::SeqCst);
	}

	fn advance(&self, by: i64) {
		self.0.fetch_add(by, Ordering::SeqCst);
	}

	fn clock(&self) -> Clock {
		let shared = Arc::clone(&self.0);
		Arc::new(move || shared.load(Ordering::SeqCst))
	}
}

fn index_at(sessions: &Path, clock: &TestClock) -> SessionIndex {
	SessionIndex::new(Some(sessions.to_path_buf()), Some(clock.clock()))
}

fn assistant(text: &str, timestamp: i64, cost: Option<f64>) -> String {
	let mut message = json!({"role": "assistant", "timestamp": timestamp, "content": [{"type": "text", "text": text}]});
	if let Some(cost) = cost {
		message["usage"] = json!({"cost": {"total": cost}});
	}
	json!({"type": "message", "message": message}).to_string()
}

fn header(cwd: &str) -> String {
	json!({"type": "session", "version": 3, "id": "s", "cwd": cwd}).to_string()
}

fn title() -> String {
	json!({"type": "title", "v": 1, "title": "demo", "pad": ""}).to_string()
}

/// Writes a session (or a nested transcript) with its lines and an mtime.
fn write_at(file: &Path, lines: &[String], mtime_ms: i64) -> PathBuf {
	fs::create_dir_all(file.parent().unwrap()).unwrap();
	fs::write(file, format!("{}\n", lines.join("\n"))).unwrap();
	set_mtime(file, mtime_ms);
	file.to_path_buf()
}

fn write(file: &Path, lines: &[String]) -> PathBuf {
	write_at(file, lines, now())
}

fn append(file: &Path, text: &str) {
	fs::OpenOptions::new().append(true).open(file).unwrap().write_all(text.as_bytes()).unwrap();
}

/// A real delay: a replacing file must be born at a later time than the first, as on his disk.
fn pause() {
	std::thread::sleep(std::time::Duration::from_millis(30));
}

/// Today's spend as `spend.ts` computes it, without its 30 s listing throttle and stepped read of a new file: every
/// session written today is read, then the ledger totals the day. The cases of `spend.ts` itself wait for that module.
fn spent_today(index: &mut SessionIndex, at: i64) -> f64 {
	let today = local_day(at);
	for session in index.list(day_start(&today, 0)) {
		index.read(&session.file, false);
	}
	index.spent_on(&today)
}

#[test]
fn the_review_s_test_result_is_the_last_tests_line_of_an_assistant_message_with_its_time_0425() {
	assert_eq!(read_stats(&assistant("No tests line here.", 1, None)).tests, None);
	let text = [
		assistant("Ran the suite.\nTESTS: fail", 10, None),
		assistant("Ran it again after the note.\n\n- `TESTS: pass`", 20, None),
		assistant("Quoted, not reported: the line said TESTS: none", 30, None),
	]
	.join("\n");
	assert_eq!(read_stats(&text).tests, Some(TestsReport { result: TestsReported::Pass, at: 20, command: None }));
	assert_eq!(
		read_stats(&assistant("tests: NONE", 5, None)).tests,
		Some(TestsReport { result: TestsReported::None, at: 5, command: None })
	);
	assert_eq!(
		read_stats(&assistant("Done.\nTESTS: pass — `bun test --bail`", 7, None)).tests,
		Some(TestsReport { result: TestsReported::Pass, at: 7, command: Some("bun test --bail".to_owned()) })
	);
}

/// A bash call in an assistant message and its result: exit 0, a failure (an error result), or started in the
/// background.
fn bash_run(id: &str, command: &str, at: i64, outcome: &str) -> Vec<String> {
	let call = json!({
		"type": "message",
		"message": {
			"role": "assistant",
			"timestamp": at,
			"content": [{"type": "toolCall", "id": id, "name": "bash", "arguments": {"command": command}}],
		},
	});
	let mut result = json!({
		"role": "toolResult",
		"toolCallId": id,
		"toolName": "bash",
		"timestamp": at + 1,
		"content": [{"type": "text", "text": "output"}],
	});
	match outcome {
		"fail" => {
			result["isError"] = json!(true);
			result["details"] = json!({"exitCode": 1});
		}
		"background" => result["details"] = json!({"async": {"state": "running", "jobId": "j1", "type": "bash"}}),
		_ => {}
	}
	vec![call.to_string(), json!({"type": "message", "message": result}).to_string()]
}

fn checked(lines: &[String], since: i64) -> TestsResult {
	checked_tests(&read_stats(&lines.join("\n")), since)
}

fn with(mut lines: Vec<String>, line: String) -> Vec<String> {
	lines.push(line);
	lines
}

#[test]
fn a_review_s_tests_count_only_when_the_command_its_tests_line_names_ran_and_exited_0_0453() {
	let end = |line: &str| assistant(&format!("Reviewed.\n{line}"), 50, None);
	let ran = |command: &str, outcome: &str| bash_run("c1", command, 10, outcome);
	let pass = end("TESTS: pass — `bun test`");
	assert_eq!(checked(&with(ran("bun test", "ok"), pass.clone()), 0), TestsResult::Pass);
	// Whitespace differences and a longer command line still match.
	let spaced = with(ran("cd x &&  bun   test", "ok"), end("TESTS: pass — bun test"));
	assert_eq!(checked(&spaced, 0), TestsResult::Pass);
	assert_eq!(checked(&with(ran("bun test", "fail"), pass), 0), TestsResult::Fail);
	// The last run of the command decides.
	let mut last = ran("bun test", "fail");
	last.extend(bash_run("c2", "bun test", 20, "ok"));
	assert_eq!(checked(&with(last, end("TESTS: pass — bun test")), 0), TestsResult::Pass);
	let said = end("TESTS: pass — bun test");
	assert_eq!(checked(&with(ran("bun test", "background"), said.clone()), 0), TestsResult::Unknown);
	assert_eq!(checked(std::slice::from_ref(&said), 0), TestsResult::Unknown);
	assert_eq!(checked(&with(ran("bun test", "ok"), end("TESTS: pass")), 0), TestsResult::Unknown);
	assert_eq!(checked(&with(ran("bun test", "ok"), end("TESTS: fail — bun test")), 0), TestsResult::Fail);
	assert_eq!(checked(&[end("TESTS: none")], 0), TestsResult::None);
	assert_eq!(checked(&[assistant("No line.", 50, None)], 0), TestsResult::Unknown);
	// A run or a line from before the review began doesn't count.
	assert_eq!(checked(&with(ran("bun test", "ok"), said.clone()), 20), TestsResult::Unknown);
	assert_eq!(checked(&with(ran("bun test", "ok"), said), 60), TestsResult::Unknown);
}

#[test]
fn a_test_command_piped_or_followed_by_another_proves_no_pass_the_exit_code_is_the_last_command_s_0453() {
	let checked = |command: &str, outcome: &str| {
		let lines =
			with(bash_run("c1", command, 10, outcome), assistant("Reviewed.\nTESTS: pass — bun test", 50, None));
		checked(&lines, 0)
	};
	for masked in [
		"bun test 2>&1 | tail -20",
		"bun test || true",
		"bun test | tee out.log",
		"bun test; echo done",
		"bun test &",
		"bun test\necho done",
	] {
		assert_eq!(checked(masked, "ok"), TestsResult::Unknown, "{masked}");
	}
	// A masked run that failed still fails the review.
	assert_eq!(checked("bun test | tail -20", "fail"), TestsResult::Fail);
	for own in [
		"bun test > t.log 2>&1",
		"bun test 2>&1",
		"bun test && echo ok",
		"bun test -t \"a|b\"",
		"cd x && bun test",
		"bun test | tail; bun test",
	] {
		assert_eq!(checked(own, "ok"), TestsResult::Pass, "{own}");
	}
}

#[test]
fn side_calls_count_toward_cost_a_superseded_retry_costs_but_says_nothing_memory_recall_is_ignored_0452_0464() {
	let at = now();
	let iso = chrono::DateTime::from_timestamp_millis(at).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
	let stats = read_stats(
		&[
			assistant("First answer", at, Some(1.0)),
			json!({
				"type": "model_usage",
				"id": "u1",
				"timestamp": iso,
				"purpose": "memory",
				"provider": "anthropic",
				"model": "claude-haiku",
				"usage": {"cost": {"total": 0.25}},
			})
			.to_string(),
			json!({"type": "memory_recall", "id": "m1", "timestamp": iso, "text": "TESTS: fail"}).to_string(),
			json!({
				"type": "message",
				"message": {
					"role": "assistant",
					"timestamp": at + 1,
					"retryRecovery": {"kind": "auto-retry", "status": "superseded", "attempt": 1},
					"content": [{"type": "text", "text": "Half an answer\nTESTS: fail"}],
					"usage": {"cost": {"total": 0.5}},
				},
			})
			.to_string(),
		]
		.join("\n"),
	);
	close(stats.cost, 1.75);
	close(stats.cost_by_day[&local_day(at)], 1.75);
	assert_eq!(stats.last_line.as_deref(), Some("First answer"));
	assert_eq!(stats.tests, None);
}

#[test]
fn the_proposed_plan_s_file_stays_known_after_approval_a_plan_sent_back_is_counted_0453_0471() {
	let propose = json!({
		"type": "message",
		"message": {
			"role": "toolResult",
			"toolName": "bash",
			"timestamp": 5,
			"details": {"toolCommands": [{"tool": "propose", "mode": "execute", "exitCode": 0, "inner": {"title": "Plan", "planFilePath": "/plans/p.md"}}]},
		},
	})
	.to_string();
	let user = json!({"type": "message", "message": {"role": "user", "timestamp": 6, "content": "Smaller, please"}})
		.to_string();
	let approved = json!({"type": "mode_change", "mode": "none"}).to_string();
	let sent_back = read_stats(&[propose.clone(), user.clone()].join("\n"));
	assert_eq!(sent_back.plan_sent_back, 1);
	assert_eq!(sent_back.plan_file.as_deref(), Some("/plans/p.md"));
	let ok = read_stats(&[propose, approved, user].join("\n"));
	assert_eq!(ok.plan_sent_back, 0);
	assert_eq!(ok.plan_awaiting, None);
	assert_eq!(ok.plan_file.as_deref(), Some("/plans/p.md"));
}

#[test]
fn a_plan_is_proposed_by_a_direct_propose_result_or_a_propose_record_that_exited_0_and_by_nothing_else() {
	let result = |tool_name: &str, is_error: bool, details: Value| {
		json!({"type": "message", "message": {"role": "toolResult", "toolName": tool_name, "isError": is_error, "timestamp": 5, "details": details}})
			.to_string()
	};
	let inner = json!({"title": "Plan", "planFilePath": "/plans/p.md", "planExists": true});
	let record = |tool: &str, mode: &str, exit_code: i64| json!({"toolCommands": [{"tool": tool, "mode": mode, "exitCode": exit_code, "inner": inner}]});
	let direct = read_stats(&result("propose", false, inner.clone()));
	assert_eq!(direct.plan_awaiting.as_deref(), Some("Plan"));
	assert_eq!(direct.plan_file.as_deref(), Some("/plans/p.md"));
	let in_bash = read_stats(&result("bash", false, record("propose", "execute", 0)));
	assert_eq!(in_bash.plan_awaiting.as_deref(), Some("Plan"));
	for nothing in [
		result("propose", true, inner.clone()),
		result("bash", false, record("propose", "execute", 1)),
		result("bash", false, record("propose", "help", 0)),
		result("bash", false, record("resolve", "execute", 0)),
		result("write", false, json!({"xdev": {"tool": "propose", "mode": "execute", "inner": inner}})),
	] {
		assert_eq!(read_stats(&nothing).plan_awaiting, None, "{nothing}");
	}
}

#[test]
fn a_session_moved_to_a_sibling_file_is_followed_there_its_copied_entries_counted_once_0464() {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	let dir = sessions.join("--C--moved--");
	let entry = |id: &str, text: &str, cost: f64| {
		json!({
			"type": "message",
			"id": id,
			"message": {
				"role": "assistant",
				"timestamp": at,
				"content": [{"type": "text", "text": text}],
				"usage": {"cost": {"total": cost}},
			},
		})
		.to_string()
	};
	let session_header = |id: &str, parent: Option<&str>| {
		let mut value = json!({"type": "session", "version": 3, "id": id, "cwd": "C:\\moved"});
		if let Some(parent) = parent {
			value["parentSession"] = json!(parent);
		}
		value.to_string()
	};
	let old = write(
		&dir.join("2026-10-23T10-00-00-000Z_old1.jsonl"),
		&[title(), session_header("old1", None), entry("a", "First", 1.0), entry("b", "Other process", 2.0)],
	);
	write(&dir.join("2026-10-23T10-00-00-000Z_old1").join("0-Explore.jsonl"), &[entry("x", "Sub", 0.5)]);
	// The moved session republishes its transcript (and copies its artifacts), then goes on.
	let moved = write(
		&dir.join("2026-10-23T10-05-00-000Z_new2.jsonl"),
		&[title(), session_header("new2", Some("old1")), entry("a", "First", 1.0), entry("c", "Mine", 4.0)],
	);
	write(&dir.join("2026-10-23T10-05-00-000Z_new2").join("0-Explore.jsonl"), &[entry("x", "Sub", 0.5)]);
	write(&dir.join("2026-10-23T10-05-00-000Z_new2").join("1-Later.jsonl"), &[entry("y", "Later", 0.25)]);
	let mut index = SessionIndex::new(Some(sessions), None);
	assert_eq!(index.moved(&old), moved);
	assert_eq!(index.moved(&moved), moved);
	close(index.read(&old, false).cost, 3.5);
	let stats = index.read(&moved, false);
	close(stats.cost, 4.25);
	assert_eq!(stats.last_line.as_deref(), Some("Mine"));
	close(spent_today(&mut index, at), 7.75);
}

#[test]
fn a_handoff_compaction_s_time_is_the_session_s_handoff_at() {
	let handoff =
		json!({"type": "compaction", "method": "handoff", "timestamp": "2026-10-14T10:00:00.000Z"}).to_string();
	let soft = json!({"type": "compaction", "method": "soft", "timestamp": "2026-10-14T11:00:00.000Z"}).to_string();
	assert_eq!(read_stats(&soft).handoff_at, None);
	assert_eq!(
		read_stats(&[handoff, soft].join("\n")).handoff_at,
		Some(date_parse("2026-10-14T10:00:00.000Z").unwrap())
	);
}

#[test]
fn a_session_is_read_incrementally_appended_lines_add_to_its_stats_a_line_being_written_waits_cr_03() {
	let j = temp_jocasta();
	let at = now();
	let file = write(
		&j.root().join("sessions").join("--C--demo--").join("a.jsonl"),
		&[title(), header("C:\\demo"), assistant("One", at, Some(0.5))],
	);
	let clock = TestClock::new(at);
	let mut index = index_at(&j.root().join("sessions"), &clock);
	let stats = index.read(&file, false);
	close(stats.cost, 0.5);
	assert_eq!(stats.last_line.as_deref(), Some("One"));
	// Half a line: not counted yet.
	let next = assistant("Two", at, Some(0.25));
	append(&file, &next[..20]);
	clock.advance(1_000);
	let stats = index.read(&file, false);
	close(stats.cost, 0.5);
	assert_eq!(stats.last_line.as_deref(), Some("One"));
	append(&file, &format!("{}\n", &next[20..]));
	// Checked under a second ago: answered from what was read.
	let stats = index.read(&file, false);
	close(stats.cost, 0.5);
	assert_eq!(stats.last_line.as_deref(), Some("One"));
	clock.advance(1_000);
	let stats = index.read(&file, false);
	close(stats.cost, 0.75);
	assert_eq!(stats.last_line.as_deref(), Some("Two"));
	assert_eq!(stats.message_times.len(), 2);
	// A rewritten (shorter) file is read again from its start.
	write(&file, &[title(), header("C:\\demo"), assistant("Three", at, Some(0.1))]);
	clock.advance(1_000);
	let stats = index.read(&file, false);
	close(stats.cost, 0.1);
	assert_eq!(stats.last_line.as_deref(), Some("Three"));
}

#[test]
fn a_session_s_cost_and_today_s_spend_include_its_subagents_and_advisor_wherever_it_ran_cr_04_f_06() {
	let j = temp_jocasta();
	let at = now();
	let yesterday = at - 36 * 60 * 60_000;
	let sessions = j.root().join("sessions");
	let demo = sessions.join("--C--demo--");
	let main = write(
		&demo.join("s1.jsonl"),
		&[title(), header("C:\\demo"), assistant("Old", yesterday, Some(4.0)), assistant("Main", at, Some(1.0))],
	);
	write(&demo.join("s1").join("0-Explore.jsonl"), &[header("C:\\demo"), assistant("Sub", at, Some(0.5))]);
	write(&demo.join("s1").join("0-Explore").join("1-Nested.jsonl"), &[assistant("Nested", at, Some(0.125))]);
	write(&demo.join("s1").join("__advisor.jsonl"), &[assistant("Advice", at, Some(0.25))]);
	// A merged thread's worktree is gone; its session still counts.
	write(
		&sessions.join("--C--gone--").join("s2.jsonl"),
		&[title(), header("C:\\gone\\worktree"), assistant("Done", at, Some(2.0))],
	);
	let mut index = SessionIndex::new(Some(sessions), None);
	let stats = index.read(&main, false);
	close(stats.cost, 5.875);
	close(stats.cost_by_day[&local_day(at)], 1.875);
	assert_eq!(stats.last_line.as_deref(), Some("Main"));
	close(spent_today(&mut index, at), 3.875);
}

#[tokio::test]
async fn warm_reads_sessions_and_their_transcripts_in_steps_letting_the_event_loop_run_read_then_parses_only_what_s_new()
 {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	// Over 2 MB, a line longer than a step among them, and a nested transcript.
	let mut lines = vec![title(), header("C:\\demo")];
	for i in 0..1500 {
		lines.push(assistant(&format!("Line {i} {}", "x".repeat(1500)), at, Some(0.001)));
	}
	lines.push(assistant(&format!("Long {}", "y".repeat(1_200_000)), at, Some(1.0)));
	lines.push(assistant("Last", at, Some(0.5)));
	let main = write(&sessions.join("--C--demo--").join("w.jsonl"), &lines);
	write(&sessions.join("--C--demo--").join("w").join("0-Sub.jsonl"), &[assistant("Sub", at, Some(0.25))]);
	let clock = TestClock::new(at);
	let mut index = index_at(&sessions, &clock);
	let ticks = Arc::new(AtomicUsize::new(0));
	let ticker = {
		let ticks = Arc::clone(&ticks);
		tokio::spawn(async move {
			loop {
				ticks.fetch_add(1, Ordering::SeqCst);
				tokio::task::yield_now().await;
			}
		})
	};
	index.warm(&[main.clone(), main.clone()], || false).await;
	ticker.abort();
	assert!(ticks.load(Ordering::SeqCst) > 0);
	let expected = SessionIndex::new(Some(sessions.clone()), None).read(&main, false).cost;
	let stats = index.read(&main, false);
	close(stats.cost, expected);
	assert_eq!(stats.last_line.as_deref(), Some("Last"));
	close(stats.cost, 1.5 + 1.0 + 0.5 + 0.25);
	append(&main, &format!("{}\n", assistant("After", at, Some(0.125))));
	clock.advance(1_000);
	assert_eq!(index.read(&main, false).last_line.as_deref(), Some("After"));
	// Stopped at once: nothing read, and read still reads it whole.
	let mut other = SessionIndex::new(Some(sessions), None);
	other.warm(std::slice::from_ref(&main), || true).await;
	assert_eq!(other.read(&main, false).last_line.as_deref(), Some("After"));
}

#[tokio::test]
async fn warm_reads_past_a_line_over_a_step_and_goes_on_stepping_after_it_pm_18() {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	let mut lines =
		vec![title(), header("C:\\demo"), assistant(&format!("Huge {}", "z".repeat(2_500_000)), at, Some(1.0))];
	for i in 0..3 {
		lines.push(assistant(&format!("After {i}"), at, Some(0.25)));
	}
	let main = write(&sessions.join("--C--demo--").join("huge.jsonl"), &lines);
	let mut index = SessionIndex::new(Some(sessions), None);
	index.warm(std::slice::from_ref(&main), || false).await;
	let stats = index.read(&main, false);
	close(stats.cost, 1.75);
	assert_eq!(stats.last_line.as_deref(), Some("After 2"));
	assert_eq!(stats.message_times.len(), 4);
}

#[test]
fn a_turn_omp_discards_durably_counts_once_in_today_s_spend_its_re_journaled_usage_is_the_same_call_srv_b_2() {
	let j = temp_jocasta();
	// Midday of today: a turn 10 minutes before it is today's whatever the wall clock says.
	let at = day_start(&local_day(now()), 0) + 12 * 3_600_000;
	let sessions = j.root().join("sessions");
	let file = sessions.join("--d--work--acme--").join("2026-11-09T10-00-00-000Z_s1.jsonl");
	let t0 = at - 10 * 60_000;
	let iso =
		|ms: i64| chrono::DateTime::from_timestamp_millis(ms).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
	let usage = json!({"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 150, "cost": {"total": 0.5}});
	let session_header =
		json!({"type": "session", "version": 3, "id": "s1", "timestamp": iso(t0), "cwd": "D:/work/acme"}).to_string();
	let user = json!({
		"type": "message",
		"id": "u1",
		"parentId": null,
		"timestamp": iso(t0),
		"message": {"role": "user", "content": [{"type": "text", "text": "hi"}], "timestamp": t0},
	})
	.to_string();
	let turn = json!({
		"type": "message",
		"id": "a1",
		"parentId": "u1",
		"timestamp": iso(t0 + 60_000),
		"message": {
			"role": "assistant",
			"content": [],
			"provider": "anthropic",
			"model": "claude-test",
			"usage": usage,
			"timestamp": t0 + 30_000,
		},
	});
	write(&file, &[session_header.clone(), user.clone(), turn.to_string()]);
	let clock = TestClock::new(at);
	let mut index = index_at(&sessions, &clock);
	close(index.read(&file, true).cost, 0.5);
	close(index.spent_on(&local_day(at)), 0.5);
	// omp's `discardEntryDurably`: the turn removed, its usage re-journaled as an off-branch `model_usage` under a new
	// id with the turn's timestamp, the file rewritten through a temp file and a rename.
	let discarded = json!({
		"type": "model_usage",
		"id": "m9",
		"parentId": "u1",
		"timestamp": turn["timestamp"],
		"activeLeafId": "u1",
		"purpose": "discarded-turn",
		"provider": "anthropic",
		"model": "claude-test",
		"usage": usage,
	});
	let temp = PathBuf::from(format!("{}.tmp", file.display()));
	write(&temp, &[session_header, user, discarded.to_string()]);
	fs::rename(&temp, &file).unwrap();
	clock.advance(2_000);
	close(index.read(&file, true).cost, 0.5);
	close(index.spent_on(&local_day(at)), 0.5);
}

#[tokio::test]
async fn an_older_day_s_message_times_are_read_whole_though_the_index_trimmed_them_at_rollover_srv_b_4() {
	let j = temp_jocasta();
	let today = local_day(now());
	let old = day_start(&today, -4) + 9 * 3_600_000;
	let old_day = local_day(old);
	let sessions = j.root().join("sessions");
	let mut lines = vec![title(), header("C:\\demo")];
	for i in 0..6 {
		lines.push(assistant(&format!("m{i}"), old + i * 5 * 60_000, None));
	}
	let file = write_at(&sessions.join("--C--demo--").join("old.jsonl"), &lines, old + 30 * 60_000);
	// A service that read the file on that day, then kept running until today.
	let clock = TestClock::new(old + 3_600_000);
	let mut index = index_at(&sessions, &clock);
	assert_eq!(index.read(&file, false).message_times.len(), 6);
	assert_eq!(index.times_on(&file, &old_day).len(), 6);
	clock.set(now());
	index.prepare(&old_day, || false).await;
	let times = index.times_on(&file, &old_day).to_vec();
	assert_eq!(times.len(), 6);
	// The worklog of that day gets its 25 minutes: the first to the last message, `worklog.ts`'s span.
	assert_eq!(times.last().unwrap() - times.first().unwrap(), 25 * 60_000);
	// Not written since yesterday: dropped from the index at rollover (PERF-4).
	assert!(!index.follows(&file));
}

#[test]
fn a_long_subagent_s_spend_shows_while_the_session_stays_untouched_srv_b_19() {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	// The session's last line, a blocking `task` call, was written 20 minutes ago.
	let main = write_at(
		&sessions.join("--C--demo--").join("long.jsonl"),
		&[title(), header("C:\\demo"), assistant("Task", at, Some(1.0))],
		at - 20 * 60_000,
	);
	let sub =
		write(&sessions.join("--C--demo--").join("long").join("0-Explore.jsonl"), &[assistant("Sub", at, Some(0.5))]);
	let clock = TestClock::new(at);
	let mut index = index_at(&sessions, &clock);
	close(index.read(&main, false).cost, 1.5);
	append(&sub, &format!("{}\n", assistant("Sub 2", at + 1, Some(0.25))));
	clock.advance(2_000);
	close(index.read(&main, false).cost, 1.75);
	// A second subagent the task starts later is found within 30 s while the first one works.
	write(&sessions.join("--C--demo--").join("long").join("1-Review.jsonl"), &[assistant("Review", at + 2, Some(2.0))]);
	clock.advance(31_000);
	close(index.read(&main, false).cost, 3.75);
	close(index.spent_on(&local_day(at)), 3.75);
}

#[test]
fn the_first_read_of_a_new_day_drops_message_times_before_yesterday_and_forgets_files_that_are_gone_pm_17() {
	let j = temp_jocasta();
	let today = local_day(now());
	let two_days_ago = day_start(&today, -2) + 3_600_000;
	let yesterday = day_start(&today, -1) + 3_600_000;
	let sessions = j.root().join("sessions");
	let demo = sessions.join("--C--demo--");
	let file = write(
		&demo.join("days.jsonl"),
		&[
			title(),
			header("C:\\demo"),
			assistant("Old", two_days_ago, Some(1.0)),
			assistant("Yesterday", yesterday, Some(1.0)),
		],
	);
	let gone = write(&demo.join("gone.jsonl"), &[title(), header("C:\\demo"), assistant("X", yesterday, Some(1.0))]);
	let clock = TestClock::new(day_start(&today, 0) - 60_000);
	let mut index = index_at(&sessions, &clock);
	assert_eq!(index.read(&file, false).message_times, vec![two_days_ago, yesterday]);
	assert_eq!(index.read(&gone, false).last_line.as_deref(), Some("X"));
	let mut listed: Vec<PathBuf> = index.list(0).into_iter().map(|session| session.file).collect();
	listed.sort();
	let mut expected = vec![file.clone(), gone.clone()];
	expected.sort();
	assert_eq!(listed, expected);
	fs::remove_file(&gone).unwrap();
	clock.set(day_start(&today, 0) + 60_000);
	assert_eq!(index.read(&file, false).message_times, vec![yesterday]);
	// Forgotten: a file of that name is read afresh.
	write(&gone, &[title(), header("C:\\demo"), assistant("Y", yesterday, Some(1.0))]);
	assert_eq!(index.read(&gone, false).last_line.as_deref(), Some("Y"));
}

#[test]
fn a_session_replaced_through_a_rename_by_a_larger_file_is_read_again_from_its_start_counted_once() {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	let file = write(
		&sessions.join("--C--demo--").join("strip.jsonl"),
		&[title(), header("C:\\demo"), assistant("One", at, Some(1.0))],
	);
	let clock = TestClock::new(at);
	let mut index = index_at(&sessions, &clock);
	let stats = index.read(&file, false);
	close(stats.cost, 1.0);
	assert_eq!(stats.message_times, vec![at]);
	pause();
	// omp's snapcompact strip: the whole session rewritten to a temp file, renamed over it, larger than before.
	let temp = PathBuf::from(format!("{}.tmp", file.display()));
	write(
		&temp,
		&[
			title(),
			header("C:\\demo"),
			assistant(&format!("One, rewritten {}", "r".repeat(200)), at, Some(1.0)),
			assistant("Two", at + 1, Some(2.0)),
		],
	);
	fs::rename(&temp, &file).unwrap();
	clock.advance(1_000);
	let stats = index.read(&file, false);
	close(stats.cost, 3.0);
	assert_eq!(stats.last_line.as_deref(), Some("Two"));
	assert_eq!(stats.message_times, vec![at, at + 1]);
	close(spent_today(&mut index, clock.get()), 3.0);
}

#[test]
fn today_s_spend_never_drops_a_file_rewritten_without_some_calls_its_session_moved_on_or_the_file_deleted_keeps_what_was_spent_0464()
 {
	let j = temp_jocasta();
	let at = now();
	let sessions = j.root().join("sessions");
	let dir = sessions.join("--C--boss--");
	let call = |id: &str, cost: f64, when: i64| {
		json!({
			"type": "message",
			"id": id,
			"message": {
				"role": "assistant",
				"timestamp": when,
				"content": [{"type": "text", "text": id}],
				"usage": {"cost": {"total": cost}},
			},
		})
		.to_string()
	};
	let side = |id: &str, cost: f64| {
		let iso = chrono::DateTime::from_timestamp_millis(at).unwrap().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
		json!({"type": "model_usage", "id": id, "timestamp": iso, "usage": {"cost": {"total": cost}}}).to_string()
	};
	let session_header = |id: &str, parent: Option<&str>| {
		let mut value = json!({"type": "session", "version": 3, "id": id, "cwd": "C:\\boss"});
		if let Some(parent) = parent {
			value["parentSession"] = json!(parent);
		}
		value.to_string()
	};
	let old = write(
		&dir.join("2026-10-24T15-00-00-000Z_old1.jsonl"),
		&[title(), session_header("old1", None), call("a", 1.0, at), call("b", 2.0, at), side("m", 0.25)],
	);
	let clock = TestClock::new(at);
	let mut index = index_at(&sessions, &clock);
	close(spent_today(&mut index, clock.get()), 3.25);
	pause();
	// A second process writes the session: its own version replaces the file through a rename, without "b".
	let temp = PathBuf::from(format!("{}.tmp", old.display()));
	write(&temp, &[title(), session_header("old1", None), call("a", 1.0, at), side("m", 0.25), call("f", 0.5, at)]);
	fs::rename(&temp, &old).unwrap();
	clock.advance(31_000);
	close(spent_today(&mut index, clock.get()), 3.75);
	// The first process moves on to a sibling file (0464), its whole transcript republished there.
	let new = dir.join("2026-10-24T15-05-00-000Z_new2.jsonl");
	write(
		&new,
		&[
			title(),
			session_header("new2", Some("old1")),
			call("a", 1.0, at),
			call("b", 2.0, at),
			side("m", 0.25),
			call("c", 4.0, at),
		],
	);
	clock.advance(31_000);
	close(spent_today(&mut index, clock.get()), 7.75);
	// The old file deleted (omp's /delete): what it cost was still spent today.
	fs::remove_file(&old).unwrap();
	clock.advance(31_000);
	close(spent_today(&mut index, clock.get()), 7.75);
	// Yesterday's calls never count today.
	append(&new, &format!("{}\n", call("y", 8.0, day_start(&local_day(at), 0) - 60_000)));
	clock.advance(31_000);
	close(spent_today(&mut index, clock.get()), 7.75);
}

/// A session entry on omp's tree (0509): its id, its parent's (null for a root), and its body.
fn node(id: &str, parent_id: Option<&str>, body: &Value) -> String {
	let mut entry = json!({"id": id, "parentId": parent_id, "timestamp": "1970-01-01T00:00:00.000Z"});
	for (key, value) in body.as_object().unwrap() {
		entry[key] = value.clone();
	}
	entry.to_string()
}

fn said(role: &str, text: &str, at: i64, cost: Option<f64>) -> Value {
	let mut message = json!({"role": role, "timestamp": at, "content": [{"type": "text", "text": text}]});
	if let Some(cost) = cost {
		message["usage"] = json!({"cost": {"total": cost}});
	}
	json!({"type": "message", "message": message})
}

fn proposal(title: &str, file: &str, at: i64) -> Value {
	json!({
		"type": "message",
		"message": {
			"role": "toolResult",
			"toolName": "bash",
			"timestamp": at,
			"details": {"toolCommands": [{"tool": "propose", "mode": "execute", "exitCode": 0, "inner": {"title": title, "planFilePath": file}}]},
		},
	})
}

/// A plan approved, work begun, a second plan proposed and waiting.
fn plan_tree() -> Vec<String> {
	vec![
		node("u1", None, &said("user", "Plan it", 1, None)),
		node("p0", Some("u1"), &proposal("First", "/plans/first.md", 2)),
		node("m0", Some("p0"), &json!({"type": "mode_change", "mode": "none"})),
		node("a1", Some("m0"), &said("assistant", "Working", 3, Some(1.0))),
		node("p1", Some("a1"), &proposal("Second", "/plans/second.md", 4)),
		node(
			"e1",
			Some("p1"),
			&json!({"type": "message", "message": {"role": "assistant", "timestamp": 5, "content": []}}),
		),
	]
}

/// Then rewound to before that proposal.
fn plan_rewind() -> Vec<String> {
	vec![
		node(
			"b1",
			Some("a1"),
			&json!({"type": "branch_summary", "fromId": "e1", "summary": "Dropped the second plan"}),
		),
		node("u2", Some("b1"), &said("user", "Do something else", 6, None)),
	]
}

#[test]
fn a_plan_proposed_on_a_branch_that_was_rewound_waits_for_nothing_the_plan_file_is_the_active_branch_s_0509() {
	let tree = plan_tree();
	let rewind = plan_rewind();
	let waiting = read_stats(&tree.join("\n"));
	assert_eq!(waiting.plan_awaiting.as_deref(), Some("Second"));
	assert_eq!(waiting.plan_file.as_deref(), Some("/plans/second.md"));
	let summarized = read_stats(&with(tree.clone(), rewind[0].clone()).join("\n"));
	assert_eq!(summarized.plan_awaiting, None);
	assert_eq!(summarized.plan_file.as_deref(), Some("/plans/first.md"));
	let rewound = read_stats(&[tree, rewind].concat().join("\n"));
	assert_eq!(rewound.plan_awaiting, None);
	assert_eq!(rewound.plan_file.as_deref(), Some("/plans/first.md"));
	assert_eq!(rewound.plan_sent_back, 0);
	assert_eq!(rewound.last_line.as_deref(), Some("Working"));
}

/// A review turn that ran `bun test` and said it passed, abandoned for a new branch from its user message.
fn review_tree() -> Vec<String> {
	vec![
		node("u1", None, &said("user", "Review it", 1, None)),
		node(
			"a1",
			Some("u1"),
			&json!({
				"type": "message",
				"message": {
					"role": "assistant",
					"timestamp": 10,
					"content": [{"type": "toolCall", "id": "c1", "name": "bash", "arguments": {"command": "bun test"}}],
					"usage": {"cost": {"total": 0.25}},
				},
			}),
		),
		node(
			"r1",
			Some("a1"),
			&json!({
				"type": "message",
				"message": {"role": "toolResult", "toolCallId": "c1", "toolName": "bash", "timestamp": 11, "content": []},
			}),
		),
		node("a2", Some("r1"), &said("assistant", "Reviewed.\nTESTS: pass — bun test", 12, Some(1.0))),
		node("u2", Some("u1"), &said("user", "Review it again", 13, None)),
		node("a3", Some("u2"), &said("assistant", "Different answer", 14, Some(0.5))),
	]
}

#[test]
fn an_abandoned_branch_s_review_proves_nothing_its_cost_still_counts_0509() {
	let stats = read_stats(&review_tree().join("\n"));
	assert_eq!(checked_tests(&stats, 0), TestsResult::Unknown);
	assert_eq!(stats.tests, None);
	assert_eq!(stats.bash_runs, vec![]);
	assert_eq!(stats.last_line.as_deref(), Some("Different answer"));
	close(stats.cost, 1.75);
	assert_eq!(stats.message_times, vec![1, 10, 11, 12, 13, 14]);
}

#[test]
fn an_off_branch_entry_leaves_the_state_as_it_was_its_cost_counts_0509() {
	let mut lines = review_tree();
	lines.push(node(
		"late",
		Some("a2"),
		&json!({
			"type": "model_usage",
			"activeLeafId": "a3",
			"timestamp": "1970-01-01T00:00:00.015Z",
			"usage": {"cost": {"total": 0.125}},
		}),
	));
	let mut late = said("assistant", "Late answer\nTESTS: fail", 16, Some(2.0));
	late["activeLeafId"] = json!("a3");
	lines.push(node("x1", Some("a2"), &late));
	let stats = read_stats(&lines.join("\n"));
	assert_eq!(stats.last_line.as_deref(), Some("Different answer"));
	assert_eq!(stats.tests, None);
	close(stats.cost, 3.875);
}

#[test]
fn a_rewind_read_across_two_incremental_reads_gives_what_reading_the_whole_file_gives_0509() {
	let j = temp_jocasta();
	let sessions = j.root().join("sessions");
	let mut lines = vec![title(), header("C:\\demo")];
	lines.extend(plan_tree());
	let file = write(&sessions.join("--C--demo--").join("rewind.jsonl"), &lines);
	let clock = TestClock::new(now());
	let mut index = index_at(&sessions, &clock);
	assert_eq!(index.read(&file, false).plan_awaiting.as_deref(), Some("Second"));
	append(&file, &format!("{}\n", plan_rewind().join("\n")));
	clock.advance(1_000);
	lines.extend(plan_rewind());
	let whole = read_stats(&lines.join("\n"));
	assert_eq!(*index.read(&file, false), whole);
	assert_eq!(index.read(&file, false).plan_file.as_deref(), Some("/plans/first.md"));
}
