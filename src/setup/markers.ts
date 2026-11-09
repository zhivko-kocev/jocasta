/**
 * What setup has done, kept in `~/.jocasta/agent/setup/` (0466, 0468): one marker per numbered migration it applied
 * (`<NNNN>-<slug>.json`), plus `calendar.json` once the calendar's secret is stored. Each marker names how to undo what
 * it did, so `jocasta uninstall` removes exactly what setup added.
 */
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { asObject, writeAtomically } from "../shared/fsx.ts";

/** How one thing setup added is undone (0468). */
export type Undo =
	/** A file setup wrote. */
	| { kind: "file"; path: string }
	/**
	 * A folder setup made (the Terminal fragment's, a skill's), with the file it wrote there (0484): the folder goes only
	 * while that file is in it. A marker from before 0484 has no `file`.
	 */
	| { kind: "dir"; path: string; file?: string }
	/** Jocasta's servers in an omp `mcp.json`; his other servers stay. */
	| { kind: "mcp"; path: string; servers: string[] }
	/** herdr's omp integration (`herdr integration uninstall omp`), whose file is `path`. */
	| { kind: "herdr-integration"; path: string }
	/** A herdr plugin setup linked (`herdr plugin unlink <id>`), from folder `path` (0553). */
	| { kind: "herdr-plugin"; id: string; path: string }
	/**
	 * Keys setup added to one of his omp files (`keybindings.yml`, `lsp.json`; 0479) with the values it wrote: each goes
	 * only while it still has that value, and the file goes when setup made it and nothing else is left in it.
	 */
	| { kind: "keys"; path: string; keys: Record<string, unknown>; created: boolean }
	/** A file setup wrote that he may make his own (`WATCHDOG.md`, 0479): it goes only while its text is still this one. */
	| { kind: "text"; path: string; fingerprint: string }
	/** A Windows Credential Manager entry. */
	| { kind: "credential"; target: string };

export interface Marker {
	/** The shipped content's fingerprint when it was applied: a new one makes the migration pending again. */
	fingerprint: string;
	appliedAt: string;
	undo: Undo[];
	/** The omp settings Jocasta shipped when applied: a key he still has at that value may follow a newer one. */
	shipped?: unknown;
}

/** `~/.jocasta/agent/setup`. */
export function markersDir(jocastaConfigDir: string): string {
	return join(jocastaConfigDir, "setup");
}

/** A short fingerprint of shipped content. */
export function fingerprint(text: string): string {
	return createHash("sha256").update(text).digest("hex").slice(0, 16);
}

/** The marker `<name>.json`, or undefined when there is none (or it isn't one). */
export function readMarker(jocastaConfigDir: string, name: string): Marker | undefined {
	let value: unknown;
	try {
		value = JSON.parse(readFileSync(join(markersDir(jocastaConfigDir), `${name}.json`), "utf8"));
	} catch {
		return undefined;
	}
	const marker = asObject(value);
	if (!marker || typeof marker.fingerprint !== "string" || !Array.isArray(marker.undo)) return undefined;
	return marker as unknown as Marker;
}

export function writeMarker(jocastaConfigDir: string, name: string, marker: Marker): void {
	writeAtomically(join(markersDir(jocastaConfigDir), `${name}.json`), `${JSON.stringify(marker, null, 2)}\n`);
}

/** Every undo the markers name, the latest migration first. */
export function recordedUndo(jocastaConfigDir: string): Undo[] {
	let names: string[];
	try {
		names = readdirSync(markersDir(jocastaConfigDir)).filter((n) => n.endsWith(".json"));
	} catch {
		return [];
	}
	return names
		.sort()
		.reverse()
		.flatMap((name) => readMarker(jocastaConfigDir, name.slice(0, -".json".length))?.undo ?? []);
}
