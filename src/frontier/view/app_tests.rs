//! The window's state without a display: what an action's answer changes and what it leaves, the first-open entrance,
//! and (ignored, by hand) every fixture brief drawn offscreen with iced's tiny-skia into PNGs to look at.

use serde_json::json;

use super::*;
use crate::frontier::brief::layout::lay_out;
use crate::frontier::brief::model::{
	Anchor, DiffHunk, DiffLine, Kind, MapItem, Mark, NodeState, Proof, Published, RanProof, Severity, WorksRow,
};
use crate::shared::testing::temp_jocasta;

fn part(id: &str, label: &str, mark: Mark, line: usize) -> SystemNode {
	let anchor = Anchor {
		file: "src/billing/invoice_reconciliation/handlers.rs".to_owned(),
		line,
		quote: "fn reconcile(".to_owned(),
	};
	SystemNode { id: id.to_owned(), label: label.to_owned(), mark, anchor, layer: 0, order: 0 }
}

fn arrow(from: &str, to: &str, label: Option<&str>, mark: Mark) -> SystemEdge {
	SystemEdge { from: from.to_owned(), to: to.to_owned(), label: label.map(str::to_owned), mark }
}

/// What a refund fix does to the billing system: the refunds filter is added, the old summing helper removed.
fn system() -> Section {
	Section::System {
		heading: Some("System".to_owned()),
		nodes: vec![
			part("api", "Invoices API", Mark::Kept, 12),
			part("reconcile", "reconcile()", Mark::Kept, 138),
			part("refunds", "Refund filter", Mark::Added, 141),
			part("ledger", "Ledger", Mark::Kept, 40),
			part("sum", "sum_lines()", Mark::Removed, 139),
		],
		edges: vec![
			arrow("api", "reconcile", Some("invoice"), Mark::Kept),
			arrow("reconcile", "refunds", None, Mark::Added),
			arrow("refunds", "ledger", Some("total"), Mark::Added),
			arrow("reconcile", "sum", None, Mark::Removed),
			arrow("sum", "ledger", Some("total"), Mark::Removed),
		],
		brings: vec![SystemRow {
			text: "A refunded line no longer counts toward the reconciled total.".to_owned(),
			ran: Some(RanProof {
				run: "cargo test -p billing reconciles_refunds".to_owned(),
				expect: "test result: ok".to_owned(),
				exit: Some(0),
				tail: "test result: ok. 1 passed; 0 failed".to_owned(),
			}),
			anchor: None,
		}],
		takes: vec![SystemRow {
			text: "sum_lines() is gone; two report jobs called it.".to_owned(),
			ran: None,
			anchor: Some(Anchor {
				file: "src/billing/reports.rs".to_owned(),
				line: 77,
				quote: "sum_lines(&invoice)".to_owned(),
			}),
		}],
		no_change: false,
	}
}

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
						label: "`mise run verify` in the clean room".to_owned(),
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
			system(),
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
	// His own (0711), right to left, with a word longer than any line (break-ui).
	let mut yours = card(
		"n5",
		Severity::P2,
		Kind::Yours,
		"התור של ההודעות שנכשלו צריך גבול; dead_letter_queue_retry_limit_per_subscription_endpoint_configuration_value",
	);
	yours.ran = None;
	yours.proof.ok = true;
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
					label: "`mise run verify` in the clean room".to_owned(),
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
					yours,
				],
			},
			Section::Verdict {
				heading: Some("Your review".to_owned()),
				summary: "Works as described; two comments on the retry limit.".to_owned(),
				submitted: None,
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
					node(
						"c",
						"client-a: export CSV with a much longer title than fits",
						NodeState::Pending,
						Some("https://meet.example.org/client-a-sync"),
					),
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
fn a_works_labels_command_is_drawn_as_code_without_its_backticks_and_an_odd_backtick_stays_as_written() {
	assert_eq!(
		code_parts("`mise run verify` in the clean room"),
		[("mise run verify", true), (" in the clean room", false)]
	);
	assert_eq!(code_parts("on this machine"), [("on this machine", false)]);
	assert_eq!(code_parts("a ` alone"), [("a ` alone", false)]);
}

#[test]
fn text_that_mixes_directions_wraps_at_any_glyph_and_other_text_at_words() {
	let mixed =
		"התור של ההודעות שנכשלו צריך גבול; dead_letter_queue_retry_limit_per_subscription_endpoint_configuration_value";
	assert_eq!(wrap_for(mixed), text::Wrapping::Glyph);
	assert_eq!(wrap_for("هذا النص يُقرأ من اليمين إلى اليسار، وفيه كلمة English في وسطه."), text::Wrapping::Glyph);
	assert_eq!(wrap_for("הטקסט הזה בעברית; 42."), text::Wrapping::WordOrGlyph);
	assert_eq!(wrap_for("Името на функцијата не кажува што прави; reconcile_refunds."), text::Wrapping::WordOrGlyph);
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
	let reword = Act::Help { card: "n1".to_owned(), text: "shorter".to_owned(), op: "polish" };
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
	assert_eq!(window.fold(&Fold::Diff(6)), None);
	let _ = window.update(Message::Toggle(Fold::Diff(6)));
	assert!(window.animating());
	let start = window.now;
	let _ = window.update(Message::Tick(start + FAST + std::time::Duration::from_millis(1)));
	assert_eq!(window.fold(&Fold::Diff(6)), Some(1.0));
	assert!(!window.animating());
	let _ = window.update(Message::Toggle(Fold::Diff(6)));
	let later = window.now;
	let _ = window.update(Message::Tick(later + FAST + std::time::Duration::from_millis(1)));
	assert_eq!(window.fold(&Fold::Diff(6)), None);
}

#[test]
fn the_system_switch_goes_to_before_over_220_ms_and_turns_back_from_where_it_is_0710() {
	let mut window = settled_window(thread_brief());
	let after = |window: &Window| window.sides.get(&2).map_or(1.0, |side| side.interpolate(0.0_f32, 1.0, window.now));
	assert_eq!(after(&window), 1.0, "After shows by default");
	assert!(!window.animating());
	let _ = window.update(Message::Side(2, false));
	assert!(window.animating());
	std::thread::sleep(std::time::Duration::from_millis(60));
	let _ = window.update(Message::Tick(Instant::now()));
	let mid = after(&window);
	assert!(mid > 0.0 && mid < 1.0, "half way: {mid}");
	let _ = window.update(Message::Side(2, true));
	assert!((after(&window) - mid).abs() < 0.2, "it turns back from where it is, not from Before");
	let _ = window.update(Message::Tick(window.now + BASE + std::time::Duration::from_millis(1)));
	assert_eq!(after(&window), 1.0);
	assert!(!window.animating(), "settled: no more frames");
}

#[test]
fn the_add_box_sends_his_comment_with_its_place_and_empties_once_the_card_is_in_0711() {
	let mut window = settled_window(pr_brief());
	window.editors.insert("n1".to_owned(), text_editor::Content::with_text("unsaved on n1"));
	window.add = text_editor::Content::with_text("  Name the limit.  ");
	let _ = window.update(Message::AddFile("src/billing/invoice_reconciliation/handlers.rs".to_owned()));
	let _ = window.update(Message::AddLine("14a2".to_owned()));
	assert_eq!(window.add_line, "142", "the line box keeps digits only");
	let act = Act::Add {
		text: "Name the limit.".to_owned(),
		file: "src/billing/invoice_reconciliation/handlers.rs".to_owned(),
		line: "142".to_owned(),
	};
	assert_eq!(
		act.to_json(),
		json!({"op": "add", "text": "Name the limit.", "file": "src/billing/invoice_reconciliation/handlers.rs", "line": "142"})
	);
	assert_eq!(
		Act::Add { text: "x".to_owned(), file: String::new(), line: String::new() }.to_json(),
		json!({"op": "add", "text": "x"})
	);
	let _ = window.update(Message::Act(act.clone()));
	assert_eq!(window.busy, Some((Target::Add, "Adding\u{2026}")));
	let mut after = pr_brief();
	if let Section::Cards { cards, .. } = &mut after.sections[1] {
		cards.push(card("n6", Severity::P2, Kind::Yours, "Name the limit."));
	}
	let _ = window.update(Message::Answered(act, Ok(serde_json::to_value(&after).unwrap())));
	assert_eq!(window.editors["n6"].text().trim(), "Name the limit.");
	assert_eq!(window.editors["n1"].text().trim(), "unsaved on n1", "his words elsewhere stay");
	assert_eq!((words(&window.add).as_str(), window.add_file.as_str(), window.add_line.as_str()), ("", "", ""));
	assert!(!window.entering);
}

#[test]
fn the_summary_is_polished_saved_and_then_submitted_with_a_verdict_0711_0712() {
	let mut window = settled_window(pr_brief());
	let help = Act::Help { card: SUMMARY.to_owned(), text: "works, two comments".to_owned(), op: "english" };
	assert_eq!(Target::of(&help), Target::Verdict);
	assert_eq!(Target::of(&Act::Submit { verdict: "approve" }), Target::Verdict);
	let mut answer = serde_json::to_value(pr_brief()).unwrap();
	answer["suggestion"] = json!({"card": SUMMARY, "text": "It works; two comments."});
	let _ = window.update(Message::Answered(help, Ok(answer)));
	assert_eq!(window.suggestion, Some((SUMMARY.to_owned(), "It works; two comments.".to_owned())));
	// Accept saves the suggestion as the summary.
	let accept = Act::Summary { text: "It works; two comments.".to_owned() };
	assert_eq!(accept.to_json(), json!({"op": "summary", "text": "It works; two comments."}));
	let mut saved = pr_brief();
	if let Section::Verdict { summary, .. } = &mut saved.sections[2] {
		*summary = "It works; two comments.".to_owned();
	}
	let _ = window.update(Message::Answered(accept, Ok(serde_json::to_value(&saved).unwrap())));
	assert_eq!(words(&window.summary), "It works; two comments.");
	assert_eq!(window.suggestion, None);
	let submit = Act::Submit { verdict: "request_changes" };
	assert_eq!(submit.to_json(), json!({"op": "submit", "verdict": "request_changes"}));
	let _ = window.update(Message::Act(submit.clone()));
	assert_eq!(window.busy, Some((Target::Verdict, "Submitting\u{2026}")));
	let mut sent = saved.clone();
	if let Section::Verdict { submitted, .. } = &mut sent.sections[2] {
		*submitted = Some(Submitted {
			verdict: "request_changes".to_owned(),
			url: "https://github.com/example-org/acme-api/pull/7#pullrequestreview-1".to_owned(),
			at_ms: 1,
		});
	}
	let _ = window.update(Message::Answered(submit, Ok(serde_json::to_value(&sent).unwrap())));
	assert!(verdict_of(&window.brief).and_then(|(_, done)| done).is_some());
}

#[test]
fn show_its_code_opens_the_hunk_that_holds_the_cards_line_0712() {
	let brief = pr_brief();
	let hunk = hunk_of(&brief, "src/billing/invoice_reconciliation/handlers.rs", 142).expect("line 142 is in the hunk");
	assert_eq!(hunk.header, "@@ -138,7 +138,9 @@ fn reconcile(");
	assert!(hunk_of(&brief, "src/billing/invoice_reconciliation/handlers.rs", 900).is_none());
	assert!(hunk_of(&brief, "", 142).is_none(), "a card on the whole change has no code to show");
	let mut window = settled_window(brief);
	let _ = window.update(Message::Toggle(Fold::Code("n1".to_owned())));
	let _ = window.update(Message::Tick(window.now + FAST));
	assert_eq!(window.fold(&Fold::Code("n1".to_owned())), Some(1.0));
}

#[test]
fn play_walks_the_nodes_once_and_stops_at_the_last_and_prev_and_next_step_by_hand_0713() {
	let mut window = settled_window(morning_brief());
	let flow = 2;
	assert!(window.steps.is_empty() && window.playing.is_none());
	let _ = window.update(Message::Walk(flow, Walk::Play));
	assert_eq!((window.steps[&flow], window.playing), (0, Some(flow)));
	let _ = window.update(Message::Step);
	assert_eq!((window.steps[&flow], window.playing), (1, Some(flow)));
	let _ = window.update(Message::Step);
	assert_eq!((window.steps[&flow], window.playing), (2, None), "it stops at the last node");
	let _ = window.update(Message::Step);
	assert_eq!(window.steps[&flow], 2, "no wrap around");
	let _ = window.update(Message::Walk(flow, Walk::Prev));
	assert_eq!(window.steps[&flow], 1);
	let _ = window.update(Message::Walk(flow, Walk::Play));
	assert_eq!((window.steps[&flow], window.playing), (1, Some(flow)), "Play goes on from where he is");
	let _ = window.update(Message::Walk(flow, Walk::Next));
	assert_eq!((window.steps[&flow], window.playing), (2, None), "a step by hand stops Play");
	let _ = window.update(Message::Walk(flow, Walk::Play));
	assert_eq!(window.steps[&flow], 0, "from the end, Play starts again at the first");
	assert!(!window.entering && !window.animating(), "the walk moves nothing: no entrance, no frames");
	let Section::Flow { nodes, .. } = &window.brief.sections[flow] else { panic!("a flow") };
	let order = steps(nodes);
	assert_eq!(caption(&nodes[order[0]], 0, 3), "1 of 3 \u{b7} acme-api: refunds-total \u{b7} review");
}

#[test]
fn a_map_row_is_lit_while_the_pointer_is_on_it_and_a_late_leave_of_another_row_changes_nothing_0713() {
	let mut window = settled_window(thread_brief());
	let _ = window.update(Message::Hover("t".to_owned(), true));
	let _ = window.update(Message::Hover("h".to_owned(), false));
	assert_eq!(window.hovered.as_deref(), Some("t"));
	let _ = window.update(Message::Hover("t".to_owned(), false));
	assert_eq!(window.hovered, None);
}

/// `snapshot` as the PNG `path` (iced_test names its files `<stem>-<renderer>.png`; this renames it to `path`).
pub(in crate::frontier::view) fn write_png(snapshot: &iced_test::simulator::Snapshot, path: &std::path::Path) {
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
	let _ = thread.update(Message::Toggle(Fold::Diff(6)));
	let _ = thread.update(Message::Toggle(Fold::Row(2, "brings", 0)));
	let _ = thread.update(Message::Toggle(Fold::Proof("n1".to_owned())));
	let now = thread.now + FAST;
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
	// The system switched to Before: half way at 110 ms, then settled.
	let mut before = settled_window(thread_brief());
	let _ = before.update(Message::Side(2, false));
	let at = before.now;
	let _ = before.update(Message::Tick(at + std::time::Duration::from_millis(60)));
	shoot(&before, "system-switch-60ms", Size::new(1040.0, 1500.0));
	let _ = before.update(Message::Tick(at + BASE));
	shoot(&before, "system-before", Size::new(1040.0, 1500.0));

	let mut pr = settled_window(pr_brief());
	pr.suggestion = Some(("n1".to_owned(), "Failing deliveries are retried without end.".to_owned()));
	pr.banners.insert(Target::Card("n1".to_owned()), "publish: gh answered 422: line is outside the diff".to_owned());
	let _ = pr.update(Message::Toggle(Fold::Code("n1".to_owned())));
	let _ = pr.update(Message::Tick(pr.now + FAST));
	pr.add = text_editor::Content::with_text("Name the limit in the config, not in the code.");
	pr.add_file = "src/billing/invoice_reconciliation/handlers.rs".to_owned();
	pr.add_line = "142".to_owned();
	shoot(&pr, "pr", Size::new(1040.0, 3000.0));
	// break-ui: a narrow window, where the long word of the right-to-left card stays inside its box.
	let mut narrow_pr = settled_window(pr_brief());
	narrow_pr.width = 460.0;
	shoot(&narrow_pr, "pr-narrow", Size::new(460.0, 3600.0));
	// The review's thread is gone: the page is its record, no live buttons.
	let mut record = settled_window(pr_brief());
	record.live = Some(false);
	shoot(&record, "pr-record", Size::new(1040.0, 2400.0));
	// break-ui: at 720 px a right-to-left card ending in a long left-to-right word ran past its card (cosmic-text
	// wraps no word that starts a run of the other direction); it wraps at any glyph now. And a narrow window.
	record.width = 720.0;
	shoot(&record, "pr-record-720", Size::new(720.0, 3000.0));
	record.width = 460.0;
	shoot(&record, "pr-record-narrow", Size::new(460.0, 3600.0));
	// The summary typed, unsaved, with To English's suggestion; then submitted.
	let mut summary = settled_window(pr_brief());
	summary.summary = text_editor::Content::with_text("Работи; два коментари за границата.");
	summary.suggestion = Some((SUMMARY.to_owned(), "It works; two comments on the limit.".to_owned()));
	summary
		.banners
		.insert(Target::Verdict, "submit: write a summary first; GitHub needs one for this verdict.".to_owned());
	shoot(&summary, "pr-verdict", Size::new(1040.0, 3000.0));
	let mut sent = pr_brief();
	if let Section::Verdict { submitted, .. } = &mut sent.sections[2] {
		*submitted = Some(Submitted {
			verdict: "request_changes".to_owned(),
			url: "https://github.com/example-org/acme-api/pull/7#pullrequestreview-1".to_owned(),
			at_ms: 1,
		});
	}
	shoot(&settled_window(sent), "pr-submitted", Size::new(1040.0, 3000.0));
	let mut morning = settled_window(morning_brief());
	shoot(&morning, "morning", Size::new(1040.0, 1600.0));
	let _ = morning.update(Message::Walk(2, Walk::Next));
	let _ = morning.update(Message::Walk(2, Walk::Next));
	let _ = morning.update(Message::Walk(2, Walk::Next));
	shoot(&morning, "morning-walk", Size::new(1040.0, 1600.0));
	let _ = morning.update(Message::Walk(2, Walk::Prev));
	shoot(&morning, "morning-prev", Size::new(1040.0, 1600.0));
	// break-ui: long words, right to left, a five-layer flow, empty sections, a 1-line file beside a 600-line one.
	shoot(&settled_window(edge_brief()), "edges", Size::new(1040.0, 2200.0));
	let mut narrow = settled_window(edge_brief());
	narrow.width = 600.0;
	shoot(&narrow, "edges-narrow", Size::new(600.0, 2600.0));
}

/// A `show` brief at the edges (break-ui, 0713).
fn edge_brief() -> Brief {
	let chain = ["Request", "Gateway", "Auth", "Queue", "Worker"];
	let nodes: Vec<FlowNode> = chain
		.iter()
		.map(|label| node(&label.to_lowercase(), label, NodeState::Done, Some("https://status.example.org/acme")))
		.collect();
	let edges: Vec<FlowEdge> =
		chain.windows(2).map(|pair| edge(&pair[0].to_lowercase(), &pair[1].to_lowercase(), None)).collect();
	let mut brief = Brief {
		id: "show-edges".to_owned(),
		kind: "show".to_owned(),
		title: "Supercalifragilisticexpialidocious_reconciliation_of_refunded_invoice_lines_across_every_tenant"
			.to_owned(),
		subtitle:
			"https://example.org/workspaces/acme/projects/billing/boards/42?tab=comments&filter=unresolved&sort=newest"
				.to_owned(),
		generated_at_ms: 1_795_000_000_000,
		sections: vec![
			Section::Text {
				heading: Some("عربي: سطر من اليمين إلى اليسار".to_owned()),
				body: "هذا النص يُقرأ من اليمين إلى اليسار، وفيه كلمة English في وسطه.\n\nהטקסט הזה בעברית.".to_owned(),
			},
			Section::Facts { heading: Some("Facts".to_owned()), rows: vec![] },
			Section::Flow { heading: Some("Five layers".to_owned()), nodes, edges },
			Section::Checks {
				heading: Some("Checks".to_owned()),
				rows: vec![
					CheckRow { command: "mise run e2e".to_owned(), exit: None, ms: 0 },
					CheckRow {
						command: "cargo test --package billing_reconciliation_integration_tests_for_every_tenant"
							.to_owned(),
						exit: Some(0),
						ms: 0,
					},
				],
			},
			Section::Map {
				heading: Some("Files".to_owned()),
				groups: vec![MapGroup {
					label: "src".to_owned(),
					items: vec![
						MapItem {
							id: "big".to_owned(),
							label: "generated_schema.rs".to_owned(),
							added: 600,
							removed: 0,
						},
						MapItem {
							id: "tiny".to_owned(),
							label: "a_very_long_module_name_without_any_break.rs".to_owned(),
							added: 1,
							removed: 1,
						},
					],
				}],
				links: vec![],
			},
			Section::List { heading: Some("Links".to_owned()), rows: vec![] },
		],
	};
	lay_out(&mut brief).unwrap();
	brief
}
