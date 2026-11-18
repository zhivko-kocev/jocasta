//! `jocasta uninstall` (0468): removes exactly what install and setup added. The installer records its part in
//! `install.json` (the folder, the release's top-level entries, its PATH entry, the autostart value, the companion's
//! WebView2 data); setup's migrations record theirs in their markers (`setup_undo`). herdr session `jocasta` and
//! Jocasta's per-project mise configs (0477) go too. The data folder (history, models) is asked about separately. Keys
//! setup merged into his omp `config.yml` stay: they are his settings. In `keybindings.yml` and `lsp.json` only the
//! keys setup added go (in `keybindings.yml` only their lines, 0584), and
//! `WATCHDOG.md` and `secrets.yml` only while each is still Jocasta's text (0479, 0594). From the install folder only
//! the release's entries go, and the folder once it is empty (0579); omp sessions still running programs from it are
//! named and refused first.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock};

use futures_util::future::BoxFuture;
use regex::Regex;
use serde_json::{Map, Value};

use crate::frontier::server::launch::ServiceState;
use crate::setup::config_file::{edit_config, parse_config};
use crate::setup::markers::{Undo, fingerprint};
use crate::setup::run_value::{RUN_KEY, delete_run_value, read_run_value};
use crate::setup::setup::{McpUndo, RunProgram, SetupIo, SetupUndo, answered_yes};
use crate::shared::env;
use crate::shared::errors::{Error, Result};
use crate::shared::fsx::{read_json_object, remove_tree, write_atomically};
use crate::shared::herdr_session::{DEFAULT_SESSION, session_args};
use crate::shared::install::INSTALL_MARKER;
use crate::shared::pathkey::{is_inside, resolve, same_path};

/// What the installer added (`install.json`). An older Windows file names only `dir`: the rest is then what
/// `install.ps1` adds today.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallRecord {
	/// The install folder.
	pub dir: PathBuf,
	/// The release's top-level entries the installer put there (0579); absent in an install from before the list.
	pub entries: Option<Vec<String>>,
	/// The entry `install.ps1` put on the user PATH.
	pub path: Option<PathBuf>,
	/// The shell rc file `install.sh` appended its PATH line to.
	pub rc: Option<PathBuf>,
	/// The autostart Run value's name under `HKCU\…\Run` (the companion writes it, 0367).
	pub autostart: Option<String>,
	/// The companion's WebView2 data, `%LOCALAPPDATA%\<app identifier>`.
	pub app_data: Option<PathBuf>,
}

/// A plain name in the install folder: never a path, `.` or `..`.
fn plain_name(value: &Value) -> Option<String> {
	let name = value.as_str()?;
	let plain = !name.is_empty() && name != "." && name != ".." && !name.contains(['\\', '/', ':']);
	plain.then(|| name.to_owned())
}

/// `<dir>/install.json`, or `None` when there is none (a source run, a copy) or it names another folder (0484). The
/// companion's window data counts only directly under `%LOCALAPPDATA%`. `windows` defaults to the platform.
pub fn read_install_record(dir: &Path, local_app_data: Option<&Path>, windows: Option<bool>) -> Option<InstallRecord> {
	let windows = windows.unwrap_or(cfg!(windows));
	let value = read_json_object(&dir.join(INSTALL_MARKER))?;
	let recorded = value.get("dir")?.as_str()?;
	if !same_path(Path::new(recorded), dir) {
		return None;
	}
	let local = local_app_data.filter(|local| !local.as_os_str().is_empty());
	let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
	let mut record = InstallRecord {
		dir: PathBuf::from(recorded),
		entries: value
			.get("entries")
			.and_then(Value::as_array)
			.and_then(|entries| entries.iter().map(plain_name).collect::<Option<Vec<_>>>()),
		path: text("path").map(PathBuf::from),
		rc: text("rc").map(PathBuf::from),
		autostart: text("autostart"),
		app_data: text("appData").map(PathBuf::from),
	};
	if windows
		&& record.path.is_none()
		&& record.rc.is_none()
		&& record.autostart.is_none()
		&& record.app_data.is_none()
	{
		record.path = Some(PathBuf::from(recorded));
		record.autostart = Some("Jocasta".to_owned());
		if let Some(local) = local {
			record.app_data = Some(local.join("local.jocasta.frontier"));
		}
	}
	if let Some(app_data) = &record.app_data {
		let parent = app_data.parent().map_or_else(
			|| Path::new("."),
			|parent| if parent.as_os_str().is_empty() { Path::new(".") } else { parent },
		);
		if !local.is_some_and(|local| same_path(parent, local)) {
			record.app_data = None;
		}
	}
	Some(record)
}

/// The line `install.sh` appends to the rc file for `dir`.
pub fn rc_path_line(dir: &Path) -> String {
	format!("export PATH=\"{}:$PATH\" # jocasta", dir.display())
}

/// Removes `install.sh`'s PATH line from `rc`; false when it isn't there.
pub fn remove_rc_line(rc: &Path, dir: &Path) -> Result<bool> {
	let Ok(bytes) = fs::read(rc) else { return Ok(false) };
	let text = String::from_utf8_lossy(&bytes);
	let line = rc_path_line(dir);
	let lines: Vec<&str> = text.split('\n').collect();
	let kept: Vec<&str> = lines.iter().copied().filter(|l| *l != line).collect();
	if kept.len() == lines.len() {
		return Ok(false);
	}
	let mut joined = kept.join("\n");
	if joined.ends_with("\n\n") {
		joined = format!("{}\n", joined.trim_end_matches('\n'));
	}
	write_atomically(rc, joined, None)?;
	Ok(true)
}

