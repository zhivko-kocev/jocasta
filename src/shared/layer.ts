/**
 * The harness-OS layer's test-only switch (0576): `JOCASTA_LAYER=off` leaves the layer out of thread panes (the
 * `check` and `report_result` tools, the task file's prompt parts, the code index), for K11's same-build comparison.
 * Like `JOCASTA_HERDR_SESSION` (0441) it is for benchmarks and tests; anything else leaves the layer on.
 */
export const LAYER_ENV = "JOCASTA_LAYER";

/** Whether the harness-OS layer is on in this environment (0576): only `off` turns it off. */
export function layerOn(env: Record<string, string | undefined> = process.env): boolean {
	return env[LAYER_ENV] !== "off";
}

/**
 * H2's `herdr` condition (0576): `JOCASTA_HOSTS=off` makes a sandbox service ignore omp's host registry, so a thread's
 * state comes from herdr's events and tick only. For benchmarks and tests, like `LAYER_ENV`.
 */
export const HOSTS_ENV = "JOCASTA_HOSTS";

/** Whether the service reads omp's host registry in this environment (0576): only `off` turns it off. */
export function hostsOn(env: Record<string, string | undefined> = process.env): boolean {
	return env[HOSTS_ENV] !== "off";
}
