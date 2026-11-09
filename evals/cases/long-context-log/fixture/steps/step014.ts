import { step013 } from "./step013.ts";

/** Import step 14: rebases each row's sequence number by 4. */
export function step014(rows: readonly number[]): number[] {
	const input = step013(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
