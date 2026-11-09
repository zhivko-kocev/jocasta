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
export const DEFAULT_MINUTES = { github: 2, azureDevOps: 5, calendar: 10, news: 15 } as const;

/** 0442: every Hacker News and LWN link, the newest 10 gaming headlines. */
const DEFAULT_FEEDS: readonly NewsFeed[] = [{ source: "hn" }, { source: "lwn" }, { topic: "gaming", limit: 10 }];

const FEEDS_SHAPE =
	"settings: pollers.news.feeds must be a list of {source: <name>} or {topic: <name>} (a-z), each with an optional limit from 1 to 200";

function feeds(value: unknown): NewsFeed[] {
	if (value === undefined) return DEFAULT_FEEDS.map((feed) => ({ ...feed }));
	if (!Array.isArray(value)) throw new Error(FEEDS_SHAPE);
	return value.map((entry): NewsFeed => {
		if (entry === null || typeof entry !== "object" || Array.isArray(entry)) throw new Error(FEEDS_SHAPE);
		const { source, topic, limit, ...rest } = entry as Record<string, unknown>;
		const name = source ?? topic;
		if (
			Object.keys(rest).length > 0 ||
			(source !== undefined) === (topic !== undefined) ||
			typeof name !== "string" ||
			!/^[a-z]+$/.test(name) ||
			(limit !== undefined && (typeof limit !== "number" || !Number.isInteger(limit) || limit < 1 || limit > 200))
		) {
			throw new Error(FEEDS_SHAPE);
		}
		const feed: NewsFeed = source !== undefined ? { source: name } : { topic: name };
		return limit === undefined ? feed : { ...feed, limit: limit as number };
	});
}

function minutes(value: unknown, key: string, fallback: number): number {
	if (value === undefined) return fallback;
	if (typeof value !== "number" || !Number.isFinite(value) || value < 1) {
		throw new Error(`settings: pollers.${key}.everyMinutes must be a number of minutes, 1 or more`);
	}
	return value;
}

function section(pollers: Record<string, unknown>, key: string): Record<string, unknown> {
	const value = pollers[key];
	if (value === undefined) return {};
	if (typeof value !== "object" || value === null || Array.isArray(value)) {
		throw new Error(`settings: pollers.${key} must be an object`);
	}
	return value as Record<string, unknown>;
}

/** Reads `pollers` (0301); a malformed value throws, naming the key (principle 2). */
export function readPollerSettings(settings: Record<string, unknown>): PollerSettings {
	const raw = settings.pollers;
	if (raw !== undefined && (typeof raw !== "object" || raw === null || Array.isArray(raw))) {
		throw new Error("settings: pollers must be an object");
	}
	const pollers = (raw ?? {}) as Record<string, unknown>;
	const github = section(pollers, "github");
	const azure = section(pollers, "azureDevOps");
	const calendar = section(pollers, "calendar");
	const news = section(pollers, "news");
	const orgs = azure.orgs ?? [];
	if (!Array.isArray(orgs) || !orgs.every((o) => typeof o === "string" && /^https:\/\/[^\s"%]+$/.test(o))) {
		throw new Error("settings: pollers.azureDevOps.orgs must be a list of https:// org URLs");
	}
	if (news.topics !== undefined || news.limit !== undefined) {
		throw new Error("settings: pollers.news.topics and pollers.news.limit are gone; use pollers.news.feeds (0442)");
	}
	return {
		github: { everyMinutes: minutes(github.everyMinutes, "github", DEFAULT_MINUTES.github) },
		azureDevOps: { orgs, everyMinutes: minutes(azure.everyMinutes, "azureDevOps", DEFAULT_MINUTES.azureDevOps) },
		calendar: { everyMinutes: minutes(calendar.everyMinutes, "calendar", DEFAULT_MINUTES.calendar) },
		news: { feeds: feeds(news.feeds), everyMinutes: minutes(news.everyMinutes, "news", DEFAULT_MINUTES.news) },
	};
}
