//! The eval runner (`evals/*.ts`, 0318), ported to Rust as part of `jocasta-bench` (0636); each module sits beside the
//! TypeScript file it replaces (0629).

pub mod a5;
pub mod args;
pub mod bench;
pub mod bench_cli;
pub mod bench_validate;
pub mod cases;
pub mod compare;
pub mod followup;
pub mod grading;
pub mod harness;
pub mod keep;
pub mod ladder;
pub mod layer;
pub mod pacer;
pub mod plan_review;
pub mod replay;
pub mod report;
pub mod run;
pub mod runner;
pub mod settings;
#[cfg(feature = "test-support")]
pub mod stand_in;
pub mod tasks;
pub mod terminal;
pub mod variants;
pub mod watch;

#[cfg(test)]
mod a5_tests;
#[cfg(test)]
mod args_tests;
#[cfg(test)]
mod arms_tests;
#[cfg(test)]
mod bench_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod keep_tests;
#[cfg(test)]
mod ladder_tests;
#[cfg(test)]
mod pacer_tests;
#[cfg(test)]
mod plan_review_tests;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod tasks_tests;
