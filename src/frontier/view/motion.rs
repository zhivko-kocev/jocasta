//! The brief windows' motion (0721, R7's tokens): one curve, two durations, and the first-open entrance of a flow.
//! Everything here is a pure function of the time since the window opened, so a press never replays it.

use std::time::Duration;

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;

/// `base`: an edge drawing itself.
pub const BASE_MS: f32 = 220.0;
/// A node's fade and rise, and a disclosure opening.
pub const REVEAL_MS: f32 = 180.0;
/// How much later each layer's nodes start.
pub const STAGGER_MS: f32 = 25.0;
/// Layers past this one start with it.
pub const STAGGER_LAYERS: usize = 5;
/// When the edges start drawing.
pub const EDGE_DELAY_MS: f32 = 80.0;
/// How far a node rises while it fades in.
pub const RISE_PX: f32 = 4.0;
/// When everything of the entrance has settled: the last staggered node's end (125 + 180 ms).
pub const SETTLED_MS: f32 = STAGGER_MS * STAGGER_LAYERS as f32 + REVEAL_MS;

/// A disclosure's opening, as `iced::Animation` takes it.
pub const DISCLOSURE: Duration = Duration::from_millis(180);

/// One point of a cubic Bézier through (0,0), `(p1, ·)`, `(p2, ·)` and (1,1) on one axis.
fn bezier(p1: f32, p2: f32, t: f32) -> f32 {
	let u = 1.0 - t;
	3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

/// The one curve, `cubic-bezier(0.23, 1, 0.32, 1)`: progress `x` in 0..=1 to eased progress.
pub fn ease(x: f32) -> f32 {
	if x <= 0.0 {
		return 0.0;
	}
	if x >= 1.0 {
		return 1.0;
	}
	// The curve's x is monotonic in t: bisection finds the t whose x is `x`.
	let (mut low, mut high) = (0.0_f32, 1.0_f32);
	for _ in 0..24 {
		let mid = f32::midpoint(low, high);
		if bezier(0.23, 0.32, mid) < x {
			low = mid;
		} else {
			high = mid;
		}
	}
	bezier(1.0, 1.0, f32::midpoint(low, high))
}

/// Eased progress of a step that starts at `start` ms and lasts `length` ms, `elapsed` ms after the window opened.
fn progress(elapsed: f32, start: f32, length: f32) -> f32 {
	ease(((elapsed - start) / length).clamp(0.0, 1.0))
}

/// How much the windows may move: all of it, or opacity only (he asked for less).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
	Full,
	Reduced,
}

/// How a node of `layer` looks `elapsed` ms after the first open: its opacity and how far below its place it still is.
/// Reduced motion keeps the fade and drops the rise.
pub fn node_at(elapsed: f32, layer: usize, motion: Motion) -> (f32, f32) {
	let start = STAGGER_MS * layer.min(STAGGER_LAYERS) as f32;
	let shown = progress(elapsed, start, REVEAL_MS);
	match motion {
		Motion::Full => (shown, RISE_PX * (1.0 - shown)),
		Motion::Reduced => (shown, 0.0),
	}
}

/// How the edges look `elapsed` ms after the first open: their opacity and the drawn share of their length. Reduced
/// motion fades them in whole instead of drawing them.
pub fn edges_at(elapsed: f32, motion: Motion) -> (f32, f32) {
	let shown = progress(elapsed, EDGE_DELAY_MS, BASE_MS);
	match motion {
		Motion::Full => (1.0, shown),
		Motion::Reduced => (shown, 1.0),
	}
}

/// Whether the entrance has settled `elapsed` ms after the first open.
pub fn settled(elapsed: f32) -> bool {
	elapsed >= SETTLED_MS.max(EDGE_DELAY_MS + BASE_MS)
}

/// How much he lets the windows move: `JOCASTA_REDUCED_MOTION=1` reduces it (iced exposes no desktop setting for it).
pub fn motion() -> Motion {
	match crate::shared::env::var("JOCASTA_REDUCED_MOTION") {
		Some(value) if value.trim() == "1" => Motion::Reduced,
		_ => Motion::Full,
	}
}
