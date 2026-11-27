//! The window's state without a display: what an action's answer changes and what it leaves, the first-open entrance,
//! and (ignored, by hand) every fixture brief drawn offscreen with iced's tiny-skia into PNGs to look at.

use serde_json::json;

use super::*;
use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{
	DiffHunk, DiffLine, Kind, MapItem, NodeState, Proof, Published, RanProof, Severity, WorksRow,
};
use crate::shared::testing::temp_jocasta;

fn node(id: &str, label: &str, state: NodeState, detail: Option<&str>) -> FlowNode {
	FlowNode {
		id: id.to_owned(),
		label: label.to_owned(),
		group: None,
		state,
		detail: detail.map(str::to_owned),
		layer: 0,
		order: 0,
	}
}

fn edge(from: &str, to: &str, label: Option<&str>) -> FlowEdge {
	FlowEdge { from: from.to_owned(), to: to.to_owned(), label: label.map(str::to_owned) }
}

fn card(id: &str, severity: Severity, kind: Kind, text: &str) -> Card {
	Card {
		id: id.to_owned(),
		kind,
		severity,
		file: "src/billing/invoice_reconciliation/handlers.rs".to_owned(),
		line: 142,
		quote: "let total = lines.iter().map(|l| l.amount).sum::<i64>();".to_owned(),
		text: text.to_owned(),
		verdict: if kind == Kind::Finding { Verdict::Post } else { Verdict::Skip },
		proof: Proof { sha: "9f3c2a1be44".to_owned(), ok: kind == Kind::Finding },
		edited: None,
		published: None,
		ran: (kind == Kind::Finding).then(|| RanProof {
			run: "cargo test -p billing reconciles_refunds".to_owned(),
			expect: "test result: FAILED".to_owned(),
			exit: Some(101),
			tail: "thread 'reconciles_refunds' panicked at src/billing/tests.rs:88:5:\nassertion `left == right` failed\n  left: 1200\n right: 1000\ntest result: FAILED. 0 passed; 1 failed".to_owned(),
		}),
	}
}

fn diff() -> Section {
	let line = |kind: &str, old: Option<usize>, new: Option<usize>, text: &str| DiffLine {
		kind: kind.to_owned(),
		old,
		new,
		text: text.to_owned(),
	};
	Section::Diff {
		heading: Some("Code".to_owned()),
		files: vec![DiffFile {
			path: "src/billing/invoice_reconciliation/handlers.rs".to_owned(),
			hunks: vec![DiffHunk {
				header: "@@ -138,7 +138,9 @@ fn reconcile(".to_owned(),
				lines: vec![
					line(" ", Some(138), Some(138), "    let lines = invoice.lines();"),
					line("-", Some(139), None, "    let total = lines.iter().map(|l| l.amount).sum::<i64>();"),
					line("+", None, Some(139), "    let total = lines"),
					line("+", None, Some(140), "        .iter()"),
					line("+", None, Some(141), "        .filter(|l| !l.refunded)"),
					line("+", None, Some(142), "        .map(|l| l.amount).sum::<i64>();"),
					line(" ", Some(140), Some(143), "    Ok(total)"),
				],
			}],
		}],
	}
}

