import { Database } from "bun:sqlite";
import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
	type Completer,
	type CompletionRequest,
	createLlmEndpoint,
	extractJson,
	type GraphUsageLine,
	ompCompleter,
} from "../../src/frontier/server/llm-endpoint.ts";
import { jsonLines } from "../../src/shared/jsonl.ts";
import { LLM_PATH } from "../../src/shared/memory/graph/callback.ts";
import { pidAlive } from "../../src/shared/proctree.ts";
import { readSettings } from "../../src/shared/settings.ts";
import { poll, useTempJocasta } from "../shared/fixture.ts";
import { fakeCompleter, serveLlmEndpoint } from "./helpers.ts";

const env = useTempJocasta();

/** The body Graphiti 0.30.2's `OpenAIGenericClient` posts in its default `json_schema` mode. */
function graphitiBody(overrides: Record<string, unknown> = {}) {
	return {
		model: "main",
		messages: [
			{ role: "system", content: "You extract entities." },
			{ role: "user", content: "Zhivko prefers Bun." },
		],
		temperature: 1,
		max_tokens: 16384,
		response_format: {
			type: "json_schema",
			json_schema: {
				name: "ExtractedEntities",
				schema: { type: "object", properties: { names: { type: "array" } }, required: ["names"] },
			},
		},
		...overrides,
	};
}

function post(token: string, body: unknown, path = `${LLM_PATH}/chat/completions`): Request {
	return new Request(`http://127.0.0.1${path}`, {
		method: "POST",
		headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
		body: JSON.stringify(body),
	});
}

async function errorOf(response: Response | undefined): Promise<{ status?: number; message: string; type: string }> {
	const body = (await response?.json()) as { error: { message: string; type: string } };
	return { status: response?.status, ...body.error };
}

