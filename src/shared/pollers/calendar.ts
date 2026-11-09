import ical, { type CalendarResponse, type VEvent } from "node-ical";
import { dayStart, localDay } from "../day.ts";
import { webLink } from "../link.ts";

/** One meeting today or tomorrow. */
export interface Meeting {
	uid: string;
	title: string;
	start: Date;
	end: Date;
	allDay: boolean;
	/** Its link (0429): the entry's URL, a link in its location, or a Teams/Meet/Zoom link in its description. */
	url?: string;
}

function text(value: unknown): string {
	if (typeof value === "string") return value;
	if (value && typeof value === "object" && "val" in value) return String((value as { val: unknown }).val);
	return "";
}

/** The first http(s) link in a text, without the brackets invites wrap links in. */
const ANY_LINK = /https?:\/\/[^\s<>"'`]+/i;
/** A Teams, Google Meet or Zoom join link, as invites put them in the description. */
const MEETING_LINK =
	/https:\/\/(?:teams\.microsoft\.com|teams\.live\.com|meet\.google\.com|(?:[\w-]+\.)?zoom\.us)\/[^\s<>"'`]+/i;

/**
 * A meeting's link (0429, DF-1): `URL`, else a link in `LOCATION`, else a meeting link in `DESCRIPTION`; only a
 * well-formed http(s) link up to 300 characters, normalized (SE-4).
 */
export function meetingLink(event: Pick<VEvent, "url" | "location" | "description">): string | undefined {
	const url = webLink(text(event.url).trim());
	if (url) return url;
	const found = ANY_LINK.exec(text(event.location))?.[0] ?? MEETING_LINK.exec(text(event.description))?.[0];
	// A link ending a sentence or inside parentheses: the trailing punctuation isn't part of it.
	return webLink(found?.replace(/[).,;\]]+$/, ""));
}

/**
 * The meetings today and tomorrow in an iCal document (0353): recurrences expanded with their exceptions and
 * overrides, cancelled ones left out, by start; each occurrence ends at its start plus the event's duration. node-ical
 * folds an override into its series, so a top-level entry with a RECURRENCE-ID is one occurrence whose series isn't
 * in the feed (an invitation to one meeting of a series, SHR-4), and counts as a meeting of its own.
 */
export function meetingsIn(parsed: CalendarResponse, now: Date): Meeting[] {
	const today = localDay(now.getTime());
	const from = new Date(dayStart(today));
	const to = new Date(dayStart(today, 2) - 1);
	const meetings: Meeting[] = [];
	for (const component of Object.values(parsed)) {
		if (component?.type !== "VEVENT") continue;
		const event = component as VEvent;
		for (const instance of ical.expandRecurringEvent(event, { from, to, expandOngoing: true })) {
			if (text(instance.event.status).toUpperCase() === "CANCELLED") continue;
			const url = meetingLink(instance.event);
			meetings.push({
				uid: event.uid,
				title: text(instance.summary) || "(no title)",
				start: new Date(instance.start.getTime()),
				end: new Date(instance.end.getTime()),
				allDay: instance.isFullDay,
				...(url ? { url } : {}),
			});
		}
	}
	return meetings.sort((a, b) => a.start.getTime() - b.start.getTime());
}

/**
 * Reads the calendar from the secret URL (0301); a `webcal://` subscription link is fetched over https (D7). The URL
 * is only ever passed to `fetchUrl`; a failure's message never holds it.
 */
export async function pollCalendar(url: string, now: Date, fetchUrl: typeof fetch = fetch): Promise<Meeting[]> {
	let body: string;
	try {
		const response = await fetchUrl(url.replace(/^webcal:\/\//i, "https://"), { signal: AbortSignal.timeout(30_000) });
		if (!response.ok) throw new Error(`HTTP ${response.status}`);
		body = await response.text();
	} catch (error) {
		const reason =
			error instanceof Error && /^HTTP \d+$/.test(error.message) ? error.message : "it couldn't be fetched";
		throw new Error(`calendar: ${reason}`);
	}
	let parsed: CalendarResponse;
	try {
		parsed = ical.sync.parseICS(body);
	} catch {
		throw new Error("calendar: the feed isn't iCal");
	}
	return meetingsIn(parsed, now);
}
