//! Ported from the `shared flags` and `process trees` cases of `test/bench/scripts.test.ts`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use jocasta::shared::errors::{Error, Result};
use jocasta::shared::testing::temp_dir;

use super::args::{ArgSpec, REPO_ROOT, flag, parse_bench_args};
use super::jocasta::herdr_server_of;
use super::launch::{BenchTool, ready_patterns, require_trusted_cwd, tool_command_with};
use super::output::claim_rows;
use super::proctree::{
	JocastaRoots, Memory, ProcessInfo, jocasta_processes, parse_ps_processes, parse_windows_processes, tree_memory,
	tree_of,
};
use super::sandbox::{Sandbox, require_sandbox_in, require_vm};

/// The message of a refusal (exit code 2).
fn refusal<T>(result: Result<T>) -> String {
	match result {
		Err(Error::User { message, exit_code }) => {
			assert_eq!(exit_code, 2);
			message
		}
		Err(other) => panic!("not a usage error: {other}"),
		Ok(_) => panic!("not refused"),
	}
}

fn words(items: &[&str]) -> Vec<String> {
	items.iter().map(|item| (*item).to_owned()).collect()
}

fn spec(default_runs: u64, valued: &'static [&'static str]) -> ArgSpec {
	ArgSpec { default_runs, valued, ..ArgSpec::default() }
}

fn text(path: &Path) -> String {
	path.to_string_lossy().into_owned()
}

#[test]
fn out_must_be_outside_the_repo_and_is_required_unless_dry_running_runs_is_a_whole_number() {
	let parsed = parse_bench_args(&words(&["--dry-run"]), &spec(5, &[])).unwrap();
	assert_eq!((parsed.runs, parsed.dry_run, parsed.out), (5, true, None));
	assert!(refusal(parse_bench_args(&[], &spec(1, &[]))).contains("--out <dir> is required"));
	let inside = text(&REPO_ROOT.join("x"));
	assert!(refusal(parse_bench_args(&words(&["--out", &inside]), &spec(1, &[]))).contains("outside the repo"));
	let tmp = text(&std::env::temp_dir());
	assert!(refusal(parse_bench_args(&words(&["--out", &tmp, "--runs", "0"]), &spec(1, &[]))).contains("--runs needs"));
	assert!(
		refusal(parse_bench_args(&words(&["--out", &tmp, "--nope"]), &spec(1, &[]))).contains("unknown option: --nope")
	);
	let args = parse_bench_args(&words(&["--out", &tmp, "--runs", "3", "--threads", "2,4"]), &spec(1, &["--threads"]))
		.unwrap();
	assert_eq!((args.runs, flag(&args, "threads")), (3, Some("2,4")));
}

#[test]
fn live_benchmarks_refuse_his_own_data_folder_herdr_session_and_config_folder_0576() {
	let home = std::env::home_dir().unwrap_or_default();
	let his = home.join("his-data");
	let sandbox: HashMap<String, String> =
		[("JOCASTA_DATA_DIR", "/sandbox"), ("JOCASTA_HERDR_SESSION", "bench"), ("JOCASTA_CODING_AGENT_DIR", "/sb-cfg")]
			.iter()
			.map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
			.collect();
	let with = |name: &str, value: Option<&str>| {
		let mut env = sandbox.clone();
		match value {
			Some(value) => env.insert(name.to_owned(), value.to_owned()),
			None => env.remove(name),
		};
		env
	};
	assert!(refusal(require_sandbox_in(&HashMap::new(), false, &his)).contains("JOCASTA_DATA_DIR"));
	assert!(
		refusal(require_sandbox_in(&with("JOCASTA_DATA_DIR", Some(&text(&his))), false, &his)).contains("not his own")
	);
	assert!(
		refusal(require_sandbox_in(&with("JOCASTA_HERDR_SESSION", Some("jocasta")), false, &his))
			.contains("not jocasta or jarvis")
	);
	let refused = "set JOCASTA_CODING_AGENT_DIR to the sandbox's config folder; with ~/.jocasta/agent the sandbox uses his projects and herdr config";
	assert!(refusal(require_sandbox_in(&with("JOCASTA_CODING_AGENT_DIR", None), false, &his)).contains(refused));
	assert!(
		refusal(require_sandbox_in(&with("JOCASTA_CODING_AGENT_DIR", Some("~/.jocasta/agent")), false, &his))
			.contains(refused)
	);
	// Windows compares folders without case.
	let shouting = text(&home.join(".JOCASTA").join("Agent"));
	assert!(
		refusal(require_sandbox_in(&with("JOCASTA_CODING_AGENT_DIR", Some(&shouting)), true, &his)).contains(refused)
	);
	assert_eq!(
		require_sandbox_in(&sandbox, false, &his).unwrap(),
		Sandbox {
			data_dir: "/sandbox".to_owned(),
			herdr_session: "bench".to_owned(),
			config_dir: "/sb-cfg".to_owned(),
		}
	);
}

