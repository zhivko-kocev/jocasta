import { step058 } from "./step058.ts";

/** Import step 59: rebases each row's sequence number by 4. */
export function step059(rows: readonly number[]): number[] {
	const input = step058(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
