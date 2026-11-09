import { existsSync } from "node:fs";
import { join } from "node:path";
import { REPO_ROOT } from "../bench/lib/args.ts";
import { UserError } from "../src/shared/errors.ts";
import type { LayerSwitches } from "./variants.ts";

/**
 * A6's seam (0543): A6 = A3 + the harness-OS layer (0532), built into each trial by Jocasta's own code: the task
 * file's brief and Done-when through `--append-system-prompt` (0540), the `report_result` tool (0539), the cached
 * checks with gate replies and, when switched on, the `check` thread tool (0542, 0546) and the code index behind
 * `graph_search` (0562). The plan review is the bench's, not the layer's (`plan-review.ts`, 0567). The ledger needs
 * nothing from it: the bench reads every trial's saved sessions itself, with the product's ledger (`src/shared/ledger.ts`).
 *
 * The layer is Jocasta's build, not the bench's: the bench imports {@link LAYER_MODULE}, which exports
 * `benchLayer: BenchLayer`. Until that module exists, a trial with a `layer` variant stops with
 * {@link LayerUnavailableError} before any model call; nothing pretends to run it.
 */

/** The module Jocasta's layer build provides, relative to the repo root; it exports `benchLayer`. */
export const LAYER_MODULE = "src/frontier/layer/bench.ts";

/** One trial as the layer sees it. */
export interface LayerTrial {
	/** The trial's root (outside the repo); the layer keeps its own state under it. */
	root: string;
	/** The trial's checkout, where the harness runs. */
	repo: string;
	/** The fixture commit the task starts from. */
	baseCommit: string;
	/** The task prompt every arm gets. */
	prompt: string;
	/**
	 * The variant's `--append-system-prompt` file (Ponytail), absolute: omp keeps only the last one given, so the layer's
	 * own prompt file starts with it, as a thread's launch prompt does (0540).
	 */
	appendSystemPrompt?: string;
	switches: LayerSwitches;
	env: Record<string, string | undefined>;
}

/** What the layer adds to a trial's omp, and how it is taken down. */
export interface LayerSetup {
	/** omp flags added after the variant's (e.g. `--append-system-prompt <task.md>`, the MCP endpoint's config). */
	args: string[];
	/** omp settings deep-merged into the trial's `--config` overlay. */
	settings: Record<string, unknown>;
	env: Record<string, string>;
	/** Stops what `setUp` started; awaited after the last turn, also when the trial fails. */
	close(): Promise<void>;
}

export interface BenchLayer {
	setUp(trial: LayerTrial): Promise<LayerSetup>;
}

export class LayerUnavailableError extends UserError {}

export const layerMissingMessage = (): string =>
	`A6 needs Jocasta's layer build: ${LAYER_MODULE} exporting \`benchLayer\` (setUp(trial) → args, settings, env, close) is not in this checkout`;

/** Whether the layer build is in this checkout. */
export const layerBuilt = (root = REPO_ROOT): boolean => existsSync(join(root, LAYER_MODULE));

/** Jocasta's layer for a trial; throws {@link LayerUnavailableError} until the build provides it. */
export async function loadBenchLayer(root = REPO_ROOT): Promise<BenchLayer> {
	if (!layerBuilt(root)) throw new LayerUnavailableError(layerMissingMessage());
	// Dynamic on purpose: the module is a later build's and may be absent; a static import would break typecheck now.
	const module = (await import(join(root, LAYER_MODULE))) as { benchLayer?: BenchLayer };
	if (typeof module.benchLayer?.setUp !== "function")
		throw new LayerUnavailableError(`${LAYER_MODULE} does not export benchLayer with setUp(trial)`);
	return module.benchLayer;
}
