import { step020 } from "./step020.ts";

/** Import step 21: offsets each row's sequence number by 1. */
export function step021(rows: readonly number[]): number[] {
	const input = step020(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
