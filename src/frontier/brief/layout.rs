//! Validation and layout of a brief's flows (0678, 0679) and systems (0710): each node's `layer` (the longest path
//! from the sources, an edge that closes a cycle ignored) and `order` (input order within its layer); a system is laid
//! out once over the union of its Before and After graphs. The errors are text for the boss's `show` tool.

use std::collections::HashMap;

use crate::frontier::brief::model::{Brief, FlowEdge, FlowNode, NodeState, Section, SystemEdge, SystemNode};

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;

/// The most nodes one flow may have.
pub const MAX_NODES: usize = 200;

/// Checks every flow and system of `brief` and writes the layers and orders into their nodes.
pub fn lay_out(brief: &mut Brief) -> Result<(), String> {
	for section in &mut brief.sections {
		match section {
			Section::Flow { nodes, edges, .. } => lay_out_flow(nodes, edges)?,
			Section::System { nodes, edges, .. } => lay_out_system(nodes, edges)?,
			_ => {}
		}
	}
	Ok(())
}

/// A system's union laid out as one flow, so a part keeps its place whichever graph is shown.
pub fn lay_out_system(nodes: &mut [SystemNode], edges: &[SystemEdge]) -> Result<(), String> {
	let mut flow: Vec<FlowNode> = nodes
		.iter()
		.map(|node| FlowNode {
			id: node.id.clone(),
			label: String::new(),
			group: None,
			state: NodeState::Pending,
			detail: None,
			layer: 0,
			order: 0,
		})
		.collect();
	let arrows: Vec<FlowEdge> =
		edges.iter().map(|edge| FlowEdge { from: edge.from.clone(), to: edge.to.clone(), label: None }).collect();
	lay_out_flow(&mut flow, &arrows)?;
	for (node, placed) in nodes.iter_mut().zip(flow) {
		node.layer = placed.layer;
		node.order = placed.order;
	}
	Ok(())
}

/// One flow's layers and orders; an error names the first problem found.
pub fn lay_out_flow(nodes: &mut [FlowNode], edges: &[FlowEdge]) -> Result<(), String> {
	if nodes.len() > MAX_NODES {
		return Err(format!("a flow has {} nodes; at most {MAX_NODES} are drawn", nodes.len()));
	}
	let mut at: HashMap<&str, usize> = HashMap::with_capacity(nodes.len());
	for (index, node) in nodes.iter().enumerate() {
		if at.insert(node.id.as_str(), index).is_some() {
			return Err(format!("the flow has two nodes with the id `{}`", node.id));
		}
	}
	let mut ends = Vec::with_capacity(edges.len());
	for edge in edges {
		let Some(&from) = at.get(edge.from.as_str()) else {
			return Err(format!(
				"the edge {} → {} starts at `{}`, which is no node of the flow",
				edge.from, edge.to, edge.from
			));
		};
		let Some(&to) = at.get(edge.to.as_str()) else {
			return Err(format!(
				"the edge {} → {} ends at `{}`, which is no node of the flow",
				edge.from, edge.to, edge.to
			));
		};
		ends.push((from, to));
	}
	let kept = acyclic(nodes.len(), &ends);
	let layers = layers_of(nodes.len(), &ends, &kept);
	let mut seen: HashMap<usize, usize> = HashMap::new();
	for (node, layer) in nodes.iter_mut().zip(layers) {
		let order = seen.entry(layer).or_insert(0);
		node.layer = layer;
		node.order = *order;
		*order += 1;
	}
	Ok(())
}

/// Which edges stay when the ones that close a cycle go: a depth-first search from each node in input order, taking
/// its edges in input order, drops an edge to a node still on the search's path.
fn acyclic(count: usize, ends: &[(usize, usize)]) -> Vec<bool> {
	let mut out: Vec<Vec<usize>> = vec![Vec::new(); count];
	for (index, (from, _)) in ends.iter().enumerate() {
		out[*from].push(index);
	}
	// 0 unseen, 1 on the path, 2 done.
	let mut state = vec![0_u8; count];
	let mut kept = vec![true; ends.len()];
	for root in 0..count {
		if state[root] != 0 {
			continue;
		}
		state[root] = 1;
		let mut path: Vec<(usize, usize)> = vec![(root, 0)];
		while let Some(&(node, next)) = path.last() {
			let Some(&edge) = out[node].get(next) else {
				state[node] = 2;
				path.pop();
				continue;
			};
			if let Some(top) = path.last_mut() {
				top.1 += 1;
			}
			let target = ends[edge].1;
			match state[target] {
				1 => kept[edge] = false,
				0 => {
					state[target] = 1;
					path.push((target, 0));
				}
				_ => {}
			}
		}
	}
	kept
}

/// The longest path from a source to each node over the kept edges. The kept edges have no cycle and a flow has at
/// most `MAX_NODES` nodes, so the recursion is shallow.
fn layers_of(count: usize, ends: &[(usize, usize)], kept: &[bool]) -> Vec<usize> {
	let mut into: Vec<Vec<usize>> = vec![Vec::new(); count];
	for (index, (from, to)) in ends.iter().enumerate() {
		if kept[index] {
			into[*to].push(*from);
		}
	}
	let mut layer: Vec<Option<usize>> = vec![None; count];
	(0..count).map(|node| layer_of(node, &into, &mut layer)).collect()
}

fn layer_of(node: usize, into: &[Vec<usize>], layer: &mut [Option<usize>]) -> usize {
	if let Some(known) = layer[node] {
		return known;
	}
	let found = into[node].iter().map(|from| layer_of(*from, into, layer) + 1).max().unwrap_or(0);
	layer[node] = Some(found);
	found
}
