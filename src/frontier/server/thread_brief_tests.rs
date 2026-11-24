use super::*;

const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs
index 1..2 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -3,3 +3,4 @@ fn a() {
 keep
-old
+new
+more
";

#[test]
fn a_diff_is_read_into_files_hunks_and_numbered_lines() {
	let files = parse_diff(DIFF, DIFF_LINES);
	assert_eq!(files.len(), 1);
	assert_eq!(files[0].path, "src/a.rs");
	let lines = &files[0].hunks[0].lines;
	assert_eq!(lines.len(), 4);
	assert_eq!((lines[0].old, lines[0].new), (Some(3), Some(3)));
	assert_eq!((lines[1].kind.as_str(), lines[1].old, lines[1].new), ("-", Some(4), None));
	assert_eq!((lines[2].kind.as_str(), lines[2].old, lines[2].new), ("+", None, Some(4)));
	assert_eq!(lines[3].new, Some(5));
}

#[test]
fn a_long_diff_is_cut_and_the_rest_counted() {
	let files = parse_diff(DIFF, 7);
	let last = files[0].hunks.last().unwrap().lines.last().unwrap();
	assert_eq!(last.text, "… 2 more lines");
}

#[test]
fn a_hunk_header_gives_both_starts() {
	assert_eq!(hunk_starts(" -12,5 +14,7 @@ fn x"), (12, 14));
	assert_eq!(hunk_starts(" -1 +1 @@"), (1, 1));
}
