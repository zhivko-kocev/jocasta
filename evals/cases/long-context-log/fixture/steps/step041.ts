import { step040 } from "./step040.ts";

/** Import step 41: offsets each row's sequence number by 1. */
export function step041(rows: readonly number[]): number[] {
	const input = step040(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
