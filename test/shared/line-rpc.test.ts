import { describe, expect, test } from "bun:test";
import { LineRpc, LineSplitter } from "../../src/shared/line-rpc.ts";

describe("LineSplitter", () => {
	test("returns the lines a chunk completes, trimmed, and keeps a torn line for the next chunk", () => {
		const lines = new LineSplitter();
		expect(lines.push('{"a":1}\r\n  b  \n{"c"')).toEqual(['{"a":1}', "b"]);
		expect(lines.push(":2}")).toEqual([]);
		expect(lines.push("\n\n")).toEqual(['{"c":2}', ""]);
	});
});

interface Line {
	id?: number;
	echo?: string;
	hello?: boolean;
}

/**
 * A child that says hello, writes a non-JSON line, answers requests in reverse order once two arrived, never answers
 * `silent`, and exits 3 with a stderr line on `die`.
 */
const CHILD = `
process.stdout.write('{"hello":true}\\nnot json\\n');
let buffered = "";
const held = [];
process.stdin.on("data", (chunk) => {
	buffered += chunk;
	for (let end = buffered.indexOf("\\n"); end >= 0; end = buffered.indexOf("\\n")) {
		const message = JSON.parse(buffered.slice(0, end));
		buffered = buffered.slice(end + 1);
		if (message.echo === "die") {
			process.stderr.write("first\\nlast words\\n");
			process.exit(3);
		}
		if (message.echo === "silent") continue;
		held.push(message);
		if (held.length === 2) for (const one of held.splice(0).reverse()) process.stdout.write(JSON.stringify(one) + "\\n");
	}
});
`;

function start(others: Line[] = [], onExit?: () => void): LineRpc<Line> {
	return new LineRpc<Line>([process.execPath, "-e", CHILD], {
		exitError: (code, stderr) => new Error(`child exited ${code}: ${stderr.trim()}`),
		onOther: (line) => others.push(line),
		...(onExit ? { onExit } : {}),
	});
}

describe("LineRpc", () => {
	test("answers by id in any order and hands unsolicited lines to onOther", async () => {
		const others: Line[] = [];
		const rpc = start(others);
		try {
			const [first, second] = await Promise.all([
				rpc.request((id) => ({ id, echo: "one" })),
				rpc.request((id) => ({ id, echo: "two" })),
			]);
			expect(first).toEqual({ id: 1, echo: "one" });
			expect(second).toEqual({ id: 2, echo: "two" });
			expect(others).toEqual([{ hello: true }]);
		} finally {
			rpc.close();
			await rpc.exited;
		}
	});

	test("a call times out or aborts with its own text", async () => {
		const rpc = start();
		try {
			await expect(
				rpc.request((id) => ({ id, echo: "silent" }), { timeoutMs: 50, timeoutMessage: "too slow" }),
			).rejects.toThrow("too slow");
			const controller = new AbortController();
			const waiting = rpc.request((id) => ({ id, echo: "silent" }), {
				signal: controller.signal,
				abortMessage: "call aborted",
			});
			controller.abort();
			await expect(waiting).rejects.toThrow("call aborted");
			await expect(rpc.request((id) => ({ id }), { signal: controller.signal, abortMessage: "x" })).rejects.toThrow(
				"x",
			);
		} finally {
			rpc.close();
			await rpc.exited;
		}
	});

	test("an exit fails the waiting calls and every later one with the exit error, then runs onExit", async () => {
		let exits = 0;
		const rpc = start([], () => exits++);
		const waiting = rpc.request((id) => ({ id, echo: "silent" }));
		rpc.send({ echo: "die" });
		await expect(waiting).rejects.toThrow("child exited 3: first\nlast words");
		expect(await rpc.exited).toBe(3);
		expect(exits).toBe(1);
		await expect(rpc.request((id) => ({ id }))).rejects.toThrow("child exited 3");
	});
});
