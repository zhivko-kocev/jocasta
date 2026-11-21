use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::shared::omp_session::set_mtime;
use crate::shared::testing::{TempJocasta, temp_jocasta};

fn t0() -> i64 {
	date_parse("2026-11-09T10:00:00Z").unwrap()
}

fn iso(minutes: i64) -> String {
	chrono::DateTime::from_timestamp_millis(t0() + minutes * 60_000)
		.unwrap()
		.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn lines(entries: &[Value]) -> String {
	let rows: Vec<String> = entries.iter().map(Value::to_string).collect();
	format!("{}\n", rows.join("\n"))
}

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 5e-13, "{actual} is not close to {expected}");
}

#[derive(Default, Clone, Copy)]
struct Usage {
	input: u64,
	cache_read: u64,
	cache_write: u64,
	output: u64,
	cost: Option<f64>,
}

/// One omp assistant turn with usage (omp's `cost.total` included when given) and tool calls, on a model.
fn turn_on(minutes: i64, usage: Usage, calls: &[Value], provider: &str, model: &str) -> Value {
	let mut counted = json!({
		"input": usage.input,
		"cacheRead": usage.cache_read,
		"cacheWrite": usage.cache_write,
		"output": usage.output,
	});
	if let Some(total) = usage.cost {
		counted["cost"] = json!({ "total": total });
	}
	let content: Vec<Value> = calls
		.iter()
		.map(|call| {
			let mut block = json!({ "type": "toolCall" });
			block.as_object_mut().unwrap().extend(call.as_object().unwrap().clone());
			block
		})
		.collect();
	json!({
		"type": "message",
		"timestamp": iso(minutes),
		"message": {
			"role": "assistant",
			"provider": provider,
			"model": model,
			"timestamp": t0() + minutes * 60_000,
			"content": content,
			"usage": counted,
		},
	})
}

/// One omp assistant turn on the default model.
fn turn(minutes: i64, usage: Usage, calls: &[Value]) -> Value {
	turn_on(minutes, usage, calls, "anthropic", "claude-test")
}

fn result(id: &str) -> Value {
	json!({
		"type": "message",
		"timestamp": iso(0),
		"message": { "role": "toolResult", "toolCallId": id, "toolName": "x", "content": [{ "type": "text", "text": "ok" }] },
	})
}

fn read(id: &str, path: &str) -> Value {
	json!({ "id": id, "name": "read", "arguments": { "path": path, "i": "Reading" } })
}

fn session_line(id: &str, minutes: i64) -> Value {
	json!({ "type": "session", "id": id, "timestamp": iso(minutes) })
}

/// Fresh roots for one test and omp's agent dir in them.
fn setup() -> (TempJocasta, PathBuf) {
	let jocasta = temp_jocasta();
	let agent = omp_agent_dir(None);
	fs::create_dir_all(&agent).unwrap();
	(jocasta, agent)
}

fn sessions_dir(agent: &Path) -> PathBuf {
	let dir = agent.join("sessions").join("--d--work--acme--");
	fs::create_dir_all(&dir).unwrap();
	dir
}

fn artifacts_of(file: &Path) -> PathBuf {
	PathBuf::from(file.to_string_lossy().strip_suffix(".jsonl").unwrap())
}

/// omp's model cache with `rows` (provider id → models).
fn write_models_db(agent: &Path, rows: Value) {
	let db = rusqlite::Connection::open(agent.join("models.db")).unwrap();
	db.execute_batch(
		"CREATE TABLE IF NOT EXISTS model_cache (provider_id TEXT PRIMARY KEY, models TEXT NOT NULL, updated_at INTEGER)",
	)
	.unwrap();
	db.execute_batch("DELETE FROM model_cache").unwrap();
	for (provider, models) in rows.as_object().unwrap() {
		let models: Vec<Value> = models
			.as_array()
			.unwrap()
			.iter()
			.map(|model| {
				let mut model = model.clone();
				model["name"] = model["id"].clone();
				model["maxTokens"] = json!(32_000);
				model
			})
			.collect();
		db.execute(
			"INSERT INTO model_cache (provider_id, models, updated_at) VALUES (?1, ?2, 0)",
			rusqlite::params![provider, Value::Array(models).to_string()],
		)
		.unwrap();
	}
}

