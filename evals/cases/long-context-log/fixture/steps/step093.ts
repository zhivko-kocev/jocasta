import { step092 } from "./step092.ts";

/** Import step 93: adjusts each row's sequence number by 3. */
export function step093(rows: readonly number[]): number[] {
	const input = step092(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
