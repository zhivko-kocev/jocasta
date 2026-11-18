//! Ported from `test/evals/args.test.ts`. The case that runs `evals/run.ts` as a process waits for `evals/run.rs`.

use super::args::{RunSettings, VariantFlags, parse_run_args, take_variant_flags};

fn words(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

fn settings(reps: u64, timeout_minutes: f64) -> RunSettings {
	RunSettings { reps, timeout_minutes: Some(timeout_minutes), profile: None, dry_run: false }
}

#[test]
fn runs_default_to_3_reps_and_10_minutes() {
	assert_eq!(parse_run_args(&[]).unwrap(), settings(3, 10.0));
}

#[test]
fn reps_and_timeout_override_the_defaults() {
	assert_eq!(parse_run_args(&words(&["--reps", "5", "--timeout", "2.5"])).unwrap(), settings(5, 2.5));
	assert_eq!(parse_run_args(&words(&["--timeout", "1"])).unwrap(), settings(3, 1.0));
	assert_eq!(parse_run_args(&words(&["--reps", "4"])).unwrap(), settings(4, 10.0));
}

#[test]
fn bad_values_are_refused() {
	let refused: [(&[&str], &str); 10] = [
		(&["--reps", "0"], "--reps needs a whole number"),
		(&["--reps", "2.5"], "--reps needs a whole number"),
		(&["--reps", "-1"], "--reps needs a whole number"),
		(&["--reps", "x"], "--reps needs a whole number"),
		(&["--timeout", "0"], "--timeout needs a number of minutes"),
		(&["--timeout", "-3"], "--timeout needs a number of minutes"),
		(&["--timeout", ""], "--timeout needs a number of minutes"),
		(&["--timeout", "Infinity"], "--timeout needs a number of minutes"),
		(&["--reps"], "--reps needs a value"),
		(&["--fast"], "unknown eval option: --fast"),
	];
	for (argv, message) in refused {
		let error = parse_run_args(&words(argv)).expect_err(&format!("{argv:?} is refused"));
		assert!(error.to_string().contains(message), "{argv:?}: {error}");
		assert!(error.is_user());
	}
}

#[test]
fn variant_is_taken_off_in_order_repeats_included() {
	assert_eq!(
		take_variant_flags(&words(&["--variant", "copilot", "--reps", "2", "--variant", "copilot"])).unwrap(),
		VariantFlags { variants: words(&["copilot", "copilot"]), rest: words(&["--reps", "2"]) }
	);
	assert_eq!(take_variant_flags(&words(&["--variant"])).unwrap_err().to_string(), "--variant needs a name");
	assert_eq!(take_variant_flags(&words(&["--variant", "-x"])).unwrap_err().to_string(), "--variant needs a name");
}
