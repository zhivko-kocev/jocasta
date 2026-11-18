//! The eval run's flags (0318, 0482).

use jocasta::shared::errors::{Error, Result};

/// Reps, per-trial limit, omp profile and dry run for one eval run (0318, 0482).
#[derive(Debug, Clone, PartialEq)]
pub struct RunSettings {
	pub reps: u64,
	/// `None`: no limit.
	pub timeout_minutes: Option<f64>,
	/// omp's `--profile` for every trial (0482 E8); `None`: his default profile.
	pub profile: Option<String>,
	/// Print the plan (cases, reps, each turn's arguments) and run nothing.
	pub dry_run: bool,
}

/// Defaults (0318): 3 reps, 10 minutes per trial.
fn run_defaults() -> RunSettings {
	RunSettings { reps: 3, timeout_minutes: Some(10.0), profile: None, dry_run: false }
}

/// `value` as omp's `--profile` name (pi-utils `dirs.ts`'s rule), for `jocasta-bench evals` and the task benchmark
/// alike; any other text is an `Error::User` (exit 2).
pub fn profile_name(value: &str) -> Result<String> {
	let mut chars = value.chars();
	let fits = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
		&& chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
		&& value.len() <= 64;
	if !fits || value.ends_with('.') {
		return Err(Error::user(format!(
			"--profile needs an omp profile name (a-z, 0-9, \".\", \"_\", \"-\"), not \"{value}\""
		)));
	}
	Ok(value.to_owned())
}

/// What JavaScript counts as white space around a number's text.
fn js_space(c: char) -> bool {
	c.is_whitespace() || c == '\u{feff}'
}

/// JavaScript's `Number(text)`: NaN for text that isn't a number.
pub fn number_of(text: &str) -> f64 {
	let text = text.trim_matches(js_space);
	if text.is_empty() {
		return 0.0;
	}
	match text {
		"Infinity" | "+Infinity" => return f64::INFINITY,
		"-Infinity" => return f64::NEG_INFINITY,
		_ => {}
	}
	for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
		if let Some(digits) = text.strip_prefix(prefix) {
			return u128::from_str_radix(digits, radix).map_or(f64::NAN, |number| number as f64);
		}
	}
	let literal = text.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
	if literal && !text.starts_with(['e', 'E']) { text.parse::<f64>().unwrap_or(f64::NAN) } else { f64::NAN }
}

/// Parses `jocasta-bench evals [--reps <n>] [--timeout <minutes>] [--profile <name>] [--dry-run]` (0318, 0482). Bad
/// values are an `Error::User` (exit 2).
pub fn parse_run_args(argv: &[String]) -> Result<RunSettings> {
	let mut settings = run_defaults();
	let mut i = 0;
	while i < argv.len() {
		let flag = argv[i].as_str();
		i += 1;
		if flag == "--dry-run" {
			settings.dry_run = true;
			continue;
		}
		if !matches!(flag, "--reps" | "--timeout" | "--profile") {
			return Err(Error::user(format!("unknown eval option: {flag}")));
		}
		let Some(value) = argv.get(i) else { return Err(Error::user(format!("{flag} needs a value"))) };
		i += 1;
		let number = number_of(value);
		match flag {
			"--profile" => settings.profile = Some(profile_name(value)?),
			"--reps" => {
				if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) || number < 1.0 {
					return Err(Error::user(format!("--reps needs a whole number of 1 or more, not \"{value}\"")));
				}
				settings.reps = number as u64;
			}
			_ => {
				if value.trim_matches(js_space).is_empty() || !number.is_finite() || number <= 0.0 {
					return Err(Error::user(format!("--timeout needs a number of minutes above 0, not \"{value}\"")));
				}
				settings.timeout_minutes = Some(number);
			}
		}
	}
	Ok(settings)
}

/// What [`take_variant_flags`] separates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantFlags {
	pub variants: Vec<String>,
	pub rest: Vec<String>,
}

/// Takes every `--variant <name>` off `jocasta-bench evals`'s arguments, in order (0375); the rest goes to
/// [`parse_run_args`]. A missing name is an `Error::User` (exit 2).
pub fn take_variant_flags(argv: &[String]) -> Result<VariantFlags> {
	let mut flags = VariantFlags { variants: Vec::new(), rest: Vec::new() };
	let mut i = 0;
	while i < argv.len() {
		let arg = &argv[i];
		i += 1;
		if arg != "--variant" {
			flags.rest.push(arg.clone());
			continue;
		}
		match argv.get(i) {
			Some(name) if !name.starts_with('-') => flags.variants.push(name.clone()),
			_ => return Err(Error::user("--variant needs a name")),
		}
		i += 1;
	}
	Ok(flags)
}
