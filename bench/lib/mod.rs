//! The benchmark harness's shared library (`bench/lib/*.ts`): argument parsing, results output, statistics, the cost
//! ledger and the helpers the live benchmarks share, each module beside the TypeScript file it replaces (0629).

pub mod args;
pub mod git;
pub mod herdr;
pub mod jocasta;
pub mod jsonl;
pub mod launch;
pub mod ledger;
pub mod output;
pub mod prices;
pub mod proctree;
pub mod sandbox;
pub mod service_feed;
pub mod sessions;
pub mod stats;
pub mod temp;
pub mod threads;
pub mod timing;

#[cfg(test)]
mod scripts_tests;
#[cfg(test)]
mod test_support_tests;
