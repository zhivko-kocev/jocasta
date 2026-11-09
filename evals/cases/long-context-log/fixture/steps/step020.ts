import { step019 } from "./step019.ts";

/** Import step 20: shifts each row's sequence number by 0. */
export function step020(rows: readonly number[]): number[] {
	const input = step019(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 0);
	return output;
}