fn thread_brief() -> Brief {
	let mut brief = Brief {
		id: "thread-task-42".to_owned(),
		kind: "thread".to_owned(),
		title: "Refunds are left out of the reconciled total".to_owned(),
		subtitle: "acme-api · thread refunds-total · ready for review".to_owned(),
		generated_at_ms: 1_795_000_000_000,
		sections: vec![
			Section::Facts {
				heading: None,
				rows: vec![
					Fact { label: "Project".to_owned(), value: "acme-api".to_owned(), url: None },
					Fact { label: "Branch".to_owned(), value: "jocasta/refunds-total".to_owned(), url: None },
					Fact {
						label: "Pull request".to_owned(),
						value: "github.com/example-org/acme-api/pull/1287".to_owned(),
						url: Some("https://github.com/example-org/acme-api/pull/1287".to_owned()),
					},
					Fact { label: "Spent".to_owned(), value: "$1.84 of $5.00".to_owned(), url: None },
				],
			},
			Section::Works {
				heading: Some("Works".to_owned()),
				rows: vec![
					WorksRow {
						label: "mise run verify (clean room)".to_owned(),
						state: WorksState::Passed,
						ms: Some(48_200),
						tail: "test result: ok. 214 passed; 0 failed".to_owned(),
						screenshots: vec![],
					},
					WorksRow {
						label: "Refund an invoice line and reconcile".to_owned(),
						state: WorksState::Failed,
						ms: Some(1_250),
						tail: "expected \"total: 1000\", got exit 1: total: 1200".to_owned(),
						screenshots: vec!["reconcile.png".to_owned()],
					},
					WorksRow {
						label: "Load test".to_owned(),
						state: WorksState::NotRun,
						ms: None,
						tail: String::new(),
						screenshots: vec![],
					},
				],
			},
			Section::Flow {
				heading: Some("Where it is".to_owned()),
				nodes: vec![
					node("brief", "Brief", NodeState::Done, None),
					node("work", "Work", NodeState::Done, Some("14 turns")),
					node("check", "Checks", NodeState::Done, Some("verify 48 s")),
					node("review", "Review", NodeState::Active, Some("2 findings")),
					node("finish", "Merge or PR", NodeState::Pending, None),
				],
				edges: vec![
					edge("brief", "work", None),
					edge("work", "check", Some("done_when")),
					edge("check", "review", None),
					edge("review", "work", Some("send back")),
					edge("review", "finish", None),
				],
			},
			Section::Cards {
				heading: Some("Findings".to_owned()),
				cards: vec![
					card(
						"n1",
						Severity::P0,
						Kind::Finding,
						"Refunded lines still count toward the total, so a refunded invoice reconciles 200 high.",
					),
					card("n2", Severity::P2, Kind::Question, "Should a partial refund split the line, or mark it?"),
				],
			},
			Section::Map {
				heading: Some("Files".to_owned()),
				groups: vec![
					MapGroup {
						label: "billing".to_owned(),
						items: vec![
							MapItem { id: "h".to_owned(), label: "handlers.rs".to_owned(), added: 4, removed: 1 },
							MapItem { id: "t".to_owned(), label: "tests.rs".to_owned(), added: 38, removed: 0 },
						],
					},
					MapGroup {
						label: "docs".to_owned(),
						items: vec![MapItem {
							id: "c".to_owned(),
							label: "CHANGELOG.md".to_owned(),
							added: 1,
							removed: 0,
						}],
					},
				],
				links: vec![MapLink { a: "h".to_owned(), b: "t".to_owned() }],
			},
			diff(),
		],
	};
	lay_out(&mut brief).unwrap();
	brief
}

fn pr_brief() -> Brief {
	let mut published = card("n3", Severity::P1, Kind::Finding, "The retry loop never backs off.");
	published.published =
		Some(Published { url: "https://github.com/example-org/acme-api/pull/7#r1".to_owned(), at_ms: 0 });
	let mut opinion = card(
		"n4",
		Severity::P2,
		Kind::Opinion,
		"Името на функцијата не кажува што прави; reconcile_refunds би било појасно. 🙂",
	);
	opinion.file = String::new();
	Brief {
		id: "pr-acme-7".to_owned(),
		kind: "pr".to_owned(),
		title: "PR #7 by jo: Retry webhook deliveries with an exponential back-off and a dead-letter queue for the ones that keep failing".to_owned(),
		subtitle: "example-org/acme-api · 3 files · +120 −14".to_owned(),
		generated_at_ms: 1_795_000_000_000,
		sections: vec![
			Section::Works {
				heading: Some("Works".to_owned()),
				rows: vec![WorksRow {
					label: "mise run verify (clean room)".to_owned(),
					state: WorksState::Passed,
					ms: Some(61_000),
					tail: String::new(),
					screenshots: vec![],
				}],
			},
			Section::Cards {
				heading: Some("Comments".to_owned()),
				cards: vec![
					card("n1", Severity::P0, Kind::Finding, "A delivery that fails forever is retried forever."),
					published,
					opinion,
				],
			},
			Section::Checks { heading: Some("Checks".to_owned()), rows: vec![] },
			diff(),
		],
	}
}

fn morning_brief() -> Brief {
	let mut brief = Brief {
		id: "morning-2026-11-27".to_owned(),
		kind: "morning".to_owned(),
		title: "Friday, 27 November".to_owned(),
		subtitle: "3 threads underway · 1 needs you · $4.10 spent yesterday".to_owned(),
		generated_at_ms: 1_795_000_000_000,
		sections: vec![
			Section::Text {
				heading: None,
				body: "Two threads finished overnight; one waits for your Allow.\n\nThe nightly pipeline of acme-web is red since 03:12.".to_owned(),
			},
			Section::List {
				heading: Some("Today".to_owned()),
				rows: vec![
					ListRow { text: "09:30 Stand-up".to_owned(), url: None, note: Some("15 min".to_owned()) },
					ListRow {
						text: "Review requested: example-org/acme-web#88".to_owned(),
						url: Some("https://github.com/example-org/acme-web/pull/88".to_owned()),
						note: None,
					},
				],
			},
			Section::Flow {
				heading: Some("Work".to_owned()),
				nodes: vec![
					node("a", "acme-api: refunds-total", NodeState::Active, Some("review")),
					node("b", "acme-web: nightly is red", NodeState::Failed, Some("CI")),
					node("c", "client-a: export CSV with a much longer title than fits", NodeState::Pending, None),
				],
				edges: vec![],
			},
			Section::Checks {
				heading: Some("Checks".to_owned()),
				rows: vec![
					CheckRow { command: "mise run verify".to_owned(), exit: Some(0), ms: 48_000 },
					CheckRow { command: "mise run test".to_owned(), exit: Some(1), ms: 900 },
					CheckRow { command: "mise run e2e".to_owned(), exit: None, ms: 0 },
				],
			},
		],
	};
	lay_out(&mut brief).unwrap();
	brief
}

