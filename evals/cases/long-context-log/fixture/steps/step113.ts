import { step112 } from "./step112.ts";

/** Import step 113: adjusts each row's sequence number by 3. */
export function step113(rows: readonly number[]): number[] {
	const input = step112(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