/// Where uninstall works.
pub struct UninstallPaths {
	pub jocasta_config_dir: PathBuf,
	pub data_dir: PathBuf,
	/// mise's global config dir, for Jocasta's per-project configs (0477).
	pub mise_config_dir: PathBuf,
	/// The install this command removes; `None` in a source run.
	pub install: Option<InstallRecord>,
	/// Whether this process runs from the install and Windows holds its program open: Jocasta's entries there go after
	/// it exits. On Linux and macOS a running program's files can be removed at once.
	pub running_from_install: bool,
}

type RemovePathEntry = Arc<dyn Fn(&Path) -> Result<bool> + Send + Sync>;
type RemoveRunValue = Arc<dyn Fn(&str, &Path) -> Result<bool> + Send + Sync>;
type RemoveAfterExit = Arc<dyn Fn(&Path, &[String]) -> Result<()> + Send + Sync>;
type ProcessesUnder = Arc<dyn Fn(&Path) -> Vec<InstallProcess> + Send + Sync>;

/// What uninstall asks of the machine, so a test can stand in for it.
#[derive(Clone)]
pub struct UninstallDeps {
	pub service_state: Arc<dyn Fn() -> BoxFuture<'static, Result<ServiceState>> + Send + Sync>,
	/// The herdr binary, or `None` when herdr isn't installed.
	pub herdr: Option<PathBuf>,
	/// herdr's session, `jocasta` (`JOCASTA_HERDR_SESSION` for checks).
	pub session: String,
	pub run: RunProgram,
	/// How to undo what setup added here (`setup_undo`).
	pub setup_undo: Vec<SetupUndo>,
	/// Removes `entry` from the user PATH; false when it wasn't there.
	pub remove_path_entry: RemovePathEntry,
	/// Removes Run value `name` when it starts `launcher`; false when it doesn't.
	pub remove_run_value: RemoveRunValue,
	pub delete_credential: Arc<dyn Fn(&str) -> bool + Send + Sync>,
	/// Removes `entries` of the install folder `dir`, then `install.json` and the folder if empty, once this process
	/// exited.
	pub remove_after_exit: RemoveAfterExit,
	/// The other processes running a program from the install folder (`processes_under`); none when absent.
	pub processes_under: Option<ProcessesUnder>,
	/// Starts herdr's server for `session` headless and waits for it, for `plugin unlink` (0558); absent in tests.
	pub start_herdr: Option<Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>>,
}

/// One thing to remove: what it is, and how.
struct Removal {
	what: String,
	action: Action,
}

/// How one removal is done.
enum Action {
	Path(PathBuf),
	SetupFolder(PathBuf, Option<String>),
	Mcp(McpUndo),
	HerdrIntegration(PathBuf),
	HerdrPlugin(String),
	HerdrSession,
	Keys(PathBuf, Map<String, Value>, bool),
	Text(PathBuf, String),
	Credential(String),
	Autostart(String, PathBuf),
	PathEntry(PathBuf),
	RcLine(PathBuf, PathBuf),
	WindowData(PathBuf),
}

/// Files in Jocasta's config folder that are his, not Jocasta's: the private-names list the repo scan reads (AGENTS.md).
const HIS_CONFIG_FILES: [&str; 1] = ["private-names.txt"];

/// Whether `path` is there, a dangling link included.
fn present(path: &Path) -> bool {
	fs::symlink_metadata(path).is_ok()
}

/// Why folder `path` mustn't be removed whole (0484): a drive root, or his home folder or one holding it.
pub fn unsafe_folder(path: &Path) -> Option<&'static str> {
	let full = resolve(path);
	if full.parent().is_none() {
		return Some("it is a drive root");
	}
	if is_inside(&full, &std::env::home_dir().unwrap_or_default()) {
		return Some("it is your home folder or holds it");
	}
	None
}

/// Removes a file or folder without following links (0484); says what it did. A folder that is a drive root or his
/// home folder is refused.
fn remove_path(path: &Path) -> Result<String> {
	if !present(path) {
		return Ok(format!("{} was already gone", path.display()));
	}
	if let Some(unsafe_reason) = unsafe_folder(path) {
		return Err(Error::other(format!("kept {}: {unsafe_reason}", path.display())));
	}
	remove_tree(path)?;
	Ok(format!("removed {}", path.display()))
}

/// Removes a folder whose path was recorded (install.json, setup's markers) only while it still holds Jocasta's
/// `marker` (0484); `holds` checks it.
fn remove_recorded_folder(path: &Path, marker: &str, holds: impl Fn(&Path) -> bool) -> Result<String> {
	if !present(path) {
		return Ok(format!("{} was already gone", path.display()));
	}
	if !holds(path) {
		return Err(Error::other(format!("kept {}: it no longer holds {marker}", path.display())));
	}
	remove_path(path)
}

/// Whether `dir` is the install `install.json` names: the marker there names this same folder.
fn holds_install_marker(dir: &Path) -> bool {
	read_json_object(&dir.join(INSTALL_MARKER))
		.and_then(|marker| marker.get("dir").and_then(Value::as_str).map(|named| same_path(Path::new(named), dir)))
		.unwrap_or(false)
}

/// The top-level names a release has shipped (appendix I), the companion's `app` (0552) included.
const RELEASE_NAMES: [&str; 11] = [
	"jocasta.exe",
	"jocasta",
	"config",
	"node",
	"whiteboard",
	"whiteboard-canvas",
	"editor.worker.js",
	"hunk",
	"README.md",
	"CHANGELOG.md",
	"app",
];

/// What of the install folder uninstall removes (0579), and what it leaves there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallContents {
	/// Jocasta's entries there now, `install.json` aside (it goes last).
	pub ours: Vec<String>,
	/// His other entries: the folder stays for them.
	pub others: Vec<String>,
}

