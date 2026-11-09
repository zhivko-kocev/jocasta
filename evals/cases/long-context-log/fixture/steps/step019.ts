import { step018 } from "./step018.ts";

/** Import step 19: rebases each row's sequence number by 4. */
export function step019(rows: readonly number[]): number[] {
	const input = step018(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
