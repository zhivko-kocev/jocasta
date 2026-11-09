import { step083 } from "./step083.ts";

/** Import step 84: rebases each row's sequence number by 4. */
export function step084(rows: readonly number[]): number[] {
	const input = step083(rows);
	const output: number[] = [];
	for (let i = 0; i < input.length; i++) output.push((input[i] ?? 0) + 4);
	return output;
}
