import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, parse } from "node:path";
import {
	CodeIndex,
	type CodeLocation,
	locationLines,
	type Relationship,
	readIndexReuse,
	relationshipOf,
} from "../../../src/frontier/server/code-index.ts";
import { useTempJocasta } from "../../shared/fixture.ts";

/** Every index a test made: stopped (awaited) before the temp dir goes, so no server holds it. */
const made: CodeIndex[] = [];
afterEach(async () => {
	await Promise.all(made.splice(0).map((codeIndex) => codeIndex.stop()));
});
const env = useTempJocasta();
const FAKE = join(import.meta.dir, "fake-cbm.ts");

/** A main checkout whose graph (as the fake CBM reads it) is `graph`. */
function checkout(name: string, graph: Record<string, unknown[]>): { id: string; mainPath: string } {
	const mainPath = join(env.root, name);
	mkdirSync(mainPath, { recursive: true });
	writeFileSync(join(mainPath, "fake-graph.json"), JSON.stringify(graph));
	return { id: `${name}-1`, mainPath };
}

function index(extra: string[] = []): CodeIndex {
	const codeIndex = new CodeIndex({
		dataDir: env.data,
		server: [process.execPath, FAKE, ...extra],
		log: () => undefined,
	});
	made.push(codeIndex);
	return codeIndex;
}

function calls(): { pid: number; cwd: string; tool: string; args: Record<string, unknown> }[] {
	const file = join(env.data, "code-index", "cache", "fake-calls.log");
	if (!existsSync(file)) return [];
	return readFileSync(file, "utf8")
		.trim()
		.split("\n")
		.map((line) => JSON.parse(line));
}

function alive(pid: number): boolean {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
}

test("relationship questions are classified by kind and symbol; anything else is facts only", () => {
	const cases: [string, Relationship | undefined][] = [
		["who calls `buildPrompt`?", { kind: "callers", symbol: "buildPrompt" }],
		["What calls renderPage", { kind: "callers", symbol: "renderPage" }],
		["callers of the function send_mail", { kind: "callers", symbol: "send_mail" }],
		["where is Service.refresh() used?", { kind: "callers", symbol: "refresh" }],
		["Which functions call isAuthorizedRequest?", { kind: "callers", symbol: "isAuthorizedRequest" }],
		["what breaks if `readBoundedRequestJson` changes", { kind: "impact", symbol: "readBoundedRequestJson" }],
		["what is affected if I change buildPrompt", { kind: "impact", symbol: "buildPrompt" }],
		["impact of changing parseConfig", { kind: "impact", symbol: "parseConfig" }],
		["what depends on loadProjects?", { kind: "impact", symbol: "loadProjects" }],
		["what implements `Component`?", { kind: "implementers", symbol: "Component" }],
		["which classes extend the Base class", { kind: "implementers", symbol: "Base" }],
		["implementations of Shape", { kind: "implementers", symbol: "Shape" }],
		["subclasses of HttpError", { kind: "implementers", symbol: "HttpError" }],
		["where is buildPrompt defined?", undefined],
		["how does the agent loop execute tool calls", undefined],
		["what did we decide about the relay?", undefined],
	];
	for (const [query, expected] of cases) expect([query, relationshipOf(query)]).toEqual([query, expected]);
});

test("answers carry at most 10 locations (path:line and symbol, never source)", async () => {
	const calls15 = Array.from({ length: 15 }, (_, i) => [`caller${i}`, `src/c${i}.ts`, i + 1, "buildPrompt", i + 2]);
	const project = checkout("big", { calls: calls15 });
	const codeIndex = index();
	const all = (await codeIndex.locations(project, "who calls `buildPrompt`?", 50)) ?? [];
	expect(all).toHaveLength(10);
	expect(all[0]).toEqual({ path: "src/c0.ts", line: 2, symbol: "caller0", relation: "caller of buildPrompt" });
	expect(await codeIndex.locations(project, "callers of buildPrompt", 3)).toHaveLength(3);
	expect(await codeIndex.locations(project, "where is buildPrompt defined?")).toBeUndefined();
	const sent = calls().filter((call) => call.tool === "query_graph");
	expect(sent.map((call) => call.args.max_rows)).toEqual([10, 3]);
	const lines = locationLines([
		{ path: "src/a.ts", line: 42, symbol: "renderPage", relation: "caller of buildPrompt" },
		{ path: "src/s.ts", line: 9, symbol: "Circle", relation: "extends Base" },
	] satisfies CodeLocation[]);
	expect(lines).toEqual(["- src/a.ts:42 caller of buildPrompt (renderPage)", "- src/s.ts:9 extends Base (Circle)"]);
});

