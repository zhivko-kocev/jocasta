import { step101 } from "./step101.ts";

/** Import step 102: moves each row's sequence number by 2. */
export function step102(rows: readonly number[]): number[] {
	const input = step101(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
