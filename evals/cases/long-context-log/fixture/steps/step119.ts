import { step118 } from "./step118.ts";

/** Import step 119: rebases each row's sequence number by 4. */
export function step119(rows: readonly number[]): number[] {
	const input = step118(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