#[test]
fn the_vm_only_scripts_refuse_any_other_machine_a_results_folder_is_claimed_once() {
	assert!(
		refusal(require_vm("HIS-PC", "install and uninstall Jocasta"))
			.contains("refusing to install and uninstall Jocasta on HIS-PC: only on the VM JOCASTA-CLEAN")
	);
	assert!(require_vm("jocasta-clean", "x").is_ok());
	let out = temp_dir("jocasta-bench");
	assert!(claim_rows(out.path(), &["h3-recovery"]).is_ok());
	fs::write(out.path().join("h3-recovery.jsonl"), "{}\n").unwrap();
	assert!(
		refusal(claim_rows(out.path(), &["h2-toast", "h3-recovery"])).contains("exists; results are never overwritten")
	);
}

#[test]
fn ready_replaces_one_tools_pattern() {
	assert_eq!(ready_patterns(Some("claude=ready>")).unwrap()[&BenchTool::Claude], "ready>");
	assert!(refusal(ready_patterns(Some("vim=x"))).contains("--ready takes tool=<regex>"));
}

#[test]
fn a_tool_starts_as_a_command_line_runs_it_an_npm_cmd_shim_for_an_exe_as_that_exe_other_shims_through_cmd() {
	let dir = temp_dir("jocasta-bench");
	let dir = dir.path();
	// npm's cmd-shim for a native binary (opencode's), and for a node script.
	fs::write(
		dir.join("opencode.cmd"),
		"@ECHO off\r\n\"%dp0%\\node_modules\\opencode-ai\\bin\\opencode.exe\"   %*\r\n",
	)
	.unwrap();
	fs::write(dir.join("claude.cmd"), "@ECHO off\r\n\"%_prog%\"  \"%dp0%\\node_modules\\cli\\cli.js\" %*\r\n").unwrap();
	let found: HashMap<&str, PathBuf> = HashMap::from([
		("opencode", dir.join("opencode.cmd")),
		("claude", dir.join("claude.cmd")),
		("omp", dir.join("omp.exe")),
	]);
	let which = |name: &str| found.get(name).cloned();
	assert_eq!(
		tool_command_with(BenchTool::Opencode, &which).unwrap(),
		vec![text(&dir.join("node_modules").join("opencode-ai").join("bin").join("opencode.exe"))]
	);
	assert_eq!(
		tool_command_with(BenchTool::Claude, &which).unwrap(),
		vec!["cmd.exe".to_owned(), "/d".to_owned(), "/c".to_owned(), text(&dir.join("claude.cmd"))]
	);
	assert_eq!(tool_command_with(BenchTool::Omp, &which).unwrap(), vec![text(&dir.join("omp.exe"))]);
	assert!(refusal(tool_command_with(BenchTool::Omp, &|_| None)).contains("omp is not on PATH"));
}

#[test]
fn with_claude_code_in_the_tools_a_bench_pane_works_only_in_a_cwd_folder_a_fresh_one_asks_for_trust() {
	assert!(refusal(require_trusted_cwd(&["omp", "claude"], None)).contains("pass --cwd"));
	assert!(require_trusted_cwd(&["omp", "claude"], Some("C:\\bench")).is_ok());
	assert!(require_trusted_cwd(&["omp", "opencode"], None).is_ok());
}

