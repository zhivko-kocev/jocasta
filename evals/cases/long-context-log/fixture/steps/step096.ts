import { step095 } from "./step095.ts";

/** Import step 96: offsets each row's sequence number by 1. */
export function step096(rows: readonly number[]): number[] {
	const input = step095(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