test("impact and implementers questions map to their own graph walks", async () => {
	const project = checkout("shapes", {
		calls: [
			["renderPage", "src/a.ts", 3, "buildGreeting", 4],
			["sendMail", "src/b.ts", 4, "renderPage", 6],
		],
		inherits: [
			["Circle", "src/shape.ts", 9, "Base", "INHERITS"],
			["Square", "src/shape.ts", 15, "Shape", "IMPLEMENTS"],
		],
	});
	const codeIndex = index();
	expect(await codeIndex.locations(project, "what breaks if buildGreeting changes?")).toEqual([
		{ path: "src/a.ts", line: 3, symbol: "renderPage", relation: "affected if buildGreeting changes" },
		{ path: "src/b.ts", line: 4, symbol: "sendMail", relation: "affected if buildGreeting changes" },
	]);
	expect(await codeIndex.locations(project, "subclasses of Base")).toEqual([
		{ path: "src/shape.ts", line: 9, symbol: "Circle", relation: "extends Base" },
	]);
	expect(await codeIndex.locations(project, "what implements `Shape`")).toEqual([
		{ path: "src/shape.ts", line: 15, symbol: "Square", relation: "implements Shape" },
	]);
});

test("each project is sealed: its own server in its main checkout, queries name only its CBM project", async () => {
	const a = checkout("alpha", { calls: [["fromAlpha", "src/alpha.ts", 1, "shared", 2]] });
	const b = checkout("beta", { calls: [["fromBeta", "lib/beta.ts", 5, "shared", 7]] });
	const codeIndex = index();
	expect((await codeIndex.locations(a, "who calls shared"))?.map((at) => at.path)).toEqual(["src/alpha.ts"]);
	expect((await codeIndex.locations(b, "who calls shared"))?.map((at) => at.path)).toEqual(["lib/beta.ts"]);
	expect((await codeIndex.locations(a, "who calls shared"))?.map((at) => at.path)).toEqual(["src/alpha.ts"]);
	const queries = calls().filter((call) => call.tool === "query_graph");
	const names = queries.map((call) => String(call.args.project));
	expect(names[0]).toEndWith("-alpha");
	expect(names[1]).toEndWith("-beta");
	expect(names[2]).toBe(names[0] ?? "");
	// One stdio server per project, started in that project's main checkout.
	const cwdByPid = new Map(queries.map((call) => [call.pid, call.cwd.toLowerCase()]));
	expect(cwdByPid.size).toBe(2);
	expect([...cwdByPid.values()].sort()).toEqual([a.mainPath.toLowerCase(), b.mainPath.toLowerCase()]);
	// The web UI and auto-indexing are turned off once, before the first server starts; never install or update.
	expect(
		calls()
			.filter((call) => call.tool === "config")
			.map((call): unknown => call.args),
	).toEqual([
		["set", "ui_enabled", "false"],
		["set", "auto_index", "false"],
	]);
});

