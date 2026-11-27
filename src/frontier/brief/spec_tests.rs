//! Tests of `spec.rs`.

use serde_json::json;

use super::*;

fn refusal(spec: Value) -> String {
	let Value::Object(spec) = spec else { panic!("a spec is an object") };
	match read(&spec) {
		Ok(read) => panic!("read: {read:?}"),
		Err(Error::User { message, exit_code: 2 }) => message,
		Err(other) => panic!("{other:?}"),
	}
}

fn spec(sections: Value) -> Value {
	json!({ "title": "T", "sections": sections })
}

fn read_ok(spec: Value) -> Spec {
	let Value::Object(spec) = spec else { panic!("a spec is an object") };
	read(&spec).unwrap()
}

#[test]
fn empty_sections_missing_title_and_unknown_types_are_refused_0714() {
	assert_eq!(refusal(spec(json!([]))), "show: sections is empty; give at least one section.");
	assert_eq!(refusal(json!({ "sections": [] })), "show: title is required.");
	assert_eq!(
		refusal(json!({ "title": "T" })),
		"show: sections is required: an array of sections, each with a `type`."
	);
	assert_eq!(
		refusal(spec(json!([{ "type": "cards", "cards": [] }]))),
		"show: section 1 has type `cards`; the types are text, facts, flow, map, diff, list."
	);
}

#[test]
fn an_unknown_key_is_named_wherever_it_is_0714() {
	assert_eq!(
		refusal(json!({ "title": "T", "subtitle": "x", "sections": [] })),
		"show: unknown field `subtitle`; a spec has title and sections."
	);
	let section = refusal(spec(json!([{ "type": "text", "body": "b", "colour": "red" }])));
	assert!(section.starts_with("show: section 1 (text) isn't well formed: unknown field `colour`"), "{section}");
	let row = refusal(spec(json!([{ "type": "facts", "rows": [{ "label": "a", "value": "b", "link": "x" }] }])));
	assert!(row.contains("unknown field `link`"), "{row}");
	let node = json!({ "id": "a", "label": "A", "state": "done", "layer": 3 });
	let layered = refusal(spec(json!([{ "type": "flow", "nodes": [node] }])));
	assert!(layered.contains("unknown field `layer`"), "{layered}");
}

#[test]
fn labels_and_diff_lines_have_limits_0714() {
	let long = |n: usize| "é".repeat(n);
	let flow = |label: String, edge: String| {
		spec(json!([{
			"type": "flow",
			"nodes": [{ "id": "a", "label": label, "state": "done" }, { "id": "b", "label": "B", "state": "done" }],
			"edges": [{ "from": "a", "to": "b", "label": edge }],
		}]))
	};
	read_ok(flow(long(80), long(24)));
	assert_eq!(
		refusal(flow(long(81), long(24))),
		"show: section 1 (flow): node `a` has a label longer than 80 characters"
	);
	assert_eq!(
		refusal(flow(long(80), long(25))),
		"show: section 1 (flow): the edge a → b has a label longer than 24 characters"
	);
	let facts = |label: String| spec(json!([{ "type": "facts", "rows": [{ "label": label, "value": "v" }] }]));
	read_ok(facts(long(40)));
	assert!(refusal(facts(long(41))).ends_with("is longer than 40 characters"));
	let diff = |n: usize| {
		let lines: Vec<Value> = (0..n).map(|i| json!({ "kind": "+", "new": i + 1, "text": "x" })).collect();
		json!({ "type": "diff", "files": [{ "path": "a.rs", "hunks": [{ "header": "@@", "lines": lines }] }] })
	};
	read_ok(spec(json!([diff(1_000), diff(1_000)])));
	assert_eq!(
		refusal(spec(json!([diff(1_000), diff(1_001)]))),
		"show: the diffs have 2001 lines; at most 2000 are drawn."
	);
}

#[test]
fn a_diff_line_kind_is_plus_minus_space_or_the_word_space_0714() {
	let line = |kind: &str| {
		spec(
			json!([{ "type": "diff", "files": [{ "path": "a", "hunks": [{ "header": "@@", "lines": [{ "kind": kind, "text": "x" }] }] }] }]),
		)
	};
	let read = read_ok(line("space"));
	let Part::Given(Section::Diff { files, .. }) = &read.sections[0] else { panic!("a diff") };
	assert_eq!(files[0].hunks[0].lines[0].kind, " ");
	for kind in ["+", "-", " "] {
		read_ok(line(kind));
	}
	assert!(refusal(line("~")).contains("a line's kind is `~`"));
}

#[test]
fn map_ids_and_links_are_checked_like_a_flows_0714() {
	let map = |items: Value, links: Value| {
		spec(json!([{ "type": "map", "groups": [{ "label": "src", "items": items }], "links": links }]))
	};
	let item = |id: &str| json!({ "id": id, "label": id, "added": 1, "removed": 0 });
	read_ok(map(json!([item("a"), item("b")]), json!([{ "a": "a", "b": "b" }])));
	assert_eq!(
		refusal(map(json!([item("a"), item("a")]), json!([]))),
		"show: section 1 (map): the map has two items with the id `a`"
	);
	assert_eq!(
		refusal(map(json!([item("a")]), json!([{ "a": "a", "b": "z" }]))),
		"show: section 1 (map): the link a – z names `z`, which is no item of the map"
	);
	let dangling = json!([{ "type": "flow", "nodes": [{ "id": "a", "label": "A", "state": "done" }], "edges": [{ "from": "a", "to": "z" }] }]);
	assert!(refusal(spec(dangling)).starts_with("show: section 1 (flow): the edge a → z ends at `z`"));
}

#[test]
fn a_diff_or_map_may_name_a_thread_instead_and_never_both_0714() {
	let read = read_ok(spec(json!([
		{ "type": "text", "body": "b" },
		{ "type": "diff", "thread": "login" },
		{ "type": "map", "heading": "Files", "thread": "login" },
	])));
	assert_eq!(read.sections[1], Part::Thread { of: Reference::Diff, heading: None, thread: "login".to_owned() });
	assert_eq!(
		read.sections[2],
		Part::Thread { of: Reference::Map, heading: Some("Files".to_owned()), thread: "login".to_owned() }
	);
	assert_eq!(read.references().collect::<Vec<_>>(), [(2, "login"), (3, "login")]);
	assert!(refusal(spec(json!([{ "type": "diff" }]))).ends_with("give files, or a thread to fill it from"));
	assert!(refusal(spec(json!([{ "type": "map", "groups": [], "thread": "x" }]))).ends_with("not both"));
	assert!(refusal(spec(json!([{ "type": "map", "links": [], "thread": "x" }]))).ends_with("not both"));
}

#[test]
fn a_read_flow_is_laid_out() {
	let read = read_ok(spec(json!([{
		"type": "flow",
		"nodes": [{ "id": "a", "label": "A", "state": "done" }, { "id": "b", "label": "B", "state": "active" }],
		"edges": [{ "from": "a", "to": "b" }],
	}])));
	let Part::Given(Section::Flow { nodes, .. }) = &read.sections[0] else { panic!("a flow") };
	assert_eq!(nodes.iter().map(|node| node.layer).collect::<Vec<_>>(), [0, 1]);
}
