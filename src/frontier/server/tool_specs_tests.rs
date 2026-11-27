use serde_json::{Value, json};

use super::*;

/// The `tools/list` of the build that served these tools over MCP, as `JSON.stringify` printed them in each mode.
const CAPTURED: &str = include_str!("tool_specs_fixture.json");

fn listed(mode: &str) -> Vec<Value> {
	let fixture: Value = serde_json::from_str(CAPTURED).unwrap();
	fixture[mode]["tools_list"]["result"]["tools"].as_array().unwrap().clone()
}

/// Name, description and schema of each tool, in order, key for key.
fn same_as_before(mode: &str, role: Role) {
	let was: Vec<Value> = listed(mode)
		.into_iter()
		.map(
			|tool| json!({ "name": tool["name"], "description": tool["description"], "parameters": tool["inputSchema"] }),
		)
		.collect();
	let now: Vec<Value> = tools_for(role, true)
		.iter()
		.map(|spec| json!({ "name": spec.name, "description": spec.description.replace(LONG_RUN_NOTE, ""), "parameters": spec.parameters }))
		.collect();
	assert_eq!(
		now.iter().map(Value::to_string).collect::<Vec<_>>(),
		was.iter().map(Value::to_string).collect::<Vec<_>>()
	);
}

#[test]
fn each_role_gets_the_tools_and_schemas_the_mcp_server_listed() {
	same_as_before("plain", Role::Project);
	same_as_before("thread", Role::Thread);
	same_as_before("boss", Role::Boss);
}

#[test]
fn a_thread_without_the_layer_gets_history_search_review_note_and_review_try() {
	let names: Vec<&str> = tools_for(Role::Thread, false).iter().map(|spec| spec.name).collect();
	assert_eq!(names, ["history_search", "review_note", "review_try"]);
}

#[test]
fn the_welcome_form_has_name_label_description_and_parameters() {
	let value = tools_for(Role::Project, true)[0].to_value();
	assert_eq!(value["name"], "history_search");
	assert_eq!(value["label"], "history_search");
	assert!(value["description"].as_str().unwrap().starts_with("Search this project's history"));
	assert_eq!(value["parameters"]["type"], "object");
}

fn args(value: Value) -> Map<String, Value> {
	value.as_object().unwrap().clone()
}

#[test]
fn arguments_that_do_not_fit_the_schema_are_named() {
	let search = &tools_for(Role::Project, true)[0];
	assert_eq!(bad_arguments(search, &args(json!({}))), Some("history_search: missing query".to_owned()));
	assert_eq!(
		bad_arguments(search, &args(json!({ "query": "x", "limit": 0 }))),
		Some("history_search: limit must be an integer in range".to_owned())
	);
	assert_eq!(
		bad_arguments(search, &args(json!({ "query": "x", "other": 1 }))),
		Some("history_search: unknown argument other".to_owned())
	);
	assert_eq!(bad_arguments(search, &args(json!({ "query": "x", "limit": "5" }))), None);
}

#[test]
fn thread_start_arguments_are_typed() {
	let start = tools_for(Role::Boss, true).into_iter().find(|spec| spec.name == "thread_start").unwrap();
	let base = json!({ "project": "acme", "title": "t", "brief": "b" });
	assert_eq!(bad_arguments(&start, &args(base.clone())), None);
	let mut bad = base.clone();
	bad["done_when"] = json!([1]);
	assert_eq!(
		bad_arguments(&start, &args(bad)),
		Some("thread_start: done_when must be a list of non-empty strings".to_owned())
	);
	let mut bad = base;
	bad["heavy"] = json!("yes");
	assert_eq!(bad_arguments(&start, &args(bad)), Some("thread_start: heavy must be true or false".to_owned()));
}

#[test]
fn thread_start_takes_a_budget_of_at_least_one_dollar_0686() {
	let start = tools_for(Role::Boss, true).into_iter().find(|spec| spec.name == "thread_start").unwrap();
	let base = json!({ "project": "acme", "title": "t", "brief": "b", "budget": 5 });
	assert_eq!(bad_arguments(&start, &args(base)), None);
	assert_eq!(start.parameters["properties"]["budget"]["minimum"], json!(1));
}
