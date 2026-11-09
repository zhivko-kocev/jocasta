import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Type } from "typebox";
import { configDir } from "./paths.ts";
import { parseYamlFile } from "./yaml.ts";

/** A file of only comments or blanks parses to null; anything else must be a mapping of setting groups. */
const SettingsSchema = Type.Union([Type.Null(), Type.Record(Type.String(), Type.Unknown())]);

/** `jocasta.yaml` in the config dir: Jocasta-only settings (`frontier.*`, `pollers.*`, `graph.*`; 0387). */
export function settingsFile(): string {
	return join(configDir(), "jocasta.yaml");
}

/**
 * The Jocasta-only settings. A missing or empty file reads as `{}`; a file that isn't YAML or isn't a mapping throws,
 * naming the file and line (principle 2).
 */
export function readSettings(): Record<string, unknown> {
	const file = settingsFile();
	let text: string;
	try {
		text = readFileSync(file, "utf8");
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
		throw error;
	}
	return parseYamlFile(SettingsSchema, text, file).value ?? {};
}

/**
 * Settings that are gone (0557), each as the line that says so: `frontier.autostart` (Jocasta no longer starts at logon)
 * and `frontier.dashboard.display` (there is no companion window). A `jocasta.yaml` that still sets them works as
 * before otherwise.
 */
export function goneSettings(settings: Record<string, unknown>): string[] {
	const frontier = settings.frontier;
	if (frontier === null || typeof frontier !== "object" || Array.isArray(frontier)) return [];
	const gone: string[] = [];
	if ("autostart" in frontier) {
		gone.push("settings: frontier.autostart is gone; Jocasta no longer starts at logon: run `jocasta` (0557)");
	}
	const dashboard = "dashboard" in frontier ? frontier.dashboard : undefined;
	if (dashboard !== null && typeof dashboard === "object" && "display" in dashboard) {
		gone.push("settings: frontier.dashboard.display is gone; there is no companion window to place (0557)");
	}
	return gone;
}