/// A thread worth 0546's line: $1.84 over the main session and a subagent, the main session's peak 82k of a 200k
/// window, cache read 71%, 3 re-reads, 1 repeated call, 1 cache miss, 2 compactions; plus a scratch copy that counts not.
fn write_thread(agent: &Path) -> PathBuf {
	let file = sessions_dir(agent).join("2026-11-09T10-00-00-000Z_s-main.jsonl");
	let artifacts = artifacts_of(&file);
	fs::create_dir_all(artifacts.join("0-Explore")).unwrap();
	let bash = |id: &str| json!({ "id": id, "name": "bash", "arguments": { "command": "bun test" } });
	fs::write(
		&file,
		lines(&[
			session_line("s-main", 0),
			turn(
				0,
				Usage { input: 1000, cache_write: 20_000, cost: Some(0.5), ..Usage::default() },
				&[read("r1", "src/a.ts"), read("r2", "src/a.ts:1-9")],
			),
			result("r1"),
			result("r2"),
			turn(
				1,
				Usage { input: 1000, cache_read: 60_000, cache_write: 21_000, cost: Some(0.6), ..Usage::default() },
				&[read("r3", "src/a.ts"), bash("b1"), bash("b2")],
			),
			result("r3"),
			result("b1"),
			result("b2"),
			json!({ "type": "compaction", "timestamp": iso(2), "tokensBefore": 82_000 }),
			// Nine minutes idle, then a large cache write: a cache miss.
			turn(10, Usage { cache_write: CACHE_MISS_WRITE_TOKENS, cost: Some(0.34), ..Usage::default() }, &[]),
		]),
	)
	.unwrap();
	// The subagent's context is larger than the main session's; the peak is the main session's all the same.
	fs::write(
		artifacts.join("0-Explore").join("0-Explore.jsonl"),
		lines(&[
			session_line("s-sub", 2),
			turn(
				2,
				Usage { cache_read: 100_000, cache_write: 12_350, cost: Some(0.4), ..Usage::default() },
				&[read("s1", "src/c.ts"), read("s2", "src/c.ts")],
			),
			result("s1"),
			result("s2"),
			json!({ "type": "compaction", "timestamp": iso(3), "tokensBefore": 112_350 }),
		]),
	)
	.unwrap();
	fs::write(
		artifacts.join("0-Explore.omp-gc-compress-123.jsonl"),
		lines(&[turn(4, Usage { input: 9, cost: Some(99.0), ..Usage::default() }, &[])]),
	)
	.unwrap();
	file
}

fn peak(tokens: u64, window: Option<u64>, percent: Option<f64>) -> PeakContext {
	PeakContext { tokens, window, percent }
}

#[test]
fn omps_own_pricing_a_request_costs_its_usage_cost_total_one_without_is_unpriced() {
	let (_j, agent) = setup();
	write_models_db(&agent, json!({ "anthropic": [{ "id": "claude-test", "contextWindow": 200_000 }] }));
	let record = read_omp_session(
		&lines(&[
			json!({ "type": "session", "id": "s" }),
			turn(0, Usage { input: 10, cache_write: 1000, output: 5, cost: Some(0.25), ..Usage::default() }, &[]),
			json!({
				"type": "model_usage",
				"timestamp": iso(1),
				"purpose": "cache-warm",
				"provider": "anthropic",
				"model": "claude-test",
				"usage": { "input": 1, "cacheRead": 150_000, "output": 1, "cost": { "total": 0.03 } },
			}),
			turn_on(2, Usage { input: 20, ..Usage::default() }, &[], "acme", "no-cost"),
		]),
		&OMP_PRICING,
		None,
	);
	let costs: Vec<Option<f64>> = record.requests.iter().map(|r| r.cost).collect();
	assert_eq!(costs, [Some(0.25), Some(0.03), None]);
	let summary = summarize_session(&record, &OMP_PRICING);
	close(summary.cost, 0.28);
	assert_eq!(summary.unpriced_models, ["no-cost"]);
	// The cache-warm request is larger but a side request; the agent's own peak is 1,010 tokens of claude-test's window.
	assert_eq!(summary.peak_context, peak(1010, Some(200_000), Some(0.5)));
}

