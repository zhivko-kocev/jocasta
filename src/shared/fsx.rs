//! Small file helpers shared by the service, setup and the CLI (one copy each).

use std::ffi::OsString;
use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

use crate::shared::errors::Result;

/// The file a write lands in: a symlink's target (a dotfile manager's link stays a link), else the file itself; and the
/// existing file's permission bits (POSIX only), so a private `config.yml` or `mcp.json` stays private (SEC-LOC-9).
struct WriteTarget {
	path: PathBuf,
	#[cfg_attr(not(unix), allow(dead_code))]
	mode: Option<u32>,
}

#[cfg(unix)]
fn mode_of(meta: &fs::Metadata) -> Option<u32> {
	use std::os::unix::fs::PermissionsExt;
	Some(meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode_of(_meta: &fs::Metadata) -> Option<u32> {
	None
}

/// `path` as the OS names it, without Windows' `\\?\` prefix on a drive path (Node's `realpath` has none).
fn plain(path: PathBuf) -> PathBuf {
	match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
		Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
		_ => path,
	}
}

/// `path` with its `.` and `..` steps resolved by name alone, as Node's `path.join` and `path.resolve` do.
pub fn normalize(path: &Path) -> PathBuf {
	let mut parts: Vec<Component> = Vec::new();
	for part in path.components() {
		match part {
			Component::CurDir => {}
			Component::ParentDir => match parts.last() {
				Some(Component::Normal(_)) => {
					parts.pop();
				}
				Some(Component::RootDir | Component::Prefix(_)) => {}
				_ => parts.push(part),
			},
			_ => parts.push(part),
		}
	}
	parts.iter().collect()
}

fn write_target(file: &Path) -> WriteTarget {
	let Ok(meta) = fs::symlink_metadata(file) else { return WriteTarget { path: file.to_path_buf(), mode: None } };
	if !meta.file_type().is_symlink() {
		return WriteTarget { path: file.to_path_buf(), mode: mode_of(&meta) };
	}
	match fs::canonicalize(file) {
		Ok(path) => {
			let path = plain(path);
			let mode = fs::symlink_metadata(&path).ok().and_then(|m| mode_of(&m));
			WriteTarget { path, mode }
		}
		Err(_) => {
			// A dangling link: its target is made.
			let link = fs::read_link(file).unwrap_or_default();
			let base = file.parent().unwrap_or(Path::new(""));
			WriteTarget { path: normalize(&base.join(link)), mode: None }
		}
	}
}

/// How often, and how far apart, a refused rename is tried again (0534): 10 times, 100 ms apart.
const RENAME_TRIES: u32 = 10;
const RENAME_RETRY_MS: u64 = 100;

/// Windows' `ERROR_ACCESS_DENIED`, `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`.
const WIN_BUSY: [i32; 3] = [5, 32, 33];

/// Errors Windows gives a rename over a file another process has open (omp reading its overlay, herdr `hud.txt`).
fn rename_busy(error: &io::Error) -> bool {
	cfg!(windows)
		&& (matches!(error.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy)
			|| error.raw_os_error().is_some_and(|code| WIN_BUSY.contains(&code)))
}

fn write_temp(temp: &Path, data: &[u8], mode: Option<u32>) -> io::Result<()> {
	#[cfg(unix)]
	if let Some(mode) = mode {
		use std::io::Write;
		use std::os::unix::fs::OpenOptionsExt;
		return fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(temp)?.write_all(data);
	}
	#[cfg(not(unix))]
	let _ = mode;
	fs::write(temp, data)
}

fn write_and_rename(temp: &Path, target: &WriteTarget, data: &[u8], mode: Option<u32>) -> io::Result<()> {
	write_temp(temp, data, mode)?;
	// Past the umask: the bits the file had.
	#[cfg(unix)]
	if let (None, Some(bits)) = (mode, target.mode) {
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(temp, fs::Permissions::from_mode(bits))?;
	}
	let mut attempt = 1;
	loop {
		match fs::rename(temp, &target.path) {
			Ok(()) => return Ok(()),
			Err(error) => {
				if !rename_busy(&error) || attempt >= RENAME_TRIES {
					return Err(error);
				}
				attempt += 1;
				std::thread::sleep(Duration::from_millis(RENAME_RETRY_MS));
			}
		}
	}
}

/// A temp file beside `file` for one write: the process id and a fresh suffix, so neither two processes nor two threads
/// of one process writing the same file at once share it (one rename would find it moved already).
pub fn temp_beside(file: &Path) -> PathBuf {
	let short = uuid::Uuid::new_v4().simple().to_string();
	let mut temp = OsString::from(file);
	temp.push(format!(".{}-{}.tmp", std::process::id(), &short[..8]));
	PathBuf::from(temp)
}

/// Writes `file` through a temp file in the same folder and a rename, so a crash or a full disk never leaves it torn.
/// A symlinked `file` writes its target; an existing file keeps its permission bits. `mode` applies to a new file
/// (POSIX only; Windows ignores it). A rename Windows refuses while another process has the file open is tried again
/// for about a second (0534).
pub fn write_atomically(file: &Path, data: impl AsRef<[u8]>, mode: Option<u32>) -> Result<()> {
	let target = write_target(file);
	if let Some(dir) = target.path.parent() {
		fs::create_dir_all(dir)?;
	}
	let temp = temp_beside(&target.path);
	write_and_rename(&temp, &target, data.as_ref(), mode).inspect_err(|_| {
		let _ = fs::remove_file(&temp);
	})?;
	Ok(())
}

/// How many times a busy entry's delete is tried, and how far apart.
const DELETE_TRIES: u32 = 5;
const DELETE_RETRY_MS: u64 = 200;

/// Windows' `ERROR_DIR_NOT_EMPTY`.
const WIN_DIR_NOT_EMPTY: i32 = 145;

/// Errors a delete on Windows meets while another process still has the entry open, or while it is read-only.
fn retried_kind(error: &io::Error) -> bool {
	matches!(
		error.kind(),
		io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy | io::ErrorKind::DirectoryNotEmpty
	) || (cfg!(windows)
		&& error.raw_os_error().is_some_and(|code| WIN_BUSY.contains(&code) || code == WIN_DIR_NOT_EMPTY))
}

/// A failed delete's `error` on try `attempt`: `Ok(true)` when the entry already is gone, `Ok(false)` while busy; else
/// the error.
fn delete_failed(error: io::Error, attempt: u32) -> io::Result<bool> {
	if error.kind() == io::ErrorKind::NotFound {
		return Ok(true);
	}
	if attempt >= DELETE_TRIES || !retried_kind(&error) {
		return Err(error);
	}
	Ok(false)
}

/// `remove()`, tried again a few times while the entry is busy; an entry already gone counts as removed.
fn retried(mut remove: impl FnMut() -> io::Result<()>) -> io::Result<()> {
	let mut attempt = 1;
	loop {
		match remove() {
			Ok(()) => return Ok(()),
			Err(error) => {
				if delete_failed(error, attempt)? {
					return Ok(());
				}
				attempt += 1;
				std::thread::sleep(Duration::from_millis(DELETE_RETRY_MS));
			}
		}
	}
}

/// `retried` without holding the thread: the waits between tries are timers (DUP-13).
async fn retried_async<F, Fut>(mut remove: F) -> io::Result<()>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = io::Result<()>>,
{
	let mut attempt = 1;
	loop {
		match remove().await {
			Ok(()) => return Ok(()),
			Err(error) => {
				if delete_failed(error, attempt)? {
					return Ok(());
				}
				attempt += 1;
				tokio::time::sleep(Duration::from_millis(DELETE_RETRY_MS)).await;
			}
		}
	}
}

/// Whether a failed `unlink` of a link may want `rmdir` instead (a folder link on Windows: junction or directory
/// symlink).
fn wants_rmdir(error: &io::Error) -> bool {
	matches!(error.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::IsADirectory)
}

/// Makes read-only file `path` writable (git's objects).
fn writable(path: &Path) -> io::Result<()> {
	let mut permissions = fs::metadata(path)?.permissions();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		permissions.set_mode(0o666);
	}
	#[cfg(not(unix))]
	#[allow(clippy::permissions_set_readonly_false)]
	permissions.set_readonly(false);
	fs::set_permissions(path, permissions)
}

