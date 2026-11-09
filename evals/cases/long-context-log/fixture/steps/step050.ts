import { step049 } from "./step049.ts";

/** Import step 50: shifts each row's sequence number by 0. */
export function step050(rows: readonly number[]): number[] {
	const input = step049(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
