import type { Server } from "bun";
import { stopJocastaContainersDetached } from "../../shared/containers/resources.ts";
import { log } from "../../shared/log.ts";
import { removeServiceFile } from "../../shared/service-file.ts";
import type { ServerMessage } from "../protocol.ts";
import type { Herdr } from "./herdr.ts";
import type { Popups } from "./popups.ts";
import type { JocastaService } from "./service.ts";
import type { Whiteboard } from "./whiteboard.ts";

/** A stop step that outlived its bound. */
export class StepTimeout extends Error {}

/** `promise`, or a `StepTimeout` after `ms` (the timer never outlives the promise). */
export async function within<T>(promise: Promise<T>, ms: number): Promise<T> {
	const timeout = Promise.withResolvers<never>();
	const timer = setTimeout(() => timeout.reject(new StepTimeout(`no answer in ${ms / 1000} s`)), ms);
	try {
		return await Promise.race([promise, timeout.promise]);
	} finally {
		clearTimeout(timer);
	}
}

/** One step of a stop: what it stops, bounded by `ms` when it waits on herdr or another process. */
export interface StopStep {
	name: string;
	run: () => unknown;
	ms?: number;
	/**
	 * `always` (the default): every stop, a failed start's too; `all`: only when Jocasta closes as one (0434, 0549);
	 * `finally`: last, after every other step.
	 */
	when?: "always" | "all" | "finally";
	/** Run once the step outlived `ms`, after the log line. */
	onTimeout?: () => void;
}

/** Runs one step; a throw or a step outliving its bound goes to `failed`, and the stop goes on. */
async function runStep(step: StopStep, failed: (name: string, error: unknown) => void): Promise<void> {
	try {
		const done = Promise.resolve(step.run());
		await (step.ms === undefined ? done : within(done, step.ms));
	} catch (error) {
		failed(step.name, error);
		if (error instanceof StepTimeout) step.onTimeout?.();
	}
}

/**
 * Runs `steps` in order: the `all` ones only with `all`, the `finally` ones last. Every step is caught and each bound
 * one is bounded, so the run always ends.
 */
export async function runStopSteps(
	steps: readonly StopStep[],
	all: boolean,
	failed: (name: string, error: unknown) => void,
): Promise<void> {
	try {
		for (const step of steps) {
			if (step.when === "finally" || (step.when === "all" && !all)) continue;
			await runStep(step, failed);
		}
	} finally {
		for (const step of steps) if (step.when === "finally") await runStep(step, failed);
	}
}

/** What a failed start undoes (SRV-A-2): each resource's undo as the start registered it, run newest first. */
export async function undoStart(undo: readonly StopStep[]): Promise<void> {
	await runStopSteps([...undo].reverse(), true, (name, error) =>
		log("warn", "server", `after the failed start, ${name} didn't stop: ${(error as Error).message}`),
	);
}

/** Each stop step's bound in ms: service 30 s, herdr 15 s, containers 20 s, whiteboard 5 s. */
export type StopBounds = Record<"service" | "herdr" | "containers" | "whiteboard", number>;

/** What the stop sequence stops. */
export interface StopParts {
	popups: Pick<Popups, "closeAll">;
	broadcast: (message: ServerMessage) => void;
	herdr: Pick<Herdr, "keepDown" | "stopServer">;
	/** herdr's session, for the log. */
	session: string;
	service: Pick<
		JocastaService,
		"pauseWorking" | "stop" | "cutPausedTurns" | "endThreadLeftovers" | "removeEndedHosts" | "endHunkDaemon"
	>;
	/** Stops Graphiti and FalkorDB, their data kept. */
	stopContainers: () => Promise<void>;
	/** What else Jocasta started that stops with it (0549), in the order registered; read when the stop runs. */
	extraStops: Iterable<StopStep>;
	whiteboard: Pick<Whiteboard, "stop"> | undefined;
	relay: { stop(): unknown };
	/** Clears the model callback (0390). */
	endLlmCallback: () => void;
	server: () => Server<undefined> | undefined;
	bounds?: Partial<StopBounds> | undefined;
}

