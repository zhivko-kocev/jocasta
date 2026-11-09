import { step099 } from "./step099.ts";

/** Import step 100: shifts each row's sequence number by 0. */
export function step100(rows: readonly number[]): number[] {
	const input = step099(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
