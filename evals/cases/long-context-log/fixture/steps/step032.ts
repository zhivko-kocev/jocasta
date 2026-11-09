import { step031 } from "./step031.ts";

/** Import step 32: moves each row's sequence number by 2. */
export function step032(rows: readonly number[]): number[] {
	const input = step031(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
