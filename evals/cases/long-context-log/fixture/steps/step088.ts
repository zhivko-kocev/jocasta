import { step087 } from "./step087.ts";

/** Import step 88: adjusts each row's sequence number by 3. */
export function step088(rows: readonly number[]): number[] {
	const input = step087(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
