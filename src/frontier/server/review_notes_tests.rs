use super::*;

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
		severity,
		file: "a.rs".to_owned(),
		line: 1,
		quote: "q".to_owned(),
		text: "t".to_owned(),
		verdict: Verdict::Post,
		proof: Proof { sha: "abc".to_owned(), ok: true },
		edited: None,
		published: None,
	};
	let notes =
		ReviewNotes { head: "abc".to_owned(), notes: vec![card(Severity::P0), card(Severity::P1), card(Severity::P2)] };
	assert_eq!(notes.counts(), NoteCounts { notes: 3, blocking: 2 });
	assert_eq!(ReviewNotes::default().counts(), NoteCounts::default());
}

#[test]
fn a_notes_file_round_trips_and_a_missing_one_is_empty() {
	let dir = std::env::temp_dir().join(format!("jocasta-notes-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let file = dir.join("review.json");
	assert_eq!(ReviewNotes::read(&file), ReviewNotes::default());
	let notes = ReviewNotes { head: "abc".to_owned(), notes: Vec::new() };
	notes.write(&file).unwrap();
	assert_eq!(ReviewNotes::read(&file), notes);
	std::fs::remove_dir_all(&dir).unwrap();
}
