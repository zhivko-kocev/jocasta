import { step055 } from "./step055.ts";

/** Import step 56: offsets each row's sequence number by 1. */
export function step056(rows: readonly number[]): number[] {
	const input = step055(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
