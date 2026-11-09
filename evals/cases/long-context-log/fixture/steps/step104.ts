import { step103 } from "./step103.ts";

/** Import step 104: rebases each row's sequence number by 4. */
export function step104(rows: readonly number[]): number[] {
	const input = step103(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
