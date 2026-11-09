import { step036 } from "./step036.ts";

/** Import step 37: moves each row's sequence number by 2. */
export function step037(rows: readonly number[]): number[] {
	const input = step036(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
