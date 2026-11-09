import { expect, test } from "bun:test";
import { join, resolve } from "node:path";
import jocastaPackage from "../package.json" with { type: "json" };
import { MCP_PROTOCOL_VERSION } from "../src/mcp/server.ts";
import { NOT_RUNNING } from "../src/shared/service-file.ts";
import { useTempJocasta } from "./shared/fixture.ts";

const ENTRY = resolve(import.meta.dir, "..", "src", "entry.ts");
const env = useTempJocasta();

function jocasta(args: string[], stdin: "pipe" | "ignore" = "ignore") {
	return Bun.spawn([process.execPath, ENTRY, ...args], {
		stdin,
		stdout: "pipe",
		stderr: "pipe",
		env: { ...process.env, JOCASTA_DATA_DIR: env.data, JOCASTA_CODING_AGENT_DIR: join(env.root, "config") },
		windowsHide: true,
	});
}

test("--version names Jocasta, omp, herdr and Bun", async () => {
	// A PATH and a home of its own (Bun runs by its full path): omp and herdr are never found, as on a CI runner. Not
	// Bun's own folder: `bun install -g` puts omp there.
	const child = Bun.spawn([process.execPath, ENTRY, "--version"], {
		stdin: "ignore",
		stdout: "pipe",
		stderr: "pipe",
		env: {
			...process.env,
			PATH: join(env.root, "bin"),
			Path: join(env.root, "bin"),
			HOME: env.root,
			USERPROFILE: env.root,
			JOCASTA_DATA_DIR: env.data,
			JOCASTA_CODING_AGENT_DIR: join(env.root, "config"),
		},
		windowsHide: true,
	});
	const out = await new Response(child.stdout).text();
	expect(await child.exited).toBe(0);
	expect(out.trim()).toBe(`jocasta ${jocastaPackage.version} (omp not found, herdr not found, bun ${Bun.version})`);
});

test("other words print usage and exit 2, a word that is no command named first (0140, 0581)", async () => {
	const unknown = jocasta(["threads", "list"]);
	const said = await new Response(unknown.stderr).text();
	expect(await unknown.exited).toBe(2);
	expect(said).toStartWith('jocasta: unknown command "threads".\nusage: jocasta');
	for (const args of [
		["mcp", "--nope"],
		["setup", "now"],
	]) {
		const child = jocasta(args);
		const err = await new Response(child.stderr).text();
		expect(await child.exited).toBe(2);
		expect(err).toStartWith("usage: jocasta");
	}
});

test("jocasta thread|worktree: bad usage exits 2 with the usage; without the service, exit 3 says so (0544)", async () => {
	for (const args of [["thread"], ["thread", "nope"], ["thread", "finish", "x", "squash"], ["worktree", "remove"]]) {
		const child = jocasta(args);
		const err = await new Response(child.stderr).text();
		expect(await child.exited).toBe(2);
		// A usage prints as it is: its continuation lines line up (0581).
		expect(err).toStartWith(`usage: jocasta ${args[0]}`);
	}
	for (const args of [
		["thread", "list"],
		["worktree", "list"],
		["thread", "stop", "x"],
	]) {
		const child = jocasta(args);
		const err = await new Response(child.stderr).text();
		expect(await child.exited).toBe(3);
		expect(err.trim()).toBe(`jocasta: ${NOT_RUNNING}`);
	}
});

test("jocasta mcp answers initialize on stdio, and its tools say Jocasta isn't running without the service", async () => {
	const child = jocasta(["mcp", "--boss"], "pipe");
	const stdin = child.stdin;
	if (!stdin) throw new Error("jocasta mcp has no stdin");
	stdin.write(
		`${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: MCP_PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: "test", version: "0" } } })}\n`,
	);
	stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
	stdin.write(
		`${JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "graph_search", arguments: { project: "demo", query: "x" } } })}\n`,
	);
	stdin.end();
	const out = await new Response(child.stdout).text();
	expect(await child.exited).toBe(0);
	const replies = out
		.trim()
		.split("\n")
		.map((l) => JSON.parse(l));
	expect(replies[0]).toEqual({
		jsonrpc: "2.0",
		id: 1,
		result: {
			protocolVersion: MCP_PROTOCOL_VERSION,
			capabilities: { tools: { listChanged: false } },
			serverInfo: { name: "jocasta", version: jocastaPackage.version },
		},
	});
	expect(replies[1]).toEqual({
		jsonrpc: "2.0",
		id: 2,
		result: { content: [{ type: "text", text: NOT_RUNNING }], isError: true },
	});
}, 20_000);
