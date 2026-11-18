//! H8 (0482, extended by 0543 K11, 0576): parallel isolation and overlap detection, in a sandbox Jocasta (VM: the
//! threads' agents call the model).
//!
//!   jocasta-bench h8-parallel --out <dir> --project <name> --repo <main checkout> [--threads 2,4,8] [--runs 1]
//!                             [--layer on|off] [--timeout 60] [--make-fixture] [--dry-run]
//!
//! `--make-fixture` writes the fixture into `--repo` (an empty folder that becomes a git repo; register it as a sandbox
//! project before the run). Threads come in pairs: an even pair both edit the same file (`src/mod<k>.ts`), an odd pair
//! edits one interface from both sides (`src/shared.ts` and `src/use<k>.ts`), in the style of "Passes Alone, Fails
//! Together": the fixture's `test/pair<k>.test.ts` calls `show<k>(makeItem<k>())`, which passes with either side alone
//! and throws with both. Each thread starts as `jocasta thread new` does (no Allow, 0544); threads open in plan mode,
//! so he approves each plan. The script reads `jocasta thread list` until every thread of the run is finished (0529:
//! idle or done with a commit on its branch; a fresh thread reads idle before its brief lands and a waiting plan reads
//! done), then measures:
//!   wrong-thread edits  files a thread changed outside its own assignment, plus any change in the main checkout;
//!   flagged before merge  pairs whose `thread list` line names the partner under "overlaps" (0456);
//!   conflicts at merge  pairs whose branches conflict in `git merge-tree --write-tree`;
//!   semantic failures  pairs that merge cleanly but whose merged tree, checked out in a temp worktree, fails `bun test`;
//!   found only at merge  conflicts and semantic failures the overlaps view didn't flag.
//! K11 (`--layer on|off`, 0543, 0576): 4 threads at once, 3 runs, with the harness-OS layer on and then off: the sandbox
//! service is started with `JOCASTA_LAYER=off` for `--layer off`, and H8 reads the layer's state from `/health` and
//! refuses a mismatch. Each run also measures the cost per finished thread (every omp session of each thread since the
//! run started, its planning session included, with their subagents, priced by the benchmark's ledger at the pinned
//! table), the peak working set and private bytes of Jocasta's processes (the service's, herdr's server's with the
//! threads, and the attached surfaces' trees; sampled every 10 s), and, layer on, the run's shared check hits, as the
//! sandbox service counts them in its data dir.
//! Writes h8-parallel.jsonl (one row per thread count and run) and h8-parallel.json.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::evals::args::number_of;
use crate::evals::report::js_text;
use crate::lib::args::{
	ArgSpec, BenchArgs, BenchUsageError, FlagValue, flag, flag_or, number_list_flag, parse_bench_args, run_script,
};
use crate::lib::jocasta::{SandboxJocasta, open_sandbox};
use crate::lib::ledger::bench_pricing;
use crate::lib::output::{append_row, claim_rows, conditions, print_plan, write_json};
use crate::lib::prices::{PriceTable, load_price_table};
use crate::lib::proctree::{jocasta_processes, snapshot_processes, tree_memory};
use crate::lib::sandbox::{Switch, require_service_switch};
use crate::lib::temp::mkdtemp;
use crate::lib::threads::{ThreadLine, list_threads, start_thread, wait_for_enter};
use jocasta::frontier::server::checks::{checks_dir, read_check_hits};
use jocasta::frontier::server::herdr::PaneInfo;
use jocasta::frontier::server::launch::OnOff;
use jocasta::frontier::server::sessions::SessionIndex;
use jocasta::shared::errors::{Error, Result};
use jocasta::shared::fsx::remove_tree;
use jocasta::shared::ledger::{thread_session_records, trial_ledger};
use jocasta::shared::pathkey::{resolve, same_path};
use jocasta::shared::process::{RunOptions, run_program_sync};

/// K11's run (0543): 4 threads at once, 3 runs per layer state.
pub const K11_THREADS: u64 = 4;
pub const K11_RUNS: u64 = 3;
const MEMORY_SAMPLE_MS: u64 = 10_000;

/// The layer's shared-service counters (K11), as the service counts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerCounters {
	pub check_hits: usize,
}

/// K11's shared check hits so far in a Jocasta data dir: the check runner logs one line per result reused or run joined
/// (`<data>/checks/hits.log`, 0542, 0559). A run's own count is the difference across it.
pub fn read_layer_counters(data_dir: &Path) -> LayerCounters {
	LayerCounters { check_hits: read_check_hits(&checks_dir(data_dir)) }
}

