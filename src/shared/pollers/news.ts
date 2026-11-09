import { type Headline, type NewsList, newsHeading } from "../../frontier/protocol.ts";
import type { NewsFeed } from "./settings.ts";

/**
 * brutalist.report's headline endpoint (0429). Undocumented: it answers a JSON array of items with `title`, `url`,
 * `source`, `published_at` and much more (article text, keywords, sentiment), of which only title, source, time and
 * link are kept: the dashboard shows headlines, never republishes articles.
 */
export const NEWS_ENDPOINT = "https://brutalist.report/api/v1/headlines";

/** One item as a headline; its `at` is empty when the item has no time. */
function headline(value: unknown): Headline | undefined {
	if (value === null || typeof value !== "object") return undefined;
	const item = value as Record<string, unknown>;
	const title = typeof item.title === "string" ? item.title.replace(/\s+/g, " ").trim() : "";
	const url = typeof item.url === "string" ? item.url.trim() : "";
	if (!title || !/^https?:\/\//i.test(url)) return undefined;
	const at = [item.published_at, item.inserted_at].find(
		(t): t is string => typeof t === "string" && !Number.isNaN(Date.parse(t)),
	);
	return {
		title,
		source: typeof item.source === "string" && item.source.trim() ? item.source.trim() : "unknown",
		url,
		at: at ? new Date(at).toISOString() : "",
	};
}

/** The headlines of one answer, newest first; throws when it isn't a list. */
export function headlinesIn(body: unknown): Headline[] {
	if (!Array.isArray(body)) throw new Error("brutalist.report's answer isn't a headline list");
	return body.flatMap((item) => headline(item) ?? []).sort((a, b) => (b.at > a.at ? 1 : b.at < a.at ? -1 : 0));
}

/**
 * One round (0442): one request per feed, in order (`?source=hn`, `?topic=gaming&limit=10`), each newest first and cut
 * to its `limit` when it has one; without one brutalist.report answers every item it holds. A failure names the feed.
 */
export async function pollNews(feeds: readonly NewsFeed[], fetchUrl: typeof fetch = fetch): Promise<NewsList[]> {
	const lists: NewsList[] = [];
	for (const feed of feeds) {
		const [key, name] = "source" in feed ? ["source", feed.source] : ["topic", feed.topic];
		const heading = newsHeading(name);
		const limit = feed.limit === undefined ? "" : `&limit=${feed.limit}`;
		const url = `${NEWS_ENDPOINT}?${key}=${encodeURIComponent(name)}${limit}`;
		let text: string;
		try {
			const response = await fetchUrl(url, {
				signal: AbortSignal.timeout(30_000),
				headers: { accept: "application/json" },
			});
			if (!response.ok) throw new Error(`HTTP ${response.status}`);
			text = await response.text();
		} catch (error) {
			const reason =
				error instanceof Error && /^HTTP \d+$/.test(error.message)
					? `brutalist.report answered ${error.message}`
					: "brutalist.report couldn't be reached";
			throw new Error(`News (${heading}): ${reason}`);
		}
		let items: Headline[];
		try {
			items = headlinesIn(JSON.parse(text));
		} catch {
			throw new Error(`News (${heading}): brutalist.report's answer isn't a headline list`);
		}
		lists.push({ heading, items: feed.limit === undefined ? items : items.slice(0, feed.limit) });
	}
	return lists;
}
