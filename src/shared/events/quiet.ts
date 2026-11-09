/** Quiet hours as minutes after midnight, local time (0298). */
export interface QuietHours {
	from: number;
	to: number;
}

/**
 * `frontier.quietHours` (0298), like `"22:00-08:00"`. Absent is none; a value that doesn't parse throws, naming the
 * key (principle 2).
 */
export function parseQuietHours(value: unknown): QuietHours | undefined {
	if (value === undefined || value === null || value === "") return undefined;
	const match = typeof value === "string" ? /^(\d{1,2}):(\d{2})-(\d{1,2}):(\d{2})$/.exec(value.trim()) : null;
	const [h1, m1, h2, m2] = (match?.slice(1) ?? []).map(Number) as number[];
	if (!match || [h1, h2].some((h) => (h as number) > 23) || [m1, m2].some((m) => (m as number) > 59)) {
		throw new Error(`settings: frontier.quietHours must be like "22:00-08:00"`);
	}
	return { from: (h1 as number) * 60 + (m1 as number), to: (h2 as number) * 60 + (m2 as number) };
}

/** Whether `at` falls in the quiet hours; a range past midnight wraps. */
export function isQuiet(quiet: QuietHours | undefined, at: Date): boolean {
	if (!quiet || quiet.from === quiet.to) return false;
	const minute = at.getHours() * 60 + at.getMinutes();
	return quiet.from < quiet.to ? minute >= quiet.from && minute < quiet.to : minute >= quiet.from || minute < quiet.to;
}
