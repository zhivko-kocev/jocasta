//! Outside text as one plain line (0680): control characters become spaces, runs of white space one space, and the line
//! is cut to `max` characters with an ellipsis.

/// `text` as one line of at most `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
	let mut line = String::new();
	for word in text.split(|c: char| c.is_whitespace() || c.is_control()).filter(|word| !word.is_empty()) {
		if !line.is_empty() {
			line.push(' ');
		}
		line.push_str(word);
	}
	if line.chars().count() <= max {
		return line;
	}
	let kept: String = line.chars().take(max.saturating_sub(1)).collect();
	format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_line_loses_control_characters_and_runs_of_space_and_is_cut_with_an_ellipsis() {
		assert_eq!(one_line("a\u{1b}[31m  b\r\nc\td", 50), "a [31m b c d");
		assert_eq!(one_line("abcdefghij", 5), "abcd…");
		assert_eq!(one_line("  ", 5), "");
	}
}
