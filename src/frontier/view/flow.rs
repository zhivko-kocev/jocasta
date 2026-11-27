//! A flow drawn on a canvas (0721, 0713): its nodes placed by `layout.rs`'s layers and orders, left to right (top down
//! with 4 or more layers or in a narrow window), curved edges with their labels, the active node ringed, the node Play
//! or Prev / Next is on ringed in the text colour, a node's detail drawn on it (a link when it is a web address), and the
//! first-open entrance of `motion.rs`. A system (0710) is drawn the same way, each part as present as its Before / After
//! switch makes it.

use std::borrow::Cow;
use std::collections::HashMap;

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Size, Theme, alignment, mouse};

use crate::frontier::brief::model::{FlowEdge, FlowNode, NodeState};
use crate::frontier::view::model::node_link;
use crate::frontier::view::motion::{Motion, edges_at, node_at};
use crate::frontier::view::style::{self, Colours, fade};

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;

/// A node's box.
pub const NODE_W: f32 = 200.0;
/// A node's height: its label and one line under it.
pub const NODE_H: f32 = 56.0;
/// Space between layers along the flow.
const LAYER_GAP: f32 = 64.0;
/// Space between nodes of one layer.
const NODE_GAP: f32 = 16.0;
/// Room around the drawing for the ring and the edges that loop back.
const PAD: f32 = 24.0;
/// The narrowest window a flow is drawn left to right in.
pub const NARROW: f32 = 720.0;

/// One edge's curve: start, two control points, end, and where its label sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
	pub from: Point,
	pub c1: Point,
	pub c2: Point,
	pub to: Point,
	pub label: Option<String>,
	pub label_at: Point,
	/// The label sits beside the curve (top down), not above it.
	pub beside: bool,
}

/// A flow placed: the drawing's size, each node's box (in the nodes' order) and each edge's curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
	pub size: Size,
	pub boxes: Vec<Rectangle>,
	pub curves: Vec<Curve>,
}

/// Which way a flow runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
	/// Left to right, a layer a column.
	Across,
	/// Top down, a layer a row.
	Down,
}

/// How a flow is drawn: top down with 4 or more layers or in a window narrower than 720 px, else left to right.
pub fn direction(nodes: &[FlowNode], width: f32) -> Direction {
	if nodes.iter().map(|node| node.layer).max().unwrap_or(0) >= 3 || width < NARROW {
		Direction::Down
	} else {
		Direction::Across
	}
}

/// The point of a cubic Bézier at `t`.
fn at(p: [Point; 4], t: f32) -> Point {
	let u = 1.0 - t;
	let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
	Point::new(a * p[0].x + b * p[1].x + c * p[2].x + d * p[3].x, a * p[0].y + b * p[1].y + c * p[2].y + d * p[3].y)
}

/// The first `t` of a cubic Bézier, as a Bézier of its own (de Casteljau).
pub fn split(p: [Point; 4], t: f32) -> [Point; 4] {
	let lerp = |a: Point, b: Point| Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
	let (ab, bc, cd) = (lerp(p[0], p[1]), lerp(p[1], p[2]), lerp(p[2], p[3]));
	let (abc, bcd) = (lerp(ab, bc), lerp(bc, cd));
	[p[0], ab, abc, lerp(abc, bcd)]
}

