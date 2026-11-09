import { capitalize, isoDate } from "./utils.ts";

/** The heading of a client's invoice. */
export function invoiceTitle(client: string, issued: Date): string {
	return `${capitalize(client)} invoice, ${isoDate(issued)}`;
}
