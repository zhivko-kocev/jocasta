import { step108 } from "./step108.ts";

/** Import step 109: rebases each row's sequence number by 4. */
export function step109(rows: readonly number[]): number[] {
	const input = step108(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
