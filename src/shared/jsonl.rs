//! JSON lines: omp's session files, the benchmark's rows and logs. omp ≥ 18.10.4 rewrites a session whole through a
//! temp file and a rename (discards, retry marks, shake), so an incremental reader also checks the file's identity; the
//! readers here only keep the byte offset after the last complete line and parse what follows it (CR-03). A line still
//! being written, or torn by a kill, is skipped, never thrown on.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::shared::errors::Result;

/// Bytes read per call: bounds memory on a first read of a large session.
const CHUNK: u64 = 4 << 20;

/// Hands each complete line of `file` between byte `from` and `to` to `on_line`, in order, and returns the offset just
/// after the last complete line (a line still being written is read again next time).
pub fn read_lines(file: &Path, from: u64, to: u64, mut on_line: impl FnMut(&str)) -> Result<u64> {
	if to <= from {
		return Ok(from);
	}
	let mut handle = fs::File::open(file)?;
	handle.seek(SeekFrom::Start(from))?;
	let mut offset = from;
	let mut carry: Vec<u8> = Vec::new();
	let mut position = from;
	while position < to {
		let mut chunk = vec![0_u8; usize::try_from(CHUNK.min(to - position)).unwrap_or(usize::MAX)];
		let n = handle.read(&mut chunk)?;
		if n == 0 {
			break;
		}
		position += n as u64;
		let mut data = std::mem::take(&mut carry);
		data.extend_from_slice(&chunk[..n]);
		let mut start = 0;
		while let Some(newline) = data[start..].iter().position(|&b| b == b'\n') {
			on_line(&String::from_utf8_lossy(&data[start..start + newline]));
			start += newline + 1;
		}
		offset += start as u64;
		carry = data.split_off(start);
	}
	Ok(offset)
}

/// A JSONL line as an object; `None` for anything else (a blank line, plain text, or one still being written).
pub fn parse_line(line: &str) -> Option<Map<String, Value>> {
	let trimmed = line.trim();
	if !trimmed.starts_with('{') {
		return None;
	}
	match serde_json::from_str(trimmed).ok()? {
		Value::Object(object) => Some(object),
		_ => None,
	}
}

/// The JSON object lines of a JSONL text as `T`, in order; blank, plain-text and torn lines are skipped.
pub fn json_lines<T: DeserializeOwned>(text: &str) -> Vec<T> {
	text.split('\n').filter_map(parse_line).filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
}

/// [`json_lines`] of a file; empty when the file doesn't exist.
pub fn read_jsonl_file<T: DeserializeOwned>(file: &Path) -> Result<Vec<T>> {
	if !file.exists() {
		return Ok(Vec::new());
	}
	Ok(json_lines(&String::from_utf8_lossy(&fs::read(file)?)))
}

/// Appends `row` as one JSON line to `file`, making its folder when missing.
pub fn append_jsonl(file: &Path, row: &impl Serialize) -> Result<()> {
	if let Some(dir) = file.parent() {
		fs::create_dir_all(dir)?;
	}
	let mut line = serde_json::to_string(row)?;
	line.push('\n');
	fs::OpenOptions::new().create(true).append(true).open(file)?.write_all(line.as_bytes())?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_dir;

	#[test]
	fn read_lines_stops_after_the_last_complete_line_and_resumes_there() {
		let dir = temp_dir("jsonl");
		let file = dir.path().join("s.jsonl");
		fs::write(&file, "{\"a\":1}\n{\"a\":2}\n{\"a\":").unwrap();
		let size = fs::metadata(&file).unwrap().len();
		let mut lines = Vec::new();
		let offset = read_lines(&file, 0, size, |l| lines.push(l.to_owned())).unwrap();
		assert_eq!(lines, ["{\"a\":1}", "{\"a\":2}"]);
		assert_eq!(offset, 16);
		fs::write(&file, "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n").unwrap();
		let size = fs::metadata(&file).unwrap().len();
		let mut rest = Vec::new();
		assert_eq!(read_lines(&file, offset, size, |l| rest.push(l.to_owned())).unwrap(), size);
		assert_eq!(rest, ["{\"a\":3}"]);
		assert_eq!(read_lines(&file, size, size, |_| {}).unwrap(), size);
	}

	#[test]
	fn only_complete_object_lines_are_rows() {
		let rows: Vec<Map<String, Value>> = json_lines("{\"a\":1}\n\nplain\n[1]\n  {\"b\":2}  \n{\"c\":");
		assert_eq!(rows.len(), 2);
		assert_eq!(rows[1]["b"], 2);
		assert!(parse_line("{}").is_some() && parse_line("1").is_none());
	}

	#[test]
	fn a_file_of_rows_is_appended_and_read_back_and_a_missing_one_is_empty() {
		let dir = temp_dir("jsonl");
		let file = dir.path().join("deep").join("rows.jsonl");
		assert!(read_jsonl_file::<Value>(&file).unwrap().is_empty());
		append_jsonl(&file, &serde_json::json!({"n": 1})).unwrap();
		append_jsonl(&file, &serde_json::json!({"n": 2})).unwrap();
		assert_eq!(fs::read_to_string(&file).unwrap(), "{\"n\":1}\n{\"n\":2}\n");
		assert_eq!(read_jsonl_file::<Map<String, Value>>(&file).unwrap().len(), 2);
	}
}