/// The install folder's entries by owner (0579): the ones `install.json` lists, with the `<name>.old-<stamp>` copies an
/// upgrade renamed aside while they were in use (install.ps1); an install from before the list is all Jocasta's only
/// when every entry is a release's name, else it is refused whole. `windows` defaults to the platform.
pub fn install_contents(record: &InstallRecord, windows: Option<bool>) -> Result<InstallContents> {
	static OLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.old-\d+$").expect("a fixed pattern"));
	let windows = windows.unwrap_or(cfg!(windows));
	let key = |name: &str| if windows { name.to_lowercase() } else { name.to_owned() };
	let listed: Vec<String> = match &record.entries {
		Some(entries) => entries.iter().map(|name| key(name)).collect(),
		None => RELEASE_NAMES.iter().map(|name| key(name)).collect(),
	};
	let is_ours = |name: &str| listed.contains(&key(&OLD.replace(name, "")));
	let mut names: Vec<String> = fs::read_dir(&record.dir)?
		.map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
		.collect::<std::io::Result<_>>()?;
	names.retain(|name| key(name) != key(INSTALL_MARKER));
	names.sort_by_key(|name| name.encode_utf16().collect::<Vec<u16>>());
	let (ours, others): (Vec<String>, Vec<String>) = names.into_iter().partition(|name| is_ours(name));
	if record.entries.is_none() && !others.is_empty() {
		return Err(Error::other(format!(
			"kept {}: it holds files that aren't Jocasta's ({}); remove Jocasta's files from it by hand",
			record.dir.display(),
			others.join(", ")
		)));
	}
	Ok(InstallContents { ours, others })
}

/// Removes `path` as `rmSync(path, { force: true })` does: a file already gone is no error.
fn remove_file_forced(path: &Path) -> Result<()> {
	match fs::remove_file(path) {
		Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
		_ => Ok(()),
	}
}

/// Removes Jocasta's entries from the install folder, `install.json` last, then the folder when nothing else is left.
fn remove_install(dir: &Path, contents: &InstallContents) -> Result<String> {
	for name in &contents.ours {
		remove_tree(&dir.join(name))?;
	}
	remove_file_forced(&dir.join(INSTALL_MARKER))?;
	if !contents.others.is_empty() {
		return Ok(format!("removed Jocasta's files from {}; kept the folder (it holds other files)", dir.display()));
	}
	remove_if_empty(dir);
	Ok(format!("removed {}", dir.display()))
}

/// A process whose program is under the install folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallProcess {
	pub pid: u32,
	/// Its program's full path.
	pub path: String,
	/// Its command line as the OS reports it.
	pub command_line: String,
}

/// `path.win32.isAbsolute`.
fn win32_is_absolute(path: &str) -> bool {
	let bytes = path.as_bytes();
	match bytes.first() {
		Some(b'/' | b'\\') => true,
		Some(first) => {
			first.is_ascii_alphabetic() && bytes.get(1) == Some(&b':') && matches!(bytes.get(2), Some(b'/' | b'\\'))
		}
		None => false,
	}
}

/// A Windows path as its root and its folded components (`.` and `..` resolved, no working folder consulted).
fn win32_parts(path: &str) -> (String, Vec<String>) {
	let text = path.replace('/', "\\");
	let bytes = text.as_bytes();
	let (root, rest) = if let Some(unc) = text.strip_prefix("\\\\") {
		let mut parts = unc.splitn(3, '\\');
		let server = parts.next().unwrap_or_default();
		let share = parts.next().unwrap_or_default();
		(format!("\\\\{server}\\{share}"), parts.next().unwrap_or_default().to_owned())
	} else if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
		(text[..2].to_owned(), text[2..].to_owned())
	} else {
		(String::new(), text.clone())
	};
	let mut parts: Vec<String> = Vec::new();
	for part in rest.split('\\').filter(|part| !part.is_empty() && *part != ".") {
		if part == ".." {
			parts.pop();
		} else {
			parts.push(part.to_owned());
		}
	}
	(root, parts)
}

/// `path.win32.relative(from, to)` for two absolute paths.
fn win32_relative(from: &str, to: &str) -> String {
	let (from_root, from_parts) = win32_parts(from);
	let (to_root, to_parts) = win32_parts(to);
	if !from_root.is_empty() && !to_root.is_empty() && from_root.to_lowercase() != to_root.to_lowercase() {
		return format!("{to_root}\\{}", to_parts.join("\\"));
	}
	let common = from_parts.iter().zip(&to_parts).take_while(|(a, b)| a.to_lowercase() == b.to_lowercase()).count();
	let mut out: Vec<&str> = vec![".."; from_parts.len() - common];
	out.extend(to_parts[common..].iter().map(String::as_str));
	out.join("\\")
}

/// `path.win32.basename`.
fn win32_basename(path: &str) -> String {
	let trimmed = path.trim_end_matches(['\\', '/']);
	let bytes = trimmed.as_bytes();
	let start = if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' { 2 } else { 0 };
	let name = &trimmed[start..];
	name.rsplit(['\\', '/']).next().unwrap_or_default().to_owned()
}

/// How the refusal names a process (0579): its program and up to three arguments, a path inside the install shortened
/// to its top-level entry (`node.exe whiteboard mcp (pid 5332)`). Windows paths: only Windows lists them.
pub fn describe_process(dir: &str, process: &InstallProcess) -> String {
	static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]*)"|(\S+)"#).expect("a fixed pattern"));
	let words = WORD
		.captures_iter(&process.command_line)
		.map(|m| m.get(1).or_else(|| m.get(2)).map_or("", |word| word.as_str()))
		.skip(1)
		.take(3);
	let mut shown = vec![win32_basename(&process.path)];
	for word in words {
		let inside = if win32_is_absolute(word) { win32_relative(dir, word) } else { String::new() };
		let within = !inside.is_empty() && !inside.starts_with("..") && !win32_is_absolute(&inside);
		shown.push(if within { inside.split('\\').next().unwrap_or_default().to_owned() } else { word.to_owned() });
	}
	format!("{} (pid {})", shown.join(" "), process.pid)
}

