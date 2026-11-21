use serde_json::{Value, json};

use super::replay::{Block, SUBAGENT_MARK, normalize, reply, spread_of, sse};

fn turn_request(assistants: usize, last_user: &str) -> Value {
	let mut messages = vec![json!({ "role": "user", "content": "Replay the scripted session." })];
	for _ in 0..assistants {
		messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": "…" }] }));
		messages.push(
			json!({ "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "t", "content": last_user }] }),
		);
	}
	json!({ "tools": [{ "name": "read" }], "messages": messages, "stream": true })
}

#[test]
fn the_session_is_read_from_the_count_of_assistant_messages_and_the_edit_takes_the_reads_tag() {
	let Block::Tool { name, input } = &reply(&turn_request(0, ""))[0] else { panic!("a tool call") };
	assert_eq!((name.as_str(), &input["path"]), ("read", &json!("src/big.rs")));
	let Block::Tool { name, input } = &reply(&turn_request(4, "[notes.md#9F3A]\n1:# Notes"))[0] else {
		panic!("a tool call")
	};
	assert_eq!(name, "edit");
	assert!(input["input"].as_str().unwrap().starts_with("[notes.md#9F3A]\n"), "{input}");
	assert_eq!(reply(&turn_request(8, "")), vec![Block::Text("Replay done.".to_owned())]);
	assert_eq!(reply(&json!({ "messages": [] })), vec![Block::Text("ok".to_owned())]);
}

#[test]
fn a_subagent_reports_at_once_through_its_reporting_tool_with_the_required_fields() {
	let request = json!({
		"tools": [
			{ "name": "read" },
			{ "name": "yield", "input_schema": { "required": ["result", "data"], "properties": {
				"result": { "type": "string" }, "data": { "type": "object" } } } },
		],
		"messages": [{ "role": "user", "content": [{ "type": "text", "text": format!("{SUBAGENT_MARK}: answer done.") }] }],
	});
	assert_eq!(
		reply(&request),
		vec![Block::Tool { name: "yield".to_owned(), input: json!({ "result": "done", "data": {} }) }]
	);
}

#[test]
fn the_stream_carries_the_tool_calls_input_and_stops_for_the_tool() {
	let input = json!({ "path": "notes.md", "i": "Replaying a step" });
	let stream = sse(&[Block::Tool { name: "read".to_owned(), input: input.clone() }], 10);
	let data: Vec<Value> = stream
		.lines()
		.filter_map(|line| line.strip_prefix("data: "))
		.map(|d| serde_json::from_str(d).unwrap())
		.collect();
	let partial = data.iter().find(|d| d["delta"]["type"] == "input_json_delta").unwrap();
	assert_eq!(serde_json::from_str::<Value>(partial["delta"]["partial_json"].as_str().unwrap()).unwrap(), input);
	assert_eq!(data.iter().find(|d| d["type"] == "message_delta").unwrap()["delta"]["stop_reason"], "tool_use");
	assert_eq!(data.last().unwrap()["type"], "message_stop");
	let text = sse(&[Block::Text("hi".to_owned())], 10);
	assert!(text.contains("\"stop_reason\":\"end_turn\""));
}

#[test]
fn two_runs_of_one_session_hash_alike_and_a_changed_request_does_not() {
	let first = json!({ "metadata": { "user_id": "s-1" }, "system": "Today is 2026-11-18 08:01:02Z; took 312ms; id 3f2b1c9a-1d2e-4f00-9a7b-0c1d2e3f4a5b",
		"messages": [] });
	let second = json!({ "metadata": { "user_id": "s-2" }, "system": "Today is 2026-11-19 21:44:59Z; took 95ms; id 00000000-1111-2222-3333-444455556666",
		"messages": [] });
	assert_eq!(normalize(&first), normalize(&second));
	let changed = json!({ "system": "Today is 2026-11-18 08:01:02Z; took 312ms; id 3f2b1c9a-1d2e-4f00-9a7b-0c1d2e3f4a5b",
		"messages": [], "max_tokens": 64000 });
	assert_ne!(normalize(&first), normalize(&changed));
}

#[test]
fn the_bases_spread_is_widened_by_a_quarter_of_its_median() {
	let spread = spread_of(&[1000.0, 1100.0, 1200.0]).unwrap();
	assert!(spread.contains(&725.0) && spread.contains(&1475.0));
	assert!(!spread.contains(&1476.0) && !spread.contains(&724.0));
	assert!(spread_of(&[]).is_none());
}
