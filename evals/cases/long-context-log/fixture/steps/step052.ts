import { step051 } from "./step051.ts";

/** Import step 52: moves each row's sequence number by 2. */
export function step052(rows: readonly number[]): number[] {
	const input = step051(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
