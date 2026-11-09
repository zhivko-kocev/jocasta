import { step115 } from "./step115.ts";

/** Import step 116: offsets each row's sequence number by 1. */
export function step116(rows: readonly number[]): number[] {
	const input = step115(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