#[test]
fn a_models_window_provider_equal_or_prefixed_model_id_equal_unknown_otherwise() {
	let (_j, agent) = setup();
	write_models_db(
		&agent,
		json!({
			"anthropic": [{ "id": "claude-test", "contextWindow": 200_000 }],
			"github-copilot:models-v2:abc123": [{ "id": "gpt-test", "contextWindow": 1_000_000 }],
			"github-copilotx": [{ "id": "other", "contextWindow": 5 }],
		}),
	);
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "claude-test"), Some(200_000));
	assert_eq!(OMP_PRICING.window(Some("github-copilot"), "gpt-test"), Some(1_000_000));
	assert_eq!(OMP_PRICING.window(Some("github-copilot"), "other"), None);
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "gpt-test"), None);
	assert_eq!(OMP_PRICING.window(None, "claude-test"), None);
}

#[test]
fn a_threads_ledger_spend_cache_read_and_waste_over_all_its_transcripts_peak_from_the_main_session() {
	let (_j, agent) = setup();
	write_models_db(&agent, json!({ "anthropic": [{ "id": "claude-test", "contextWindow": 200_000 }] }));
	let file = write_thread(&agent);
	let ledger = thread_ledger(&[file]).expect("a ledger");
	close(ledger.spend, 1.84);
	assert_eq!(ledger.requests, 4);
	close(ledger.cache_read_share.unwrap(), 160_000.0 / 225_350.0);
	assert_eq!(ledger.peak_context, Some(peak(82_000, Some(200_000), Some(41.0))));
	let waste = &ledger.waste;
	assert_eq!((waste.rereads, waste.repeated_calls, waste.cache_misses, waste.compactions), (3, 1, 1, 2));
	assert_eq!(
		ledger_line(&ledger),
		"spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 repeated call · 1 cache miss · 2 compactions"
	);
}

