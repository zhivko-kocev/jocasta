import { getJson } from "./client.ts";
import { type ClientConfig, makeConfig } from "./config.ts";

/** Parses `[--base-url <url>] <path>`. */
export function parseArgs(argv: readonly string[]): { config: ClientConfig; path: string } {
	const overrides: Partial<ClientConfig> = {};
	let path = "/status";
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === "--base-url") overrides.baseUrl = argv[++i];
		else if (arg !== undefined) path = arg;
	}
	return { config: makeConfig(overrides), path };
}

if (import.meta.main) {
	const { config, path } = parseArgs(process.argv.slice(2));
	console.log(JSON.stringify(await getJson(config, path), null, 2));
}
