import { describe, expect, jest, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
	type ComposeRun,
	ContainerError,
	Containers,
	composeFile,
	composeProject,
	dockerEngineRunning,
	type Resource,
	renderCompose,
} from "../../src/shared/containers/index.ts";
import {
	GRAPHITI_PORT,
	GRAPHITI_TOKEN_ENV,
	graphitiPort,
	graphitiResource,
} from "../../src/shared/containers/resources.ts";
import { GraphError } from "../../src/shared/memory/graph/index.ts";
import { useTempJocasta } from "../shared/fixture.ts";

const env = useTempJocasta();

function recorder(codes: number[] = []): {
	run: ComposeRun;
	calls: { file: string; args: string[]; env: Record<string, string> | undefined }[];
} {
	const calls: { file: string; args: string[]; env: Record<string, string> | undefined }[] = [];
	return {
		calls,
		run: async (file, args, extra) => {
			calls.push({ file, args, env: extra });
			return { code: codes.shift() ?? 0, stdout: "", stderr: "boom" };
		},
	};
}

const tiny: Resource = {
	name: "demo",
	idleMs: 30,
	services: [
		{
			name: "demo",
			image: "example/demo@sha256:abc",
			healthcheck: { test: ["CMD", "true"], interval: "1s", timeout: "1s", retries: 1, startPeriod: "1s" },
		},
	],
};

describe("compose.yaml", () => {
	test("pins official images by digest, builds Jocasta's own from its context, keeps FalkorDB off the host", () => {
		const doc = Bun.YAML.parse(renderCompose([graphitiResource()])) as {
			name: string;
			services: Record<string, Record<string, unknown>>;
			volumes: Record<string, unknown>;
		};
		// A config dir other than the one in his home gets its own project (env.config is a temp dir).
		expect(doc.name).toMatch(/^jocasta-[0-9a-f]{8}$/);
		expect(doc.name).toBe(composeProject());
		const { falkordb, graphiti } = doc.services;
		expect(falkordb?.image).toMatch(/^falkordb\/falkordb:[\w.]+@sha256:[0-9a-f]{64}$/);
		expect(falkordb?.ports).toBeUndefined();
		expect(falkordb?.restart).toBe("on-failure");
		expect(graphiti?.image).toMatch(/^jocasta\/graphiti:[0-9a-f]{12}$/);
		expect(graphiti?.build).toEqual({ context: join(env.config, "containers", "graphiti") });
		// A test's own project never takes his port (F-04); the token comes from the environment, not the file (0430).
		expect(graphiti?.ports).toEqual([`127.0.0.1:${graphitiPort()}:8000`]);
		expect(graphitiPort()).not.toBe(GRAPHITI_PORT);
		expect(graphiti?.environment).toMatchObject({
			GRAPHITI_TELEMETRY_ENABLED: "false",
			GRAPHITI_TOKEN: `\${${GRAPHITI_TOKEN_ENV}:-}`,
		});
		expect(graphiti?.depends_on).toEqual({ falkordb: { condition: "service_healthy" } });
		expect(graphiti?.healthcheck).toMatchObject({ retries: 12, start_period: "120s" });
		// Python as PID 1 ignores SIGTERM; Docker's init passes it on so the idle stop doesn't end in a kill (F6).
		expect(graphiti?.init).toBe(true);
		expect(falkordb?.init).toBeUndefined();
		expect(doc.volumes).toEqual({ "falkordb-data": {} });
	});

	test("only the config dir in his home runs under the project `jocasta`", () => {
		const own = composeProject();
		const saved = process.env.JOCASTA_CODING_AGENT_DIR;
		try {
			process.env.JOCASTA_CODING_AGENT_DIR = join(homedir(), ".jocasta", "agent");
			expect(composeProject()).toBe("jocasta");
		} finally {
			process.env.JOCASTA_CODING_AGENT_DIR = saved;
		}
		expect(composeProject()).toBe(own);
	});

	test("the Graphiti image pins python:3.12-slim by digest, every package by hash, and the model revision", () => {
		const build = graphitiResource().services.find((s) => s.name === "graphiti")?.build ?? {};
		const dockerfile = build.Dockerfile ?? "";
		expect(dockerfile).toMatch(/^FROM python:3\.12-slim@sha256:[0-9a-f]{64}$/m);
		expect(dockerfile).toContain("pip install --require-hashes --no-deps -r /app/requirements.txt");
		expect(dockerfile).toMatch(/EMBEDDING_REVISION=[0-9a-f]{40}/);
		expect(dockerfile).toContain("GRAPHITI_TELEMETRY_ENABLED=false");
		const requirements = build["requirements.txt"] ?? "";
		expect(requirements).toMatch(/^torch==[\d.]+\+cpu \\$/m);
		// Every pinned package carries at least one hash.
		for (const pin of requirements.split(/\n(?=[a-z])/).filter((block) => /^[a-z]/.test(block))) {
			expect(pin).toMatch(/--hash=sha256:[0-9a-f]{64}/);
		}
	});

	test("the Graphiti service runs as a non-root user and downloads nothing at run time (0484)", () => {
		const dockerfile = graphitiResource().services.find((s) => s.name === "graphiti")?.build?.Dockerfile ?? "";
		const users = [...dockerfile.matchAll(/^USER (.+)$/gm)].map((m) => m[1]);
		expect(users).toEqual(["10001"]);
		// Installs run as root before it; the model is baked, and the service starts, as that user.
		expect(dockerfile.indexOf("USER 10001")).toBeGreaterThan(dockerfile.indexOf("pip install"));
		expect(dockerfile.indexOf("USER 10001")).toBeLessThan(dockerfile.indexOf("SentenceTransformer("));
		expect(dockerfile).toContain("ENV HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1");
		expect(dockerfile).toContain("GRAPHITI_TELEMETRY_ENABLED=false");
	});
});

