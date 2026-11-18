//! git for the task benchmark and the eval runner: stdout of `git <args>` in `cwd` (with `env`, default this
//! process's), on the product's hidden runner; a failure fails with `git <args> failed in <cwd>: <stderr>`. A large
//! change's `diff --numstat` passes the 1 MiB default buffer (EVL-4), hence the 256 MiB one. No time limit: the
//! product's 30 s (CO-10) would cut a task repo's `git fetch`.

use std::collections::HashMap;
use std::path::Path;

use jocasta::shared::errors::{Error, Result};
use jocasta::shared::process::{RunOptions, run_program_sync};

pub fn git(cwd: &Path, args: &[impl AsRef<str>], env: Option<HashMap<String, String>>) -> Result<String> {
	let words: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
	let failed = |why: &str| Error::other(format!("git {} failed in {}: {why}", words.join(" "), cwd.display()));
	let mut argv = vec!["git"];
	argv.extend(&words);
	let options = RunOptions { cwd: Some(cwd.to_path_buf()), env, max_bytes: Some(1 << 28), ..RunOptions::default() };
	let result = run_program_sync(&argv, options).map_err(|error| failed(&error.to_string()))?;
	if result.code != Some(0) {
		return Err(failed(&result.stderr));
	}
	Ok(result.stdout)
}