/// `powershell.exe -NoProfile -NonInteractive -Command <script>` with no window.
fn powershell(script: &str) -> Command {
	let mut command = Command::new("powershell.exe");
	command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		command.creation_flags(0x0800_0000);
	}
	env::apply_to(&mut command);
	command
}

/// The processes, other than this one, whose program is under `dir` (Windows: Win32_Process); none elsewhere.
pub fn processes_under(dir: &Path) -> Vec<InstallProcess> {
	if !cfg!(windows) {
		return Vec::new();
	}
	let script = "
$dir = $env:JOCASTA_INSTALL_DIR.TrimEnd('\\') + '\\'
Get-CimInstance Win32_Process | Where-Object {
	$_.ExecutablePath -and $_.ExecutablePath.StartsWith($dir, [StringComparison]::OrdinalIgnoreCase)
} | ForEach-Object { \"$($_.ProcessId)`t$($_.ExecutablePath)`t$($_.CommandLine)\" }
";
	let mut command = powershell(script);
	command.env("JOCASTA_INSTALL_DIR", dir);
	let Ok(output) = command.output() else { return Vec::new() };
	String::from_utf8_lossy(&output.stdout)
		.lines()
		.filter_map(|line| {
			let mut parts = line.split('\t');
			let (pid, path) = (parts.next()?, parts.next()?);
			let pid: u32 = pid.parse().ok().filter(|pid| *pid != std::process::id() && !path.is_empty())?;
			Some(InstallProcess {
				pid,
				path: path.to_owned(),
				command_line: parts.next().unwrap_or_default().to_owned(),
			})
		})
		.collect()
}

/// Whether `dir` holds the companion's WebView2 data: `<channel>/WebView2` (Electrobun's layout).
fn holds_window_data(dir: &Path) -> bool {
	let Ok(entries) = fs::read_dir(dir) else { return false };
	entries
		.flatten()
		.any(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()) && entry.path().join("WebView2").exists())
}

/// A folder setup recorded (0484): removed whole only with the file setup wrote in it still there. A record from before
/// 0484 doesn't name the file and may name a folder setup didn't make: that folder goes only when it is empty.
fn remove_setup_folder(path: &Path, file: Option<&str>) -> Result<String> {
	if let Some(file) = file {
		return remove_recorded_folder(path, file, |dir| dir.join(file).exists());
	}
	if !present(path) {
		return Ok(format!("{} was already gone", path.display()));
	}
	Ok(match fs::remove_dir(path) {
		Ok(()) => format!("removed {}", path.display()),
		Err(_) => format!("kept {}: setup may not have made it, and it isn't empty", path.display()),
	})
}

/// `"a", "b"`.
fn quoted(names: &[String]) -> String {
	names.iter().map(|name| format!("\"{name}\"")).collect::<Vec<_>>().join(", ")
}

/// Jocasta's servers out of an omp `mcp.json` (0468), each only while it still is what setup wrote: the entry setup
/// writes today, or, all together, the entries its marker fingerprinted. His other servers and keys stay; the file goes
/// when nothing else is in it.
fn remove_mcp_servers(item: &McpUndo) -> Result<String> {
	let path = Path::new(&item.path);
	let shown = &item.path;
	let text = match fs::read(path) {
		Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
		Err(error) if error.kind() == ErrorKind::NotFound => return Ok(format!("{shown} was already gone")),
		Err(_) => return Err(not_json(item)),
	};
	let Ok(parsed) = serde_json::from_str::<Value>(&text) else { return Err(not_json(item)) };
	let Some(config) = parsed.as_object() else { return Ok(format!("{shown} has no MCP servers to remove")) };
	let Some(current) = config.get("mcpServers").and_then(Value::as_object) else {
		return Ok(format!("{shown} has no MCP servers to remove"));
	};
	let present: Vec<&String> = item.servers.iter().filter(|name| current.contains_key(*name)).collect();
	let written: Map<String, Value> = present.iter().map(|name| ((*name).clone(), current[*name].clone())).collect();
	let as_written = fingerprint(&Value::Object(written).to_string()) == item.wrote;
	let (gone, yours): (Vec<String>, Vec<String>) = present
		.into_iter()
		.cloned()
		.partition(|name| as_written || item.writes.get(name).is_some_and(|wrote| *wrote == current[name]));
	if !gone.is_empty() {
		let kept: Map<String, Value> =
			current.iter().filter(|(name, _)| !gone.contains(name)).map(|(k, v)| (k.clone(), v.clone())).collect();
		if kept.is_empty() && config.len() == 1 {
			remove_file_forced(path)?;
			return Ok(format!("removed {shown}"));
		}
		let mut rewritten = config.clone();
		rewritten.insert("mcpServers".to_owned(), Value::Object(kept));
		write_atomically(path, format!("{}\n", serde_json::to_string_pretty(&Value::Object(rewritten))?), None)?;
	}
	let said = if gone.is_empty() {
		format!("{shown} has none of Jocasta's MCP servers left")
	} else {
		format!("removed MCP server{} {} from {shown}", if gone.len() > 1 { "s" } else { "" }, quoted(&gone))
	};
	Ok(if yours.is_empty() { said } else { format!("{said}; kept {} (not what setup wrote)", quoted(&yours)) })
}

fn not_json(item: &McpUndo) -> Error {
	Error::other(format!("{} is not JSON; remove {} from it by hand", item.path, item.servers.join(", ")))
}

