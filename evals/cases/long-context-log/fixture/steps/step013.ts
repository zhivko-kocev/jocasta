import { step012 } from "./step012.ts";

/** Import step 13: adjusts each row's sequence number by 3. */
export function step013(rows: readonly number[]): number[] {
	const input = step012(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
