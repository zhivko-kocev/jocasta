import * as cli from "./cli.ts";
import * as client from "./client.ts";
import * as config from "./config.ts";

let failed = 0;
const fail = (message: string) => {
	console.error(message);
	failed++;
};

const defaults = config.makeConfig() as unknown as Record<string, unknown>;
if (defaults.timeoutMs !== 5000) fail(`makeConfig().timeoutMs is ${defaults.timeoutMs}, expected 5000`);

const parsed = cli.parseArgs(["--timeout", "250", "/health"]) as unknown as {
	config: Record<string, unknown>;
	path: string;
};
if (parsed.config.timeoutMs !== 250 || parsed.path !== "/health")
	fail(`parseArgs(--timeout 250 /health) gave ${JSON.stringify(parsed)}`);
const plain = cli.parseArgs(["/health"]) as unknown as { config: Record<string, unknown> };
if (plain.config.timeoutMs !== 5000) fail(`parseArgs(/health) gave timeoutMs ${plain.config.timeoutMs}`);

// A fetch that never answers on its own: only an abort ends it.
const hanging = ((_url: unknown, init?: RequestInit) =>
	new Promise<Response>((_resolve, reject) => {
		init?.signal?.addEventListener("abort", () => reject(new Error("aborted")));
	})) as unknown as typeof fetch;
const quick = config.makeConfig({ timeoutMs: 50 } as unknown as Partial<config.ClientConfig>);
const outcome = await Promise.race([
	client.getJson(quick, "/slow", hanging).then(
		() => "resolved",
		() => "aborted",
	),
	new Promise<string>((resolve) => setTimeout(() => resolve("hung"), 2000)),
]);
if (outcome !== "aborted") fail(`a request past timeoutMs 50 ${outcome}; expected it to be aborted`);

const ok = (() => Promise.resolve(Response.json({ up: true }))) as unknown as typeof fetch;
const body = await client.getJson(config.makeConfig(), "/status", ok).catch((error: Error) => error.message);
if (JSON.stringify(body) !== JSON.stringify({ up: true })) fail(`a quick request gave ${JSON.stringify(body)}`);

if (failed > 0) process.exit(1);
console.log("timeoutMs: all checks pass");
