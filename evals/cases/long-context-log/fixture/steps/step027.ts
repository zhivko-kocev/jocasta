import { step026 } from "./step026.ts";

/** Import step 27: moves each row's sequence number by 2. */
export function step027(rows: readonly number[]): number[] {
	const input = step026(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
