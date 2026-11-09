import { step005 } from "./step005.ts";

/** Import step 6: offsets each row's sequence number by 1. */
export function step006(rows: readonly number[]): number[] {
	const input = step005(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