/// Places a flow whose nodes carry their layers and orders.
pub fn shape(nodes: &[FlowNode], edges: &[FlowEdge], direction: Direction) -> Shape {
	let layers = nodes.iter().map(|node| node.layer + 1).max().unwrap_or(0);
	let mut per_layer = vec![0_usize; layers];
	for node in nodes {
		per_layer[node.layer] = per_layer[node.layer].max(node.order + 1);
	}
	let widest = per_layer.iter().copied().max().unwrap_or(0) as f32;
	// Along the flow a layer takes a node and a gap; across it, the widest layer sets the room and the rest centre.
	let (along, across) = match direction {
		Direction::Down => (NODE_H, NODE_W),
		Direction::Across => (NODE_W, NODE_H),
	};
	let room = widest * (across + NODE_GAP) - NODE_GAP;
	let boxes: Vec<Rectangle> = nodes
		.iter()
		.map(|node| {
			let count = per_layer[node.layer] as f32;
			let offset = (room - (count * (across + NODE_GAP) - NODE_GAP)) / 2.0;
			let main = PAD + node.layer as f32 * (along + LAYER_GAP);
			let cross = PAD + offset + node.order as f32 * (across + NODE_GAP);
			let (x, y) = match direction {
				Direction::Down => (cross, main),
				Direction::Across => (main, cross),
			};
			Rectangle::new(Point::new(x, y), Size::new(NODE_W, NODE_H))
		})
		.collect();
	let length = PAD * 2.0 + layers as f32 * (along + LAYER_GAP) - LAYER_GAP;
	let breadth = PAD * 2.0 + room.max(0.0);
	// Top down, the edges that go back loop out on the right: room for them.
	let mut size = match direction {
		Direction::Down => Size::new(breadth + LOOP, length),
		Direction::Across => Size::new(length, breadth),
	};
	let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, node)| (node.id.as_str(), i)).collect();
	let curves = edges
		.iter()
		.filter_map(|edge| {
			let (a, b) = (*index.get(edge.from.as_str())?, *index.get(edge.to.as_str())?);
			let mut curve = curve(boxes[a], boxes[b], nodes[b].layer > nodes[a].layer, direction, edge.label.clone());
			hold_label(&mut curve, &mut size);
			Some(curve)
		})
		.collect();
	Shape { size, boxes, curves }
}

/// An edge label's height on the canvas.
const LABEL_H: f32 = style::META * 1.4;

/// Keeps `curve`'s label inside the drawing: moved in from the top and the left, the drawing grown to the right and
/// the bottom (a label past the canvas is clipped, and its leftovers stay on screen when the window scrolls).
fn hold_label(curve: &mut Curve, size: &mut Size) {
	let Some(label) = &curve.label else { return };
	let width = label.chars().count() as f32 * style::META * GLYPH;
	let at = &mut curve.label_at;
	let (left, top) = if curve.beside {
		at.y = at.y.max(LABEL_H / 2.0 + 2.0);
		(at.x, at.y - LABEL_H / 2.0)
	} else {
		at.x = at.x.max(width / 2.0 + 2.0);
		at.y = at.y.max(LABEL_H + 2.0);
		(at.x - width / 2.0, at.y - LABEL_H)
	};
	size.width = size.width.max(left + width + 4.0);
	size.height = size.height.max(top + LABEL_H + 4.0);
}

/// How far an edge that goes back loops out of the boxes, top down.
const LOOP: f32 = 72.0;

/// An edge from box `a` to box `b`: forward along the flow, or a loop out to the side when it goes back. A label sits
/// above the curve's middle left to right, beside it top down.
fn curve(a: Rectangle, b: Rectangle, forward: bool, direction: Direction, label: Option<String>) -> Curve {
	let down = direction == Direction::Down;
	let (from, to) = match (forward, down) {
		(true, true) => (Point::new(a.center_x(), a.y + a.height), Point::new(b.center_x(), b.y)),
		(false, true) => (Point::new(a.x + a.width, a.center_y()), Point::new(b.x + b.width, b.center_y())),
		_ => (Point::new(a.x + a.width, a.center_y()), Point::new(b.x, b.center_y())),
	};
	let (c1, c2) = match (forward, down) {
		(true, false) => {
			let dx = ((to.x - from.x) / 2.0).max(24.0);
			(Point::new(from.x + dx, from.y), Point::new(to.x - dx, to.y))
		}
		(true, true) => {
			let dy = ((to.y - from.y) / 2.0).max(24.0);
			(Point::new(from.x, from.y + dy), Point::new(to.x, to.y - dy))
		}
		(false, false) => {
			let lift = PAD.min(a.y.min(b.y)) + a.height / 2.0;
			(Point::new(from.x + 60.0, from.y - lift), Point::new(to.x - 60.0, to.y - lift))
		}
		(false, true) => (Point::new(from.x + LOOP, from.y), Point::new(to.x + LOOP, to.y)),
	};
	let middle = at([from, c1, c2, to], 0.5);
	let label_at = match direction {
		Direction::Down => Point::new(middle.x + 8.0, middle.y),
		Direction::Across => Point::new(middle.x, middle.y - 4.0),
	};
	Curve { from, c1, c2, to, label, label_at, beside: down }
}

