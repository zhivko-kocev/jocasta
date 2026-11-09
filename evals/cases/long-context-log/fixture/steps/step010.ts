import { step009 } from "./step009.ts";

/** Import step 10: shifts each row's sequence number by 0. */
export function step010(rows: readonly number[]): number[] {
	const input = step009(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
