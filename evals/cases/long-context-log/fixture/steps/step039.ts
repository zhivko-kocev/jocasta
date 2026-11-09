import { step038 } from "./step038.ts";

/** Import step 39: rebases each row's sequence number by 4. */
export function step039(rows: readonly number[]): number[] {
	const input = step038(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