fn listed() -> Vec<ProcessInfo> {
	parse_windows_processes(
		r#"[
			{"ProcessId":1,"ParentProcessId":0,"Name":"explorer.exe","WorkingSetSize":100,"PrivatePageCount":50},
			{"ProcessId":10,"ParentProcessId":1,"Name":"omp.exe","WorkingSetSize":1000,"PrivatePageCount":800},
			{"ProcessId":11,"ParentProcessId":10,"Name":"bun.exe","WorkingSetSize":500,"PrivatePageCount":400},
			{"ProcessId":12,"ParentProcessId":11,"Name":"omp.exe","WorkingSetSize":200,"PrivatePageCount":100},
			{"ProcessId":20,"ParentProcessId":1,"Name":"herdr.exe","WorkingSetSize":300,"PrivatePageCount":null}
		]"#,
	)
	.unwrap()
}

#[test]
fn a_tree_its_memory_and_roots_by_name() {
	let processes = listed();
	let pids: Vec<u32> = tree_of(10, &processes).iter().map(|p| p.pid).collect();
	assert_eq!(pids, [10, 11, 12]);
	assert_eq!(
		tree_memory(&tree_of(10, &processes)),
		Memory { processes: 3, working_set: 1700, private_bytes: Some(1300) }
	);
	assert_eq!(tree_memory(&tree_of(20, &processes)).private_bytes, None);
	assert_eq!(parse_windows_processes(r#"{"ProcessId":5,"ParentProcessId":1,"Name":"a"}"#).unwrap().len(), 1);
}

#[test]
fn jocastas_processes_the_service_herdrs_server_and_the_attached_surfaces_each_once_and_apart_0552() {
	let proc = |pid: u32, ppid: u32, working_set: u64| ProcessInfo {
		pid,
		ppid,
		name: "p".to_owned(),
		command_line: "p".to_owned(),
		working_set,
		private_bytes: Some(working_set),
		created: None,
	};
	// The surface (30) started the service (31, with a child 32) and its herdr client (33); herdr's server (40) runs
	// the panes (41).
	let list =
		[proc(30, 1, 10), proc(31, 30, 100), proc(32, 31, 5), proc(33, 30, 20), proc(40, 1, 50), proc(41, 40, 7)];
	let roots = JocastaRoots { service: Some(31), herdr_server: Some(40), surfaces: vec![30, 99] };
	let found = jocasta_processes(&roots, &list);
	let mut pids: Vec<u32> = found.all.iter().map(|p| p.pid).collect();
	pids.sort_unstable();
	assert_eq!(pids, [30, 31, 32, 33, 40, 41]);
	assert_eq!(
		[found.parts.service.working_set, found.parts.herdr.working_set, found.parts.surfaces.working_set],
		[105, 57, 30]
	);
}

#[test]
fn herdrs_server_is_the_parent_of_the_first_pane_process_that_is_running_ben_18() {
	let list = parse_windows_processes(
		r#"[{"ProcessId":40,"ParentProcessId":1,"Name":"herdr.exe"},{"ProcessId":41,"ParentProcessId":40,"Name":"omp.exe"}]"#,
	)
	.unwrap();
	// Pane 77's process is gone; pane 41's names the server.
	assert_eq!(herdr_server_of([77, 41], &list), Some(40));
	assert_eq!(herdr_server_of([77], &list), None);
	assert_eq!(herdr_server_of([], &list), None);
}

#[test]
fn ps_output() {
	assert_eq!(
		parse_ps_processes("  10     1  2048 omp omp --mode json\n"),
		vec![ProcessInfo {
			pid: 10,
			ppid: 1,
			working_set: 2048 * 1024,
			name: "omp".to_owned(),
			command_line: "omp --mode json".to_owned(),
			private_bytes: None,
			created: None,
		}]
	);
}
