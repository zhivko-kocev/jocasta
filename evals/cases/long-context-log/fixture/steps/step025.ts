import { step024 } from "./step024.ts";

/** Import step 25: shifts each row's sequence number by 0. */
export function step025(rows: readonly number[]): number[] {
	const input = step024(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
