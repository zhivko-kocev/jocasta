//! Every benchmark writes `<out>/<benchmark>.json` (the summary with the run's conditions, 0482 / [S16]) and, where it
//! has them, raw rows in `<out>/<benchmark>.jsonl`. No host or user names: the conditions are the OS, CPU, memory,
//! load and the pinned tools' versions.
//!
//! The files keep the bytes the TypeScript's `JSON.stringify` wrote: [`json_text`] prints a number as JavaScript does
//! (`25`, not `25.0`; `1e-7`; NaN as `null`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, Timelike, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::lib::args::BenchUsageError;
use jocasta::frontier::protocol::js_number_text;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conditions {
	pub started_at: String,
	/// Local hour 0–23: the design interleaves runs over times of day ([S16]).
	pub local_hour: u32,
	pub platform: String,
	pub os_release: String,
	pub arch: String,
	pub cpu: String,
	pub cpu_count: usize,
	pub memory_gb: f64,
	pub versions: Map<String, Value>,
	/// SHA-256 of the `jocasta` on PATH, when the tools include it (0576): with `versions.jocasta` (`jocasta
	/// --version`) it ties each row to the build it measured (0533); `Some(None)` (null) when it isn't on PATH.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub jocasta_sha256: Option<Option<String>>,
	/// Set when the run ends; last, where the TypeScript's `{ ...started, finishedAt }` put it.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub finished_at: Option<String>,
}

impl Conditions {
	/// These conditions ended now.
	pub fn finished(self) -> Self {
		Self { finished_at: Some(iso_now()), ..self }
	}
}

/// `new Date().toISOString()`.
pub fn iso_now() -> String {
	Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// `<launcher> --version`'s first line: the eval runner's harness version and every benchmark's tool versions (EVL-14:
/// one reader). A failure or an empty first line fails, naming the launcher.
pub fn read_version(launcher: &[impl AsRef<str>]) -> Result<String> {
	let words: Vec<&str> = launcher.iter().map(AsRef::as_ref).collect();
	let failed = |why: String| Error::other(format!("{} --version failed: {why}", words.join(" ")));
	let mut argv: Vec<&str> = words.clone();
	argv.push("--version");
	let options = RunOptions { timeout_ms: Some(30_000), ..RunOptions::default() };
	let result = run_program_sync(&argv, options).map_err(|error| failed(error.to_string()))?;
	let version = result.stdout.trim().lines().next().unwrap_or("");
	if result.code != Some(0) || version.is_empty() {
		return Err(failed(result.stderr));
	}
	Ok(version.to_owned())
}

/// `<command> --version`'s first line, or `None` when it isn't on PATH or fails. Never starts a session.
pub fn tool_version(command: &str) -> Option<String> {
	let found = on_path(command, None)?;
	read_version(&[found.to_string_lossy().as_ref()]).ok()
}

/// SHA-256 of the program `command` runs on PATH, or `None`.
pub fn program_sha256(command: &str) -> Option<String> {
	let found = on_path(command, None)?;
	let digest = Sha256::digest(fs::read(found).ok()?);
	Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn conditions(tools: &[impl AsRef<str>]) -> Conditions {
	let now = Local::now();
	let versions = tools
		.iter()
		.map(|tool| (tool.as_ref().to_owned(), tool_version(tool.as_ref()).map_or(Value::Null, Value::String)))
		.collect();
	Conditions {
		started_at: iso_now(),
		local_hour: now.hour(),
		platform: host::platform().to_owned(),
		os_release: host::release(),
		arch: host::arch().to_owned(),
		cpu: host::cpu_model().unwrap_or_else(|| "unknown".to_owned()),
		cpu_count: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
		memory_gb: host::memory_gb(),
		versions,
		jocasta_sha256: tools.iter().any(|tool| tool.as_ref() == "jocasta").then(|| program_sha256("jocasta")),
		finished_at: None,
	}
}

/// What `node:os` tells about the host.
mod host {
	use jocasta::shared::pollers::machine::total_memory_bytes;
	use jocasta::shared::process::{RunOptions, run_program_sync};

	/// `os.platform()`.
	pub(super) fn platform() -> &'static str {
		match std::env::consts::OS {
			"windows" => "win32",
			"macos" => "darwin",
			other => other,
		}
	}

	/// `os.arch()`.
	pub(super) fn arch() -> &'static str {
		match std::env::consts::ARCH {
			"x86_64" => "x64",
			"aarch64" => "arm64",
			"x86" => "ia32",
			other => other,
		}
	}

	/// The output of `program`, trimmed; `None` when it fails.
	fn output(program: &str, args: &[&str]) -> Option<String> {
		let mut argv = vec![program];
		argv.extend_from_slice(args);
		let result = run_program_sync(&argv, RunOptions::default()).ok()?;
		(result.code == Some(0)).then(|| result.stdout.trim().to_owned())
	}

	/// `os.release()`: Windows' `major.minor.build`, the kernel's release elsewhere.
	pub(super) fn release() -> String {
		if cfg!(windows) {
			let ver = output("cmd.exe", &["/d", "/c", "ver"]).unwrap_or_default();
			let version = ver.split("Version ").nth(1).unwrap_or("");
			return version.split('.').take(3).collect::<Vec<_>>().join(".").trim_end_matches(']').to_owned();
		}
		output("uname", &["-r"]).unwrap_or_default()
	}

	/// `os.cpus()[0].model`.
	pub(super) fn cpu_model() -> Option<String> {
		if cfg!(windows) {
			let query = ["query", r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0", "/v", "ProcessorNameString"];
			let text = output("reg.exe", &query)?;
			return text.split("REG_SZ").nth(1).map(|name| name.trim().to_owned());
		}
		if cfg!(target_os = "macos") {
			return output("sysctl", &["-n", "machdep.cpu.brand_string"]);
		}
		let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
		info.lines().find_map(|line| line.strip_prefix("model name")?.split_once(':').map(|(_, m)| m.trim().to_owned()))
	}

	/// `Math.round(os.totalmem() / 2 ** 30 * 10) / 10`.
	pub(super) fn memory_gb() -> f64 {
		let bytes = total_memory_bytes().unwrap_or(0) as f64;
		((bytes / (1u64 << 30) as f64) * 10.0 + 0.5).floor() / 10.0
	}
}

fn write_value(out: &mut String, value: &Value, depth: usize, pretty: bool) {
	let newline = |out: &mut String, depth: usize| {
		if pretty {
			out.push('\n');
			out.push_str(&"  ".repeat(depth));
		}
	};
	match value {
		Value::Number(number) if number.is_f64() => out.push_str(&js_number_text(number.as_f64().unwrap_or(f64::NAN))),
		Value::Array(items) if !items.is_empty() => {
			out.push('[');
			for (i, item) in items.iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				newline(out, depth + 1);
				write_value(out, item, depth + 1, pretty);
			}
			newline(out, depth);
			out.push(']');
		}
		Value::Object(map) if !map.is_empty() => {
			out.push('{');
			for (i, (key, item)) in map.iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				newline(out, depth + 1);
				out.push_str(&Value::String(key.clone()).to_string());
				out.push_str(if pretty { ": " } else { ":" });
				write_value(out, item, depth + 1, pretty);
			}
			newline(out, depth);
			out.push('}');
		}
		other => out.push_str(&other.to_string()),
	}
}

