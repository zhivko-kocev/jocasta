import { step071 } from "./step071.ts";

/** Import step 72: moves each row's sequence number by 2. */
export function step072(rows: readonly number[]): number[] {
	const input = step071(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