/// The keys setup added out of one of his omp files (0479), each only while it still has the value setup wrote; in a
/// YAML file only their lines go, his comments and formatting stay (0584), and a file whose lines can't be edited is
/// left as it is. The file goes when setup made it and nothing else is left in it: no key, comment or line of his.
fn remove_keys(path: &Path, keys: &Map<String, Value>, created: bool) -> Result<String> {
	let names: Vec<&String> = keys.keys().collect();
	let shown = path.display();
	let unreadable = || {
		let list = names.iter().map(|name| name.as_str()).collect::<Vec<_>>().join(", ");
		Error::other(format!("{shown} can't be read; remove {list} from it by hand"))
	};
	let text = match fs::read(path) {
		Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
		Err(error) if error.kind() == ErrorKind::NotFound => return Ok(format!("{shown} was already gone")),
		Err(_) => return Err(unreadable()),
	};
	let config = parse_config(path, &text).map_err(|_| unreadable())?;
	let mut gone: Vec<&str> = Vec::new();
	let mut yours: Vec<&str> = Vec::new();
	for name in &names {
		match config.get(*name) {
			None => {}
			Some(value) if *value == keys[*name] => gone.push(name),
			Some(_) => yours.push(name),
		}
	}
	let left: Map<String, Value> =
		config.iter().filter(|(name, _)| !gone.contains(&name.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
	let edited = if gone.is_empty() { Some(text.clone()) } else { edit_config(path, &text, &left, &[], &gone) };
	let Some(edited) = edited else {
		return Err(Error::other(format!(
			"{shown} can't be edited line by line, so uninstall left it as it is; remove {} from it by hand",
			gone.join(", ")
		)));
	};
	// `{}` is what an emptied JSON file reads as; a YAML one with anything of his left still has lines.
	if created && left.is_empty() && matches!(edited.trim(), "" | "{}") {
		remove_file_forced(path)?;
		return Ok(format!("removed {shown}"));
	}
	if !gone.is_empty() {
		write_atomically(path, &edited, None)?;
	}
	let said = if gone.is_empty() {
		format!("{shown} has none of Jocasta's keys left")
	} else {
		format!("removed {} from {shown}", gone.join(", "))
	};
	Ok(if yours.is_empty() { said } else { format!("{said}; kept {} (you changed it)", yours.join(", ")) })
}

/// A file setup wrote, only while it is still that text (0479).
fn remove_text(path: &Path, expected: &str) -> Result<String> {
	let text = match fs::read(path) {
		Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
		Err(error) if error.kind() == ErrorKind::NotFound => return Ok(format!("{} was already gone", path.display())),
		Err(error) => return Err(error.into()),
	};
	if fingerprint(&text) != expected {
		return Ok(format!("kept {}: you changed it", path.display()));
	}
	remove_file_forced(path)?;
	Ok(format!("removed {}", path.display()))
}

/// What an undo item removes, and how.
fn undo_removal(item: &SetupUndo) -> Result<Removal> {
	let (what, action) = match item {
		SetupUndo::Other(Undo::File { path }) => (path.clone(), Action::Path(PathBuf::from(path))),
		SetupUndo::Other(Undo::Dir { path, file }) => {
			(path.clone(), Action::SetupFolder(PathBuf::from(path), file.clone()))
		}
		SetupUndo::Mcp(mcp) => (
			format!(
				"MCP server{} {} in {} (each while it is still what setup wrote)",
				if mcp.servers.len() > 1 { "s" } else { "" },
				quoted(&mcp.servers),
				mcp.path
			),
			Action::Mcp(mcp.clone()),
		),
		SetupUndo::Other(Undo::HerdrIntegration { path }) => {
			(format!("herdr's omp integration: {path}"), Action::HerdrIntegration(PathBuf::from(path)))
		}
		SetupUndo::Other(Undo::HerdrPlugin { id, .. }) => {
			(format!("the `{id}` herdr plugin (unlinked from herdr)"), Action::HerdrPlugin(id.clone()))
		}
		SetupUndo::Other(Undo::Keys { path, keys, created }) => (
			format!("Jocasta's {} in {path}", keys.keys().map(String::as_str).collect::<Vec<_>>().join(", ")),
			Action::Keys(PathBuf::from(path), keys.clone(), *created),
		),
		SetupUndo::Other(Undo::Text { path, fingerprint }) => (
			format!("{path} (while it is still Jocasta's text)"),
			Action::Text(PathBuf::from(path), fingerprint.clone()),
		),
		SetupUndo::Other(Undo::Credential { target }) => {
			(format!("Windows Credential Manager: {target}"), Action::Credential(target.clone()))
		}
		_ => return Err(Error::other(format!("no undo kind {}.", item.to_json()))),
	};
	Ok(Removal { what, action })
}

/// Jocasta's per-project mise configs (0477), `config.jocasta-<project id>.override.toml`.
fn mise_project_configs(dir: &Path) -> Vec<PathBuf> {
	static NAME: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"^config\.jocasta-.+\.override\.toml$").expect("a fixed pattern"));
	let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
	entries
		.flatten()
		.map(|entry| entry.file_name().to_string_lossy().into_owned())
		.filter(|name| NAME.is_match(name))
		.map(|name| dir.join(name))
		.collect()
}

/// Removes `dir` when it is empty.
fn remove_if_empty(dir: &Path) {
	// Not empty, or already gone.
	let _ = fs::remove_dir(dir);
}

/// The argv of a herdr command.
fn argv(herdr: &Path, rest: &[&str]) -> Vec<String> {
	std::iter::once(herdr.to_string_lossy().into_owned()).chain(rest.iter().map(|word| (*word).to_owned())).collect()
}