/// `JSON.stringify(value, null, 2)` when `pretty`, else `JSON.stringify(value)`.
pub fn json_text(value: &Value, pretty: bool) -> String {
	let mut out = String::new();
	write_value(&mut out, value, 0, pretty);
	out
}

fn to_value(data: &impl Serialize) -> Result<Value> {
	Ok(serde_json::to_value(data)?)
}

/// Writes `<out>/<name>.json`, pretty, creating `out`.
pub fn write_json(out: &Path, name: &str, data: &impl Serialize) -> Result<PathBuf> {
	fs::create_dir_all(out)?;
	let file = out.join(format!("{name}.json"));
	fs::write(&file, format!("{}\n", json_text(&to_value(data)?, true)))?;
	Ok(file)
}

/// Refuses a run whose `<out>/<name>.jsonl` exists (as the H1 scripts do): a rerun in the same folder would mix earlier
/// rows into the raw file while `<name>.json` summarizes only the new run. Called once at a script's start.
pub fn claim_rows(out: &Path, names: &[&str]) -> Result<()> {
	for name in names {
		let file = out.join(format!("{name}.jsonl"));
		if file.exists() {
			return Err(BenchUsageError::err(format!("{} exists; results are never overwritten", file.display())));
		}
	}
	Ok(())
}

/// Appends one row to `<out>/<name>.jsonl`.
pub fn append_row(out: &Path, name: &str, row: &impl Serialize) -> Result<()> {
	fs::create_dir_all(out)?;
	let line = format!("{}\n", json_text(&to_value(row)?, false));
	fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(out.join(format!("{name}.jsonl")))?
		.write_all(line.as_bytes())?;
	Ok(())
}

/// Writes `<out>/<name>.jsonl` whole: rows derived again from the run's own files each time (tokscale parity).
pub fn write_rows(out: &Path, name: &str, rows: &[impl Serialize]) -> Result<()> {
	fs::create_dir_all(out)?;
	let mut text = String::new();
	for row in rows {
		text.push_str(&json_text(&to_value(row)?, false));
		text.push('\n');
	}
	fs::write(out.join(format!("{name}.jsonl")), text)?;
	Ok(())
}

/// A dry run's plan: the benchmark, what each run measures, and where results would go.
pub fn print_plan(benchmark: &str, lines: &[impl AsRef<str>], out: Option<&Path>) {
	println!("{benchmark} (0482) dry run: nothing is started, killed or measured.");
	for line in lines {
		println!("  - {}", line.as_ref());
	}
	println!("  results: {}", out.map_or_else(|| "<--out dir>".to_owned(), |out| out.display().to_string()));
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
