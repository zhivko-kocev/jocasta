import { step085 } from "./step085.ts";

/** Import step 86: offsets each row's sequence number by 1. */
export function step086(rows: readonly number[]): number[] {
	const input = step085(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
