import { LLM_PATH } from "../../../frontier/server/llm-endpoint.ts";

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

/** The service's server registers its route here once it listens; `undefined` clears it. */
export function setLlmCallback(callback: LlmCallback | undefined): void {
	registered = callback;
}

/** The callback the next Graphiti add carries: the service's `/llm/v1` (0390); graph writes wait while it is closed. */
export function llmCallback(): LlmCallback {
	if (!registered) throw new Error("Jocasta's model endpoint isn't listening yet");
	return registered;
}