/// Each thread's omp session files since `since` (epoch ms), as the service's own ledger finds them (0538, 0546): the
/// session herdr reports for the pane in the thread's worktree, followed where omp moved it, and every other session in
/// its folder that began since, oldest first. omp starts a new session on "Approve and execute", so the session herdr
/// reports at the end is only the execution's; the planning one sits beside it. `worktrees` are (branch, folder) in
/// order; the result keeps that order. `index` is a fresh one when `None`.
pub fn thread_session_files(
	panes: &[PaneInfo],
	worktrees: &[(String, String)],
	since: i64,
	index: Option<&mut SessionIndex>,
) -> Vec<(String, Vec<PathBuf>)> {
	let mut fresh;
	let index = match index {
		Some(index) => index,
		None => {
			fresh = SessionIndex::new(None, None);
			&mut fresh
		}
	};
	let mut files = Vec::new();
	for (branch, worktree) in worktrees {
		let found = panes.iter().find(|pane| {
			pane.session_path.is_some() && pane.cwd.as_deref().is_some_and(|cwd| same_path(cwd, Path::new(worktree)))
		});
		let Some(session) = found.and_then(|pane| pane.session_path.as_deref()) else { continue };
		let moved = index.moved(session);
		files.push((branch.clone(), index.sessions_since(&moved, since)));
	}
	files
}

/// The fixture: one shared interface file with each odd pair's `makeItem<k>()`, a module and a user per pair, a test per
/// odd pair that calls `show<k>(makeItem<k>())` (it passes with either side of the pair alone and throws with both:
/// the semantic break, 0576), and a package file. Paths are in the order the TypeScript wrote them.
pub fn fixture_files(max_threads: u64) -> Vec<(String, String)> {
	let pairs = max_threads.div_ceil(2);
	let shared: Vec<String> = (0..pairs)
		.map(|k| {
			let mut part = format!("export interface Item{k} {{\n\tid: string;\n\tlabel: string;\n}}\n");
			if k % 2 == 1 {
				part.push_str(&format!(
					"\nexport function makeItem{k}(): Item{k} {{\n\treturn {{ id: \"{k}\", label: \"item {k}\" }};\n}}\n"
				));
			}
			part
		})
		.collect();
	let mut files = vec![
		(
			"package.json".to_owned(),
			"{\n  \"name\": \"acme-parallel\",\n  \"private\": true,\n  \"type\": \"module\"\n}\n".to_owned(),
		),
		("src/shared.ts".to_owned(), shared.join("\n")),
	];
	for k in 0..pairs {
		files.push((
			format!("src/mod{k}.ts"),
			format!("export function base{k}(value: number): number {{\n\treturn value;\n}}\n"),
		));
		files.push((
			format!("src/use{k}.ts"),
			format!(
				"import type {{ Item{k} }} from \"./shared.ts\";\n\nexport function show{k}(item: Item{k}): string {{\n\treturn item.label;\n}}\n"
			),
		));
		if k % 2 == 1 {
			files.push((
				format!("test/pair{k}.test.ts"),
				[
					"import { expect, test } from \"bun:test\";".to_owned(),
					format!("import {{ makeItem{k} }} from \"../src/shared.ts\";"),
					format!("import {{ show{k} }} from \"../src/use{k}.ts\";"),
					String::new(),
					format!("test(\"show{k} shows a made item\", () => {{"),
					format!("\texpect(() => show{k}(makeItem{k}())).not.toThrow();"),
					"});".to_owned(),
					String::new(),
				]
				.join("\n"),
			));
		}
	}
	files
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
	pub thread: usize,
	pub pair: usize,
	pub title: String,
	pub files: Vec<String>,
	pub brief: String,
}

/// Thread i's assignment among n: pair ⌊i/2⌋; even pairs share a file, odd pairs share an interface.
pub fn assignments(n: usize, run: u64) -> Vec<Assignment> {
	(0..n)
		.map(|thread| {
			let pair = thread / 2;
			let first = thread % 2 == 0;
			let title = format!("h8 r{run} n{n} t{thread}");
			if pair % 2 == 0 {
				let file = format!("src/mod{pair}.ts");
				let brief = if first {
					format!("Benchmark H8: in {file}, change base{pair} so it returns value * 2. Change no other file. Commit when done.")
				} else {
					format!("Benchmark H8: in {file}, change base{pair} so it returns value + 1. Change no other file. Commit when done.")
				};
				return Assignment { thread, pair, title, files: vec![file], brief };
			}
			if first {
				Assignment {
					thread,
					pair,
					title,
					files: vec!["src/shared.ts".to_owned()],
					brief: format!("Benchmark H8: in src/shared.ts, rename Item{pair}'s field label to title. Change no other file. Commit when done."),
				}
			} else {
				Assignment {
					thread,
					pair,
					title,
					files: vec![format!("src/use{pair}.ts")],
					brief: format!("Benchmark H8: in src/use{pair}.ts, make show{pair} return the label in upper case. Change no other file. Commit when done."),
				}
			}
		})
		.collect()
}

