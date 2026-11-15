//! A line diff of two small texts (setup's config files): `  ` kept, `- ` removed, `+ ` added, in order. Plain LCS;
//! the files are a few hundred lines at most.

/// `text` without one final line break, split at `\n` or `\r\n`.
fn lines_of(text: &str) -> Vec<&str> {
	let text = text.strip_suffix('\n').map_or(text, |rest| rest.strip_suffix('\r').unwrap_or(rest));
	let mut lines: Vec<&str> = text.split('\n').collect();
	let last = lines.len() - 1;
	for line in lines.iter_mut().take(last) {
		*line = line.strip_suffix('\r').unwrap_or(line);
	}
	lines
}

pub fn line_diff(before: &str, after: &str) -> Vec<String> {
	let a = lines_of(before);
	let b = lines_of(after);
	// lcs[i][j] = longest common subsequence of a[i..] and b[j..].
	let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
	for i in (0..a.len()).rev() {
		for j in (0..b.len()).rev() {
			lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
		}
	}
	let mut out = Vec::new();
	let (mut i, mut j) = (0, 0);
	while i < a.len() || j < b.len() {
		if i < a.len() && j < b.len() && a[i] == b[j] {
			out.push(format!("  {}", a[i]));
			i += 1;
			j += 1;
		} else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
			out.push(format!("+ {}", b[j]));
			j += 1;
		} else {
			out.push(format!("- {}", a[i]));
			i += 1;
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn line_diff_keeps_removes_and_adds_lines_in_order_a_crlf_files_last_line_is_no_change() {
		assert_eq!(line_diff("a\nb\nc\n", "a\nx\nc\nd\n"), ["  a", "+ x", "- b", "  c", "+ d"]);
		assert_eq!(line_diff("a: 1\r\nb: 2\r\n", "a: 1\nb: 2\n"), ["  a: 1", "  b: 2"]);
	}

	#[test]
	fn an_empty_side_is_one_empty_line_as_the_typescript_splits_it() {
		assert_eq!(line_diff("", "a\n"), ["+ a", "- "]);
		assert_eq!(line_diff("a", "a"), ["  a"]);
	}
}
