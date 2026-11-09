/** The model endpoint's base path; an OpenAI client posts to `<base>/chat/completions`. */
export const LLM_PATH = "/llm/v1";

/** How the Graphiti container reaches the service's model endpoint (0334, 0390). */
export interface LlmCallback {
	baseUrl: string;
	token: string;
}

let registered: LlmCallback | undefined;

/** The endpoint's URL as a container sees a port on this machine (Docker Desktop's `host.docker.internal`, 0071). */
export function containerCallbackUrl(port: number): string {
	return `http://host.docker.internal:${port}${LLM_PATH}`;
}

/**
 * Why Graphiti stays off on Linux's own Docker engine (SEC-LOC-10, 0584): its `host.docker.internal` doesn't reach the
 * endpoint on 127.0.0.1, so memory stays off there; the services row says "Graphiti down (<this>)".
 */
export const CALLBACK_UNREACHABLE = "needs Docker Desktop on Linux: the container can't reach Jocasta's model endpoint";

/** The service's server registers its route here once it listens; `undefined` clears it. */
export function setLlmCallback(callback: LlmCallback | undefined): void {
	registered = callback;
}

/** The callback the next Graphiti add carries: the service's `/llm/v1` (0390); graph writes wait while it is closed. */
export function llmCallback(): LlmCallback {
	if (!registered) throw new Error("Jocasta's model endpoint isn't listening yet");
	return registered;
}
