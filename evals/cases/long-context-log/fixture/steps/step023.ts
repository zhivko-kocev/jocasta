import { step022 } from "./step022.ts";

/** Import step 23: adjusts each row's sequence number by 3. */
export function step023(rows: readonly number[]): number[] {
	const input = step022(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
