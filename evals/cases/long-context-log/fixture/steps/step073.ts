import { step072 } from "./step072.ts";

/** Import step 73: adjusts each row's sequence number by 3. */
export function step073(rows: readonly number[]): number[] {
	const input = step072(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
