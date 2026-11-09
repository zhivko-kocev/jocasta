import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { type Static, Type } from "typebox";
import { UserError } from "../src/shared/errors.ts";
import { parseYamlFile, YamlFileError } from "../src/shared/yaml.ts";

/**
 * Eval variants (0375): `evals/variants/<name>.yaml` changes how each trial's omp starts (0386). Every key is
 * optional; `--variant a --variant b` layers b over a.
 */

type Settings = Record<string, unknown>;

const isPlainObject = (value: unknown): value is Settings =>
	typeof value === "object" && value !== null && !Array.isArray(value);

/** Deep-merges `delta` over `base` as omp merges config overlays: objects merge, everything else replaces. */
export function deepMerge(base: Settings, delta: Settings): Settings {
	const merged: Settings = { ...base };
	for (const [key, value] of Object.entries(delta)) {
		if (value === undefined) continue;
		const current = merged[key];
		merged[key] = isPlainObject(current) && isPlainObject(value) ? deepMerge(current, value) : value;
	}
	return merged;
}

export const VariantSchema = Type.Object(
	{
		/** Variants in this folder layered first, in order (the arms A1–A3 build on each other, 0482). */
		extends: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/**
		 * Which headless harness runs the trial (0482 E4, 0537): omp (default), Claude Code's `claude -p` or opencode's
		 * `opencode run`; the last one wins.
		 */
		harness: Type.Optional(Type.Union([Type.Literal("omp"), Type.Literal("claude"), Type.Literal("opencode")])),
		/** The model, passed as `--model`; plan-first needs it (0482 E3). The last one wins. */
		model: Type.Optional(Type.String({ minLength: 1 })),
		/** Plan first (0482 E3): omp's `--plan-yolo --plan-yolo-into <model>`, so execution stays on the same model. */
		plan_first: Type.Optional(Type.Boolean()),
		/**
		 * Plan first, answered in omp's interactive plan review (0567): the first turn runs omp interactive in a bench
		 * terminal, starting in plan mode, and an extension answers the review with "Approve and execute" (`execute`) or
		 * "Approve and keep context" (`keep`) (`evals/plan-review.ts`). Not with `plan_first`. The last one wins.
		 */
		plan_review: Type.Optional(Type.Union([Type.Literal("execute"), Type.Literal("keep")])),
		/**
		 * Follow-up turns (0482 E2), each run after the first as `omp -c -p <prompt>` (`claude -c -p`) in the same repo;
		 * `{base}` in a prompt becomes the fixture commit. Lists concatenate.
		 */
		followups: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/** Config files, relative to the variant file, each passed as `--config` before the trial's overlay; lists concatenate. */
		configs: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/** A file, relative to the variant file, passed as `--append-system-prompt`; the last one wins. */
		append_system_prompt: Type.Optional(Type.String({ minLength: 1 })),
		/** Extension paths, relative to the variant file, each passed as `-e`; lists concatenate. */
		extensions: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/** Tool names passed as `--tools a,b`; the last one wins. */
		tools: Type.Optional(Type.Array(Type.String({ minLength: 1 }), { minItems: 1 })),
		/** Extra harness arguments, verbatim, after the others; lists concatenate. */
		args: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
		/** omp settings deltas in `config.yml` shape, deep-merged into the trial's `--config` overlay. */
		settings: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
		/** Merged into omp's env. */
		env: Type.Optional(Type.Record(Type.String(), Type.String())),
		/**
		 * The harness-OS layer (0532, 0543 A6): report_result, the task file's brief, the cached checks with the `check`
		 * thread tool, the code index and the ledger line, built into each trial by Jocasta's layer (`evals/layer.ts`).
		 * `index` (the code index behind graph_search, default on) switches for the follow-up study; the last value wins.
		 */
		layer: Type.Optional(Type.Object({ index: Type.Optional(Type.Boolean()) }, { additionalProperties: false })),
	},
	{ additionalProperties: false },
);

type VariantFile = Static<typeof VariantSchema>;

export type Harness = "omp" | "claude" | "opencode";

/** The layer's switches as a trial gets them (see the schema's `layer`). */
export interface LayerSwitches {
	index: boolean;
}

/** The variants of one run, layered in flag order. */
export interface Variant {
	/** The names joined with "+", or "default" without `--variant`. */
	label: string;
	harness: Harness;
	model?: string;
	planFirst: boolean;
	/** Answer omp's interactive plan review with this choice (0567); unset: no interactive plan review. */
	planReview?: "execute" | "keep";
	followups: string[];
	configs: string[];
	appendSystemPrompt?: string;
	extensions: string[];
	tools?: string[];
	args: string[];
	settings?: Record<string, unknown>;
	env: Record<string, string>;
	/** Set when any layered variant has `layer`: the trial runs with the harness-OS layer. */
	layer?: LayerSwitches;
}