/// A thread is finished when it is idle or done and its branch has a commit beyond `main` (0529): each brief ends
/// "Commit when done", while a fresh thread reads idle before its brief lands and a waiting plan reads done.
pub fn finished(line: &ThreadLine, commits_ahead: u64) -> bool {
	(line.state == "idle" || line.state == "done") && commits_ahead > 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResult {
	pub pair: usize,
	pub flagged: bool,
	pub conflict: bool,
	/// The pair merged cleanly and the merged tree failed `bun test` (0576).
	pub semantic_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunScore {
	pub threads: usize,
	pub wrong_thread_edits: usize,
	pub main_checkout_changes: usize,
	pub pairs: Vec<PairResult>,
	pub flagged_before_merge: usize,
	pub conflicts_at_merge: usize,
	pub semantic_failures: usize,
	pub found_only_at_merge: usize,
}

/// K11's measures of one run (0543); null fields where the run has none.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct K11Measures {
	pub layer: &'static str,
	pub finished_threads: usize,
	/// Dollars over every thread's sessions at the pinned price table.
	pub cost_usd: f64,
	pub cost_per_finished_thread: Option<f64>,
	/// Models the price table lacks; their requests aren't in `cost_usd`.
	pub unpriced_models: Vec<String>,
	pub peak_working_set_mb: Option<f64>,
	pub peak_private_mb: Option<f64>,
	pub check_hits: Option<i64>,
}

/// One run's row: the score, the run's number and, with `--layer`, K11's measures (flattened, in that order).
#[derive(Debug, Clone, PartialEq, Serialize)]
struct RunRow {
	#[serde(flatten)]
	score: RunScore,
	run: u64,
	#[serde(flatten)]
	k11: Option<K11Measures>,
}

/// What a run's score is computed from: the assignments, the threads' lines, their changed files, the pairs' merge
/// conflicts and semantic failures, and the main checkout's changes.
pub struct ScoreInputs<'a> {
	pub work: &'a [Assignment],
	pub lines: &'a [ThreadLine],
	pub changed: &'a HashMap<String, Vec<String>>,
	pub conflicts: &'a HashMap<usize, bool>,
	pub main_changes: usize,
	pub semantic_failures: &'a HashMap<usize, bool>,
}

pub fn score(inputs: &ScoreInputs) -> RunScore {
	let ScoreInputs { work, lines, changed, conflicts, main_changes, semantic_failures } = *inputs;
	let by_title: HashMap<&str, &ThreadLine> = lines.iter().map(|line| (line.title.as_str(), line)).collect();
	let mut wrong = 0;
	for assignment in work {
		let files = changed.get(&assignment.title).map_or(&[][..], Vec::as_slice);
		wrong += files.iter().filter(|file| !assignment.files.contains(file)).count();
	}
	let mut order: Vec<usize> = Vec::new();
	for assignment in work {
		if !order.contains(&assignment.pair) {
			order.push(assignment.pair);
		}
	}
	let pairs: Vec<PairResult> = order
		.into_iter()
		.map(|pair| {
			let mut members = work.iter().filter(|x| x.pair == pair);
			let (a, b) = (members.next(), members.next());
			let names = |from: &Assignment, other: &Assignment| {
				by_title.get(from.title.as_str()).is_some_and(|line| line.overlaps.contains(&other.title))
			};
			let flagged = matches!((a, b), (Some(a), Some(b)) if names(a, b) || names(b, a));
			PairResult {
				pair,
				flagged,
				conflict: conflicts.get(&pair).copied().unwrap_or(false),
				semantic_failure: semantic_failures.get(&pair).copied().unwrap_or(false),
			}
		})
		.collect();
	RunScore {
		threads: work.len(),
		wrong_thread_edits: wrong,
		main_checkout_changes: main_changes,
		flagged_before_merge: pairs.iter().filter(|p| p.flagged).count(),
		conflicts_at_merge: pairs.iter().filter(|p| p.conflict).count(),
		semantic_failures: pairs.iter().filter(|p| p.semantic_failure).count(),
		found_only_at_merge: pairs.iter().filter(|p| (p.conflict || p.semantic_failure) && !p.flagged).count(),
		pairs,
	}
}

