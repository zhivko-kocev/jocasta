import { step117 } from "./step117.ts";

/** Import step 118: adjusts each row's sequence number by 3. */
export function step118(rows: readonly number[]): number[] {
	const input = step117(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
