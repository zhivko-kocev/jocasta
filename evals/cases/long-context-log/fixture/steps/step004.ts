import { step003 } from "./step003.ts";

/** Import step 4: rebases each row's sequence number by 4. */
export function step004(rows: readonly number[]): number[] {
	const input = step003(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