describe("the OpenAI-compatible callback endpoint", () => {
	test("maps a Graphiti chat completion onto the completer and answers in OpenAI's shape", async () => {
		const seen: CompletionRequest[] = [];
		const completer = fakeCompleter((_model, request) => {
			seen.push(request);
			return 'Here you go:\n```json\n{"names": ["Zhivko", "Bun"]}\n```';
		});
		const endpoint = createLlmEndpoint({ completer });
		const response = (await endpoint.handle(post(endpoint.token, graphitiBody()))) as Response;
		expect(response.status).toBe(200);
		const body = (await response.json()) as {
			object: string;
			model: string;
			choices: { message: { role: string; content: string }; finish_reason: string }[];
			usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
		};
		expect(body.object).toBe("chat.completion");
		expect(body.model).toBe("fake-main");
		expect(completer.used).toEqual(["main"]);
		expect(body.choices[0]?.message.role).toBe("assistant");
		expect(JSON.parse(body.choices[0]?.message.content ?? "")).toEqual({ names: ["Zhivko", "Bun"] });
		expect(body.choices[0]?.finish_reason).toBe("stop");
		expect(body.usage).toEqual({ prompt_tokens: 13, completion_tokens: 5, total_tokens: 18 });
		expect(seen[0]?.messages).toEqual(["Zhivko prefers Bun."]);
		expect(seen[0]?.system).toStartWith("You extract entities.\n\nReply with only one JSON object");
		expect(seen[0]?.system).toContain('{"type":"object","properties":{"names":{"type":"array"}},"required":["names"]}');
		expect(seen[0]?.maxTokens).toBe(16384);
	});

	test("asks for JSON on json_object, and passes plain text through when no format is asked", async () => {
		const seen: CompletionRequest[] = [];
		const completer = fakeCompleter((_model, request) => {
			seen.push(request);
			return 'Sure: {"a": 1}';
		});
		const endpoint = createLlmEndpoint({ completer });
		const replyOf = async (response: Response | undefined) =>
			((await (response as Response).json()) as { choices: { message: { content: string } }[] }).choices[0]?.message
				.content;
		const asJson = await endpoint.handle(
			post(endpoint.token, graphitiBody({ response_format: { type: "json_object" } })),
		);
		expect(await replyOf(asJson)).toBe('{"a": 1}');
		expect(seen[0]?.system).toBe(
			"You extract entities.\n\nReply with only one JSON object, with no other text and no code fence.",
		);
		const plain = await endpoint.handle(
			post(endpoint.token, { model: "small", messages: [{ role: "user", content: [{ type: "text", text: "hi" }] }] }),
		);
		expect(await replyOf(plain)).toBe('Sure: {"a": 1}');
		expect(seen[1]).toEqual({ messages: ["hi"], signal: expect.any(AbortSignal) });
	});

	test("Graphiti's small model name goes to the small model; an unknown name is refused (0341)", async () => {
		const completer = fakeCompleter(() => "{}");
		const endpoint = createLlmEndpoint({ completer });
		const small = (await endpoint.handle(post(endpoint.token, graphitiBody({ model: "small" })))) as Response;
		expect(((await small.json()) as { model: string }).model).toBe("fake-small");
		const unknown = await endpoint.handle(post(endpoint.token, graphitiBody({ model: "gpt-4.1-mini" })));
		expect(await errorOf(unknown)).toEqual({
			status: 404,
			message: 'no model for "gpt-4.1-mini"',
			type: "model_not_found",
		});
		expect(completer.used).toEqual(["small"]);
	});

	test("refuses a missing or wrong token, and paths outside its own", async () => {
		const completer = fakeCompleter(() => "{}");
		const endpoint = createLlmEndpoint({ completer, token: "secret" });
		expect(endpoint.token).toBe("secret");
		expect(await errorOf(await endpoint.handle(post("wrong", graphitiBody())))).toEqual({
			status: 401,
			message: "invalid token",
			type: "authentication_error",
		});
		expect((await endpoint.handle(post("secret-but-longer", graphitiBody())))?.status).toBe(401);
		const bare = new Request(`http://127.0.0.1${LLM_PATH}/chat/completions`, { method: "POST", body: "{}" });
		expect((await endpoint.handle(bare))?.status).toBe(401);
		expect(await endpoint.handle(post(endpoint.token, {}, "/other"))).toBeUndefined();
		expect((await endpoint.handle(post(endpoint.token, {}, `${LLM_PATH}/embeddings`)))?.status).toBe(404);
		expect(completer.used).toEqual([]);
	});

	test("a failing model call is a 502 with the model's message", async () => {
		const endpoint = createLlmEndpoint({ completer: fakeCompleter(() => new Error("rate limited")) });
		expect(await errorOf(await endpoint.handle(post(endpoint.token, graphitiBody())))).toEqual({
			status: 502,
			message: "rate limited",
			type: "upstream_error",
		});
	});

	test("rejects bad bodies, streaming and roles it doesn't map", async () => {
		const completer = fakeCompleter(() => "{}");
		const endpoint = createLlmEndpoint({ completer });
		const notJson = new Request(`http://127.0.0.1${LLM_PATH}/chat/completions`, {
			method: "POST",
			headers: { authorization: `Bearer ${endpoint.token}` },
			body: "{",
		});
		expect((await errorOf(await endpoint.handle(notJson))).message).toBe("body is not JSON");
		expect((await endpoint.handle(post(endpoint.token, graphitiBody({ stream: true }))))?.status).toBe(400);
		const tool = graphitiBody({ messages: [{ role: "tool", content: "x" }] });
		expect((await errorOf(await endpoint.handle(post(endpoint.token, tool)))).message).toBe(
			"unsupported message role: tool",
		);
		const image = graphitiBody({ messages: [{ role: "user", content: [{ type: "image_url" }] }] });
		expect((await errorOf(await endpoint.handle(post(endpoint.token, image)))).message).toBe(
			"message content must be text",
		);
		const noUser = graphitiBody({ messages: [{ role: "system", content: "x" }] });
		expect((await errorOf(await endpoint.handle(post(endpoint.token, noUser)))).message).toBe("no user message");
		expect(completer.used).toEqual([]);
	});

	test("serves on its own 127.0.0.1 port", async () => {
		const endpoint = createLlmEndpoint({ completer: fakeCompleter(() => '{"names": []}') });
		const served = serveLlmEndpoint(endpoint);
		try {
			const response = await fetch(`http://127.0.0.1:${served.port}${LLM_PATH}/chat/completions`, {
				method: "POST",
				headers: { authorization: `Bearer ${endpoint.token}`, "content-type": "application/json" },
				body: JSON.stringify(graphitiBody()),
			});
			expect(response.status).toBe(200);
			expect((await fetch(`http://127.0.0.1:${served.port}/elsewhere`)).status).toBe(404);
		} finally {
			served.stop();
		}
	});

	test("each answered call appends one line to the graph usage file, priced from omp's models.db (0582)", async () => {
		const agentDir = join(env.root, "omp-agent");
		mkdirSync(agentDir, { recursive: true });
		const db = new Database(join(agentDir, "models.db"));
		db.run("CREATE TABLE model_cache (provider_id TEXT, models TEXT)");
		db.run("INSERT INTO model_cache VALUES (?, ?)", [
			"anthropic",
			JSON.stringify([
				{ id: "claude-x", contextWindow: 200_000, cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 } },
			]),
		]);
		db.close();
		const saved = process.env.PI_CODING_AGENT_DIR;
		process.env.PI_CODING_AGENT_DIR = agentDir;
		try {
			const models = ["anthropic/claude-x", "unpriced"];
			const completer: Completer = {
				async complete() {
					const model = models.shift() ?? "";
					return {
						model,
						provider: model.includes("/") ? "anthropic" : null,
						text: "{}",
						stopReason: "stop",
						usage: { input: 1000, output: 200, cacheRead: 5000, cacheWrite: 100 },
					};
				},
			};
			const usageFile = join(env.root, "usage", "graph-usage.jsonl");
			const endpoint = createLlmEndpoint({ completer, usageFile });
			expect((await endpoint.handle(post(endpoint.token, graphitiBody())))?.status).toBe(200);
			expect((await endpoint.handle(post(endpoint.token, graphitiBody({ model: "small" }))))?.status).toBe(200);
			const lines = jsonLines<GraphUsageLine>(readFileSync(usageFile, "utf8"));
			expect(lines).toHaveLength(2);
			expect(lines[0]).toEqual({
				at: expect.any(String),
				model: "anthropic/claude-x",
				input: 1000,
				output: 200,
				cacheRead: 5000,
				cacheWrite: 100,
				cost: expect.closeTo((1000 * 3 + 200 * 15 + 5000 * 0.3 + 100 * 3.75) / 1e6, 12),
			});
			expect(Number.isNaN(Date.parse(lines[0]?.at ?? ""))).toBe(false);
			// A model omp doesn't price is still recorded, at no cost.
			expect(lines[1]).toMatchObject({ model: "unpriced", input: 1000, cost: 0 });
			// A failed call records nothing.
			const failing = createLlmEndpoint({ completer: fakeCompleter(() => new Error("down")), usageFile });
			expect((await failing.handle(post(failing.token, graphitiBody())))?.status).toBe(502);
			expect(jsonLines(readFileSync(usageFile, "utf8"))).toHaveLength(2);
		} finally {
			if (saved === undefined) delete process.env.PI_CODING_AGENT_DIR;
			else process.env.PI_CODING_AGENT_DIR = saved;
		}
	});

	test("a usage file that can't be written leaves the call answered", async () => {
		const blocker = join(env.root, "not-a-folder");
		writeFileSync(blocker, "");
		const endpoint = createLlmEndpoint({
			completer: fakeCompleter(() => "{}"),
			usageFile: join(blocker, "graph-usage.jsonl"),
		});
		expect((await endpoint.handle(post(endpoint.token, graphitiBody())))?.status).toBe(200);
		expect(existsSync(join(blocker, "graph-usage.jsonl"))).toBe(false);
	});
});