/// What one git command left: its exit code (`None` when it was ended) and stdout.
#[derive(Debug, Clone)]
pub struct GitRun {
	pub status: Option<i32>,
	pub stdout: String,
}

/// `git <args>` in `cwd` as the script runs it: no time limit, a failure's code and stdout returned.
pub fn run_git(cwd: &Path, args: &[&str]) -> Result<GitRun> {
	let mut argv = vec!["git"];
	argv.extend_from_slice(args);
	let options = RunOptions { cwd: Some(cwd.to_path_buf()), ..RunOptions::default() };
	let result = run_program_sync(&argv, options)?;
	Ok(GitRun { status: result.code, stdout: result.stdout })
}

/// Whether two branches that merge cleanly break the fixture's tests together (0576): the merged tree as a commit,
/// checked out in a temp worktree, and `bun test` run there; false when they conflict or the tree has no tests.
pub fn merged_tests_fail(
	git: &dyn Fn(&Path, &[&str]) -> Result<GitRun>,
	repo: &Path,
	a: &str,
	b: &str,
	bun: &str,
) -> Result<bool> {
	let merged = git(repo, &["merge-tree", "--write-tree", a, b])?;
	if merged.status != Some(0) {
		return Ok(false);
	}
	let tree = merged.stdout.split('\n').next().unwrap_or("").trim().to_owned();
	let commit = git(
		repo,
		&[
			"-c",
			"user.name=Bench",
			"-c",
			"user.email=bench@example.org",
			"commit-tree",
			&tree,
			"-p",
			a,
			"-p",
			b,
			"-m",
			"h8 merged pair",
		],
	)?
	.stdout
	.trim()
	.to_owned();
	let parent = mkdtemp("h8-merge-")?;
	let dir = parent.join("tree");
	let dir_text = dir.to_string_lossy().into_owned();
	git(repo, &["worktree", "add", "--detach", &dir_text, &commit])?;
	let failed = (|| {
		if !dir.join("test").exists() {
			return false;
		}
		let options = RunOptions { cwd: Some(dir.clone()), timeout_ms: Some(120_000), ..RunOptions::default() };
		// A program that couldn't start leaves no status, as the TypeScript's `spawnSync` did: not a pass.
		run_program_sync(&[bun, "test"], options).map_or(true, |run| run.code != Some(0))
	})();
	git(repo, &["worktree", "remove", "--force", &dir_text])?;
	remove_tree(&parent)?;
	Ok(failed)
}

/// `Math.max(...counts)` as the plan prints it.
fn max_text(counts: &[u64]) -> String {
	counts.iter().max().map_or_else(|| "-Infinity".to_owned(), u64::to_string)
}

/// A number as JavaScript prints it in a template.
fn js_display(value: f64) -> String {
	if value == f64::INFINITY {
		"Infinity".to_owned()
	} else if value == f64::NEG_INFINITY {
		"-Infinity".to_owned()
	} else {
		js_text(value)
	}
}

fn now_ms() -> i64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn layer_text(layer: OnOff) -> &'static str {
	match layer {
		OnOff::On => "on",
		OnOff::Off => "off",
	}
}

fn plan(args: &BenchArgs, counts: &[u64], runs: u64, layer: Option<OnOff>, timeout_ms: f64) {
	let project = flag(args, "project");
	let repo_flag = flag(args, "repo");
	let make_fixture = args.flags.get("make-fixture") == Some(&FlagValue::Set);
	let biggest = counts.iter().max().copied().unwrap_or(0) as usize;
	let mut lines = vec![
		if make_fixture {
			format!("write the fixture into {} for up to {} threads", repo_flag.unwrap_or("<--repo>"), max_text(counts))
		} else {
			"use the existing fixture".to_owned()
		},
		format!(
			"{} threads × {runs} run(s) in {} via jocasta thread new (no Allow; model calls; he approves each plan: VM only)",
			counts.iter().map(u64::to_string).collect::<Vec<_>>().join(", "),
			project.unwrap_or("<--project>")
		),
	];
	lines.extend(assignments(biggest.min(4), 0).iter().map(|a| format!("thread {}: {}", a.thread, a.brief)));
	lines.push(format!(
		"read jocasta thread list up to {} min until all are idle or done with a commit on their branch",
		js_display(timeout_ms / 60_000.0)
	));
	lines.push("measure wrong-thread edits, overlaps flagged before merge, merge-tree conflicts, semantic failures (a clean merge whose tree fails bun test), conflicts found only at merge".to_owned());
	if let Some(layer) = layer {
		let word = layer_text(layer);
		lines.push(format!(
			"K11 (0543): the layer {word} in the sandbox (the service started {} JOCASTA_LAYER=off; /health checked); cost per finished thread from every omp session of each thread since the run started at the pinned price table; peak memory of the service's, herdr's and the surfaces' trees every {} s",
			if layer == OnOff::Off { "with" } else { "without" },
			MEMORY_SAMPLE_MS / 1000
		));
		lines.push(
			if layer == OnOff::On {
				"shared check hits from the service's log in its data dir"
			} else {
				"no shared check hits with the layer off"
			}
			.to_owned(),
		);
	}
	print_plan("H8 parallel isolation", &lines, args.out.as_deref());
}

