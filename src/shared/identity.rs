//! A project's identity (0103, 0107, 0110): the normalized origin, the first root commit or the path.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::shared::errors::{Error, Result};
use crate::shared::git::{GitOptions, git_async};
use crate::shared::pathkey::path_key;

const AZURE: &str = "dev.azure.com";

/// `^[a-z][a-z0-9+.-]*:\/\//i`: the address starts with a URL scheme.
static SCHEME: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*://").expect("the scheme pattern"));

/// `^(?:[^@/\\]+@)?([^:/\\]{2,}):(.*)$`: scp-style `user@host:path`; `.` is JavaScript's, which stops at any line
/// terminator.
static SCP: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^(?:[^@/\\]+@)?([^:/\\]{2,}):([^\n\r\u{2028}\u{2029}]*)$").expect("the scp pattern"));

/// Maps an Azure DevOps host and path to `[org, project, repo]`, or `None` for any other host (0103).
fn azure_parts(host: &str, segments: &[String]) -> Option<[String; 3]> {
	let at = |i: usize| segments.get(i).map(String::as_str);
	if host == AZURE {
		// dev.azure.com/{org}/{project}/_git/{repo}
		return match (at(0), at(1), at(2), at(3)) {
			(Some(org), Some(project), Some("_git"), Some(repo)) => {
				Some([org.to_owned(), project.to_owned(), repo.to_owned()])
			}
			_ => None,
		};
	}
	if host == "ssh.dev.azure.com" || host == "vs-ssh.visualstudio.com" {
		// v3/{org}/{project}/{repo}
		return match (at(0), at(1), at(2), at(3)) {
			(Some("v3"), Some(org), Some(project), Some(repo)) => {
				Some([org.to_owned(), project.to_owned(), repo.to_owned()])
			}
			_ => None,
		};
	}
	let org = host.strip_suffix(".visualstudio.com")?;
	// {org}.visualstudio.com/[DefaultCollection/]{project}/_git/{repo}
	let rest = if at(0) == Some("defaultcollection") { &segments[1..] } else { segments };
	match (rest.first(), rest.get(1).map(String::as_str), rest.get(2)) {
		(Some(project), Some("_git"), Some(repo)) => Some([org.to_owned(), project.clone(), repo.clone()]),
		_ => None,
	}
}

/// `decodeURIComponent`: `None` where it throws (a stray `%`, bytes that aren't UTF-8).
fn decode_uri_component(text: &str) -> Option<String> {
	let bytes = text.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut i = 0;
	while i < bytes.len() {
		if bytes[i] == b'%' {
			let hex = text.get(i + 1..i + 3)?;
			out.push(u8::from_str_radix(hex, 16).ok().filter(|_| hex.bytes().all(|b| b.is_ascii_hexdigit()))?);
			i += 3;
		} else {
			out.push(bytes[i]);
			i += 1;
		}
	}
	String::from_utf8(out).ok()
}

/// The URL path percent-decoded; a stray `%` (`50%-off`) leaves it as written rather than failing.
pub fn decoded(path: &str) -> String {
	decode_uri_component(path).unwrap_or_else(|| path.to_owned())
}

/// A local path or file URL origin: the path itself is the key.
fn local_key(raw: &str) -> String {
	let slashed = raw.replace('\\', "/");
	let trimmed = slashed.trim_end_matches('/');
	let head = trimmed.len().checked_sub(4).and_then(|at| trimmed.get(..at).zip(trimmed.get(at..)));
	let stripped = match head {
		Some((head, tail)) if tail.eq_ignore_ascii_case(".git") => head,
		_ => trimmed,
	};
	stripped.to_lowercase()
}

/// The normalized origin key (0103): lowercased host plus path, `.git` stripped, user and port dropped;
/// every Azure DevOps form maps to `dev.azure.com/<org>/<project>/<repo>`. An address that starts like a URL but isn't
/// one fails, as the URL parser does.
pub fn normalize_origin(url: &str) -> Result<String> {
	let raw = js_trim(url);
	let scheme = SCHEME.is_match(raw);
	let scp = SCP.captures(raw);
	let (host, path) = if scheme && !raw.to_lowercase().starts_with("file:") {
		let parsed = url::Url::parse(raw).map_err(|error| Error::other(format!("Invalid URL: {raw}: {error}")))?;
		(parsed.host_str().unwrap_or("").to_owned(), parsed.path().to_owned())
	} else if let (false, Some(found)) = (scheme, scp) {
		(found[1].to_owned(), found[2].to_owned())
	} else {
		return Ok(local_key(raw));
	};
	let path = decoded(&path);
	let lowered = path.to_lowercase();
	let path = strip_dot_git(&lowered);
	let segments: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(str::to_owned).collect();
	let host = host.to_lowercase();
	Ok(match azure_parts(&host, &segments) {
		Some(parts) => [AZURE, &parts[0], &parts[1], &parts[2]].join("/"),
		None => std::iter::once(host).chain(segments).collect::<Vec<_>>().join("/"),
	})
}

/// `.replace(/\.git\/?$/i, "")`; the path is already lowercased.
fn strip_dot_git(path: &str) -> &str {
	let trimmed = path.strip_suffix('/').unwrap_or(path);
	trimmed.strip_suffix(".git").or_else(|| path.strip_suffix(".git")).unwrap_or(path)
}

/// JavaScript's `String.prototype.trim`: the white space and line terminators ECMAScript names, and the byte order mark.
fn js_trim(text: &str) -> &str {
	text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// How long one git call of the identity may take (CO-10).
const GIT_TIMEOUT_MS: i64 = 30_000;

/// Trimmed stdout of a git call that exited 0, else `None`; a call killed at its timeout fails.
async fn git_out(dir: &Path, args: &[&str]) -> Result<Option<String>> {
	let result = git_async(dir, args, GIT_TIMEOUT_MS, GitOptions::default()).await?;
	let Some(code) = result.code else {
		return Err(Error::other(format!(
			"git {} was killed after {} s in {}",
			args.join(" "),
			GIT_TIMEOUT_MS / 1000,
			dir.display()
		)));
	};
	Ok((code == 0).then(|| result.stdout.trim().to_owned()))
}

/// A project's identity key (0103): the origin key, else `root:<sha>`, else `path:<path>`. Git runs without holding the
/// event loop (SHR-3).
pub async fn identity_key(dir: &Path) -> Result<String> {
	if let Some(origin) = git_out(dir, &["config", "--get", "remote.origin.url"]).await?.filter(|o| !o.is_empty()) {
		return normalize_origin(&origin);
	}
	let roots = git_out(dir, &["rev-list", "--max-parents=0", "HEAD"]).await?;
	// rev-list lists newest first; the first root commit is the last line.
	if let Some(first) = roots.as_deref().and_then(|roots| roots.lines().rfind(|line| !line.is_empty())) {
		return Ok(format!("root:{first}"));
	}
	Ok(format!("path:{}", path_key(dir)))
}

/// `p-` plus the first 16 hex of SHA-256 of the identity key (0107).
pub fn project_id_for(identity: &str) -> String {
	let digest = Sha256::digest(identity.as_bytes());
	let hex: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
	format!("p-{hex}")
}

/// The last segment of the origin key, else the main path's folder name (0110).
pub fn default_project_name(identity: &str, main_path: &Path) -> String {
	let folder = || main_path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
	if identity.starts_with("root:") || identity.starts_with("path:") {
		return folder();
	}
	identity.rsplit('/').next().filter(|last| !last.is_empty()).map_or_else(folder, str::to_owned)
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
