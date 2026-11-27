use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use super::*;
use crate::frontier::shell::term::screen_text;

type Done = Box<dyn FnOnce(Result<(), String>) + Send>;

/// A sleeping pane whose wakes are held for the test to answer.
fn sleeper(text: &str) -> (Pane, Arc<AtomicUsize>, Arc<Mutex<Vec<Done>>>) {
	let (ui, _rx) = channel();
	let (asked, held) = (Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::<Done>::new())));
	let (count, hold) = (Arc::clone(&asked), Arc::clone(&held));
	let wake: Wake = Arc::new(move |done| {
		count.fetch_add(1, Ordering::SeqCst);
		locked(&hold).push(done);
	});
	(Pane::still(1, ui, (40, 8), text, wake), asked, held)
}

fn shown(pane: &Pane) -> String {
	screen_text(&pane.term.lock())
}

fn eventually(pane: &Pane, wanted: &str) {
	let deadline = Instant::now() + Duration::from_secs(2);
	while !shown(pane).contains(wanted) {
		assert!(Instant::now() < deadline, "{wanted:?} never shown in {:?}", shown(pane));
		std::thread::sleep(Duration::from_millis(10));
	}
}

#[test]
fn a_sleeping_pane_shows_its_text_and_asks_one_wake_at_a_time_saying_why_one_failed() {
	let (pane, asked, held) = sleeper("acme \u{1b}[31m· asleep");
	let text = shown(&pane);
	assert!(text.contains("acme [31m· asleep") && text.contains(WAKE_LINE), "{text:?}");
	assert!(pane.pid.is_none() && pane.asleep());

	pane.send(b"x");
	pane.send(b"y");
	assert_eq!(asked.load(Ordering::SeqCst), 1, "a key while a wake is under way asks nothing more");
	eventually(&pane, "waking...");

	let done = locked(&held).pop().expect("the wake was asked");
	done(Err("Jocasta isn't running.".to_owned()));
	eventually(&pane, "Jocasta isn't running.");
	pane.send(b"z");
	assert_eq!(asked.load(Ordering::SeqCst), 2, "after a failed wake a key asks again");
}

/// The `Ui::Exit` a pane running `argv` sends, waited for up to 10 s.
fn exit_of(argv: &[&str], kill: bool) -> bool {
	let (ui, rx) = channel();
	let argv: Vec<String> = argv.iter().map(|arg| (*arg).to_owned()).collect();
	let cwd = std::env::temp_dir();
	let launch = Launch { argv: &argv, cwd: &cwd, env: &[], cols: 40, rows: 8 };
	let pane = Pane::spawn(7, ui, &launch).expect("the pane starts");
	if kill {
		pane.close();
	}
	let deadline = Instant::now() + Duration::from_secs(10);
	loop {
		let left = deadline.saturating_duration_since(Instant::now());
		if let Ui::Exit(uid, clean) = rx.recv_timeout(left).expect("the pane's exit is told") {
			assert_eq!(uid, 7);
			return clean;
		}
	}
}

#[test]
fn a_panes_exit_says_whether_its_process_ended_cleanly_or_was_killed_0521() {
	let (done, waits): (&[&str], &[&str]) = if cfg!(windows) {
		(&["cmd", "/c", "exit 0"], &["cmd", "/c", "ping -n 30 127.0.0.1 >nul"])
	} else {
		(&["sh", "-c", "exit 0"], &["sh", "-c", "sleep 30"])
	};
	assert!(exit_of(done, false));
	assert!(!exit_of(waits, true));
}
