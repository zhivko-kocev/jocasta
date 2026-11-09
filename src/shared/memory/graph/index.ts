import { nativeLinuxDocker } from "../../containers/index.ts";
import { graphitiPort, graphitiResource, graphitiToken, jocastaContainers } from "../../containers/resources.ts";
import { CALLBACK_UNREACHABLE, type LlmCallback, llmCallback } from "./callback.ts";

/** One fact from the graph's search: an edge Graphiti extracted, with when it held. */
export interface GraphFact {
	fact: string;
	name: string;
	valid_at: string | null;
	invalid_at: string | null;
	created_at: string | null;
	/** The episodes it came from (`<kind> <session>`, 0484); absent from an image built before 0484. */
	sources?: string[];
}

/** One episode as the Graphiti service takes it (`docker/graphiti/service.py`). */
export interface Episode {
	name: string;
	body: string;
	source: string;
	reference_time: string;
}

/** The Graphiti service's HTTP API (test seam: tests serve a fake). */
export interface GraphService {
	add(groupId: string, episodes: readonly Episode[], llm: LlmCallback): Promise<number>;
	search(groupId: string, query: string, limit: number): Promise<GraphFact[]>;
}

/** What adding and searching need; the real service, container manager and callback by default. */
export interface GraphDeps {
	service: GraphService;
	/** Runs `fn` with the Graphiti containers up (0073). */
	use<T>(fn: () => Promise<T>): Promise<T>;
	callback(): LlmCallback;
}

export class GraphError extends Error {
	constructor(
		message: string,
		/** The service's HTTP status, when the service answered with an error. */
		readonly status?: number,
	) {
		super(message);
		this.name = "GraphError";
	}
}

/** Graphiti's group id for a project: the project id itself, so a project's graph is its own FalkorDB graph (0277). */
export function projectGroup(projectId: string): string {
	if (!/^p-[0-9a-f]{16}$/.test(projectId)) throw new GraphError(`not a project id: ${JSON.stringify(projectId)}`);
	return projectId;
}

/** How long a call may take before it fails (CO-9): an extraction runs Graphiti's LLM calls, a search none. */
interface GraphTimeouts {
	episodesMs: number;
	searchMs: number;
}

const TIMEOUTS: GraphTimeouts = { episodesMs: 600_000, searchMs: 30_000 };

async function post<T>(url: string, token: string, body: unknown, timeoutMs: number): Promise<T> {
	let response: Response;
	let text: string;
	try {
		response = await fetch(url, {
			method: "POST",
			headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
			body: JSON.stringify(body),
			signal: AbortSignal.timeout(timeoutMs),
		});
		text = await response.text();
	} catch (error) {
		if (error instanceof Error && error.name === "TimeoutError") {
			throw new GraphError(`graphiti ${new URL(url).pathname}: no answer in ${Math.round(timeoutMs / 1000)} s`);
		}
		throw error;
	}
	if (!response.ok) {
		let message = text;
		try {
			const parsed: unknown = JSON.parse(text);
			if (parsed && typeof parsed === "object" && "error" in parsed && typeof parsed.error === "string") {
				message = parsed.error;
			}
		} catch {}
		throw new GraphError(`graphiti ${new URL(url).pathname}: ${response.status} ${message}`, response.status);
	}
	return JSON.parse(text) as T;
}

/**
 * The Graphiti service at `baseUrl` (the container's port on 127.0.0.1 by default), called with the run's bearer
 * token (0430). A call that outlasts its timeout fails like any other (the watcher's backoff); `timeouts` is a test seam.
 */
export function graphitiService(
	baseUrl = `http://127.0.0.1:${graphitiPort()}`,
	token: string = graphitiToken(),
	timeouts: GraphTimeouts = TIMEOUTS,
): GraphService {
	return {
		async add(groupId, episodes, llm) {
			const result = await post<{ added: number }>(
				`${baseUrl}/episodes`,
				token,
				{
					group_id: groupId,
					episodes,
					llm: { base_url: llm.baseUrl, api_key: llm.token },
				},
				timeouts.episodesMs,
			);
			return result.added;
		},
		async search(groupId, query, limit) {
			const result = await post<{ facts: GraphFact[] }>(
				`${baseUrl}/search`,
				token,
				{ group_id: groupId, query, limit },
				timeouts.searchMs,
			);
			return result.facts;
		},
	};
}

/**
 * The real Graphiti service, behind the shared container manager, with the service's callback. On Linux's own Docker
 * engine memory stays off (SEC-LOC-10, 0584): every call fails at once and no container starts.
 */
export function defaultGraphDeps(): GraphDeps {
	const name = graphitiResource().name;
	return {
		service: graphitiService(),
		use: (fn) => {
			if (nativeLinuxDocker()) return Promise.reject(new GraphError(`Graphiti ${CALLBACK_UNREACHABLE}`));
			return jocastaContainers().use(name, fn);
		},
		callback: llmCallback,
	};
}

/**
 * Adds episodes to a project's graph (0389, 0400): the watcher's summaries; extraction calls come back to the
 * service's model endpoint (0390). Episodes without text are skipped; none left makes no call.
 */
export async function addEpisodes(
	projectId: string,
	episodes: readonly Episode[],
	deps: GraphDeps = defaultGraphDeps(),
): Promise<void> {
	const group = projectGroup(projectId);
	const kept = episodes.filter((episode) => episode.body.trim().length > 0);
	if (kept.length === 0) return;
	await deps.use(() => deps.service.add(group, kept, deps.callback()));
}

/** A project's graph facts for `query` (5.5); never another project's. No model call. */
export async function searchGraph(
	projectId: string,
	query: string,
	limit = 10,
	deps: GraphDeps = defaultGraphDeps(),
): Promise<GraphFact[]> {
	const group = projectGroup(projectId);
	return deps.use(() => deps.service.search(group, query, limit));
}
