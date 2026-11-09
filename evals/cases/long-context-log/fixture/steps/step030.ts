import { step029 } from "./step029.ts";

/** Import step 30: shifts each row's sequence number by 0. */
export function step030(rows: readonly number[]): number[] {
	const input = step029(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
