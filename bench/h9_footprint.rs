//! H9 (0482): install and uninstall footprint on Windows, in 0481's VM.
//!
//!   jocasta-bench h9-footprint --out <dir> --install "<command>" --uninstall "<command>" [--between "<command>"]
//!                              [--roots <dir,…>] [--runs 1] [--dry-run]
//!
//! Snapshots the file system under `--roots` (default: %LOCALAPPDATA%, %APPDATA%, %USERPROFILE%\.jocasta,
//! %USERPROFILE%\.omp, %USERPROFILE%\.herdr, %USERPROFILE%\.local, %USERPROFILE%\.config (setup's mise files, 0468),
//! %ProgramData%) and the registry (`reg export` of HKCU
//! and HKLM\SOFTWARE), runs `--install` (timed), snapshots again, runs `--between` when given (setup and use: 0481's
//! run does setup, a first run and a thread here) and snapshots again, runs `--uninstall` (timed) and snapshots a last
//! time. Reports what install added, what setup and use added, and what uninstall left behind (files and registry
//! values), all of it and the part naming jocasta, herdr, omp or mise. Prerequisites and minutes to the first task are
//! noted by hand in the run's notes (README). h9-footprint.json's conditions carry the installed jocasta.exe's
//! `--version` line and `jocastaSha256`, read after the first install (0576). Writes h9-footprint.json,
//! h9-install-added.jsonl, h9-use-added.jsonl and
//! h9-uninstall-left.jsonl. It refuses to run anywhere but the VM, before anything runs: it runs the given uninstall
//! and writes the machine's registry values and paths into its rows.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::lib::args::{ArgSpec, BenchArgs, BenchUsageError, flag, list_flag, parse_bench_args, run_script};
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, program_sha256, tool_version, write_json};
use crate::lib::sandbox::{machine_name, require_vm};
use crate::lib::temp::mkdtemp;
use jocasta::shared::env;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::{normalize, remove_tree};
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};

/// What a snapshot holds: file paths with size, and registry lines `key\value=data`, each in the order it was found.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
	pub files: Vec<(String, i64)>,
	pub registry: Vec<String>,
}

impl Snapshot {
	fn file_names(&self) -> Vec<String> {
		self.files.iter().map(|(path, _)| path.clone()).collect()
	}

	fn size_of(&self, path: &str) -> Option<i64> {
		self.files.iter().find(|(known, _)| known == path).map(|(_, size)| *size)
	}
}

/// Parses `reg export`'s text (UTF-16LE with a BOM on disk) into `[key]\name=data` lines; a key alone is `[key]`.
pub fn parse_reg_export(text: &str) -> Vec<String> {
	let mut lines: Vec<String> = Vec::new();
	let mut seen: HashSet<String> = HashSet::new();
	let mut add = |line: String| {
		if seen.insert(line.clone()) {
			lines.push(line);
		}
	};
	let mut key = String::new();
	let mut pending = String::new();
	let text = text.strip_prefix('\u{feff}').unwrap_or(text);
	for raw in text.split('\n') {
		let raw = raw.strip_suffix('\r').unwrap_or(raw);
		// A value continues on the next line after a trailing backslash.
		let line = format!("{pending}{}", raw.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'));
		if let Some(continued) = line.strip_suffix('\\') {
			pending = continued.to_owned();
			continue;
		}
		pending.clear();
		if line.is_empty() || line.starts_with("Windows Registry Editor") {
			continue;
		}
		if line.starts_with('[') && line.ends_with(']') {
			key = line;
			add(key.clone());
			continue;
		}
		add(format!("{key}\\{line}"));
	}
	lines
}

/// Added and removed entries between two lists, each once, in the order they appear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff<T> {
	pub added: Vec<T>,
	pub removed: Vec<T>,
}

pub fn diff_sets<T: Eq + Hash + Clone>(before: &[T], after: &[T]) -> Diff<T> {
	let a: HashSet<&T> = before.iter().collect();
	let b: HashSet<&T> = after.iter().collect();
	let unique = |items: &[T], not_in: &HashSet<&T>| {
		let mut seen: HashSet<&T> = HashSet::new();
		items.iter().filter(|item| !not_in.contains(item) && seen.insert(item)).cloned().collect()
	};
	Diff { added: unique(after, &a), removed: unique(before, &b) }
}

