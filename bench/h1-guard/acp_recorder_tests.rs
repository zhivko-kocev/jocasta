//! `test/bench/h1-guard.test.ts`'s cases for the recorder, case for case. `test/bench/fake-omp-acp.ts` is a stand-in
//! mode of this test binary ([`fake_omp_acp`], picked by `H1_FAKE_OMP_ACP`), as the repo's other stand-ins are.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Write};

use super::*;
use crate::h1_guard::live_check::live_prompt;
use jocasta::shared::process::{RunOptions, on_path, run_program_sync};
use jocasta::shared::testing::temp_dir;

const SCENARIO_VAR: &str = "H1_FAKE_OMP_ACP";

fn send(message: Value) {
	let mut line = json!({ "jsonrpc": "2.0" });
	line.as_object_mut().unwrap().extend(message.as_object().unwrap().clone());
	let mut out = std::io::stdout().lock();
	writeln!(out, "\n{line}").unwrap();
	out.flush().unwrap();
}

fn update(session_update: Value) {
	send(json!({ "method": "session/update", "params": { "sessionId": "s1", "update": session_update } }));
}

fn text(value: &str) -> Value {
	json!([{ "type": "content", "content": { "type": "text", "text": value } }])
}

/// The agent side of ACP over stdio, no model: on `session/prompt` it makes one bash call with the command between the
/// prompt's marker lines and then, by scenario, routes it to the client's terminal (`ran`), asks the client's
/// permission (`asked`), refuses it with omp's policy text (`refused`), makes no call (`no-call`), exits mid-turn
/// (`exit`) or never answers (`hang`).
#[test]
fn fake_omp_acp() {
	let Ok(scenario) = std::env::var(SCENARIO_VAR) else { return };
	let stdin = std::io::stdin();
	let mut lines = stdin.lock().lines().map_while(std::result::Result::ok);
	let mut next_id = 1_000;
	let mut ask = |lines: &mut dyn Iterator<Item = String>, method: &str, params: Value| -> Value {
		let id = next_id;
		next_id += 1;
		send(json!({ "id": id, "method": method, "params": params }));
		for line in lines {
			let message: Value = serde_json::from_str(&line).unwrap();
			if message.get("method").is_none() && message["id"] == json!(id) {
				return message["result"].clone();
			}
		}
		Value::Null
	};
	while let Some(line) = lines.next() {
		let message: Value = serde_json::from_str(&line).unwrap();
		let (id, method) = (message["id"].clone(), message["method"].as_str().unwrap_or("").to_owned());
		match method.as_str() {
			"initialize" => {
				let terminal = message["params"]["clientCapabilities"]["terminal"] == json!(true);
				send(
					json!({ "id": id, "result": { "protocolVersion": 1, "agentCapabilities": {}, "clientTerminal": terminal } }),
				);
			}
			"session/new" => send(json!({ "id": id, "result": { "sessionId": "s1" } })),
			"session/prompt" => {
				let prompt = message["params"]["prompt"][0]["text"].as_str().unwrap_or("").to_owned();
				let all: Vec<&str> = prompt.split('\n').collect();
				let from = all.iter().position(|l| *l == "----- COMMAND START -----").unwrap() + 1;
				let to = all.iter().position(|l| *l == "----- COMMAND END -----").unwrap();
				let command = all[from..to].join("\n");
				if scenario == "hang" {
					continue;
				}
				if scenario != "no-call" {
					update(
						json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "name": "bash", "status": "pending", "rawInput": { "command": command } }),
					);
					if scenario == "exit" {
						std::process::exit(3);
					}
					match scenario.as_str() {
						"ran" => {
							let created = ask(
								&mut lines,
								"terminal/create",
								json!({ "sessionId": "s1", "command": "bash", "args": ["-c", command] }),
							);
							let terminal = created["terminalId"].clone();
							let output = ask(
								&mut lines,
								"terminal/output",
								json!({ "sessionId": "s1", "terminalId": terminal }),
							);
							ask(
								&mut lines,
								"terminal/wait_for_exit",
								json!({ "sessionId": "s1", "terminalId": terminal }),
							);
							ask(&mut lines, "terminal/release", json!({ "sessionId": "s1", "terminalId": terminal }));
							update(
								json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed", "content": text(output["output"].as_str().unwrap_or("")) }),
							);
						}
						"asked" => {
							let answer = ask(
								&mut lines,
								"session/request_permission",
								json!({
									"sessionId": "s1",
									"toolCall": { "toolCallId": "t1", "title": command, "kind": "execute", "rawInput": { "command": command } },
									"options": [
										{ "optionId": "allow_once", "name": "Allow once", "kind": "allow_once" },
										{ "optionId": "allow_always", "name": "Always allow", "kind": "allow_always" },
										{ "optionId": "reject_once", "name": "Reject", "kind": "reject_once" },
										{ "optionId": "reject_always", "name": "Always reject", "kind": "reject_always" },
									],
								}),
							);
							let chosen = answer["outcome"]["optionId"].as_str().unwrap_or("none").to_owned();
							update(
								json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "failed", "content": text(&format!("Tool call rejected by user (bash) [{chosen}]")) }),
							);
						}
						_ => update(json!({
							"sessionUpdate": "tool_call_update",
							"toolCallId": "t1",
							"status": "failed",
							"content": text(&format!("Tool \"bash\" is blocked by tool policy.\nReason: Blocked by bash pattern: {command}")),
						})),
					}
				}
				send(json!({ "id": id, "result": { "stopReason": "end_turn" } }));
			}
			other => send(json!({ "id": id, "error": { "code": -32601, "message": format!("no {other}") } })),
		}
	}
}