describe("extractJson", () => {
	test("strips a fence or surrounding prose and leaves plain JSON alone", () => {
		expect(extractJson('{"a":1}')).toBe('{"a":1}');
		expect(extractJson('```json\n{"a":1}\n```')).toBe('{"a":1}');
		expect(extractJson('Sure: {"a":{"b":2}} done')).toBe('{"a":{"b":2}}');
		expect(extractJson("no json")).toBe("no json");
	});
});

/**
 * A stand-in `omp` for `omp auth-gateway stdio` (pi-ai `auth-gateway/stdio.ts`): the ready line, then one answer per
 * request line, by id. The last user message picks the answer: `fail` a 404 error, `length` a cut reply, `held` an
 * answer sent only after the next request's, `hang` none, `die` an exit with a stderr line; anything else echoes argv
 * and the request. Each start appends `start <pid>` to the starts file, and the end of its stdin `end`.
 */
function fakeGateway(root: string): { command: string[]; starts: () => number; ends: () => number } {
	const file = join(root, "fake-omp-gateway.ts");
	const starts = join(root, "gateway-starts.txt");
	writeFileSync(starts, "");
	writeFileSync(
		file,
		`import { appendFileSync } from "node:fs";
		const [log, ...args] = process.argv.slice(2);
		appendFileSync(log, "start " + process.pid + "\\n");
		const send = (o) => process.stdout.write(JSON.stringify(o) + "\\n");
		send({ ready: true, version: "18.12.6" });
		let held;
		const answer = (id, body) => {
			const last = body.messages.at(-1).content;
			if (last === "hang") return;
			if (last === "die") { process.stderr.write("no login for anthropic\\n"); process.exit(3); }
			if (last === "fail") {
				return send({ id, status: 404, body: { error: { message: \`No available model matches "\${body.model}"\`, type: "invalid_request_error" } } });
			}
			const reply = { id, status: 200, body: {
				id: "chatcmpl-x", object: "chat.completion", model: "p/answered",
				choices: [{ index: 0, message: { role: "assistant", content: last === "length" ? "cut" : JSON.stringify({ args, body }) },
					finish_reason: last === "length" ? "length" : "stop" }],
				usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13, prompt_tokens_details: { cached_tokens: 4 } },
			} };
			if (last === "held") { held = reply; return; }
			send(reply);
			if (held) { send(held); held = undefined; }
		};
		const decoder = new TextDecoder();
		let buffered = "";
		for await (const chunk of Bun.stdin.stream()) {
			buffered += decoder.decode(chunk, { stream: true });
			let at = buffered.indexOf("\\n");
			while (at >= 0) {
				const line = JSON.parse(buffered.slice(0, at));
				buffered = buffered.slice(at + 1);
				at = buffered.indexOf("\\n");
				answer(line.id, line.body);
			}
		}
		appendFileSync(log, "end\\n");`,
	);
	const lines = () => readFileSync(starts, "utf8").split("\n");
	const count = (word: string) => lines().filter((line) => line.split(" ")[0] === word).length;
	gatewaysGone.push(() => {
		const pids = lines().flatMap((line) => (line.startsWith("start ") ? [Number(line.slice(6))] : []));
		return poll(() => !pids.some(pidAlive), { timeoutMs: 15_000, what: "the gateway children to exit" });
	});
	return { command: [process.execPath, file, starts], starts: () => count("start"), ends: () => count("end") };
}