#[test]
fn a_threads_files_are_parsed_again_only_when_their_size_or_mtime_changes() {
	let (_j, agent) = setup();
	write_models_db(&agent, json!({}));
	let file = write_thread(&agent);
	// A whole-millisecond mtime, so setting it back is exact.
	set_mtime(&file, t0());
	let spend = |file: &Path| thread_ledger(&[file.to_path_buf()]).unwrap().spend;
	close(spend(&file), 1.84);
	// Same size and mtime: the cached parse stands.
	let text = fs::read_to_string(&file).unwrap().replace(r#""total":0.5}"#, r#""total":0.9}"#);
	fs::write(&file, text).unwrap();
	set_mtime(&file, t0());
	close(spend(&file), 1.84);
	set_mtime(&file, t0() + 5000);
	close(spend(&file), 2.24);
}

#[test]
fn a_threads_ledger_covers_every_session_of_it_a_new_sessions_spend_adds_a_moved_sessions_copies_count_once_the_peak_is_the_largest_sessions_own()
 {
	let (_j, agent) = setup();
	write_models_db(&agent, json!({ "anthropic": [{ "id": "claude-test", "contextWindow": 200_000 }] }));
	let first = write_thread(&agent);
	let sessions = sessions_dir(&agent);
	// "Approve and execute" goes on in a new session, which names no parent.
	let executing = turn(
		20,
		Usage { input: 1000, cache_read: 150_000, cache_write: 1000, cost: Some(0.22), ..Usage::default() },
		&[],
	);
	let second = sessions.join("2026-11-09T10-20-00-000Z_s-exec.jsonl");
	fs::write(&second, lines(&[session_line("s-exec", 20), executing.clone()])).unwrap();
	// That session moved to a sibling file, which copies what it held and goes on.
	let third = sessions.join("2026-11-09T10-25-00-000Z_s-moved.jsonl");
	fs::write(
		&third,
		lines(&[
			json!({ "type": "session", "id": "s-moved", "parentSession": "s-exec", "timestamp": iso(25) }),
			executing,
			turn(30, Usage { input: 10, cost: Some(0.1), ..Usage::default() }, &[]),
		]),
	)
	.unwrap();
	let ledger = thread_ledger(&[first, second, third.clone()]).unwrap();
	close(ledger.spend, 2.16);
	assert_eq!(ledger.requests, 6);
	assert_eq!(ledger.peak_context, Some(peak(152_000, Some(200_000), Some(76.0))));
	let waste = &ledger.waste;
	assert_eq!((waste.rereads, waste.repeated_calls, waste.cache_misses, waste.compactions), (3, 1, 1, 2));
	// The current session alone, as before: the earlier one's spend is missing.
	close(thread_ledger(&[third]).unwrap().spend, 0.32);
}

#[test]
fn no_models_db_the_peak_shows_tokens_only_no_session_file_no_ledger() {
	let (_j, agent) = setup();
	let file = write_thread(&agent);
	let ledger = thread_ledger(&[file]).unwrap();
	assert_eq!(ledger.peak_context, Some(peak(82_000, None, None)));
	assert!(ledger_line(&ledger).starts_with("spend $1.84 · peak context 82k · cache read 71% · "));
	assert!(thread_ledger(&[agent.join("missing.jsonl")]).is_none());
}

#[test]
fn the_ledger_line_two_decimal_spend_m_sized_windows_nothing_shown_for_what_is_zero_singular_and_plural() {
	let base =
		ThreadLedger { spend: 0.0, requests: 0, cache_read_share: None, peak_context: None, waste: Waste::default() };
	assert_eq!(ledger_line(&base), "spend $0.00");
	let busy = ThreadLedger {
		spend: 12.3456,
		requests: 3,
		cache_read_share: Some(0.5),
		peak_context: Some(peak(82_400, Some(1_000_000), Some(8.2))),
		waste: Waste {
			rereads: 1,
			repeated_calls: 2,
			cache_misses: 2,
			cache_miss_idle_minutes: vec![6, 7],
			compactions: 1,
			..Waste::default()
		},
	};
	assert_eq!(
		ledger_line(&busy),
		"spend $12.35 · peak context 8% (82k of 1M) · cache read 50% · 1 re-read · 2 repeated calls · 2 cache misses · 1 compaction"
	);
	let large = ThreadLedger {
		spend: 0.1,
		requests: 1,
		cache_read_share: Some(0.0),
		peak_context: Some(peak(1_234_567, Some(2_000_000), Some(61.7))),
		..base.clone()
	};
	assert_eq!(ledger_line(&large), "spend $0.10 · peak context 62% (1.2M of 2M) · cache read 0%");
	let unknown = ThreadLedger { peak_context: Some(peak(400, None, None)), ..base };
	assert_eq!(ledger_line(&unknown), "spend $0.00 · peak context 0k");
}

/// Per-token prices (dollars per million: input 3, cache read 0.3, cache write 3.75, output 15), as fork-copies.ts priced.
struct PerToken;

impl LedgerPricing for PerToken {
	fn cost(&self, r: &RequestFields, _usage: &Map<String, Value>) -> Option<f64> {
		let tokens = |n: u64| n as f64;
		Some(
			(tokens(r.input) * 3.0
				+ tokens(r.cache_read) * 0.3
				+ tokens(r.cache_write) * 3.75
				+ tokens(r.output) * 15.0)
				/ 1e6,
		)
	}

	fn window(&self, _provider: Option<&str>, _model: &str) -> Option<u64> {
		None
	}
}

/// SRV-B-1's session (fork-copies.ts): two reads of one file, a compaction, a third turn; `cost` maps omp's cost.
fn fork_history(cost: impl Fn(f64) -> f64) -> Vec<Value> {
	vec![
		turn(
			0,
			Usage { input: 1000, cache_write: 20_000, output: 500, cost: Some(cost(0.5)), ..Usage::default() },
			&[read("r1", "src/a.ts")],
		),
		result("r1"),
		turn(
			1,
			Usage { input: 1000, cache_read: 60_000, cache_write: 1000, output: 500, cost: Some(cost(0.6)) },
			&[read("r2", "src/a.ts")],
		),
		result("r2"),
		json!({ "type": "compaction", "timestamp": iso(2), "summary": "s", "tokensBefore": 82_000 }),
		turn(3, Usage { input: 1000, cache_read: 30_000, output: 500, cost: Some(cost(0.2)), ..Usage::default() }, &[]),
	]
}

fn jsonl_files_under(dir: &Path, found: &mut Vec<PathBuf>) {
	for entry in fs::read_dir(dir).unwrap().flatten() {
		let path = entry.path();
		if path.is_dir() {
			jsonl_files_under(&path, found);
		} else if path.extension().is_some_and(|ext| ext == "jsonl") {
			found.push(path);
		}
	}
}

/// Every `.jsonl` under a trial's folder, read as the benchmark reads it (agent `main` at the top, else its path).
fn trial_records(dir: &Path) -> Vec<SessionRecord> {
	let mut files = Vec::new();
	jsonl_files_under(dir, &mut files);
	files.sort();
	files
		.iter()
		.map(|file| {
			let rel = file.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
			let agent = if rel.contains('/') { rel.strip_suffix(".jsonl").unwrap() } else { "main" };
			read_omp_session(&fs::read_to_string(file).unwrap(), &PerToken, Some(agent))
		})
		.collect()
}

fn nested_fork_counts_its_inherited_history_once(file: &str, header: Value) {
	let (_j, agent) = setup();
	let sessions = sessions_dir(&agent);
	let main = sessions.join("2026-11-09T10-00-00-000Z_s-main.jsonl");
	let artifacts = artifacts_of(&main);
	fs::create_dir_all(&artifacts).unwrap();
	let mut entries = vec![session_line("s-main", 0)];
	entries.extend(fork_history(|dollars| dollars));
	fs::write(&main, lines(&entries)).unwrap();
	assert_eq!(thread_ledger(std::slice::from_ref(&main)).unwrap().requests, 3);
	let before = trial_ledger(&thread_session_records(std::slice::from_ref(&main), &PerToken), &PerToken);
	close(before.totals.cost, 0.13725);
	// omp 18.20.2 writes the fork beside the parent's subagents: the parent's history again, cost zeroed, tokens kept.
	// omp 18.21 removed seance, so only older session files hold such a fork.
	let mut fork_header = json!({ "type": "session", "timestamp": iso(5) });
	fork_header.as_object_mut().unwrap().extend(header.as_object().unwrap().clone());
	let mut fork = vec![fork_header];
	fork.extend(fork_history(|_| 0.0));
	fork.push(turn(
		6,
		Usage { input: 500, cache_read: 10_000, output: 200, cost: Some(0.05), ..Usage::default() },
		&[],
	));
	fs::write(artifacts.join(file), lines(&fork)).unwrap();
	let product = thread_ledger(std::slice::from_ref(&main)).unwrap();
	assert_eq!(product.requests, 4);
	assert_eq!(ledger_line(&product), "spend $1.35 · peak context 62k · cache read 80% · 1 re-read · 1 compaction");
	let mut reversed = trial_records(&sessions);
	reversed.reverse();
	for records in [thread_session_records(&[main], &PerToken), reversed] {
		let totals = trial_ledger(&records, &PerToken).totals;
		assert_eq!(
			(totals.requests, totals.input, totals.cache_read, totals.cache_write, totals.output),
			(4, 3500, 100_000, 21_000, 1700)
		);
		close(totals.cost, 0.14475);
		assert_eq!((totals.waste.rereads, totals.waste.compactions), (1, 1));
	}
}

#[test]
fn a_tan_clone_nested_in_the_sessions_artifacts_counts_its_inherited_history_once_in_the_product_and_the_benchmark() {
	nested_fork_counts_its_inherited_history_once("Tan-1.jsonl", json!({ "id": "tan-1", "parentSession": "s-main" }));
}

#[test]
fn a_seance_subtask_fork_nested_in_the_sessions_artifacts_counts_its_inherited_history_once_in_the_product_and_the_benchmark()
 {
	nested_fork_counts_its_inherited_history_once(
		"3-Consult.jsonl",
		json!({ "id": "seance-1", "parentSession": "s-main", "seanceSource": "s-main.jsonl" }),
	);
}

#[test]
fn a_session_omp_moved_to_a_sibling_counts_its_copies_once_its_artifacts_copies_too_the_entry_that_moved_it_counts() {
	let (_j, agent) = setup();
	let sessions = sessions_dir(&agent);
	let old = sessions.join("2026-11-09T10-20-00-000Z_s-exec.jsonl");
	let moved = sessions.join("2026-11-09T10-25-00-000Z_s-moved.jsonl");
	let executing = turn(
		20,
		Usage { input: 1000, cache_read: 150_000, cache_write: 1000, cost: Some(0.22), ..Usage::default() },
		&[],
	);
	fs::write(&old, lines(&[session_line("s-exec", 20), executing.clone()])).unwrap();
	let subagent = lines(&[
		json!({ "type": "session", "id": "s-sub", "parentSession": old.to_string_lossy(), "timestamp": iso(21) }),
		turn(
			21,
			Usage { input: 100, cost: Some(0.1), ..Usage::default() },
			&[read("s1", "src/c.ts"), read("s2", "src/c.ts")],
		),
		result("s1"),
		result("s2"),
	]);
	for file in [&old, &moved] {
		let explore = artifacts_of(file).join("0-Explore");
		fs::create_dir_all(&explore).unwrap();
		fs::write(explore.join("0-Explore.jsonl"), &subagent).unwrap();
	}
	// The turn whose write moved the session is only in the new file, though it predates that file's header.
	fs::write(
		&moved,
		lines(&[
			json!({
				"type": "session",
				"id": "s-moved",
				"parentSession": "s-exec",
				"continuationOf": "s-exec",
				"timestamp": iso(25),
			}),
			executing,
			turn(24, Usage { input: 10, cost: Some(0.03), ..Usage::default() }, &[]),
			turn(30, Usage { input: 10, cost: Some(0.1), ..Usage::default() }, &[]),
		]),
	)
	.unwrap();
	let files = [old, moved];
	let product = thread_ledger(&files).unwrap();
	assert_eq!(product.requests, 4);
	close(product.spend, 0.45);
	assert_eq!(product.waste.rereads, 1);
	let mut reversed = trial_records(&sessions);
	reversed.reverse();
	for records in [thread_session_records(&files, &PerToken), reversed] {
		let totals = trial_ledger(&records, &PerToken).totals;
		assert_eq!((totals.requests, totals.input, totals.cache_read, totals.cache_write), (4, 1120, 150_000, 1000));
		assert_eq!(totals.waste.rereads, 1);
	}
}

#[test]
fn jocastas_own_handoff_isnt_a_compaction_omps_are() {
	let (_j, agent) = setup();
	let record = read_omp_session(
		&lines(&[
			json!({ "type": "session", "id": "s" }),
			turn(0, Usage { input: 10, cost: Some(0.1), ..Usage::default() }, &[]),
			json!({ "type": "compaction", "method": "soft", "timestamp": iso(1) }),
			json!({ "type": "compaction", "method": "handoff", "timestamp": iso(2) }),
		]),
		&OMP_PRICING,
		None,
	);
	assert_eq!(waste_of(&record).compactions, 1);
	let file = sessions_dir(&agent).join("2026-11-09T10-00-00-000Z_s.jsonl");
	fs::write(
		&file,
		lines(&[
			json!({ "type": "session", "id": "s" }),
			turn(0, Usage { input: 10, cost: Some(0.1), ..Usage::default() }, &[]),
			json!({ "type": "compaction", "method": "handoff", "timestamp": iso(2) }),
		]),
	)
	.unwrap();
	let ledger = thread_ledger(&[file]).unwrap();
	assert_eq!(ledger_line(&ledger), "spend $0.10 · peak context 0k · cache read 0%");
}

#[test]
fn a_cache_miss_is_measured_against_the_same_models_previous_request_a_side_call_on_another_model_doesnt_count() {
	let record = read_omp_session(
		&lines(&[
			json!({ "type": "session", "id": "s" }),
			turn(0, Usage { input: 10, ..Usage::default() }, &[]),
			json!({
				"type": "model_usage",
				"timestamp": iso(7),
				"purpose": "classifier",
				"provider": "openai",
				"model": "small",
				"usage": { "cacheWrite": 2 * CACHE_MISS_WRITE_TOKENS },
			}),
			turn(9, Usage { cache_write: CACHE_MISS_WRITE_TOKENS, ..Usage::default() }, &[]),
		]),
		&OMP_PRICING,
		None,
	);
	let waste = waste_of(&record);
	assert_eq!(waste.cache_misses, 1);
	assert_eq!(waste.cache_miss_idle_minutes, [9]);
}

#[test]
fn models_db_is_read_again_when_it_changes_one_that_fails_to_open_isnt_kept() {
	let (_j, agent) = setup();
	let db = agent.join("models.db");
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "claude-test"), None);
	// An empty file opens but has no model_cache table yet.
	fs::write(&db, "").unwrap();
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "claude-test"), None);
	write_models_db(&agent, json!({ "anthropic": [{ "id": "claude-test", "contextWindow": 200_000 }] }));
	set_mtime(&db, t0());
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "claude-test"), Some(200_000));
	write_models_db(&agent, json!({ "anthropic": [{ "id": "claude-test", "contextWindow": 1_000_000 }] }));
	set_mtime(&db, t0() + 5000);
	assert_eq!(OMP_PRICING.window(Some("anthropic"), "claude-test"), Some(1_000_000));
}