fn unlink_link(path: &Path) -> io::Result<()> {
	fs::remove_file(path).or_else(|error| if wants_rmdir(&error) { fs::remove_dir(path) } else { Err(error) })
}

fn unlink_file(path: &Path) -> io::Result<()> {
	fs::remove_file(path).or_else(|error| {
		if !matches!(error.kind(), io::ErrorKind::PermissionDenied) {
			return Err(error);
		}
		writable(path)?;
		fs::remove_file(path)
	})
}

fn remove_children(path: &Path) -> io::Result<()> {
	for entry in fs::read_dir(path)? {
		remove_tree_io(&entry?.path())?;
	}
	Ok(())
}

fn remove_dir_again(path: &Path) -> io::Result<()> {
	fs::remove_dir(path).or_else(|error| {
		if error.kind() != io::ErrorKind::DirectoryNotEmpty
			&& !(cfg!(windows) && error.raw_os_error() == Some(WIN_DIR_NOT_EMPTY))
		{
			return Err(error);
		}
		remove_children(path)?;
		fs::remove_dir(path)
	})
}

fn remove_tree_io(path: &Path) -> io::Result<()> {
	let meta = match fs::symlink_metadata(path) {
		Ok(meta) => meta,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
		Err(error) => return Err(error),
	};
	if meta.file_type().is_symlink() {
		return retried(|| unlink_link(path));
	}
	if meta.is_dir() {
		remove_children(path)?;
		// An entry written after the listing (a git still finishing in a worktree) leaves the folder not empty: each
		// try lists it again.
		return retried(|| remove_dir_again(path));
	}
	retried(|| unlink_file(path))
}

