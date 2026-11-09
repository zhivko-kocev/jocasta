import { expect, test } from "bun:test";
import { tmpdir } from "node:os";
import ical from "node-ical";
import { meetingLink, meetingsIn } from "../../src/shared/pollers/calendar.ts";
import { cpuTimes, sampleMachine } from "../../src/shared/pollers/machine.ts";
import { headlinesIn, NEWS_ENDPOINT, pollNews } from "../../src/shared/pollers/news.ts";
import {
	FORECAST_ENDPOINT,
	findPlace,
	GEOCODING_ENDPOINT,
	pollWeather,
	readWeatherPlace,
} from "../../src/shared/pollers/weather.ts";

/** A fetch that answers from a table of URL prefixes and records what was asked; nothing reaches the network. */
function fakeFetch(answers: [prefix: string, answer: () => Response][]): { fetchUrl: typeof fetch; asked: string[] } {
	const asked: string[] = [];
	const fetchUrl = (async (input: string | URL | Request) => {
		const url = String(input);
		asked.push(url);
		const found = answers.find(([prefix]) => url.startsWith(prefix));
		if (!found) throw new TypeError("fetch failed");
		return found[1]();
	}) as typeof fetch;
	return { fetchUrl, asked };
}

/** Two items as brutalist.report's `/api/v1/headlines` answers them (fields trimmed from a live answer, 2026-10-14). */
const HEADLINES = [
	{
		uuid: "f8d2098b",
		title: "Amazon Alexa Tablet 12 Pro review:\n gorgeous screen",
		url: "https://www.techmeme.com/261014/p20#a261014p20",
		comment_url: null,
		source: "techmeme",
		topic: "tech",
		age: "[2m]",
		published_at: "2026-10-14T13:20:02Z",
		inserted_at: "2026-10-14T13:21:29.952846Z",
		content: "the whole article text, never kept",
		keywords: ["Amazon"],
		sentiment_compound: 0.57,
	},
	{
		uuid: "a1",
		title: "Kernel 7.1 released",
		url: "https://lwn.net/Articles/1/",
		source: "lwn",
		published_at: "2026-10-14T13:40:00Z",
	},
	{ uuid: "bad", title: "no link", url: "javascript:alert(1)", source: "x", published_at: "2026-10-14T13:00:00Z" },
];

test("news (0429, 0442): titles, source, time and link only, newest first; one request per feed; failures name the feed, never the payload", async () => {
	expect(headlinesIn(HEADLINES)).toEqual([
		{ title: "Kernel 7.1 released", source: "lwn", url: "https://lwn.net/Articles/1/", at: "2026-10-14T13:40:00.000Z" },
		{
			title: "Amazon Alexa Tablet 12 Pro review: gorgeous screen",
			source: "techmeme",
			url: "https://www.techmeme.com/261014/p20#a261014p20",
			at: "2026-10-14T13:20:02.000Z",
		},
	]);
	const { fetchUrl, asked } = fakeFetch([
		[`${NEWS_ENDPOINT}?source=hn`, () => Response.json(HEADLINES)],
		[`${NEWS_ENDPOINT}?source=lwn`, () => Response.json([])],
		[`${NEWS_ENDPOINT}?topic=gaming`, () => Response.json(HEADLINES)],
		[`${NEWS_ENDPOINT}?topic=acme`, () => Response.json([])],
	]);
	const lists = await pollNews(
		[{ source: "hn" }, { source: "lwn" }, { topic: "gaming", limit: 1 }, { topic: "acme" }],
		fetchUrl,
	);
	// No `limit` for a feed without one: brutalist.report then answers every item it holds.
	expect(asked).toEqual([
		`${NEWS_ENDPOINT}?source=hn`,
		`${NEWS_ENDPOINT}?source=lwn`,
		`${NEWS_ENDPOINT}?topic=gaming&limit=1`,
		`${NEWS_ENDPOINT}?topic=acme`,
	]);
	expect(lists.map((l) => [l.heading, l.items.map((i) => i.title)])).toEqual([
		["Hacker News", ["Kernel 7.1 released", "Amazon Alexa Tablet 12 Pro review: gorgeous screen"]],
		["LWN", []],
		["Gaming", ["Kernel 7.1 released"]],
		["Acme", []],
	]);

	const down = fakeFetch([[NEWS_ENDPOINT, () => new Response("busy", { status: 503 })]]);
	await expect(pollNews([{ source: "hn" }], down.fetchUrl)).rejects.toThrow(
		"News (Hacker News): brutalist.report answered HTTP 503",
	);
	await expect(pollNews([{ source: "lwn" }], fakeFetch([]).fetchUrl)).rejects.toThrow(
		"News (LWN): brutalist.report couldn't be reached",
	);
	const moved = fakeFetch([[NEWS_ENDPOINT, () => Response.json({ message: "this API has been moved" })]]);
	await expect(pollNews([{ topic: "gaming", limit: 10 }], moved.fetchUrl)).rejects.toThrow(
		"News (Gaming): brutalist.report's answer isn't a headline list",
	);
});

/** Open-Meteo's answers for a sample city (shaped like live answers, 2026-10-14; placeholder names). */
const GEOCODING = {
	results: [
		{
			id: 1,
			name: "Example City",
			latitude: 12.34567,
			longitude: 56.78901,
			country: "Example Country",
			admin1: "Example Region",
		},
		{ id: 2, name: "Example City", latitude: -21.3, longitude: 47.1, country: "Other Country" },
	],
};
const FORECAST = {
	latitude: 12.35,
	longitude: 56.79,
	utc_offset_seconds: 10800,
	timezone: "Etc/GMT-3",
	current: { time: "2026-10-14T16:15", interval: 900, temperature_2m: 11.9, weather_code: 2 },
	daily: { time: ["2026-10-14"], temperature_2m_max: [11.9], temperature_2m_min: [6.7] },
};

