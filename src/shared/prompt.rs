//! An appended prompt's text without its leading HTML comment (0577).

/// An appended prompt's text without its leading HTML comment (0577): Ponytail's license and file notes stay in the
/// shipped file and out of every request. Only a comment that opens the text (after blank lines) is cut, with the
/// blank lines after it; any other text is returned as it is.
pub fn without_leading_comment(text: &str) -> String {
	let Some(open) = text.trim_start().strip_prefix("<!--") else { return text.to_owned() };
	let Some(end) = open.find("-->") else { return text.to_owned() };
	let rest = &open[end + 3..];
	// `replace(/^\s*\n/, "")`: the leading whitespace up to and including its last newline.
	let blank = rest.len() - rest.trim_start().len();
	match rest[..blank].rfind('\n') {
		Some(newline) => rest[newline + 1..].to_owned(),
		None => rest.to_owned(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_leading_comment_goes_with_the_blank_lines_after_it() {
		assert_eq!(without_leading_comment("\n<!-- licence\nnotes -->\n\n\n# Title\n"), "# Title\n");
		assert_eq!(without_leading_comment("<!-- x -->  \nbody"), "body");
		assert_eq!(without_leading_comment("<!-- x -->body"), "body");
	}

	#[test]
	fn any_other_text_stays_as_it_is() {
		for text in ["# Title\n<!-- later -->\n", "<!-- never closed\n", "", "  text"] {
			assert_eq!(without_leading_comment(text), text);
		}
	}
}