static NAMED: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(?i)jocasta|jarvis|herdr|\bomp\b|oh-my-pi|\bmise\b").expect("a fixed pattern"));

pub fn named(entries: &[String]) -> Vec<String> {
	entries.iter().filter(|entry| NAMED.is_match(entry)).cloned().collect()
}

fn walk(root: &Path, files: &mut Vec<(String, i64)>) {
	let Ok(entries) = fs::read_dir(root) else { return };
	for entry in entries.flatten() {
		let path = normalize(&root.join(entry.file_name()));
		let text = path.to_string_lossy().into_owned();
		let Ok(kind) = entry.file_type() else { continue };
		if kind.is_symlink() {
			files.push((format!("{text} ->"), 0));
		} else if kind.is_dir() {
			files.push((format!("{text}\\"), 0));
			walk(&path, files);
		} else {
			files.push((text, fs::metadata(&path).map_or(-1, |meta| i64::try_from(meta.len()).unwrap_or(i64::MAX))));
		}
	}
}

/// A UTF-16LE file as `readFileSync(file, "utf16le")` reads it.
fn read_utf16le(file: &Path) -> Result<String> {
	let bytes = fs::read(file)?;
	let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
	Ok(String::from_utf16_lossy(&units))
}

fn snapshot(roots: &[String]) -> Result<Snapshot> {
	let mut files: Vec<(String, i64)> = Vec::new();
	for root in roots {
		let root = Path::new(root);
		if root.exists() {
			walk(&normalize(root), &mut files);
		}
	}
	let mut registry: Vec<String> = Vec::new();
	let mut seen: HashSet<String> = HashSet::new();
	let dir = mkdtemp("h9-reg-")?;
	let exported: Result<()> = (|| {
		for (i, hive) in ["HKCU", "HKLM\\SOFTWARE"].iter().enumerate() {
			let file = dir.join(format!("{i}.reg"));
			let result = run_program_sync(
				&[
					"reg".to_owned(),
					"export".to_owned(),
					(*hive).to_owned(),
					file.to_string_lossy().into_owned(),
					"/y".to_owned(),
				],
				RunOptions::default(),
			)?;
			if result.code != Some(0) {
				return Err(Error::other(format!("reg export {hive} failed: {}", result.stderr)));
			}
			for line in parse_reg_export(&read_utf16le(&file)?) {
				if seen.insert(line.clone()) {
					registry.push(line);
				}
			}
		}
		Ok(())
	})();
	let _ = remove_tree(&dir);
	exported?;
	Ok(Snapshot { files, registry })
}

/// A command run through the shell with this process's streams: its exit code (`None` when a signal ended it) and
/// seconds.
fn timed(command: &str) -> Result<(Option<i32>, f64)> {
	let started = Instant::now();
	#[cfg(windows)]
	let mut shell = {
		use std::os::windows::process::CommandExt;
		let program = env::var("ComSpec").unwrap_or_else(|| "cmd.exe".to_owned());
		let mut shell = Command::new(program);
		shell.raw_arg(format!("/d /s /c \"{command}\""));
		shell
	};
	#[cfg(not(windows))]
	let mut shell = {
		let mut shell = Command::new("/bin/sh");
		shell.arg("-c").arg(command);
		shell
	};
	env::apply_to(&mut shell);
	let status = shell.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).status()?;
	Ok((status.code(), started.elapsed().as_secs_f64()))
}

fn timing(result: (Option<i32>, f64)) -> Value {
	json!({ "exitCode": result.0, "seconds": result.1 })
}

/// The `jocasta.exe` an install put in place (0576 BEN-22): the one on PATH, else the one in the installer's folder
/// (`JOCASTA_INSTALL_DIR`, else `%LOCALAPPDATA%\Programs\jocasta`, as install.ps1 picks it), since this process's PATH
/// predates the user PATH entry the install adds. `None` when there is neither.
pub fn installed_jocasta(vars: &HashMap<String, String>) -> Option<PathBuf> {
	if let Some(found) =
		on_path("jocasta", Some(&HashMap::from([("PATH".to_owned(), vars.get("PATH").cloned().unwrap_or_default())])))
	{
		return Some(found);
	}
	let get = |name: &str| vars.get(name).cloned();
	let local = get("LOCALAPPDATA").map_or_else(
		|| normalize(&PathBuf::from(get("USERPROFILE").unwrap_or_default()).join("AppData").join("Local")),
		PathBuf::from,
	);
	let folder =
		get("JOCASTA_INSTALL_DIR").map_or_else(|| normalize(&local.join("Programs").join("jocasta")), PathBuf::from);
	let exe = normalize(&folder.join("jocasta.exe"));
	exe.exists().then_some(exe)
}

