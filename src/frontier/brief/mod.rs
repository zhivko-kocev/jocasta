//! Briefs (0678): HTML pages the service serves on loopback, built by code from a typed model. `model.rs` is the
//! model, `layout.rs` validates a brief and lays out its flows, `store.rs` keeps them under `<data>/briefs`, `routes.rs`
//! serves them with the hardening of 0678, and `page/` is the page itself, embedded.

pub mod layout;
pub mod model;
pub mod morning;
pub mod routes;
pub mod store;

/// How a brief reaches his browser (0678): its URL as the toast carries it, and opening it with a fresh launch code
/// (`http://127.0.0.1:<port>/brief/<id>?t=<code>`).
pub trait BriefLinks: Send + Sync {
	/// The brief's page URL without a launch code, `http://127.0.0.1:<port>/brief/<id>`.
	fn url(&self, id: &str) -> String;
	/// Opens the brief in his browser.
	fn open(&self, id: &str);
}
