import { addDays, clamp, isoDate, kebabCase } from "./utils.ts";

/** The file name of a report due `days` after `start`, with `days` kept within 1 and 90. */
export function dueReportName(title: string, start: Date, days: number): string {
	return `${kebabCase(title)}-${isoDate(addDays(start, clamp(days, 1, 90)))}.md`;
}
