//! A JSONL input the bench reads strictly (a task list, a guard corpus, verdicts): unlike the product's
//! `shared::jsonl`, which skips what doesn't parse, a malformed line here is a usage error naming it.

use serde_json::{Map, Value};

use crate::lib::args::BenchUsageError;
use jocasta::shared::errors::Result;

/// One non-blank line, parsed, with `where_` (`<file>:<line>`) for the caller's own field checks.
#[derive(Debug, Clone)]
pub struct StrictLine {
	pub row: Map<String, Value>,
	pub where_: String,
}

/// Yields each non-blank line parsed, in order; lazily, so the first bad line, whether not JSON or failing the
/// caller's own check, is the one reported. A line that isn't JSON is `<where>: not JSON`, followed by the parser's
/// reason in parentheses when `reason` is set. A line that is JSON but not an object is yielded as an empty row, as the
/// TypeScript yielded the value for the caller's checks to refuse.
pub fn strict_json_lines<'a>(
	text: &'a str,
	file: &'a str,
	reason: bool,
) -> impl Iterator<Item = Result<StrictLine>> + 'a {
	text.split('\n').enumerate().filter_map(move |(i, raw)| {
		let line = raw.strip_suffix('\r').unwrap_or(raw);
		if line.trim().is_empty() {
			return None;
		}
		let where_ = format!("{file}:{}", i + 1);
		Some(match serde_json::from_str::<Value>(line) {
			Ok(Value::Object(row)) => Ok(StrictLine { row, where_ }),
			Ok(_) => Ok(StrictLine { row: Map::new(), where_ }),
			Err(error) => Err(BenchUsageError::err(format!(
				"{where_}: not JSON{}",
				if reason { format!(" ({error})") } else { String::new() }
			))),
		})
	})
}
