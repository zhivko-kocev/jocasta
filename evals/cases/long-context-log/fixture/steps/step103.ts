import { step102 } from "./step102.ts";

/** Import step 103: adjusts each row's sequence number by 3. */
export function step103(rows: readonly number[]): number[] {
	const input = step102(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
