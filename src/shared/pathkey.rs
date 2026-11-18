//! Folders as identity (0103, 0521): the one key form folders are compared and stored by, and the one resolver of a
//! folder's real long name.

use std::path::{Component, Path, PathBuf};

/// `path.resolve`: an absolute path against the working folder, with `.` and `..` folded away (no file system access).
pub fn resolve(path: &Path) -> PathBuf {
	let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
	let mut out = PathBuf::new();
	for part in absolute.components() {
		match part {
			Component::Prefix(_) | Component::RootDir => out.push(part.as_os_str()),
			Component::CurDir => {}
			Component::ParentDir => {
				if matches!(out.components().next_back(), Some(Component::Normal(_))) {
					out.pop();
				}
			}
			Component::Normal(name) => out.push(name),
		}
	}
	out
}

/// `std::fs::canonicalize` as Node's `realpathSync.native` spells it: Windows' `\\?\` prefix dropped.
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
	let real = std::fs::canonicalize(path)?;
	let text = real.to_string_lossy();
	if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
		return Ok(PathBuf::from(format!(r"\\{rest}")));
	}
	if let Some(rest) = text.strip_prefix(r"\\?\") {
		let bytes = rest.as_bytes();
		if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
			return Ok(PathBuf::from(rest));
		}
	}
	Ok(real)
}

/// The real, absolute form of a path: links followed and 8.3 short names (`C:\Users\ZHIVKO~1`, what a short `%TEMP%`
/// gives) expanded to long ones. A missing path is expanded through its nearest existing ancestor, the rest kept.
pub fn real_path(path: &Path) -> PathBuf {
	let mut rest: Vec<std::ffi::OsString> = Vec::new();
	let mut current = resolve(path);
	loop {
		if let Ok(real) = canonical(&current) {
			return rest.iter().rev().fold(real, |joined, part| joined.join(part));
		}
		let Some(parent) = current.parent().map(Path::to_path_buf) else { return resolve(path) };
		let Some(name) = current.file_name().map(std::ffi::OsStr::to_os_string) else { return resolve(path) };
		rest.push(name);
		current = parent;
	}
}

/// A path as identity: resolved to an absolute path, forward slashes, no trailing slash, and case-folded on Windows
/// (whose file systems ignore case). Links and 8.3 short names are kept as written: key `real_path(path)` when two
/// spellings of one folder must meet.
pub fn path_key(path: &Path) -> String {
	let forward = resolve(path).to_string_lossy().replace('\\', "/");
	let forward = forward.trim_end_matches('/');
	if cfg!(windows) { forward.to_lowercase() } else { forward.to_owned() }
}

/// Whether `a` and `b` name the same path, whatever their slashes, case (Windows) or trailing separator.
pub fn same_path(a: &Path, b: &Path) -> bool {
	path_key(a) == path_key(b)
}

/// Whether `a` and `b` are the same folder, links and short names followed (`real_path`).
pub fn same_dir(a: &Path, b: &Path) -> bool {
	path_key(&real_path(a)) == path_key(&real_path(b))
}

/// Whether `path` is `root` or inside it, by key: `<root>-old/x` is not inside `<root>`.
pub fn is_inside(root: &Path, path: &Path) -> bool {
	let top = path_key(root);
	let at = path_key(path);
	if at == top {
		return true;
	}
	if top.ends_with('/') { at.starts_with(&top) } else { at.starts_with(&format!("{top}/")) }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_dir;

	fn resolved(parts: &[&str]) -> PathBuf {
		parts.iter().fold(std::env::current_dir().unwrap(), |joined, part| joined.join(part))
	}

	#[test]
	fn path_key_is_resolved_forward_slashes_no_trailing_slash_case_folded_only_on_windows() {
		let at = resolved(&["Acme", "App"]);
		let sep = if cfg!(windows) { "\\" } else { "/" };
		let key = path_key(Path::new(&format!("{}{sep}", at.display())));
		let shown = at.to_string_lossy().to_string();
		let expected = if cfg!(windows) { shown.to_lowercase() } else { shown };
		assert_eq!(key, expected.replace('\\', "/"));
		assert_eq!(path_key(Path::new("Acme/App")), key);
		assert_eq!(same_path(&at, Path::new(&at.to_string_lossy().to_uppercase())), cfg!(windows));
		assert!(same_path(&at.join("x").join(".."), &at));
	}

	#[test]
	fn is_inside_the_folder_itself_and_what_is_under_it_never_a_sibling_sharing_its_prefix() {
		let root = resolved(&["acme.wt"]);
		assert!(is_inside(&root, &root));
		assert!(is_inside(&root, &root.join("x").join("y")));
		assert!(!is_inside(&root, &resolved(&["acme.wt-old", "x"])));
		assert!(!is_inside(&root.join("x"), &root));
	}

	#[test]
	fn real_path_follows_links_and_expands_a_missing_path_through_its_nearest_existing_ancestor() {
		let dir = temp_dir("pathkey");
		let dir = canonical(dir.path()).unwrap();
		let real = dir.join("real");
		std::fs::create_dir(&real).unwrap();
		let link = dir.join("link");
		make_link(&real, &link);
		assert_eq!(real_path(&link), real);
		assert_eq!(real_path(&link.join("missing").join("deeper")), real.join("missing").join("deeper"));
		assert!(same_dir(&link, &real));
		assert!(!same_path(&link, &real));
	}

	#[cfg(windows)]
	fn make_link(target: &Path, link: &Path) {
		let made =
			std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().unwrap();
		assert!(made.status.success(), "mklink /J failed");
	}

	#[cfg(unix)]
	fn make_link(target: &Path, link: &Path) {
		std::os::unix::fs::symlink(target, link).unwrap();
	}
}
