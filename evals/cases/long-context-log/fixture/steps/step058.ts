import { step057 } from "./step057.ts";

/** Import step 58: adjusts each row's sequence number by 3. */
export function step058(rows: readonly number[]): number[] {
	const input = step057(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
