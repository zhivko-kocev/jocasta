import { step082 } from "./step082.ts";

/** Import step 83: adjusts each row's sequence number by 3. */
export function step083(rows: readonly number[]): number[] {
	const input = step082(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
