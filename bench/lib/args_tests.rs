//! `parse_bench_args`: the flags every script takes, and its refusals.

use super::*;

fn parse(words: &[&str], spec: &ArgSpec) -> Result<BenchArgs> {
	let argv: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
	parse_bench_args(&argv, spec)
}

fn refusal(result: Result<BenchArgs>) -> String {
	let Err(Error::User { message, exit_code }) = result else { panic!("not refused") };
	assert_eq!(exit_code, 2);
	message
}

fn spec() -> ArgSpec {
	ArgSpec { default_runs: 5, valued: &["--threads"], booleans: &["--verbose"], ..ArgSpec::default() }
}

#[test]
fn out_runs_and_the_scripts_own_flags_are_read_and_the_last_value_wins() {
	let out = std::env::temp_dir().join("jocasta-bench-args-out");
	let out_text = out.to_string_lossy().into_owned();
	let args = parse(&["--out", &out_text, "--threads", "2,4", "--verbose", "--threads", "8", "--runs", "7"], &spec())
		.unwrap();
	assert_eq!((args.out.as_deref(), args.runs, args.dry_run), (Some(out.as_path()), 7, false));
	assert_eq!(flag(&args, "threads"), Some("8"));
	assert_eq!(args.flags.get("verbose"), Some(&FlagValue::Set));
	assert_eq!(flag_or(&args, "missing", "fallback"), "fallback");
	assert_eq!(parse(&["--out", &out_text], &spec()).unwrap().runs, 5);
}

#[test]
fn a_dry_run_needs_no_out_and_a_real_run_does() {
	let args = parse(&["--dry-run"], &spec()).unwrap();
	assert!(args.dry_run && args.out.is_none());
	assert_eq!(refusal(parse(&[], &spec())), "--out <dir> is required (outside the repo)");
}

#[test]
fn refusals_name_the_flag() {
	assert_eq!(refusal(parse(&["--nope"], &spec())), "unknown option: --nope");
	assert_eq!(refusal(parse(&["--threads"], &spec())), "--threads needs a value");
	assert_eq!(refusal(parse(&["--threads", "--verbose"], &spec())), "--threads needs a value");
	assert_eq!(
		refusal(parse(&["--dry-run", "--runs", "0"], &spec())),
		"--runs needs a whole number of 1 or more, not \"0\""
	);
	assert_eq!(
		refusal(parse(&["--dry-run", "--runs", "2x"], &spec())),
		"--runs needs a whole number of 1 or more, not \"2x\""
	);
	let inside = REPO_ROOT.join("results");
	let message = refusal(parse(&["--out", &inside.to_string_lossy()], &spec()));
	assert!(
		message.starts_with("--out must be outside the repo (raw results live in D:/thesis/data/), not "),
		"{message}"
	);
}

#[test]
fn list_flags_split_on_commas_and_number_lists_take_whole_numbers() {
	let args = parse(&["--dry-run", "--threads", " 2, 4 ,,8"], &spec()).unwrap();
	assert_eq!(list_flag(&args, "threads", &["1"]), ["2", "4", "8"]);
	assert_eq!(number_list_flag(&args, "threads", &[1]).unwrap(), [2, 4, 8]);
	assert_eq!(number_list_flag(&args, "other", &[1, 3]).unwrap(), [1, 3]);
	let bad = parse(&["--dry-run", "--threads", "2,x"], &spec()).unwrap();
	let Err(Error::User { message, .. }) = number_list_flag(&bad, "threads", &[]) else { panic!("not refused") };
	assert_eq!(message, "--threads takes whole numbers, not \"x\"");
}
