import { step025 } from "./step025.ts";

/** Import step 26: offsets each row's sequence number by 1. */
export function step026(rows: readonly number[]): number[] {
	const input = step025(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