/// The widest a glyph is on average, in ems: a monospace font's (ultron's font is monospace; a sans is narrower).
const GLYPH: f32 = 0.6;

/// `text` cut to what fits one line of `room` px at `size` px, with `…` when cut: a node's label and the line under
/// it never wrap onto each other.
pub fn fit(text: &str, room: f32, size: f32) -> String {
	let most = (room / (size * GLYPH)).floor().max(1.0) as usize;
	if text.chars().count() <= most {
		return text.to_owned();
	}
	let kept: String = text.chars().take(most.saturating_sub(1)).collect();
	format!("{}\u{2026}", kept.trim_end())
}

/// How present each node and edge is, in the flow's order: a node's opacity and scale, an edge's opacity. Empty lists
/// draw everything whole (a flow); a system's switch fills them (0710).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Presence {
	pub nodes: Vec<(f32, f32)>,
	pub edges: Vec<f32>,
}

/// The canvas program of one flow: what it draws `elapsed` ms after the window first opened.
pub struct FlowCanvas<'a, M> {
	pub nodes: Cow<'a, [FlowNode]>,
	pub shape: Shape,
	pub elapsed: f32,
	pub motion: Motion,
	pub presence: Presence,
	/// The node Play, Prev or Next is on (its index): a static ring in the text colour, no motion (0713).
	pub step: Option<usize>,
	/// What a click on a node's link sends (0713).
	pub on_link: fn(String) -> M,
}

fn state_colour(c: &Colours, state: NodeState) -> iced::Color {
	match state {
		NodeState::Done => c.green,
		NodeState::Active => c.blue,
		NodeState::Failed => c.red,
		NodeState::Pending => c.control,
	}
}

impl<M> FlowCanvas<'_, M> {
	/// The node under `point` whose detail is a link.
	fn link_at(&self, point: Point) -> Option<usize> {
		self.shape
			.boxes
			.iter()
			.position(|at| at.contains(point))
			.filter(|&index| self.nodes.get(index).is_some_and(|node| node_link(node).is_some()))
	}

	fn draw_edges(&self, frame: &mut Frame, c: &Colours) {
		let (whole_opacity, drawn) = edges_at(self.elapsed, self.motion);
		for (index, curve) in self.shape.curves.iter().enumerate() {
			let opacity = whole_opacity * self.presence.edges.get(index).copied().unwrap_or(1.0);
			if opacity <= 0.001 {
				continue;
			}
			let whole = [curve.from, curve.c1, curve.c2, curve.to];
			let part = if drawn < 1.0 { split(whole, drawn) } else { whole };
			let path = Path::new(|p| {
				p.move_to(part[0]);
				p.bezier_curve_to(part[1], part[2], part[3]);
			});
			frame.stroke(&path, Stroke::default().with_color(fade(c.control, opacity)).with_width(1.25));
			if drawn >= 1.0 {
				let head = Path::circle(curve.to, 2.5);
				frame.fill(&head, fade(c.control, opacity));
				if let Some(label) = &curve.label {
					frame.fill_text(Text {
						content: label.clone(),
						position: curve.label_at,
						color: fade(c.muted, opacity),
						size: style::META.into(),
						font: style::ui_font(),
						align_x: if curve.beside { alignment::Horizontal::Left } else { alignment::Horizontal::Center }
							.into(),
						align_y: if curve.beside { alignment::Vertical::Center } else { alignment::Vertical::Bottom },
						..Text::default()
					});
				}
			}
		}
	}

	/// The node at `index` in its box, as present (its opacity and its scale around its centre) as the switch makes it;
	/// its detail a link (blue, the text colour under the pointer) when it is a web address.
	fn draw_node(&self, frame: &mut Frame, c: &Colours, index: usize, hovered: bool) {
		let (node, at) = (&self.nodes[index], self.shape.boxes[index]);
		let shown = self.presence.nodes.get(index).copied().unwrap_or((1.0, 1.0));
		let (entered, rise) = node_at(self.elapsed, node.layer, self.motion);
		let opacity = entered * shown.0;
		if opacity <= 0.001 {
			return;
		}
		let size = Size::new(at.width * shown.1, at.height * shown.1);
		let at = Rectangle::new(Point::new(at.center_x() - size.width / 2.0, at.center_y() - size.height / 2.0), size);
		let top = Point::new(at.x, at.y + rise);
		let body = Path::rounded_rectangle(top, at.size(), style::RADIUS.into());
		frame.fill(&body, fade(c.raised, opacity));
		let edge = state_colour(c, node.state);
		frame.stroke(&body, Stroke::default().with_color(fade(edge, opacity)).with_width(1.0));
		if node.state == NodeState::Active {
			let ring = Path::rounded_rectangle(
				Point::new(top.x - 3.0, top.y - 3.0),
				Size::new(at.width + 6.0, at.height + 6.0),
				(style::RADIUS + 3.0).into(),
			);
			frame.stroke(&ring, Stroke::default().with_color(fade(c.blue, 0.45 * opacity)).with_width(2.0));
		}
		if self.step == Some(index) {
			let ring = Path::rounded_rectangle(
				Point::new(top.x - 4.0, top.y - 4.0),
				Size::new(at.width + 8.0, at.height + 8.0),
				(style::RADIUS + 4.0).into(),
			);
			frame.stroke(&ring, Stroke::default().with_color(fade(c.fg, opacity)).with_width(2.0));
		}
		let label = Text {
			content: fit(&node.label, at.width - 24.0, style::LABEL),
			position: Point::new(top.x + 12.0, top.y + 10.0),
			max_width: at.width - 24.0,
			color: fade(c.fg, opacity),
			size: style::LABEL.into(),
			font: style::ui_font(),
			..Text::default()
		};
		frame.fill_text(label);
		let link = node_link(node);
		let detail =
			link.as_deref().map_or(node.detail.as_deref(), |url| Some(url.split_once("://").map_or(url, |s| s.1)));
		let under = [node.group.as_deref(), detail].into_iter().flatten().collect::<Vec<_>>().join(" · ");
		if !under.is_empty() {
			let colour = match (&link, hovered) {
				(Some(_), true) => c.fg,
				(Some(_), false) => c.blue,
				(None, _) => c.muted,
			};
			frame.fill_text(Text {
				content: fit(&under, at.width - 24.0, style::META),
				position: Point::new(top.x + 12.0, top.y + 32.0),
				max_width: at.width - 24.0,
				color: fade(colour, opacity),
				size: style::META.into(),
				font: style::ui_font(),
				..Text::default()
			});
		}
	}
}

