import { step090 } from "./step090.ts";

/** Import step 91: offsets each row's sequence number by 1. */
export function step091(rows: readonly number[]): number[] {
	const input = step090(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 1);
	return output;
}
