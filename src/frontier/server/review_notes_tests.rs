use serde_json::json;

use super::*;
use crate::shared::testing::temp_dir;

#[test]
fn a_quote_reads_when_its_lines_start_at_the_line_with_whitespace_collapsed() {
	let text = "fn a() {\n\tlet  x =   1;\n\tx + 1\n}\n";
	assert!(reads(text, 2, "let x = 1;"));
	assert!(reads(text, 2, "let x = 1;\n    x + 1"));
	assert!(!reads(text, 1, "let x = 1;"));
	assert!(!reads(text, 0, "fn a() {"));
	assert!(!reads(text, 2, "   "));
}

#[test]
fn the_refusal_names_the_five_lines_around() {
	let text = (1..=9).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
	assert_eq!(around(&text, 5), "Lines 3–7 are:\n3: line 3\n4: line 4\n5: line 5\n6: line 6\n7: line 7");
	assert_eq!(around(&text, 1), "Lines 1–3 are:\n1: line 1\n2: line 2\n3: line 3");
}

#[test]
fn notes_count_with_the_blocking_ones() {
	let card = |severity| Card {
		id: "c".to_owned(),
		kind: Kind::Finding,
		severity,
		file: "a.rs".to_owned(),
		line: 1,
		quote: "q".to_owned(),
		text: "t".to_owned(),
		verdict: Verdict::Post,
		proof: Proof { sha: "abc".to_owned(), ok: true },
		edited: None,
		published: None,
		ran: None,
	};
	let notes = ReviewNotes {
		head: "abc".to_owned(),
		notes: vec![card(Severity::P0), card(Severity::P1), card(Severity::P2)],
		..ReviewNotes::default()
	};
	assert_eq!(notes.counts(), NoteCounts { notes: 3, blocking: 2 });
	assert_eq!(ReviewNotes::default().counts(), NoteCounts::default());
}

#[test]
fn a_notes_file_round_trips_and_a_missing_one_is_empty() {
	let dir = std::env::temp_dir().join(format!("jocasta-notes-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let file = dir.join("review.json");
	assert_eq!(ReviewNotes::read(&file), ReviewNotes::default());
	let notes = ReviewNotes { head: "abc".to_owned(), ..ReviewNotes::default() };
	notes.write(&file).unwrap();
	assert_eq!(ReviewNotes::read(&file), notes);
	std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_check_ends_the_review_with_one_p0_card_for_him_only_and_a_new_head_starts_afresh_0706() {
	let mut notes = ReviewNotes { head: "abc".to_owned(), ..ReviewNotes::default() };
	assert_eq!(notes.ended(), None);
	let row = WorksRow {
		label: "`mise run test` in the clean room".to_owned(),
		state: WorksState::Failed,
		ms: Some(5),
		tail: "1 failed".to_owned(),
		screenshots: Vec::new(),
	};
	let proof = RanProof {
		run: "mise run test".to_owned(),
		expect: "exit:0".to_owned(),
		exit: Some(1),
		tail: "1 failed".to_owned(),
	};
	notes.failed(row, proof.clone());
	assert_eq!(notes.ended(), Some("`mise run test` in the clean room"));
	let card = &notes.notes[0];
	assert_eq!(
		(card.severity, card.verdict, card.text.as_str()),
		(Severity::P0, Verdict::Skip, "It doesn't work: `mise run test` in the clean room failed")
	);
	assert_eq!(card.ran.as_ref(), Some(&proof));
	assert_eq!(notes.counts(), NoteCounts { notes: 1, blocking: 1 });
	let dir = std::env::temp_dir().join(format!("jocasta-notes-ended-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let file = dir.join("review.json");
	notes.write(&file).unwrap();
	assert_eq!(ReviewNotes::of_head(&file, "abc"), notes);
	assert_eq!(ReviewNotes::of_head(&file, "def"), ReviewNotes { head: "def".to_owned(), ..ReviewNotes::default() });
	std::fs::remove_dir_all(&dir).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
	let mut command = std::process::Command::new("git");
	command.args(["-c", "user.name=Test", "-c", "user.email=test@example.com"]).args(args).current_dir(dir);
	crate::shared::env::apply_to(&mut command);
	let output = command.output().expect("running git");
	assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

#[tokio::test]
async fn a_finding_needs_a_proof_and_questions_and_opinions_carry_none_and_wait_for_him_0707() {
	let root = temp_dir("jocasta-notes-kinds");
	let dir = root.path().to_path_buf();
	git(&dir, &["init", "-q", "-b", "main"]);
	std::fs::write(dir.join("a.rs"), "fn a() {\n\tlet x = 1;\n}\n").unwrap();
	git(&dir, &["add", "."]);
	git(&dir, &["commit", "-q", "-m", "a"]);
	let head = head_of(&dir).await.unwrap();
	let note = |extra: Value| {
		let mut values = json!({ "file": "a.rs", "line": 2, "quote": "let x = 1;", "text": "t", "severity": "P1" });
		values.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
		values.as_object().unwrap().clone()
	};
	let checked = |values: Map<String, Value>| {
		let (dir, head) = (dir.clone(), head.clone());
		async move { checked_card(&dir, &head, &Args::new(&values, REVIEW_NOTE), 1).await }
	};
	let refused = checked(note(json!({ "kind": "finding" }))).await.unwrap_err().to_string();
	assert_eq!(
		refused,
		"review_note: a finding needs a proof (a command and the output it must give); hand it in as a question or an opinion otherwise"
	);
	let proof = json!({ "run": "cargo test", "expect": "exit:0" });
	let refused = checked(note(json!({ "kind": "opinion", "proof": proof.clone() }))).await.unwrap_err().to_string();
	assert!(refused.contains("only a finding carries a proof"), "{refused}");
	let refused = checked(note(json!({ "kind": "nit" }))).await.unwrap_err().to_string();
	assert!(refused.contains("kind is finding, question or opinion"), "{refused}");
	let (card, step) = checked(note(json!({ "kind": "finding", "proof": proof }))).await.unwrap();
	assert_eq!((card.kind, card.severity, card.verdict), (Kind::Finding, Severity::P1, Verdict::Post));
	assert_eq!(step, Some(Step { run: "cargo test".to_owned(), expect: "exit:0".to_owned() }));
	let (card, step) = checked(note(json!({ "kind": "question", "verdict": "post" }))).await.unwrap();
	assert_eq!((card.kind, card.severity, card.verdict), (Kind::Question, Severity::P2, Verdict::Skip));
	assert_eq!(step, None);
	let refused = checked(note(json!({ "kind": "opinion", "quote": "let y" }))).await.unwrap_err().to_string();
	assert!(refused.contains("doesn't read that"), "{refused}");
}
