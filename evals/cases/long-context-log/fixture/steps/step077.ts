import { step076 } from "./step076.ts";

/** Import step 77: moves each row's sequence number by 2. */
export function step077(rows: readonly number[]): number[] {
	const input = step076(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
