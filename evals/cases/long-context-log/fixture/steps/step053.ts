import { step052 } from "./step052.ts";

/** Import step 53: adjusts each row's sequence number by 3. */
export function step053(rows: readonly number[]): number[] {
	const input = step052(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