describe("Containers", () => {
	test("writes compose.yaml and the build context, then starts the services once and waits for health", async () => {
		const { run, calls } = recorder();
		const containers = new Containers([graphitiResource()], { run, env: () => ({ [GRAPHITI_TOKEN_ENV]: "t" }) });
		await Promise.all([containers.ensure("graphiti"), containers.ensure("graphiti")]);
		await containers.ensure("graphiti");
		expect(calls).toHaveLength(1);
		expect(calls[0]?.file).toBe(composeFile());
		expect(calls[0]?.args.slice(0, 3)).toEqual(["up", "--detach", "--wait"]);
		expect(calls[0]?.args.slice(-2)).toEqual(["falkordb", "graphiti"]);
		expect(calls[0]?.env).toEqual({ [GRAPHITI_TOKEN_ENV]: "t" });
		expect(readFileSync(composeFile(), "utf8")).not.toContain("JOCASTA_GRAPHITI_TOKEN:-t");
		expect(readFileSync(composeFile(), "utf8")).toStartWith("# Written by Jocasta");
		const context = join(env.config, "containers", "graphiti");
		expect(existsSync(join(context, "Dockerfile"))).toBe(true);
		expect(existsSync(join(context, "service.py"))).toBe(true);
	});

	test("a failed start is an error naming the resource, and the next use tries again", async () => {
		const { run, calls } = recorder([1]);
		const containers = new Containers([tiny], { run });
		await expect(containers.ensure("demo")).rejects.toThrow(ContainerError);
		await containers.ensure("demo");
		expect(calls).toHaveLength(2);
	});

	test("stops a resource after its idle time, never while a call is in flight", async () => {
		const { run, calls } = recorder();
		const containers = new Containers([tiny], { run });
		const { promise: gate, resolve: open } = Promise.withResolvers<void>();
		const { promise: started, resolve: start } = Promise.withResolvers<void>();
		const call = containers.use("demo", () => {
			start();
			return gate;
		});
		await started;
		jest.useFakeTimers();
		try {
			jest.advanceTimersByTime(tiny.idleMs * 10);
			expect(calls.map((c) => c.args[0])).toEqual(["up"]);
			open();
			await call;
			expect(containers.isRunning("demo")).toBe(true);
			jest.advanceTimersByTime(tiny.idleMs);
		} finally {
			jest.useRealTimers();
		}
		expect(calls.map((c) => c.args[0])).toEqual(["up", "stop"]);
		expect(containers.isRunning("demo")).toBe(false);
	});

	test("starts the resource again when its service went away under a call", async () => {
		const { run, calls } = recorder();
		const containers = new Containers([tiny], { run });
		let attempts = 0;
		const result = await containers.use("demo", async () => {
			attempts++;
			if (attempts === 1) throw Object.assign(new Error("refused"), { code: "ConnectionRefused" });
			return "ok";
		});
		expect(result).toBe("ok");
		expect(calls.map((c) => c.args[0])).toEqual(["up", "up"]);
	});

	test("a 401 from Graphiti (another run's token) brings it up again with this run's token and retries once", async () => {
		const { run, calls } = recorder();
		let token = "a";
		const containers = new Containers([graphitiResource()], { run, env: () => ({ [GRAPHITI_TOKEN_ENV]: token }) });
		await containers.ensure("graphiti");
		token = "b";
		let attempts = 0;
		const result = await containers.use("graphiti", async () => {
			attempts++;
			if (attempts === 1) throw new GraphError("graphiti /search: 401 invalid token", 401);
			return "ok";
		});
		expect(result).toBe("ok");
		expect(calls.map((c) => [c.args[0], c.env?.[GRAPHITI_TOKEN_ENV]])).toEqual([
			["up", "a"],
			["up", "b"],
		]);
		// Any other error is the caller's, without a restart.
		await expect(
			containers.use("graphiti", () => Promise.reject(new GraphError("graphiti /search: 500 boom", 500))),
		).rejects.toThrow("500 boom");
		expect(calls).toHaveLength(2);
	});

	test("the last process using a resource stops it; one finishing earlier leaves it running for the others", async () => {
		const { run, calls } = recorder();
		let now = 1_000_000;
		const alive = new Set([101, 202]);
		// A long idle time: only the explicit stops below run.
		const slow: Resource = { ...tiny, idleMs: 60_000 };
		const options = { run, alive: (pid: number) => alive.has(pid), now: () => now };
		const host = new Containers([slow], { ...options, pid: 101 });
		const harness = new Containers([slow], { ...options, pid: 202 });
		await host.use("demo", async () => {});
		await harness.use("demo", async () => {});
		// The host's idle time ran out, but the harness used it just now: still up.
		now += 10;
		await host.stop("demo");
		expect(calls.map((c) => c.args[0])).toEqual(["up", "up"]);
		// The harness is the last user: it stops it.
		await harness.stop("demo");
		expect(calls.map((c) => c.args[0])).toEqual(["up", "up", "stop"]);

		// A harness that exited without releasing its lease doesn't keep the resource up.
		await harness.use("demo", async () => {});
		await host.use("demo", async () => {});
		alive.delete(202);
		await host.stop("demo");
		expect(calls.map((c) => c.args[0])).toEqual(["up", "up", "stop", "up", "up", "stop"]);
	});

	test("says while it is starting a resource, for the services row's 'starting' (0444)", async () => {
		const started = Promise.withResolvers<void>();
		const containers = new Containers([tiny], {
			run: async () => {
				await started.promise;
				return { code: 0, stdout: "", stderr: "" };
			},
		});
		expect(containers.isStarting("demo")).toBe(false);
		const up = containers.ensure("demo");
		expect(containers.isStarting("demo")).toBe(true);
		started.resolve();
		await up;
		expect(containers.isStarting("demo")).toBe(false);
		expect(containers.isRunning("demo")).toBe(true);
	});
});

