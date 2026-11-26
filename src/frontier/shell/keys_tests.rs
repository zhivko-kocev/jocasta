use super::*;

#[test]
fn the_shell_asks_for_the_shifted_key() {
	assert!(KEYBOARD_FLAGS.contains(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS));
}

#[test]
fn a_shifted_character_reaches_the_pane_as_typed() {
	// foot sends Shift+a as `CSI 97:65;2u` once the alternate key is asked for; crossterm reads it as `A` with Shift.
	let shifted = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
	let bang = KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT);
	for mode in [TermMode::NONE, TermMode::DISAMBIGUATE_ESC_CODES] {
		assert_eq!(encode_key(&shifted, mode), Some(b"A".to_vec()));
		assert_eq!(encode_key(&bang, mode), Some(b"!".to_vec()));
	}
}
