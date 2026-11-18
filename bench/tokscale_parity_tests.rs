//! `test/bench/tokscale-parity.test.ts`'s cases, case for case (the one that runs the script as a process is in
//! `tests/jocasta_bench.rs`).

use serde_json::json;

use super::*;
use crate::lib::ledger::bench_pricing;
use jocasta::shared::ledger::read_omp_session;
use jocasta::shared::testing::temp_dir;

const SESSION_ID: &str = "01b2c3d4-0000-7000-8000-00000000beef";

fn omp_session() -> String {
	[
		json!({ "type": "session", "version": 3, "id": SESSION_ID, "timestamp": "2026-11-09T10:00:00.000Z", "cwd": "C:\\t\\repo" }),
		json!({
			"type": "message",
			"id": "a1",
			"timestamp": "2026-11-09T10:00:01.000Z",
			"message": {
				"role": "assistant",
				"provider": "anthropic",
				"model": "claude-opus-5-5",
				"timestamp": chrono::DateTime::parse_from_rfc3339("2026-11-09T10:00:01.000Z").unwrap().timestamp_millis(),
				"content": [{ "type": "text", "text": "Done." }],
				"usage": { "input": 12, "output": 34, "cacheRead": 5600, "cacheWrite": 780, "totalTokens": 6426 },
			},
		}),
	]
	.iter()
	.map(Value::to_string)
	.collect::<Vec<_>>()
	.join("\n")
}

fn totals(input: f64, output: f64, cache_read: f64, cache_write: f64) -> SessionTotals {
	SessionTotals { input, cache_read, cache_write, output }
}

#[test]
fn tokscale_s_per_session_entries_sum_over_models_opencode_s_reasoning_joins_its_output() {
	let json = json!({
		"groupBy": "client,session,model",
		"entries": [
			{ "sessionId": "s1", "client": "claude", "model": "a", "input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "reasoning": 9 },
			{ "sessionId": "s1", "client": "claude", "model": "b", "input": 10, "output": 20, "cacheRead": 30, "cacheWrite": 40 },
			{ "sessionId": "ses_2", "client": "opencode", "model": "a", "input": 1, "output": 2, "cacheRead": 0, "cacheWrite": 0, "reasoning": 5 },
			{ "client": "codex", "model": "x", "input": 1 },
		],
	})
	.to_string();
	assert_eq!(
		tokscale_totals(&json).unwrap(),
		[
			("s1".to_owned(), TokscaleSession { client: "claude".into(), totals: totals(11.0, 22.0, 33.0, 44.0) }),
			("ses_2".to_owned(), TokscaleSession { client: "opencode".into(), totals: totals(1.0, 7.0, 0.0, 0.0) }),
		]
	);
}

#[test]
fn the_ledger_s_sessions_match_tokscale_s_by_id_or_an_id_holding_it_as_omp_s_file_names_do_differences_are_named() {
	let prices = load_price_table().unwrap();
	let session = read_omp_session(&omp_session(), &bench_pricing(prices), None);
	let ledger = ledger_totals(&[session]);
	assert_eq!(
		ledger,
		[(
			SESSION_ID.to_owned(),
			LedgerSession { harness: LedgerHarness::Omp, totals: totals(12.0, 34.0, 5600.0, 780.0) }
		)]
	);
	let same = [(format!("2026-11-09T10-00-00-000Z_{SESSION_ID}"), totals(12.0, 34.0, 5600.0, 780.0))];
	assert_eq!(compare_totals("t-a0-0", &ledger, &same)[0].differences, Vec::<String>::new());
	let off = [(SESSION_ID.to_owned(), totals(12.0, 35.0, 5600.0, 0.0))];
	assert_eq!(compare_totals("t-a0-0", &ledger, &off)[0].differences, ["cacheWrite", "output"]);
	let missing = &compare_totals("t-a0-0", &ledger, &[])[0];
	assert_eq!((missing.tokscale, missing.differences.clone()), (None, vec!["missing in tokscale".to_owned()]));
}

#[test]
fn tokscale_reads_an_omp_session_as_the_ledger_does_skipped_without_tokscale_on_path_bench_only_vm() {
	if on_path("tokscale", None).is_none() {
		println!("tokscale parity: skipped. {}", tokscale_missing());
		return;
	}
	let dir = temp_dir("jocasta-parity-");
	fs::write(dir.path().join(format!("2026-11-09T10-00-00-000Z_{SESSION_ID}.jsonl")), format!("{}\n", omp_session()))
		.unwrap();
	let session = read_omp_session(&omp_session(), &bench_pricing(load_price_table().unwrap()), None);
	let ledger = ledger_totals(&[session]);
	let found = tokscale_for(&[dir.path().to_string_lossy().into_owned()]).unwrap();
	let rows = compare_totals("fixture", &ledger, &found);
	assert_eq!(rows.iter().map(|row| row.differences.clone()).collect::<Vec<_>>(), [Vec::<String>::new()]);
}
