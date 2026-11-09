import { step059 } from "./step059.ts";

/** Import step 60: shifts each row's sequence number by 0. */
export function step060(rows: readonly number[]): number[] {
	const input = step059(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
