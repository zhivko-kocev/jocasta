/**
 * The omp settings Jocasta ships (`config/omp-config.yml`) merged into his existing `config.yml` key by key (0466): a
 * key he doesn't have is added; a key still at the value Jocasta shipped last time follows the new one; a key he set
 * to another value is kept and shown. His other keys are untouched. Keys are leaves: a list is one value.
 */
import { asObject } from "../shared/fsx.ts";

/** A key he set to another value than Jocasta ships, kept. */
export interface KeptSetting {
	key: string;
	yours: unknown;
	ships: unknown;
}

export interface SettingsMerge {
	/** His settings with Jocasta's merged in. */
	merged: Record<string, unknown>;
	/** Whether `merged` differs from his. */
	changed: boolean;
	kept: KeptSetting[];
}

/** Merges `shipped` into `his`; `before` is what Jocasta shipped when it last merged (undefined: never). */
export function mergeOmpSettings(
	his: Record<string, unknown>,
	shipped: Record<string, unknown>,
	before: Record<string, unknown> | undefined,
): SettingsMerge {
	const merged = structuredClone(his);
	const kept: KeptSetting[] = [];
	let changed = false;
	const walk = (
		target: Record<string, unknown>,
		ships: Record<string, unknown>,
		was: Record<string, unknown> | undefined,
		prefix: string,
	) => {
		for (const [name, value] of Object.entries(ships)) {
			const key = `${prefix}${name}`;
			const yours = target[name];
			const previous = was?.[name];
			if (yours === undefined) {
				target[name] = structuredClone(value);
				changed = true;
			} else if (asObject(value) && asObject(yours)) {
				walk(yours as Record<string, unknown>, value as Record<string, unknown>, asObject(previous), `${key}.`);
			} else if (Bun.deepEquals(yours, value, true)) {
				// Already Jocasta's value.
			} else if (previous !== undefined && Bun.deepEquals(yours, previous, true)) {
				target[name] = structuredClone(value);
				changed = true;
			} else {
				kept.push({ key, yours, ships: value });
			}
		}
	};
	walk(merged, shipped, before, "");
	return { merged, changed, kept };
}

/** Settings as omp writes `config.yml` itself (`@oh-my-pi/pi-utils` `stringifyYamlConfig`). */
export function ompYaml(value: unknown): string {
	return Bun.YAML.stringify(value, null, 2).replace(/: +$/gm, ":");
}
