//! A5 (0482): his case study, 10 real tasks end to end through the boss. This tool records and counts; the tasks are
//! his.
//!
//!   jocasta-bench evals a5 record --out <dir> [--dry-run]
//!     appends the service's feed (toasts and every other broadcast) to <dir>/service-feed.jsonl until Ctrl+C.
//!   jocasta-bench evals a5 report --feed <service-feed.jsonl> --boss <boss session.jsonl> --threads <session.jsonl,…>
//!                                 [--cap <minutes>] [--out <dir>] [--dry-run]
//!     interventions per thread, attention minutes (each urgent toast to his next response) and the batch time.
//!
//! His responses: his user messages in the boss session (not the service's `[event]` lines) and in thread sessions
//! (not each thread's brief, nor any line the service types into a pane: the review prompt, `/handoff`, the report
//! steer, a failed check on idle, `/plan-review`, the PR request), and plan approvals (leaving plan mode after a plan was
//! ready). A boss relay of his words into a thread (`thread_send`) counts in both sessions.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::to_value;

use super::args::number_of;
use crate::lib::args::{ArgSpec, BenchUsageError, flag, flag_or, list_flag, parse_bench_args, run_script};
use crate::lib::output::{append_row, json_text, print_plan, write_json};
use crate::lib::service_feed::{FeedEntry, parse_feed, record_feed, toasts_of};
use crate::lib::sessions::{UserMessage, plan_approval_times, session_start, user_messages};
use crate::lib::stats::{BootstrapOptions, Summary, summarize};
use jocasta::frontier::server::wording::by_service;
use jocasta::shared::errors::Result;

/// A message the service typed into a pane (SVC-1): one list, `wording.rs`'s, beside the texts themselves.
fn typed_by_service(message: &UserMessage) -> bool {
	by_service(&message.text)
}

pub struct ThreadInput {
	pub name: String,
	pub session: String,
}

