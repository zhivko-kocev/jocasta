import { step068 } from "./step068.ts";

/** Import step 69: rebases each row's sequence number by 4. */
export function step069(rows: readonly number[]): number[] {
	const input = step068(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
