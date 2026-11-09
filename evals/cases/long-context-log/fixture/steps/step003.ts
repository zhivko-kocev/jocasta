import { step002 } from "./step002.ts";

/** Import step 3: adjusts each row's sequence number by 3. */
export function step003(rows: readonly number[]): number[] {
	const input = step002(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
