//! The check a project pane's agent gets when its turn ends (0617): the words the omp extension, the internal role and
//! the service share. The extension itself imports nothing (omp loads it from the shipped config folder), so it repeats
//! `PANE_CHECK_ENV` and the time limits below; `test/config/pane-check-extension.test.ts` holds them together.

use serde::{Deserialize, Serialize};

/// The internal word of the `jocasta` command the extension runs when a turn ends (0617); not in the usage text.
pub const PANE_CHECK_ROLE: &str = "pane-check";

/// Where the role asks the service (`POST`, the API token as the bearer): body `{paneKey}`, answered `{text}`.
pub const PANE_CHECK_PATH: &str = "/api/pane-check";

/// What the service answers (`PANE_CHECK_PATH`): the text for the agent, empty when it has nothing to say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneCheckAnswer {
	pub text: String,
}

/// The variable that gives a project pane's extension the argv of the role (`selfCommand()`, then `PANE_CHECK_ROLE`) as a
/// JSON array of strings (0617), the way `push_hook_env` gives git the pre-push hook's command.
pub const PANE_CHECK_ENV: &str = "JOCASTA_PANE_CHECK";

/// The extension's file in the shipped config folder, loaded by `-e` in project panes only (0617).
pub const PANE_CHECK_EXTENSION: &str = "pane-check-extension.ts";

/// How long the service holds a pane's request for its check (0617): under the role's limit and omp's 30 s for a
/// handler, so a slower check lets the turn end and is owed to the next one.
pub const PANE_CHECK_WAIT_MS: i64 = 20_000;

/// How long the role waits for the service: past `PANE_CHECK_WAIT_MS` and the tree key, under the extension's limit.
pub const PANE_CHECK_ASK_MS: i64 = 25_000;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_words_and_limits_are_the_ones_the_extension_repeats() {
		assert_eq!(PANE_CHECK_ROLE, "pane-check");
		assert_eq!(PANE_CHECK_PATH, "/api/pane-check");
		assert_eq!(PANE_CHECK_ENV, "JOCASTA_PANE_CHECK");
		assert_eq!(PANE_CHECK_EXTENSION, "pane-check-extension.ts");
		const { assert!(PANE_CHECK_WAIT_MS < PANE_CHECK_ASK_MS && PANE_CHECK_ASK_MS < 30_000) };
	}

	#[test]
	fn the_answer_is_a_text_object() {
		let answer: PaneCheckAnswer = serde_json::from_str(r#"{"text":"tests are red"}"#).unwrap();
		assert_eq!(answer, PaneCheckAnswer { text: "tests are red".to_owned() });
		assert_eq!(serde_json::to_string(&answer).unwrap(), r#"{"text":"tests are red"}"#);
	}
}
