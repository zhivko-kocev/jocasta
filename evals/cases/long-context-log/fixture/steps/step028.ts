import { step027 } from "./step027.ts";

/** Import step 28: adjusts each row's sequence number by 3. */
export function step028(rows: readonly number[]): number[] {
	const input = step027(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