/**
 * The stop sequence: a failed start stops the service only; Jocasta closing as one (`all`, 0434, 0549) stops
 * everything, in this order. Each step that waits on herdr or another process is bounded (LC-5, LC-8, SRV-A-3): the
 * threads noted and herdr's stop by `herdr`, the service, the containers, whiteboard and the extra stops by their own.
 */
export function stopSteps(parts: StopParts): StopStep[] {
	const { popups, broadcast, herdr, service, whiteboard } = parts;
	const bounds: Partial<StopBounds> = {
		service: 30_000,
		herdr: 15_000,
		containers: 20_000,
		whiteboard: 5_000,
		...parts.bounds,
	};
	return [
		// His open questions go unanswered: Cancel, Deny (0519).
		{ name: "his questions", run: () => popups.closeAll() },
		{ name: "goodbye", run: () => broadcast({ type: "bye" }) },
		// herdr is never started again while Jocasta closes (0431's restart, 0434).
		{ name: "holding herdr down", run: () => herdr.keepDown(), when: "all" },
		// Threads at work come back with `--restore` on the next start (0549).
		{ name: "noting the threads at work", run: () => service.pauseWorking(), ms: bounds.herdr, when: "all" },
		{ name: "the service", run: () => service.stop(), ms: bounds.service },
		// Their omps end before herdr closes their panes, so their turns stay open for `--restore` (0549).
		{ name: "the turns at work", run: () => service.cutPausedTurns(), when: "all" },
		{ name: `herdr session ${parts.session}`, run: () => herdr.stopServer(), ms: bounds.herdr, when: "all" },
		// What the thread panes' omps left, an earlier herdr server's included, once herdr is gone (0486, 0490).
		{ name: "the threads' leftovers", run: () => service.endThreadLeftovers(), when: "all" },
		// The host registry entries of the omps that ended (0547).
		{ name: "omp's host entries", run: () => service.removeEndedHosts(), when: "all" },
		// Once herdr's hunk panes are gone, the hunk session daemon the service started (0434, 0486).
		{ name: "hunk's daemon", run: () => service.endHunkDaemon(), when: "all" },
		{
			name: "Graphiti's containers",
			run: () => parts.stopContainers(),
			ms: bounds.containers,
			when: "all",
			// Containers that don't stop in time stop after this process has gone.
			onTimeout: stopJocastaContainersDetached,
		},
		...parts.extraStops,
		...(whiteboard ? [{ name: "whiteboard", run: () => whiteboard.stop(), ms: bounds.whiteboard }] : []),
		// Also after a failed start: nothing else would close its port.
		{ name: "the collab relay", run: () => parts.relay.stop() },
		{ name: "the model callback", run: () => parts.endLlmCallback(), when: "finally" },
		{ name: "the server", run: () => parts.server()?.stop(true), when: "finally" },
		{ name: "jocasta-service.json", run: () => removeServiceFile(process.pid), when: "finally" },
	];
}

/** Runs the stop sequence once, whoever asks first; `stopped` resolves when it ended. */
export interface Stopper {
	stop(all?: boolean): Promise<void>;
	/**
	 * Registers something else Jocasta started that stops with it (0549), in the order registered. Returns the
	 * function that unregisters it.
	 */
	add(step: StopStep): () => void;
	/** The stop under way, if any. */
	readonly stopping: Promise<void> | undefined;
	/** Resolves once the stop ended; every step is bounded, so it always does. */
	stopped: Promise<void>;
}

/** A `Stopper` over `steps` with the extra stops registered, read when the stop runs. */
export function stopper(steps: (extras: Iterable<StopStep>) => readonly StopStep[]): Stopper {
	const { promise: stopped, resolve: markStopped } = Promise.withResolvers<void>();
	const extras = new Set<StopStep>();
	let stopping: Promise<void> | undefined;
	return {
		stop(all = false) {
			stopping ??= (async () => {
				try {
					await runStopSteps(steps(extras), all, (name, error) =>
						log("warn", "shutdown", `${name} didn't finish: ${error instanceof Error ? error.message : String(error)}`),
					);
				} finally {
					markStopped();
				}
			})();
			return stopping;
		},
		add(step) {
			extras.add(step);
			return () => extras.delete(step);
		},
		get stopping() {
			return stopping;
		},
		stopped,
	};
}
