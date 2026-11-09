/** Local calendar days as `YYYY-MM-DD`: the one place they are named and turned back into times. */

/** The local calendar day of a time, as `YYYY-MM-DD`. */
export function localDay(ms: number): string {
	const d = new Date(ms);
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** The local midnight starting `day` (`YYYY-MM-DD`) plus `plusDays` calendar days, epoch ms (right across DST). */
export function dayStart(day: string, plusDays = 0): number {
	const [year = 0, month = 1, date = 1] = day.split("-").map(Number);
	return new Date(year, month - 1, date + plusDays).getTime();
}

/** The local midnight starting the day of `ms`, epoch ms. */
export function midnightOf(ms: number): number {
	const d = new Date(ms);
	return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** A time's local clock as `HH:MM`. */
export function hhmm(at: Date | number): string {
	const d = typeof at === "number" ? new Date(at) : at;
	return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** A time's local weekday, short: `Mon`. */
export function weekday(at: Date): string {
	return at.toLocaleDateString("en-GB", { weekday: "short" });
}

/** A day's label: `Mon 3 Nov`, or with `year` `Mon 3 Nov 2026`. `day` is a time on it or its `YYYY-MM-DD`. */
export function dayLabel(day: Date | string, { year = false }: { year?: boolean } = {}): string {
	const date = typeof day === "string" ? new Date(dayStart(day)) : day;
	const month = date.toLocaleDateString("en-GB", { month: "short" });
	return `${weekday(date)} ${date.getDate()} ${month}${year ? ` ${date.getFullYear()}` : ""}`;
}