#[test]
fn omps_price_of_a_model_per_million_tokens_provider_equal_or_prefixed_any_provider_when_none() {
	let (_j, agent) = setup();
	write_models_db(
		&agent,
		json!({
			"anthropic": [
				{ "id": "claude-test", "contextWindow": 200_000, "cost": { "input": 3, "output": 15, "cacheRead": 0.3, "cacheWrite": 3.75 } },
				{ "id": "no-price", "contextWindow": 200_000 },
			],
			"github-copilot:models-v2:abc123": [{ "id": "gpt-test", "cost": { "input": 2, "output": 8 } }],
		}),
	);
	let cost = |input, output, cache_read, cache_write| Some(ModelCost { input, output, cache_read, cache_write });
	assert_eq!(omp_model_cost(Some("anthropic"), "claude-test"), cost(3.0, 15.0, 0.3, 3.75));
	assert_eq!(omp_model_cost(Some("github-copilot"), "gpt-test"), cost(2.0, 8.0, 0.0, 0.0));
	assert_eq!(omp_model_cost(None, "gpt-test"), cost(2.0, 8.0, 0.0, 0.0));
	assert_eq!(omp_model_cost(Some("anthropic"), "gpt-test"), None);
	assert_eq!(omp_model_cost(Some("anthropic"), "no-price"), None);
	// A model with a price but no window has no window.
	assert_eq!(OMP_PRICING.window(Some("github-copilot"), "gpt-test"), None);
}