/// The peaks the sampler has seen so far, in bytes.
#[derive(Default)]
struct Peaks {
	working_set: Option<f64>,
	private: Option<f64>,
}

/// One sample of the memory of Jocasta's processes.
async fn sample(jocasta: &SandboxJocasta, peaks: &Mutex<Peaks>) -> Result<()> {
	let processes =
		tokio::task::spawn_blocking(snapshot_processes).await.map_err(|error| Error::other(error.to_string()))??;
	let roots = jocasta.processes(&processes, None).await?;
	let memory = tree_memory(&jocasta_processes(&roots, &processes).all);
	let mut peaks = peaks.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	peaks.working_set = Some(peaks.working_set.unwrap_or(0.0).max(memory.working_set as f64));
	if let Some(private) = memory.private_bytes {
		peaks.private = Some(peaks.private.unwrap_or(0.0).max(private as f64));
	}
	Ok(())
}

/// Inserts `(key, value)` into an ordered map kept as a list: a known key keeps its place.
fn put(map: &mut Vec<(String, String)>, key: String, value: String) {
	match map.iter_mut().find(|(known, _)| *known == key) {
		Some(entry) => entry.1 = value,
		None => map.push((key, value)),
	}
}

fn json_f64(value: f64) -> Value {
	json!(value)
}

/// What a live run needs: the flags, the sandbox and, with `--layer`, the price table.
struct Live {
	project: String,
	repo: PathBuf,
	out: PathBuf,
	counts: Vec<u64>,
	runs: u64,
	layer: Option<OnOff>,
	timeout_ms: f64,
	jocasta: Arc<SandboxJocasta>,
	prices: Option<&'static PriceTable>,
}

/// How many commits `branch` has beyond `main`; 0 when git says otherwise.
fn ahead(repo: &Path, branch: &str) -> Result<u64> {
	let count = run_git(repo, &["rev-list", "--count", &format!("main..{branch}")])?;
	Ok(count.stdout.trim().parse().unwrap_or(0))
}

fn finished_count(repo: &Path, lines: &[ThreadLine]) -> Result<usize> {
	let mut done = 0;
	for line in lines {
		if finished(line, ahead(repo, &line.branch)?) {
			done += 1;
		}
	}
	Ok(done)
}

/// `--make-fixture`: writes the fixture into the repo and commits it.
fn make_fixture(project: &str, repo: &Path, counts: &[u64]) -> Result<()> {
	let biggest = counts.iter().max().copied().unwrap_or(0);
	for (path, text) in fixture_files(biggest) {
		let file = repo.join(&path);
		if let Some(parent) = file.parent() {
			fs::create_dir_all(parent)?;
		}
		fs::write(&file, text)?;
	}
	for words in [
		vec!["init", "-q", "-b", "main"],
		vec!["add", "-A"],
		vec!["-c", "user.name=Bench", "-c", "user.email=bench@example.org", "commit", "-q", "-m", "h8 fixture"],
	] {
		run_git(repo, &words)?;
	}
	println!(
		"fixture in {}; add it as a sandbox project named {project}, then run again without --make-fixture",
		repo.display()
	);
	Ok(())
}

/// Samples the memory of Jocasta's processes every 10 s until `sampling` is cleared.
fn start_sampler(
	jocasta: &Arc<SandboxJocasta>,
	layer: Option<OnOff>,
) -> (Arc<Mutex<Peaks>>, Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
	let peaks = Arc::new(Mutex::new(Peaks::default()));
	let sampling = Arc::new(AtomicBool::new(layer.is_some()));
	let sampler = tokio::spawn({
		let (jocasta, peaks, sampling) = (jocasta.clone(), peaks.clone(), sampling.clone());
		async move {
			while sampling.load(Ordering::SeqCst) {
				if let Err(error) = sample(&jocasta, &peaks).await {
					// One missed sample lowers the peak's resolution, not the run.
					eprintln!("memory sample failed: {error}");
				}
				tokio::time::sleep(Duration::from_millis(MEMORY_SAMPLE_MS)).await;
			}
		}
	});
	(peaks, sampling, sampler)
}

