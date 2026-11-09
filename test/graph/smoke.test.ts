import { describe, expect, test } from "bun:test";
import { createLlmEndpoint } from "../../src/frontier/server/llm-endpoint.ts";
import { type ComposeRun, Containers } from "../../src/shared/containers/index.ts";
import {
	GRAPHITI_PORT,
	GRAPHITI_TOKEN_ENV,
	graphitiPort,
	graphitiResource,
	graphitiToken,
} from "../../src/shared/containers/resources.ts";
import { containerCallbackUrl } from "../../src/shared/memory/graph/callback.ts";
import { addEpisodes, type GraphDeps, graphitiService, searchGraph } from "../../src/shared/memory/graph/index.ts";
import { useTempJocasta } from "../shared/fixture.ts";
import { fakeCompleter, graphitiResponder, serveLlmEndpoint } from "./helpers.ts";

/**
 * Opt-in smoke (`JOCASTA_GRAPHITI_SMOKE=1`): builds and runs the real FalkorDB and Graphiti containers on this machine
 * (Docker Desktop, 0071) under a separate Compose project on a free port (never `jocasta` or 47380, F-04), adds an
 * episode whose extraction comes back through the callback endpoint to a fake completer, and searches it; and restarts
 * the service's container logic with a new token while the container runs (0430). The project and its volume are
 * removed afterwards.
 */
const SMOKE = process.env.JOCASTA_GRAPHITI_SMOKE === "1";
const PROJECT = `jocasta-smoke-${process.pid}`;

useTempJocasta();

const smokeRun: ComposeRun = async (file, args, env) => {
	const child = Bun.spawn(["docker", "compose", "-f", file, "-p", PROJECT, ...args], {
		stdout: "pipe",
		stderr: "pipe",
		windowsHide: true,
		env: { ...process.env, ...env },
	});
	const [stdout, stderr, code] = await Promise.all([
		new Response(child.stdout).text(),
		new Response(child.stderr).text(),
		child.exited,
	]);
	return { code, stdout, stderr };
};

const FACT = { source: "Zhivko", target: "Bun", relation: "PREFERS", fact: "Zhivko prefers Bun over Node" };

describe.skipIf(!SMOKE)("Graphiti in Docker (smoke)", () => {
	test("add and search round-trip through the containers and the callback endpoint", async () => {
		const prompts: string[] = [];
		const completer = fakeCompleter(graphitiResponder([FACT], prompts));
		const endpoint = createLlmEndpoint({ completer });
		const served = serveLlmEndpoint(endpoint);
		expect(graphitiPort()).not.toBe(GRAPHITI_PORT);
		const resource = graphitiResource();
		const containers = new Containers([resource], {
			run: smokeRun,
			env: () => ({ [GRAPHITI_TOKEN_ENV]: graphitiToken() }),
		});
		const deps: GraphDeps = {
			service: graphitiService(),
			use: (fn) => containers.use(resource.name, fn),
			callback: () => ({ baseUrl: containerCallbackUrl(served.port), token: endpoint.token }),
		};
		try {
			await addEpisodes(
				"p-5e0ce5e0ce5e0ce5",
				[
					{
						name: "compact smoke",
						body: "Zhivko said he prefers Bun over Node for every tool.",
						source: "omp-session",
						reference_time: "2026-10-09T10:00:00Z",
					},
				],
				deps,
			);
			expect(prompts.length).toBeGreaterThan(0);
			// Graphiti's main prompts went to the main model and its small prompts to the small one (0341).
			expect([...new Set(completer.used)].sort()).toEqual(["main", "small"]);
			const question = "which runtime does Zhivko prefer?";
			const facts = await searchGraph("p-5e0ce5e0ce5e0ce5", question, 5, deps);
			expect(facts.map((f) => f.fact)).toContain(FACT.fact);
			expect(await searchGraph("p-0000000000000000", question, 5, deps)).toEqual([]);
			// 0430: a wrong token, a text body and a foreign Host are refused.
			const url = `http://127.0.0.1:${graphitiPort()}/search`;
			const body = JSON.stringify({ group_id: "p-5e0ce5e0ce5e0ce5", query: question });
			const json = { "content-type": "application/json" };
			const ask = (headers: Record<string, string>) => fetch(url, { method: "POST", headers, body });
			expect((await ask({ ...json, authorization: "Bearer wrong" })).status).toBe(401);
			expect((await ask({ "content-type": "text/plain", authorization: `Bearer ${graphitiToken()}` })).status).toBe(
				415,
			);
			expect((await ask({ ...json, authorization: `Bearer ${graphitiToken()}`, host: "evil.example" })).status).toBe(
				403,
			);
		} finally {
			served.stop();
			await smokeRun(containers.write(), ["down", "--volumes"]);
		}
	}, 3_600_000);

	test("a restarted service brings the running container onto its token, at start and on a 401", async () => {
		const resource = graphitiResource();
		const baseUrl = `http://127.0.0.1:${graphitiPort()}`;
		// One service run: its own token, container manager and service client, as `jocastaContainers` builds them.
		const serviceRun = (token: string) => {
			const containers = new Containers([resource], { run: smokeRun, env: () => ({ [GRAPHITI_TOKEN_ENV]: token }) });
			const deps: GraphDeps = {
				service: graphitiService(baseUrl, token),
				use: (fn) => containers.use(resource.name, fn),
				callback: () => ({ baseUrl: "http://unused.invalid", token: "unused" }),
			};
			// `/search` checks the token first and calls no model.
			return () => searchGraph("p-5e0ce5e0ce5e0ce5", "anything", 1, deps);
		};
		const file = new Containers([resource], { run: smokeRun }).write();
		const falkordbId = async () => (await smokeRun(file, ["ps", "-q", "falkordb"])).stdout.trim();
		const status = (token: string) =>
			fetch(`${baseUrl}/search`, {
				method: "POST",
				headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
				body: JSON.stringify({ group_id: "p-5e0ce5e0ce5e0ce5", query: "x" }),
			}).then((r) => r.status);
		try {
			const first = serviceRun("token-a");
			expect(await first()).toEqual([]);
			const falkordb = await falkordbId();
			expect(falkordb).not.toBe("");
			// The service restarts with a new token while the container still runs with the old one: its first use
			// recreates Graphiti (Compose sees the environment changed), and FalkorDB stays the same container.
			const second = serviceRun("token-b");
			expect(await second()).toEqual([]);
			expect(await status("token-a")).toBe(401);
			expect((await fetch(`${baseUrl}/health`)).ok).toBe(true);
			// The first run, still up, now gets a 401 and brings the container back onto its token; the second run's next
			// call gets the 401 and takes it back the same way.
			expect(await first()).toEqual([]);
			expect(await status("token-b")).toBe(401);
			expect(await second()).toEqual([]);
			expect(await status("token-b")).toBe(200);
			expect(await falkordbId()).toBe(falkordb);
		} finally {
			await smokeRun(file, ["down", "--volumes"]);
		}
	}, 3_600_000);
});
