import { readFileSync } from "node:fs";
import { join } from "node:path";
import { type Static, Type } from "typebox";
import { Value } from "typebox/value";
import { configDir } from "./paths.ts";
import { parseYamlFile, YamlFileError, type YamlPath } from "./yaml.ts";

/** `jocasta.yaml` in the config dir: Jocasta-only settings (`service.*`, `dashboard.*`, `pollers.*`, `graph.*`; 0387). */
export function settingsFile(): string {
	return join(configDir(), "jocasta.yaml");
}

/** How a problem names the file: by name, as in `jocasta.yaml:5: …` (0584). */
const FILE_NAME = "jocasta.yaml";

/** A file of only comments or blanks parses to null; anything else must be a mapping of setting groups. */
const RootSchema = Type.Union([Type.Record(Type.String(), Type.Unknown()), Type.Null()]);

const strict = { additionalProperties: false } as const;
const Minutes = Type.Optional(Type.Number({ minimum: 1 }));
const FeedName = Type.String({ pattern: "^[a-z]+$" });
const FeedLimit = Type.Optional(Type.Integer({ minimum: 1, maximum: 200 }));
const GraphModelName = Type.Optional(Type.String({ pattern: "^[^/\\s]+/\\S+$" }));

/** `service.quietHours` (0298): `HH:MM-HH:MM`, hours 0–23; empty or null is none. */
const QuietHoursSchema = Type.Union([
	Type.Null(),
	Type.Literal(""),
	Type.String({ pattern: "^\\s*([01]?\\d|2[0-3]):[0-5]\\d-([01]?\\d|2[0-3]):[0-5]\\d\\s*$" }),
]);

/**
 * Every key `jocasta.yaml` knows (0298, 0301, 0358, 0387, 0390, 0429, 0442, 0557), validated once per read (0584).
 * `service` was `frontier` before 0584: that group is still read, and `service` wins where both set a key
 * (`serviceSetting`). `frontier.autostart` and `frontier.dashboard` are known keys that are gone (`goneSettings`);
 * `pollers.news.topics` and `pollers.news.limit` are gone too and refused (0442).
 */
export const SettingsSchema = Type.Object(
	{
		service: Type.Optional(
			Type.Object(
				{
					port: Type.Optional(Type.Integer({ minimum: 1, maximum: 65_535 })),
					quietHours: Type.Optional(QuietHoursSchema),
				},
				strict,
			),
		),
		frontier: Type.Optional(
			Type.Object(
				{
					port: Type.Optional(Type.Integer({ minimum: 1, maximum: 65_535 })),
					quietHours: Type.Optional(QuietHoursSchema),
					autostart: Type.Optional(Type.Unknown()),
					dashboard: Type.Optional(Type.Unknown()),
				},
				strict,
			),
		),
		dashboard: Type.Optional(
			Type.Object({ weather: Type.Optional(Type.Union([Type.Null(), Type.String({ pattern: "\\S" })])) }, strict),
		),
		pollers: Type.Optional(
			Type.Object(
				{
					github: Type.Optional(Type.Object({ everyMinutes: Minutes }, strict)),
					azureDevOps: Type.Optional(
						Type.Object(
							{
								orgs: Type.Optional(Type.Array(Type.String({ pattern: '^https://[^\\s"%]+$' }))),
								everyMinutes: Minutes,
							},
							strict,
						),
					),
					calendar: Type.Optional(Type.Object({ everyMinutes: Minutes }, strict)),
					news: Type.Optional(
						Type.Object(
							{
								feeds: Type.Optional(
									Type.Array(
										Type.Union([
											Type.Object({ source: FeedName, limit: FeedLimit }, strict),
											Type.Object({ topic: FeedName, limit: FeedLimit }, strict),
										]),
									),
								),
								everyMinutes: Minutes,
								topics: Type.Optional(Type.Never()),
								limit: Type.Optional(Type.Never()),
							},
							strict,
						),
					),
				},
				strict,
			),
		),
		graph: Type.Optional(Type.Object({ main: GraphModelName, small: GraphModelName }, strict)),
	},
	strict,
);

/** `jocasta.yaml` once it passed `SettingsSchema`. */
export type Settings = Static<typeof SettingsSchema>;

/** What a bad value at a key means: the reason, and what Jocasta does instead (0584). `map` marks a mapping key. */
interface KeyRule {
	reason: string;
	instead: string;
	map?: boolean;
}

