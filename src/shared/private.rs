//! Private data (0681): a value that cannot become text for the model. `Private<T>` has no `Display`, prints as
//! `Private(..)` in `Debug`, and gives its content to one reader, [`Private::reveal`], which asks for a [`HumanView`]:
//! a token only the code that draws for his eyes (`src/frontier/today/`, `src/frontier/brief/`) may make, which
//! `tests/tree_checks.rs` enforces. Serde carries the content as it is, for the service's own storage and for the
//! messages to the dock.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A value only he reads: mail subjects and links, Jira summaries, Azure resource names.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Private<T>(T);

impl<T> From<T> for Private<T> {
	fn from(value: T) -> Self {
		Self(value)
	}
}

impl<T> Private<T> {
	/// The content, for a screen he reads.
	pub fn reveal(&self, _view: &HumanView) -> &T {
		&self.0
	}

	/// The content for the event queue's columns and nothing else (`tests/tree_checks.rs` holds it to
	/// `src/shared/events/queue.rs`): never for text a model could get.
	pub fn for_storage(&self) -> &T {
		&self.0
	}
}

impl<T> fmt::Debug for Private<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("Private(..)")
	}
}

/// The proof that the code asking draws for his eyes. `HumanView::for_his_eyes()` is called only under
/// `src/frontier/today/` and `src/frontier/brief/` (0681; `tests/tree_checks.rs`), and in this file's own test.
#[derive(Debug, Clone, Copy)]
pub struct HumanView(());

impl HumanView {
	/// The token for code that draws for his eyes: only `src/frontier/today/` and `src/frontier/brief/` call it.
	pub fn for_his_eyes() -> Self {
		Self(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn private_prints_nothing_and_reveals_only_to_a_human_view() {
		let secret = Private::from("canary subject".to_owned());
		assert_eq!(format!("{secret:?}"), "Private(..)");
		assert_eq!(format!("{:?}", Some(&secret)), "Some(Private(..))");
		assert_eq!(secret.reveal(&HumanView::for_his_eyes()), "canary subject");
	}

	#[test]
	fn private_serializes_as_its_content() {
		let secret = Private::from("canary subject".to_owned());
		assert_eq!(serde_json::to_string(&secret).unwrap(), "\"canary subject\"");
		let back: Private<String> = serde_json::from_str("\"canary subject\"").unwrap();
		assert_eq!(back, secret);
	}
}
