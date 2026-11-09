import type {
	Completer,
	CompletionRequest,
	createLlmEndpoint,
	GraphModel,
} from "../../src/frontier/server/llm-endpoint.ts";

/** One call a fake completer answers: the reply text, or an error the model call fails with. */
export type FakeResponder = (model: GraphModel, request: CompletionRequest) => string | Error;

export interface FakeCompleter extends Completer {
	/** The Graphiti model name each call went to, in order. */
	used: GraphModel[];
}

/** A completer answering through `respond`; `main` and `small` answer as models `fake-main` and `fake-small`. */
export function fakeCompleter(respond: FakeResponder): FakeCompleter {
	const used: GraphModel[] = [];
	return {
		used,
		async complete(model, request) {
			used.push(model);
			const reply = respond(model, request);
			if (reply instanceof Error) throw reply;
			return {
				model: `fake-${model}`,
				provider: null,
				text: reply,
				stopReason: "stop",
				usage: { input: 10, output: 5, cacheRead: 2, cacheWrite: 1 },
			};
		},
	};
}

type Schema = {
	type?: string | string[];
	properties?: Record<string, Schema>;
	required?: string[];
	items?: Schema;
	anyOf?: Schema[];
	$ref?: string;
	$defs?: Record<string, Schema>;
	enum?: unknown[];
	default?: unknown;
};

/** The JSON Schema the endpoint appended to the system prompt (`jsonInstruction`), if any. */
export function requestedSchema(system: string): Schema | undefined {
	const marker = "matches this JSON Schema, with no other text and no code fence:\n";
	const at = system.indexOf(marker);
	if (at < 0) return undefined;
	const rest = system.slice(at + marker.length);
	const end = rest.indexOf("\n");
	return JSON.parse(end < 0 ? rest : rest.slice(0, end)) as Schema;
}

/** The smallest value that satisfies `schema`: required fields only, empty lists, zeros, nulls where allowed. */
export function minimalInstance(schema: Schema, root: Schema = schema): unknown {
	if (schema.$ref) {
		const name = schema.$ref.split("/").at(-1) ?? "";
		return minimalInstance(root.$defs?.[name] ?? {}, root);
	}
	if (schema.default !== undefined) return schema.default;
	if (schema.enum) return schema.enum[0];
	if (schema.anyOf) {
		if (schema.anyOf.some((s) => s.type === "null")) return null;
		return minimalInstance(schema.anyOf[0] ?? {}, root);
	}
	const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
	if (type === "object" || schema.properties) {
		const out: Record<string, unknown> = {};
		for (const key of schema.required ?? []) out[key] = minimalInstance(schema.properties?.[key] ?? {}, root);
		return out;
	}
	if (type === "array") return [];
	if (type === "string") return "";
	if (type === "integer" || type === "number") return 0;
	if (type === "boolean") return false;
	return null;
}

/** A relation the scripted extractor reports for every episode. */
export interface ScriptedFact {
	source: string;
	target: string;
	relation: string;
	fact: string;
}

/**
 * A faux model that answers Graphiti's structured prompts: entity extraction returns the facts' endpoints, edge
 * extraction returns the facts, and every other prompt gets the smallest valid object for its schema.
 */
export function graphitiResponder(facts: readonly ScriptedFact[], seen: string[] = []): FakeResponder {
	return (_model, request) => {
		const system = request.system ?? "";
		seen.push(system);
		const schema = requestedSchema(system);
		if (!schema) return "ok";
		const value = minimalInstance(schema) as Record<string, unknown>;
		if (schema.properties?.extracted_entities) {
			const names = [...new Set(facts.flatMap((f) => [f.source, f.target]))];
			value.extracted_entities = names.map((name) => ({ name, entity_type_id: 0, episode_indices: [0] }));
		}
		if (schema.properties?.edges) {
			value.edges = facts.map((f) => ({
				source_entity_name: f.source,
				target_entity_name: f.target,
				relation_type: f.relation,
				fact: f.fact,
				valid_at: null,
				invalid_at: null,
				episode_indices: [0],
			}));
		}
		// Fenced, as models often answer; the endpoint strips it.
		return `\`\`\`json\n${JSON.stringify(value)}\n\`\`\``;
	};
}

export interface ServedEndpoint {
	port: number;
	stop(): void;
}

/** Serves the endpoint on its own 127.0.0.1 port, standing in for the service's server in tests. */
export function serveLlmEndpoint(endpoint: ReturnType<typeof createLlmEndpoint>): ServedEndpoint {
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		async fetch(request) {
			return (await endpoint.handle(request)) ?? new Response("Not found", { status: 404 });
		},
	});
	server.unref();
	return { port: server.port as number, stop: () => void server.stop(true) };
}