/// Removes `path` and everything under it without following links (0484): a symlink or a Windows junction is removed
/// itself and its target is never touched. Read-only files (git's objects) are made writable first.
pub fn remove_tree(path: &Path) -> Result<()> {
	Ok(remove_tree_io(path)?)
}

async fn remove_children_async(path: &Path) -> io::Result<()> {
	let mut entries = tokio::fs::read_dir(path).await?;
	while let Some(entry) = entries.next_entry().await? {
		Box::pin(remove_tree_async_io(entry.path())).await?;
	}
	Ok(())
}

async fn unlink_link_async(path: &Path) -> io::Result<()> {
	match tokio::fs::remove_file(path).await {
		Err(error) if wants_rmdir(&error) => tokio::fs::remove_dir(path).await,
		other => other,
	}
}

async fn unlink_file_async(path: &Path) -> io::Result<()> {
	match tokio::fs::remove_file(path).await {
		Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
			let owned = path.to_path_buf();
			tokio::task::spawn_blocking(move || writable(&owned)).await.map_err(io::Error::other)??;
			tokio::fs::remove_file(path).await
		}
		other => other,
	}
}

async fn remove_dir_again_async(path: &Path) -> io::Result<()> {
	match tokio::fs::remove_dir(path).await {
		Err(error)
			if error.kind() == io::ErrorKind::DirectoryNotEmpty
				|| (cfg!(windows) && error.raw_os_error() == Some(WIN_DIR_NOT_EMPTY)) =>
		{
			remove_children_async(path).await?;
			tokio::fs::remove_dir(path).await
		}
		other => other,
	}
}

async fn remove_tree_async_io(path: PathBuf) -> io::Result<()> {
	let meta = match tokio::fs::symlink_metadata(&path).await {
		Ok(meta) => meta,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
		Err(error) => return Err(error),
	};
	if meta.file_type().is_symlink() {
		return retried_async(|| unlink_link_async(&path)).await;
	}
	if meta.is_dir() {
		remove_children_async(&path).await?;
		return retried_async(|| remove_dir_again_async(&path)).await;
	}
	retried_async(|| unlink_file_async(&path)).await
}

/// [`remove_tree`] for the service's event loop (DUP-13): the same removal, links never followed and read-only files
/// made writable first, with the file system asked asynchronously and the waits between a busy entry's tries as
/// timers. The CLI, setup and tests keep the synchronous one.
pub async fn remove_tree_async(path: &Path) -> Result<()> {
	Ok(remove_tree_async_io(path.to_path_buf()).await?)
}

/// Makes folder `path` (and its missing parents) and, on Linux and macOS, sets it to 0700 so other local users can't
/// read what is in it (0484). Windows keeps the folder's inherited ACL (`%LOCALAPPDATA%` and the profile are private).
/// `posix` defaults to whether the OS is not Windows.
pub fn private_dir(path: &Path, posix: Option<bool>) -> Result<()> {
	let posix = posix.unwrap_or(!cfg!(windows));
	#[cfg(unix)]
	if posix {
		use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
		fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
		fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
		return Ok(());
	}
	let _ = posix;
	fs::create_dir_all(path)?;
	Ok(())
}

/// `value` when it is a plain JSON object.
pub fn as_object(value: &Value) -> Option<&Map<String, Value>> {
	value.as_object()
}

/// `file` parsed as a JSON object; `None` when it is missing, unreadable, not JSON or not an object.
pub fn read_json_object(file: &Path) -> Option<Map<String, Value>> {
	match serde_json::from_str(&fs::read_to_string(file).ok()?).ok()? {
		Value::Object(object) => Some(object),
		_ => None,
	}
}

