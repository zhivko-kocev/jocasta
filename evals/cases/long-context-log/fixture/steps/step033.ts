import { step032 } from "./step032.ts";

/** Import step 33: adjusts each row's sequence number by 3. */
export function step033(rows: readonly number[]): number[] {
	const input = step032(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
