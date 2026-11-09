// A stand-in for `omp`: `--version` prints a version; otherwise it prints canned `--mode json` lines (the session
// header, a `task` tool's result with one subagent's usage, two assistant `message_end` turns). A prompt
// `Write <file>: <text>` writes that file in the cwd; a prompt starting `Fail` exits 1; a prompt starting `Review`
// answers with two findings, `[P1]` and `[P3]`. Each run appends its argv, its `--config` overlay's contents, its cwd,
// git branch and `--append-system-prompt` file's text to `JOCASTA_TEST_OMP_LOG` when set. With `--session-dir <dir>` it
// also writes a session file there
// (one priced request on `claude-opus-5-5`, one `read` call and its result), as omp keeps one per session.
import { spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const argv = process.argv.slice(2);
if (argv.includes("--version")) {
	console.log("omp/0.0.0-stub");
	process.exit(0);
}
const after = (flag: string) => {
	const i = argv.indexOf(flag);
	return i === -1 ? undefined : argv[i + 1];
};
const prompt = after("-p") ?? "";
const config = after("--config");
if (process.env.JOCASTA_TEST_OMP_LOG) {
	const overlay = config === undefined ? undefined : JSON.parse(readFileSync(config, "utf8"));
	const branch = spawnSync("git", ["branch", "--show-current"], { encoding: "utf8", windowsHide: true }).stdout.trim();
	const appended = after("--append-system-prompt");
	appendFileSync(
		process.env.JOCASTA_TEST_OMP_LOG,
		`${JSON.stringify({ argv, overlay, cwd: process.cwd(), branch, appendedText: appended === undefined ? undefined : readFileSync(appended, "utf8") })}\n`,
	);
}
const write = /^Write (\S+): (.*)$/.exec(prompt);
if (write) writeFileSync(write[1] ?? "", `${write[2]}\n`);

const usage = (input: number, total: number) => ({
	input,
	output: 10,
	cacheRead: 400,
	cacheWrite: 0,
	totalTokens: input + 410,
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total },
});
const lines = [
	{ type: "session", version: 3, id: "stub-session", cwd: process.cwd() },
	{ type: "agent_start" },
	{ type: "message_end", message: { role: "user", content: [{ type: "text", text: prompt }] } },
	{
		type: "message_end",
		message: {
			role: "assistant",
			content: [{ type: "toolCall", id: "call-1", name: "write", arguments: { path: write?.[1] ?? "" } }],
			provider: "anthropic",
			model: "stub-model",
			usage: usage(500, 0.5),
		},
	},
	{
		type: "tool_execution_end",
		toolCallId: "call-2",
		toolName: "task",
		result: { content: [], details: { results: [{ requests: 2 }], usage: usage(100, 0.25) } },
	},
	{
		type: "message_end",
		message: {
			role: "assistant",
			content: [
				{ type: "text", text: prompt.startsWith("Review") ? "[P1] a.ts:1 wrong sum\n[P3] b.ts:2 naming" : "Done." },
			],
			provider: "anthropic",
			model: "stub-model",
			usage: usage(600, 0.5),
		},
	},
	{ type: "agent_end", messages: [] },
];
for (const line of lines) console.log(JSON.stringify(line));
const sessionDir = after("--session-dir");
if (sessionDir !== undefined) {
	mkdirSync(sessionDir, { recursive: true });
	const at = "2026-11-09T10:00:00.000Z";
	const entries = [
		{ type: "session", version: 3, id: "stub-session", timestamp: at, cwd: process.cwd() },
		{
			type: "message",
			id: "m1",
			timestamp: at,
			message: {
				role: "assistant",
				provider: "anthropic",
				model: "claude-opus-5-5",
				content: [{ type: "toolCall", id: "c1", name: "read", arguments: { path: "lib.txt", i: "Reading lib" } }],
				usage: { input: 1000, output: 100, cacheRead: 0, cacheWrite: 0, totalTokens: 1100 },
			},
		},
		{
			type: "message",
			id: "m2",
			timestamp: at,
			message: { role: "toolResult", toolCallId: "c1", toolName: "read", content: [{ type: "text", text: "hello" }] },
		},
	];
	appendFileSync(join(sessionDir, "stub-session.jsonl"), `${entries.map((e) => JSON.stringify(e)).join("\n")}\n`);
}
if (prompt.startsWith("Fail")) process.exit(1);
