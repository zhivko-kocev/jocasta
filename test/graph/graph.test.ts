import { afterEach, describe, expect, test } from "bun:test";
import { type Completer, createLlmEndpoint } from "../../src/frontier/server/llm-endpoint.ts";
import { LLM_PATH } from "../../src/shared/memory/graph/callback.ts";
import {
	addEpisodes,
	type Episode,
	type GraphDeps,
	GraphError,
	type GraphFact,
	graphitiService,
	projectGroup,
	searchGraph,
} from "../../src/shared/memory/graph/index.ts";
import { useTempJocasta } from "../shared/fixture.ts";
import { fakeCompleter, graphitiResponder, type ServedEndpoint, serveLlmEndpoint } from "./helpers.ts";

const _env = useTempJocasta();

/** The service's bearer token in these tests (0430). */
const TOKEN = "graph-test-token";

interface Stored {
	fact: string;
	source: string;
}

/**
 * A stand-in for `docker/graphiti/service.py`: on add it calls the request's model endpoint the way Graphiti 0.30.2's
 * `OpenAIGenericClient` does (bearer token, `json_schema` response format) and stores the edges' facts in the group;
 * search returns the group's facts sharing a word with the query.
 */
interface FakeGraphiti {
	url: string;
	groups: Map<string, Stored[]>;
	stop(): void;
}

function fakeGraphiti(): FakeGraphiti {
	const groups = new Map<string, Stored[]>();
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		async fetch(request) {
			if (request.headers.get("authorization") !== `Bearer ${TOKEN}`) {
				return Response.json({ error: "invalid token" }, { status: 401 });
			}
			const body = (await request.json()) as {
				group_id: string;
				episodes?: { name: string; body: string; source: string; reference_time: string }[];
				llm?: { base_url: string; api_key: string };
				query?: string;
				limit?: number;
			};
			const path = new URL(request.url).pathname;
			if (path === "/episodes" && body.llm && body.episodes) {
				for (const episode of body.episodes) {
					const response = await fetch(`${body.llm.base_url}/chat/completions`, {
						method: "POST",
						headers: { authorization: `Bearer ${body.llm.api_key}`, "content-type": "application/json" },
						body: JSON.stringify({
							model: "main",
							messages: [
								{ role: "system", content: "Extract fact triples." },
								{ role: "user", content: episode.body },
							],
							temperature: 1,
							max_tokens: 16384,
							response_format: {
								type: "json_schema",
								json_schema: {
									name: "ExtractedEdges",
									schema: {
										type: "object",
										properties: { edges: { type: "array", items: { type: "object" } } },
										required: ["edges"],
									},
								},
							},
						}),
					});
					if (!response.ok) return Response.json({ error: await response.text() }, { status: 500 });
					const completion = (await response.json()) as { choices: { message: { content: string } }[] };
					const edges = JSON.parse(completion.choices[0]?.message.content ?? "{}") as { edges: { fact: string }[] };
					const stored = groups.get(body.group_id) ?? [];
					stored.push(...edges.edges.map((e) => ({ fact: e.fact, source: episode.source })));
					groups.set(body.group_id, stored);
				}
				return Response.json({ added: body.episodes.length });
			}
			if (path === "/search") {
				const words = (body.query ?? "").toLowerCase().split(/\W+/).filter(Boolean);
				const facts: GraphFact[] = (groups.get(body.group_id) ?? [])
					.filter((s) => words.some((w) => s.fact.toLowerCase().includes(w)))
					.slice(0, body.limit ?? 10)
					.map((s) => ({ fact: s.fact, name: "RELATES", valid_at: null, invalid_at: null, created_at: null }));
				return Response.json({ facts });
			}
			return Response.json({ error: "group_id is required" }, { status: 400 });
		},
	});
	return { url: `http://127.0.0.1:${server.port}`, groups, stop: () => void server.stop(true) };
}

const cleanups: (() => void)[] = [];
afterEach(() => {
	for (const fn of cleanups.splice(0)) fn();
});

