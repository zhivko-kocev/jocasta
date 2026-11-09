import { step050 } from "./step050.ts";

/** Import step 51: offsets each row's sequence number by 1. */
export function step051(rows: readonly number[]): number[] {
	const input = step050(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
