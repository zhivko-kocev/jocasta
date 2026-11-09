import { containerCallbackUrl, LLM_PATH, setLlmCallback } from "../../shared/memory/graph/callback.ts";
import { createLlmEndpoint } from "./llm-endpoint.ts";
import type { Route } from "./routes.ts";

/** Whether `path` is under the model callback's `/llm/v1`. */
export function isLlmPath(path: string): boolean {
	return path === LLM_PATH || path.startsWith(`${LLM_PATH}/`);
}

/**
 * Graphiti's model callback (0334, 0390): its container calls back into this process for its model calls, under
 * `/llm/v1`; `listening` points the callback at this server's port, `end` clears it.
 */
export function llmCallback(): { route: Route; listening(port: number): void; end(): void } {
	const llm = createLlmEndpoint();
	return {
		route: { matches: isLlmPath, handle: (request) => llm.handle(request) },
		listening: (port) => setLlmCallback({ baseUrl: containerCallbackUrl(port), token: llm.token }),
		end: () => setLlmCallback(undefined),
	};
}
