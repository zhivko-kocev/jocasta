/**
 * A stand-in for codebase-memory-mcp 0.12.1 as the code index drives it: `config set <key> <value>`, and the stdio
 * server (newline-delimited JSON-RPC MCP) with `index_repository`, `list_projects` and `query_graph` answering in CBM's
 * JSON shapes. Indexed projects persist in `$CBM_CACHE_DIR/fake-projects.json`; each project's graph is the
 * `fake-graph.json` in its root (`calls`: [caller, file, callerLine, callee, callLine]; `inherits`: [name, file, line,
 * target, "IMPLEMENTS" | "INHERITS"]). Every call is logged to `$CBM_CACHE_DIR/fake-calls.log` as a JSON line.
 * `--index-ms=N` makes indexing take N ms. Like CBM, it refuses to start when `$CBM_RUNTIME_DIR` is missing.
 */
import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const cache = process.env.CBM_CACHE_DIR ?? "";
const runtime = process.env.CBM_RUNTIME_DIR ?? "";
const args = process.argv.slice(2);
const indexMs = Number(args.find((arg) => arg.startsWith("--index-ms="))?.slice("--index-ms=".length) ?? 0);
const record = (entry: Record<string, unknown>) =>
	appendFileSync(
		join(cache, "fake-calls.log"),
		`${JSON.stringify({ pid: process.pid, cwd: process.cwd(), ...entry })}\n`,
	);

const configAt = args.indexOf("config");
if (configAt >= 0) {
	record({ tool: "config", args: args.slice(configAt + 1) });
	console.log(`${args[configAt + 2]} = ${args[configAt + 3]}`);
	process.exit(0);
}
if (!existsSync(runtime)) {
	console.error("codebase-memory-mcp: secure CLI coordination could not be created (endpoint)");
	process.exit(1);
}

const projectsFile = join(cache, "fake-projects.json");
const projects = (): Record<string, string> =>
	existsSync(projectsFile) ? JSON.parse(readFileSync(projectsFile, "utf8")) : {};

type Graph = {
	calls?: [string, string, number, string, number][];
	inherits?: [string, string, number, string, string][];
};

function query(project: string, cypher: string): unknown {
	const root = projects()[project];
	if (!root) return { error: "project not found or not indexed", available_projects: Object.keys(projects()) };
	const graph: Graph = JSON.parse(readFileSync(join(root, "fake-graph.json"), "utf8"));
	const target = /\{name:'([^']+)'\}/.exec(cypher)?.[1] ?? "";
	const limit = Number(/LIMIT (\d+)/.exec(cypher)?.[1] ?? 200);
	const calls = graph.calls ?? [];
	let columns: string[];
	let rows: string[][];
	if (cypher.includes("CALLS*1..2")) {
		columns = ["c.name", "c.file_path", "c.start_line"];
		const first = calls.filter((call) => call[3] === target);
		const second = calls.filter((call) => first.some((hop) => hop[0] === call[3]));
		const seen = new Set<string>();
		rows = [...first, ...second]
			.map((call) => [call[0], call[1], String(call[2])])
			.filter((row) => !seen.has(row.join("|")) && seen.add(row.join("|")));
	} else if (cypher.includes(":CALLS]")) {
		columns = ["c.name", "c.file_path", "r.line", "c.start_line"];
		rows = calls
			.filter((call) => call[3] === target)
			.map((call) => [call[0], call[1], String(call[4]), String(call[2])]);
	} else {
		columns = ["c.name", "c.file_path", "c.start_line", "type(r)"];
		rows = (graph.inherits ?? [])
			.filter((edge) => edge[3] === target)
			.map((edge) => [edge[0], edge[1], String(edge[2]), edge[4]]);
	}
	const visible = rows.slice(0, limit);
	return { columns, rows: visible, returned: visible.length, total: visible.length, has_more: false };
}

async function tool(name: string, input: Record<string, unknown>): Promise<unknown> {
	record({ tool: name, args: input });
	if (name === "index_repository") {
		await Bun.sleep(indexMs);
		const root = String(input.repo_path).replaceAll("\\", "/");
		const project = root.replace(":", "").replaceAll(/[/ ]+/g, "-");
		writeFileSync(projectsFile, JSON.stringify({ ...projects(), [project]: root }));
		return { project, nodes: 1, edges: 1, status: "indexed" };
	}
	if (name === "list_projects") {
		const all = Object.entries(projects()).map(([project, root]) => ({ name: project, root_path: root }));
		return { projects: all, total: all.length, returned: all.length, has_more: false };
	}
	if (name === "query_graph") return query(String(input.project), String(input.query));
	return { error: `unknown tool ${name}` };
}

const decoder = new TextDecoder();
let buffered = "";
for await (const chunk of Bun.stdin.stream()) {
	buffered += decoder.decode(chunk, { stream: true });
	let newline = buffered.indexOf("\n");
	while (newline >= 0) {
		const line = buffered.slice(0, newline).trim();
		buffered = buffered.slice(newline + 1);
		newline = buffered.indexOf("\n");
		if (!line) continue;
		const message = JSON.parse(line) as { id?: number; method: string; params?: Record<string, unknown> };
		if (message.id === undefined) continue;
		const reply = (result: unknown) =>
			process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, result })}\n`);
		if (message.method === "initialize") {
			reply({ protocolVersion: "2025-06-18", serverInfo: { name: "codebase-memory-mcp", version: "0.12.1" } });
		} else if (message.method === "tools/call") {
			const params = message.params ?? {};
			void tool(String(params.name), (params.arguments ?? {}) as Record<string, unknown>).then((body) =>
				reply({ content: [{ type: "text", text: JSON.stringify(body) }], structuredContent: body }),
			);
		}
	}
}
