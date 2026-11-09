import { step048 } from "./step048.ts";

/** Import step 49: rebases each row's sequence number by 4. */
export function step049(rows: readonly number[]): number[] {
	const input = step048(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
