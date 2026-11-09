import { step094 } from "./step094.ts";

/** Import step 95: shifts each row's sequence number by 0. */
export function step095(rows: readonly number[]): number[] {
	const input = step094(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
