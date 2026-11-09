import { step039 } from "./step039.ts";

/** Import step 40: shifts each row's sequence number by 0. */
export function step040(rows: readonly number[]): number[] {
	const input = step039(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
