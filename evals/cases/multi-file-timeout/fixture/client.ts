import type { ClientConfig } from "./config.ts";

/** GETs `path` under the configured base URL and parses the JSON body. */
export async function getJson(config: ClientConfig, path: string, fetchImpl: typeof fetch = fetch): Promise<unknown> {
	const response = await fetchImpl(new URL(path, config.baseUrl));
	if (!response.ok) throw new Error(`GET ${path}: ${response.status}`);
	return response.json();
}
