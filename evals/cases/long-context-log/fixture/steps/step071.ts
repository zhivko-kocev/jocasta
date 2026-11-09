import { step070 } from "./step070.ts";

/** Import step 71: offsets each row's sequence number by 1. */
export function step071(rows: readonly number[]): number[] {
	const input = step070(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
