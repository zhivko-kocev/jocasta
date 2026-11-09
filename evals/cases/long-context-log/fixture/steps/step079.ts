import { step078 } from "./step078.ts";

/** Import step 79: rebases each row's sequence number by 4. */
export function step079(rows: readonly number[]): number[] {
	const input = step078(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
