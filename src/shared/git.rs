use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use crate::shared::errors::{Error, Result};
use crate::shared::process::{RunOptions, run_program, run_program_sync};

/// How long one synchronous git call may take before it is killed (CO-10).
const GIT_TIMEOUT_MS: i64 = 30_000;

/// What a git call may add: the whole environment it runs with (`GIT_INDEX_FILE`) and what it reads on stdin.
#[derive(Default)]
pub struct GitOptions {
	pub env: Option<HashMap<String, String>>,
	pub stdin: Option<Vec<u8>>,
}

/// What a git call left: its exit code (`None` when it was ended) and both streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
	pub code: Option<i32>,
	pub stdout: String,
	pub stderr: String,
}

fn argv(cwd: &Path, args: &[impl AsRef<OsStr>]) -> Vec<std::ffi::OsString> {
	let mut argv: Vec<std::ffi::OsString> = vec!["git".into(), "-C".into(), cwd.into()];
	argv.extend(args.iter().map(|arg| arg.as_ref().to_owned()));
	argv
}

/// Runs git through its CLI (0103) in `cwd` without holding the event loop (hooks, signing and long logs take their
/// time, CO-10). Past `timeout_ms` git and everything it started are ended, which reads as `code: None`.
pub async fn git_async(
	cwd: &Path,
	args: &[impl AsRef<OsStr>],
	timeout_ms: i64,
	options: GitOptions,
) -> Result<GitOutput> {
	let run = run_program(
		&argv(cwd, args),
		RunOptions { env: options.env, stdin: options.stdin, timeout_ms: Some(timeout_ms), ..RunOptions::default() },
	)
	.await?;
	Ok(GitOutput { code: run.code, stdout: run.stdout, stderr: run.stderr })
}

/// Runs git through its CLI (0103) in `dir` synchronously. Returns trimmed stdout, or `None` when git exits non-zero;
/// fails when git could not run, was killed or ran past 30 s.
pub fn git(dir: &Path, args: &[impl AsRef<OsStr>]) -> Result<Option<String>> {
	let result =
		run_program_sync(&argv(dir, args), RunOptions { timeout_ms: Some(GIT_TIMEOUT_MS), ..RunOptions::default() })?;
	let Some(code) = result.code else {
		let how = if result.timed_out {
			format!("after {} s", GIT_TIMEOUT_MS / 1000)
		} else {
			result.signal.unwrap_or_else(|| "null".to_owned())
		};
		let shown: Vec<_> = args.iter().map(|arg| arg.as_ref().to_string_lossy().into_owned()).collect();
		return Err(Error::other(format!("git {} was killed ({how}) in {}", shown.join(" "), dir.display())));
	};
	Ok((code == 0).then(|| result.stdout.trim().to_owned()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::testing::temp_dir;

	#[test]
	fn git_returns_trimmed_stdout_and_none_on_a_non_zero_exit() {
		let dir = temp_dir("git");
		assert_eq!(git(dir.path(), &["init", "-q"]).unwrap(), Some(String::new()));
		assert_eq!(git(dir.path(), &["rev-parse", "--is-inside-work-tree"]).unwrap().as_deref(), Some("true"));
		assert_eq!(git(dir.path(), &["rev-parse", "--verify", "no-such-ref"]).unwrap(), None);
		// A folder that isn't there makes git itself exit non-zero: that reads as `None`, not as a failure to run.
		assert_eq!(git(&dir.path().join("missing"), &["status"]).unwrap(), None);
	}

	#[tokio::test]
	async fn git_async_returns_the_code_and_both_streams() {
		let dir = temp_dir("git");
		let init = git_async(dir.path(), &["init", "-q"], 30_000, GitOptions::default()).await.unwrap();
		assert_eq!(init.code, Some(0));
		let ref_less = git_async(dir.path(), &["rev-parse", "--verify", "no-such-ref"], 30_000, GitOptions::default())
			.await
			.unwrap();
		assert_ne!(ref_less.code, Some(0));
		assert!(!ref_less.stderr.is_empty());
	}
}
