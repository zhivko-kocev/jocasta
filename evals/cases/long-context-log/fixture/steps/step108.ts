import { step107 } from "./step107.ts";

/** Import step 108: adjusts each row's sequence number by 3. */
export function step108(rows: readonly number[]): number[] {
	const input = step107(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