pub struct A5Input<'a> {
	pub feed: &'a [FeedEntry],
	pub boss: &'a str,
	pub threads: &'a [ThreadInput],
	/// A toast unanswered this long counts as unanswered, not as attention.
	pub cap_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThreadReport {
	pub name: String,
	pub interventions: usize,
	pub messages: usize,
	pub approvals: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct A5Report {
	pub threads: Vec<ThreadReport>,
	pub boss_messages: usize,
	pub interventions: usize,
	pub urgent_toasts: usize,
	pub answered: usize,
	pub attention_minutes: f64,
	/// Per answered toast, minutes to his response.
	pub response_minutes: Summary,
	/// First thread start to the last `done` toast, minutes; `None` without both.
	pub batch_minutes: Option<f64>,
}

pub fn a5_report(input: &A5Input) -> A5Report {
	let boss_messages: Vec<UserMessage> =
		user_messages(input.boss).into_iter().filter(|message| !typed_by_service(message)).collect();
	struct Thread {
		name: String,
		messages: Vec<UserMessage>,
		approvals: Vec<f64>,
		start: f64,
	}
	let threads: Vec<Thread> = input
		.threads
		.iter()
		.map(|thread| Thread {
			name: thread.name.clone(),
			// The first user message is the brief `thread_start` typed.
			messages: user_messages(&thread.session)
				.into_iter()
				.skip(1)
				.filter(|message| !typed_by_service(message))
				.collect(),
			approvals: plan_approval_times(&thread.session),
			start: session_start(&thread.session),
		})
		.collect();
	let mut responses: Vec<f64> = boss_messages
		.iter()
		.map(|message| message.at)
		.chain(threads.iter().flat_map(|thread| thread.messages.iter().map(|m| m.at).chain(thread.approvals.clone())))
		.filter(|at| at.is_finite())
		.collect();
	responses.sort_by(f64::total_cmp);
	let urgent = toasts_of(input.feed, true);
	let mut waits: Vec<f64> = Vec::new();
	for toast in &urgent {
		if let Some(response) = responses.iter().find(|at| **at >= toast.at)
			&& response - toast.at <= input.cap_ms
		{
			waits.push(response - toast.at);
		}
	}
	let starts: Vec<f64> = threads.iter().map(|thread| thread.start).filter(|at| at.is_finite()).collect();
	let done: Vec<f64> = toasts_of(input.feed, false)
		.into_iter()
		.filter(|toast| toast.message.get("reason").and_then(|reason| reason.as_str()) == Some("done"))
		.map(|toast| toast.at)
		.collect();
	let minutes = |ms: f64| ms / 60_000.0;
	let extreme = |values: &[f64], pick: fn(f64, f64) -> f64| values.iter().copied().reduce(pick).unwrap_or(f64::NAN);
	let waits_minutes: Vec<f64> = waits.iter().map(|wait| minutes(*wait)).collect();
	A5Report {
		threads: threads
			.iter()
			.map(|thread| ThreadReport {
				name: thread.name.clone(),
				interventions: thread.messages.len() + thread.approvals.len(),
				messages: thread.messages.len(),
				approvals: thread.approvals.len(),
			})
			.collect(),
		boss_messages: boss_messages.len(),
		interventions: boss_messages.len()
			+ threads.iter().map(|thread| thread.messages.len() + thread.approvals.len()).sum::<usize>(),
		urgent_toasts: urgent.len(),
		answered: waits.len(),
		attention_minutes: minutes(waits.iter().sum()),
		response_minutes: summarize(&waits_minutes, BootstrapOptions::default()),
		batch_minutes: if !starts.is_empty() && !done.is_empty() {
			Some(minutes(extreme(&done, f64::max) - extreme(&starts, f64::min)))
		} else {
			None
		},
	}
}

/// `jocasta-bench evals a5 record|report …`.
pub async fn main(argv: Vec<String>) -> std::process::ExitCode {
	run_script(run(argv)).await
}

async fn run(argv: Vec<String>) -> Result<()> {
	let mode = argv.first().map(String::as_str);
	let rest = argv.get(1..).unwrap_or_default();
	if mode == Some("record") {
		let args = parse_bench_args(rest, &ArgSpec::new(1))?;
		let file = args.out.as_ref().map_or_else(
			|| "<out>/service-feed.jsonl".to_owned(),
			|out| out.join("service-feed.jsonl").display().to_string(),
		);
		if args.dry_run {
			print_plan(
				"A5 record",
				&[
					"connect to the state feed of the service <data>/jocasta-service.json names, as a page does"
						.to_owned(),
					format!("append every broadcast with its arrival time to {file} until Ctrl+C"),
				],
				args.out.as_deref(),
			);
			return Ok(());
		}
		let recorder = record_feed(Path::new(&file)).await?;
		println!("recording the service's feed into {file}; Ctrl+C stops");
		let stop = recorder.stopper();
		tokio::spawn(async move {
			let _ = tokio::signal::ctrl_c().await;
			stop.notify_one();
		});
		recorder.closed().await;
		return Ok(());
	}
	if mode != Some("report") {
		return Err(BenchUsageError::err("usage: jocasta-bench evals a5 record|report …"));
	}
	let spec = ArgSpec { default_runs: 1, valued: &["--feed", "--boss", "--threads", "--cap"], ..ArgSpec::default() };
	let args = parse_bench_args(rest, &spec)?;
	let feed_path = flag(&args, "feed");
	let boss_path = flag(&args, "boss");
	let thread_paths = list_flag(&args, "threads", &[]);
	let cap = number_of(flag_or(&args, "cap", "60"));
	if !cap.is_finite() || cap <= 0.0 {
		return Err(BenchUsageError::err("--cap needs minutes above 0"));
	}
	if args.dry_run {
		print_plan(
			"A5 report",
			&[
				format!(
					"toasts from {}; his boss messages from {}",
					feed_path.unwrap_or("<--feed>"),
					boss_path.unwrap_or("<--boss>")
				),
				format!(
					"{} thread sessions: interventions = his messages after the brief + plan approvals",
					thread_paths.len()
				),
				format!(
					"attention = Σ (urgent toast → his next response), responses later than {} min count as unanswered",
					super::report::js_text(cap)
				),
				"batch time = first thread start → last done toast".to_owned(),
			],
			args.out.as_deref(),
		);
		return Ok(());
	}
	let (Some(feed_path), Some(boss_path)) = (feed_path, boss_path) else {
		return Err(BenchUsageError::err("report needs --feed, --boss and --threads"));
	};
	if thread_paths.is_empty() {
		return Err(BenchUsageError::err("report needs --feed, --boss and --threads"));
	}
	let feed = parse_feed(&fs::read_to_string(feed_path)?);
	let boss = fs::read_to_string(boss_path)?;
	let threads = thread_paths
		.iter()
		.map(|path| {
			Ok(ThreadInput {
				name: Path::new(path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
				session: fs::read_to_string(path)?,
			})
		})
		.collect::<Result<Vec<_>>>()?;
	let report = a5_report(&A5Input { feed: &feed, boss: &boss, threads: &threads, cap_ms: cap * 60_000.0 });
	println!("{}", json_text(&to_value(&report)?, true));
	if let Some(out) = &args.out {
		for thread in &report.threads {
			append_row(out, "a5", thread)?;
		}
		let summary = serde_json::json!({ "benchmark": "A5", "report": report });
		println!("wrote {}", write_json(out, "a5", &summary)?.display());
	}
	Ok(())
}
