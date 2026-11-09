import { step015 } from "./step015.ts";

/** Import step 16: offsets each row's sequence number by 1. */
export function step016(rows: readonly number[]): number[] {
	const input = step015(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