impl<M> canvas::Program<M> for FlowCanvas<'_, M> {
	/// The node whose link is under the pointer.
	type State = Option<usize>;

	fn update(
		&self,
		hovered: &mut Option<usize>,
		event: &canvas::Event,
		bounds: Rectangle,
		cursor: mouse::Cursor,
	) -> Option<canvas::Action<M>> {
		let under = cursor.position_in(bounds).and_then(|point| self.link_at(point));
		match event {
			canvas::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) if under != *hovered => {
				*hovered = under;
				Some(canvas::Action::request_redraw())
			}
			canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
				let url = node_link(&self.nodes[under?])?;
				Some(canvas::Action::publish((self.on_link)(url)).and_capture())
			}
			_ => None,
		}
	}

	fn draw(
		&self,
		hovered: &Option<usize>,
		renderer: &Renderer,
		_theme: &Theme,
		bounds: Rectangle,
		_cursor: mouse::Cursor,
	) -> Vec<Geometry> {
		let c = style::colours();
		let mut frame = Frame::new(renderer, bounds.size());
		// The software renderer repaints only what changed, by each shape's own outline: a ring's outer edge or an
		// edge label left of its anchor would be left behind after Prev / Next or a scroll. A clear fill over the
		// whole canvas makes every redraw of the flow repaint all of it.
		frame.fill_rectangle(Point::ORIGIN, bounds.size(), iced::Color::TRANSPARENT);
		self.draw_edges(&mut frame, &c);
		for index in 0..self.nodes.len().min(self.shape.boxes.len()) {
			self.draw_node(&mut frame, &c, index, *hovered == Some(index));
		}
		vec![frame.into_geometry()]
	}

	fn mouse_interaction(
		&self,
		_hovered: &Option<usize>,
		bounds: Rectangle,
		cursor: mouse::Cursor,
	) -> mouse::Interaction {
		match cursor.position_in(bounds).and_then(|point| self.link_at(point)) {
			Some(_) => mouse::Interaction::Pointer,
			None => mouse::Interaction::default(),
		}
	}
}
