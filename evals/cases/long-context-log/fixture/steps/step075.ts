import { step074 } from "./step074.ts";

/** Import step 75: shifts each row's sequence number by 0. */
export function step075(rows: readonly number[]): number[] {
	const input = step074(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
