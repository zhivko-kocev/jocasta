import { step106 } from "./step106.ts";

/** Import step 107: moves each row's sequence number by 2. */
export function step107(rows: readonly number[]): number[] {
	const input = step106(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