/** Graph deps over the fake service, the endpoint served for `completer`, and a counter of container uses. */
function fakeDeps(completer: Completer): { deps: GraphDeps; graphiti: FakeGraphiti; uses: number[] } {
	const graphiti = fakeGraphiti();
	const endpoint = createLlmEndpoint({ completer });
	const served: ServedEndpoint = serveLlmEndpoint(endpoint);
	cleanups.push(graphiti.stop, served.stop);
	const uses: number[] = [];
	return {
		graphiti,
		uses,
		deps: {
			service: graphitiService(graphiti.url, TOKEN),
			use: (fn) => {
				uses.push(1);
				return fn();
			},
			callback: () => ({ baseUrl: `http://127.0.0.1:${served.port}${LLM_PATH}`, token: endpoint.token }),
		},
	};
}

const BUN = { source: "Zhivko", target: "Bun", relation: "PREFERS", fact: "Zhivko prefers Bun over Node" };

const episode = (body: string, source = "omp-session"): Episode => ({
	name: `compact ${source}`,
	body,
	source,
	reference_time: "2026-10-09T10:00:00Z",
});

describe("graph episodes and search", () => {
	test("episodes go to the project's group, extracted through the callback endpoint", async () => {
		const source = fakeCompleter(graphitiResponder([BUN]));
		const { deps, graphiti, uses } = fakeDeps(source);
		await addEpisodes("p-0123456789abcdef", [episode("He said he prefers Bun."), episode("   ")], deps);
		expect(graphiti.groups.get("p-0123456789abcdef")).toEqual([{ fact: BUN.fact, source: "omp-session" }]);
		expect(uses).toHaveLength(1);
		const facts = await searchGraph("p-0123456789abcdef", "which runtime does he prefer? Bun", 5, deps);
		expect(facts.map((f) => f.fact)).toEqual([BUN.fact]);
	});

	test("leak check (0281): one project's graph facts never come back for another project", async () => {
		const source = fakeCompleter(graphitiResponder([BUN]));
		const { deps } = fakeDeps(source);
		await addEpisodes("p-aaaaaaaaaaaaaaaa", [episode("He prefers Bun.")], deps);
		expect(await searchGraph("p-bbbbbbbbbbbbbbbb", "Bun", 10, deps)).toEqual([]);
		expect((await searchGraph("p-aaaaaaaaaaaaaaaa", "Bun", 10, deps)).length).toBe(1);
	});

	test("nothing to add makes no call; a group id that isn't a project id is refused", async () => {
		const source = fakeCompleter(graphitiResponder([BUN]));
		const { deps, uses } = fakeDeps(source);
		await addEpisodes("p-0123456789abcdef", [], deps);
		expect(uses).toHaveLength(0);
		expect(() => projectGroup("p-1 OR 1=1")).toThrow(GraphError);
		await expect(addEpisodes("personal", [episode("x")], deps)).rejects.toThrow(GraphError);
		await expect(searchGraph("", "x", 1, deps)).rejects.toThrow(GraphError);
	});

	test("a service error names the call and the service's message", async () => {
		const source = fakeCompleter(() => new Error("no auth"));
		const { deps } = fakeDeps(source);
		const add = addEpisodes("p-0123456789abcdef", [episode("x")], deps);
		await expect(add).rejects.toThrow(/graphiti \/episodes: 500/);
	});

	test("calls carry the service's bearer token; a wrong one is refused (0430)", async () => {
		const source = fakeCompleter(graphitiResponder([BUN]));
		const { graphiti } = fakeDeps(source);
		await expect(graphitiService(graphiti.url, "wrong").search("p-0123456789abcdef", "Bun", 5)).rejects.toThrow(
			/graphiti \/search: 401 invalid token/,
		);
	});

	test("a service that never answers fails the call once its timeout passes (CO-9)", async () => {
		const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Promise<Response>(() => {}) });
		cleanups.push(() => void server.stop(true));
		const service = graphitiService(`http://127.0.0.1:${server.port}`, TOKEN, { episodesMs: 200, searchMs: 100 });
		const started = Date.now();
		await expect(service.search("p-0123456789abcdef", "Bun", 5)).rejects.toThrow(/graphiti \/search: no answer/);
		await expect(
			service.add("p-0123456789abcdef", [episode("x")], { baseUrl: "http://127.0.0.1:1", token: "t" }),
		).rejects.toThrow(/graphiti \/episodes: no answer/);
		expect(Date.now() - started).toBeLessThan(5_000);
	});
});
