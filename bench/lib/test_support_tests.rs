//! The library's test support, as the binary's tests reach it: with the `test-support` feature the overlay of the
//! Jocasta and omp roots is on in every process whose `main` doesn't opt out, and no test runs `main`, so a bare read
//! of a root in a bin test never reaches his real folders.

use std::path::Path;

use jocasta::shared::env;
use jocasta::shared::paths::data_dir;
use jocasta::shared::testing::{sandbox, temp_dir, temp_jocasta};

#[test]
fn a_bare_read_of_jocasta_data_dir_in_a_bin_test_is_under_the_sandbox() {
	// No helper has run: nothing a test has to remember.
	let value = env::var("JOCASTA_DATA_DIR").expect("the overlay sets the data dir");
	assert!(Path::new(&value).starts_with(sandbox()), "{value} is not under {}", sandbox().display());
	assert!(data_dir().starts_with(sandbox()));
}

#[test]
fn a_temp_jocasta_gives_the_test_roots_of_its_own_and_a_temp_dir_lies_in_the_sandbox() {
	let temp = temp_jocasta();
	assert!(data_dir().starts_with(temp.root()));
	assert!(temp_dir("bench").path().starts_with(sandbox()));
}