/// Starts the run's threads and reads `thread list` until all are finished or the time is up.
async fn run_threads(live: &Live, work: &[Assignment], n: u64, run: u64) -> Result<Vec<ThreadLine>> {
	let starts = futures_util::future::join_all(
		work.iter().map(|assignment| start_thread(&live.project, &assignment.title, &assignment.brief)),
	)
	.await;
	for start in starts {
		start?;
	}
	let deadline = now_ms() as f64 + live.timeout_ms;
	loop {
		let lines: Vec<ThreadLine> = list_threads(Some(&live.project))
			.await?
			.into_iter()
			.filter(|line| work.iter().any(|a| a.title == line.title))
			.collect();
		let done = finished_count(&live.repo, &lines)?;
		if lines.len() == work.len() && done == work.len() {
			return Ok(lines);
		}
		if now_ms() as f64 > deadline {
			println!("run {run} with {n} threads: timed out with {done}/{} finished", work.len());
			return Ok(lines);
		}
		tokio::time::sleep(Duration::from_secs(10)).await;
	}
}

/// Each branch's worktree folder, from `git worktree list --porcelain`, in the order listed.
fn worktree_folders(repo: &Path) -> Result<Vec<(String, String)>> {
	let mut worktrees: Vec<(String, String)> = Vec::new();
	let mut current: Option<String> = None;
	for line in run_git(repo, &["worktree", "list", "--porcelain"])?.stdout.split('\n') {
		if let Some(path) = line.strip_prefix("worktree ") {
			current = Some(path.trim().to_owned());
		}
		if let (Some(branch), Some(path)) = (line.strip_prefix("branch refs/heads/"), &current) {
			put(&mut worktrees, branch.trim().to_owned(), path.clone());
		}
	}
	Ok(worktrees)
}

/// Each thread's changed files: its branch's commits since main plus its worktree's uncommitted files.
fn changed_files(
	repo: &Path,
	lines: &[ThreadLine],
	worktrees: &[(String, String)],
) -> Result<HashMap<String, Vec<String>>> {
	let mut changed: HashMap<String, Vec<String>> = HashMap::new();
	for line in lines {
		let committed = run_git(repo, &["diff", "--name-only", &format!("main...{}", line.branch)])?.stdout;
		let loose: Vec<String> = match worktrees.iter().find(|(branch, _)| *branch == line.branch) {
			Some((_, path)) => run_git(Path::new(path), &["status", "--porcelain"])?
				.stdout
				.split('\n')
				.map(|l| l.chars().skip(3).collect())
				.collect(),
			None => Vec::new(),
		};
		let mut files: Vec<String> = Vec::new();
		for file in committed.split('\n').map(str::to_owned).chain(loose) {
			let file = file.trim().to_owned();
			if !file.is_empty() && !files.contains(&file) {
				files.push(file);
			}
		}
		changed.insert(line.title.clone(), files);
	}
	Ok(changed)
}

/// Each pair's conflicts in `merge-tree` and its clean merges' semantic failures.
fn merge_checks(
	repo: &Path,
	work: &[Assignment],
	lines: &[ThreadLine],
) -> Result<(HashMap<usize, bool>, HashMap<usize, bool>)> {
	let mut conflicts: HashMap<usize, bool> = HashMap::new();
	let mut semantic: HashMap<usize, bool> = HashMap::new();
	let mut pair_order: Vec<usize> = Vec::new();
	for assignment in work {
		if !pair_order.contains(&assignment.pair) {
			pair_order.push(assignment.pair);
		}
	}
	for pair in pair_order {
		let branches: Vec<Option<&str>> = work
			.iter()
			.filter(|x| x.pair == pair)
			.map(|x| lines.iter().find(|l| l.title == x.title).map(|l| l.branch.as_str()))
			.collect();
		let (Some(Some(a)), Some(Some(b))) = (branches.first(), branches.get(1)) else { continue };
		if a.is_empty() || b.is_empty() {
			continue;
		}
		let conflict = run_git(repo, &["merge-tree", "--write-tree", a, b])?.status == Some(1);
		conflicts.insert(pair, conflict);
		let fails = !conflict && merged_tests_fail(&|cwd, words| run_git(cwd, words), repo, a, b, "bun")?;
		semantic.insert(pair, fails);
	}
	Ok((conflicts, semantic))
}

