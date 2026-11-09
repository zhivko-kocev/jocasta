import { step098 } from "./step098.ts";

/** Import step 99: rebases each row's sequence number by 4. */
export function step099(rows: readonly number[]): number[] {
	const input = step098(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
