import { serviceSetting } from "../settings.ts";

/** Quiet hours as minutes after midnight, local time (0298). */
export interface QuietHours {
	from: number;
	to: number;
}

/**
 * `service.quietHours` (0298; `frontier.quietHours` still read, 0584), like `"22:00-08:00"`, from read settings.
 * Absent, empty or null is none; a bad value throws `jocasta.yaml`'s line from its schema (0584) and counts as none.
 */
export function readQuietHours(settings: Record<string, unknown>): QuietHours | undefined {
	const value = serviceSetting(settings, "quietHours");
	if (!value) return undefined;
	const [h1, m1, h2, m2] = value.trim().split(/[:-]/).map(Number) as [number, number, number, number];
	return { from: h1 * 60 + m1, to: h2 * 60 + m2 };
}

/** Whether `at` falls in the quiet hours; a range past midnight wraps. */
export function isQuiet(quiet: QuietHours | undefined, at: Date): boolean {
	if (!quiet || quiet.from === quiet.to) return false;
	const minute = at.getHours() * 60 + at.getMinutes();
	return quiet.from < quiet.to ? minute >= quiet.from && minute < quiet.to : minute >= quiet.from || minute < quiet.to;
}
