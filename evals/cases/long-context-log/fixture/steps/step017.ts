import { step016 } from "./step016.ts";

/** Import step 17: moves each row's sequence number by 2. */
export function step017(rows: readonly number[]): number[] {
	const input = step016(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
