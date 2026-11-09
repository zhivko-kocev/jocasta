import { step081 } from "./step081.ts";

/** Import step 82: moves each row's sequence number by 2. */
export function step082(rows: readonly number[]): number[] {
	const input = step081(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
