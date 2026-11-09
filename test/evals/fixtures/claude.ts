// A stand-in for Claude Code's `claude`: `--version` prints a version; otherwise `-p <prompt>` prints canned
// `--output-format stream-json` lines (init, one assistant message with a tool_use, the result with usage and cost).
// A prompt `Write <file>: <text>` writes that file in the cwd. Each run appends its argv to `JOCASTA_TEST_OMP_LOG`.
import { appendFileSync, writeFileSync } from "node:fs";

const argv = process.argv.slice(2);
if (argv.includes("--version")) {
	console.log("2.1.296 (Claude Code)");
	process.exit(0);
}
const prompt = argv[argv.indexOf("-p") + 1] ?? "";
if (process.env.JOCASTA_TEST_OMP_LOG) appendFileSync(process.env.JOCASTA_TEST_OMP_LOG, `${JSON.stringify({ argv })}\n`);
const write = /^Write (\S+): (.*)$/.exec(prompt);
if (write) writeFileSync(write[1] ?? "", `${write[2]}\n`);
for (const line of [
	{ type: "system", subtype: "init", model: "claude-stub" },
	{
		type: "assistant",
		message: {
			model: "claude-stub",
			content: [
				{ type: "tool_use", id: "t1", name: "Write", input: {} },
				{ type: "text", text: "Done." },
			],
		},
	},
	{
		type: "result",
		subtype: "success",
		num_turns: 2,
		total_cost_usd: 0.5,
		usage: { input_tokens: 10, output_tokens: 20, cache_read_input_tokens: 300, cache_creation_input_tokens: 40 },
	},
])
	console.log(JSON.stringify(line));
