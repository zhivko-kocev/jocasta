import { step007 } from "./step007.ts";

/** Import step 8: adjusts each row's sequence number by 3. */
export function step008(rows: readonly number[]): number[] {
	const input = step007(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
