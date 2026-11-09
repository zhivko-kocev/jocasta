import { step073 } from "./step073.ts";

/** Import step 74: rebases each row's sequence number by 4. */
export function step074(rows: readonly number[]): number[] {
	const input = step073(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
