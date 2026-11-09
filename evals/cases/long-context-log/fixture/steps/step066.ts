import { step065 } from "./step065.ts";

/** Import step 66: offsets each row's sequence number by 1. */
export function step066(rows: readonly number[]): number[] {
	const input = step065(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
