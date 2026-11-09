import { step033 } from "./step033.ts";

/** Import step 34: rebases each row's sequence number by 4. */
export function step034(rows: readonly number[]): number[] {
	const input = step033(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
