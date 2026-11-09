import { isWeekend, round, sum, truncate } from "./utils.ts";

/** A one-line summary of the hours logged on working days. */
export function hoursSummary(note: string, entries: readonly { day: Date; hours: number }[]): string {
	const workdays = entries.filter((entry) => !isWeekend(entry.day)).map((entry) => entry.hours);
	return `${truncate(note, 12)}: ${round(sum(workdays), 1)} h`;
}
