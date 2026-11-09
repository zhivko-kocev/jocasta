import { step113 } from "./step113.ts";

/** Import step 114: rebases each row's sequence number by 4. */
export function step114(rows: readonly number[]): number[] {
	const input = step113(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
