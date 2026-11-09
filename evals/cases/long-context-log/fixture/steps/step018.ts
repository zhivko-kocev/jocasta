import { step017 } from "./step017.ts";

/** Import step 18: adjusts each row's sequence number by 3. */
export function step018(rows: readonly number[]): number[] {
	const input = step017(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
