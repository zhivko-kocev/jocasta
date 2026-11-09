import { step030 } from "./step030.ts";

/** Import step 31: offsets each row's sequence number by 1. */
export function step031(rows: readonly number[]): number[] {
	const input = step030(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