/// K11's measures of a finished run (0543): cost over the threads' sessions, peak memory and check hits.
async fn k11_measures(
	live: &Live,
	(layer, prices): (OnOff, &PriceTable),
	lines: &[ThreadLine],
	worktrees: &[(String, String)],
	(started_at, counted_before, peaks): (i64, Option<LayerCounters>, &Mutex<Peaks>),
) -> Result<K11Measures> {
	let finished_threads = finished_count(&live.repo, lines)?;
	let panes = live.jocasta.herdr().await?.panes().await?;
	let ours: Vec<(String, String)> =
		worktrees.iter().filter(|(branch, _)| lines.iter().any(|line| line.branch == *branch)).cloned().collect();
	let pricing = bench_pricing(prices);
	let records: Vec<_> = thread_session_files(&panes, &ours, started_at, None)
		.iter()
		.flat_map(|(_, files)| thread_session_records(files, &pricing))
		.collect();
	let ledger = trial_ledger(&records, &pricing);
	let before = counted_before;
	let after = before.as_ref().map(|_| read_layer_counters(Path::new(&live.jocasta.sandbox.data_dir)));
	let peaks = peaks.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	let mb = |bytes: f64| bytes / 2f64.powi(20);
	Ok(K11Measures {
		layer: layer_text(layer),
		finished_threads,
		cost_usd: ledger.totals.cost,
		cost_per_finished_thread: (finished_threads != 0).then(|| ledger.totals.cost / finished_threads as f64),
		unpriced_models: ledger.totals.unpriced_models.clone(),
		peak_working_set_mb: peaks.working_set.map(mb),
		peak_private_mb: peaks.private.map(mb),
		check_hits: before.zip(after).map(|(before, after)| after.check_hits as i64 - before.check_hits as i64),
	})
}

/// One run of `n` threads: starts them, waits, measures.
async fn measure_run(live: &Live, n: u64, run: u64) -> Result<RunRow> {
	let work = assignments(n as usize, run);
	let started_at = now_ms();
	let counted_before =
		(live.layer == Some(OnOff::On)).then(|| read_layer_counters(Path::new(&live.jocasta.sandbox.data_dir)));
	let (peaks, sampling, sampler) = start_sampler(&live.jocasta, live.layer);
	let lines = run_threads(live, &work, n, run).await?;
	let worktrees = worktree_folders(&live.repo)?;
	let changed = changed_files(&live.repo, &lines, &worktrees)?;
	let (conflicts, semantic) = merge_checks(&live.repo, &work, &lines)?;
	let main_changes =
		run_git(&live.repo, &["status", "--porcelain"])?.stdout.split('\n').filter(|l| !l.trim().is_empty()).count();
	let inputs = ScoreInputs {
		work: &work,
		lines: &lines,
		changed: &changed,
		conflicts: &conflicts,
		main_changes,
		semantic_failures: &semantic,
	};
	let mut result = RunRow { score: score(&inputs), run, k11: None };
	sampling.store(false, Ordering::SeqCst);
	let _ = sampler.await;
	if let (Some(layer), Some(prices)) = (live.layer, live.prices) {
		let since = (started_at, counted_before, &*peaks);
		result.k11 = Some(k11_measures(live, (layer, prices), &lines, &worktrees, since).await?);
	}
	Ok(result)
}

/// The layer-only fields of one thread count's summary.
fn layer_totals(row: &mut Map<String, Value>, layer: OnOff, measured: &[&K11Measures]) {
	let total_cost: f64 = measured.iter().map(|k| k.cost_usd).sum();
	let total_finished: usize = measured.iter().map(|k| k.finished_threads).sum();
	row.insert(
		"costPerFinishedThread".to_owned(),
		if total_finished == 0 { Value::Null } else { json_f64(total_cost / total_finished as f64) },
	);
	let peak = measured.iter().map(|k| k.peak_working_set_mb.unwrap_or(0.0)).fold(f64::NEG_INFINITY, f64::max);
	row.insert("peakWorkingSetMb".to_owned(), json_f64(peak));
	row.insert(
		"checkHits".to_owned(),
		if layer == OnOff::On {
			json!(measured.iter().map(|k| k.check_hits.unwrap_or(0)).sum::<i64>())
		} else {
			Value::Null
		},
	);
}

