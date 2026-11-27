//! Links from outside, and opening one in the browser (or a file in its viewer).

use std::path::Path;
use std::process::{Command, Stdio};

use url::Url;

use crate::shared::env;
use crate::shared::errors::Result;

/// Longest link kept: a longer one is no link a person clicks (SE-6).
const MAX_LINK: usize = 300;

/// A link from outside (a calendar entry, a CI check) as one to open: an http(s) URL of at most 300 characters,
/// normalized; anything else is `None` (SE-4, SE-6).
pub fn web_link(url: Option<&str>) -> Option<String> {
	let parsed = Url::parse(url.filter(|u| !u.is_empty())?).ok()?;
	if !matches!(parsed.scheme(), "http" | "https") {
		return None;
	}
	let href = parsed.as_str();
	// An href is ASCII (percent-encoded, punycode), so its bytes are the TypeScript's UTF-16 units.
	(href.len() <= MAX_LINK).then(|| href.to_owned())
}

/// Opens `url` in his default browser, without waiting (Windows' URL handler; `open`; `xdg-open`). The same handlers
/// open a file path in its default viewer.
pub fn open_in_browser(url: &str) -> Result<()> {
	let mut command = if cfg!(windows) {
		let mut command = Command::new("rundll32.exe");
		command.args(["url.dll,FileProtocolHandler", url]);
		command
	} else {
		let mut command = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
		command.arg(url);
		command
	};
	command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
	#[cfg(windows)]
	std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
	env::apply_to(&mut command);
	let mut child = command.spawn()?;
	// Reaped in the background, so the child never lingers as a zombie.
	std::thread::spawn(move || child.wait());
	Ok(())
}

/// Opens the file at `path` in its default viewer (a screenshot in his image viewer, 0721), without waiting.
pub fn open_file(path: &Path) -> Result<()> {
	open_in_browser(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn only_http_and_https_links_of_at_most_300_characters_are_kept_normalized() {
		assert_eq!(web_link(Some("https://Example.com/a b")).as_deref(), Some("https://example.com/a%20b"));
		assert_eq!(web_link(Some("http://example.com")).as_deref(), Some("http://example.com/"));
		for bad in [None, Some(""), Some("javascript:alert(1)"), Some("file:///etc/passwd"), Some("not a url")] {
			assert_eq!(web_link(bad), None);
		}
		let long = format!("https://example.com/{}", "a".repeat(300));
		assert_eq!(web_link(Some(&long)), None);
		let fits = format!("https://example.com/{}", "a".repeat(300 - "https://example.com/".len()));
		assert_eq!(web_link(Some(&fits)).as_deref(), Some(fits.as_str()));
	}
}
