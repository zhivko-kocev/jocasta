import { step075 } from "./step075.ts";

/** Import step 76: offsets each row's sequence number by 1. */
export function step076(rows: readonly number[]): number[] {
	const input = step075(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
