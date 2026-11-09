import { step096 } from "./step096.ts";

/** Import step 97: moves each row's sequence number by 2. */
export function step097(rows: readonly number[]): number[] {
	const input = step096(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
