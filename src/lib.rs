//! Jocasta, the Rust build (0625). Each module sits beside the TypeScript file it replaces (0629) until the cutover
//! (0628); the TypeScript build is the contract it matches, with plan appendix I.
//!
//! Conventions every module follows:
//! - A TypeScript export keeps its name in Rust's case (`runProgram` → `run_program`, `UserError` → `Error::User`).
//!   A function that returns a Promise becomes an `async fn`; a synchronous one stays synchronous.
//! - Errors are [`shared::errors::Error`] through [`shared::errors::Result`]; a refusal he reads is `Error::User`.
//! - The environment is read through [`shared::env`], never `std::env::var` directly, so unit tests can point every
//!   Jocasta and omp root at their own folders ([`shared::testing`]).
//! - What goes to disk or over the wire keeps the TypeScript build's bytes: JSON keys, file names, exit codes.

pub mod entry;
pub mod frontier;
pub mod mcp;
pub mod setup;
pub mod shared;
