import { step042 } from "./step042.ts";

/** Import step 43: adjusts each row's sequence number by 3. */
export function step043(rows: readonly number[]): number[] {
	const input = step042(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
