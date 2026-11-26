use std::sync::Arc;

use super::*;
use crate::frontier::server::surface::NoteLink;

fn note(sound: Sound, link: Option<NoteLink>) -> Note {
	Note { title: "demo/Readme line".to_owned(), body: Some("done".to_owned()), sound, link }
}

#[test]
fn a_notification_that_needs_him_is_critical_with_a_sound_and_a_plain_one_is_neither() {
	assert_eq!(
		notify_send_argv(&note(Sound::Request, None)),
		[
			"notify-send",
			"--app-name=Jocasta",
			"-u",
			"critical",
			"--hint=string:sound-name:message-new-instant",
			"--",
			"demo/Readme line",
			"done"
		]
	);
	assert_eq!(
		notify_send_argv(&note(Sound::None, None)),
		["notify-send", "--app-name=Jocasta", "--", "demo/Readme line", "done"]
	);
}

#[test]
fn a_notification_with_a_link_offers_the_click_and_waits_for_it() {
	let link = NoteLink { url: "http://127.0.0.1:1/brief/thread-x".to_owned(), open: Arc::new(|| {}) };
	assert_eq!(
		notify_send_argv(&note(Sound::None, Some(link))),
		["notify-send", "--app-name=Jocasta", "-A", "default=Open", "--wait", "--", "demo/Readme line", "done"]
	);
}
