//! Tests of `review_system.rs` without a thread: what a call may hand in, and the union of the two graphs.

use serde_json::json;

use super::*;

fn args(value: Value) -> Map<String, Value> {
	value.as_object().unwrap().clone()
}

fn anchor() -> Value {
	json!({ "file": "src/a.rs", "line": 1, "quote": "fn a() {}" })
}

fn graph(ids: &[&str], edges: &[(&str, &str)]) -> Graph {
	let nodes: Vec<Value> =
		ids.iter().map(|id| json!({ "id": id, "label": id.to_uppercase(), "anchor": anchor() })).collect();
	let edges: Vec<Value> =
		edges.iter().map(|(from, to)| json!({ "from": from, "to": to, "anchor": anchor() })).collect();
	serde_json::from_value(json!({ "nodes": nodes, "edges": edges })).unwrap()
}

fn refusal(result: Result<impl std::fmt::Debug>) -> String {
	result.unwrap_err().to_string()
}

#[test]
fn the_union_marks_each_part_kept_added_or_removed_and_lays_it_out_once() {
	let before = graph(&["api", "db", "cache"], &[("api", "db"), ("api", "cache")]);
	let after = graph(&["api", "db", "queue"], &[("api", "db"), ("api", "queue")]);
	let (nodes, edges) = union(&before, &after).unwrap();
	let marks: Vec<(&str, Mark)> = nodes.iter().map(|node| (node.id.as_str(), node.mark)).collect();
	assert_eq!(marks, [("api", Mark::Kept), ("db", Mark::Kept), ("queue", Mark::Added), ("cache", Mark::Removed)]);
	let edge_marks: Vec<(&str, Mark)> = edges.iter().map(|edge| (edge.to.as_str(), edge.mark)).collect();
	assert_eq!(edge_marks, [("db", Mark::Kept), ("queue", Mark::Added), ("cache", Mark::Removed)]);
	let layers: Vec<usize> = nodes.iter().map(|node| node.layer).collect();
	assert_eq!(layers, [0, 1, 1, 1], "one layout over both graphs");
	assert_eq!(nodes[0].label, "API", "a kept part takes After's label");
}

#[test]
fn a_graph_is_refused_for_too_many_nodes_a_twice_used_id_a_stray_edge_or_both_empty() {
	let ids: Vec<String> = (0..=MAX_NODES).map(|n| format!("n{n}")).collect();
	let big = graph(&ids.iter().map(String::as_str).collect::<Vec<_>>(), &[]);
	assert!(refusal(union(&graph(&[], &[]), &big)).contains("after has 13 nodes; at most 12"));
	assert!(refusal(union(&graph(&["a", "a"], &[]), &graph(&["b"], &[]))).contains("two nodes with the id `a`"));
	assert!(
		refusal(union(&graph(&["a"], &[]), &graph(&["b"], &[("b", "a")])))
			.contains("names `a`, which is no after node")
	);
	assert!(refusal(union(&graph(&[], &[]), &graph(&[], &[]))).contains("hand in no_system_change instead"));
}

#[test]
fn a_call_is_the_no_change_form_alone_or_the_two_graphs_with_known_keys() {
	assert!(matches!(given(&args(json!({ "no_system_change": true }))), Ok(Given::NoChange)));
	assert!(
		refusal(given(&args(json!({ "no_system_change": true, "before": {} })))).contains("comes alone"),
		"the two forms don't mix"
	);
	assert!(refusal(given(&args(json!({ "no_system_change": false })))).contains("comes alone"));
	assert!(refusal(given(&args(json!({ "graphs": [] })))).contains("unknown argument graphs"));
	assert!(refusal(given(&args(json!({ "after": { "nodes": [] } })))).contains("hand in before and after"));
	let unknown = json!({ "before": { "nodes": [] }, "after": { "nodes": [{ "id": "a", "label": "A", "anchor": anchor(), "kind": "x" }] } });
	assert!(refusal(given(&args(unknown))).contains("after: unknown field `kind`"));
	let ok =
		json!({ "before": { "nodes": [] }, "after": { "nodes": [] }, "brings": [{ "text": "t", "anchor": anchor() }] });
	let Ok(Given::Graphs { brings, takes, .. }) = given(&args(ok)) else { panic!("the graphs form") };
	assert_eq!((brings.len(), takes.len()), (1, 0), "takes may be left out");
}

#[test]
fn the_stored_view_becomes_the_pages_system_section() {
	let view = SystemReview { no_change: true, ..SystemReview::default() };
	let Section::System { heading, no_change, nodes, .. } = view.section() else { panic!("a system section") };
	assert_eq!((heading.as_deref(), no_change, nodes.len()), (Some("System"), true, 0));
}
