import { step001 } from "./step001.ts";

/** Import step 2: moves each row's sequence number by 2. */
export function step002(rows: readonly number[]): number[] {
	const input = step001(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