/** What a bad value under `pollers` turns off: its own poller (0584); a bad `pollers` itself turns them all off. */
const POLLER_OFF: Record<string, string> = {
	github: "the GitHub poller is off",
	azureDevOps: "the Azure DevOps poller is off",
	calendar: "the calendar poller is off",
	news: "the news poller is off",
};
const WEATHER_OFF = "the weather is off";
const minutesRule = (poller: string): KeyRule => ({
	reason: `pollers.${poller}.everyMinutes must be a number of minutes, 1 or more`,
	instead: POLLER_OFF[poller] as string,
});
const pollerMap = (poller: string): KeyRule => ({
	reason: `pollers.${poller} must be a mapping`,
	instead: POLLER_OFF[poller] as string,
	map: true,
});
const NEWS_GONE: KeyRule = {
	reason: "pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)",
	instead: POLLER_OFF.news as string,
};
const graphModelRule = (name: string): KeyRule => ({
	reason: `graph.${name} must be an omp model as provider/id`,
	instead: "Graphiti's model calls are off",
});

/** Each key of `SettingsSchema` by its dotted path; a problem belongs to the deepest key on its path. */
const RULES: Record<string, KeyRule> = {
	service: { reason: "service must be a mapping", instead: "using port 47400 and no quiet hours", map: true },
	"service.port": { reason: "service.port must be a port number from 1 to 65535", instead: "using 47400" },
	"service.quietHours": { reason: 'service.quietHours must be like "22:00-08:00"', instead: "quiet hours are off" },
	frontier: { reason: "frontier must be a mapping", instead: "using port 47400 and no quiet hours", map: true },
	"frontier.port": { reason: "frontier.port must be a port number from 1 to 65535", instead: "using 47400" },
	"frontier.quietHours": { reason: 'frontier.quietHours must be like "22:00-08:00"', instead: "quiet hours are off" },
	dashboard: { reason: "dashboard must be a mapping", instead: WEATHER_OFF, map: true },
	"dashboard.weather": {
		reason: 'dashboard.weather must be a place, e.g. "Example City, Example Country" or "12.34, 56.78"',
		instead: WEATHER_OFF,
	},
	pollers: { reason: "pollers must be a mapping", instead: "pollers are off", map: true },
	"pollers.github": pollerMap("github"),
	"pollers.github.everyMinutes": minutesRule("github"),
	"pollers.azureDevOps": pollerMap("azureDevOps"),
	"pollers.azureDevOps.orgs": {
		reason: "pollers.azureDevOps.orgs must be a list of https:// org URLs",
		instead: POLLER_OFF.azureDevOps as string,
	},
	"pollers.azureDevOps.everyMinutes": minutesRule("azureDevOps"),
	"pollers.calendar": pollerMap("calendar"),
	"pollers.calendar.everyMinutes": minutesRule("calendar"),
	"pollers.news": pollerMap("news"),
	"pollers.news.feeds": {
		reason:
			"pollers.news.feeds must be a list of {source: <name>} or {topic: <name>} (a-z), each with an optional limit from 1 to 200",
		instead: POLLER_OFF.news as string,
	},
	"pollers.news.everyMinutes": minutesRule("news"),
	"pollers.news.topics": NEWS_GONE,
	"pollers.news.limit": NEWS_GONE,
	graph: { reason: "graph must be a mapping", instead: "using omp's default and smol roles", map: true },
	"graph.main": graphModelRule("main"),
	"graph.small": graphModelRule("small"),
};

/** One thing wrong in `jocasta.yaml`: an unknown key (ignored) or a bad value at `key` (its feature falls back). */
interface Problem {
	/** The dotted key the problem belongs to: the bad value's key, or the mapping holding the unknown key. */
	key: string;
	/** The 1-based line, when the settings came from the file. */
	line?: number;
	/** `unknown key gihub in pollers`, or the bad value's reason. */
	reason: string;
	/** `ignored`, or what Jocasta does instead. */
	instead: string;
	unknown: boolean;
	/** Whether `settingsProblems` returned it, so the service said it at start (C7). */
	said?: boolean;
}

/** Where each settings object read from the file finds its lines. */
const linesOf = new WeakMap<object, (path: YamlPath) => number>();
/** Each settings object's problems, worked out once. */
const problemsCache = new WeakMap<object, Problem[]>();