fn settled_window(brief: Brief) -> Window {
	let (mut window, _) = Window::open(brief);
	window.entering = false;
	window
}

#[test]
fn a_window_opens_with_its_cards_text_in_editors_and_only_a_flow_plays_an_entrance() {
	let (window, _) = Window::open(thread_brief());
	assert!(window.entering && window.animating());
	assert_eq!(
		window.editors["n1"].text().trim(),
		card_text(crate::frontier::view::model::card(&window.brief, "n1").unwrap())
	);
	let (plain, _) = Window::open(pr_brief());
	assert!(!plain.entering && !plain.animating(), "no flow, nothing moves, no frames asked for");
}

#[test]
fn an_answer_patches_its_card_keeps_his_unsaved_words_elsewhere_and_never_replays_the_entrance() {
	let mut window = settled_window(pr_brief());
	let opened = window.opened;
	window.editors.insert("n4".to_owned(), text_editor::Content::with_text("unsaved words"));
	let act = Act::Edit { card: "n1".to_owned(), text: "His words".to_owned() };
	let _ = window.update(Message::Act(act.clone()));
	assert_eq!(window.busy, Some((Target::Card("n1".to_owned()), "Saving\u{2026}")));
	// A second press while one waits does nothing.
	let _ = window.update(Message::Act(Act::Publish { card: "n1".to_owned() }));
	assert_eq!(window.busy.as_ref().map(|busy| busy.1), Some("Saving\u{2026}"));
	let mut after = pr_brief();
	if let Section::Cards { cards, .. } = &mut after.sections[1] {
		cards[0].edited = Some("His words".to_owned());
		cards[2].text = "someone else's change".to_owned();
	}
	let _ = window.update(Message::Answered(act, Ok(serde_json::to_value(&after).unwrap())));
	assert_eq!(window.busy, None);
	assert_eq!(window.editors["n1"].text().trim(), "His words");
	assert_eq!(window.editors["n4"].text().trim(), "unsaved words");
	let shown = |id: &str| card_text(crate::frontier::view::model::card(&window.brief, id).unwrap()).to_owned();
	assert_eq!(shown("n1"), "His words");
	assert!(shown("n4").starts_with("Името"), "only the acted-on card changes");
	assert!(!window.entering && window.opened == opened);
}

#[test]
fn a_refusal_shows_under_its_buttons_and_a_reword_brings_its_suggestion() {
	let mut window = settled_window(pr_brief());
	let reword = Act::Reword { card: "n1".to_owned(), instruction: "shorter".to_owned() };
	let _ = window
		.update(Message::Answered(reword.clone(), Err("Jocasta isn't running; start it with jocasta.".to_owned())));
	assert_eq!(window.banners[&Target::Card("n1".to_owned())], "Jocasta isn't running; start it with jocasta.");
	let mut answer = serde_json::to_value(pr_brief()).unwrap();
	answer["suggestion"] = json!({"card": "n1", "text": "Failing deliveries retry forever."});
	let _ = window.update(Message::Act(reword.clone()));
	assert!(window.banners.is_empty(), "a new press clears the old refusal");
	let _ = window.update(Message::Answered(reword, Ok(answer)));
	assert_eq!(window.suggestion, Some(("n1".to_owned(), "Failing deliveries retry forever.".to_owned())));
	let _ = window.update(Message::Dismiss);
	assert_eq!(window.suggestion, None);
}

#[test]
fn the_threads_buttons_take_the_whole_brief_and_a_closed_thread_shows_its_record_line() {
	let mut window = settled_window(thread_brief());
	window.send = text_editor::Content::with_text("Add a test for partial refunds.");
	let act = Act::Send { text: "Add a test for partial refunds.".to_owned() };
	let mut after = thread_brief();
	after.subtitle = "sent back".to_owned();
	let _ = window.update(Message::Answered(act, Ok(serde_json::to_value(&after).unwrap())));
	assert_eq!(window.brief.subtitle, "sent back");
	assert_eq!(window.send.text().trim(), "");
	let _ = window.update(Message::Live(Ok(false)));
	assert_eq!(window.live, Some(false));
	let _ = window.update(Message::Live(Err("Jocasta isn't running".to_owned())));
	assert_eq!(window.live, None, "no answer: the buttons stay and say why when pressed");
}

