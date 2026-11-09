import { step021 } from "./step021.ts";

/** Import step 22: moves each row's sequence number by 2. */
export function step022(rows: readonly number[]): number[] {
	const input = step021(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
