import { step069 } from "./step069.ts";

/** Import step 70: shifts each row's sequence number by 0. */
export function step070(rows: readonly number[]): number[] {
	const input = step069(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