describe("dockerEngineRunning (0444)", () => {
	const only =
		(...paths: string[]) =>
		(path: string) =>
			paths.includes(path);

	test("on Windows: Docker Desktop's engine pipe or docker_engine's", () => {
		const win = { platform: "win32" as const, env: {} };
		expect(dockerEngineRunning({ ...win, exists: only("\\\\.\\pipe\\dockerDesktopLinuxEngine") })).toBe(true);
		expect(dockerEngineRunning({ ...win, exists: only("\\\\.\\pipe\\docker_engine") })).toBe(true);
		expect(dockerEngineRunning({ ...win, exists: only() })).toBe(false);
	});

	test("elsewhere: /var/run/docker.sock or ~/.docker/run/docker.sock", () => {
		const linux = { platform: "linux" as const, env: {}, home: "/home/acme" };
		expect(dockerEngineRunning({ ...linux, exists: only("/var/run/docker.sock") })).toBe(true);
		expect(dockerEngineRunning({ ...linux, exists: only(join("/home/acme", ".docker", "run", "docker.sock")) })).toBe(
			true,
		);
		expect(dockerEngineRunning({ ...linux, exists: only() })).toBe(false);
	});

	test("DOCKER_HOST: its pipe or socket is checked; tcp:// counts as running", () => {
		const pipe = { platform: "win32" as const, env: { DOCKER_HOST: "npipe:////./pipe/acme_engine" } };
		expect(dockerEngineRunning({ ...pipe, exists: only("\\\\.\\pipe\\acme_engine") })).toBe(true);
		expect(dockerEngineRunning({ ...pipe, exists: only("\\\\.\\pipe\\docker_engine") })).toBe(false);
		const unix = { platform: "linux" as const, env: { DOCKER_HOST: "unix:///run/acme.sock" } };
		expect(dockerEngineRunning({ ...unix, exists: only("/run/acme.sock") })).toBe(true);
		expect(dockerEngineRunning({ ...unix, exists: only("/var/run/docker.sock") })).toBe(false);
		expect(
			dockerEngineRunning({ platform: "linux", env: { DOCKER_HOST: "tcp://127.0.0.1:2375" }, exists: only() }),
		).toBe(true);
	});
});
