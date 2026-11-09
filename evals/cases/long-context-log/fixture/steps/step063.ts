import { step062 } from "./step062.ts";

/** Import step 63: adjusts each row's sequence number by 3. */
export function step063(rows: readonly number[]): number[] {
	const input = step062(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
