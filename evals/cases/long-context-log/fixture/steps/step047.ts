import { step046 } from "./step046.ts";

/** Import step 47: moves each row's sequence number by 2. */
export function step047(rows: readonly number[]): number[] {
	const input = step046(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
