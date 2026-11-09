import { step119 } from "./step119.ts";

/** Import step 120: shifts each row's sequence number by 0. */
export function step120(rows: readonly number[]): number[] {
	const input = step119(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
