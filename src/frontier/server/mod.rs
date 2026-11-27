pub mod allow;
pub mod ask_question;
pub mod asker;
pub mod board;
pub mod boss;
pub mod bridge;
pub mod brief_actions;
pub mod check_costs;
pub mod checks;
pub mod ci_fix;
pub mod cleanroom;
pub mod close;
pub mod context;
pub mod cost_cap;
pub mod day;
pub mod day_briefs;
#[cfg(test)]
mod e2e_allow_tests;
#[cfg(test)]
mod e2e_day_tests;
#[cfg(test)]
mod e2e_finish_tests;
#[cfg(test)]
mod e2e_handoff_tests;
#[cfg(test)]
mod e2e_keys_tests;
#[cfg(test)]
mod e2e_pane_check_tests;
#[cfg(test)]
mod e2e_plans_tests;
#[cfg(test)]
mod e2e_privacy_tests;
#[cfg(test)]
mod e2e_push_gate_tests;
#[cfg(test)]
mod e2e_regressions_tests;
#[cfg(test)]
mod e2e_relaunch_tests;
#[cfg(test)]
mod e2e_thread_start_tests;
#[cfg(test)]
mod e2e_tools_tests;
pub mod embedded;
pub mod events;
pub mod feed;
pub mod finish;
pub mod first_run;
pub mod health_route;
#[cfg(test)]
#[allow(dead_code, reason = "the later waves' service tests build on these helpers")]
pub(crate) mod helpers;
#[cfg(test)]
mod helpers_procs;
pub mod history;
pub mod instance;
pub mod journal;
pub mod keys;
pub mod launch;
pub mod lessons;
pub mod mise;
pub mod native;
pub mod omp_processes;
pub mod overlay;
pub mod own_surface;
pub mod pane_launch;
pub mod pending;
pub mod phone;
pub mod places;
pub mod popups;
pub mod pr_actions;
pub mod pr_review;
pub mod production;
pub mod ptt;
pub mod ptt_hold;
pub mod ptt_worker;
pub mod push_gate;
pub mod quota;
pub mod refresh;
pub mod reopen;
pub mod report;
pub mod review_flow;
pub mod review_notes;
pub mod review_prompt;
pub mod review_proof;
pub mod review_system;
pub mod routes;
#[allow(clippy::module_inception)]
pub mod server;
pub mod service;
pub mod sessions;
pub mod sleep;
pub mod spend;
pub mod status;
pub mod steps;
pub mod stop_sequence;
pub mod surface;
pub mod task_file;
pub mod thread;
pub mod thread_brief;
pub mod thread_facts;
pub mod thread_keys;
pub mod thread_lifecycle;
pub mod thread_start;
pub mod tool_specs;
pub mod tools;
pub mod watcher;
pub mod weekly_brief;
pub mod weekly_review;
pub mod wording;
pub mod worklog;
pub mod works;
pub mod worktrees;