#[test]
fn forgetting_stale_sessions_drops_kept_parses_of_files_unchanged_since_then_except_those_kept() {
	let (_j, agent) = setup();
	write_models_db(&agent, json!({}));
	let file = write_thread(&agent);
	set_mtime(&file, t0());
	let spend = |file: &Path| thread_ledger(&[file.to_path_buf()]).unwrap().spend;
	close(spend(&file), 1.84);
	// Same size and mtime: only a forgotten parse sees the change.
	let text = fs::read_to_string(&file).unwrap().replace(r#""total":0.5}"#, r#""total":0.9}"#);
	fs::write(&file, text).unwrap();
	set_mtime(&file, t0());
	let kept = file.clone();
	forget_stale_sessions(t0() + 1, Some(&move |path: &Path| path == kept));
	close(spend(&file), 1.84);
	forget_stale_sessions(t0(), None);
	close(spend(&file), 1.84);
	forget_stale_sessions(t0() + 1, None);
	close(spend(&file), 2.24);
}

/// A pricing whose model has a window of 0.
struct ZeroWindow;

impl LedgerPricing for ZeroWindow {
	fn cost(&self, _r: &RequestFields, _usage: &Map<String, Value>) -> Option<f64> {
		None
	}

	fn window(&self, _provider: Option<&str>, _model: &str) -> Option<u64> {
		Some(0)
	}
}

#[test]
fn a_window_of_0_is_kept_as_0_with_no_percent_and_the_line_names_no_window() {
	let record = read_omp_session(
		&lines(&[json!({ "type": "session", "id": "s" }), turn(0, Usage { input: 400, ..Usage::default() }, &[])]),
		&ZeroWindow,
		None,
	);
	let summary = summarize_session(&record, &ZeroWindow);
	assert_eq!(summary.peak_context, peak(400, Some(0), None));
	let json = serde_json::to_value(&summary.peak_context).unwrap();
	assert_eq!(json, json!({ "tokens": 400, "window": 0, "percent": null }));
	let ledger = ThreadLedger {
		spend: 0.0,
		requests: 0,
		cache_read_share: None,
		peak_context: Some(summary.peak_context),
		waste: Waste::default(),
	};
	assert!(ledger_line(&ledger).contains("peak context 0k"), "{}", ledger_line(&ledger));
}
