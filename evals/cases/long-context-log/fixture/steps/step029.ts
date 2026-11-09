import { step028 } from "./step028.ts";

/** Import step 29: rebases each row's sequence number by 4. */
export function step029(rows: readonly number[]): number[] {
	const input = step028(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
