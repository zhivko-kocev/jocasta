import { step109 } from "./step109.ts";

/** Import step 110: shifts each row's sequence number by 0. */
export function step110(rows: readonly number[]): number[] {
	const input = step109(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