fn process_vars() -> HashMap<String, String> {
	env::vars()
		.into_iter()
		.filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
		.collect()
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(async { run(argv) }).await
}

/// `{ files: …, registry: … }` of what a later snapshot has that an earlier one lacked.
fn grown(before: &Snapshot, after: &Snapshot) -> (Vec<String>, Vec<String>) {
	(diff_sets(&before.file_names(), &after.file_names()).added, diff_sets(&before.registry, &after.registry).added)
}

/// The default `--roots`: the folders an install and its setup write to.
fn default_roots(vars: &HashMap<String, String>) -> Vec<String> {
	let var = |name: &str| vars.get(name).cloned();
	let profile = PathBuf::from(var("USERPROFILE").unwrap_or_default());
	let joined = |parts: &[&str]| {
		let mut path = profile.clone();
		for part in parts {
			path.push(part);
		}
		normalize(&path).to_string_lossy().into_owned()
	};
	vec![
		var("LOCALAPPDATA").unwrap_or_else(|| joined(&["AppData", "Local"])),
		var("APPDATA").unwrap_or_else(|| joined(&["AppData", "Roaming"])),
		joined(&[".jocasta"]),
		joined(&[".omp"]),
		joined(&[".herdr"]),
		joined(&[".local"]),
		joined(&[".config"]),
		var("ProgramData").unwrap_or_else(|| "C:\\ProgramData".to_owned()),
	]
}

/// The commands the script runs: each one as given, `between` only when it is not empty.
struct Commands<'a> {
	install: Option<&'a str>,
	uninstall: Option<&'a str>,
	between: Option<&'a str>,
}

/// The `--dry-run` plan.
fn print_h9_plan(args: &BenchArgs, roots: &[String], commands: &Commands) {
	let mut lines = vec![
		format!("snapshot files under {} and the registry (reg export HKCU, HKLM\\SOFTWARE)", roots.join(", ")),
		format!(
			"run and time: {}; snapshot; record the installed jocasta.exe's --version and SHA-256",
			commands.install.unwrap_or("<--install>")
		),
	];
	if let Some(between) = commands.between {
		lines.push(format!("run and time: {between}; snapshot"));
	}
	lines.push(format!("run and time: {}; snapshot", commands.uninstall.unwrap_or("<--uninstall>")));
	lines.push(
		"report added by install, added by setup and use, and left after uninstall, all and those naming jocasta, herdr, omp or mise"
			.to_owned(),
	);
	lines.push("Windows only, in 0481's VM".to_owned());
	print_plan("H9 install footprint", &lines, args.out.as_deref());
}

/// Appends the entries `grown` found to the run's rows of `name`.
fn append_entries(out: &Path, name: &str, run: u64, (files, registry): &(Vec<String>, Vec<String>)) -> Result<()> {
	for file in files {
		append_row(out, name, &json!({ "run": run, "kind": "file", "entry": file }))?;
	}
	for value in registry {
		append_row(out, name, &json!({ "run": run, "kind": "registry", "entry": value }))?;
	}
	Ok(())
}

/// The megabytes of `paths` in the snapshot `at`.
fn megabytes(paths: &[String], at: &Snapshot) -> f64 {
	paths.iter().map(|path| at.size_of(path).unwrap_or(0).max(0) as f64).sum::<f64>() / 1_048_576.0
}