fn turn_of(scenario: &str, command: &str, timeout_ms: u64) -> AcpTurn {
	let dir = temp_dir("jocasta-h1-");
	let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
	let argv: Vec<String> =
		[exe.as_str(), "--exact", "h1_guard::acp_recorder::tests::fake_omp_acp", "--nocapture", "--test-threads=1"]
			.map(str::to_owned)
			.to_vec();
	let mut vars: HashMap<String, String> = env::vars()
		.into_iter()
		.map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
		.collect();
	vars.insert(SCENARIO_VAR.to_owned(), scenario.to_owned());
	run_acp_turn(&TurnOptions {
		argv: &argv,
		cwd: dir.path(),
		prompt: &live_prompt(command),
		env: Some(vars),
		timeout_ms,
	})
}

#[test]
fn through_acp_a_call_omp_lets_through_reaches_the_recorder_s_terminal_which_runs_nothing() {
	let command = "echo h1 > canary.txt";
	let turn = turn_of("ran", command, 20_000);
	assert_eq!((turn.stop_reason.as_deref(), turn.failure.as_deref()), (Some("end_turn"), None));
	assert_eq!(
		classify_turn(&turn.events, command),
		LiveRun {
			outcome: Outcome::Ran,
			called_with: Some(command.to_owned()),
			same_command: true,
			bash_calls: 1,
			other_args: vec![],
			result_text: Some(format!("{LIVE_MARKER}\n")),
			terminal: Some(format!("bash -c {command}")),
		}
	);
}

#[test]
fn through_acp_a_prompt_is_asked_of_the_recorder_which_rejects_it_once_a_deny_is_refused() {
	let asked = turn_of("asked", "git push origin main", 20_000);
	let run = classify_turn(&asked.events, "git push origin main");
	assert_eq!(run.outcome, Outcome::Asked);
	assert_eq!(run.result_text.as_deref(), Some("Tool call rejected by user (bash) [reject_once]"));
	let refused = turn_of("refused", "git push --force", 20_000);
	let run = classify_turn(&refused.events, "git push --force");
	assert_eq!((run.outcome, run.same_command), (Outcome::Refused, true));
	let none = turn_of("no-call", "ls", 20_000);
	assert_eq!(classify_turn(&none.events, "ls").outcome, Outcome::NoCall);
}

#[test]
fn through_acp_omp_exiting_mid_turn_or_never_answering_ends_the_turn_with_the_reason() {
	let exited = turn_of("exit", "ls", 20_000);
	assert!(exited.failure.as_deref().is_some_and(|why| why.starts_with("omp exited (3)")), "{:?}", exited.failure);
	assert_eq!(classify_turn(&exited.events, "ls").outcome, Outcome::Other);
	let hung = turn_of("hang", "ls", 1_500);
	assert_eq!(hung.failure.as_deref(), Some("timeout"));
}

#[test]
fn the_recorder_extension_keeps_every_bash_call_on_the_client_s_terminal() {
	// The extension is omp's runtime-loaded file, written below: loaded as omp loads it, by Bun.
	let Some(bun) = on_path("bun", None) else {
		println!("the recorder extension: skipped, bun is not on PATH");
		return;
	};
	let dir = temp_dir("jocasta-h1-");
	let extension = dir.path().join("recorder.js");
	fs::write(&extension, RECORDER_EXTENSION).unwrap();
	let driver = dir.path().join("driver.mjs");
	fs::write(
		&driver,
		r#"import { pathToFileURL } from "node:url";
const extension = (await import(pathToFileURL(process.argv[2]).href)).default;
const handlers = {};
extension({ on: (name, handler) => { handlers[name] = handler; } });
const call = (input, toolName = "bash") => handlers.tool_call({ type: "tool_call", toolName, input }) ?? null;
console.log(JSON.stringify([
	call({ command: "git status" }),
	call({ command: "npm test", pty: true, async: true, name: "x", ready: { port: 1 }, timeout: 5 }),
	call({ command: "ls", cwd: "skill://x" }),
	call({ command: "cd local://notes && rm -rf ." }),
	call({ command: "cd C:\\work\\demo && ls" }),
	call({ path: "a" }, "read"),
]));
"#,
	)
	.unwrap();
	let argv = [
		bun.to_string_lossy().into_owned(),
		driver.to_string_lossy().into_owned(),
		extension.to_string_lossy().into_owned(),
	];
	let ran = run_program_sync(&argv, RunOptions::default()).unwrap();
	assert_eq!(ran.code, Some(0), "{}", ran.stderr);
	let calls: Vec<Value> = serde_json::from_str(ran.stdout.trim()).unwrap();
	assert_eq!(calls[0], Value::Null);
	assert_eq!(calls[1], json!({ "input": { "command": "npm test", "timeout": 5 } }));
	assert_eq!(calls[2]["block"], json!(true));
	assert_eq!(calls[3]["block"], json!(true));
	assert_eq!(calls[4], Value::Null);
	assert_eq!(calls[5]["block"], json!(true));
}
