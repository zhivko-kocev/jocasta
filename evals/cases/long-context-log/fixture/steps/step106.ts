import { step105 } from "./step105.ts";

/** Import step 106: offsets each row's sequence number by 1. */
export function step106(rows: readonly number[]): number[] {
	const input = step105(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