/// One install, optional setup and use, and uninstall, with a snapshot around each: its entry of h9-footprint.json.
/// `build` is read once, after the first install, before its uninstall removes it (0576 BEN-22).
fn one_run(
	run: u64,
	roots: &[String],
	commands: &Commands,
	out: &Path,
	build: &mut Option<(Option<String>, Option<String>)>,
) -> Result<Value> {
	let (Some(install), Some(uninstall)) = (commands.install, commands.uninstall) else {
		return Err(BenchUsageError::err("--install and --uninstall are required"));
	};
	let before = snapshot(roots)?;
	let installed = timed(install)?;
	let after_install = snapshot(roots)?;
	if build.is_none() {
		*build = Some(match installed_jocasta(&process_vars()) {
			Some(exe) => {
				let exe = exe.to_string_lossy().into_owned();
				(tool_version(&exe), program_sha256(&exe))
			}
			None => (None, None),
		});
	}
	let used = commands.between.map(timed).transpose()?;
	let after_use = if commands.between.is_some() { snapshot(roots)? } else { after_install.clone() };
	let removed = timed(uninstall)?;
	let after_uninstall = snapshot(roots)?;
	let added = grown(&before, &after_install);
	let use_added = grown(&after_install, &after_use);
	let left = grown(&before, &after_uninstall);
	append_entries(out, "h9-install-added", run, &added)?;
	append_entries(out, "h9-use-added", run, &use_added)?;
	append_entries(out, "h9-uninstall-left", run, &left)?;
	let mut entry = Map::new();
	entry.insert("run".to_owned(), json!(run));
	entry.insert("install".to_owned(), timing(installed));
	if let Some(used) = used {
		entry.insert("use".to_owned(), timing(used));
	}
	entry.insert("uninstall".to_owned(), timing(removed));
	entry.insert(
		"added".to_owned(),
		json!({ "files": added.0.len(), "registry": added.1.len(), "megabytes": megabytes(&added.0, &after_install) }),
	);
	if commands.between.is_some() {
		entry.insert(
			"useAdded".to_owned(),
			json!({
				"files": use_added.0.len(),
				"registry": use_added.1.len(),
				"megabytes": megabytes(&use_added.0, &after_use),
				"namedFiles": named(&use_added.0),
				"namedRegistry": named(&use_added.1),
			}),
		);
	}
	entry.insert(
		"left".to_owned(),
		json!({
			"files": left.0.len(),
			"registry": left.1.len(),
			"megabytes": megabytes(&left.0, &after_uninstall),
			"namedFiles": named(&left.0),
			"namedRegistry": named(&left.1),
		}),
	);
	Ok(Value::Object(entry))
}

fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec {
		default_runs: 1,
		valued: &["--install", "--uninstall", "--between", "--roots"],
		..ArgSpec::default()
	};
	let args = parse_bench_args(&argv, &spec)?;
	let defaults = default_roots(&process_vars());
	let default_refs: Vec<&str> = defaults.iter().map(String::as_str).collect();
	let roots = list_flag(&args, "roots", &default_refs);
	let commands = Commands {
		install: flag(&args, "install").filter(|text| !text.is_empty()),
		uninstall: flag(&args, "uninstall").filter(|text| !text.is_empty()),
		between: flag(&args, "between").filter(|text| !text.is_empty()),
	};
	if args.dry_run {
		print_h9_plan(&args, &roots, &commands);
		return Ok(());
	}
	if !cfg!(windows) {
		return Err(BenchUsageError::err("H9 runs on Windows (0481's VM)"));
	}
	require_vm(&machine_name(), "install and uninstall Jocasta")?;
	if commands.install.is_none() || commands.uninstall.is_none() {
		return Err(BenchUsageError::err("--install and --uninstall are required"));
	}
	let out = args.out.clone().expect("a real run has --out");
	claim_rows(&out, &["h9-install-added", "h9-use-added", "h9-uninstall-left"])?;
	let mut started = conditions(&["git"]);
	let mut build: Option<(Option<String>, Option<String>)> = None;
	let mut runs: Vec<Value> = Vec::new();
	for run in 0..args.runs {
		runs.push(one_run(run, &roots, &commands, &out, &mut build)?);
	}
	let (version, sha256) = build.unwrap_or((None, None));
	started.versions.insert("jocasta".to_owned(), version.map_or(Value::Null, Value::String));
	started.jocasta_sha256 = Some(sha256);
	let file = write_json(
		&out,
		"h9-footprint",
		&json!({
			"benchmark": "H9",
			"conditions": started.finished(),
			"roots": roots,
			"runs": runs,
		}),
	)?;
	println!("wrote {}", file.display());
	Ok(())
}

#[cfg(test)]
#[path = "h9_footprint_tests.rs"]
mod tests;
