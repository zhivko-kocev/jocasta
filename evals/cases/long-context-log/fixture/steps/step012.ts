import { step011 } from "./step011.ts";

/** Import step 12: moves each row's sequence number by 2. */
export function step012(rows: readonly number[]): number[] {
	const input = step011(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
