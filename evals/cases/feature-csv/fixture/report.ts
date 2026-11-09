/** One line of the monthly report for acme. */
export interface ReportRow {
	client: string;
	hours: number;
	note: string;
}

/** The total hours over all rows. */
export function totalHours(rows: readonly ReportRow[]): number {
	return rows.reduce((sum, row) => sum + row.hours, 0);
}