test.if(process.platform === "win32")(
	"a data folder outside his profile is refused with the folder and the reason; off and missing binaries say so",
	async () => {
		const outside = join(parse(homedir()).root, "jocasta-not-profile");
		const refused = new CodeIndex({ dataDir: outside, server: [process.execPath, FAKE] });
		made.push(refused);
		const reason = refused.unavailable() ?? "";
		expect(reason).toBe(
			`The code index can't keep its data in ${join(outside, "code-index")}: codebase-memory-mcp refuses folders other users may write to (D:\\ lets them by default), so Jocasta's data folder must be under ${homedir()}.`,
		);
		const project = checkout("refused", { calls: [] });
		await expect(refused.locations(project, "who calls x1")).rejects.toThrow(reason);
		expect(await refused.locations(project, "what did we decide?")).toBeUndefined();
		expect(existsSync(outside)).toBe(false);

		expect(new CodeIndex({ dataDir: env.data, server: null }).unavailable()).toBe("The code index is off.");
		expect(new CodeIndex({ dataDir: env.data, mise: null }).unavailable()).toBe(
			"codebase-memory-mcp can't be found: mise isn't on PATH.",
		);
		const noTool = [process.execPath, "-e", "process.exit(1)"];
		expect(new CodeIndex({ dataDir: env.data, mise: noTool }).unavailable()).toBe(
			"codebase-memory-mcp 0.12.1 isn't installed: run `jocasta setup`, then `mise install`.",
		);
		expect(new CodeIndex({ dataDir: env.data, server: [join(env.root, "no-cbm.exe")] }).unavailable()).toBe(
			"codebase-memory-mcp 0.12.1 isn't installed: run `jocasta setup`, then `mise install`.",
		);
		expect(index().unavailable()).toBeUndefined();
	},
);

test("answers from an index that already existed count as reuse, in memory and one line each in reuse.log", async () => {
	const project = checkout("reused", { calls: [["caller", "src/a.ts", 1, "target", 2]] });
	expect(readIndexReuse(env.data)).toBe(0);
	const first = index();
	await first.locations(project, "who calls target");
	expect(first.reuse).toBe(0);
	await first.locations(project, "who calls target");
	expect(first.reuse).toBe(1);
	await first.stop();

	// A later run finds the index CBM kept: served without indexing, so it counts.
	const second = index();
	await second.locations(project, "what breaks if target changes");
	expect(second.reuse).toBe(1);
	expect(calls().filter((call) => call.tool === "index_repository")).toHaveLength(1);
	expect(readIndexReuse(env.data)).toBe(2);
	const lines = readFileSync(join(env.data, "code-index", "reuse.log"), "utf8")
		.trim()
		.split("\n");
	expect(lines.map((line) => line.split(" ").slice(1))).toEqual([
		["reused-1", "callers"],
		["reused-1", "impact"],
	]);
	await second.stop();

	// A merge reported before the next run's first question: the kept index is brought up to date first.
	const third = index();
	await third.refresh(project);
	expect(calls().filter((call) => call.tool === "index_repository")).toHaveLength(1);
	await third.locations(project, "who calls target");
	expect(calls().filter((call) => call.tool === "index_repository")).toHaveLength(2);
	expect(third.reuse).toBe(1);
});

test("refresh re-indexes the main checkout single-flight, and does nothing for a project never indexed", async () => {
	const project = checkout("fresh", { calls: [["caller", "src/a.ts", 1, "target", 2]] });
	const codeIndex = index(["--index-ms=300"]);
	await codeIndex.refresh(project);
	expect(calls()).toEqual([]);
	await codeIndex.locations(project, "who calls target");
	const indexed = () => calls().filter((call) => call.tool === "index_repository");
	expect(indexed()).toHaveLength(1);
	await Promise.all([codeIndex.refresh(project), codeIndex.refresh(project), codeIndex.refresh(project)]);
	expect(indexed()).toHaveLength(2);
	expect(String(indexed()[1]?.args.repo_path).toLowerCase()).toBe(project.mainPath.toLowerCase());
	await codeIndex.refresh(project);
	expect(indexed()).toHaveLength(3);
});

test("stop ends every server it started, is bounded, and never throws", async () => {
	const a = checkout("one", { calls: [] });
	const b = checkout("two", { calls: [] });
	const codeIndex = index();
	await codeIndex.locations(a, "who calls x1");
	await codeIndex.locations(b, "who calls x1");
	const pids = [...new Set(calls().map((call) => call.pid))].filter(
		(pid) => calls().find((call) => call.pid === pid)?.tool !== "config",
	);
	expect(pids).toHaveLength(2);
	expect(pids.every(alive)).toBe(true);
	const started = Date.now();
	await codeIndex.stop();
	expect(Date.now() - started).toBeLessThan(8_000);
	expect(pids.some(alive)).toBe(false);
	await codeIndex.stop();
	// A question after stop starts a fresh server.
	expect(await codeIndex.locations(a, "who calls x1")).toEqual([]);
});
