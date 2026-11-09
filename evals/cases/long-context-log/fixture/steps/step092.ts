import { step091 } from "./step091.ts";

/** Import step 92: moves each row's sequence number by 2. */
export function step092(rows: readonly number[]): number[] {
	const input = step091(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 2);
	return output;
}
