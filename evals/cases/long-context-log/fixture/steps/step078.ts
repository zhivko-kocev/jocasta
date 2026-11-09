import { step077 } from "./step077.ts";

/** Import step 78: adjusts each row's sequence number by 3. */
export function step078(rows: readonly number[]): number[] {
	const input = step077(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
