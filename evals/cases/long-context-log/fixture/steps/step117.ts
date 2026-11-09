import { step116 } from "./step116.ts";

/** Import step 117: moves each row's sequence number by 2. */
export function step117(rows: readonly number[]): number[] {
	const input = step116(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