/** The problems in `settings`, by line; unknown keys and bad values both (0584). */
function problemsOf(settings: Record<string, unknown>): Problem[] {
	const cached = problemsCache.get(settings);
	if (cached) return cached;
	const lineOf = linesOf.get(settings);
	const problems = new Map<string, Problem>();
	for (const error of Value.Errors(SettingsSchema, settings)) {
		// An unknown key yields "schema is false" at the key and `additionalProperties` at its mapping; the first is used.
		if (error.keyword === "additionalProperties") continue;
		const path = error.instancePath
			.split("/")
			.slice(1)
			.map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"));
		let depth = 0;
		for (let i = 1; i <= path.length; i++) if (RULES[path.slice(0, i).join(".")]) depth = i;
		const key = path.slice(0, depth).join(".");
		const rule = RULES[key];
		const unknownKey = error.keyword === "boolean" && path.length === depth + 1 && (depth === 0 || rule?.map);
		if (unknownKey) {
			const name = path[depth] as string;
			const id = `${key}\0${name}`;
			if (problems.has(id)) continue;
			problems.set(id, {
				key,
				line: lineOf?.(path),
				reason: depth === 0 ? `unknown key ${name}` : `unknown key ${name} in ${key}`,
				instead: "ignored",
				unknown: true,
			});
		} else if (rule && !problems.has(key)) {
			problems.set(key, {
				key,
				line: lineOf?.(path.slice(0, depth)),
				reason: rule.reason,
				instead: rule.instead,
				unknown: false,
			});
		}
	}
	const sorted = [...problems.values()].sort((a, b) => (a.line ?? 0) - (b.line ?? 0));
	problemsCache.set(settings, sorted);
	return sorted;
}

/**
 * A bad value a reader met (`checkSettings`): `jocasta.yaml:<line>: <reason>`. `said` is whether the service already
 * said it at start (0584, C7), so a reader that reports failures says it only when the file changed since.
 */
export class SettingsError extends Error {
	readonly said: boolean;
	constructor(message: string, said: boolean) {
		super(message);
		this.name = "SettingsError";
		this.said = said;
	}
}

/**
 * For the reader of one feature (`pollers.github`, `dashboard.weather`, `service.quietHours`…): throws the first bad
 * value at, under or above `key` as a `SettingsError` `jocasta.yaml:<line>: <reason>` (`settings: <reason>` for
 * settings not read from the file), so that feature alone is off (0584, principle 4). Unknown keys never throw;
 * `settingsProblems` says them.
 */
export function checkSettings(settings: Record<string, unknown>, key: string): void {
	const problem = problemsOf(settings).find(
		(p) => !p.unknown && (p.key === key || p.key.startsWith(`${key}.`) || key.startsWith(`${p.key}.`)),
	);
	if (!problem) return;
	throw new SettingsError(
		`${problem.line === undefined ? "settings" : `${FILE_NAME}:${problem.line}`}: ${problem.reason}`,
		problem.said === true,
	);
}

/**
 * A key of the `service` group (`port`, `quietHours`), read where his file sets it (0584): `service.<name>`, else the
 * old `frontier.<name>`; `service` wins when both set it. Checked like `checkSettings`, so a bad value throws naming
 * the key as his file spells it; undefined when neither sets it.
 */
export function serviceSetting<K extends "port" | "quietHours">(
	settings: Record<string, unknown>,
	name: K,
): NonNullable<Settings["service"]>[K] {
	// A group speaks for the key when it sets it, or is there but no mapping (its own problem is then the one thrown).
	const sets = (group: unknown) =>
		group !== undefined && (group === null || typeof group !== "object" || Array.isArray(group) || name in group);
	const group = sets(settings.service) || !sets(settings.frontier) ? "service" : "frontier";
	checkSettings(settings, `${group}.${name}`);
	return (settings as Settings)[group]?.[name];
}

/** The last file text read and what it parsed to, so an unchanged file is parsed and validated once. */
let last: { text: string; value: Record<string, unknown> } | undefined;

/**
 * The Jocasta-only settings. A missing or empty file reads as `{}`; a file that isn't YAML or isn't a mapping throws,
 * naming the file and line (principle 2). Unknown keys and bad values stay in the object: each reader checks its own
 * group (`checkSettings`) and `settingsProblems` says them all.
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
	if (last?.text === text) return last.value;
	const parsed = parseYamlFile(RootSchema, text, file);
	const value = parsed.value ?? {};
	linesOf.set(value, parsed.lineOf);
	last = { text, value };
	return value;
}

/**
 * Everything wrong in `jocasta.yaml`, each as one line for a notification at start (0584, contract C7):
 * `jocasta.yaml:<line>: unknown key <k> in <group>; ignored`, or `jocasta.yaml:<line>: <reason>; <what Jocasta does
 * instead>`. A file that isn't YAML is one problem; this never throws.
 */
export function settingsProblems(): string[] {
	let settings: Record<string, unknown>;
	try {
		settings = readSettings();
	} catch (error) {
		if (error instanceof YamlFileError) return [`${FILE_NAME}:${error.line}: ${error.problem}; its settings are off`];
		return [`${FILE_NAME}: ${(error as Error).message}; its settings are off`];
	}
	const problems = problemsOf(settings);
	for (const problem of problems) problem.said = true;
	return problems.map((p) => `${FILE_NAME}:${p.line ?? 1}: ${p.reason}; ${p.instead}`);
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
