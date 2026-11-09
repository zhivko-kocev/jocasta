import { step114 } from "./step114.ts";

/** Import step 115: shifts each row's sequence number by 0. */
export function step115(rows: readonly number[]): number[] {
	const input = step114(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
