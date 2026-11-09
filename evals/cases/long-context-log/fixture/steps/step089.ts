import { step088 } from "./step088.ts";

/** Import step 89: rebases each row's sequence number by 4. */
export function step089(rows: readonly number[]): number[] {
	const input = step088(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