test("weather (0429): dashboard.weather resolved through Open-Meteo's geocoding, then the forecast; hidden until set", async () => {
	expect(readWeatherPlace({})).toBeUndefined();
	expect(readWeatherPlace({ dashboard: { weather: " Example City, Example Country " } })).toBe(
		"Example City, Example Country",
	);
	expect(() => readWeatherPlace({ dashboard: { weather: 42 } })).toThrow("dashboard.weather");

	const { fetchUrl, asked } = fakeFetch([
		[GEOCODING_ENDPOINT, () => Response.json(GEOCODING)],
		[FORECAST_ENDPOINT, () => Response.json(FORECAST)],
	]);
	expect(await findPlace("Example City, Other Country", fetchUrl)).toEqual({
		name: "Example City, Other Country",
		latitude: -21.3,
		longitude: 47.1,
	});
	const city = await findPlace("Example City", fetchUrl);
	expect(city).toEqual({ name: "Example City, Example Country", latitude: 12.34567, longitude: 56.78901 });
	expect(asked.at(-1)).toBe(`${GEOCODING_ENDPOINT}?name=Example%20City&count=10&language=en&format=json`);
	// Coordinates need no lookup.
	expect(await findPlace("12.34, 56.78", fakeFetch([]).fetchUrl)).toEqual({
		name: "12.34, 56.78",
		latitude: 12.34,
		longitude: 56.78,
	});
	await expect(findPlace("Example City, Nowhere", fetchUrl)).rejects.toThrow(
		'Open-Meteo knows no place "Example City, Nowhere"',
	);

	expect(await pollWeather(city, fetchUrl)).toEqual({
		place: "Example City, Example Country",
		tempC: 11.9,
		code: 2,
		summary: "Partly cloudy",
		highC: 11.9,
		lowC: 6.7,
		at: "2026-10-14T13:15:00.000Z",
	});
	expect(asked.at(-1)).toContain("latitude=12.34567&longitude=56.78901&current=temperature_2m%2Cweather_code");
	const odd = fakeFetch([[FORECAST_ENDPOINT, () => Response.json({ error: true, reason: "x" })]]);
	await expect(pollWeather(city, odd.fetchUrl)).rejects.toThrow(
		"Weather: Open-Meteo's forecast answer has an unknown shape",
	);
	const down = fakeFetch([[FORECAST_ENDPOINT, () => new Response("", { status: 500 })]]);
	await expect(pollWeather(city, down.fetchUrl)).rejects.toThrow("Weather: Open-Meteo's forecast answered HTTP 500");
});

test("calendar meeting links (0429, DF-1): URL, else a link in LOCATION, else a Teams/Meet/Zoom link in DESCRIPTION", () => {
	expect(meetingLink({ url: "https://example.test/m/1" })).toBe("https://example.test/m/1");
	expect(meetingLink({ location: { val: "Room 4 (https://zoom.example.test/j/9)", params: {} } })).toBe(
		"https://zoom.example.test/j/9",
	);
	expect(
		meetingLink({
			location: "Microsoft Teams Meeting",
			description:
				"Join: <https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0?context=%7b%7d>\nhttps://example.test/other",
		}),
	).toBe("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0?context=%7b%7d");
	expect(meetingLink({ description: "Join with Google Meet: https://meet.google.com/abc-defg-hij." })).toBe(
		"https://meet.google.com/abc-defg-hij",
	);
	expect(meetingLink({ description: "Zoom: https://acme.zoom.us/j/123?pwd=x" })).toBe(
		"https://acme.zoom.us/j/123?pwd=x",
	);
	expect(meetingLink({ description: "see https://example.test/agenda", location: "Room 4" })).toBeUndefined();

	const feed = [
		"BEGIN:VCALENDAR",
		"VERSION:2.0",
		"BEGIN:VEVENT",
		"UID:review@test",
		"SUMMARY:Design review",
		"DTSTART:20261008T120000Z",
		"DTEND:20261008T130000Z",
		"DESCRIPTION:Join with Google Meet: https://meet.google.com/abc-defg-hij",
		"END:VEVENT",
		"END:VCALENDAR",
		"",
	].join("\r\n");
	const [meeting] = meetingsIn(ical.sync.parseICS(feed), new Date("2026-10-08T06:00:00Z"));
	expect(meeting?.url).toBe("https://meet.google.com/abc-defg-hij");
});

test("machine (0429): CPU since the last reading, RAM and disk in whole percents", () => {
	const sample = sampleMachine({ busy: 100, total: 400 }, { busy: 160, total: 600 }, tmpdir());
	expect(sample.cpu).toBe(0.3);
	expect(sample.ram.total).toBeGreaterThan(0);
	expect(sample.ram.used).toBeGreaterThan(0);
	expect(sample.ram.used).toBeLessThanOrEqual(sample.ram.total);
	expect(sample.disk.total).toBeGreaterThan(0);
	expect(sample.disk.free).toBeLessThanOrEqual(sample.disk.total);
	// Rounded to a whole percent of the total.
	const percent = (sample.ram.used / sample.ram.total) * 100;
	expect(Math.abs(percent - Math.round(percent))).toBeLessThan(1e-6);
	const now = cpuTimes();
	expect(sampleMachine(now, now, tmpdir()).cpu).toBe(0);
});