/**
 * Waits, per fake gateway of the test, until every child it started has exited. A closed child still appends `end`
 * on its way out; a write into the test's temp dir while the fixture removes it fails the removal (ENOTEMPTY under
 * load), so this runs first: a describe's afterEach runs before the file's.
 */
const gatewaysGone: (() => Promise<unknown>)[] = [];

interface Echoed {
	args: string[];
	body: { model: string; messages: { role: string; content: string }[]; max_tokens?: number };
}

describe("ompCompleter over omp auth-gateway stdio (0463)", () => {
	// Waits on real child processes ending, which takes seconds on a loaded machine.
	afterEach(() => Promise.all(gatewaysGone.splice(0).map((gone) => gone())), 20_000);

	test("sends main and small as omp's @default and @smol roles to one gateway child, with the system and user turns", async () => {
		const gateway = fakeGateway(env.root);
		const completer = ompCompleter({ command: gateway.command, settings: () => ({}) });
		try {
			const main = await completer.complete("main", { system: "sys", messages: ["a", "b"], maxTokens: 16384 });
			expect(main).toMatchObject({
				model: "p/answered",
				stopReason: "stop",
				usage: { input: 6, output: 3, cacheRead: 4, cacheWrite: 0 },
			});
			const echoed = JSON.parse(main.text) as Echoed;
			expect(echoed.args).toEqual(["auth-gateway", "stdio"]);
			expect(echoed.body).toEqual({
				model: "@default",
				messages: [
					{ role: "system", content: "sys" },
					{ role: "user", content: "a" },
					{ role: "user", content: "b" },
				],
				max_tokens: 16384,
			});
			const small = JSON.parse((await completer.complete("small", { messages: ["c"] })).text) as Echoed;
			expect(small.body).toEqual({ model: "@smol", messages: [{ role: "user", content: "c" }] });
			expect(gateway.starts()).toBe(1);
		} finally {
			completer.close();
		}
	});

	test("graph.main / graph.small in jocasta.yaml name the model; a malformed one names its line and turns the calls off", async () => {
		const gateway = fakeGateway(env.root);
		let settings: () => Record<string, unknown> = () => ({ graph: { small: "q/other" } });
		const completer = ompCompleter({ command: gateway.command, settings: () => settings() });
		try {
			const answered = await completer.complete("small", { messages: ["x"] });
			expect((JSON.parse(answered.text) as Echoed).body.model).toBe("q/other");
			// The selector's provider prices the call; an omp role names none.
			expect(answered.provider).toBe("q");
			expect((await completer.complete("main", { messages: ["x"] })).provider).toBeNull();
			settings = () => ({ graph: { main: "just-an-id" } });
			await expect(completer.complete("main", { messages: ["x"] })).rejects.toThrow(
				"settings: graph.main must be an omp model as provider/id",
			);
			// Read from the file, the message is jocasta.yaml's own check with its line (0584), for both models' calls.
			mkdirSync(env.config, { recursive: true });
			writeFileSync(join(env.config, "jocasta.yaml"), "graph:\n  small: q/other\n  main: just-an-id\n");
			settings = readSettings;
			for (const name of ["main", "small"] as const) {
				await expect(completer.complete(name, { messages: ["x"] })).rejects.toThrow(
					"jocasta.yaml:3: graph.main must be an omp model as provider/id",
				);
			}
		} finally {
			completer.close();
		}
	});

	test("a gateway error throws with its message; length is reported; answers match by id in any order", async () => {
		const gateway = fakeGateway(env.root);
		const completer = ompCompleter({ command: gateway.command, settings: () => ({}) });
		try {
			await expect(completer.complete("main", { messages: ["fail"] })).rejects.toThrow(
				'No available model matches "@default"',
			);
			expect(await completer.complete("main", { messages: ["length"] })).toMatchObject({
				text: "cut",
				stopReason: "length",
			});
			const [held, next] = await Promise.all([
				completer.complete("main", { messages: ["held"] }),
				completer.complete("small", { messages: ["next"] }),
			]);
			expect((JSON.parse(held.text) as Echoed).body.model).toBe("@default");
			expect((JSON.parse(next.text) as Echoed).body.model).toBe("@smol");
		} finally {
			completer.close();
		}
	});

	test("an aborted call stops waiting; a gateway that exits fails its calls and is started again", async () => {
		const gateway = fakeGateway(env.root);
		const completer = ompCompleter({ command: gateway.command, settings: () => ({}) });
		try {
			const controller = new AbortController();
			const hanging = completer.complete("main", { messages: ["hang"], signal: controller.signal });
			controller.abort();
			await expect(hanging).rejects.toThrow("model call aborted");
			await expect(completer.complete("main", { messages: ["die"] })).rejects.toThrow(
				"omp auth-gateway stdio exited 3: no login for anthropic",
			);
			expect((await completer.complete("small", { messages: ["again"] })).model).toBe("p/answered");
			expect(gateway.starts()).toBe(2);
		} finally {
			completer.close();
		}
	});

	test("the gateway child closes after its idle time without calls, and the next call starts it again", async () => {
		const gateway = fakeGateway(env.root);
		const completer = ompCompleter({ command: gateway.command, settings: () => ({}), idleMs: 200 });
		try {
			await completer.complete("main", { messages: ["a"] });
			await completer.complete("main", { messages: ["b"] });
			expect(gateway.starts()).toBe(1);
			// A real child ending on the completer's real idle clock: polled until its stdin closed, bounded.
			for (let waited = 0; gateway.ends() === 0 && waited < 5_000; waited += 50) await Bun.sleep(50);
			expect(gateway.ends()).toBe(1);
			expect((await completer.complete("small", { messages: ["c"] })).model).toBe("p/answered");
			expect(gateway.starts()).toBe(2);
		} finally {
			completer.close();
		}
	});

	test("a missing omp fails the call", async () => {
		const completer = ompCompleter({ command: [join(env.root, "no-such-omp")], settings: () => ({}) });
		await expect(completer.complete("main", { messages: ["x"] })).rejects.toThrow();
	});
});

