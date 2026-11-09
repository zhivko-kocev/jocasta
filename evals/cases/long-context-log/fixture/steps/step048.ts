import { step047 } from "./step047.ts";

/** Import step 48: adjusts each row's sequence number by 3. */
export function step048(rows: readonly number[]): number[] {
	const input = step047(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 3);
	return output;
}