/// The summary of each thread count over its runs.
fn by_threads(counts: &[u64], scores: &[RunRow], layer: Option<OnOff>) -> Vec<Value> {
	counts
		.iter()
		.map(|&n| {
			let of: Vec<&RunRow> = scores.iter().filter(|s| s.score.threads == n as usize).collect();
			let measured: Vec<&K11Measures> = of.iter().filter_map(|s| s.k11.as_ref()).collect();
			let sum = |measure: fn(&RunScore) -> usize| of.iter().map(|s| measure(&s.score)).sum::<usize>();
			let mut row = Map::new();
			row.insert("threads".to_owned(), json!(n));
			row.insert("runs".to_owned(), json!(of.len()));
			row.insert("wrongThreadEdits".to_owned(), json!(sum(|s| s.wrong_thread_edits)));
			row.insert("mainCheckoutChanges".to_owned(), json!(sum(|s| s.main_checkout_changes)));
			row.insert("pairs".to_owned(), json!(of.iter().map(|s| s.score.pairs.len()).sum::<usize>()));
			row.insert("flaggedBeforeMerge".to_owned(), json!(sum(|s| s.flagged_before_merge)));
			row.insert("conflictsAtMerge".to_owned(), json!(sum(|s| s.conflicts_at_merge)));
			row.insert("semanticFailures".to_owned(), json!(sum(|s| s.semantic_failures)));
			row.insert("foundOnlyAtMerge".to_owned(), json!(sum(|s| s.found_only_at_merge)));
			if let Some(layer) = layer {
				layer_totals(&mut row, layer, &measured);
			}
			Value::Object(row)
		})
		.collect()
}

/// Every thread count and run; the rows and the summary.
async fn run_live(live: &Live) -> Result<()> {
	let started = conditions(&["omp", "herdr", "git", "jocasta"]);
	let mut scores: Vec<RunRow> = Vec::new();
	for &n in &live.counts {
		for run in 0..live.runs {
			let result = measure_run(live, n, run).await?;
			append_row(&live.out, "h8-parallel", &result)?;
			println!(
				"{n} threads, run {run}: {} wrong-thread edits, {} flagged, {} conflicts, {} semantic failures",
				result.score.wrong_thread_edits,
				result.score.flagged_before_merge,
				result.score.conflicts_at_merge,
				result.score.semantic_failures
			);
			scores.push(result);
			if run + 1 < live.runs || live.counts.last() != Some(&n) {
				// Stopping a thread and removing its worktree ask him first (0484); the bench leaves closing to him.
				wait_for_enter("close this run's h8 threads, then press Enter").await;
			}
		}
	}
	let file = write_json(
		&live.out,
		"h8-parallel",
		&json!({
			"benchmark": "H8",
			"layer": live.layer.map(layer_text),
			"conditions": started.finished(),
			"byThreads": by_threads(&live.counts, &scores, live.layer),
		}),
	)?;
	println!("wrote {}", file.display());
	Ok(())
}

async fn run(argv: Vec<String>) -> Result<()> {
	let spec = ArgSpec {
		default_runs: 1,
		valued: &["--project", "--repo", "--threads", "--timeout", "--layer"],
		booleans: &["--make-fixture"],
		..ArgSpec::default()
	};
	let args = parse_bench_args(&argv, &spec)?;
	let layer = match flag(&args, "layer") {
		None => None,
		Some("on") => Some(OnOff::On),
		Some("off") => Some(OnOff::Off),
		Some(other) => return Err(BenchUsageError::err(format!("--layer is on or off, not \"{other}\""))),
	};
	let counts = number_list_flag(&args, "threads", if layer.is_some() { &[K11_THREADS] } else { &[2, 4, 8] })?;
	let runs = if layer.is_some() && !argv.iter().any(|word| word == "--runs") { K11_RUNS } else { args.runs };
	let timeout_ms = number_of(flag_or(&args, "timeout", "60")) * 60_000.0;
	if args.dry_run {
		plan(&args, &counts, runs, layer, timeout_ms);
		return Ok(());
	}
	let out = args.out.clone().expect("parse_bench_args requires --out outside a dry run");
	let given = |name: &str| flag(&args, name).filter(|text| !text.is_empty());
	let (Some(project), Some(repo_flag)) = (given("project"), given("repo")) else {
		return Err(BenchUsageError::err("--project and --repo are required"));
	};
	let repo = resolve(Path::new(repo_flag));
	if args.flags.get("make-fixture") == Some(&FlagValue::Set) {
		return make_fixture(project, &repo, &counts);
	}
	let jocasta = Arc::new(open_sandbox(None)?);
	claim_rows(&out, &["h8-parallel"])?;
	if let Some(layer) = layer {
		require_service_switch(Switch::Layer, layer).await?;
	}
	let prices = if layer.is_some() { Some(load_price_table()?) } else { None };
	let live = Live { project: project.to_owned(), repo, out, counts, runs, layer, timeout_ms, jocasta, prices };
	run_live(&live).await
}

pub async fn main(argv: Vec<String>) -> ExitCode {
	run_script(run(argv)).await
}

#[cfg(test)]
#[path = "h8_parallel_tests.rs"]
mod tests;
