use super::*;

fn node(id: &str, layer: usize, order: usize) -> FlowNode {
	FlowNode {
		id: id.to_owned(),
		label: id.to_owned(),
		group: None,
		state: NodeState::Done,
		detail: None,
		layer,
		order,
	}
}

fn edge(from: &str, to: &str) -> FlowEdge {
	FlowEdge { from: from.to_owned(), to: to.to_owned(), label: Some("calls".to_owned()) }
}

#[test]
fn layers_run_left_to_right_and_a_short_layer_is_centred_across() {
	let nodes = [node("a", 0, 0), node("b", 1, 0), node("c", 1, 1)];
	let shape = shape(&nodes, &[edge("a", "b"), edge("a", "c")], Direction::Across);
	let [a, b, c] = [shape.boxes[0], shape.boxes[1], shape.boxes[2]];
	assert!(b.x > a.x + a.width && b.x == c.x, "{a:?} {b:?}");
	assert!(c.y > b.y + b.height);
	assert!((a.center_y() - (b.center_y() + c.center_y()) / 2.0).abs() < 0.01, "a sits between b and c");
	assert!(shape.size.width >= c.x + c.width && shape.size.height >= c.y + c.height);
	let first = &shape.curves[0];
	assert_eq!((first.from, first.to), (Point::new(a.x + a.width, a.center_y()), Point::new(b.x, b.center_y())));
	assert_eq!(first.label.as_deref(), Some("calls"));
}

#[test]
fn four_layers_or_a_narrow_window_go_top_down() {
	let three = [node("a", 0, 0), node("b", 1, 0), node("c", 2, 0)];
	assert_eq!(direction(&three, 1000.0), Direction::Across);
	assert_eq!(direction(&three, 600.0), Direction::Down);
	let four = [node("a", 0, 0), node("d", 3, 0)];
	assert_eq!(direction(&four, 1000.0), Direction::Down);
	let shape = shape(&three, &[edge("a", "b"), edge("c", "a")], Direction::Down);
	let (a, b) = (shape.boxes[0], shape.boxes[1]);
	assert!(b.y > a.y + a.height && a.x == b.x);
	assert_eq!(shape.curves[0].from, Point::new(a.center_x(), a.y + a.height));
	// Top down, an edge back loops out on the right, inside the drawing.
	let back = &shape.curves[1];
	assert!(back.c1.x > a.x + a.width && back.c1.x <= shape.size.width && back.beside, "{back:?}");
}

#[test]
fn an_edge_back_loops_out_and_an_edge_to_a_missing_node_is_left_out() {
	let nodes = [node("a", 0, 0), node("b", 1, 0)];
	let placed = shape(&nodes, &[edge("b", "a"), edge("a", "zz")], Direction::Across);
	assert_eq!(placed.curves.len(), 1);
	let back = &placed.curves[0];
	assert!(back.c1.y < back.from.y && back.c2.y < back.to.y, "{back:?}");
	assert!(shape(&[], &[], Direction::Across).boxes.is_empty());
}

#[test]
fn drawing_part_of_an_edge_follows_its_curve_and_long_text_is_cut_to_one_line() {
	let curve = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(20.0, 10.0), Point::new(30.0, 10.0)];
	assert_eq!(split(curve, 1.0), curve);
	assert_eq!(split(curve, 0.5)[3], at(curve, 0.5));
	assert_eq!(fit("short", 160.0, 13.0), "short");
	let cut = fit("A node label far longer than one line of its box", 160.0, 13.0);
	assert!(cut.ends_with('\u{2026}') && cut.chars().count() == 20, "{cut}");
	// A monospace glyph is 0.6 em: what is kept fits the room, so it never wraps onto the line under it.
	let label = fit("Stream invoices from Postgres", NODE_W - 24.0, style::LABEL);
	assert!(label.chars().count() as f32 * style::LABEL * 0.6 <= NODE_W - 24.0, "{label}");
	assert!(label.starts_with("Stream invoices from") && label.ends_with('\u{2026}'), "{label}");
}

#[test]
fn an_edge_label_stays_inside_the_drawing() {
	let nodes = [node("a", 0, 0), node("b", 1, 0)];
	let long = |from: &str, to: &str| FlowEdge {
		from: from.to_owned(),
		to: to.to_owned(),
		label: Some("retries until it gives up".to_owned()),
	};
	// Left to right an edge back loops over the boxes: its label sits above the loop, inside the top.
	let across = shape(&nodes, &[long("b", "a")], Direction::Across);
	let back = &across.curves[0];
	assert!(back.label_at.y - style::META * 1.4 >= 0.0, "{back:?}");
	// Top down a label sits beside its curve: the drawing is wide enough for it.
	let down = shape(&nodes, &[long("a", "b"), long("b", "a")], Direction::Down);
	for curve in &down.curves {
		let right = curve.label_at.x + 25.0 * style::META * 0.6;
		assert!(right <= down.size.width, "{curve:?} in {:?}", down.size);
	}
}
