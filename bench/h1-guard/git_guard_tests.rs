//! `test/bench/h1-guard.test.ts`'s cases for the git guard's reimplementation and the measured-lines format.

use super::*;

fn words(argv: &[&str]) -> Vec<String> {
	argv.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn the_git_guard_as_reimplemented_the_git_builtin_only_stash_reset_checkout_switch_restore_forms() {
	let refusal = |argv: &[&str]| git_guard_refusal(argv);
	assert_eq!(refusal(&["git", "stash"]).as_deref(), Some("git stash"));
	assert_eq!(refusal(&["git", "stash", "list"]), None);
	assert_eq!(refusal(&["git", "-C", "x", "reset", "--hard"]).as_deref(), Some("git reset --hard"));
	assert_eq!(refusal(&["git", "reset", "HEAD~1"]).as_deref(), Some("git reset to another commit"));
	assert_eq!(refusal(&["git", "reset", "--", "src/a.ts"]), None);
	assert_eq!(refusal(&["git", "checkout", "main"]).as_deref(), Some("git checkout"));
	assert_eq!(refusal(&["git", "restore", "--staged", "a.ts"]), None);
	assert_eq!(refusal(&["git", "restore", "a.ts"]).as_deref(), Some("git restore"));
	assert_eq!(refusal(&["GIT_DIR=/x", "git", "stash"]).as_deref(), Some("git stash with git pointed elsewhere"));
	assert_eq!(refusal(&["git.exe", "stash"]), None);
	assert_eq!(refusal(&["pwsh", "-c", "git stash"]), None);
	assert_eq!(refusal(&["git", "status"]), None);
	let segments = [words(&["cd", "x"]), words(&["git", "switch", "main"])];
	assert_eq!(git_guard_refusal_of(&segments).as_deref(), Some("git switch"));
}