/// Unlinks herdr plugin `id`; herdr 0.9.5's replies decide what that said (0558).
async fn unlink_plugin(id: &str, deps: &UninstallDeps) -> Result<String> {
	let Some(herdr) = &deps.herdr else {
		return Ok(format!("herdr isn't installed; the {id} plugin had nothing to unlink"));
	};
	static NOT_RUNNING: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"(?i)not.running|unreachable").expect("a fixed pattern"));
	static NOT_REMOVED: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r#""removed":\s*false"#).expect("a fixed pattern"));
	let unlink = || {
		let words: Vec<String> =
			session_args(&deps.session).into_iter().chain(["plugin", "unlink", id].map(str::to_owned)).collect();
		(deps.run)(argv(herdr, &words.iter().map(String::as_str).collect::<Vec<_>>()))
	};
	let mut out = unlink().await?;
	// Unlinking needs a running server (0558): a headless one for the session, which its own step stops.
	if out.code != 0
		&& NOT_RUNNING.is_match(&out.output)
		&& let Some(start) = &deps.start_herdr
	{
		start().await?;
		out = unlink().await?;
	}
	// herdr 0.9.5 answers an unlink of a plugin it doesn't have with exit 0 and `"removed":false`.
	if out.code == 0 && NOT_REMOVED.is_match(&out.output) {
		return Ok(format!("the {id} herdr plugin was already unlinked"));
	}
	if out.code != 0 {
		return Err(Error::other(format!("herdr plugin unlink {id} exited {}: {}", out.code, out.output.trim())));
	}
	Ok(format!("unlinked the {id} herdr plugin"))
}

/// Does one removal.
async fn perform(action: &Action, deps: &UninstallDeps) -> Result<String> {
	match action {
		Action::Path(path) => remove_path(path),
		Action::SetupFolder(path, file) => remove_setup_folder(path, file.as_deref()),
		Action::Mcp(item) => remove_mcp_servers(item),
		Action::HerdrIntegration(path) => {
			let Some(herdr) = &deps.herdr else { return remove_path(path) };
			let out = (deps.run)(argv(herdr, &["integration", "uninstall", "omp"])).await?;
			if out.code != 0 {
				return Err(Error::other(format!(
					"herdr integration uninstall omp exited {}: {}",
					out.code,
					out.output.trim()
				)));
			}
			Ok("removed herdr's omp integration".to_owned())
		}
		Action::HerdrPlugin(id) => unlink_plugin(id, deps).await,
		Action::HerdrSession => {
			let herdr = deps.herdr.as_deref().unwrap_or(Path::new("herdr"));
			// Jocasta's own session is herdr's default one in Jocasta's data folder: its server stops, and its files go
			// with that folder.
			if deps.session == DEFAULT_SESSION {
				let out = (deps.run)(argv(herdr, &["server", "stop"])).await?;
				return Ok(
					if out.code == 0 { "stopped herdr's server" } else { "herdr's server wasn't running" }.to_owned()
				);
			}
			(deps.run)(argv(herdr, &["session", "stop", &deps.session])).await?;
			let out = (deps.run)(argv(herdr, &["session", "delete", &deps.session])).await?;
			if out.code != 0 {
				return Err(Error::other(format!(
					"herdr session delete {} exited {}: {}",
					deps.session,
					out.code,
					out.output.trim()
				)));
			}
			Ok(format!("removed herdr session `{}`", deps.session))
		}
		Action::Keys(path, keys, created) => remove_keys(path, keys, *created),
		Action::Text(path, expected) => remove_text(path, expected),
		Action::Credential(target) => Ok(if (deps.delete_credential)(target) {
			format!("removed {target}")
		} else {
			format!("{target} was already gone")
		}),
		Action::Autostart(name, launcher) => Ok(if (deps.remove_run_value)(name, launcher)? {
			format!("removed the autostart value {name}")
		} else {
			format!("the autostart value {name} was already gone")
		}),
		Action::PathEntry(entry) => Ok(if (deps.remove_path_entry)(entry)? {
			format!("removed {} from the user PATH", entry.display())
		} else {
			format!("{} was already off the user PATH", entry.display())
		}),
		Action::RcLine(rc, dir) => Ok(if remove_rc_line(rc, dir)? {
			format!("removed the PATH line from {}", rc.display())
		} else {
			format!("{} had no Jocasta PATH line", rc.display())
		}),
		Action::WindowData(path) => remove_recorded_folder(path, "the companion's WebView2 data", holds_window_data),
	}
}

/// What install added, as removals.
fn install_removals(install: &InstallRecord) -> Vec<Removal> {
	let mut removals = Vec::new();
	if let Some(autostart) = &install.autostart {
		let launcher = install.dir.join("app").join("bin").join("launcher.exe");
		removals.push(Removal {
			what: format!("the autostart value {RUN_KEY}\\{autostart}"),
			action: Action::Autostart(autostart.clone(), launcher),
		});
	}
	if let Some(path) = &install.path {
		removals.push(Removal {
			what: format!("the PATH entry {}", path.display()),
			action: Action::PathEntry(path.clone()),
		});
	}
	if let Some(rc) = &install.rc {
		removals.push(Removal {
			what: format!("the PATH line in {}", rc.display()),
			action: Action::RcLine(rc.clone(), install.dir.clone()),
		});
	}
	if let Some(app_data) = &install.app_data {
		removals.push(Removal {
			what: format!("the companion's window data: {}", app_data.display()),
			action: Action::WindowData(app_data.clone()),
		});
	}
	removals
}

