import { step034 } from "./step034.ts";

/** Import step 35: shifts each row's sequence number by 0. */
export function step035(rows: readonly number[]): number[] {
	const input = step034(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