#[test]
fn a_disclosure_opens_over_its_duration_then_asks_for_no_more_frames() {
	let mut window = settled_window(thread_brief());
	assert_eq!(window.fold(&Fold::Diff(5)), None);
	let _ = window.update(Message::Toggle(Fold::Diff(5)));
	assert!(window.animating());
	let start = window.now;
	let _ = window.update(Message::Tick(start + DISCLOSURE + std::time::Duration::from_millis(1)));
	assert_eq!(window.fold(&Fold::Diff(5)), Some(1.0));
	assert!(!window.animating());
	let _ = window.update(Message::Toggle(Fold::Diff(5)));
	let later = window.now;
	let _ = window.update(Message::Tick(later + DISCLOSURE + std::time::Duration::from_millis(1)));
	assert_eq!(window.fold(&Fold::Diff(5)), None);
}

/// `snapshot` as the PNG `path` (iced_test names its files `<stem>-<renderer>.png`; this renames it to `path`).
fn write_png(snapshot: &iced_test::simulator::Snapshot, path: &std::path::Path) {
	let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
	let dir = path.parent().unwrap();
	let _ = std::fs::remove_file(path);
	assert!(snapshot.matches_image(path).expect("the PNG is written"));
	let written = std::fs::read_dir(dir)
		.unwrap()
		.flatten()
		.map(|entry| entry.path())
		.find(|file| file.file_name().is_some_and(|name| name.to_string_lossy().starts_with(&format!("{stem}-"))));
	std::fs::rename(written.expect("iced_test wrote it"), path).unwrap();
}

/// Draws `window` offscreen at `size` into `<temp>/jocasta-view-shots/<name>.png`.
fn shoot(window: &Window, name: &str, size: Size) {
	let path = std::env::temp_dir().join("jocasta-view-shots").join(format!("{name}.png"));
	let settings = iced::Settings { default_font: style::ui_font(), ..iced::Settings::default() };
	let mut simulator = iced_test::Simulator::with_size(settings, size, window.view());
	write_png(&simulator.snapshot(&style::theme()).expect("it draws"), &path);
}

/// By hand (`cargo nextest run --run-ignored only view_shots`): every fixture brief as a PNG to look at.
#[test]
#[ignore = "writes PNGs for a person to look at"]
fn view_shots() {
	let _j = temp_jocasta();
	let tall = Size::new(1040.0, 2600.0);
	// The screenshot a review_try left, itself drawn offscreen.
	let assets = crate::frontier::brief::store::brief_dir().join("assets").join("thread-task-42");
	std::fs::create_dir_all(&assets).unwrap();
	let c = colours();
	let fake: Element<'_, Message> = container(text("acme-web · Invoices · total 1 200").size(18).color(c.fg))
		.center(Fill)
		.style(move |_| container::Style { background: Some(c.panel.into()), ..container::Style::default() })
		.into();
	let mut simulator = iced_test::Simulator::with_size(iced::Settings::default(), Size::new(480.0, 270.0), fake);
	write_png(&simulator.snapshot(&style::theme()).unwrap(), &assets.join("reconcile.png"));

	let mut thread = settled_window(thread_brief());
	shoot(&thread, "thread", tall);
	let _ = thread.update(Message::Toggle(Fold::Diff(5)));
	let _ = thread.update(Message::Toggle(Fold::Proof("n1".to_owned())));
	let now = thread.now + DISCLOSURE;
	let _ = thread.update(Message::Tick(now));
	thread.width = 640.0;
	shoot(&thread, "thread-open-narrow", Size::new(640.0, 3200.0));
	thread.live = Some(false);
	thread.width = 1040.0;
	shoot(&thread, "thread-closed", Size::new(1040.0, 900.0));
	// Mid-entrance: 100 ms after the first open.
	let (mut entering, _) = Window::open(thread_brief());
	let _ = entering.update(Message::Tick(entering.opened + std::time::Duration::from_millis(100)));
	shoot(&entering, "thread-entrance-100ms", Size::new(1040.0, 1400.0));

	let mut pr = settled_window(pr_brief());
	pr.suggestion = Some(("n1".to_owned(), "Failing deliveries are retried without end.".to_owned()));
	pr.banners.insert(Target::Card("n1".to_owned()), "publish: gh answered 422: line is outside the diff".to_owned());
	shoot(&pr, "pr", tall);
	shoot(&settled_window(morning_brief()), "morning", Size::new(1040.0, 1600.0));
}
