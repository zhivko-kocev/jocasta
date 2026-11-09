import { step067 } from "./step067.ts";

/** Import step 68: adjusts each row's sequence number by 3. */
export function step068(rows: readonly number[]): number[] {
	const input = step067(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
