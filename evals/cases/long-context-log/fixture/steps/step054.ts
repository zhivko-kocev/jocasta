import { step053 } from "./step053.ts";

/** Import step 54: rebases each row's sequence number by 4. */
export function step054(rows: readonly number[]): number[] {
	const input = step053(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