/// Constant-time token comparison; tokens of different UTF-8 lengths differ.
pub fn same_token(given: Option<&str>, token: &str) -> bool {
	let Some(given) = given.filter(|g| !g.is_empty()) else { return false };
	if given.len() != token.len() {
		return false;
	}
	given.bytes().zip(token.bytes()).fold(0_u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

/// A loopback port the OS says is free now.
pub fn free_port() -> Result<u16> {
	Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::{TempDir, temp_dir};

	/// A folder link at `link` to `target`: a junction on Windows (no admin needed), a symlink elsewhere.
	fn folder_link(target: &Path, link: &Path) {
		#[cfg(windows)]
		{
			let mut command = std::process::Command::new("cmd");
			command.args(["/C", "mklink", "/J"]).arg(link).arg(target);
			crate::shared::env::apply_to(&mut command);
			assert!(command.output().unwrap().status.success(), "mklink /J failed");
		}
		#[cfg(unix)]
		std::os::unix::fs::symlink(target, link).unwrap();
	}

	fn root() -> (TempDir, PathBuf) {
		let dir = temp_dir("hardening");
		let path = fs::canonicalize(dir.path()).map(plain).unwrap();
		(dir, path)
	}

	#[test]
	fn remove_tree_removes_a_link_inside_the_tree_never_what_it_points_at() {
		let (_dir, root) = root();
		let victim = root.join("victim");
		fs::create_dir_all(victim.join("deep")).unwrap();
		fs::write(victim.join("keep.txt"), "his").unwrap();
		fs::write(victim.join("deep").join("keep.txt"), "his").unwrap();
		let tree = root.join("worktree");
		fs::create_dir_all(tree.join("node_modules")).unwrap();
		fs::write(tree.join("a.txt"), "a").unwrap();
		let linked = tree.join("node_modules").join("linked");
		folder_link(&victim, &linked);
		assert!(fs::symlink_metadata(&linked).unwrap().file_type().is_symlink());

		remove_tree(&tree).unwrap();

		assert!(!tree.exists());
		assert_eq!(fs::read_to_string(victim.join("keep.txt")).unwrap(), "his");
		assert_eq!(fs::read_to_string(victim.join("deep").join("keep.txt")).unwrap(), "his");
	}

	#[test]
	fn remove_tree_given_a_link_itself_removes_only_the_link_and_a_dangling_one_too() {
		let (_dir, root) = root();
		let victim = root.join("victim");
		fs::create_dir(&victim).unwrap();
		fs::write(victim.join("keep.txt"), "his").unwrap();
		let link = root.join("link");
		folder_link(&victim, &link);
		remove_tree(&link).unwrap();
		assert!(fs::symlink_metadata(&link).is_err());
		assert!(victim.join("keep.txt").exists());

		let gone = root.join("gone");
		fs::create_dir(&gone).unwrap();
		let dangling = root.join("dangling");
		folder_link(&gone, &dangling);
		fs::remove_dir_all(&gone).unwrap();
		remove_tree(&dangling).unwrap();
		assert!(fs::symlink_metadata(&dangling).is_err());
	}

	#[test]
	fn remove_tree_removes_read_only_files_and_is_quiet_about_a_path_already_gone() {
		let (_dir, root) = root();
		let tree = root.join("tree");
		fs::create_dir_all(tree.join(".git").join("objects")).unwrap();
		let object = tree.join(".git").join("objects").join("ab");
		fs::write(&object, "blob").unwrap();
		let mut permissions = fs::metadata(&object).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&object, permissions).unwrap();
		remove_tree(&tree).unwrap();
		assert!(!tree.exists());
		remove_tree(&tree).unwrap();
		remove_tree(&root.join("never")).unwrap();
	}

	#[tokio::test]
	async fn remove_tree_async_removes_links_not_targets_and_read_only_files() {
		let (_dir, root) = root();
		let victim = root.join("victim");
		fs::create_dir(&victim).unwrap();
		fs::write(victim.join("keep.txt"), "his").unwrap();
		let tree = root.join("tree");
		fs::create_dir_all(tree.join("sub")).unwrap();
		let object = tree.join("sub").join("ab");
		fs::write(&object, "blob").unwrap();
		let mut permissions = fs::metadata(&object).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&object, permissions).unwrap();
		folder_link(&victim, &tree.join("linked"));
		remove_tree_async(&tree).await.unwrap();
		assert!(!tree.exists());
		assert!(victim.join("keep.txt").exists());
		remove_tree_async(&tree).await.unwrap();
	}

	#[cfg(unix)]
	#[test]
	fn write_atomically_writes_a_symlinked_files_target_and_keeps_the_files_mode() {
		use std::os::unix::fs::{PermissionsExt, symlink};
		let (_dir, root) = root();
		let dotfiles = root.join("dotfiles");
		fs::create_dir(&dotfiles).unwrap();
		let real = dotfiles.join("config.yml");
		fs::write(&real, "old: 1\n").unwrap();
		fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
		let link = root.join("config.yml");
		symlink(&real, &link).unwrap();

		write_atomically(&link, "new: 2\n", None).unwrap();

		assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
		assert_eq!(fs::read_to_string(&real).unwrap(), "new: 2\n");
		assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);

		// A dangling link makes its target.
		let dangling = root.join("rc");
		symlink(dotfiles.join("rc"), &dangling).unwrap();
		write_atomically(&dangling, "x\n", None).unwrap();
		assert!(fs::symlink_metadata(&dangling).unwrap().file_type().is_symlink());
		assert_eq!(fs::read_to_string(dotfiles.join("rc")).unwrap(), "x\n");
	}

	#[test]
	fn write_atomically_replaces_a_plain_files_text_and_makes_a_missing_one_with_its_folders() {
		let (_dir, root) = root();
		let file = root.join("a").join("b.txt");
		write_atomically(&file, "one\n", None).unwrap();
		write_atomically(&file, "two\n", None).unwrap();
		assert_eq!(fs::read_to_string(&file).unwrap(), "two\n");
	}

	#[test]
	fn write_atomically_leaves_the_whole_file_and_no_temp_file_behind() {
		let (_dir, root) = root();
		let file = root.join("deep").join("state.json");
		write_atomically(&file, "one\n", None).unwrap();
		write_atomically(&file, "two\n", None).unwrap();
		assert_eq!(fs::read_to_string(&file).unwrap(), "two\n");
		let names: Vec<_> = fs::read_dir(root.join("deep")).unwrap().map(|e| e.unwrap().file_name()).collect();
		assert_eq!(names, vec![OsString::from("state.json")]);
	}

	#[test]
	fn write_atomically_lets_threads_of_one_process_write_the_same_file_at_once() {
		// The service's tasks run on many threads: two writes of one file must not share a temp file, or one rename
		// finds it already moved (ENOENT).
		let (_dir, root) = root();
		let file = root.join("state.json");
		let writers: Vec<_> = (0..8)
			.map(|writer| {
				let file = file.clone();
				std::thread::spawn(move || {
					(0..40).try_for_each(|round| write_atomically(&file, format!("{writer}-{round}\n"), None))
				})
			})
			.collect();
		for writer in writers {
			writer.join().unwrap().unwrap();
		}
		let text = fs::read_to_string(&file).unwrap();
		let (writer, round) = text.trim_end().split_once('-').unwrap();
		assert!(writer.parse::<u8>().unwrap() < 8 && round.parse::<u8>().unwrap() < 40 && text.ends_with('\n'));
		let names: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect();
		assert_eq!(names, vec![OsString::from("state.json")]);
	}

	#[test]
	fn private_dir_makes_the_folder_and_its_parents_0700_where_the_os_has_modes() {
		let (_dir, root) = root();
		let dir = root.join("a").join("b");
		private_dir(&dir, None).unwrap();
		assert!(dir.is_dir());
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
		}
	}

	#[test]
	fn same_token_compares_whole_tokens() {
		assert!(same_token(Some("abc"), "abc"));
		assert!(!same_token(Some("abd"), "abc"));
		assert!(!same_token(None, "abc"));
		assert!(!same_token(Some(""), ""));
		// Same length in characters, not in UTF-8 bytes: unequal, never an error (SE-3).
		assert!(!same_token(Some(&"é".repeat(64)), &"a".repeat(64)));
	}

	#[test]
	fn json_helpers_and_a_free_port() {
		let (_dir, root) = root();
		let file = root.join("o.json");
		fs::write(&file, r#"{"a": 1}"#).unwrap();
		assert_eq!(read_json_object(&file).unwrap()["a"], 1);
		fs::write(&file, "[1]").unwrap();
		assert!(read_json_object(&file).is_none());
		fs::write(&file, "{").unwrap();
		assert!(read_json_object(&file).is_none());
		assert!(read_json_object(&root.join("missing.json")).is_none());
		assert!(as_object(&serde_json::json!([])).is_none());
		assert!(free_port().unwrap() > 0);
	}
}
