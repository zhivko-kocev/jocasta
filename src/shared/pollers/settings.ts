import { checkSettings, type Settings } from "../settings.ts";

/** `pollers` in `jocasta.yaml` (0301, 0387; news 0429, 0442). */
export interface PollerSettings {
	github: { everyMinutes: number };
	azureDevOps: { orgs: string[]; everyMinutes: number };
	calendar: { everyMinutes: number };
	/** brutalist.report feeds, in order (0442). */
	news: { feeds: NewsFeed[]; everyMinutes: number };
}

/** One brutalist.report feed (0442): a source (`hn`) or a topic (`gaming`), with at most `limit` headlines. */
export type NewsFeed = ({ source: string } | { topic: string }) & { limit?: number };

/** 0301's intervals: GitHub 2, Azure DevOps 5, Calendar 10 minutes; 0429's news 15. */
const DEFAULT_MINUTES = { github: 2, azureDevOps: 5, calendar: 10, news: 15 } as const;

/** 0442: every Hacker News and LWN link, the newest 10 gaming headlines. */
const DEFAULT_FEEDS: readonly NewsFeed[] = [{ source: "hn" }, { source: "lwn" }, { topic: "gaming", limit: 10 }];

type Pollers = NonNullable<Settings["pollers"]>;

/** Each poller's settings out of its schema-checked `pollers.<name>`, with 0301's and 0442's defaults. */
const READERS: { [K in keyof PollerSettings]: (pollers: Pollers) => PollerSettings[K] } = {
	github: (pollers) => ({ everyMinutes: pollers.github?.everyMinutes ?? DEFAULT_MINUTES.github }),
	azureDevOps: (pollers) => ({
		orgs: pollers.azureDevOps?.orgs ?? [],
		everyMinutes: pollers.azureDevOps?.everyMinutes ?? DEFAULT_MINUTES.azureDevOps,
	}),
	calendar: (pollers) => ({ everyMinutes: pollers.calendar?.everyMinutes ?? DEFAULT_MINUTES.calendar }),
	news: (pollers) => ({
		feeds: (pollers.news?.feeds ?? DEFAULT_FEEDS).map((feed): NewsFeed => {
			const named: NewsFeed = "source" in feed ? { source: feed.source } : { topic: feed.topic };
			return feed.limit === undefined ? named : { ...named, limit: feed.limit };
		}),
		everyMinutes: pollers.news?.everyMinutes ?? DEFAULT_MINUTES.news,
	}),
};

/**
 * Reads one poller's `pollers.<name>` (0301) as `jocasta.yaml`'s schema checked it (0584): a bad value there (or a
 * bad `pollers`) throws a `SettingsError` naming the file, line and key (principle 2) and turns that poller alone off;
 * the other pollers' keys aren't looked at. Unknown keys are ignored here and said by `settingsProblems`.
 */
export function readPoller<K extends keyof PollerSettings>(
	settings: Record<string, unknown>,
	name: K,
): PollerSettings[K] {
	checkSettings(settings, `pollers.${name}`);
	return READERS[name]((settings as Settings).pollers ?? {});
}
