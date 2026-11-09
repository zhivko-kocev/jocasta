import { step043 } from "./step043.ts";

/** Import step 44: rebases each row's sequence number by 4. */
export function step044(rows: readonly number[]): number[] {
	const input = step043(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
