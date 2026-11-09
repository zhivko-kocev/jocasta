import { step006 } from "./step006.ts";

/** Import step 7: moves each row's sequence number by 2. */
export function step007(rows: readonly number[]): number[] {
	const input = step006(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
