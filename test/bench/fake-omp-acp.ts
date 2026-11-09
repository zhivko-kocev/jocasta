// A stand-in for `omp --mode acp` in the H1 recorder's tests (0585): the agent side of ACP over stdio, no model.
// `bun fake-omp-acp.ts <scenario>`: on `session/prompt` it makes one bash call with the command between the prompt's
// marker lines and then, by scenario, routes it to the client's terminal (`ran`), asks the client's permission
// (`asked`), refuses it with omp's policy text (`refused`), makes no call (`no-call`), exits mid-turn (`exit`) or never
// answers (`hang`).
const scenario = process.argv[2] ?? "ran";
const send = (message: Record<string, unknown>) =>
	process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
const waiting = new Map<number, (result: Record<string, unknown>) => void>();
let nextId = 1_000;
const ask = (method: string, params: unknown) => {
	const id = nextId++;
	const { promise, resolve } = Promise.withResolvers<Record<string, unknown>>();
	waiting.set(id, resolve);
	send({ id, method, params });
	return promise;
};
const update = (sessionUpdate: Record<string, unknown>) =>
	send({ method: "session/update", params: { sessionId: "s1", update: sessionUpdate } });
const text = (value: string) => [{ type: "content", content: { type: "text", text: value } }];

async function prompt(id: number, promptText: string): Promise<void> {
	const lines = promptText.split("\n");
	const command = lines
		.slice(lines.indexOf("----- COMMAND START -----") + 1, lines.indexOf("----- COMMAND END -----"))
		.join("\n");
	if (scenario === "hang") return;
	if (scenario !== "no-call") {
		update({ sessionUpdate: "tool_call", toolCallId: "t1", name: "bash", status: "pending", rawInput: { command } });
		if (scenario === "exit") process.exit(3);
		if (scenario === "ran") {
			const created = await ask("terminal/create", { sessionId: "s1", command: "bash", args: ["-c", command] });
			const terminalId = created.terminalId;
			const output = await ask("terminal/output", { sessionId: "s1", terminalId });
			await ask("terminal/wait_for_exit", { sessionId: "s1", terminalId });
			await ask("terminal/release", { sessionId: "s1", terminalId });
			update({
				sessionUpdate: "tool_call_update",
				toolCallId: "t1",
				status: "completed",
				content: text(String(output.output)),
			});
		} else if (scenario === "asked") {
			const answer = await ask("session/request_permission", {
				sessionId: "s1",
				toolCall: { toolCallId: "t1", title: command, kind: "execute", rawInput: { command } },
				options: [
					{ optionId: "allow_once", name: "Allow once", kind: "allow_once" },
					{ optionId: "allow_always", name: "Always allow", kind: "allow_always" },
					{ optionId: "reject_once", name: "Reject", kind: "reject_once" },
					{ optionId: "reject_always", name: "Always reject", kind: "reject_always" },
				],
			});
			const chosen = (answer.outcome as { optionId?: string } | undefined)?.optionId ?? "none";
			update({
				sessionUpdate: "tool_call_update",
				toolCallId: "t1",
				status: "failed",
				content: text(`Tool call rejected by user (bash) [${chosen}]`),
			});
		} else {
			update({
				sessionUpdate: "tool_call_update",
				toolCallId: "t1",
				status: "failed",
				content: text(`Tool "bash" is blocked by tool policy.\nReason: Blocked by bash pattern: ${command}`),
			});
		}
	}
	send({ id, result: { stopReason: "end_turn" } });
}

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk: string) => {
	buffer += chunk;
	for (let at = buffer.indexOf("\n"); at >= 0; at = buffer.indexOf("\n")) {
		const message = JSON.parse(buffer.slice(0, at)) as Record<string, unknown>;
		buffer = buffer.slice(at + 1);
		const id = message.id as number;
		if (typeof message.method !== "string") {
			waiting.get(id)?.(message.result as Record<string, unknown>);
			waiting.delete(id);
			continue;
		}
		const params = message.params as Record<string, unknown>;
		if (message.method === "initialize") {
			const terminal = (params.clientCapabilities as { terminal?: boolean } | undefined)?.terminal === true;
			send({ id, result: { protocolVersion: 1, agentCapabilities: {}, clientTerminal: terminal } });
		} else if (message.method === "session/new") send({ id, result: { sessionId: "s1" } });
		else if (message.method === "session/prompt")
			void prompt(id, ((params.prompt as { text: string }[])[0] as { text: string }).text);
		else send({ id, error: { code: -32601, message: `no ${message.method}` } });
	}
});
