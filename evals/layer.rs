//! A6's seam (0543): A6 = A3 + the harness-OS layer (0532), built into each trial by Jocasta's own code: the task
//! file's brief and Done-when through `--append-system-prompt` (0540), the `report_result` tool (0539), the cached
//! checks with gate replies, the `check` thread tool (0542, 0546, 0571) and the history search tool (0604). The plan
//! review is the bench's, not the layer's (`plan_review.rs`, 0567). The ledger needs
//! nothing from it: the bench reads every trial's saved sessions itself, with the product's ledger (`src/shared/ledger.rs`).
//!
//! The layer is Jocasta's build, linked into this binary from the library ([`LAYER_MODULE`]); its types
//! (`BenchLayer`, `LayerSetup`, `LayerTrial`) live there too, since the library can't import from the binary. Bun
//! loaded the module by a dynamic import that could find it missing (`LayerUnavailableError`); nothing here can.

pub use jocasta::frontier::layer::bench::{BENCH_LAYER, BenchLayer, LayerSetup, LayerTrial};

/// The module Jocasta's layer build provides, relative to the repo root; the dry run's plan names it.
pub const LAYER_MODULE: &str = "src/frontier/layer/bench.rs";
