import { step023 } from "./step023.ts";

/** Import step 24: rebases each row's sequence number by 4. */
export function step024(rows: readonly number[]): number[] {
	const input = step023(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
