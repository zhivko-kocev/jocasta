//! Tests of `layout.rs`.

use super::*;
use crate::frontier::brief::model::NodeState;

fn node(id: &str) -> FlowNode {
	FlowNode {
		id: id.to_owned(),
		label: id.to_owned(),
		group: None,
		state: NodeState::Pending,
		detail: None,
		layer: 0,
		order: 0,
	}
}

fn edge(from: &str, to: &str) -> FlowEdge {
	FlowEdge { from: from.to_owned(), to: to.to_owned(), label: None }
}

fn laid(ids: &[&str], edges: &[(&str, &str)]) -> Result<Vec<(usize, usize)>, String> {
	let mut nodes: Vec<FlowNode> = ids.iter().map(|id| node(id)).collect();
	let edges: Vec<FlowEdge> = edges.iter().map(|(from, to)| edge(from, to)).collect();
	lay_out_flow(&mut nodes, &edges)?;
	Ok(nodes.iter().map(|node| (node.layer, node.order)).collect())
}

#[test]
fn a_chain_is_one_layer_per_node() {
	assert_eq!(laid(&["a", "b", "c"], &[("a", "b"), ("b", "c")]).unwrap(), vec![(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn a_cycle_is_cut_at_its_closing_edge_so_a_b_c_get_layers_0_1_2() {
	let layers = laid(&["a", "b", "c"], &[("a", "b"), ("b", "c"), ("c", "a")]).unwrap();
	assert_eq!(layers, vec![(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn a_node_sits_on_the_longest_path_and_siblings_keep_input_order() {
	// a→b→d and a→d: d is on layer 2; b and c share layer 1 in input order.
	let layers = laid(&["a", "b", "c", "d"], &[("a", "b"), ("a", "d"), ("a", "c"), ("b", "d")]).unwrap();
	assert_eq!(layers, vec![(0, 0), (1, 0), (1, 1), (2, 0)]);
}

#[test]
fn a_self_loop_and_unlinked_nodes_are_fine() {
	assert_eq!(laid(&["a", "b"], &[("a", "a")]).unwrap(), vec![(0, 0), (0, 1)]);
}

#[test]
fn a_duplicate_id_an_unknown_edge_end_and_too_many_nodes_are_errors() {
	assert!(laid(&["a", "a"], &[]).unwrap_err().contains("two nodes with the id `a`"));
	assert!(laid(&["a"], &[("a", "z")]).unwrap_err().contains("`z`, which is no node"));
	assert!(laid(&["a"], &[("y", "a")]).unwrap_err().contains("`y`, which is no node"));
	let many: Vec<String> = (0..=MAX_NODES).map(|n| format!("n{n}")).collect();
	let refs: Vec<&str> = many.iter().map(String::as_str).collect();
	assert!(laid(&refs, &[]).unwrap_err().contains("at most 200"));
}
