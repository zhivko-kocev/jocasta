//! Briefs (0678, 0721): what Jocasta shows him beside his work, built by code from a typed model and drawn as native
//! windows (`frontier::view`). `model.rs` is the model, `layout.rs` validates a brief and lays out its flows,
//! `store.rs` keeps them under `<data>/briefs`, `morning.rs` builds the morning brief.

pub mod layout;
pub mod model;
pub mod morning;
pub mod store;

/// How a brief reaches him (0721): its link as a toast and `today` carry it, and opening its window.
pub trait BriefLinks: Send + Sync {
	/// The brief's link, `jocasta:brief/<id>`.
	fn url(&self, id: &str) -> String;
	/// Opens the brief in its own window.
	fn open(&self, id: &str);
}