export const DEFAULT_VARIANT: Variant = {
	label: "default",
	harness: "omp",
	planFirst: false,
	followups: [],
	configs: [],
	extensions: [],
	args: [],
	env: {},
};

/** Reads and layers `<dir>/<name>.yaml` for each name. An unknown or malformed variant throws a `UserError` (exit 2). */
export function loadVariants(dir: string, names: readonly string[]): Variant {
	if (names.length === 0) return DEFAULT_VARIANT;
	const variant: Variant = {
		...DEFAULT_VARIANT,
		label: names.join("+"),
		followups: [],
		configs: [],
		extensions: [],
		args: [],
		env: {},
	};
	const layer = (name: string, chain: readonly string[]) => {
		// A name is a file stem in the variants folder, never a path.
		const file = join(dir, `${name}.yaml`);
		if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name) || !existsSync(file))
			throw new UserError(`unknown variant: ${name}`);
		if (chain.includes(name)) throw new UserError(`variant ${name} extends itself: ${[...chain, name].join(" → ")}`);
		let doc: VariantFile;
		try {
			doc = parseYamlFile(VariantSchema, readFileSync(file, "utf8"), file).value;
		} catch (error) {
			if (error instanceof YamlFileError) throw new UserError(error.message);
			throw error;
		}
		for (const base of doc.extends ?? []) layer(base, [...chain, name]);
		const path = (relative: string) => {
			const absolute = resolve(dirname(file), relative);
			if (!existsSync(absolute)) throw new UserError(`${file}: ${relative} does not exist`);
			return absolute;
		};
		if (doc.harness !== undefined) variant.harness = doc.harness;
		if (doc.model !== undefined) variant.model = doc.model;
		if (doc.plan_first !== undefined) variant.planFirst = doc.plan_first;
		if (doc.plan_review !== undefined) variant.planReview = doc.plan_review;
		variant.followups.push(...(doc.followups ?? []));
		variant.configs.push(...(doc.configs ?? []).map(path));
		if (doc.append_system_prompt !== undefined) variant.appendSystemPrompt = path(doc.append_system_prompt);
		variant.extensions.push(...(doc.extensions ?? []).map(path));
		if (doc.tools !== undefined) variant.tools = doc.tools;
		variant.args.push(...(doc.args ?? []));
		if (doc.settings !== undefined) variant.settings = deepMerge(variant.settings ?? {}, doc.settings);
		Object.assign(variant.env, doc.env);
		if (doc.layer !== undefined) {
			const before = variant.layer ?? { index: true };
			variant.layer = {
				index: doc.layer.index ?? before.index,
			};
		}
	};
	for (const name of names) layer(name, []);
	if (variant.planFirst && variant.model === undefined)
		throw new UserError(`${variant.label}: plan_first needs a model, so execution stays on the same model (0482)`);
	if (variant.planFirst && variant.planReview !== undefined)
		throw new UserError(
			`${variant.label}: plan_first (--plan-yolo) and plan_review are two ways to plan first; set plan_first: false`,
		);
	if (variant.harness !== "omp") {
		const ompOnly = [
			variant.planFirst && "plan_first",
			variant.planReview !== undefined && "plan_review",
			variant.configs.length > 0 && "configs",
			variant.extensions.length > 0 && "extensions",
			variant.tools !== undefined && "tools",
			variant.settings !== undefined && "settings",
			variant.layer !== undefined && "layer",
			variant.harness === "opencode" && variant.appendSystemPrompt !== undefined && "append_system_prompt",
		].filter((key): key is string => typeof key === "string");
		if (ompOnly.length > 0)
			throw new UserError(`${variant.label}: ${ompOnly.join(", ")} apply to omp only, not ${variant.harness}`);
	}
	return variant;
}

/** The harness flags a variant adds; omp's `settings` go into the trial's `--config` overlay instead. */
export function variantArgs(variant: Variant): string[] {
	return [
		...(variant.model === undefined ? [] : ["--model", variant.model]),
		...(variant.planFirst && variant.model !== undefined ? ["--plan-yolo", "--plan-yolo-into", variant.model] : []),
		...variant.configs.flatMap((config) => ["--config", config]),
		...(variant.appendSystemPrompt === undefined ? [] : ["--append-system-prompt", variant.appendSystemPrompt]),
		...variant.extensions.flatMap((extension) => ["-e", extension]),
		...(variant.tools === undefined ? [] : ["--tools", variant.tools.join(",")]),
		...variant.args,
	];
}
