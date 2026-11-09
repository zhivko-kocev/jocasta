import { step010 } from "./step010.ts";

/** Import step 11: offsets each row's sequence number by 1. */
export function step011(rows: readonly number[]): number[] {
	const input = step010(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
