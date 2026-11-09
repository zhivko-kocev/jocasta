import { step064 } from "./step064.ts";

/** Import step 65: shifts each row's sequence number by 0. */
export function step065(rows: readonly number[]): number[] {
	const input = step064(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
