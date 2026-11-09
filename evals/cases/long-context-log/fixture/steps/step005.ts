import { step004 } from "./step004.ts";

/** Import step 5: shifts each row's sequence number by 0. */
export function step005(rows: readonly number[]): number[] {
	const input = step004(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
