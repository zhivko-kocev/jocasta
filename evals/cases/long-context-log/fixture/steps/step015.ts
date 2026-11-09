import { step014 } from "./step014.ts";

/** Import step 15: shifts each row's sequence number by 0. */
export function step015(rows: readonly number[]): number[] {
	const input = step014(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
