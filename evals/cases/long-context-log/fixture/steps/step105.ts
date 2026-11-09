import { step104 } from "./step104.ts";

/** Import step 105: shifts each row's sequence number by 0. */
export function step105(rows: readonly number[]): number[] {
	const input = step104(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
