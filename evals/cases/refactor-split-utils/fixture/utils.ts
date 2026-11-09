/** "acme" becomes "Acme". */
export function capitalize(text: string): string {
	return text.charAt(0).toUpperCase() + text.slice(1);
}

/** "Client A Report" becomes "client-a-report". */
export function kebabCase(text: string): string {
	return text
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-");
}

/** Cuts `text` to `max` characters, ending with "…" when it was longer. */
export function truncate(text: string, max: number): string {
	return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/** The UTC date of `date` as YYYY-MM-DD. */
export function isoDate(date: Date): string {
	return date.toISOString().slice(0, 10);
}

/** `date` moved by `days` whole days. */
export function addDays(date: Date, days: number): Date {
	return new Date(date.getTime() + days * 86_400_000);
}

/** Whether `date` falls on a Saturday or Sunday, in UTC. */
export function isWeekend(date: Date): boolean {
	const day = date.getUTCDay();
	return day === 0 || day === 6;
}

/** `value` kept within `min` and `max`. */
export function clamp(value: number, min: number, max: number): number {
	return Math.min(max, Math.max(min, value));
}

/** The sum of `values`. */
export function sum(values: readonly number[]): number {
	return values.reduce((total, value) => total + value, 0);
}

/** `value` rounded to `digits` decimal places. */
export function round(value: number, digits: number): number {
	const factor = 10 ** digits;
	return Math.round(value * factor) / factor;
}