/**
 * Opt-in live check (`JOCASTA_OMP_GATEWAY_SMOKE=1`): one small call through his installed omp's `omp auth-gateway
 * stdio`, his `~/.omp/agent` logins and his `smol` role, served by the endpoint as Graphiti would call it.
 */
describe.skipIf(process.env.JOCASTA_OMP_GATEWAY_SMOKE !== "1")("his omp (smoke)", () => {
	test("the endpoint answers Graphiti's small model through his omp", async () => {
		const completer = ompCompleter({ settings: () => ({}) });
		const endpoint = createLlmEndpoint({ completer });
		try {
			const response = (await endpoint.handle(
				post(endpoint.token, {
					model: "small",
					messages: [
						{ role: "system", content: "You answer with one word." },
						{ role: "user", content: "Reply with the word pong." },
					],
					max_tokens: 256,
					response_format: { type: "json_object" },
				}),
			)) as Response;
			const body = (await response.json()) as {
				model?: string;
				choices?: { message: { content: string } }[];
				error?: unknown;
			};
			console.log(
				`omp smoke: ${response.status} model=${body.model} reply=${JSON.stringify(body.choices?.[0]?.message.content ?? body.error)}`,
			);
			expect(response.status).toBe(200);
			expect(body.choices?.[0]?.message.content.length).toBeGreaterThan(0);
		} finally {
			completer.close();
		}
	}, 120_000);
});