/// Everything to remove, in order.
fn plan(paths: &UninstallPaths, deps: &UninstallDeps) -> Result<Vec<Removal>> {
	let is_plugin = |item: &&SetupUndo| matches!(item, SetupUndo::Other(Undo::HerdrPlugin { .. }));
	let mut removals = Vec::new();
	// The plugin is unlinked first, while session `jocasta` can still run the server unlinking needs (0558).
	for item in deps.setup_undo.iter().filter(is_plugin) {
		removals.push(undo_removal(item)?);
	}
	if deps.herdr.is_some() {
		removals.push(Removal {
			what: if deps.session == DEFAULT_SESSION {
				"herdr's server (stopped, its panes closed)".to_owned()
			} else {
				format!("herdr session `{}` (stopped, its panes closed, then deleted)", deps.session)
			},
			action: Action::HerdrSession,
		});
	}
	for item in deps.setup_undo.iter().filter(|item| !is_plugin(item)) {
		removals.push(undo_removal(item)?);
	}
	for file in mise_project_configs(&paths.mise_config_dir) {
		removals.push(Removal {
			what: format!("Jocasta's mise config for a project: {}", file.display()),
			action: Action::Path(file),
		});
	}
	if let Some(install) = &paths.install {
		removals.extend(install_removals(install));
	}
	Ok(removals)
}

/// Removes the install folder's Jocasta files, or has them removed once this process exits.
fn remove_install_dir(install: &InstallRecord, paths: &UninstallPaths, deps: &UninstallDeps) -> Result<String> {
	let dir = &install.dir;
	if !present(dir) {
		return Ok(format!("{} was already gone", dir.display()));
	}
	if let Some(unsafe_reason) = unsafe_folder(dir) {
		return Err(Error::other(format!("kept {}: {unsafe_reason}", dir.display())));
	}
	if !holds_install_marker(dir) {
		return Err(Error::other(format!("kept {}: it no longer holds {INSTALL_MARKER} naming it", dir.display())));
	}
	let contents = install_contents(install, None)?;
	if !paths.running_from_install {
		return remove_install(dir, &contents);
	}
	(deps.remove_after_exit)(dir, &contents.ours)?;
	Ok(if contents.others.is_empty() {
		format!("{} goes when this command exits.", dir.display())
	} else {
		format!(
			"Jocasta's files in {} go when this command exits; the folder stays (it holds other files).",
			dir.display()
		)
	})
}

/// Removes Jocasta's config folder, keeping `own_files` (his) in it.
fn remove_config_folder(paths: &UninstallPaths, own_files: &[PathBuf]) -> Result<String> {
	let dir = &paths.jocasta_config_dir;
	if own_files.is_empty() {
		let said = remove_path(dir)?;
		if let Some(parent) = dir.parent() {
			remove_if_empty(parent);
		}
		return Ok(said);
	}
	let keep: Vec<String> = own_files
		.iter()
		.filter_map(|file| file.file_name())
		.map(|name| name.to_string_lossy().to_lowercase())
		.collect();
	for entry in fs::read_dir(dir)? {
		let name = entry?.file_name();
		if !keep.contains(&name.to_string_lossy().to_lowercase()) {
			remove_path(&dir.join(name))?;
		}
	}
	let kept = own_files.iter().map(|file| file.display().to_string()).collect::<Vec<_>>().join(", ");
	Ok(format!("removed {} except {kept}", dir.display()))
}

/// Removes the data folder, and `%LOCALAPPDATA%\jocasta` with it when it held only that.
fn remove_data_folder(data_dir: &Path) -> Result<String> {
	let said = remove_path(data_dir)?;
	if let Some(parent) = data_dir.parent()
		&& parent.file_name().is_some_and(|name| name.to_string_lossy().to_lowercase() == "jocasta")
	{
		remove_if_empty(parent);
	}
	Ok(said)
}

/// Shows what goes, asks, and removes it; the data folder is asked about separately.
pub async fn run_uninstall(paths: &UninstallPaths, deps: &UninstallDeps, io: &dyn SetupIo) -> Result<i32> {
	if (deps.service_state)().await? != ServiceState::Down {
		return Err(Error::user_with_code(
			"Jocasta is running: close it (quit herdr's client in the `jocasta` terminal), then run `jocasta uninstall` again.",
			1,
		));
	}
	// Open omp sessions run `jocasta.exe mcp` and whiteboard's node.exe from the install: its files can't go (0579).
	if let Some(install) = &paths.install {
		let running = deps.processes_under.as_ref().map(|list| list(&install.dir)).unwrap_or_default();
		if !running.is_empty() {
			let dir = install.dir.to_string_lossy();
			let named: Vec<String> = running.iter().map(|process| describe_process(&dir, process)).collect();
			return Err(Error::user_with_code(
				format!(
					"omp sessions still run programs from {dir}: {}. Close them, then run jocasta uninstall again.",
					named.join(", ")
				),
				1,
			));
		}
	}
	let removals = plan(paths, deps)?;
	io.print("jocasta uninstall will remove what install and setup added:");
	for removal in &removals {
		io.print(&format!("- {}", removal.what));
	}
	io.print(&format!("- Jocasta's config folder: {}", paths.jocasta_config_dir.display()));
	let own_files: Vec<PathBuf> =
		HIS_CONFIG_FILES.iter().map(|name| paths.jocasta_config_dir.join(name)).filter(|path| path.exists()).collect();
	for file in &own_files {
		io.print(&format!("  (kept: {}, yours, not Jocasta's)", file.display()));
	}
	if let Some(install) = &paths.install {
		io.print(&format!("- the install: {}", install.dir.display()));
	}
	io.print("Keys setup merged into omp's config.yml stay: they are your settings.");
	io.print("");
	if !answered_yes(io.ask("Remove these? [y/N] ").await.as_deref()) {
		io.print("Nothing removed.");
		return Ok(0);
	}
	let data = paths.data_dir.exists()
		&& answered_yes(
			io.ask(&format!(
				"Also remove Jocasta's data folder {} (history, models, logs)? [y/N] ",
				paths.data_dir.display()
			))
			.await
			.as_deref(),
		);
	let mut failed = 0;
	let mut attempt = |what: &str, result: Result<String>| match result {
		Ok(said) => io.print(&said),
		Err(error) => {
			failed += 1;
			io.print(&format!("couldn't remove {what}: {error}"));
		}
	};
	for removal in &removals {
		attempt(&removal.what, perform(&removal.action, deps).await);
	}
	if data {
		attempt(&paths.data_dir.to_string_lossy(), remove_data_folder(&paths.data_dir));
	}
	attempt(&paths.jocasta_config_dir.to_string_lossy(), remove_config_folder(paths, &own_files));
	if let Some(install) = &paths.install {
		attempt(&install.dir.to_string_lossy(), remove_install_dir(install, paths, deps));
	}
	if failed > 0 {
		io.print(&format!(
			"Done, except {failed} thing{} above; remove {} by hand.",
			if failed > 1 { "s" } else { "" },
			if failed > 1 { "them" } else { "it" }
		));
		return Ok(1);
	}
	io.print("Jocasta is uninstalled.");
	Ok(0)
}

