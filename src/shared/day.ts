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
