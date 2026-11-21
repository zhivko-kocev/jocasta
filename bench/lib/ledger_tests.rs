//! `test/bench/ledger.test.ts`, case for case.

use serde_json::{Map, Value, json};

use super::*;
use crate::lib::prices::{
	ModelPrice, RequestTokens, candidate_keys, load_price_table, price_request, price_table_from,
};
use jocasta::shared::ledger::{
	CACHE_MISS_WRITE_TOKENS, OVERSIZED_RESULT_BYTES, PeakContext, Waste, arg_hash, normalize_path, paths_of,
	summarize_session,
};

fn t0() -> i64 {
	chrono::DateTime::parse_from_rfc3339("2026-11-09T10:00:00Z").unwrap().timestamp_millis()
}

fn iso(minutes: i64) -> String {
	chrono::DateTime::from_timestamp_millis(t0() + minutes * 60_000)
		.unwrap()
		.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn lines(entries: &[Value]) -> String {
	entries.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")
}

fn close(actual: f64, expected: f64) {
	assert!((actual - expected).abs() < 5e-13, "{actual} is not close to {expected}");
}

fn object(value: Value) -> Map<String, Value> {
	let Value::Object(map) = value else { panic!("not an object") };
	map
}

/// A small table: one model with a 1-hour cache write rate and a 200k tier, one without.
fn prices() -> PriceTable {
	price_table_from(object(json!({
		"claude-test": {
			"input_cost_per_token": 1e-6,
			"output_cost_per_token": 1e-5,
			"cache_read_input_token_cost": 1e-7,
			"cache_creation_input_token_cost": 2e-6,
			"cache_creation_input_token_cost_above_1hr": 3e-6,
			"input_cost_per_token_above_200k_tokens": 2e-6,
			"output_cost_per_token_above_200k_tokens": 2e-5,
			"max_input_tokens": 200_000,
		},
		"plain-model": { "input_cost_per_token": 1e-6, "output_cost_per_token": 1e-6 },
	})))
}

#[test]
fn the_committed_price_table_matches_its_pinned_sha_256_and_prices_the_arms_model_0538() {
	let table = load_price_table().unwrap();
	// Every model the model study runs has a price (0652), Fable 5.2 among them.
	for model in ["claude-fable-5-2", "claude-sonnet-5-5"] {
		assert_eq!(table.lookup(Some("anthropic"), model).unwrap().key, model);
	}
	let opus = table.lookup(Some("anthropic"), "claude-opus-5-5").unwrap();
	assert_eq!(opus.key, "claude-opus-5-5");
	assert_eq!((opus.input, opus.output, opus.cache_read, opus.cache_write), (4e-6, 2e-5, 2e-7, 5e-6));
	assert_eq!(opus.cache_write_1h, 8e-6);
	assert_eq!(opus.context_window, Some(1_000_000));
	// omp's model names, Claude Code's `[1m]` suffix and opencode's provider/model all find it.
	assert_eq!(table.lookup(None, "anthropic/claude-opus-5-5").unwrap().key, "claude-opus-5-5");
	assert_eq!(table.lookup(Some("anthropic"), "claude-opus-5-5[1m]").unwrap().key, "claude-opus-5-5");
	assert_eq!(table.lookup(Some("anthropic"), "no-such-model"), None);
	assert_eq!(candidate_keys(Some("anthropic"), "claude-x[1m]"), ["anthropic/claude-x", "claude-x"]);
}

#[test]
fn a_requests_price_cache_read_and_write_apart_a_1_hour_write_at_its_rate_the_200k_tier_past_200k() {
	let prices = prices();
	let price: ModelPrice = prices.lookup(None, "claude-test").expect("no test price");
	let tokens =
		RequestTokens { input: 1000, cache_read: 10_000, cache_write: 3000, cache_write_1h: 1000, output: 100 };
	close(
		price_request(&price, &tokens),
		1000.0 * 1e-6 + 10_000.0 * 1e-7 + 2000.0 * 2e-6 + 1000.0 * 3e-6 + 100.0 * 1e-5,
	);
	let past = RequestTokens { input: 250_000, cache_read: 0, cache_write: 0, cache_write_1h: 0, output: 10 };
	close(price_request(&price, &past), 250_000.0 * 2e-6 + 10.0 * 2e-5);
}

#[test]
fn paths_compare_without_omps_read_selectors_edits_name_their_files_arguments_hash_without_omps_intent() {
	assert_eq!(normalize_path("./src/A.ts:50-80"), "src/a.ts");
	assert_eq!(normalize_path("src\\a.ts:raw"), "src/a.ts");
	assert_eq!(normalize_path("D:/x/a.ts:2-4:raw"), "d:/x/a.ts");
	assert_eq!(normalize_path("D:/x/a.ts"), "d:/x/a.ts");
	assert_eq!(
		paths_of(&json!({ "input": "[src/a.ts#1A2B]\nPUT 1.=1:\n+x\n[src/b.ts#3C4D]\nCUT 2" })),
		["src/a.ts", "src/b.ts"]
	);
	assert_eq!(
		paths_of(&json!({ "patchText": "*** Begin Patch\n*** Update File: arc/x.ts\n@@\n*** Add File: arc/y.ts\n" })),
		["arc/x.ts", "arc/y.ts"]
	);
	assert_eq!(
		arg_hash(&json!({ "path": "a", "i": "Reading a" }), LedgerHarness::Omp),
		arg_hash(&json!({ "path": "a", "i": "Reading it again" }), LedgerHarness::Omp)
	);
	assert_eq!(
		arg_hash(&json!({ "b": 1, "a": 2 }), LedgerHarness::Claude),
		arg_hash(&json!({ "a": 2, "b": 1 }), LedgerHarness::Claude)
	);
	assert_ne!(
		arg_hash(&json!({ "path": "a", "i": "x" }), LedgerHarness::Claude),
		arg_hash(&json!({ "path": "a", "i": "y" }), LedgerHarness::Claude)
	);
}

/// One omp assistant message entry with usage and tool calls.
fn omp_turn(minutes: i64, usage: Value, calls: &[Value]) -> Value {
	let content: Vec<Value> = calls
		.iter()
		.map(|call| {
			let mut block = object(json!({ "type": "toolCall" }));
			block.extend(call.as_object().unwrap().clone());
			Value::Object(block)
		})
		.collect();
	json!({
		"type": "message",
		"timestamp": iso(minutes),
		"message": {
			"role": "assistant",
			"provider": "anthropic",
			"model": "claude-test",
			"timestamp": t0() + minutes * 60_000,
			"content": content,
			"usage": usage,
		},
	})
}

fn call(id: &str, name: &str, arguments: Value) -> Value {
	json!({ "id": id, "name": name, "arguments": arguments })
}

fn omp_result(id: &str, text: &str, is_error: bool) -> Value {
	json!({
		"type": "message",
		"timestamp": iso(0),
		"message": {
			"role": "toolResult",
			"toolCallId": id,
			"toolName": "x",
			"content": [{ "type": "text", "text": text }],
			"isError": is_error,
		},
	})
}

#[test]
fn omp_requests_side_requests_tool_calls_waste_and_the_peak_context_of_one_session_file() {
	let prices = prices();
	let pricing = bench_pricing(&prices);
	let text = lines(&[
		json!({ "type": "title", "title": "t" }),
		json!({ "type": "session", "id": "s-omp", "timestamp": iso(0) }),
		omp_turn(
			0,
			json!({ "input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 20_000, "cttl": { "ephemeral1h": 0 } }),
			&[
				call("r1", "read", json!({ "path": "src/a.ts", "i": "Reading" })),
				call("r2", "read", json!({ "path": "src/a.ts:10-20", "i": "Reading again" })),
			],
		),
		omp_result("r1", &"x".repeat(OVERSIZED_RESULT_BYTES), false),
		omp_result("r2", "y", false),
		omp_turn(
			1,
			json!({ "input": 5, "output": 5, "cacheRead": 20_000, "cacheWrite": 100 }),
			&[
				call("e1", "edit", json!({ "input": "[src/a.ts#1A2B]\nPUT 1.=1:\n+z" })),
				call("r3", "read", json!({ "path": "src/a.ts" })),
				call("b1", "bash", json!({ "command": "bun test" })),
				call("b2", "bash", json!({ "command": "bun test" })),
			],
		),
		omp_result("e1", "ok", false),
		omp_result("r3", "z", false),
		omp_result("b1", "fail", true),
		omp_result("b2", "fail", true),
		json!({
			"type": "model_usage",
			"timestamp": iso(4),
			"purpose": "cache-warm",
			"provider": "anthropic",
			"model": "claude-test",
			"usage": { "input": 2, "output": 5, "cacheRead": 20_100, "cacheWrite": 0 },
		}),
		json!({ "type": "compaction", "timestamp": iso(5), "tokensBefore": 100 }),
		// Twelve minutes after the last turn: a cache miss (a large write after more than 5 minutes idle).
		omp_turn(13, json!({ "input": 5, "output": 5, "cacheRead": 0, "cacheWrite": CACHE_MISS_WRITE_TOKENS }), &[]),
	]);
	let record = read_omp_session(&text, &pricing, None);
	assert_eq!(record.session, "s-omp");
	let purposes: Vec<&str> = record.requests.iter().map(|r| r.purpose.as_deref().unwrap_or("turn")).collect();
	assert_eq!(purposes, ["turn", "turn", "cache-warm", "turn"]);
	let tools: Vec<(&str, Option<&str>, Option<usize>, bool)> =
		record.tools.iter().map(|t| (t.tool.as_str(), t.path.as_deref(), t.result_bytes, t.error)).collect();
	assert_eq!(
		tools,
		[
			("read", Some("src/a.ts"), Some(OVERSIZED_RESULT_BYTES), false),
			("read", Some("src/a.ts"), Some(1), false),
			("edit", Some("src/a.ts"), Some(2), false),
			("read", Some("src/a.ts"), Some(1), false),
			("bash", None, Some(4), true),
			("bash", None, Some(4), true),
		]
	);
	let summary = summarize_session(&record, &pricing);
	// The range read right after the whole file is a re-read; the read after the edit isn't.
	assert_eq!(
		summary.waste,
		Waste {
			rereads: 1,
			repeated_calls: 1,
			oversized_results: 1,
			repeated_failures: 1,
			cache_misses: 1,
			// The cache-warm request at minute 4 is the last before the miss: 9 minutes idle.
			cache_miss_idle_minutes: vec![9],
			compactions: 1,
		}
	);
	// The cache-warm request's 20,102 tokens don't count: the agent's own peak is 20,105 at minute 1.
	assert_eq!(summary.peak_context, PeakContext { tokens: 20_105, window: Some(200_000), percent: Some(10.1) });
	assert_eq!(summary.requests, 4);
	close(summary.cache_read_share.unwrap(), 40_100.0 / (22.0 + 40_100.0 + 30_100.0));
	close(summary.cost, record.requests.iter().map(|r| r.cost.unwrap_or(0.0)).sum());
}

#[test]
fn claude_code_one_request_per_message_id_synthetic_skipped_results_and_compactions_read() {
	let prices = prices();
	let pricing = bench_pricing(&prices);
	let usage = json!({
		"input_tokens": 3,
		"output_tokens": 40,
		"cache_read_input_tokens": 1000,
		"cache_creation_input_tokens": 500,
		"cache_creation": { "ephemeral_1h_input_tokens": 500, "ephemeral_5m_input_tokens": 0 },
	});
	let text = lines(&[
		json!({ "type": "user", "sessionId": "s-claude", "timestamp": iso(0), "message": { "role": "user", "content": "fix it" } }),
		json!({
			"type": "assistant",
			"sessionId": "s-claude",
			"timestamp": iso(0),
			"message": { "id": "msg_1", "model": "claude-test", "usage": usage, "content": [{ "type": "thinking", "thinking": "…" }] },
		}),
		json!({
			"type": "assistant",
			"sessionId": "s-claude",
			"timestamp": iso(0),
			"message": {
				"id": "msg_1",
				"model": "claude-test",
				"usage": usage,
				"content": [{ "type": "tool_use", "id": "tu1", "name": "Read", "input": { "file_path": "D:\\r\\a.ts" } }],
			},
		}),
		json!({
			"type": "user",
			"sessionId": "s-claude",
			"timestamp": iso(1),
			"message": {
				"role": "user",
				"content": [{ "type": "tool_result", "tool_use_id": "tu1", "content": "abc", "is_error": false }],
			},
		}),
		json!({ "type": "system", "subtype": "compact_boundary", "sessionId": "s-claude", "timestamp": iso(2) }),
		json!({
			"type": "assistant",
			"sessionId": "s-claude",
			"timestamp": iso(3),
			"message": { "id": "synthetic", "model": "<synthetic>", "usage": { "input_tokens": 0, "output_tokens": 0 }, "content": [] },
		}),
	]);
	let record = read_claude_transcript(&text, &pricing, None);
	assert_eq!(record.session, "s-claude");
	assert_eq!(record.requests.len(), 1);
	let request = &record.requests[0];
	assert_eq!(
		(request.input, request.output, request.cache_read, request.cache_write, request.cache_write_1h),
		(3, 40, 1000, 500, 500)
	);
	close(request.cost.unwrap(), 3.0 * 1e-6 + 40.0 * 1e-5 + 1000.0 * 1e-7 + 500.0 * 3e-6);
	let tools: Vec<(&str, Option<&str>, Option<usize>)> =
		record.tools.iter().map(|t| (t.tool.as_str(), t.path.as_deref(), t.result_bytes)).collect();
	assert_eq!(tools, [("Read", Some("d:/r/a.ts"), Some(3))]);
	assert_eq!(record.compactions.len(), 1);
}

#[test]
fn opencode_each_step_finish_is_a_request_reasoning_as_output_tool_parts_are_calls_errors_read() {
	let prices = prices();
	let pricing = bench_pricing(&prices);
	let t = t0();
	let exported = json!({
		"info": { "id": "ses_x" },
		"messages": [
			{ "info": { "role": "user", "time": { "created": t } }, "parts": [{ "type": "text", "text": "fix" }] },
			{
				"info": { "role": "assistant", "providerID": "anthropic", "modelID": "claude-test", "time": { "created": t + 1000 } },
				"parts": [
					{ "type": "step-start" },
					{
						"type": "tool",
						"tool": "read",
						"state": {
							"status": "completed",
							"input": { "filePath": "src/a.ts" },
							"output": "abcd",
							"time": { "start": t + 2000, "end": t + 3000 },
						},
					},
					{
						"type": "step-finish",
						"tokens": { "input": 10, "output": 5, "reasoning": 7, "cache": { "read": 100, "write": 50 } },
						"cost": 0,
					},
					{
						"type": "tool",
						"tool": "apply_patch",
						"state": {
							"status": "error",
							"input": { "patchText": "*** Update File: src/a.ts\n" },
							"error": "no",
							"time": { "start": t + 4000, "end": t + 5000 },
						},
					},
					{
						"type": "step-finish",
						"tokens": { "input": 1, "output": 1, "reasoning": 0, "cache": { "read": 160, "write": 0 } },
						"cost": 0,
					},
				],
			},
		],
	});
	let record = read_opencode_export(&exported.to_string(), &pricing, None).unwrap();
	assert_eq!(record.session, "ses_x");
	let requests: Vec<(i64, u64, u64, u64, u64)> =
		record.requests.iter().map(|r| (r.at.unwrap() - t, r.input, r.output, r.cache_read, r.cache_write)).collect();
	assert_eq!(requests, [(3000, 10, 12, 100, 50), (5000, 1, 1, 160, 0)]);
	let tools: Vec<(&str, Option<&str>, Option<usize>, bool)> =
		record.tools.iter().map(|t| (t.tool.as_str(), t.path.as_deref(), t.result_bytes, t.error)).collect();
	assert_eq!(tools, [("read", Some("src/a.ts"), Some(4), false), ("apply_patch", Some("src/a.ts"), Some(2), true)]);
}

#[test]
fn a_trials_ledger_sums_its_sessions_keeps_the_largest_peak_and_lists_models_the_table_lacks() {
	let prices = prices();
	let pricing = bench_pricing(&prices);
	let main = read_omp_session(
		&lines(&[
			json!({ "type": "session", "id": "main" }),
			omp_turn(0, json!({ "input": 100, "output": 10, "cacheRead": 0, "cacheWrite": 0 }), &[]),
		]),
		&pricing,
		None,
	);
	let sub = read_omp_session(
		&lines(&[
			json!({ "type": "session", "id": "sub" }),
			json!({
				"type": "message",
				"timestamp": iso(1),
				"message": {
					"role": "assistant",
					"provider": "x",
					"model": "unknown-model",
					"content": [],
					"usage": { "input": 900, "output": 1 },
				},
			}),
		]),
		&pricing,
		Some("sub/agent"),
	);
	let ledger = trial_ledger(&[main, sub], &pricing);
	assert_eq!((ledger.totals.requests, ledger.totals.input, ledger.totals.output), (2, 1000, 11));
	assert_eq!(ledger.totals.unpriced_models, ["unknown-model"]);
	close(ledger.totals.cost, 100.0 * 1e-6 + 10.0 * 1e-5);
	assert_eq!(ledger.totals.peak_context, PeakContext { tokens: 900, window: None, percent: None });
	let agents: Vec<&str> = ledger.sessions.iter().map(|s| s.agent.as_str()).collect();
	assert_eq!(agents, ["main", "sub/agent"]);
	assert_eq!(ledger.rows.len(), 2);
	assert_eq!(price_table_from(Map::new()).lookup(None, "plain-model"), None);
}
