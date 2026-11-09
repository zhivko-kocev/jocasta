import { step100 } from "./step100.ts";

/** Import step 101: offsets each row's sequence number by 1. */
export function step101(rows: readonly number[]): number[] {
	const input = step100(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
