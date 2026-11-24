//! The few mail headers the Gmail poller reads (0680): `From` and `Subject` out of a header block, unfolded, with RFC 2047
//! encoded words (`=?UTF-8?B?…?=`, `=?UTF-8?Q?…?=`) decoded. Hand-written for two headers; no mail library.

/// The value of header `name` (any case) in `block`, unfolded and decoded; `None` when it isn't there.
pub fn header_value(block: &str, name: &str) -> Option<String> {
	let mut found: Option<String> = None;
	for line in block.lines() {
		if line.starts_with([' ', '\t']) {
			if let Some(value) = found.as_mut() {
				value.push(' ');
				value.push_str(line.trim());
			}
			continue;
		}
		if found.is_some() {
			break;
		}
		if let Some((key, value)) = line.split_once(':')
			&& key.trim().eq_ignore_ascii_case(name)
		{
			found = Some(value.trim().to_owned());
		}
	}
	found.map(|value| decode_words(&value))
}

/// The name part of a `From` value: the display name, else the address's local part.
pub fn sender_name(from: &str) -> String {
	let from = from.trim();
	let (name, address) = match from.rfind('<') {
		Some(at) => (from[..at].trim(), from[at + 1..].trim_end_matches('>').trim()),
		None => ("", from),
	};
	let name = name.trim_matches('"').trim();
	if !name.is_empty() {
		return name.to_owned();
	}
	address.split('@').next().unwrap_or(address).to_owned()
}

/// `text` with its RFC 2047 encoded words decoded; white space between two encoded words is dropped.
pub fn decode_words(text: &str) -> String {
	let mut out = String::new();
	let mut rest = text;
	let mut after_word = false;
	while let Some(start) = rest.find("=?") {
		let (before, tail) = rest.split_at(start);
		match encoded_word(tail) {
			Some((decoded, used)) => {
				if !(after_word && before.trim().is_empty()) {
					out.push_str(before);
				}
				out.push_str(&decoded);
				rest = &tail[used..];
				after_word = true;
			}
			None => {
				out.push_str(before);
				out.push_str("=?");
				rest = &tail[2..];
				after_word = false;
			}
		}
	}
	out.push_str(rest);
	out
}

/// One `=?charset?B|Q?text?=` at the start of `text`: its decoded text and the bytes it took.
fn encoded_word(text: &str) -> Option<(String, usize)> {
	let body = text.strip_prefix("=?")?;
	let (charset, body) = body.split_once('?')?;
	let (encoding, body) = body.split_once('?')?;
	let (payload, _) = body.split_once("?=")?;
	if payload.contains(char::is_whitespace) {
		return None;
	}
	let bytes = match encoding {
		"B" | "b" => base64(payload)?,
		"Q" | "q" => quoted(payload)?,
		_ => return None,
	};
	let used = 2 + charset.len() + 1 + encoding.len() + 1 + payload.len() + 2;
	let charset = charset.split('*').next().unwrap_or(charset).to_ascii_lowercase();
	let decoded = match charset.as_str() {
		"iso-8859-1" | "latin1" | "windows-1252" => bytes.iter().map(|byte| char::from(*byte)).collect(),
		_ => String::from_utf8_lossy(&bytes).into_owned(),
	};
	Some((decoded, used))
}

fn quoted(payload: &str) -> Option<Vec<u8>> {
	let bytes = payload.as_bytes();
	let mut out = Vec::new();
	let mut at = 0;
	while at < bytes.len() {
		match bytes[at] {
			b'_' => out.push(b' '),
			b'=' => {
				let pair = payload.get(at + 1..at + 3)?;
				out.push(u8::from_str_radix(pair, 16).ok()?);
				at += 2;
			}
			byte => out.push(byte),
		}
		at += 1;
	}
	Some(out)
}

fn base64(payload: &str) -> Option<Vec<u8>> {
	let mut out = Vec::new();
	let (mut bits, mut have) = (0u32, 0u32);
	for byte in payload.bytes().take_while(|byte| *byte != b'=') {
		let value = match byte {
			b'A'..=b'Z' => byte - b'A',
			b'a'..=b'z' => byte - b'a' + 26,
			b'0'..=b'9' => byte - b'0' + 52,
			b'+' => 62,
			b'/' => 63,
			_ => return None,
		};
		bits = (bits << 6) | u32::from(value);
		have += 6;
		if have >= 8 {
			have -= 8;
			out.push(((bits >> have) & 0xff) as u8);
		}
	}
	Some(out)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn headers_are_unfolded_and_their_encoded_words_decoded() {
		let block = "From: =?UTF-8?B?SmFuZSBEb2U=?= <jane@example.org>\r\nSubject: =?UTF-8?Q?Caf=C3=A9_menu?=\r\n =?UTF-8?Q?_for_Friday?=\r\nDate: Mon, 24 Nov 2026 09:00:00 +0000\r\n\r\n";
		assert_eq!(header_value(block, "subject").as_deref(), Some("Café menu for Friday"));
		assert_eq!(sender_name(&header_value(block, "from").unwrap()), "Jane Doe");
		assert_eq!(header_value(block, "x-missing"), None);
	}

	#[test]
	fn a_sender_is_its_display_name_or_its_local_part() {
		assert_eq!(sender_name("\"Doe, Jane\" <jane@example.org>"), "Doe, Jane");
		assert_eq!(sender_name("<billing@example.org>"), "billing");
		assert_eq!(sender_name("someone@example.org"), "someone");
	}

	#[test]
	fn text_that_only_looks_encoded_stays_as_it_is() {
		assert_eq!(decode_words("a =? b =?x?Z?q?= c"), "a =? b =?x?Z?q?= c");
		assert_eq!(decode_words("=?ISO-8859-1?Q?caf=E9?="), "café");
	}
}
