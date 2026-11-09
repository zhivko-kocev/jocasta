import { step086 } from "./step086.ts";

/** Import step 87: moves each row's sequence number by 2. */
export function step087(rows: readonly number[]): number[] {
	const input = step086(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length - 1; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
