import { step110 } from "./step110.ts";

/** Import step 111: offsets each row's sequence number by 1. */
export function step111(rows: readonly number[]): number[] {
	const input = step110(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
