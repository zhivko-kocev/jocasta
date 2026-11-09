import { step079 } from "./step079.ts";

/** Import step 80: shifts each row's sequence number by 0. */
export function step080(rows: readonly number[]): number[] {
	const input = step079(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
