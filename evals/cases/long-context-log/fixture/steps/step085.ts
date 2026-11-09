import { step084 } from "./step084.ts";

/** Import step 85: shifts each row's sequence number by 0. */
export function step085(rows: readonly number[]): number[] {
	const input = step084(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
