use super::*;

#[test]
fn the_curve_starts_fast_and_ends_at_its_ends() {
	assert_eq!((ease(-1.0), ease(0.0), ease(1.0), ease(2.0)), (0.0, 0.0, 1.0, 1.0));
	// A strong ease-out: well past half way at a quarter of the time, and never back.
	assert!(ease(0.25) > 0.6, "{}", ease(0.25));
	let mut last = 0.0;
	for step in 1..=100 {
		let now = ease(step as f32 / 100.0);
		assert!(now >= last, "the curve went back at {step}");
		last = now;
	}
}

#[test]
fn nodes_stagger_by_layer_up_to_five_and_everything_settles_by_305_ms() {
	let full = Motion::Full;
	assert_eq!(node_at(0.0, 0, full), (0.0, RISE_PX));
	// Layer 2 hasn't started at 49 ms; layer 1 has.
	assert_eq!(node_at(49.0, 2, full).0, 0.0);
	assert!(node_at(49.0, 1, full).0 > 0.0);
	// Layer 9 starts with layer 5.
	assert_eq!(node_at(140.0, 9, full), node_at(140.0, 5, full));
	assert_eq!(node_at(305.0, 9, full), (1.0, 0.0));
	assert_eq!(edges_at(79.0, full), (1.0, 0.0));
	assert_eq!(edges_at(300.0, full), (1.0, 1.0));
	assert_eq!(SETTLED_MS, 305.0);
	assert!(!settled(304.0) && settled(305.0));
}

#[test]
fn reduced_motion_keeps_the_fade_and_drops_the_rise_and_the_drawing() {
	let (opacity, rise) = node_at(60.0, 0, Motion::Reduced);
	assert!(opacity > 0.0 && opacity < 1.0);
	assert_eq!(rise, 0.0);
	let (fade, drawn) = edges_at(150.0, Motion::Reduced);
	assert!(fade > 0.0 && fade < 1.0);
	assert_eq!(drawn, 1.0);
}
