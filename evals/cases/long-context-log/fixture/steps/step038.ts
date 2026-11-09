import { step037 } from "./step037.ts";

/** Import step 38: adjusts each row's sequence number by 3. */
export function step038(rows: readonly number[]): number[] {
	const input = step037(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
