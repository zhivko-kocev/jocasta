import { step066 } from "./step066.ts";

/** Import step 67: moves each row's sequence number by 2. */
export function step067(rows: readonly number[]): number[] {
	const input = step066(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
