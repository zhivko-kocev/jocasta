//! Tests of `app.rs`'s layout.

use super::*;

fn area(width: u16, height: u16) -> Rect {
	Rect { x: 0, y: 0, width, height }
}

#[test]
fn the_dock_is_35_percent_of_the_window_clamped_to_46_72_beside_a_pane_at_least_44_wide_0717() {
	// A wide window: 35 % of 200 is 70 columns.
	let wide = layout(area(200, 50), Dock::Shut);
	assert_eq!(wide.dock.map(|dock| dock.width), Some(70));
	assert_eq!((wide.sidebar.width, wide.pane.width), (28, 102));
	assert!(!wide.over);
	// Wider still: at most 72.
	assert_eq!(layout(area(400, 50), Dock::Shut).dock.map(|dock| dock.width), Some(72));
	// 35 % of 120 is 42, so 46; the pane keeps 46.
	let middle = layout(area(120, 40), Dock::Shut);
	assert_eq!((middle.dock.map(|dock| dock.width), middle.pane.width), (Some(46), 46));
	// The narrowest window with the dock beside the pane.
	assert_eq!(layout(area(118, 40), Dock::Shut).pane.width, PANE_MIN);
	assert_eq!(layout(area(117, 40), Dock::Shut).dock, None);
	// Not the boss's tab: no dock, however wide.
	let other = layout(area(200, 50), Dock::None);
	assert_eq!((other.dock, other.docked, other.pane.width), (None, false, 172));
}

#[test]
fn a_window_too_narrow_for_the_dock_lays_it_over_the_pane_only_while_it_has_the_keys_0717() {
	for (width, sidebar) in [(80, 26), (59, 0)] {
		let shut = layout(area(width, 30), Dock::Shut);
		assert_eq!((shut.dock, shut.docked, shut.over), (None, true, false), "{width}");
		let open = layout(area(width, 30), Dock::Open);
		assert!(open.over, "{width}");
		assert_eq!(open.sidebar.width, sidebar);
		// Over the whole pane, which keeps its size.
		assert_eq!(open.dock, Some(open.pane), "{width}");
		assert_eq!(open.pane.width, width - sidebar);
		assert_eq!(open.pane.height, 29);
	}
}