/// The user PATH without `entry` (as stored, `%VARS%` kept); open programs are told the environment changed.
pub fn remove_user_path_entry(entry: &Path) -> Result<bool> {
	if !cfg!(windows) {
		return Ok(false);
	}
	let script = "
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
try {
	$raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
	$entries = @($raw -split ';' | Where-Object { $_ })
	$kept = @($entries | Where-Object { $_.TrimEnd('\\') -ne $env:JOCASTA_PATH_ENTRY.TrimEnd('\\') })
	if ($kept.Count -eq $entries.Count) { exit 3 }
	$key.SetValue('Path', ($kept -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
} finally { $key.Close() }
Add-Type -Namespace JocastaUninstall -Name Native -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);'
$result = [UIntPtr]::Zero
[void][JocastaUninstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
";
	let mut command = powershell(script);
	command.env("JOCASTA_PATH_ENTRY", entry);
	let output = command.output()?;
	if output.status.code() == Some(3) {
		return Ok(false);
	}
	if !output.status.success() {
		let said = String::from_utf8_lossy(if output.stderr.is_empty() { &output.stdout } else { &output.stderr });
		return Err(Error::other(format!("the user PATH couldn't be changed: {}", said.trim())));
	}
	Ok(true)
}

/// Removes Run value `name` when its data starts `launcher` (another program's value of that name stays).
pub fn remove_run_value_for(name: &str, launcher: &Path) -> Result<bool> {
	if read_run_value(name).as_deref() != Some(&format!("\"{}\"", launcher.display())) {
		return Ok(false);
	}
	if !delete_run_value(name) {
		return Err(Error::other(format!("reg delete {RUN_KEY}\\{name} failed")));
	}
	Ok(true)
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
	const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
	for chunk in bytes.chunks(3) {
		let word = (u32::from(chunk[0]) << 16)
			| (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
			| u32::from(*chunk.get(2).unwrap_or(&0));
		for (at, shift) in [18, 12, 6, 0].into_iter().enumerate() {
			if at <= chunk.len() {
				out.push(char::from(DIGITS[((word >> shift) & 63) as usize]));
			} else {
				out.push('=');
			}
		}
	}
	out
}

/// Removes `entries` of the install folder `dir` once this process has exited (Windows holds the running jocasta.exe):
/// a hidden PowerShell of its own console, so closing this terminal doesn't end it, tries every few seconds for five
/// minutes, as omp sessions closing free their programs (0579); `install.json` goes, then the folder if empty, only once
/// every entry is gone. .NET's recursive delete never follows a junction. The names reach it through the environment,
/// never its command line. Fails when the remover didn't start. `pid` defaults to this process.
pub fn remove_after_exit(dir: &Path, entries: &[String], pid: Option<u32>) -> Result<()> {
	let script = "
$dir = $env:JOCASTA_UNINSTALL_DIR
$entries = @($env:JOCASTA_UNINSTALL_ENTRIES -split \"`n\" | Where-Object { $_ })
Wait-Process -Id ([int]$env:JOCASTA_UNINSTALL_PID) -Timeout 30 -ErrorAction SilentlyContinue
for ($round = 0; $round -lt 60; $round++) {
	$left = 0
	foreach ($name in $entries) {
		$path = Join-Path $dir $name
		try {
			if ([IO.Directory]::Exists($path)) { [IO.Directory]::Delete($path, $true) }
			elseif ([IO.File]::Exists($path)) { [IO.File]::Delete($path) }
		} catch { $left++ }
	}
	if ($left -eq 0) { break }
	Start-Sleep -Seconds 5
}
if ($left -eq 0) {
	[IO.File]::Delete((Join-Path $dir '@MARKER@'))
	try { [IO.Directory]::Delete($dir) } catch {}
}
"
	.replace("@MARKER@", INSTALL_MARKER);
	// Started by a short-lived PowerShell's Start-Process: a detached console program gets no console and Windows
	// PowerShell then never runs. The script goes as -EncodedCommand (UTF-16LE, base64), its quotes untouched.
	let start = "Start-Process powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand',$env:JOCASTA_UNINSTALL_SCRIPT";
	let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
	let mut command = powershell(start);
	command
		.env("JOCASTA_UNINSTALL_SCRIPT", base64(&utf16))
		.env("JOCASTA_UNINSTALL_DIR", dir)
		.env("JOCASTA_UNINSTALL_ENTRIES", entries.join("\n"))
		.env("JOCASTA_UNINSTALL_PID", pid.unwrap_or_else(std::process::id).to_string());
	let output = command.output().map_err(|error| Error::other(format!("its remover didn't start ({error})")))?;
	if !output.status.success() {
		let said = String::from_utf8_lossy(&output.stderr);
		return Err(Error::other(format!("its remover didn't start ({})", said.trim())));
	}
	Ok(())
}

#[cfg(test)]
#[path = "uninstall_tests.rs"]
mod tests;
